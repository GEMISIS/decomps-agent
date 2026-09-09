//! nesrom: deterministic NES ROM analysis and asset extraction.

use anyhow::Result;
use nesrom::{apu, chr, compress, disasm, extract, ines, lint, mapper, reach, stats, text, vram, xrefs};
use clap::{Parser, Subcommand};
use ines::Rom;
use mapper::parse_addr;
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "nesrom", version, about = "Deterministic NES ROM analysis and asset extraction")]
struct Cli {
    /// Compact line-oriented text output instead of JSON
    #[arg(long, global = true)]
    text: bool,
    /// Truncate text output to N lines (appends a hint)
    #[arg(long, global = true)]
    max_lines: Option<usize>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse the iNES / NES 2.0 header and describe the bank layout
    Header { rom: String },
    /// Hex dump a range of PRG ROM
    Bytes { rom: String, #[arg(long)] addr: String, #[arg(long)] bank: Option<usize>, #[arg(long, default_value_t = 64)] len: usize },
    /// Disassemble from an address (linear, or recursive descent with --follow)
    Disasm { rom: String, #[arg(long)] start: String, #[arg(long)] bank: Option<usize>, #[arg(long)] end: Option<String>, #[arg(long, default_value_t = 64)] count: usize, #[arg(long)] follow: bool, #[arg(long)] show_bytes: bool },
    /// Recursive-descent reachability from the vectors (plus --entry addresses)
    Reach { rom: String, #[arg(long)] bank: Option<usize>, #[arg(long)] entry: Vec<String>, #[arg(long)] no_vectors: bool },
    /// Cross-references to an address
    Xrefs { rom: String, #[arg(long)] addr: String },
    /// Entropy / repetition statistics over a range
    Entropy { rom: String, #[arg(long)] start: String, #[arg(long)] len: usize, #[arg(long)] bank: Option<usize>, #[arg(long, default_value_t = 256)] window: usize },
    /// Dump CHR pattern tables (or locate CHR-RAM sources in PRG with --from-prg)
    Chr { rom: String, #[arg(long)] bank: Option<usize>, #[arg(long)] from_prg: bool, #[arg(long)] out: Option<String>, #[arg(long)] png: bool },
    /// APU register usage grouped by routine
    Apu { rom: String },
    /// Statically resolvable VRAM uploads (palettes, nametables, pattern tables)
    Vram { rom: String, #[arg(long)] bank: Option<usize>, #[arg(long)] entry: Vec<String> },
    /// Try a decoder on a range and report plausibility
    DecodeTry { rom: String, #[arg(long)] addr: String, #[arg(long)] len: usize, #[arg(long)] bank: Option<usize>, #[arg(long)] decoder: String, #[arg(long)] max_out: Option<usize> },
    /// Extract assets described by a manifest
    Extract { #[arg(long)] rom: String, #[arg(long)] manifest: String, #[arg(long)] out: String },
    /// Barrier lint: refuse a spec that leaks addresses/opcodes
    LintSpec { spec: String },
}

fn header(rom: &Rom) -> Result<Value> {
    let supported = mapper::is_supported(rom.header.mapper);
    let mut v = json!({ "rom": rom.info(), "header": rom.header, "supported": supported, "prg_file_base": rom.prg_file_base });
    if supported {
        let m = mapper::for_rom(rom)?;
        let st = m.reset_state();
        let mut vectors = serde_json::Map::new();
        for (name, va) in [("nmi", 0xFFFAu16), ("reset", 0xFFFC), ("irq", 0xFFFE)] {
            let entry = mapper::cpu_to_rom(m.as_ref(), rom, va, &st).and_then(|l| mapper::read_u16(&rom.prg, l.file_offset)).map(|t| {
                let loc = mapper::cpu_to_rom(m.as_ref(), rom, t, &st);
                json!({ "addr": reach::hex4(t), "bank": loc.as_ref().map(|l| l.bank), "file_offset": loc.as_ref().map(|l| l.file_offset + rom.prg_file_base) })
            });
            vectors.insert(name.into(), entry.unwrap_or(Value::Null));
        }
        v["mapper_name"] = json!(m.name());
        v["prg_bank_size"] = json!(m.prg_bank_size());
        v["prg_bank_count"] = json!(m.bank_count());
        v["chr_bank_count_8k"] = json!(m.chr_bank_count_8k());
        v["vectors"] = Value::Object(vectors);
        v["bank_layout"] = json!(m.windows());
        v["reset_state_prg8k"] = json!(st.prg8k);
    }
    Ok(v)
}

fn bytes(rom: &Rom, addr: &str, bank: Option<usize>, len: usize) -> Result<Value> {
    let m = mapper::for_rom(rom)?;
    let a = parse_addr(addr)?;
    let loc = mapper::resolve(m.as_ref(), rom, a, bank, &m.reset_state())?;
    let end = (loc.file_offset + len).min(rom.prg.len());
    let data = &rom.prg[loc.file_offset..end];
    let rows: Vec<Value> = data.chunks(16).enumerate().map(|(i, c)| {
        json!({
            "addr": reach::hex4(a.wrapping_add((i * 16) as u16)),
            "file_offset": loc.file_offset + i * 16 + rom.prg_file_base,
            "hex": c.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
            "ascii": c.iter().map(|&b| if (0x20..0x7F).contains(&b) { b as char } else { '.' }).collect::<String>(),
        })
    }).collect();
    Ok(json!({ "rom": rom.info(), "start": reach::hex4(a), "bank": loc.bank, "len": data.len(), "rows": rows }))
}

fn run(cli: &Cli) -> Result<(Value, Option<String>)> {
    Ok(match &cli.cmd {
        Cmd::Header { rom } => {
            let r = Rom::load(rom)?;
            let v = header(&r)?;
            let t = text::header(&v);
            (v, Some(t))
        }
        Cmd::Bytes { rom, addr, bank, len } => {
            let r = Rom::load(rom)?;
            let v = bytes(&r, addr, *bank, *len)?;
            let t = text::bytes(&v);
            (v, Some(t))
        }
        Cmd::Disasm { rom, start, bank, end, count, follow, show_bytes } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let s = parse_addr(start)?;
            let e = end.as_deref().map(parse_addr).transpose()?;
            let lines = if *follow { disasm::follow(&r, m.as_ref(), s, *bank, &m.reset_state(), if end.is_some() { usize::MAX } else { (*count).max(64) * 8 })? } else { disasm::linear(&r, m.as_ref(), s, *bank, e, *count, &m.reset_state())? };
            let t = disasm::render_text(&lines, *show_bytes, *follow);
            (json!({ "rom": r.info(), "start": reach::hex4(s), "follow": follow, "instructions": lines }), Some(t))
        }
        Cmd::Reach { rom, bank, entry, no_vectors } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let extra: Vec<(u16, Option<usize>)> = entry.iter().map(|e| parse_addr(e).map(|a| (a, *bank))).collect::<Result<_>>()?;
            let t = reach::trace(&r, m.as_ref(), &extra, !*no_vectors);
            let v = t.to_json(&r, m.as_ref(), *bank);
            let tx = text::reach(&v);
            (v, Some(tx))
        }
        Cmd::Xrefs { rom, addr } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let v = xrefs::find(&r, m.as_ref(), parse_addr(addr)?);
            let t = text::xrefs(&v);
            (v, Some(t))
        }
        Cmd::Entropy { rom, start, len, bank, window } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let s = parse_addr(start)?;
            let loc = mapper::resolve(m.as_ref(), &r, s, *bank, &m.reset_state())?;
            let end = (loc.file_offset + len).min(r.prg.len());
            let w = stats::analyze(&r.prg[loc.file_offset..end], s, loc.file_offset, (*window).max(16));
            let v = json!({ "rom": r.info(), "bank": loc.bank, "windows": w });
            let t = text::entropy(&v);
            (v, Some(t))
        }
        Cmd::Chr { rom, bank, from_prg, out, png } => {
            let r = Rom::load(rom)?;
            if *from_prg {
                let m = mapper::for_rom(&r)?;
                let v = chr::from_prg(&r, m.as_ref());
                let t = text::chr_from_prg(&v);
                (v, Some(t))
            } else {
                let v = chr::dump(&r, *bank, out.as_deref(), *png)?;
                let t = text::chr(&v);
                (v, Some(t))
            }
        }
        Cmd::Apu { rom } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let v = apu::analyze(&r, m.as_ref());
            let t = text::apu(&v);
            (v, Some(t))
        }
        Cmd::Vram { rom, bank, entry } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let extra: Vec<(u16, Option<usize>)> = entry.iter().map(|e| parse_addr(e).map(|a| (a, *bank))).collect::<Result<_>>()?;
            let t = reach::trace(&r, m.as_ref(), &extra, true);
            let mut v = vram::to_json(&t, *bank);
            v["rom"] = r.info();
            let tx = vram::text(&v);
            (v, Some(tx))
        }
        Cmd::DecodeTry { rom, addr, len, bank, decoder, max_out } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let a = parse_addr(addr)?;
            let loc = mapper::resolve(m.as_ref(), &r, a, *bank, &m.reset_state())?;
            let end = (loc.file_offset + len).min(r.prg.len());
            let input = &r.prg[loc.file_offset..end];
            let d = compress::decode(decoder, input, *max_out)?;
            let sample: String = d.data.iter().take(64).map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
            let v = json!({
                "rom": r.info(), "decoder": decoder, "start": reach::hex4(a), "bank": loc.bank,
                "input_len": input.len(), "output_len": d.data.len(), "ok": d.ok, "consumed": d.consumed,
                "ratio": if d.consumed > 0 { d.data.len() as f64 / d.consumed as f64 } else { 0.0 },
                "tileness": (stats::tileness(&d.data) * 1000.0).round() / 1000.0,
                "entropy_bits": (stats::entropy(&d.data) * 1000.0).round() / 1000.0,
                "sample_hex": sample,
            });
            let t = text::decode_try(&v);
            (v, Some(t))
        }
        Cmd::Extract { rom, manifest, out } => {
            let r = Rom::load(rom)?;
            let m = mapper::for_rom(&r)?;
            let rep = extract::run(&r, m.as_ref(), manifest, out)?;
            let v = json!({ "rom": r.info(), "out": out, "assets": rep });
            let t = text::extract(&v);
            (v, Some(t))
        }
        Cmd::LintSpec { spec } => {
            let text = std::fs::read_to_string(spec)?;
            let v: Value = serde_json::from_str(&text)?;
            let viol = lint::lint(&v);
            let ok = viol.is_empty();
            let out = json!({ "ok": ok, "violations": viol });
            let t = text::lint(&out);
            if !ok {
                emit(&out, if cli.text { Some(text::cap(t, cli.max_lines)) } else { None });
                std::process::exit(1);
            }
            (out, Some(t))
        }
    })
}

fn emit(v: &Value, text: Option<String>) {
    match text {
        Some(t) => println!("{t}"),
        None => println!("{}", serde_json::to_string_pretty(v).unwrap()),
    }
}

fn main() {
    let cli = Cli::parse();
    match run(&cli) {
        Ok((v, t)) => emit(&v, if cli.text { Some(text::cap(t.unwrap_or_default(), cli.max_lines)) } else { None }),
        Err(e) => {
            println!("{}", json!({ "error": format!("{e:#}") }));
            std::process::exit(1);
        }
    }
}
