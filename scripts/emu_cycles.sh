#!/usr/bin/env bash
# CPU cycles per frame spent in one routine (entry -> last APU write of the frame).
# scripts/emu_cycles.sh <rom> <map> <symbol> <out.txt> [from=0] [to=300] [button] [press_at]
HERE="$(cd "$(dirname "$0")" && pwd)"
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing $1" >&2; exit 1; }; }
die() { echo "error: $*" >&2; exit 1; }
ROM="${1:?rom}"; MAP="${2:?map}"; SYM="${3:?symbol}"; OUT="${4:?out}"; FROM="${5:-0}"; TO="${6:-300}"; PRESS="${7:-}"; PRESS_AT="${8:-0}"
need fceux; [ -f "$ROM" ] || die "ROM not found: $ROM"; [ -f "$MAP" ] || die "map not found: $MAP"
ADDR="$(grep -o -E "$SYM +[0-9A-F]{6}" "$MAP" | head -1 | awk '{print $2}')"; [ -n "$ADDR" ] || die "symbol $SYM not in map"
ROM="$(cd "$(dirname "$ROM")" && pwd)/$(basename "$ROM")"; OUT="$(cd "$(dirname "$OUT")" && pwd)/$(basename "$OUT")"
rm -f "$OUT" "$OUT.part"; export NESCYC_OUT="$OUT" NESCYC_FROM="$FROM" NESCYC_TO="$TO" NESCYC_ADDR="$ADDR" NESCYC_PRESS="$PRESS" NESCYC_PRESS_AT="$PRESS_AT"
FCEUX="fceux"
# Headless hosts: run our OWN Xvfb (not xvfb-run) so this shell is the parent of both Xvfb and the emulator and
# reaps both; a container whose init is `sleep` never reaps orphans, and every leftover zombie counts against its
# pid limit.
start_display() { XVFB_PID=""; if [ -z "${DISPLAY:-}" ] && command -v Xvfb >/dev/null 2>&1; then local n=$(( 100 + ($$ % 800) )); Xvfb ":$n" -screen 0 640x480x24 -nolisten tcp >/dev/null 2>&1 & XVFB_PID=$!; export DISPLAY=":$n"; sleep 0.5; fi; }
stop_display() { [ -n "${XVFB_PID:-}" ] && { kill "$XVFB_PID" 2>/dev/null || true; wait "$XVFB_PID" 2>/dev/null || true; }; return 0; }
# Stop the emulator cleanly on both hosts: SIGKILL only the emulator leaf, let xvfb-run/bash exit and reap on their
# own (a killed launcher leaves Xvfb + fceux orphaned, and inside the sandbox they pile up until the 512-pid cgroup
# limit), then force the rest after a grace period. Uses /proc where it exists (the sandbox image has no pgrep).
_children() { if [ -d /proc ]; then local d; for d in /proc/[0-9]*; do [ "$(cut -d' ' -f4 "$d/stat" 2>/dev/null)" = "$1" ] && basename "$d"; done; else pgrep -P "$1" 2>/dev/null; fi; }
_comm() { if [ -d /proc ]; then cat "/proc/$1/comm" 2>/dev/null; else ps -o comm= -p "$1" 2>/dev/null | xargs basename 2>/dev/null; fi; }
_descendants() { local c; for c in $(_children "$1"); do echo "$c"; _descendants "$c"; done; }
kill_tree() { local c; for c in $(_children "$1"); do kill_tree "$c"; done; kill -KILL "$1" 2>/dev/null || true; }
stop_emu() { local p i; for p in $(_descendants "$1"); do case "$(_comm "$p")" in fceux*) kill -KILL "$p" 2>/dev/null || true;; esac; done
  for i in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$1" 2>/dev/null || break; sleep 0.5; done; kill_tree "$1"; wait "$1" 2>/dev/null || true; stop_display; }
start_display
bash -c "$FCEUX --loadlua \"$HERE/emu_cycles.lua\" \"$ROM\"; true" >/dev/null 2>&1 &
PID=$!
for i in $(seq 1 180); do kill -0 $PID 2>/dev/null || break; [ -f "$OUT" ] && { sleep 1; break; }; sleep 1; done; stop_emu $PID
[ -f "$OUT" ] || die "no measurement produced"
echo "$SYM at \$$ADDR: $(wc -l < "$OUT" | tr -d ' ') frames measured (frames $FROM-$TO)"
awk '{c=$2; n++; s+=c; if(c>m)m=c} END{if(n) printf "avg %.0f max %d cycles per frame\n", s/n, m}' "$OUT"
