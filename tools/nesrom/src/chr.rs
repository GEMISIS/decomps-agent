//! CHR pattern-table dumping and CHR-RAM source detection.

use crate::cpu6502::Mode;
use crate::ines::Rom;
use crate::mapper::Mapper;
use crate::reach::{self, hex4};
use crate::stats;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

/// Render 256 tiles (4 KB) as a 128x128 grayscale image (4 shades).
pub fn tiles_to_gray(data: &[u8]) -> Vec<u8> {
    let tiles = data.len() / 16;
    let cols = 16;
    let rows = (tiles + cols - 1) / cols;
    let w = cols * 8;
    let h = rows.max(1) * 8;
    let mut img = vec![0u8; w * h];
    for t in 0..tiles {
        let tile = &data[t * 16..t * 16 + 16];
        let tx = (t % cols) * 8;
        let ty = (t / cols) * 8;
        for y in 0..8 {
            for x in 0..8 {
                let bit = 7 - x;
                let lo = (tile[y] >> bit) & 1;
                let hi = (tile[y + 8] >> bit) & 1;
                let v = lo | (hi << 1);
                img[(ty + y) * w + tx + x] = [0, 85, 170, 255][v as usize];
            }
        }
    }
    img
}

pub fn write_png(path: &Path, data: &[u8]) -> Result<()> {
    let tiles = data.len() / 16;
    let rows = ((tiles + 15) / 16).max(1);
    let (w, h) = (128u32, (rows * 8) as u32);
    let img = tiles_to_gray(data);
    let file = std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header()?;
    writer.write_image_data(&img)?;
    Ok(())
}

pub fn dump(rom: &Rom, bank: Option<usize>, out: Option<&str>, png: bool) -> Result<Value> {
    let banks = rom.chr.len() / 8192;
    let mut tables = Vec::new();
    let sel: Vec<usize> = match bank { Some(b) => vec![b], None => (0..banks).collect() };
    for b in sel {
        if b >= banks { anyhow::bail!("CHR bank {b} out of range ({banks} banks)"); }
        let data = &rom.chr[b * 8192..(b + 1) * 8192];
        let mut files = Vec::new();
        if let Some(dir) = out {
            std::fs::create_dir_all(dir)?;
            let raw = Path::new(dir).join(format!("chr_bank_{b}.chr"));
            std::fs::write(&raw, data)?;
            files.push(raw.display().to_string());
            if png {
                for (i, half) in data.chunks(4096).enumerate() {
                    let p = Path::new(dir).join(format!("chr_bank_{b}_table{i}.png"));
                    write_png(&p, half)?;
                    files.push(p.display().to_string());
                }
            }
        }
        for (i, half) in data.chunks(4096).enumerate() {
            let nonblank = half.chunks(16).filter(|t| t.iter().any(|&x| x != 0)).count();
            tables.push(json!({
                "chr_bank": b, "table": i, "ppu_addr": format!("${:04X}", i * 0x1000), "tile_count": 256,
                "nonblank_tiles": nonblank, "entropy_bits": (stats::entropy(half) * 1000.0).round() / 1000.0,
            }));
        }
        if !files.is_empty() { tables.push(json!({"chr_bank": b, "files": files})); }
    }
    Ok(json!({ "rom": rom.info(), "chr_size": rom.chr.len(), "chr_banks": banks, "chr_ram": rom.header.chr_ram, "tables": tables }))
}

