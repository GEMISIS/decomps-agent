#!/usr/bin/env bash
# Everything an unattended pipeline run needs, checked up front so it fails in seconds, not hours.
#   scripts/preflight.sh <rom.nes>
source "$(dirname "$0")/lib.sh"
ROM="${1:-}"; fails=0
ok()   { printf '  ok    %s\n' "$*"; }
bad()  { printf '  FAIL  %s\n' "$*"; fails=$((fails+1)); }
for t in lev jq python3 fceux cc65 ca65 ld65 docker git; do command -v "$t" >/dev/null 2>&1 && ok "$t" || bad "$t missing (scripts/setup.sh)"; done
command -v nesrom >/dev/null 2>&1 && ok "nesrom" || { [ -x "$ROOT/tools/nesrom/target/release/nesrom" ] && ok "nesrom (tools/nesrom/target/release)" || bad "nesrom not built: (cd tools/nesrom && cargo build --release)"; }
python3 -c "import jsonschema" 2>/dev/null && ok "python jsonschema" || bad "python3 -m pip install jsonschema"
docker info >/dev/null 2>&1 && ok "docker daemon" || bad "docker daemon not running (the writer sandbox needs it)"
[ -n "$(docker image ls -q nes-decomp-cc65:latest 2>/dev/null)" ] && ok "sandbox image nes-decomp-cc65" || printf '  note  sandbox image will be built on first writer run\n'
if lev ps --json >/dev/null 2>&1; then ok "leviath daemon"; else bad "leviath daemon not responding (lev daemon)"; fi
H="$(lev ps --json 2>/dev/null | jq -r '(.health.providers_down // []) | map("\(.provider): \(.reason)") | join(", ")')"
[ -z "$H" ] && ok "providers up" || bad "provider down: $H (top up credits before an unattended run)"
# OpenRouter balance: an unattended run must not start on an empty account (key read from config, never printed)
KEY="$(grep -E '^openrouter_api_key' "$HOME/.leviath/config.toml" 2>/dev/null | sed -E 's/^[^=]*= *"?([^"]*)"?.*/\1/')"
if [ -n "$KEY" ]; then
  BAL="$(curl -s -m 10 -H "Authorization: Bearer $KEY" https://openrouter.ai/api/v1/credits 2>/dev/null | jq -r '(.data.total_credits - .data.total_usage) // empty' 2>/dev/null)"
  if [ -z "$BAL" ]; then printf '  note  could not read the OpenRouter balance (offline?)\n'
  elif python3 -c "import sys; sys.exit(0 if float('$BAL') >= float('${PIPELINE_BUDGET:-50}') else 1)"; then ok "openrouter balance \$$(printf '%.2f' "$BAL") (budget \$${PIPELINE_BUDGET:-50})"
  else bad "openrouter balance \$$(printf '%.2f' "$BAL") is below the budget \$${PIPELINE_BUDGET:-50} - top up or pass --budget"; fi
fi
grep -q '^\[tool_script_permissions\]' "$HOME/.leviath/config.toml" 2>/dev/null && grep -q 'shell *= *"allow"' "$HOME/.leviath/config.toml" && ok "tool script shell permission" || bad "~/.leviath/config.toml needs [tool_script_permissions] shell = \"allow\" (scripts/setup.sh)"
for a in genesis-reader genesis-reader-worker genesis-writer genesis-writer-worker remedy forge forge-worker; do lev validate "$ROOT/agents/$a" >/dev/null 2>&1 && ok "blueprint $a" || bad "blueprint $a does not validate"; done
if [ -n "$ROM" ]; then
  [ -f "$ROM" ] && ok "rom $ROM" || bad "rom not found: $ROM"
  [ -f "$ROM" ] && { nesrom header "$ROM" 2>/dev/null | jq -e '.supported == true' >/dev/null 2>&1 && ok "mapper supported ($(nesrom header "$ROM" | jq -r .mapper_name))" || bad "nesrom cannot parse this ROM or the mapper is unsupported"; }
fi
[ "$fails" -eq 0 ] && { log "preflight ok"; exit 0; } || { log "preflight: $fails problem(s)"; exit 1; }
