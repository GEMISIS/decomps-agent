#!/usr/bin/env bash
# Shared helpers for the NES clean-room pipeline scripts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

log()  { printf '\033[1;34m[%s]\033[0m %s\n' "$(basename "$0" .sh)" "$*" >&2; }
die()  { printf '\033[1;31m[%s] error:\033[0m %s\n' "$(basename "$0" .sh)" "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing '$1' — run scripts/setup.sh"; }

# lev run ... --json  →  prints the run id
lev_spawn() {
  local out
  out="$(lev run "$@" --json 2>/dev/null)" || die "lev run failed: $out"
  printf '%s' "$out" | jq -r '.run_id'
}

# Poll until the run is finished (or vanishes). Prints final status.
# Unattended rules: a provider that reports credits exhausted (or stays down) for PROVIDER_DOWN_MIN
# minutes ends the wait with status ProviderDown (the run is cancelled so it cannot resume and spend
# later); a run older than RUN_TIMEOUT_MIN minutes is cancelled with status Timeout.
lev_wait() {
  local rid="$1" interval="${2:-10}" status="" misses=0 json active fin down_since=0 t0 now
  local down_max=$(( ${PROVIDER_DOWN_MIN:-10} * 60 )) run_max=$(( ${RUN_TIMEOUT_MIN:-240} * 60 ))
  t0=$(date +%s)
  while :; do
    now=$(date +%s)
    json="$(lev ps --all --json 2>/dev/null)" || json="{}"
    active="$(jq -r --arg r "$rid" '(.runs // []) | map(select(.run_id==$r)) | .[0].status // empty' <<<"$json")"
    fin="$(jq -r --arg r "$rid" '(.finished // []) | map(select(.run_id==$r)) | .[0].status // empty' <<<"$json")"
    if [ -n "$fin" ]; then status="$fin"; break; fi
    # Leviath pauses a run (parent OR fan-out worker) after a stalled provider call; a paused
    # worker silently blocks its parent, so resume every paused run while we wait.
    for prun in $(jq -r '(.runs // [])[] | select(.status=="Paused") | .run_id' <<<"$json"); do
      log "run $prun paused by the daemon (provider timeout?) — resuming"; lev resume "$prun" >/dev/null 2>&1 || true
    done
    # provider health: credits_exhausted / repeated failures
    local reason; reason="$(jq -r '(.health.providers_down // [])[0].reason // empty' <<<"$json")"
    if [ -n "$reason" ]; then
      [ "$down_since" = 0 ] && { down_since=$now; log "provider down ($reason) - waiting up to ${PROVIDER_DOWN_MIN:-10} min"; }
      if [ $((now - down_since)) -ge "$down_max" ]; then
        log "provider still down ($reason) after ${PROVIDER_DOWN_MIN:-10} min - cancelling run $rid"; lev cancel "$rid" >/dev/null 2>&1 || true
        status="ProviderDown"; break
      fi
    else down_since=0; fi
    if [ $((now - t0)) -ge "$run_max" ]; then
      log "run $rid exceeded ${RUN_TIMEOUT_MIN:-240} min - cancelling"; lev cancel "$rid" >/dev/null 2>&1 || true; status="Timeout"; break
    fi
    if [ -z "$active" ]; then
      misses=$((misses+1))
      if lev result "$rid" --json >/dev/null 2>&1; then status="Complete"; break; fi
      [ "$misses" -ge 6 ] && { status="Unknown"; break; }
    else
      misses=0
    fi
    sleep "$interval"
  done
  printf '%s' "$status"
}

# Dollars spent by every run started at or after unix time $1 (from the daemon's run ledgers).
spend_since() {
  python3 - "$1" <<'PY'
import json, glob, os, sys
t0 = int(sys.argv[1]); tot = 0.0
for p in glob.glob(os.path.expanduser('~/.leviath/runs/*/meta.json')):
    try: m = json.load(open(p)); ts = int(m.get('run_id', '').split('-')[-2])
    except Exception: continue
    if ts >= t0: tot += float(m.get('cost_usd') or m.get('cost') or 0)
print(f"{tot:.2f}")
PY
}

# Stop (exit 3) when the pipeline's spend since PIPELINE_T0 exceeds PIPELINE_BUDGET dollars (unset = no cap).
budget_check() {
  [ -n "${PIPELINE_BUDGET:-}" ] && [ -n "${PIPELINE_T0:-}" ] || return 0
  local spent; spent="$(spend_since "$PIPELINE_T0")"
  if python3 -c "import sys; sys.exit(0 if float('$spent') >= float('$PIPELINE_BUDGET') else 1)"; then
    log "budget reached: \$$spent spent since the pipeline started (cap \$$PIPELINE_BUDGET) - stopping before '$1'"; return 3
  fi
  log "spend so far \$$spent of \$$PIPELINE_BUDGET before '$1'"; return 0
}

# The daemon must be up for lev run / lev ps; start it in the background if it is not.
ensure_daemon() {
  lev ps --json >/dev/null 2>&1 && return 0
  log "leviath daemon not responding - starting it"; (nohup lev daemon >/dev/null 2>&1 &) ; sleep 3
  lev ps --json >/dev/null 2>&1 || die "leviath daemon did not start (run 'lev daemon' in another terminal)"
}

lev_cost() {  # prints the run's total cost from the stage ledger, e.g. ~$1.23
  lev stages "$1" 2>/dev/null | awk '/^TOTAL/ {print $NF}' | head -1 || echo "?"
}

sha256_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
