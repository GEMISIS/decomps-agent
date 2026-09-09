#!/usr/bin/env bash
# User-side BYOROM build: extract assets from YOUR ROM with the manifest, then
# build the reimplementation.   scripts/build_byorom.sh <game> <rom.nes>
source "$(dirname "$0")/lib.sh"
GAME="${1:?usage: build_byorom.sh <game> <rom.nes>}"; ROM="${2:?rom path}"
[ -f "$ROM" ] || die "ROM not found: $ROM"; ROM="$(cd "$(dirname "$ROM")" && pwd)/$(basename "$ROM")"
need nesrom; need make; need cc65
Wr="$ROOT/workspace/$GAME/writer"; MANIFEST="$ROOT/workspace/$GAME/reader/spec/asset_manifest.json"
[ -f "$Wr/Makefile" ] || die "no Makefile in $Wr — run the writer first"
[ -f "$MANIFEST" ] || die "no manifest at $MANIFEST"
rm -rf "$Wr/assets"; mkdir -p "$Wr/assets"
log "extracting assets from $ROM"
( cd "$ROOT/workspace/$GAME/reader" && nesrom extract --rom "$ROM" --manifest "$MANIFEST" --out "$Wr/assets" ) | jq -c '.[]? // .' >&2
# Every file src/assets.s includes must now be a REAL extracted file. A missing or all-zero
# file means the writer named it differently from the extractor (e.g. .bin vs .nam): copy the
# extracted twin over and warn, or fail loudly - never build a ROM on top of a zero stub.
for inc in $(grep -oE '\.incbin +"assets/[^"]+"' "$Wr/src/assets.s" 2>/dev/null | grep -oE 'assets/[^"]+'); do
  f="$Wr/$inc"; base="${inc%.*}"; ok=0
  if [ -s "$f" ] && [ "$(LC_ALL=C tr -d '\000' < "$f" | head -c 1 | wc -c)" -gt 0 ]; then ok=1; fi
  if [ "$ok" = 0 ]; then
    for ext in nam bin pal chr attr; do alt="$Wr/$base.$ext"; [ "$alt" = "$f" ] && continue
      if [ -s "$alt" ] && [ "$(LC_ALL=C tr -d '\000' < "$alt" | head -c 1 | wc -c)" -gt 0 ]; then cp "$alt" "$f"; log "NAMING MISMATCH: assets.s includes $inc but the extractor wrote $base.$ext - copied (fix the writer: spec/asset_files.json has the exact names)"; ok=1; break; fi
    done
  fi
  if [ "$ok" = 0 ]; then [ -f "$f" ] || die "asset $inc is missing after extraction - the manifest or the writer's asset naming is wrong"; log "WARNING: $inc is all zeros after extraction (manifest offset wrong, or a genuinely blank table)"; fi
done
log "building"
make -C "$Wr" clean >/dev/null 2>&1 || true
make -C "$Wr" all
OUT="$(ls "$Wr"/build/*.nes 2>/dev/null | head -1)"; [ -n "$OUT" ] || die "build produced no .nes"
log "built $OUT ($(stat -f %z "$OUT") bytes) — open it in Mesen2 or FCEUX"
