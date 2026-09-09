// @tool rom_vram
// @description Statically resolvable video-memory uploads: which routines write which palette entries / nametable regions / pattern tables, with literal values when constant. Use it to capture initial palettes and screen layouts for the spec.
// @param bank integer optional "Restrict to one PRG bank (native bank index)"
// @param entries array optional "Extra entry addresses to trace from, e.g. [\"$8608\"]"
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
let extra = "";
if params.contains("entries") && type_of(params.entries) == "array" {
    for e in params.entries { extra += ` --entry ${hex(e)}`; }
}
let max_lines = int(params, "max_lines", 400);
shell(`nesrom vram ${rom()}${opt_bank(params)}${extra} --text --max-lines ${max_lines}`)
