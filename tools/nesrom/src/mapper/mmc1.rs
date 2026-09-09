//! Mapper 1 (MMC1 / SxROM): 5-bit serial register, four internal registers.
//! regs[0] = control, regs[1] = CHR bank 0, regs[2] = CHR bank 1, regs[3] = PRG bank.
use super::*;

pub struct Mmc1 { prg: usize, chr: usize }

impl Mmc1 {
    pub fn new(prg: usize, chr: usize) -> Self { Mmc1 { prg, chr } }

    fn apply(&self, st: &mut BankState) {
        let control = st.regs[0];
        let prg_reg = (st.regs[3] & 0x0F) as usize;
        let count = self.bank_count();
        let last = self.last_bank();
        match (control >> 2) & 3 {
            0 | 1 => {
                let b = (prg_reg & !1) % count;
                st.prg8k = [b * 2, b * 2 + 1, (b + 1) % count * 2, (b + 1) % count * 2 + 1];
            }
            2 => {
                let b = prg_reg % count;
                st.prg8k = [0, 1, b * 2, b * 2 + 1];
            }
            _ => {
                let b = prg_reg % count;
                st.prg8k = [b * 2, b * 2 + 1, last * 2, last * 2 + 1];
            }
        }
        let chr_4k_count = (self.chr / 4096).max(2);
        if control & 0x10 != 0 {
            let c0 = (st.regs[1] as usize) % chr_4k_count;
            let c1 = (st.regs[2] as usize) % chr_4k_count;
            for i in 0..4 { st.chr1k[i] = c0 * 4 + i; st.chr1k[4 + i] = c1 * 4 + i; }
        } else {
            let c = ((st.regs[1] as usize) & !1) % chr_4k_count;
            for i in 0..8 { st.chr1k[i] = c * 4 + i; }
        }
    }
}

impl Mapper for Mmc1 {
    fn number(&self) -> u16 { 1 }
    fn name(&self) -> &'static str { "MMC1" }
    fn prg_size(&self) -> usize { self.prg }
    fn chr_size(&self) -> usize { self.chr }
    fn prg_bank_size(&self) -> usize { 16 * 1024 }
    fn reset_state(&self) -> BankState {
        let mut st = BankState::new([0, 1, 0, 1], [0, 1, 2, 3, 4, 5, 6, 7]);
        st.regs[0] = 0x0C;
        self.apply(&mut st);
        st
    }
    fn on_write(&self, addr: u16, value: u8, st: &mut BankState) -> Option<BankSwitch> {
        if addr < 0x8000 { return None; }
        let before = st.prg8k;
        if value & 0x80 != 0 {
            st.shift = 0;
            st.shift_n = 0;
            st.regs[0] |= 0x0C;
            self.apply(st);
            return Some(BankSwitch { register: addr, value, description: "MMC1: reset shift register; PRG mode -> fix last bank at $C000".into(), changed: before != st.prg8k });
        }
        st.shift = (st.shift >> 1) | ((value & 1) << 4);
        st.shift_n += 1;
        if st.shift_n < 5 {
            return Some(BankSwitch { register: addr, value, description: format!("MMC1: serial write {}/5", st.shift_n), changed: false });
        }
        let reg = ((addr >> 13) & 3) as usize;
        let v = st.shift;
        st.regs[reg] = v;
        st.shift = 0;
        st.shift_n = 0;
        self.apply(st);
        let what = match reg {
            0 => format!("control = ${v:02X} (mirroring {}, PRG mode {}, CHR mode {})", v & 3, (v >> 2) & 3, (v >> 4) & 1),
            1 => format!("CHR bank 0 = {v}"),
            2 => format!("CHR bank 1 = {v}"),
            _ => format!("PRG bank = {} (now $8000={} $C000={})", v & 0x0F, st.prg8k[0] / 2, st.prg8k[2] / 2),
        };
        Some(BankSwitch { register: addr, value, description: format!("MMC1: commit {what}"), changed: before != st.prg8k })
    }
    fn windows(&self) -> Vec<Window> {
        vec![
            Window::new(0x8000, 0xBFFF, "switchable", 0, self.bank_count()),
            Window::new(0xC000, 0xFFFF, "fixed", self.last_bank(), self.bank_count()),
        ]
    }
    fn is_mapper_reg(&self, addr: u16) -> bool { addr >= 0x8000 }
    fn write_note(&self, addr: u16) -> Option<String> {
        if addr < 0x8000 { return None; }
        let reg = match (addr >> 13) & 3 { 0 => "control", 1 => "CHR bank 0", 2 => "CHR bank 1", _ => "PRG bank" };
        Some(format!("MMC1: serial write (bit 0 of value) to {reg} register; 5 writes commit, bit 7 resets"))
    }
    fn bank_cpu_base(&self, bank: usize) -> u16 { if bank == self.last_bank() { 0xC000 } else { 0x8000 } }
}
