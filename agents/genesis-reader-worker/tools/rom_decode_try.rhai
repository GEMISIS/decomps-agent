// @tool rom_decode_try
// @description Try a decompressor on a byte range and report whether it produced plausible output (size, tile-likeness, entropy, sample). Decoders: rle, pb53, lz, or script:<path to a .rhai file you wrote under spec/decoders/ that defines fn decode(bytes)>.
// @param addr string required "CPU address of the compressed data"
// @param len integer required "Max input bytes to feed the decoder"
// @param decoder string required "rle | pb53 | lz | script:spec/decoders/<name>.rhai"
// @param max_out integer optional "Cap decoded output bytes (pb53 needs a tile bound: tiles*16; default 8192)"
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
let len = int(params, "len", 4096);
let dec = word(params.decoder);
let max_out = int(params, "max_out", 8192);
let max_lines = int(params, "max_lines", 400);
shell(`nesrom decode-try ${rom()} --addr ${hex(params.addr)} --len ${len} --decoder ${dec} --max-out ${max_out}${opt_bank(params)} --text --max-lines ${max_lines}`)
