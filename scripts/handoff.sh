#!/usr/bin/env bash
# The information barrier. Copies ONLY the behavioral spec from the reader
# workspace to the writer workspace, after the barrier lint and schema checks.
#   scripts/handoff.sh <game>
source "$(dirname "$0")/lib.sh"
GAME="${1:?usage: handoff.sh <game>}"
need nesrom; need jq; need python3
R="$ROOT/workspace/$GAME/reader"; Wr="$ROOT/workspace/$GAME/writer"
SPEC="$R/spec/behavioral_spec.published.json"; [ -f "$SPEC" ] || SPEC="$R/spec/behavioral_spec.json"
[ -f "$SPEC" ] || die "no spec at $R/spec — run scripts/run_reader.sh first"
MANIFEST="$R/spec/asset_manifest.json"; [ -f "$MANIFEST" ] || die "no asset manifest at $MANIFEST"

# normalize sha256: models abbreviate hashes; the extractor validates it, so replace with the real one
if [ -f "$R/rom/game.nes" ]; then
  REAL="$(sha256_of "$R/rom/game.nes")"; python3 - "$MANIFEST" "$REAL" <<'PY'
import json, sys; p, real = sys.argv[1:3]; m = json.load(open(p)); rf = m.setdefault("rom_format", {})
changed = False
if rf.get("sha256") != real: print(f"note: manifest sha256 {rf.get('sha256')!r} -> {real}", file=sys.stderr); rf["sha256"] = real; changed = True
import re
for k in ("mapper", "prg_banks", "chr_rom_banks"):   # models sometimes write 'NROM (mapper 0)' or '2 x 16 KB'
    v = rf.get(k)
    if isinstance(v, str):
        mm = re.search(r"\d+", v)
        if mm: rf[k] = int(mm.group(0)); changed = True; print(f"note: manifest {k} {v!r} -> {rf[k]}", file=sys.stderr)
if isinstance(rf.get("chr_ram"), str): rf["chr_ram"] = rf["chr_ram"].strip().lower() in ("true", "yes", "1"); changed = True
# models write optional fields as null ("attribute_offset": null); the schema wants them absent
def strip_nulls(o):
    global changed
    if isinstance(o, dict):
        for k in [k for k, v in o.items() if v is None]: del o[k]; changed = True
        for v in o.values(): strip_nulls(v)
    elif isinstance(o, list):
        for v in o: strip_nulls(v)
strip_nulls(m)
if changed: json.dump(m, open(p, "w"), indent=2)
PY
fi
log "barrier lint: $SPEC"
LINT="$(nesrom lint-spec "$SPEC")" || { echo "$LINT" | jq . >&2; die "barrier lint FAILED — the spec leaks ROM detail; fix it on the reader side"; }
log "schema check (spec + manifest)"
python3 - "$SPEC" "$MANIFEST" "$ROOT/schemas" <<'PY'
import json, sys, jsonschema
spec, man, schemas = sys.argv[1:4]
S = json.load(open(spec)); M = json.load(open(man))
jsonschema.Draft202012Validator(json.load(open(f"{schemas}/behavioral_spec.schema.json"))).validate(S)
jsonschema.Draft202012Validator(json.load(open(f"{schemas}/asset_manifest.schema.json"))).validate(M)
spec_ids = {a["id"] for a in S.get("assets", [])}
man_ids = {a["id"] for group in M.get("assets", {}).values() for a in group}
missing = spec_ids - man_ids; extra = man_ids - spec_ids
if missing: print(f"WARNING: spec assets missing from manifest: {sorted(missing)}", file=sys.stderr)
if extra:   print(f"WARNING: manifest assets not in spec: {sorted(extra)}", file=sys.stderr)
print(f"spec ok: {len(S['systems'])} systems, {len(S['ram_variables'])} ram vars, {len(spec_ids)} assets")
PY

rm -rf "$Wr/spec"; mkdir -p "$Wr/spec" "$Wr/reference"
cp "$SPEC" "$Wr/spec/behavioral_spec.json"
cp -R "$ROOT/agents/genesis-writer/reference/." "$Wr/reference/"
# Exact asset file names and sizes the extractor will produce (naming is a tool rule, not ROM content):
# pattern_tables -> <id>.chr (tile_count*16), palettes -> <id>.pal, tilemaps -> <id>.nam (+ <id>.attr), others -> <id>.bin
# Unconfirmed asset locations extract garbage; refuse them here rather than at build time.
if grep -iqE "best-guess|best guess|unconfirmed|not confirmed|not independently confirmed" "$MANIFEST"; then die "asset manifest contains an unconfirmed location (grep -inE 'best-guess|unconfirmed|not confirmed' $MANIFEST) - the reader must locate every asset via the routine that reads it"; fi
python3 - "$MANIFEST" "$Wr/spec/asset_files.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1])); out = {}
for cat, lst in (m.get("assets") or {}).items():
    for a in lst:
        i = a["id"]
        if cat == "pattern_tables": out[i] = {"file": f"assets/{i}.chr", "bytes": int(a.get("tile_count", 256)) * 16}
        elif cat == "palettes":
            c = int(a.get("count", 4)); b = int(a.get("byte_count", c if c > 8 else c * 4)); out[i] = {"file": f"assets/{i}.pal", "bytes": b}
        elif cat == "tilemaps":
            out[i] = {"file": f"assets/{i}.nam", "bytes": int(a.get("byte_count", 960))}
            if a.get("attribute_offset") is not None: out[i + "_attr"] = {"file": f"assets/{i}.attr", "bytes": int(a.get("attribute_bytes", 64))}
        else: out[i] = {"file": f"assets/{i}.bin", "bytes": int(a.get("byte_count", 0))}
json.dump(out, open(sys.argv[2], "w"), indent=1); print(f"asset_files.json: {len(out)} files")
PY
# Belt and braces: nothing ROM-shaped may exist on the writer side.
find "$Wr" -name '*.nes' -not -path '*/build/*' -delete 2>/dev/null || true

RID="$(cat "$R/run_id" 2>/dev/null || echo unknown)"
jq -n --arg game "$GAME" --arg rid "$RID" --arg sha "$(sha256_of "$Wr/spec/behavioral_spec.json")" \
      --arg rom "$(sha256_of "$R/rom/game.nes" 2>/dev/null || echo unknown)" --arg ts "$(date -u +%FT%TZ)" \
      --argjson lint "$LINT" '{game:$game, reader_run_id:$rid, rom_sha256:$rom, spec_sha256:$sha, barrier_lint:$lint, handoff_at:$ts, writer_run_id:null}' \
      > "$ROOT/workspace/$GAME/provenance.json"
log "handoff complete → $Wr/spec/behavioral_spec.json ; provenance: workspace/$GAME/provenance.json"
