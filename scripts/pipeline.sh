#!/usr/bin/env bash
# One command, unattended: ROM in, clean-room C project + rebuilt ROM + report out.
#   scripts/pipeline.sh <game> <rom.nes> [--cheap] [--skip-reader] [--skip-writer] [--rounds N] [--budget DOLLARS]
#   --cheap          every stage on openrouter/openai/gpt-5.6-sol (default: the blueprints' per-stage mix)
#   --skip-reader    reuse workspace/<game>/reader/spec       --skip-writer  reuse workspace/<game>/writer
#   --rounds N       Remedy loop rounds (default 4)           --budget D     stop when the run has spent D dollars (default 400)
# Never prompts. Exit 0 = acceptance passes; 1 = still differs (see REPORT.md); 2 = provider/timeout; 3 = budget; 4 = preflight/build failure.
# Environment knobs: PROVIDER_DOWN_MIN (10), RUN_TIMEOUT_MIN (240), LOOP_BUDGET (80).
source "$(dirname "$0")/lib.sh"
GAME="${1:?usage: pipeline.sh <game> <rom.nes> [options]}"; ROM="${2:?rom path}"; shift 2
EXTRA=(); SKIP_R=0; SKIP_W=0; ROUNDS=4; export PIPELINE_BUDGET="${PIPELINE_BUDGET:-400}"
while [ $# -gt 0 ]; do case "$1" in --cheap) EXTRA=(--model openrouter/openai/gpt-5.6-sol);; --skip-reader) SKIP_R=1;; --skip-writer) SKIP_W=1;; --rounds) ROUNDS="$2"; shift;; --budget) PIPELINE_BUDGET="$2"; shift;; *) EXTRA+=("$1");; esac; shift; done
[ -f "$ROM" ] || die "ROM not found: $ROM"; ROM="$(cd "$(dirname "$ROM")" && pwd)/$(basename "$ROM")"
export PIPELINE_T0=$(date +%s); T0=$PIPELINE_T0
G="$ROOT/workspace/$GAME"; mkdir -p "$G/shots"; STATUS="$G/pipeline.status"
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
finish() { echo "$1" > "$STATUS"; log "pipeline status: $1"; exit "$2"; }

step "0/5 preflight"; "$ROOT/scripts/preflight.sh" "$ROM" || finish "preflight failed" 4
if [ "$SKIP_R" = 0 ]; then
  step "1/5 reader (Clean Room A)"; budget_check reader || finish "budget" 3
  "$ROOT/scripts/run_reader.sh" "$GAME" "$ROM" ${EXTRA[@]+"${EXTRA[@]}"} || log "reader run did not end in publish; continuing with the spec on disk if present"
fi
step "2/5 handoff (barrier)"; "$ROOT/scripts/handoff.sh" "$GAME" || finish "handoff failed (no valid spec)" 4
if [ "$SKIP_W" = 0 ]; then
  step "3/5 writer (Clean Room B)"; budget_check writer || finish "budget" 3
  "$ROOT/scripts/run_writer.sh" "$GAME" ${EXTRA[@]+"${EXTRA[@]}"} || log "writer run did not end in publish; trying to build what is there"
fi
step "4/5 BYOROM build"; "$ROOT/scripts/build_byorom.sh" "$GAME" "$ROM" || finish "real-asset build failed" 4
OUT="$G/writer/build/game.nes"
step "5/5 real-asset acceptance vs the original + Remedy loop"
rc=0; LEV_EXTRA="${EXTRA[*]+"${EXTRA[*]}"}" "$ROOT/scripts/remedy_loop.sh" "$GAME" "$ROM" "$ROUNDS" || rc=$?   # never let set -e swallow the loop's verdict
# final report, whatever the outcome
"$ROOT/scripts/compare_full.sh" "$GAME" "$ROM" > "$G/shots/compare_table.txt" 2>&1 || true
last_smoke="$(ls -t "$G"/shots/smoke_after*.txt "$G"/shots/smoke_round*.txt 2>/dev/null | head -1)"
{ echo "# $GAME — clean-room rebuild report"; echo; echo "Generated $(date -u +%FT%TZ) in $((($(date +%s)-T0)/60)) min. Spend: \$$(spend_since "$T0"). Remedy loop: $(cat "$G/remedy_loop.status" 2>/dev/null || echo n/a)."
  echo; echo "ROM: workspace/$GAME/writer/build/game.nes  (commit $(git -C "$G/writer" log --oneline -1 2>/dev/null))"; echo
  echo "## Acceptance (real assets, rebuilt vs original)"; echo; echo '```'; grep -E "PASS|FAIL|SMOKE" "$last_smoke" 2>/dev/null; echo '```'; echo
  echo "## Strict comparison"; echo; echo '```'; grep -E "PASS|FAIL|RESULT|identical|differ" "$G/shots/compare_table.txt" 2>/dev/null | head -20; echo '```'; echo "Side by side: workspace/$GAME/shots/compare_$GAME.png"; echo
  echo "## Code"; echo; echo "- C lines: $(cat "$G"/writer/src/*.c 2>/dev/null | wc -l | tr -d ' ')  tests: $(ls "$G"/writer/test/test_*.c 2>/dev/null | wc -l | tr -d ' ')  SPEC-GAP markers: $(grep -c 'SPEC-GAP' "$G"/writer/src/*.c 2>/dev/null | awk -F: '{s+=$2} END{print s+0}')"
  echo "- Host tests (strict): $(cd "$G/writer" && "$ROOT/scripts/host_tests_strict.sh" . 2>/dev/null | tail -1)"; echo
  echo "## Runs"; echo; echo "- reader: $(cat "$G/reader/run_id" 2>/dev/null)"; echo "- writer: $(cat "$G/writer/run_id" 2>/dev/null)"; echo "- remedy commits:"; git -C "$G/writer" log --oneline 2>/dev/null | grep -i 'fix #' | sed 's/^/  - /'
} > "$G/REPORT.md"
log "report: workspace/$GAME/REPORT.md; ROM: $OUT; source: workspace/$GAME/writer/{src,include,Makefile}"
case "$rc" in 0) finish "pass" 0;; 2) finish "provider down or run timeout - top up / rerun with --skip-reader --skip-writer" 2;; 3) finish "budget reached" 3;; *) finish "acceptance still failing after $ROUNDS rounds" 1;; esac
