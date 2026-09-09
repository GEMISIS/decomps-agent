// @tool build_ramlog
// @description Watch YOUR OWN build's C variables per frame in the emulator: give C symbol names (resolved through build/game.map) or hex addresses, a frame range and an optional button press; prints the values on every frame where any changed. Use it to separate logic from presentation: if a position variable advances every frame but build_oamlog shows the sprite stalling, the sprite builder is wrong, not the physics; if a state flag flips unexpectedly, that is the bug. Run `make` first (the map file must exist).
// @param symbols string required "Comma-separated C variable names without the leading underscore (e.g. ball_x,gameplay_state_flag) or hex addresses; up to 12"
// @param from integer required "First frame"
// @param to integer required "Last frame (keep ranges under ~600 frames)"
// @param press string optional "Button to press for 3 frames: A, B, start, select, up, down, left, right"
// @param hold integer optional "How many frames to keep the button held (default 3; use 100-200 to find a clamp or an extreme)"
// @param press_at integer optional "Frame at which to press (default 0 = never)"
// @param max_lines integer optional "Cap output lines (default 120)"
// @requires shell
fn int(params, key, dflt) { if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt } }
fn word(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".contains(`${c}`) { out += c; } } out }
fn names(v) { let out = ""; for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_,".contains(`${c}`) { out += c; } } out }
let syms = names(params.symbols); let from = int(params, "from", 1); let to = int(params, "to", from + 120); let at = int(params, "press_at", 0);
let hold = int(params, "hold", 3); let press = if params.contains("press") && params.press != () { word(params.press) } else { "" }; let max_lines = int(params, "max_lines", 120);
shell(`test -f build/game.nes || echo "build/game.nes missing - run make first"; MAP=$(ls build/*.map 2>/dev/null | head -1); ADDRS=""; for s in $(echo "${syms}" | tr ',' ' '); do a=$(grep -oE "_$s +[0-9A-F]{6}" "$MAP" 2>/dev/null | head -1 | awk '{print substr($2,3,4)}'); if [ -z "$a" ]; then case "$s" in [0-9A-Fa-f]*) a="$s";; *) echo "symbol $s not in $MAP";; esac; fi; [ -n "$a" ] && ADDRS="$ADDRS,$a ($s)"; done; echo "watching:$ADDRS"; observe/emu_ramlog.sh build/game.nes "$(echo "$ADDRS" | sed 's/ ([a-z_0-9]*)//g; s/^,//')" ${from} ${to} "${press}" ${at} 2>&1 | head -n ${max_lines}`)
