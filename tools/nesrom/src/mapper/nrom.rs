//! Mapper 0 (NROM): 16 or 32 KB PRG, fixed; 8 KB CHR, fixed.
use super::*;

pub struct Nrom { prg: usize, chr: usize }

impl Nrom {
    pub fn new(prg: usize, chr: usize) -> Self { Nrom { prg, chr } }
}

pub(crate) fn fixed_prg8k(prg: usize) -> [usize; 4] {
    if prg <= 16 * 1024 { [0, 1, 0, 1] } else { [0, 1, 2, 3] }
}

impl Mapper for Nrom {
    fn number(&self) -> u16 { 0 }
    fn name(&self) -> &'static str { "NROM" }
    fn prg_size(&self) -> usize { self.prg }
    fn chr_size(&self) -> usize { self.chr }
    fn prg_bank_size(&self) -> usize { 16 * 1024 }
    fn reset_state(&self) -> BankState { BankState::new(fixed_prg8k(self.prg), [0, 1, 2, 3, 4, 5, 6, 7]) }
    fn on_write(&self, _addr: u16, _value: u8, _st: &mut BankState) -> Option<BankSwitch> { None }
    fn windows(&self) -> Vec<Window> {
        let second = if self.prg <= 16 * 1024 { 0 } else { 1 };
        vec![
            Window::new(0x8000, 0xBFFF, "fixed", 0, 1),
            Window::new(0xC000, 0xFFFF, "fixed", second, 1),
        ]
    }
    fn is_mapper_reg(&self, _addr: u16) -> bool { false }
    fn write_note(&self, _addr: u16) -> Option<String> { None }
    fn bank_cpu_base(&self, bank: usize) -> u16 { if bank == 0 { 0x8000 } else { 0xC000 } }
}
