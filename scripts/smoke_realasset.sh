#!/usr/bin/env bash
# Real-asset smoke test after build_byorom: the sandbox only ever sees zero-filled assets, so
# data-driven failures (a sequencer spinning on real song data, a decoder mis-sizing a real
# tilemap) show up here first. Prints PASS/FAIL lines and, on failure, files a Remedy issue
# into <writer>/.remedy/inbox/ with the evidence (observations of the REBUILT only).
#   scripts/smoke_realasset.sh <game> <original.nes> [button=start] [press_at=500]
source "$(dirname "$0")/lib.sh"
set +e +o pipefail   # this script reports failures itself; a grep with no match must not abort it
GAME="${1:?game}"; ORIG="${2:?original rom}"; BTN="${3:-start}"; AT="${4:-500}"
Wr="$ROOT/workspace/$GAME/writer"; REB="$Wr/build/game.nes"; MAP="$(ls "$Wr"/build/*.map 2>/dev/null | head -1)"
[ -f "$REB" ] || die "no rebuilt ROM at $REB"; need fceux; need python3
S="$ROOT/workspace/$GAME/shots"; mkdir -p "$S"; fails=0; body=""
say() { echo "$1"; body="$body$1"$'\n'; case "$1" in FAIL*) fails=$((fails+1));; esac; }
# 1. spec check on the initial screen (palette literals, drawn, boot)
"$ROOT/scripts/emu_observe.sh" "$REB" 120 > "$S/smoke_rebuilt_120.txt" 2>/dev/null
python3 "$ROOT/scripts/spec_check.py" "$Wr/spec/behavioral_spec.json" "$S/smoke_rebuilt_120.txt" > "$S/smoke_speccheck.txt" 2>&1
if grep -q "SUMMARY: ALL PASS" "$S/smoke_speccheck.txt"; then say "PASS  initial screen vs spec (build_check on the real build)"; else say "FAIL  initial screen vs spec: $(grep FAIL "$S/smoke_speccheck.txt" | head -3 | tr '\n' ';')"; fi
# 2. runaway routine: no single routine may own more than 60% of a frame in steady state
if [ -n "$MAP" ]; then
  "$ROOT/scripts/emu_profile.sh" "$REB" "$MAP" 200 260 "" 0 4 > "$S/smoke_profile.txt" 2>/dev/null
  top="$(sed -n '3p' "$S/smoke_profile.txt")"; pct="$(echo "$top" | grep -oE '\([ 0-9.]+%' | tr -dc '0-9.' | cut -d. -f1)"
  if [ -n "$pct" ] && [ "$pct" -gt 60 ] && ! echo "$top" | grep -q "hw_wait_vblank"; then say "FAIL  runaway routine (real data): $top"; else say "PASS  no routine dominates the frame (top: $(echo "$top" | awk '{print $1, $2}'))"; fi
fi
# 3. the input that leaves the first screen must change the sprite layer, as it does on the original
o=$("$ROOT/scripts/emu_observe.sh" "$ORIG" $((AT+20)) "$BTN" "$AT" 2>/dev/null | grep -oE '\([0-9]+ visible' | tr -dc '0-9'); r=$("$ROOT/scripts/emu_observe.sh" "$REB" $((AT+20)) "$BTN" "$AT" 2>/dev/null | grep -oE '\([0-9]+ visible' | tr -dc '0-9')
if [ "${o:-0}" = "${r:-0}" ]; then say "PASS  sprites visible 20 frames after $BTN at $AT: $r (original $o)"; else say "FAIL  sprites visible 20 frames after $BTN at $AT: rebuilt $r, original $o"; fi
# 4. controller is being read after boot: the button must show in RAM (any variable named *button*/*pad*/*joy*)
if [ -n "$MAP" ]; then
  sym=$(grep -oE "_(button_mask_p1|pad1_buttons|pad1|joy1|pad1_state|buttons_p1|pad_held)[ ]+[0-9A-F]{6}" "$MAP" | head -1 | awk '{print substr($2,3,4)}')
  if [ -n "$sym" ]; then v=$(HOLD=6 "$ROOT/scripts/emu_ramlog.sh" "$REB" "$sym" $((AT+1)) $((AT+5)) "$BTN" "$AT" 2>/dev/null | grep -oE '=[0-9A-F]{2}' | tr -d '=' | sort -u | tr '\n' ' ')
    case "$v" in *00*) [ "$(echo $v | wc -w)" -gt 1 ] && say "PASS  controller read reaches RAM while $BTN is held (values $v)" || say "FAIL  controller never reaches RAM while $BTN is held (value stays 00): main loop not running?";; *) say "PASS  controller read reaches RAM while $BTN is held (values $v)";; esac
  fi
