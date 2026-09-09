#!/usr/bin/env bash
# Add a feature to a reimplementation.   scripts/forge.sh <game|repo-dir> "<feature>" [--attended]
source "$(dirname "$0")/lib.sh"
# LEV_EXTRA="--model openrouter/openai/gpt-5.6-sol" passes extra flags to lev run (cheap iteration).
TARGET="${1:?usage: forge.sh <game|repo-dir> \"<feature>\" [--attended]}"; FEATURE="${2:?feature description}"; MODE="${3:-}"
need lev; need jq; need docker
DIR="$TARGET"; [ -d "$DIR" ] || DIR="$ROOT/workspace/$TARGET/writer"
[ -f "$DIR/spec/behavioral_spec.json" ] || die "$DIR has no spec/behavioral_spec.json"
[ -d "$DIR/.git" ] || ( cd "$DIR" && git init -q && git add -A && git commit -qm "Baseline before forge" )
YOLO=(--yolo); [ "$MODE" = "--attended" ] && YOLO=()
lev add "$ROOT/agents/forge-worker" >/dev/null 2>&1 || true
RID="$(lev_spawn "$ROOT/agents/forge" --workdir "$DIR" --task "$FEATURE" "${YOLO[@]}" ${LEV_EXTRA:-})"
log "forge run $RID started (attended runs pause at plan sign-off: lev respond / lev dash)"
STATUS="$(lev_wait "$RID" 15)"; log "forge finished: $STATUS  cost $(lev_cost "$RID")"
lev result "$RID" --raw 2>/dev/null | head -40
