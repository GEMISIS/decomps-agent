#!/usr/bin/env bash
# Clean Room B: build a cc65 C project from the spec. Never sees the ROM.
#   scripts/run_writer.sh <game> [extra lev run args]
source "$(dirname "$0")/lib.sh"
GAME="${1:?usage: run_writer.sh <game> [lev args]}"; shift
need lev; need jq; need docker
Wr="$ROOT/workspace/$GAME/writer"
[ -f "$Wr/spec/behavioral_spec.json" ] || die "no spec in $Wr — run scripts/handoff.sh first"
ensure_sandbox_image() {
  for i in 1 2 3; do [ -n "$(docker image ls -q nes-decomp-cc65:latest 2>/dev/null)" ] && return 0; sleep 2; done
  log "sandbox image missing — building nes-decomp-cc65:latest"
  docker build -q -t nes-decomp-cc65:latest -f "$ROOT/docker/cc65.Dockerfile" "$ROOT/docker" >/dev/null || die "could not build the sandbox image (is Docker running?)"
}
ensure_sandbox_image
if ls "$Wr"/rom/*.nes >/dev/null 2>&1 || [ -d "$Wr/rom" ]; then die "a rom/ directory exists in the writer workdir — the barrier is broken; remove it"; fi

mkdir -p "$Wr/observe" && cp "$ROOT/scripts/emu_observe.sh" "$ROOT/scripts/emu_observe.lua" "$ROOT/scripts/emu_apu_trace.sh" "$ROOT/scripts/emu_apu_trace.lua" "$ROOT/scripts/emu_oamlog.sh" "$ROOT/scripts/emu_oamlog.lua" "$ROOT/scripts/spec_check.py" "$ROOT/scripts/emu_profile.sh" "$ROOT/scripts/emu_profile.lua" "$ROOT/scripts/emu_ramlog.sh" "$ROOT/scripts/emu_ramlog.lua" "$ROOT/scripts/host_tests_strict.sh" "$Wr/observe/"   # build_observe / build_apu_trace tools
lev add "$ROOT/agents/genesis-writer-worker" >/dev/null 2>&1 || true   # keep the installed worker blueprint current
lev validate "$ROOT/agents/genesis-writer" >/dev/null 2>&1 || die "genesis-writer blueprint does not validate"
RID="$(lev_spawn "$ROOT/agents/genesis-writer" --workdir "$Wr" --task "$GAME" --yolo "$@")"
echo "$RID" > "$Wr/run_id"; log "writer run $RID started (lev dash to watch)"
STATUS="$(lev_wait "$RID" 15)"
log "writer run finished: $STATUS  cost $(lev_cost "$RID")"
lev result "$RID" --raw > "$Wr/BUILD_REPORT.published.md" 2>/dev/null || true
P="$ROOT/workspace/$GAME/provenance.json"
[ -f "$P" ] && jq --arg rid "$RID" --arg ts "$(date -u +%FT%TZ)" '.writer_run_id=$rid | .writer_finished_at=$ts' "$P" > "$P.tmp" && mv "$P.tmp" "$P"
# Local git history of the reimplementation (Remedy branches from it; nothing is ever pushed)
if [ ! -d "$Wr/.git" ]; then (cd "$Wr" && git init -q && printf 'assets/\nbuild/\nobserve/\nrun_id\n.remedy/\n' > .gitignore && log "git: initialised local repo in $Wr"); fi
# Always snapshot the tree the writer left behind: the Remedy loop's regression guard resets to HEAD,
# so HEAD must be THIS run's output, not an earlier attempt's.
(cd "$Wr" && git add -A >/dev/null 2>&1 && git -c user.name=genesis -c user.email=genesis@local commit -q -m "Genesis writer output ($RID)" >/dev/null 2>&1 && log "git: committed writer output") || true
log "audit: lev context $RID   (shows every tool call; there must be no rom_* calls and no reads outside $Wr)"
[ "$STATUS" = "Complete" ] || exit 1