/// For CHR-RAM games: find routines that stream PRG bytes into PPUDATA ($2007)
/// and report candidate source ranges.
pub fn from_prg(rom: &Rom, m: &dyn Mapper) -> Value {
    let t = reach::trace(rom, m, &[], true);
    let mut candidates = Vec::new();
    let mut routines = Vec::new();
    let writer_subs: std::collections::BTreeSet<usize> = t.hw.iter()
        .filter(|h| h.addr_num == 0x2007 && h.access == "write")
        .filter_map(|h| t.insns.get(&h.from_off).and_then(|i| i.sub))
        .collect();
    for sub_off in writer_subs {
        let Some(sub) = t.subs.get(&sub_off) else { continue };
        let insns = t.insns_of_sub(sub_off);
        let abs_src: Vec<u16> = insns.iter().filter(|i| i.insn.is_load() && matches!(i.insn.mode, Mode::Abx | Mode::Aby | Mode::Abs) && i.insn.operand.map(|o| o >= 0x8000).unwrap_or(false)).filter_map(|i| i.insn.operand).collect();
        let mut ind_ptrs: Vec<u16> = insns.iter().filter(|i| i.insn.is_load() && matches!(i.insn.mode, Mode::Izy | Mode::Izx)).filter_map(|i| i.insn.operand).collect();
        ind_ptrs.sort();
        ind_ptrs.dedup();
        let bit_ops = insns.iter().filter(|i| matches!(i.insn.mnemonic, "EOR" | "ASL" | "LSR" | "ROL" | "ROR")).count();
        let cmp_imm: Vec<u16> = insns.iter().filter(|i| matches!(i.insn.mnemonic, "CPX" | "CPY" | "CMP") && i.insn.mode == Mode::Imm).filter_map(|i| i.insn.operand).collect();
        let how = if !abs_src.is_empty() { "direct_copy" } else if !ind_ptrs.is_empty() && bit_ops >= 2 { "decompressed" } else if !ind_ptrs.is_empty() { "pointer_copy" } else { "unknown" };
        routines.push(json!({
            "routine": hex4(sub.cpu_addr), "bank": sub.bank, "how": how,
            "pointer_zp": ind_ptrs.iter().map(|p| format!("${p:02X}")).collect::<Vec<_>>(),
            "callers": sub.callers.iter().filter_map(|c| t.insns.get(c)).map(|i| hex4(i.cpu_addr)).collect::<Vec<_>>(),
        }));
        for src in &abs_src {
            let len = cmp_imm.iter().copied().max().map(|v| v as usize).filter(|&v| v > 0).unwrap_or(256);
            candidates.push(json!({ "bank": sub.bank, "start": hex4(*src), "len": len, "confidence": 0.6, "how": "direct_copy", "routine": hex4(sub.cpu_addr) }));
        }
        // Pointer set-up in callers: LDA #lo / STA zp / LDA #hi / STA zp+1 before the JSR.
        for zp in &ind_ptrs {
            for c in &sub.callers {
                let Some(call) = t.insns.get(c) else { continue };
                let Some(caller_sub) = call.sub else { continue };
                let mut lo: Option<u8> = None;
                let mut hi: Option<u8> = None;
                let mut a: Option<u8> = None;
                let mut body: Vec<&reach::TraceInsn> = t.insns_of_sub(caller_sub).into_iter().filter(|i| i.file_offset < call.file_offset).collect();
                body.sort_by_key(|i| i.file_offset);
                for i in body.iter().rev().take(24).collect::<Vec<_>>().into_iter().rev() {
                    match (i.insn.mnemonic, i.insn.mode) {
                        ("LDA", Mode::Imm) => a = i.insn.operand.map(|v| v as u8),
                        ("STA", Mode::Zp) => {
                            if i.insn.operand == Some(*zp) { lo = a; }
                            if i.insn.operand == Some(zp + 1) { hi = a; }
                        }
                        ("LDA", _) => a = None,
                        _ => {}
                    }
                }
                if let (Some(l), Some(h)) = (lo, hi) {
                    let start = (h as u16) << 8 | l as u16;
                    if start >= 0x8000 {
                        candidates.push(json!({ "bank": call.bank, "start": hex4(start), "len": 0, "confidence": if how == "decompressed" { 0.5 } else { 0.45 }, "how": how, "routine": hex4(sub.cpu_addr), "set_up_at": hex4(call.cpu_addr) }));
                    }
                }
            }
        }
    }
    json!({ "rom": rom.info(), "chr_ram": rom.header.chr_ram, "ppudata_writer_routines": routines, "candidate_prg_sources": candidates })
}
