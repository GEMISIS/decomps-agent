//! Mapper 3 (CNROM): PRG fixed like NROM; writes to $8000-$FFFF select an 8 KB CHR bank.
use super::*;

pub struct Cnrom { prg: usize, chr: usize }

impl Cnrom {
    pub fn new(prg: usize, chr: usize) -> Self { Cnrom { prg, chr } }
}

impl Mapper for Cnrom {
    fn number(&self) -> u16 { 3 }
    fn name(&self) -> &'static str { "CNROM" }
    fn prg_size(&self) -> usize { self.prg }
    fn chr_size(&self) -> usize { self.chr }
    fn prg_bank_size(&self) -> usize { 16 * 1024 }
    fn reset_state(&self) -> BankState {
        BankState::new(super::nrom::fixed_prg8k(self.prg), [0, 1, 2, 3, 4, 5, 6, 7])
    }
    fn on_write(&self, addr: u16, value: u8, st: &mut BankState) -> Option<BankSwitch> {
        if addr < 0x8000 { return None; }
        let count = self.chr_bank_count_8k().max(1);
        let bank = (value as usize & 0x03) % count;
        let old = st.chr1k[0] / 8;
        for i in 0..8 { st.chr1k[i] = bank * 8 + i; }
        st.regs[0] = value;
        Some(BankSwitch { register: addr, value, description: format!("CNROM: select CHR 8K bank {bank}"), changed: old != bank })
    }
    fn windows(&self) -> Vec<Window> {
        let second = if self.prg <= 16 * 1024 { 0 } else { 1 };
        vec![Window::new(0x8000, 0xBFFF, "fixed", 0, 1), Window::new(0xC000, 0xFFFF, "fixed", second, 1)]
    }
    fn is_mapper_reg(&self, addr: u16) -> bool { addr >= 0x8000 }
    fn write_note(&self, addr: u16) -> Option<String> {
        if addr >= 0x8000 { Some("CNROM: select CHR bank (value bits 0-1)".into()) } else { None }
    }
    fn bank_cpu_base(&self, bank: usize) -> u16 { if bank == 0 { 0x8000 } else { 0xC000 } }
}
