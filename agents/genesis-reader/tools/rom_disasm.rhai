// @tool rom_disasm
// @description Disassemble 6502 code from a CPU address. Each instruction carries a note when it touches a hardware register (role, not just address) or a mapper register (bank switch). follow=true does recursive descent from start (follows JSR/JMP/branches) instead of a linear listing.
// @param start string required "CPU address to start at, e.g. $C000"
// @param count integer optional "Max instructions (default 64, max 400)"
// @param end string optional "Stop at this CPU address (linear mode)"
// @param follow boolean optional "Recursive descent instead of linear"
// @param bank integer optional "PRG bank index for switchable windows"
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
let count = int(params, "count", 64);
if count > 250 { count = 250; }
let extra = "";
if params.contains("end") && params.end != () { extra += ` --end ${hex(params.end)}`; }
if params.contains("follow") && params.follow == true { extra += " --follow"; }
let max_lines = int(params, "max_lines", 400);
shell(`nesrom disasm ${rom()} --start ${hex(params.start)} --count ${count}${extra}${opt_bank(params)} --text --max-lines ${max_lines}`)
