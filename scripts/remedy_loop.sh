#!/usr/bin/env bash
# Self-healing tail of the pipeline: smoke-test the real-asset build against the original, hand each
# failure to Remedy (one issue at a time, highest priority first), rebuild, repeat. Runs unattended:
#   scripts/remedy_loop.sh <game> <original.nes> [max_rounds=4]
#   ATTEMPTED="motion sound"  seed areas to skip; the list persists in workspace/<game>/remedy_attempted.txt
#                             across restarts so a relaunched loop never pays twice for the same wall
#                             (ATTEMPTED_RESET=1 clears it)
#   BASELINE_COMMIT=<sha>     reset the tree to a known-best commit first
#   LOOP_BUDGET=<dollars>     stop before a round once this loop has spent that much (default 80)
#   PIPELINE_BUDGET/PIPELINE_T0 (from pipeline.sh) cap the whole pipeline the same way
# Exit codes: 0 smoke passes, 1 still failing after the rounds, 2 provider down / run timeout, 3 budget reached.
source "$(dirname "$0")/lib.sh"
GAME="${1:?game}"; ORIG="${2:?rom}"; MAX="${3:-4}"; Wr="$ROOT/workspace/$GAME/writer"; need lev; need jq; need fceux
ORIG="$(cd "$(dirname "$ORIG")" && pwd)/$(basename "$ORIG")"
ensure_daemon
ATT_FILE="$ROOT/workspace/$GAME/remedy_attempted.txt"; STATUS_FILE="$ROOT/workspace/$GAME/remedy_loop.status"
[ "${ATTEMPTED_RESET:-0}" = 1 ] && rm -f "$ATT_FILE"
attempted="$( (cat "$ATT_FILE" 2>/dev/null || true) | tr '\n' ' ') ${ATTEMPTED:-}"
LOOP_T0=$(date +%s); LOOP_BUDGET="${LOOP_BUDGET:-80}"
finish() { echo "$1" > "$STATUS_FILE"; log "status: $1"; exit "$2"; }
[ -n "${BASELINE_COMMIT:-}" ] && { git -C "$Wr" reset -q --hard "$BASELINE_COMMIT" && "$ROOT/scripts/build_byorom.sh" "$GAME" "$ORIG" >/dev/null 2>&1 && log "reset to known-best commit $BASELINE_COMMIT"; }
for round in $(seq 1 "$MAX"); do
  rm -f "$Wr"/.remedy/inbox/smoke-*.json
  if "$ROOT/scripts/smoke_realasset.sh" "$GAME" "$ORIG" > "$ROOT/workspace/$GAME/shots/smoke_round$round.txt" 2>&1; then log "round $round: smoke test PASSES - done"; cat "$ROOT/workspace/$GAME/shots/smoke_round$round.txt" | grep -E "PASS|FAIL|SMOKE"; finish "pass" 0; fi
  grep -E "PASS|FAIL|SMOKE" "$ROOT/workspace/$GAME/shots/smoke_round$round.txt" || true
  # priority: screen > sprites > motion > sound > general; never retry an area a previous round already
  # attempted without clearing it (that would spend the same Remedy run twice on the same wall)
  F=""; for k in screen sprites motion sound tests smoke; do
    for cand in $(grep -l "\"title\": \"Real-asset smoke test: $k" "$Wr"/.remedy/inbox/smoke-*.json 2>/dev/null | sort -t- -k2,2n); do   # numeric: the smoke files areas in priority order
      area=$(jq -r .title "$cand" | sed 's/.*smoke test: //; s/ differs.*//')
      case " $attempted " in *" $area "*) continue;; esac
      F="$cand"; break
    done; [ -n "$F" ] && break
  done
  [ -n "$F" ] || { log "every failing area was already attempted ($attempted) - stopping"; finish "stopped: all failing areas attempted" 1; }
  # budgets: the whole pipeline's cap and this loop's own cap, checked before each Remedy run
  budget_check "remedy round $round" || finish "stopped: pipeline budget" 3
  spent="$(spend_since "$LOOP_T0")"; python3 -c "import sys; sys.exit(0 if float('$spent') < float('$LOOP_BUDGET') else 1)" || { log "loop budget reached (\$$spent of \$$LOOP_BUDGET)"; finish "stopped: loop budget" 3; }
  before=$(git -C "$Wr" rev-parse HEAD)
  N=$(jq -r .number "$F"); mkdir -p "$Wr/.remedy/claimed"; mv "$F" "$Wr/.remedy/claimed/issue-$N.json"
  area_now="$(jq -r .title "$Wr/.remedy/claimed/issue-$N.json" | sed 's/.*smoke test: //; s/ differs.*//')"; attempted="$attempted $area_now"; echo "$area_now" >> "$ATT_FILE"
  log "round $round: Remedy on issue #$N ($(jq -r .title "$Wr/.remedy/claimed/issue-$N.json"))"
  RID="$(lev_spawn "$ROOT/agents/remedy" --workdir "$Wr" --task "$N" --issue "@$Wr/.remedy/claimed/issue-$N.json" --yolo ${LEV_EXTRA:-})"
  echo "$RID" > "$ROOT/workspace/$GAME/remedy_run_id"; log "remedy run $RID"
  STATUS="$(lev_wait "$RID" 15)"; log "remedy finished: $STATUS cost $(lev_cost "$RID"); $(git -C "$Wr" log --oneline | head -1)"
  case "$STATUS" in ProviderDown|Timeout) git -C "$Wr" reset -q --hard "$before"; git -C "$Wr" clean -fdq -e .remedy -e observe -e build -e assets >/dev/null 2>&1 || true; finish "stopped: $STATUS during round $round" 2;; esac
  # Remedy scratch left outside src/include/test (root_cause dumps, throwaway programs) never ships
  { git -C "$Wr" ls-files --others --exclude-standard | grep -vE '^(src|include|test|spec)/|^Makefile$|\.cfg$|\.md$' || true; } | while read -r f; do [ -n "$f" ] && { rm -f "$Wr/$f"; log "removed Remedy scratch file $f"; }; done; true
  if [ -n "$(git -C "$Wr" status --porcelain -- src include test spec Makefile 2>/dev/null)" ]; then (cd "$Wr" && git add -A && git -c user.name=genesis -c user.email=genesis@local commit -q -m "Remedy leftovers for issue #$N ($RID)") && log "committed uncommitted Remedy changes"; fi
  "$ROOT/scripts/build_byorom.sh" "$GAME" "$ORIG" >/dev/null 2>&1 || die "rebuild failed after Remedy"
  # Regression guard: a round that turns a previously passing check into a failure is reverted.
  prev="$ROOT/workspace/$GAME/shots/smoke_round$round.txt"
  "$ROOT/scripts/smoke_realasset.sh" "$GAME" "$ORIG" > "$ROOT/workspace/$GAME/shots/smoke_after$round.txt" 2>&1 || true; rm -f "$Wr"/.remedy/inbox/smoke-*.json
  after="$ROOT/workspace/$GAME/shots/smoke_after$round.txt"
  regressed=$(grep "^PASS" "$prev" | sed 's/ (frame.*//; s/:.*//' | while read -r line; do name="${line#PASS  }"; grep -q "^PASS  $name" "$after" || echo "$name"; done; true)
  # a failing check that now diverges EARLIER is also a regression (motion "first at fN", sound "first difference at fN")
  for chk in motion sound; do
    b=$(grep "^FAIL  $chk" "$prev" | grep -oE "first (difference )?at f[0-9]+" | grep -oE "[0-9]+$" || true); a=$(grep "^FAIL  $chk" "$after" | grep -oE "first (difference )?at f[0-9]+" | grep -oE "[0-9]+$" || true)
    if [ -n "$b" ] && [ -n "$a" ] && [ "$a" -lt "$b" ]; then regressed="$regressed $chk(first diff f$b -> f$a)"; fi
  done
  # a failing check that got WORSE by count (more rows/records/frames differ, sound write count further off) is a
  # regression too (megablast rounds 1-2 were kept while motion went from 1 to 46 differing frames)
  worse=$(python3 "$ROOT/scripts/smoke_severity.py" "$prev" "$after" 2>/dev/null || true)
  [ -n "$worse" ] && regressed="$regressed