fi
# 5. state-machine watch: how the top-level state variable moves from boot through the press (evidence for the fixer)
if [ -n "$MAP" ]; then
  st=$(grep -oE "_(game_state|state|game_phase|gameplay_state_flag|current_state)[ ]+[0-9A-F]{6}" "$MAP" | head -1 | awk '{print substr($2,3,4)}')
  if [ -n "$st" ]; then trace=$("$ROOT/scripts/emu_ramlog.sh" "$REB" "$st" 1 $((AT+40)) "$BTN" "$AT" 2>/dev/null | grep -v "^(only" | head -12 | tr '\n' ';'); say "INFO  top-level state variable through boot and the $BTN press at $AT: $trace"; fi
fi
# 6. behaviour vs the original at two checkpoints (user side: the original ROM is legitimately here).
#    Observations only - tile grids, palettes, sprite lists, write counts - never code or addresses.
issues=""
ckpt() { # <tag> <frames> <press>
  local tag="$1" fr="$2" pr="$3"; local o="$S/smoke_o_$tag.txt" r="$S/smoke_r_$tag.txt"
  "$ROOT/scripts/emu_observe.sh" "$ORIG" "$fr" "$pr" "$AT" > "$o" 2>/dev/null; "$ROOT/scripts/emu_observe.sh" "$REB" "$fr" "$pr" "$AT" > "$r" 2>/dev/null
  local po=$(grep '^PALETTE' "$o" | cut -c1-140) pr_=$(grep '^PALETTE' "$r" | cut -c1-140)
  local ro=$(grep -cE "^[0-9][0-9] .*[0-9A-F]" "$o") rr=$(grep -cE "^[0-9][0-9] .*[0-9A-F]" "$r")
  local so=$(sed -n '/SPRITES/,/visible/p' "$o" | grep -c ' tile=') sr=$(sed -n '/SPRITES/,/visible/p' "$r" | grep -c ' tile=')
  local rowdiff=$(diff <(grep -E "^[0-9][0-9] " "$o") <(grep -E "^[0-9][0-9] " "$r") | grep -c "^<")
  local co=$(grep '^PPU CTRL' "$o" | cut -c1-60) cr=$(grep '^PPU CTRL' "$r" | cut -c1-60)
  local ad=$(diff <(grep -E "^  attr row" "$o") <(grep -E "^  attr row" "$r") | grep -c "^<")
  if [ "$po" = "$pr_" ] && [ "$rowdiff" = 0 ] && [ "$co" = "$cr" ] && [ "$ad" = 0 ]; then say "PASS  $tag screen (frame $fr): palette, PPU control (pattern tables), all 30 tile rows and all 8 attribute rows identical to the original"
  else say "FAIL  $tag screen (frame $fr): palette $([ "$po" = "$pr_" ] && echo same || echo DIFFERS), PPU control $([ "$co" = "$cr" ] && echo same || echo "DIFFERS (original: $co; rebuilt: $cr)"), $rowdiff of 30 tile rows differ, $ad of 8 attribute rows differ (original has $ro rows with content, rebuilt $rr)"; issues="$issues screen-$tag"
    { echo "== $tag screen, frame $fr (press $pr at $AT): original palette: $po"; echo "   rebuilt palette:  $pr_"; echo "   original $co"; echo "   rebuilt  $cr"; echo "   rows differing (original / rebuilt):"; diff <(grep -E "^[0-9][0-9] " "$o") <(grep -E "^[0-9][0-9] " "$r") | grep "^[<>]" | head -12; echo "   attribute rows: original"; grep -A8 "^ATTRIBUTES" "$o" | tail -8; echo "   rebuilt"; grep -A8 "^ATTRIBUTES" "$r" | tail -8; } >> "$S/smoke_evidence.txt"; fi
  local sd=$(diff <(sed -n '/SPRITES/,/visible/p' "$o" | grep ' tile=') <(sed -n '/SPRITES/,/visible/p' "$r" | grep ' tile=') | grep -c "^<")
  if [ "$sd" = 0 ]; then say "PASS  $tag sprites (frame $fr): $so records identical"; else say "FAIL  $tag sprites (frame $fr): $sd records differ (original $so visible, rebuilt $sr)"; issues="$issues sprites-$tag"
    { echo "== $tag sprites, frame $fr: original"; sed -n '/SPRITES/,/visible/p' "$o" | grep ' tile='; echo "   rebuilt"; sed -n '/SPRITES/,/visible/p' "$r" | grep ' tile='; } >> "$S/smoke_evidence.txt"; fi
}
: > "$S/smoke_evidence.txt"
ckpt title 240 ""; ckpt play $((AT+200)) "$BTN"; ckpt late $((AT+2300)) "$BTN"   # late = end of a no-input match on the original (win/lose text, game-over prompt)
# 7. motion: the first 8 frames after the press, sprite records of both
"$ROOT/scripts/emu_oamlog.sh" "$ORIG" "$AT" $((AT+70)) "$BTN" "$AT" > "$S/smoke_o_motion.txt" 2>/dev/null; "$ROOT/scripts/emu_oamlog.sh" "$REB" "$AT" $((AT+70)) "$BTN" "$AT" > "$S/smoke_r_motion.txt" 2>/dev/null
if diff -q "$S/smoke_o_motion.txt" "$S/smoke_r_motion.txt" >/dev/null; then say "PASS  motion: sprite records identical for 70 frames after $BTN (spawn, flight and first collision)"; else
  fd=$(diff "$S/smoke_o_motion.txt" "$S/smoke_r_motion.txt" | grep -oE "^< f[0-9]+" | head -1 | tr -d '< '); nd=$(diff "$S/smoke_o_motion.txt" "$S/smoke_r_motion.txt" | grep -c "^<")
  say "FAIL  motion: sprite records differ on $nd of 71 frames after $BTN at $AT, first at $fd"; issues="$issues motion"
  { echo "== motion after $BTN at $AT (first difference at $fd; original then rebuilt, the frames around it):"; grep -A6 "^$fd" "$S/smoke_o_motion.txt" | head -8; echo "   rebuilt"; grep -A6 "^$fd" "$S/smoke_r_motion.txt" | head -8; } >> "$S/smoke_evidence.txt"; fi
