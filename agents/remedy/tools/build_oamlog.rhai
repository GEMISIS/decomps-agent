// @tool build_oamlog
// @description Per-frame sprite (OAM) timeline of YOUR OWN build (build/game.nes) in the emulator: for every frame in [from, to] prints the OAM records whose tile matches `tile` (or every visible record when tile is omitted) as "fN [slot] x= y= attr=". Use it after every state change (button press, first scoring event, respawn) to capture EXACT motion: the frame an object first appears, its position each frame, its attribute/tile animation cycle (flip bits, palette), the runtime OAM slot order, rest positions and pause lengths, and lag frames (frames where nothing changed). Compare the output line by line with the spec.s `timelines` and `acceptance_observations`. Run `make` first.
// @param from integer required "First frame to log"
// @param to integer required "Last frame to log (keep ranges under ~120 frames per call)"
// @param press string optional "Button to press for 3 frames: A, B, start, select, up, down, left, right"
// @param hold integer optional "How many frames to keep the button held (default 3; use 100-200 to find a clamp or an extreme)"
// @param press_at integer optional "Frame at which to press (default 0 = never)"
// @param tile integer optional "Only log OAM records with this tile number (decimal); omit for all visible sprites"
// @param max_lines integer optional "Cap output lines (default 130)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
let from = int(params, "from", 1); let to = int(params, "to", from + 60); let at = int(params, "press_at", 0);
let tile = int(params, "tile", -1); let max_lines = int(params, "max_lines", 130);
let hold = int(params, "hold", 3); let press = if params.contains("press") && params.press != () { word(params.press) } else { "" };
shell(`test -f build/game.nes || echo "build/game.nes missing - run make first"; observe/emu_oamlog.sh build/game.nes ${from} ${to} "${press}" ${at} ${tile} 2>&1 | head -n ${max_lines}`)
