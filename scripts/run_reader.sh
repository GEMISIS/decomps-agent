#!/usr/bin/env bash
# Clean Room A: analyze a ROM and produce the behavioral spec + asset manifest.
#   scripts/run_reader.sh <game> <rom.nes> [extra lev run args, e.g. --model openai/gpt-5.6-sol]
source "$(dirname "$0")/lib.sh"
GAME="${1:?usage: run_reader.sh <game> <rom.nes> [lev args]}"; ROM="${2:?rom path}"; shift 2
need lev; need jq; need nesrom
[ -f "$ROM" ] || die "ROM not found: $ROM"

W="$ROOT/workspace/$GAME/reader"
mkdir -p "$W/rom" "$W/spec/decoders" "$W/analysis" "$W/reference"
cp "$ROM" "$W/rom/game.nes"
cp -R "$ROOT/agents/genesis-reader/reference/." "$W/reference/"
mkdir -p "$W/observe" && cp "$ROOT/scripts/emu_observe.sh" "$ROOT/scripts/emu_observe.lua" "$ROOT/scripts/emu_apu_trace.sh" "$ROOT/scripts/emu_apu_trace.lua" "$ROOT/scripts/emu_oamlog.sh" "$ROOT/scripts/emu_oamlog.lua" "$ROOT/scripts/emu_ramlog.sh" "$ROOT/scripts/emu_ramlog.lua" "$W/observe/"   # rom_observe tool (FCEUX)
log "workdir $W (rom sha256 $(sha256_of "$W/rom/game.nes"))"
nesrom header "$W/rom/game.nes" | jq -c '{mapper, prg_size, chr_size, chr_ram, supported}' >&2 || die "nesrom cannot parse this ROM"

lev add "$ROOT/agents/genesis-reader-worker" >/dev/null 2>&1 || true   # keep the installed worker blueprint current
lev validate "$ROOT/agents/genesis-reader" >/dev/null 2>&1 || die "genesis-reader blueprint does not validate"
if [ -n "${ATTACH_RUN_ID:-}" ]; then RID="$ATTACH_RUN_ID"; log "attaching to existing reader run $RID"; else
RID="$(lev_spawn "$ROOT/agents/genesis-reader" --workdir "$W" --task "$GAME" --yolo "$@")"; fi
echo "$RID" > "$W/run_id"; log "reader run $RID started (lev dash to watch)"
STATUS="$(lev_wait "$RID" 15)"
log "reader run finished: $STATUS  cost $(lev_cost "$RID")"
END_STAGE="$(lev result "$RID" --json 2>/dev/null | jq -r '.stage // empty')"
if { [ "$END_STAGE" = "publish" ] || [ "$END_STAGE" = "recover" ]; } && lev result "$RID" --raw > "$W/spec/behavioral_spec.published.json" 2>/dev/null && jq -e . "$W/spec/behavioral_spec.published.json" >/dev/null 2>&1; then
  log "validated spec saved to $W/spec/behavioral_spec.published.json"
else
  rm -f "$W/spec/behavioral_spec.published.json"
  log "run ended in stage '${END_STAGE:-unknown}' (not publish); handoff will use spec/behavioral_spec.json from disk if present. See: lev result $RID"
fi
[ "$STATUS" = "Complete" ] || exit 1
