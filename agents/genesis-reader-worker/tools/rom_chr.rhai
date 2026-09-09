// @tool rom_chr
// @description Pattern-table (tile graphics) analysis. For CHR-ROM games: dumps each 4 KB table to analysis/chr/ and reports tile counts and blank tiles. For CHR-RAM games (chr size 0) pass from_prg=true to find which PRG ranges are copied or decompressed into video memory.
// @param bank integer optional "CHR bank index"
// @param from_prg boolean optional "Locate CHR-RAM source data in PRG (CHR-RAM games)"
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
if params.contains("from_prg") && params.from_prg == true { extra += " --from-prg"; }
if params.contains("bank") && type_of(params.bank) == "i64" { extra += ` --bank ${params.bank}`; }
shell(`mkdir -p analysis/chr && nesrom chr ${rom()} --out analysis/chr --png${extra}`)
