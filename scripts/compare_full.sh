#!/usr/bin/env bash
# Full behavioural comparison of a rebuilt ROM against the original.
#   scripts/compare_full.sh <game> <original.nes> [rebuilt.nes=workspace/<game>/writer/build/game.nes] [button=start] [press_at=500]
# Writes workspace/<game>/shots/{original,rebuilt}_*.{png,observe.txt,oam.txt,apu.txt},
# compare_<game>.png (side by side) and prints a PASS/FAIL table on stdout.
source "$(dirname "$0")/lib.sh"
GAME="${1:?game}"; ORIG="${2:?original rom}"; REB="${3:-$ROOT/workspace/$GAME/writer/build/game.nes}"; BTN="${4:-start}"; AT="${5:-500}"
need fceux; need python3
[ -f "$ORIG" ] || die "original not found: $ORIG"; [ -f "$REB" ] || die "rebuilt not found: $REB"
S="$ROOT/workspace/$GAME/shots"; mkdir -p "$S"
sc() { local rom="$1" tag="$2" frames="$3" press="$4"
  "$ROOT/scripts/emu_check.sh"   "$rom" "$S/${tag}.png" "$frames" "$press" "$AT" >/dev/null 2>&1 || true
  "$ROOT/scripts/emu_observe.sh" "$rom" "$frames" "$press" "$AT" > "$S/${tag}.observe.txt" 2>/dev/null || true; }
res=(); pass=0; fail=0
check() { local name="$1" a="$2" b="$3"; local d; d=$(diff "$a" "$b" 2>/dev/null | /usr/bin/grep -c '^[<>]' || true)
  if [ "$d" = 0 ]; then res+=("PASS  $name"); pass=$((pass+1)); else res+=("FAIL  $name ($d differing lines: diff $a $b)"); fail=$((fail+1)); fi; }
# 1. screens: title (frame 240, no input), gameplay (press at AT, observe AT+200), late (AT+2300)
for pair in "title:240:" "play:$((AT+200)):$BTN" "late:$((AT+2300)):$BTN"; do IFS=: read -r tag frames press <<<"$pair"
  sc "$ORIG" "original_$tag" "$frames" "$press"; sc "$REB" "rebuilt_$tag" "$frames" "$press"
  # nametable + palette (lines 2-33), attributes (34), sprites (35+ up to RAM)
  sed -n '2,34p' "$S/original_$tag.observe.txt" > "$S/.o_bg"; sed -n '2,34p' "$S/rebuilt_$tag.observe.txt" > "$S/.r_bg"; check "$tag screen: palette + nametable + attributes" "$S/.o_bg" "$S/.r_bg"
  sed -n '/SPRITES/,/visible sprites/p' "$S/original_$tag.observe.txt" > "$S/.o_sp"; sed -n '/SPRITES/,/visible sprites/p' "$S/rebuilt_$tag.observe.txt" > "$S/.r_sp"; check "$tag sprites (slot, x, y, tile, attr)" "$S/.o_sp" "$S/.r_sp"
done
# 2. sprite timeline around the input (first 60 frames after the press) and 200 frames later
"$ROOT/scripts/emu_oamlog.sh" "$ORIG" "$AT" "$((AT+60))" "$BTN" "$AT" > "$S/original_serve.oam.txt" 2>/dev/null; "$ROOT/scripts/emu_oamlog.sh" "$REB" "$AT" "$((AT+60))" "$BTN" "$AT" > "$S/rebuilt_serve.oam.txt" 2>/dev/null
check "sprite timeline frames $AT-$((AT+60)) after $BTN" "$S/original_serve.oam.txt" "$S/rebuilt_serve.oam.txt"
"$ROOT/scripts/emu_oamlog.sh" "$ORIG" "$((AT+140))" "$((AT+260))" "$BTN" "$AT" > "$S/original_mid.oam.txt" 2>/dev/null; "$ROOT/scripts/emu_oamlog.sh" "$REB" "$((AT+140))" "$((AT+260))" "$BTN" "$AT" > "$S/rebuilt_mid.oam.txt" 2>/dev/null
check "sprite timeline frames $((AT+140))-$((AT+260))" "$S/original_mid.oam.txt" "$S/rebuilt_mid.oam.txt"
# 3. sound: title music (frames 0-300) and gameplay effects (AT..AT+400)
"$ROOT/scripts/emu_apu_trace.sh" "$ORIG" "$S/original_title.apu.txt" 0 300 >/dev/null 2>&1; "$ROOT/scripts/emu_apu_trace.sh" "$REB" "$S/rebuilt_title.apu.txt" 0 300 >/dev/null 2>&1
check "sound registers frames 0-300 (title music)" "$S/original_title.apu.txt" "$S/rebuilt_title.apu.txt"
"$ROOT/scripts/emu_apu_trace.sh" "$ORIG" "$S/original_play.apu.txt" "$AT" "$((AT+400))" "$BTN" "$AT" >/dev/null 2>&1; "$ROOT/scripts/emu_apu_trace.sh" "$REB" "$S/rebuilt_play.apu.txt" "$AT" "$((AT+400))" "$BTN" "$AT" >/dev/null 2>&1
check "sound registers frames $AT-$((AT+400)) (gameplay effects)" "$S/original_play.apu.txt" "$S/rebuilt_play.apu.txt"
# 4. side-by-side image
python3 "$ROOT/scripts/compare_shots.py" "$S/compare_$GAME.png" "Title (frame 240)" "$S/original_title.png" "$S/rebuilt_title.png" "Gameplay (frame $((AT+200)))" "$S/original_play.png" "$S/rebuilt_play.png" "Late match (frame $((AT+2300)))" "$S/original_late.png" "$S/rebuilt_late.png" 2>/dev/null && res+=("image $S/compare_$GAME.png")
rm -f "$S"/.o_* "$S"/.r_*
printf '%s\n' "${res[@]}"; echo "SUMMARY: $pass pass, $fail fail"
[ "$fail" = 0 ]
