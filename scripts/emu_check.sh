#!/usr/bin/env bash
# Run a ROM in FCEUX for N frames and save a screenshot (visual smoke test).
#   scripts/emu_check.sh <rom.nes> [out.png] [frames=240] [button e.g. start|A] [press_at_frame=120]
source "$(dirname "$0")/lib.sh"
ROM="${1:?usage: emu_check.sh <rom.nes> [out.png] [frames] [button]}"; OUT="${2:-${ROM%.nes}.png}"; FRAMES="${3:-240}"; PRESS="${4:-}"; PRESS_AT="${5:-120}"
need fceux
ROM="$(cd "$(dirname "$ROM")" && pwd)/$(basename "$ROM")"; OUT="$(cd "$(dirname "$OUT")" && pwd)/$(basename "$OUT")"
rm -f "$OUT"; export NESSHOT_OUT="$OUT" NESSHOT_FRAMES="$FRAMES" NESSHOT_PRESS="$PRESS" NESSHOT_PRESS_AT="$PRESS_AT"
# Headless hosts: run our OWN Xvfb (not xvfb-run) so this shell is the parent of both Xvfb and the emulator and
# reaps both; a container whose init is `sleep` never reaps orphans, and every leftover zombie counts against its
# pid limit.
start_display() { XVFB_PID=""; if [ -z "${DISPLAY:-}" ] && command -v Xvfb >/dev/null 2>&1; then local n=$(( 100 + ($$ % 800) )); Xvfb ":$n" -screen 0 640x480x24 -nolisten tcp >/dev/null 2>&1 & XVFB_PID=$!; export DISPLAY=":$n"; sleep 0.5; fi; }
stop_display() { [ -n "${XVFB_PID:-}" ] && { kill "$XVFB_PID" 2>/dev/null || true; wait "$XVFB_PID" 2>/dev/null || true; }; return 0; }
start_display
FCEUX_CMD="fceux --loadlua "$ROOT/scripts/emu_screenshot.lua" "$ROM""
# Stop the emulator cleanly on both hosts: SIGKILL only the emulator leaf, let xvfb-run/bash exit and reap on their
# own (a killed launcher leaves Xvfb + fceux orphaned, and inside the sandbox they pile up until the 512-pid cgroup
# limit), then force the rest after a grace period. Uses /proc where it exists (the sandbox image has no pgrep).
_children() { if [ -d /proc ]; then local d; for d in /proc/[0-9]*; do [ "$(cut -d' ' -f4 "$d/stat" 2>/dev/null)" = "$1" ] && basename "$d"; done; else pgrep -P "$1" 2>/dev/null; fi; }
_comm() { if [ -d /proc ]; then cat "/proc/$1/comm" 2>/dev/null; else ps -o comm= -p "$1" 2>/dev/null | xargs basename 2>/dev/null; fi; }
_descendants() { local c; for c in $(_children "$1"); do echo "$c"; _descendants "$c"; done; }
kill_tree() { local c; for c in $(_children "$1"); do kill_tree "$c"; done; kill -KILL "$1" 2>/dev/null || true; }
stop_emu() { local p i; for p in $(_descendants "$1"); do case "$(_comm "$p")" in fceux*) kill -KILL "$p" 2>/dev/null || true;; esac; done
  for i in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$1" 2>/dev/null || break; sleep 0.5; done; kill_tree "$1"; wait "$1" 2>/dev/null || true; stop_display; }
bash -c "$FCEUX_CMD; true" >/dev/null 2>&1 &
PID=$!; for i in $(seq 1 120); do kill -0 $PID 2>/dev/null || break; [ -f "$OUT" ] && { sleep 1; break; }; sleep 1; done
stop_emu $PID
[ -f "$OUT" ] && log "screenshot: $OUT" || die "no screenshot produced (is a display available? try running fceux manually)"
