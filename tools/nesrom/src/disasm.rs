//! Disassembly listing (linear or recursive-descent) with labels and annotations.

use crate::cpu6502::{self, Insn};
use crate::ines::Rom;
use crate::mapper::{self, BankState, Mapper, RomLoc};
use crate::reach;
use crate::sim;
use crate::vram;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct Line {
    pub addr: String,
    pub bank: usize,
    pub file_offset: usize,
    pub bytes: String,
    pub mnemonic: &'static str,
    pub mode: &'static str,
    pub operand: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub illegal: bool,
    /// Subroutine (entry address) this line belongs to; only set in --follow mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip)]
    pub addr_num: u16,
    #[serde(skip)]
    pub target_num: Option<u16>,
    #[serde(skip)]
    pub is_jsr: bool,
}

fn note_for(insn: &Insn, m: &dyn Mapper) -> Option<String> {
    let op = insn.mem_operand()?;
    if let Some((name, role)) = cpu6502::hw_register(op) {
        return Some(format!("{name}: {role}"));
    }
    if insn.is_store() && op >= 0x8000 {
        return m.write_note(op);
    }
    None
}

pub fn line(insn: &Insn, loc: &RomLoc, m: &dyn Mapper) -> Line {
    let target = insn.target(loc.cpu_addr);
    Line {
        addr: reach::hex4(loc.cpu_addr),
        bank: loc.bank,
        file_offset: loc.file_offset,
        bytes: insn.hex_bytes(),
        mnemonic: insn.mnemonic,
        mode: insn.mode.as_str(),
        operand: if insn.illegal { format!("${:02X}", insn.opcode) } else { insn.format_operand(loc.cpu_addr) },
        target: target.map(reach::hex4),
        label: None,
        note: note_for(insn, m),
        illegal: insn.illegal,
        sub: None,
        addr_num: loc.cpu_addr,
        target_num: target,
        is_jsr: insn.is_jsr(),
    }
}

/// Replace the generic register role with the resolved VRAM note.
fn merge_note(l: &mut Line, vram_note: &str) {
    let name = l.note.as_deref().and_then(|n| n.split(':').next()).unwrap_or("VRAM-QUEUE").to_string();
    l.note = Some(format!("{name}: {vram_note}"));
}

fn apply_labels(lines: &mut [Line]) {
    let mut labels: BTreeMap<u16, &'static str> = BTreeMap::new();
    for l in lines.iter() {
        if let Some(t) = l.target_num {
            let e = labels.entry(t).or_insert("loc");
            if l.is_jsr { *e = "sub"; }
        }
    }
    for l in lines.iter_mut() {
        if let Some(k) = labels.get(&l.addr_num) {
            l.label = Some(format!("{k}_{:04X}", l.addr_num));
        }
    }
}

/// Linear disassembly from `start` (resolved via `bank` or state) for `count` instructions or until `end`.
pub fn linear(rom: &Rom, m: &dyn Mapper, start: u16, bank: Option<usize>, end: Option<u16>, count: usize, st: &BankState) -> Result<Vec<Line>> {
    let first = mapper::resolve(m, rom, start, bank, st)?;
    let st = match bank { Some(b) => m.state_for_bank(b, start, st), None => st.clone() };
    let mut lines = Vec::new();
    let mut pc = start;
    let mut n = 0;
    let limit = if end.is_some() { usize::MAX } else { count };
    let mut regs = sim::Regs::default();
    let mut vs = vram::Vram::default();
    while n < limit {
        if let Some(e) = end { if pc > e { break; } }
        let loc = match mapper::cpu_to_rom(m, rom, pc, &st) { Some(l) => l, None => break };
        let stop = (loc.file_offset + 3).min(rom.prg.len());
        let insn = cpu6502::decode(&rom.prg[loc.file_offset..stop]);
        let mut l = line(&insn, &loc, m);
        if let Some(ev) = vram::step(&insn, &regs, &mut vs, &[]) { merge_note(&mut l, &vram::note_for(&ev)); }
        if insn.is_return() || insn.is_jmp_abs() { vs = vram::Vram::default(); regs = sim::Regs::default(); } else { sim::update(&insn, &mut regs, rom, m, &st); }
        lines.push(l);
        let next = pc.wrapping_add(insn.len as u16);
        if next < pc { break; }
        pc = next;
        n += 1;
    }
    let _ = first;
    apply_labels(&mut lines);
    Ok(lines)
}

/// Recursive-descent disassembly from `start`.
pub fn follow(rom: &Rom, m: &dyn Mapper, start: u16, bank: Option<usize>, st: &BankState, max_lines: usize) -> Result<Vec<Line>> {
    mapper::resolve(m, rom, start, bank, st)?;
    let st = match bank { Some(b) => m.state_for_bank(b, start, st), None => st.clone() };
    let known = vram::detect_queues(&reach::trace(rom, m, &[], true));
    let t = reach::trace_from_with_queues(rom, m, vec![("start".to_string(), start, st)], known);
    let mut lines: Vec<Line> = t.insns.values().map(|ti| {
        let loc = RomLoc { cpu_addr: ti.cpu_addr, bank: ti.bank, offset: 0, file_offset: ti.file_offset };
        let mut l = line(&ti.insn, &loc, m);
        l.sub = ti.sub.and_then(|s| t.subs.get(&s)).map(|s| reach::hex4(s.cpu_addr));
        if let Some(nt) = t.notes.get(&ti.file_offset) { merge_note(&mut l, nt); }
        l
    }).collect();
    lines.sort_by_key(|l| (l.bank, l.addr_num));
    lines.truncate(max_lines);
    apply_labels(&mut lines);
    Ok(lines)
}

pub fn render_text(lines: &[Line], show_bytes: bool, follow: bool) -> String {
    let mut s = String::new();
    let mut sorted: Vec<&Line> = lines.iter().collect();
    if follow { sorted.sort_by_key(|l| (l.sub.clone(), l.bank, l.addr_num)); }
    let mut current_sub: Option<String> = None;
    for l in sorted {
        if follow && l.sub != current_sub {
            if current_sub.is_some() { s.push('\n'); }
            current_sub = l.sub.clone();
        }
        if let Some(lab) = &l.label { s.push_str(&format!("{lab}:\n")); }
        let mut row = if show_bytes {
            format!("  {} b{:02} {:<9} {} {}", l.addr, l.bank, l.bytes, l.mnemonic, l.operand)
        } else {
            format!("  {} b{:02} {} {}", l.addr, l.bank, l.mnemonic, l.operand)
        };
        if let Some(n) = &l.note { row.push_str(&format!(" ; {n}")); }
        s.push_str(row.trim_end());
        s.push('\n');
    }
    s
}
