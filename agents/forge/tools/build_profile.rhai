// @tool build_profile
// @description CPU profile of YOUR OWN build per frame: runs build/game.nes in the emulator over a frame range (optionally pressing a button) and reports cycles per frame attributed to each routine (from build/game.map). Use it when build_oamlog shows a repeat/advance cadence (a lag frame: the main loop plus NMI exceed the 29,780-cycle frame) to find the routine over budget. Requires `make` with the map file.
// @param from integer optional "First frame to profile (default 60)"
// @param to integer optional "Last frame (default from+60)"
// @param press string optional "Button to press for 3 frames: A, B, start, select, up, down, left, right"
// @param press_at integer optional "Frame at which to press (default 0 = never)"
// @param top integer optional "How many routines to list (default 16)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
let from = int(params, "from", 60); let to = int(params, "to", from + 60); let at = int(params, "press_at", 0); let top = int(params, "top", 16);
let press = if params.contains("press") && params.press != () { word(params.press) } else { "" };
shell(`test -f build/game.nes || echo "build/game.nes missing - run make first"; MAP=$(ls build/*.map 2>/dev/null | head -1); test -n "$MAP" || echo "no build/*.map - link with ld65 -m build/game.map"; observe/emu_profile.sh build/game.nes "$MAP" ${from} ${to} "${press}" ${at} ${top} 2>&1 | head -n 40`)
