//! Mapper abstraction: CPU address <-> ROM location, bank-register semantics.
//!
//! All mappers expose PRG as four 8 KB CPU slots ($8000/$A000/$C000/$E000) in
//! `BankState::prg8k` (values are 8 KB-unit indices into PRG). The "native"
//! bank size (16 KB for NROM/UxROM/CNROM/MMC1, 8 KB for MMC3) is what the CLI's
//! `--bank` argument and every `bank` field in the output refer to.

pub mod cnrom;
pub mod mmc1;
pub mod mmc3;
pub mod nrom;
pub mod uxrom;

use crate::ines::Rom;
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BankState {
    pub prg8k: [usize; 4],
    pub chr1k: [usize; 8],
    pub regs: [u8; 16],
    pub shift: u8,
    pub shift_n: u8,
    /// Set once a bank-switch with an unknown value happened on this path.
    pub dynamic: bool,
}

impl BankState {
    pub fn new(prg8k: [usize; 4], chr1k: [usize; 8]) -> Self {
        BankState { prg8k, chr1k, regs: [0; 16], shift: 0, shift_n: 0, dynamic: false }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RomLoc {
    pub cpu_addr: u16,
    pub bank: usize,
    pub offset: usize,
    pub file_offset: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Window {
    pub window: String,
    pub start: u16,
    pub end: u16,
    pub kind: &'static str,
    pub default_bank: usize,
    pub bank_count: usize,
}

impl Window {
    pub fn new(start: u16, end: u16, kind: &'static str, default_bank: usize, bank_count: usize) -> Self {
        Window { window: format!("${start:04X}-${end:04X}"), start, end, kind, default_bank, bank_count }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BankSwitch {
    pub register: u16,
    pub value: u8,
    pub description: String,
    pub changed: bool,
}

pub trait Mapper {
    fn number(&self) -> u16;
    fn name(&self) -> &'static str;
    fn prg_size(&self) -> usize;
    fn chr_size(&self) -> usize;
    fn prg_bank_size(&self) -> usize;
    fn reset_state(&self) -> BankState;
    /// Apply a CPU write. Returns Some when a mapper register was hit.
    fn on_write(&self, addr: u16, value: u8, st: &mut BankState) -> Option<BankSwitch>;
    fn windows(&self) -> Vec<Window>;
    fn is_mapper_reg(&self, addr: u16) -> bool;
    /// Disassembly annotation for a store to a mapper register.
    fn write_note(&self, addr: u16) -> Option<String>;
    /// The CPU window base a native bank most naturally appears at.
    fn bank_cpu_base(&self, bank: usize) -> u16;

    fn bank_count(&self) -> usize {
        (self.prg_size() / self.prg_bank_size()).max(1)
    }
    fn last_bank(&self) -> usize {
        self.bank_count() - 1
    }
    fn chr_bank_count_8k(&self) -> usize {
        self.chr_size() / 8192
    }
    fn native_per_8k(&self) -> usize {
        (self.prg_bank_size() / 8192).max(1)
    }
    /// State in which `addr`'s window shows native bank `bank`.
    fn state_for_bank(&self, bank: usize, addr: u16, st: &BankState) -> BankState {
        let mut s = st.clone();
        let size = self.prg_bank_size() as u32;
        if addr < 0x8000 {
            return s;
        }
        let win_start = 0x8000 + ((addr as u32 - 0x8000) / size) * size;
        let slot0 = ((win_start - 0x8000) / 0x2000) as usize;
        let n = (size / 0x2000).max(1) as usize;
        for i in 0..n {
            if slot0 + i < 4 {
                s.prg8k[slot0 + i] = bank * n + i;
            }
        }
        s
    }
}

pub fn for_rom(rom: &Rom) -> Result<Box<dyn Mapper>> {
    let prg = rom.header.prg_size;
    let chr = rom.header.chr_size;
    Ok(match rom.header.mapper {
        0 => Box::new(nrom::Nrom::new(prg, chr)),
        1 => Box::new(mmc1::Mmc1::new(prg, chr)),
        2 => Box::new(uxrom::Uxrom::new(prg, chr)),
        3 => Box::new(cnrom::Cnrom::new(prg, chr)),
        4 => Box::new(mmc3::Mmc3::new(prg, chr)),
        n => bail!("mapper {n} is not supported (supported: 0 NROM, 1 MMC1, 2 UxROM, 3 CNROM, 4 MMC3)"),
    })
}

pub fn is_supported(mapper: u16) -> bool {
    matches!(mapper, 0..=4)
}

/// Map a CPU address to a ROM location under the given bank state.
pub fn cpu_to_rom(m: &dyn Mapper, rom: &Rom, addr: u16, st: &BankState) -> Option<RomLoc> {
    if addr < 0x8000 {
        return None;
    }
    let slot = ((addr - 0x8000) / 0x2000) as usize;
    let file_offset = st.prg8k[slot] * 0x2000 + (addr as usize & 0x1FFF);
    if file_offset >= rom.prg.len() {
        return None;
    }
    let size = m.prg_bank_size();
    Some(RomLoc { cpu_addr: addr, bank: file_offset / size, offset: file_offset % size, file_offset })
}

/// Resolve `addr` either through an explicit native bank or the bank state.
pub fn resolve(m: &dyn Mapper, rom: &Rom, addr: u16, bank: Option<usize>, st: &BankState) -> Result<RomLoc> {
    if addr < 0x8000 {
        bail!("address ${addr:04X} is not in PRG ROM space ($8000-$FFFF)");
    }
    match bank {
        Some(b) => {
            if b >= m.bank_count() {
                bail!("bank {b} out of range (ROM has {} banks of {} bytes)", m.bank_count(), m.prg_bank_size());
            }
            let size = m.prg_bank_size();
            let file_offset = b * size + (addr as usize % size);
            Ok(RomLoc { cpu_addr: addr, bank: b, offset: addr as usize % size, file_offset })
        }
        None => cpu_to_rom(m, rom, addr, st)
            .ok_or_else(|| anyhow::anyhow!("address ${addr:04X} does not map to PRG ROM in the current bank state")),
    }
}

/// CPU address for a PRG file offset under the given state, if that 8 KB unit is mapped.
pub fn file_offset_to_cpu(st: &BankState, file_offset: usize) -> Option<u16> {
    let unit = file_offset / 0x2000;
    for (slot, &u) in st.prg8k.iter().enumerate() {
        if u == unit {
            return Some(0x8000 + (slot as u16) * 0x2000 + (file_offset & 0x1FFF) as u16);
        }
    }
    None
}

/// CPU address a file offset most naturally lives at (through the native bank's default window).
pub fn file_offset_to_default_cpu(m: &dyn Mapper, file_offset: usize) -> u16 {
    let size = m.prg_bank_size();
    let bank = file_offset / size;
    m.bank_cpu_base(bank).wrapping_add((file_offset % size) as u16)
}

pub fn read_u16(prg: &[u8], file_offset: usize) -> Option<u16> {
    if file_offset + 1 < prg.len() {
        Some(prg[file_offset] as u16 | ((prg[file_offset + 1] as u16) << 8))
    } else {
        None
    }
}

/// Parse `$C000`, `0xC000`, or `C000` (also plain decimal when prefixed with `#`).
pub fn parse_addr(s: &str) -> Result<u16> {
    let t = s.trim();
    let hex = if let Some(r) = t.strip_prefix('$') {
        r
    } else if let Some(r) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        r
    } else if let Some(r) = t.strip_prefix('#') {
        return r.parse::<u16>().map_err(|e| anyhow::anyhow!("bad decimal address {s}: {e}"));
    } else {
        t
    };
    u16::from_str_radix(hex, 16).map_err(|e| anyhow::anyhow!("bad hex address {s}: {e}"))
}