# 8. sound cadence: register writes per frame over the first 120 frames
"$ROOT/scripts/emu_apu_trace.sh" "$ORIG" "$S/smoke_o_apu.txt" 0 300 >/dev/null 2>&1; "$ROOT/scripts/emu_apu_trace.sh" "$REB" "$S/smoke_r_apu.txt" 0 300 >/dev/null 2>&1
"$ROOT/scripts/emu_apu_trace.sh" "$ORIG" "$S/smoke_o_apu2.txt" "$AT" $((AT+300)) "$BTN" "$AT" >/dev/null 2>&1; "$ROOT/scripts/emu_apu_trace.sh" "$REB" "$S/smoke_r_apu2.txt" "$AT" $((AT+300)) "$BTN" "$AT" >/dev/null 2>&1
cat "$S/smoke_o_apu2.txt" >> "$S/smoke_o_apu.txt"; cat "$S/smoke_r_apu2.txt" >> "$S/smoke_r_apu.txt"
if diff -q "$S/smoke_o_apu.txt" "$S/smoke_r_apu.txt" >/dev/null; then say "PASS  sound: register writes identical over frames 0-300 and $AT-$((AT+300)) (title music, its stop, serve and first effects)"; else
  no=$(grep -c "<=" "$S/smoke_o_apu.txt"); nr=$(grep -c "<=" "$S/smoke_r_apu.txt"); fd=$(diff "$S/smoke_o_apu.txt" "$S/smoke_r_apu.txt" | grep -oE "^[<>] f[0-9]+" | head -1 | cut -c3-); say "FAIL  sound: register writes differ over frames 0-300 / $AT-$((AT+300)) (original $no writes, rebuilt $nr, first difference at $fd)"; issues="$issues sound"
  { echo "== sound register writes (frames 0-300 then $AT-$((AT+300))): first differences (original <, rebuilt >)"; diff "$S/smoke_o_apu.txt" "$S/smoke_r_apu.txt" | head -40; echo "   writes per frame, original (frame:count) first 20 frames:"; grep -oE "^f[0-9]+" "$S/smoke_o_apu.txt" | sort | uniq -c | sort -k2.2n | head -20 | awk '{printf "%s:%s ", $2, $1}'; echo; echo "   rebuilt:"; grep -oE "^f[0-9]+" "$S/smoke_r_apu.txt" | sort | uniq -c | sort -k2.2n | head -20 | awk '{printf "%s:%s ", $2, $1}'; echo; } >> "$S/smoke_evidence.txt"; fi
