// @tool rom_entropy
// @description Statistical classification of a byte range in sliding windows: entropy, repetition, zero ratio, tile-likeness, and a guess (code / tiles / text / table / compressed / empty). Use on unreached ranges to decide what kind of data they hold.
// @param start string required "CPU address"
// @param len integer required "Bytes to analyze"
// @param window integer optional "Window size (default 256)"
// @param bank integer optional "PRG bank index"
// @param max_lines integer optional "Cap output lines (default 400)"
// @requires shell
// Helpers ------------------------------------------------------------------
// The ROM is always at rom/game.nes inside the run workdir. The model never
// chooses the path, so every ROM read in the run journal is this one file.
fn rom() { "rom/game.nes" }
// Normalize "$C000" / "0xC000" / "C000" to 0xC000 and strip anything that is
// not a hex digit, so nothing the model types can reach the shell unescaped.
fn hex(v) {
    let t = `${v}`.to_upper();
    t.replace("$", ""); t.replace("0X", "");
    let out = "";
    for c in t.chars() { if "0123456789ABCDEF".contains(`${c}`) { out += c; } }
    if out.len() == 0 { throw `bad address: ${v}`; }
    "0x" + out
}
fn int(params, key, dflt) {
    if params.contains(key) && type_of(params[key]) == "i64" { params[key] } else { dflt }
}
fn opt_bank(params) { if params.contains("bank") && type_of(params.bank) == "i64" { ` --bank ${params.bank}` } else { "" } }
fn word(v) {
    let out = "";
    for c in `${v}`.chars() { if "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-./:".contains(`${c}`) { out += c; } }
    out
}
let len = int(params, "len", 1024);
let win = int(params, "window", 256);
let max_lines = int(params, "max_lines", 400);
shell(`nesrom entropy ${rom()} --start ${hex(params.start)} --len ${len} --window ${win}${opt_bank(params)} --text --max-lines ${max_lines}`)
