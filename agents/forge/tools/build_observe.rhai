// @tool build_observe
// @description Run YOUR OWN build (build/game.nes) in an emulator for N frames (optionally pressing a button at a frame) and report the resulting screen state: the 32 palette entries, the 32x30 nametable tile grid, attributes, visible sprites and zero-page RAM. Use it in verify/integrate to check the spec's `video`, `state_machine` and `acceptance_observations` against what the build actually does. Run `make` first.
// @param frames integer optional "Frames to run (default 240)"
// @param press string optional "Button to press briefly (twice): A, B, start, select, up, down, left, right"
// @param press_at integer optional "Frame at which to press (default 120)"
// @param max_lines integer optional "Cap output lines (default 80)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
let frames = int(params, "frames", 240); let at = int(params, "press_at", 120);
let press = if params.contains("press") && params.press != () { word(params.press) } else { "" };
let max_lines = int(params, "max_lines", 80);
shell(`test -f build/game.nes || echo "build/game.nes missing - run make first"; observe/emu_observe.sh build/game.nes ${frames} "${press}" ${at} 2>&1 | head -n ${max_lines}`)