# 9. the host test suite must build and pass here (a suite that only builds in the sandbox is a defect)
if (cd "$Wr" && "$ROOT/scripts/host_tests_strict.sh" . > "$S/smoke_hosttests.txt" 2>&1); then say "PASS  host test suite builds and passes on this machine with strict flags: $(grep -c "^PASS" "$S/smoke_hosttests.txt") binaries"
else say "FAIL  host test suite does not build/pass on this machine: $(grep -iE "error|^FAIL" "$S/smoke_hosttests.txt" | head -2 | tr '\n' ';' | cut -c1-200)"; issues="$issues tests"
  { echo "== host test suite (make test) on the user's machine:"; grep -iE "error|fail|warning" "$S/smoke_hosttests.txt" | head -20; } >> "$S/smoke_evidence.txt"; fi
echo "SMOKE: $fails fail"
if [ "$fails" -gt 0 ]; then
  mkdir -p "$Wr/.remedy/inbox" "$Wr/.remedy/claimed"; n=$(( $(find "$Wr/.remedy" -name '*.json' 2>/dev/null | wc -l | tr -d ' ') + 1 ))
  python3 - "$Wr/.remedy/inbox" "$n" "$body" "$(cat "$S/smoke_profile.txt" 2>/dev/null | head -8)" "$(cat "$S/smoke_speccheck.txt" 2>/dev/null | head -14)" "$issues" "$S/smoke_evidence.txt" <<'PY'
import json, sys, os
inbox, n, body, prof, chk, issues, evf = sys.argv[1:8]
os.makedirs(inbox, exist_ok=True); n = int(n)
ev = open(evf).read() if os.path.exists(evf) else ""
areas = issues.split() or ["general"]
passing = "\n".join(l for l in body.splitlines() if l.startswith("PASS"))
head = ("The ROM built with the real extracted assets (assets/ is populated in this checkout) fails the user-side smoke test. Full PASS/FAIL table:\n\n" + body +
        "\nNO REGRESSIONS: these checks pass today and must still pass after your fix (the loop reverts any round that breaks one):\n" + passing + "\n")
hint = {"screen": "A screen differs from the original: compare the tile rows and palette below with the spec's video / assets sections; a whole-screen draw with rendering off, the palette literals for that state, and text/HUD placement are the usual causes.",
        "sprites": "Sprite records differ: check OAM slot order, the template load, hidden sprites, and which state shows which sprites (spec timelines).",
        "motion": "Object motion differs frame by frame right after the input: compare with the spec timelines (spawn position, speed per frame, direction, attribute cycle) and use build_oamlog / build_ramlog on the position and direction variables.",
        "sound": "Sound register writes differ: compare the per-frame cadence and the first differing writes with the spec's sound_engine (tick source, init sequence, register write order, tempo).",
        "tests": "The host test suite fails to build or pass on the user's machine (clang, -Werror): run build_hosttests (it applies the strict flags whatever the Makefile says), fix the test files (missing includes, implicit declarations, C89 violations, test files not wired into the test target) and make the Makefile's test target itself use -Wall -Wextra -Werror -Wimplicit-function-declaration so this cannot recur.",
        "general": "See the table; typical cause: a data walker without an iteration cap (c_conventions 'Bounded data walkers')."}
for i, area in enumerate(areas):
    kind = area.split("-")[0]
    sect = ""
    for block in ev.split("== "):
        if block.startswith(area.replace("-", " ")) or block.startswith(kind) or (kind == "screen" and block.startswith(area.split("-")[1] + " screen") ) or (kind == "sprites" and block.startswith(area.split("-")[1] + " sprites")):
            sect += "== " + block
    doc = {"number": n + i, "title": f"Real-asset smoke test: {area} differs from the original",
           "body": head + "\nThis issue covers: " + area + ". " + hint.get(kind, "") + "\n\nEvidence (observations of the original vs this build; no code or addresses):\n" + (sect or ev[:6000]) +
                   ("\n\nProfile of this build (cycles per frame by routine):\n" + prof if prof else "") + "\n\nReproduce with build_observe / build_oamlog / build_ramlog / build_apu_trace / build_profile (assets/ holds real data here). Fix the code, add or extend the host test, keep the fixed BSP files untouched.",
           "labels": ["bug", "behavioral-mismatch"], "comments": []}
    path = os.path.join(inbox, f"smoke-{n + i}.json"); json.dump(doc, open(path, "w"), indent=1); print("filed", path)
PY
  exit 1
fi
