// @tool rom_apu_trace
// @description Run the ROM in an emulator and log every write to the sound (APU) registers per frame: which register, which value, on which frame. Use it to learn the sound engine's real output cadence (writes per frame), to decode the music data grammar by matching your hypothesis against actual writes, and to describe sound effects (which registers change when a game event happens). Optionally press a button at a frame to reach gameplay.
// @param from integer optional "First frame to log (default 0)"
// @param to integer optional "Last frame to log (default 240)"
// @param press string optional "Button to press briefly: A, B, start, select, up, down, left, right"
// @param press_at integer optional "Frame at which to press (default 0 = no press)"
// @param max_lines integer optional "Cap listed writes (default 200); the per-frame summary is always included"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
let from = int(params, "from", 0); let to = int(params, "to", 240); let at = int(params, "press_at", 0);
let press = if params.contains("press") && params.press != () { word(params.press) } else { "" };
let max_lines = int(params, "max_lines", 200);
shell(`NESAPU_MAX_LINES=${max_lines} observe/emu_apu_trace.sh rom/game.nes /tmp/apu_trace_$$.txt ${from} ${to} "${press}" ${at} 2>&1`)
