// @tool rom_observe
// @description Run the ROM in an emulator for N frames (optionally pressing a button at a given frame) and report what is on screen: the 32 palette entries as they end up in PPU palette RAM, the 32x30 nametable tile grid, attribute bytes, visible sprites, and zero-page RAM. Use it to capture exact initial palettes, screen layouts, text positions and border tiles, and to confirm behaviour you inferred statically. Pure observation of behaviour; costs a few seconds per call.
// @param frames integer optional "Frames to run before observing (default 240; use 60-120 for the first screen, more for later states)"
// @param press string optional "Button to press briefly (twice): A, B, start, select, up, down, left, right"
// @param press_at integer optional "Frame at which to press (default 120)"
// @param max_lines integer optional "Cap output lines (default 80)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
let frames = int(params, "frames", 240);
let at = int(params, "press_at", 120);
let press = if params.contains("press") && params.press != () { word(params.press) } else { "" };
let max_lines = int(params, "max_lines", 80);
shell(`observe/emu_observe.sh rom/game.nes ${frames} "${press}" ${at} 2>&1 | head -n ${max_lines}`)
