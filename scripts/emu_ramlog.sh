#!/usr/bin/env bash
# scripts/emu_ramlog.sh <rom> <addrs e.g. 22,1A,0300> <from> <to> [button] [press_at] → per-frame values (frames where any changed)
HERE="$(cd "$(dirname "$0")" && pwd)"
ROM="${1:?rom}"; ADDRS="${2:?addrs}"; FROM="${3:?from}"; TO="${4:?to}"; PRESS="${5:-}"; AT="${6:-0}"
ROM="$(cd "$(dirname "$ROM")" && pwd)/$(basename "$ROM")"
OUT="$(mktemp "${TMPDIR:-/tmp}/nesram.XXXXXX").txt"; rm -f "$OUT"
export NESRAM_HOLD="${HOLD:-3}" NESRAM_OUT="$OUT" NESRAM_ADDRS="$ADDRS" NESRAM_FROM="$FROM" NESRAM_TO="$TO" NESRAM_PRESS="$PRESS" NESRAM_AT="$AT"
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
bash -c "$FCEUX --loadlua '$HERE/emu_ramlog.lua' '$ROM'; true" >/dev/null 2>&1 &
PID=$!
for i in $(seq 1 240); do kill -0 $PID 2>/dev/null || break; [ -s "$OUT" ] && { sleep 1; break; }; sleep 1; done; stop_emu $PID
[ -s "$OUT" ] && cat "$OUT" || { echo "no output" >&2; exit 1; }
rm -f "$OUT"
