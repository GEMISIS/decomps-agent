// @tool rom_ramlog
// @description Watch RAM variables over time in the emulator: for frames [from, to] prints the values of the given RAM addresses on every frame where any of them changed (optionally pressing a button). Use it to pin down what a variable really does: which values it takes in each state, when a flag is set/cleared relative to an input or an event, how a counter wraps, whether two "roles" of one byte are really one. Cheap and exact — prefer it over guessing from code when a variable's role is unclear or dual.
// @param addrs string required "Comma-separated RAM addresses in hex, e.g. 22,1A,0300 (up to 12)"
// @param from integer required "First frame"
// @param to integer required "Last frame (keep ranges under ~600 frames)"
// @param press string optional "Button to press for 3 frames: A, B, start, select, up, down, left, right"
// @param hold integer optional "How many frames to keep the button held (default 3; use 100-200 to find a clamp or an extreme)"
// @param press_at integer optional "Frame at which to press (default 0 = never)"
// @param max_lines integer optional "Cap output lines (default 120)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
fn hexlist(v) { let out = ""; for c in `${v}`.chars() { if "0123456789abcdefABCDEF,".contains(`${c}`) { out += c; } } out }
let addrs = hexlist(params.addrs); let from = int(params, "from", 1); let to = int(params, "to", from + 120); let at = int(params, "press_at", 0);
let hold = int(params, "hold", 3); let press = if params.contains("press") && params.press != () { word(params.press) } else { "" }; let max_lines = int(params, "max_lines", 120);
shell(`HOLD=${hold} observe/emu_ramlog.sh rom/game.nes "${addrs}" ${from} ${to} "${press}" ${at} 2>&1 | head -n ${max_lines}`)
