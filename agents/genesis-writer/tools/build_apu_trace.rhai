// @tool build_apu_trace
// @description Run YOUR OWN build (build/game.nes) in an emulator and log every sound (APU) register write per frame. Use it to verify the audio driver actually plays: the spec's `acceptance_observations` say how many register writes per frame the music produces and which registers a sound effect touches.
// @param from integer optional "First frame to log (default 0)"
// @param to integer optional "Last frame to log (default 240)"
// @param press string optional "Button to press briefly: A, B, start, select, up, down, left, right"
// @param press_at integer optional "Frame at which to press (default 0 = no press)"
// @param max_lines integer optional "Cap listed writes (default 120)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
let from = int(params, "from", 0); let to = int(params, "to", 240); let at = int(params, "press_at", 0);
let press = if params.contains("press") && params.press != () { word(params.press) } else { "" };
let max_lines = int(params, "max_lines", 120);
shell(`test -f build/game.nes || echo "build/game.nes missing - run make first"; NESAPU_MAX_LINES=${max_lines} observe/emu_apu_trace.sh build/game.nes /tmp/apu_trace_$$.txt ${from} ${to} "${press}" ${at} 2>&1`)
