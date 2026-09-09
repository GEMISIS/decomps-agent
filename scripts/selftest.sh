#!/usr/bin/env bash
# Offline self-test (no model calls): nesrom tests, blueprint validation, the
# handoff barrier on a synthetic spec (positive + negative), asset extraction.
#   scripts/selftest.sh <rom.nes>   (any small NROM image you own; nothing ships with the repo)
source "$(dirname "$0")/lib.sh"
ROM="${1:?usage: selftest.sh <rom.nes>}"
need cargo; need lev; need jq; need nesrom
fail=0
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "cargo test (nesrom)"
( cd "$ROOT/tools/nesrom" && cargo test --release --quiet 2>&1 | tail -3 ) || fail=1

step "blueprints validate"
for a in genesis-reader genesis-writer remedy forge; do
  lev validate "$ROOT/agents/$a" >/dev/null 2>&1 && echo "  ok  $a" || { echo "  FAIL $a"; fail=1; }
done

step "handoff barrier on synthetic spec"
T="$ROOT/workspace/_selftest"; rm -rf "$T"; mkdir -p "$T/reader/spec" "$T/reader/rom"
cp "$ROOT/tests/fixtures/selftest/"*.json "$T/reader/spec/"; echo selftest > "$T/reader/run_id"
[ -f "$ROM" ] && cp "$ROM" "$T/reader/rom/game.nes"
"$ROOT/scripts/handoff.sh" _selftest >/dev/null 2>&1 && echo "  ok  positive handoff" || { echo "  FAIL positive handoff"; fail=1; }
sed 's/eight button bits/eight bits at $4016/' "$T/reader/spec/behavioral_spec.json" > "$T/reader/spec/behavioral_spec.published.json"
if "$ROOT/scripts/handoff.sh" _selftest >/dev/null 2>&1; then echo "  FAIL negative handoff (leak not caught)"; fail=1; else echo "  ok  negative handoff rejected"; fi

if [ -f "$ROM" ]; then
  step "nesrom on $ROM"
  nesrom header "$ROM" | jq -e '.supported' >/dev/null && echo "  ok  header" || { echo "  FAIL header"; fail=1; }
  nesrom reach "$ROM" | jq -e '.subroutines|length>0' >/dev/null && echo "  ok  reach" || { echo "  FAIL reach"; fail=1; }
  mkdir -p "$T/writer/assets"
  ( cd "$T/reader" && nesrom extract --rom rom/game.nes --manifest spec/asset_manifest.json --out ../writer/assets ) >/dev/null && echo "  ok  extract" || echo "  skip extract (manifest is for hello.nes)"
fi
rm -rf "$T"
step "docker sandbox image"
docker image inspect nes-decomp-cc65:latest >/dev/null 2>&1 && echo "  ok  nes-decomp-cc65:latest" || echo "  missing (run scripts/setup.sh)"
[ "$fail" = 0 ] && { echo; echo "selftest: all good"; } || { echo; echo "selftest: FAILURES"; exit 1; }
