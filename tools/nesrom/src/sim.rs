//! Lightweight constant propagation for A/X/Y, the stack, and zero page along a trace.

use crate::cpu6502::{Insn, Mode};
use crate::ines::Rom;
use crate::mapper::{self, BankState, Mapper};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Src { Unknown, Imm, Table(u16), Ptr(u8), Mem(u16), Stack, Reg }

#[derive(Debug, Clone)]
pub struct Val { pub v: Option<u8>, pub src: Src }

impl Default for Val { fn default() -> Self { Val { v: None, src: Src::Unknown } } }

impl Val {
    pub fn imm(v: u8) -> Self { Val { v: Some(v), src: Src::Imm } }
    pub fn unknown() -> Self { Val::default() }
    pub fn map(&self, f: impl Fn(u8) -> u8) -> Val { Val { v: self.v.map(f), src: if self.v.is_some() { self.src.clone() } else { Src::Unknown } } }
    /// Human description of where a value comes from.
    pub fn describe(&self, reg: &str) -> String {
        if let Some(v) = self.v { return format!("#${v:02X}"); }
        match &self.src {
            Src::Table(t) => format!("table ${t:04X},idx"),
            Src::Ptr(z) => format!("(${z:02X}),Y"),
            Src::Mem(a) => if *a < 0x100 { format!("${a:02X}") } else { format!("${a:04X}") },
            Src::Stack => "stack".into(),
            _ => reg.into(),
        }
    }
    pub fn source_kind(&self) -> String {
        match &self.src {
            Src::Imm => "immediate".into(),
            Src::Table(t) => format!("table ${t:04X}"),
            Src::Ptr(_) => "pointer".into(),
            Src::Mem(a) => format!("memory ${a:04X}"),
            Src::Stack => "stack".into(),
            _ => "unknown".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Regs { pub a: Val, pub x: Val, pub y: Val, pub stack: Vec<Val>, pub zp: BTreeMap<u8, Val> }

impl Regs {
    pub fn get(&self, name: &str) -> &Val { match name { "A" => &self.a, "X" => &self.x, _ => &self.y } }
    /// Value the instruction stores (STA/STX/STY) and its register name.
    pub fn stored(&self, insn: &Insn) -> (Val, &'static str) {
        match insn.mnemonic { "STA" => (self.a.clone(), "A"), "STX" => (self.x.clone(), "X"), _ => (self.y.clone(), "Y") }
    }
    pub fn clear_axy(&mut self) { self.a = Val::unknown(); self.x = Val::unknown(); self.y = Val::unknown(); }
}

fn rom_byte(rom: &Rom, m: &dyn Mapper, st: &BankState, addr: u16) -> Option<u8> {
    mapper::cpu_to_rom(m, rom, addr, st).map(|l| rom.prg[l.file_offset])
}

/// Value produced by a load instruction under the current registers.
fn load_value(insn: &Insn, r: &Regs, rom: &Rom, m: &dyn Mapper, st: &BankState) -> Val {
    let op = insn.operand.unwrap_or(0);
    match insn.mode {
        Mode::Imm => Val::imm(op as u8),
        Mode::Zp => r.zp.get(&(op as u8)).cloned().unwrap_or(Val { v: None, src: Src::Mem(op) }),
        Mode::Abs => {
            if op >= 0x8000 { Val { v: rom_byte(rom, m, st, op), src: Src::Mem(op) } } else { Val { v: None, src: Src::Mem(op) } }
        }
        Mode::Abx | Mode::Aby | Mode::Zpx | Mode::Zpy => {
            let idx = if matches!(insn.mode, Mode::Abx | Mode::Zpx) { &r.x } else { &r.y };
            let base = if matches!(insn.mode, Mode::Zpx | Mode::Zpy) { (op as u8) as u16 } else { op };
            let v = match idx.v { Some(i) if base >= 0x8000 => rom_byte(rom, m, st, base.wrapping_add(i as u16)), _ => None };
            Val { v, src: Src::Table(base) }
        }
        Mode::Izy | Mode::Izx => Val { v: None, src: Src::Ptr(op as u8) },
        _ => Val::unknown(),
    }
}

/// Advance register knowledge across one instruction.
pub fn update(insn: &Insn, r: &mut Regs, rom: &Rom, m: &dyn Mapper, st: &BankState) {
    let imm = insn.operand.map(|v| v as u8);
    match insn.mnemonic {
        "LDA" => r.a = load_value(insn, r, rom, m, st),
        "LDX" => r.x = load_value(insn, r, rom, m, st),
        "LDY" => r.y = load_value(insn, r, rom, m, st),
        "STA" | "STX" | "STY" => {
            if insn.mode == Mode::Zp {
                let (v, _) = r.stored(insn);
                r.zp.insert(insn.operand.unwrap_or(0) as u8, v);
            }
        }
        "TAX" => r.x = r.a.clone(),
        "TAY" => r.y = r.a.clone(),
        "TXA" => r.a = r.x.clone(),
        "TYA" => r.a = r.y.clone(),
        "TSX" => r.x = Val::unknown(),
        "PHA" => { r.stack.push(r.a.clone()); if r.stack.len() > 16 { r.stack.remove(0); } }
        "PLA" => r.a = r.stack.pop().map(|v| Val { v: v.v, src: if v.v.is_some() { v.src } else { Src::Stack } }).unwrap_or(Val { v: None, src: Src::Stack }),
        "PHP" => { r.stack.push(Val::unknown()); }
        "PLP" => { r.stack.pop(); }
        "ADC" | "SBC" => r.a = Val::unknown(),
        "INX" => r.x = r.x.map(|v| v.wrapping_add(1)),
        "DEX" => r.x = r.x.map(|v| v.wrapping_sub(1)),
        "INY" => r.y = r.y.map(|v| v.wrapping_add(1)),
        "DEY" => r.y = r.y.map(|v| v.wrapping_sub(1)),
        "AND" => r.a = if insn.mode == Mode::Imm { r.a.map(|a| a & imm.unwrap_or(0)) } else { Val::unknown() },
        "ORA" => r.a = if insn.mode == Mode::Imm { r.a.map(|a| a | imm.unwrap_or(0)) } else { Val::unknown() },
        "EOR" => r.a = if insn.mode == Mode::Imm { r.a.map(|a| a ^ imm.unwrap_or(0)) } else { Val::unknown() },
        "LSR" if insn.mode == Mode::Acc => r.a = r.a.map(|v| v >> 1),
        "ASL" if insn.mode == Mode::Acc => r.a = r.a.map(|v| v << 1),
        "ROL" | "ROR" if insn.mode == Mode::Acc => r.a = Val::unknown(),
        "INC" | "DEC" | "ASL" | "LSR" | "ROL" | "ROR" if insn.mode == Mode::Zp => { r.zp.remove(&(insn.operand.unwrap_or(0) as u8)); }
        "JSR" => r.clear_axy(),
        _ => {}
    }
}