$worse"
  # Net-improvement rule: a round that fixes more checks than it breaks is KEPT (the broken check becomes the
  # next round's ticket, since the smoke test files one per failing area); an earlier divergence is never
  # accepted because it makes the visible gameplay drift sooner. Otherwise the round is reverted.
  # ...and a round is also kept when everything it broke ranks BELOW the area it fixed in the loop's own
  # priority order (screen > sprites > motion > sound > tests): trading a host-test compile error for an
  # identical late screen is progress, and the cheaper ticket comes next.
  fb=$(grep -oE "^SMOKE: [0-9]+" "$prev" | grep -oE "[0-9]+" || true); fa=$(grep -oE "^SMOKE: [0-9]+" "$after" | grep -oE "[0-9]+" || true)
  rank() { case "$1" in *screen*|*"initial screen"*|*routine*) echo 0;; *sprites*) echo 1;; *motion*) echo 2;; *sound*) echo 3;; *test*) echo 4;; *) echo 5;; esac; }
  ticket_area=$(jq -r .title "$Wr/.remedy/claimed/issue-$N.json" | sed 's/.*smoke test: //; s/ differs.*//'); tr_=$(rank "$ticket_area")
  kind="${ticket_area%%-*}"; sub="${ticket_area#*-}"
  case "$kind" in screen) pat="^FAIL  $sub screen";; sprites) pat="^FAIL  $sub sprites";; tests) pat="^FAIL  host test";; *) pat="^FAIL  $kind";; esac
  ticket_fixed=1; grep -q "$pat" "$after" && ticket_fixed=0
  worst=-1; while read -r r; do [ -n "$r" ] || continue; k=$(rank "$r"); [ "$k" -gt "$worst" ] && worst=$k; done <<< "$regressed"
  earlier=0; echo "$regressed" | grep -qE "first diff|severity" && earlier=1
  if [ -n "$regressed" ] && [ "$earlier" -eq 0 ] && [ -n "$fb" ] && [ -n "$fa" ] && { [ "$fa" -lt "$fb" ] || { [ "$fa" -le "$fb" ] && [ "$ticket_fixed" -eq 1 ] && [ "$worst" -gt "$tr_" ]; }; }; then
    log "round $round KEPT despite regression [$(echo $regressed | tr '\n' ';')]: failing checks $fb -> $fa, ticket area '$ticket_area' fixed, regressed areas rank lower (next ticket)"
    regressed=""
  fi
  if [ -n "$regressed" ]; then
    log "round $round REGRESSED [$(echo $regressed | tr '\n' ';')] - reverting to $before (candidate kept as tag round$round-candidate)"
    git -C "$Wr" tag -f "round$round-candidate" >/dev/null 2>&1 || true
    git -C "$Wr" reset -q --hard "$before"; "$ROOT/scripts/build_byorom.sh" "$GAME" "$ORIG" >/dev/null 2>&1 || die "rebuild failed after revert"
  fi
done
log "reached $MAX rounds; smoke test still failing - see workspace/$GAME/shots/smoke_round*.txt"; finish "failing after $MAX rounds" 1
