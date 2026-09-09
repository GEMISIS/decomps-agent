//! Byte-range statistics for classifying data regions.

use crate::cpu6502;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct WindowStats {
    pub start: String,
    pub file_offset: usize,
    pub len: usize,
    pub entropy_bits: f64,
    pub repetition_ratio: f64,
    pub zero_ratio: f64,
    pub ff_ratio: f64,
    pub tileness: f64,
    pub code_plausibility: f64,
    pub text_ratio: f64,
    pub guess: &'static str,
}

pub fn entropy(bytes: &[u8]) -> f64 {
    if bytes.is_empty() { return 0.0; }
    let mut counts = [0usize; 256];
    for &b in bytes { counts[b as usize] += 1; }
    let n = bytes.len() as f64;
    counts.iter().filter(|&&c| c > 0).map(|&c| { let p = c as f64 / n; -p * p.log2() }).sum()
}

/// Fraction of 16-byte blocks that look like 2bpp tiles.
pub fn tileness(bytes: &[u8]) -> f64 {
    let blocks: Vec<&[u8]> = bytes.chunks_exact(16).collect();
    if blocks.is_empty() { return 0.0; }
    let tile_like = blocks.iter().filter(|b| {
        let all_same = b.iter().all(|&x| x == b[0]);
        if all_same { return false; }
        let distinct = { let mut s = [false; 256]; let mut n = 0; for &x in b.iter() { if !s[x as usize] { s[x as usize] = true; n += 1; } } n };
        let p0 = &b[0..8];
        let p1 = &b[8..16];
        let rows_vary = (1..8).any(|i| p0[i] != p0[0]) || (1..8).any(|i| p1[i] != p1[0]);
        // Tiles rarely use many distinct byte values and their planes are correlated.
        let overlap = (0..8).filter(|&i| p0[i] & p1[i] != 0 || p0[i] == p1[i]).count();
        distinct <= 11 && rows_vary && overlap >= 2
    }).count();
    tile_like as f64 / blocks.len() as f64
}

/// Fraction of a linear decode that is legal, "typical" 6502 code.
pub fn code_plausibility(bytes: &[u8]) -> f64 {
    if bytes.is_empty() { return 0.0; }
    let mut i = 0;
    let mut total = 0usize;
    let mut good = 0usize;
    while i < bytes.len() {
        let insn = cpu6502::decode(&bytes[i..]);
        total += 1;
        if !insn.illegal {
            let common = matches!(insn.mnemonic, "LDA" | "STA" | "LDX" | "LDY" | "STX" | "STY" | "JSR" | "RTS" | "JMP" | "BNE" | "BEQ" | "CMP" | "INC" | "DEC" | "INX" | "INY" | "DEX" | "DEY" | "AND" | "ORA" | "ADC" | "SBC" | "BCC" | "BCS" | "BPL" | "BMI" | "TAX" | "TAY" | "TXA" | "TYA" | "PHA" | "PLA" | "CLC" | "SEC" | "ASL" | "LSR" | "ROL" | "ROR" | "EOR" | "BIT" | "CPX" | "CPY" | "RTI" | "SEI" | "CLD" | "NOP" | "TXS");
            if common { good += 1; }
        }
        i += insn.len as usize;
    }
    good as f64 / total as f64
}

pub fn analyze(bytes: &[u8], start_addr: u16, file_offset: usize, window: usize) -> Vec<WindowStats> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let end = (pos + window).min(bytes.len());
        let w = &bytes[pos..end];
        let n = w.len() as f64;
        let ent = entropy(w);
        let rep = w.windows(2).filter(|p| p[0] == p[1]).count() as f64 / (n - 1.0).max(1.0);
        let zero = w.iter().filter(|&&b| b == 0).count() as f64 / n;
        let ff = w.iter().filter(|&&b| b == 0xFF).count() as f64 / n;
        let tile = tileness(w);
        let code = code_plausibility(w);
        let text = w.iter().filter(|&&b| (0x20..=0x7E).contains(&b)).count() as f64 / n;
        let guess = if zero > 0.9 || ff > 0.9 {
            "empty"
        } else if text > 0.85 {
            "text"
        } else if code > 0.72 && ent > 4.0 {
            "code"
        } else if tile > 0.5 {
            "tiles"
        } else if ent > 7.4 {
            "compressed"
        } else {
            "table"
        };
        out.push(WindowStats {
            start: format!("${:04X}", start_addr.wrapping_add(pos as u16)),
            file_offset: file_offset + pos,
            len: w.len(),
            entropy_bits: (ent * 1000.0).round() / 1000.0,
            repetition_ratio: (rep * 1000.0).round() / 1000.0,
            zero_ratio: (zero * 1000.0).round() / 1000.0,
            ff_ratio: (ff * 1000.0).round() / 1000.0,
            tileness: (tile * 1000.0).round() / 1000.0,
            code_plausibility: (code * 1000.0).round() / 1000.0,
            text_ratio: (text * 1000.0).round() / 1000.0,
            guess,
        });
        pos = end;
    }
    out
}
