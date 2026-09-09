//! Mapper 4 (MMC3 / TxROM): $8000 bank select, $8001 bank data, 8 KB PRG banks.
//! regs[0] = bank select; regs[8..16] = R0..R7.
use super::*;

pub struct Mmc3 { prg: usize, chr: usize }

impl Mmc3 {
    pub fn new(prg: usize, chr: usize) -> Self { Mmc3 { prg, chr } }

    fn apply(&self, st: &mut BankState) {
        let count = self.bank_count();
        let last = self.last_bank();
        let r6 = (st.regs[8 + 6] as usize) % count;
        let r7 = (st.regs[8 + 7] as usize) % count;
        if st.regs[0] & 0x40 == 0 {
            st.prg8k = [r6, r7, last - 1, last];
        } else {
            st.prg8k = [last - 1, r7, r6, last];
        }
        let chr1k_count = (self.chr / 1024).max(8);
        let r = |i: usize| (st.regs[8 + i] as usize) % chr1k_count;
        let two = [r(0) & !1, (r(0) & !1) + 1, r(1) & !1, (r(1) & !1) + 1];
        let one = [r(2), r(3), r(4), r(5)];
        if st.regs[0] & 0x80 == 0 {
            st.chr1k = [two[0], two[1], two[2], two[3], one[0], one[1], one[2], one[3]];
        } else {
            st.chr1k = [one[0], one[1], one[2], one[3], two[0], two[1], two[2], two[3]];
        }
    }
}

impl Mapper for Mmc3 {
    fn number(&self) -> u16 { 4 }
    fn name(&self) -> &'static str { "MMC3" }
    fn prg_size(&self) -> usize { self.prg }
    fn chr_size(&self) -> usize { self.chr }
    fn prg_bank_size(&self) -> usize { 8 * 1024 }
    fn reset_state(&self) -> BankState {
        let mut st = BankState::new([0, 1, 0, 1], [0, 1, 2, 3, 4, 5, 6, 7]);
        st.regs[8 + 6] = 0;
        st.regs[8 + 7] = 1;
        self.apply(&mut st);
        st
    }
    fn on_write(&self, addr: u16, value: u8, st: &mut BankState) -> Option<BankSwitch> {
        if addr < 0x8000 { return None; }
        let before = st.prg8k;
        let desc = match (addr & 0xE001, addr & 1) {
            (0x8000, _) => {
                st.regs[0] = value;
                self.apply(st);
                format!("MMC3: bank select R{} (PRG mode {}, CHR mode {})", value & 7, (value >> 6) & 1, value >> 7)
            }
            (0x8001, _) => {
                let sel = (st.regs[0] & 7) as usize;
                st.regs[8 + sel] = value;
                self.apply(st);
                let what = match sel { 6 => "PRG bank at $8000 (or $C000 in mode 1)", 7 => "PRG bank at $A000", 0 | 1 => "CHR 2K bank", _ => "CHR 1K bank" };
                format!("MMC3: bank data R{sel} = {value} ({what})")
            }
            (0xA000, _) => "MMC3: mirroring".to_string(),
            (0xA001, _) => "MMC3: PRG-RAM protect".to_string(),
            (0xC000, _) => "MMC3: IRQ latch".to_string(),
            (0xC001, _) => "MMC3: IRQ reload".to_string(),
            (0xE000, _) => "MMC3: IRQ disable".to_string(),
            _ => "MMC3: IRQ enable".to_string(),
        };
        Some(BankSwitch { register: addr, value, description: desc, changed: before != st.prg8k })
    }
    fn windows(&self) -> Vec<Window> {
        let n = self.bank_count();
        vec![
            Window::new(0x8000, 0x9FFF, "switchable", 0, n),
            Window::new(0xA000, 0xBFFF, "switchable", 1, n),
            Window::new(0xC000, 0xDFFF, "fixed", n.saturating_sub(2), n),
            Window::new(0xE000, 0xFFFF, "fixed", n - 1, 1),
        ]
    }
    fn is_mapper_reg(&self, addr: u16) -> bool { addr >= 0x8000 }
    fn write_note(&self, addr: u16) -> Option<String> {
        if addr < 0x8000 { return None; }
        Some(match addr & 0xE001 {
            0x8000 => "MMC3: bank select (bits 0-2 = register, bit 6 = PRG mode, bit 7 = CHR mode)",
            0x8001 => "MMC3: bank data for the selected register",
            0xA000 => "MMC3: nametable mirroring",
            0xA001 => "MMC3: PRG-RAM protect",
            0xC000 => "MMC3: scanline IRQ latch",
            0xC001 => "MMC3: scanline IRQ reload",
            0xE000 => "MMC3: IRQ disable/acknowledge",
            _ => "MMC3: IRQ enable",
        }.to_string())
    }
    fn bank_cpu_base(&self, bank: usize) -> u16 {
        let last = self.last_bank();
        if bank == last { 0xE000 } else if bank + 1 == last { 0xC000 } else if bank % 2 == 0 { 0x8000 } else { 0xA000 }
    }
}
