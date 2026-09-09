#!/usr/bin/env bash
# Launch Remedy runs for bug reports. LOCAL by default: issues are JSON files in
# <repo>/.remedy/inbox/*.json ({"number","title","body","labels":[],"comments":[]}).
# Only with --github does it read GitHub issues (and label them claimed); Remedy
# itself never pushes or comments online either way.
#   scripts/remedy_poll.sh <repo-checkout-dir> [--github [label=bug]] [max=3]
source "$(dirname "$0")/lib.sh"
# LEV_EXTRA="--model openrouter/openai/gpt-5.6-sol" passes extra flags to lev run (cheap iteration).
REPO="${1:?usage: remedy_poll.sh <repo-dir> [--github [label]] [max]}"; shift; REPO="$(cd "$REPO" && pwd)"
MODE=local; LABEL=bug; MAX=3
while [ $# -gt 0 ]; do case "$1" in --github) MODE=github; [ -n "${2:-}" ] && [[ "$2" != --* ]] && [[ ! "$2" =~ ^[0-9]+$ ]] && { LABEL="$2"; shift; };; [0-9]*) MAX="$1";; esac; shift; done
need lev; need jq; need docker
cd "$REPO"; [ -f spec/behavioral_spec.json ] || die "$REPO has no spec/behavioral_spec.json"
mkdir -p .remedy/inbox .remedy/claimed
launch() {  # <number> <issue.json>
  RID="$(lev_spawn "$ROOT/agents/remedy" --workdir "$REPO" --task "$1" --issue "@$2" --yolo ${LEV_EXTRA:-})"
  log "issue #$1 → remedy run $RID (local branch remedy/issue-$1; PR text in .remedy/pr-$1.md)"
}
if [ "$MODE" = local ]; then
  ls .remedy/inbox/*.json >/dev/null 2>&1 || die "no issues in $REPO/.remedy/inbox/ — drop {\"number\",\"title\",\"body\"} JSON files there"
  for f in $(ls .remedy/inbox/*.json | head -n "$MAX"); do
    N="$(jq -r .number "$f")"; mv "$f" ".remedy/claimed/issue-$N.json"; launch "$N" "$REPO/.remedy/claimed/issue-$N.json"
  done
else
  need gh
  log "GitHub mode: reading open '$LABEL' issues and labelling them remedy:claimed (the only online write this script makes)"
  gh label create "remedy:claimed" --color 5319e7 --description "Remedy agent is working on it" 2>/dev/null || true
  gh issue list --label "$LABEL" --state open --json number,title,labels --limit 50 \
   | jq -r '.[] | select(any(.labels[]; .name=="remedy:claimed") | not) | .number' | head -n "$MAX" | while read -r N; do
    gh issue edit "$N" --add-label "remedy:claimed" >/dev/null
    gh issue view "$N" --json number,title,body,labels,comments > ".remedy/claimed/issue-$N.json"
    launch "$N" "$REPO/.remedy/claimed/issue-$N.json"
  done
fi
