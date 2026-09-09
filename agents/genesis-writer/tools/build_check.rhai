// @tool build_check
// @description Deterministic acceptance check of YOUR OWN build against the spec: runs the emulator for N frames (optionally pressing a button) and compares the observation with spec/behavioral_spec.json — every initial palette entry, whether anything is drawn, whether the boot reached the palette upload. Prints PASS/FAIL per check and a SUMMARY line. Stub assets do not affect any of these checks (palettes are spec literals). BUILD: OK requires SUMMARY: ALL PASS. Run `make` first.
// @param frames integer optional "Frames to run (default 120)"
// @param press string optional "Button to press briefly: A, B, start, select, up, down, left, right"
// @param press_at integer optional "Frame at which to press (default 0 = never)"
// @param screen string optional "initial (default: compare initial palettes) or later (skip palette comparison)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
let frames = int(params, "frames", 120); let at = int(params, "press_at", 0);
let press = if params.contains("press") && params.press != () { word(params.press) } else { "" };
let screen = if params.contains("screen") && params.screen != () { word(params.screen) } else { "initial" };
shell(`test -f build/game.nes || echo "build/game.nes missing - run make first"; observe/emu_observe.sh build/game.nes ${frames} "${press}" ${at} > build/observe_check.txt 2>/dev/null; python3 observe/spec_check.py spec/behavioral_spec.json build/observe_check.txt --screen ${screen} 2>&1 | head -n 60`)
