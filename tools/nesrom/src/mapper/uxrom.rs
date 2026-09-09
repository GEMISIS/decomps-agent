//! Mapper 2 (UxROM): $8000-$BFFF switchable 16 KB, $C000-$FFFF fixed to the last bank.
use super::*;

pub struct Uxrom { prg: usize, chr: usize }

impl Uxrom {
    pub fn new(prg: usize, chr: usize) -> Self { Uxrom { prg, chr } }
}

impl Mapper for Uxrom {
    fn number(&self) -> u16 { 2 }
    fn name(&self) -> &'static str { "UxROM" }
    fn prg_size(&self) -> usize { self.prg }
    fn chr_size(&self) -> usize { self.chr }
    fn prg_bank_size(&self) -> usize { 16 * 1024 }
    fn reset_state(&self) -> BankState {
        let last = self.last_bank();
        BankState::new([0, 1, last * 2, last * 2 + 1], [0, 1, 2, 3, 4, 5, 6, 7])
    }
    fn on_write(&self, addr: u16, value: u8, st: &mut BankState) -> Option<BankSwitch> {
        if addr < 0x8000 { return None; }
        let bank = (value as usize) % self.bank_count();
        let old = st.prg8k[0] / 2;
        st.prg8k[0] = bank * 2;
        st.prg8k[1] = bank * 2 + 1;
        st.regs[0] = value;
        Some(BankSwitch { register: addr, value, description: format!("UxROM: select PRG bank {bank} at $8000-$BFFF"), changed: old != bank })
    }
    fn windows(&self) -> Vec<Window> {
        vec![
            Window::new(0x8000, 0xBFFF, "switchable", 0, self.bank_count()),
            Window::new(0xC000, 0xFFFF, "fixed", self.last_bank(), 1),
        ]
    }
    fn is_mapper_reg(&self, addr: u16) -> bool { addr >= 0x8000 }
    fn write_note(&self, addr: u16) -> Option<String> {
        if addr >= 0x8000 { Some("UxROM: select PRG bank at $8000-$BFFF (value = bank number)".into()) } else { None }
    }
    fn bank_cpu_base(&self, bank: usize) -> u16 { if bank == self.last_bank() { 0xC000 } else { 0x8000 } }
}
