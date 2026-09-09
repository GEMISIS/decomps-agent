//! 6502 opcode table and instruction decoder.

use serde::Serialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode { Imp, Acc, Imm, Zp, Zpx, Zpy, Abs, Abx, Aby, Ind, Izx, Izy, Rel }

impl Mode {
    pub fn len(self) -> u8 {
        match self {
            Mode::Imp | Mode::Acc => 1,
            Mode::Imm | Mode::Zp | Mode::Zpx | Mode::Zpy | Mode::Izx | Mode::Izy | Mode::Rel => 2,
            Mode::Abs | Mode::Abx | Mode::Aby | Mode::Ind => 3,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Imp => "implied", Mode::Acc => "accumulator", Mode::Imm => "immediate", Mode::Zp => "zeropage",
            Mode::Zpx => "zeropage,x", Mode::Zpy => "zeropage,y", Mode::Abs => "absolute", Mode::Abx => "absolute,x",
            Mode::Aby => "absolute,y", Mode::Ind => "indirect", Mode::Izx => "(indirect,x)", Mode::Izy => "(indirect),y",
            Mode::Rel => "relative",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Op { pub mnemonic: &'static str, pub mode: Mode, pub len: u8, pub illegal: bool }

const ILLEGAL: Op = Op { mnemonic: ".byte", mode: Mode::Imp, len: 1, illegal: true };

use Mode::*;
const LEGAL: &[(u8, &str, Mode)] = &[
    (0x69,"ADC",Imm),(0x65,"ADC",Zp),(0x75,"ADC",Zpx),(0x6D,"ADC",Abs),(0x7D,"ADC",Abx),(0x79,"ADC",Aby),(0x61,"ADC",Izx),(0x71,"ADC",Izy),
    (0x29,"AND",Imm),(0x25,"AND",Zp),(0x35,"AND",Zpx),(0x2D,"AND",Abs),(0x3D,"AND",Abx),(0x39,"AND",Aby),(0x21,"AND",Izx),(0x31,"AND",Izy),
    (0x0A,"ASL",Acc),(0x06,"ASL",Zp),(0x16,"ASL",Zpx),(0x0E,"ASL",Abs),(0x1E,"ASL",Abx),
    (0x90,"BCC",Rel),(0xB0,"BCS",Rel),(0xF0,"BEQ",Rel),(0x30,"BMI",Rel),(0xD0,"BNE",Rel),(0x10,"BPL",Rel),(0x50,"BVC",Rel),(0x70,"BVS",Rel),
    (0x24,"BIT",Zp),(0x2C,"BIT",Abs),
    (0x00,"BRK",Imp),(0x18,"CLC",Imp),(0xD8,"CLD",Imp),(0x58,"CLI",Imp),(0xB8,"CLV",Imp),(0x38,"SEC",Imp),(0xF8,"SED",Imp),(0x78,"SEI",Imp),(0xEA,"NOP",Imp),
    (0xC9,"CMP",Imm),(0xC5,"CMP",Zp),(0xD5,"CMP",Zpx),(0xCD,"CMP",Abs),(0xDD,"CMP",Abx),(0xD9,"CMP",Aby),(0xC1,"CMP",Izx),(0xD1,"CMP",Izy),
    (0xE0,"CPX",Imm),(0xE4,"CPX",Zp),(0xEC,"CPX",Abs),
    (0xC0,"CPY",Imm),(0xC4,"CPY",Zp),(0xCC,"CPY",Abs),
    (0xC6,"DEC",Zp),(0xD6,"DEC",Zpx),(0xCE,"DEC",Abs),(0xDE,"DEC",Abx),
    (0xCA,"DEX",Imp),(0x88,"DEY",Imp),(0xE8,"INX",Imp),(0xC8,"INY",Imp),
    (0x49,"EOR",Imm),(0x45,"EOR",Zp),(0x55,"EOR",Zpx),(0x4D,"EOR",Abs),(0x5D,"EOR",Abx),(0x59,"EOR",Aby),(0x41,"EOR",Izx),(0x51,"EOR",Izy),
    (0xE6,"INC",Zp),(0xF6,"INC",Zpx),(0xEE,"INC",Abs),(0xFE,"INC",Abx),
    (0x4C,"JMP",Abs),(0x6C,"JMP",Ind),(0x20,"JSR",Abs),
    (0xA9,"LDA",Imm),(0xA5,"LDA",Zp),(0xB5,"LDA",Zpx),(0xAD,"LDA",Abs),(0xBD,"LDA",Abx),(0xB9,"LDA",Aby),(0xA1,"LDA",Izx),(0xB1,"LDA",Izy),
    (0xA2,"LDX",Imm),(0xA6,"LDX",Zp),(0xB6,"LDX",Zpy),(0xAE,"LDX",Abs),(0xBE,"LDX",Aby),
    (0xA0,"LDY",Imm),(0xA4,"LDY",Zp),(0xB4,"LDY",Zpx),(0xAC,"LDY",Abs),(0xBC,"LDY",Abx),
    (0x4A,"LSR",Acc),(0x46,"LSR",Zp),(0x56,"LSR",Zpx),(0x4E,"LSR",Abs),(0x5E,"LSR",Abx),
    (0x09,"ORA",Imm),(0x05,"ORA",Zp),(0x15,"ORA",Zpx),(0x0D,"ORA",Abs),(0x1D,"ORA",Abx),(0x19,"ORA",Aby),(0x01,"ORA",Izx),(0x11,"ORA",Izy),
    (0x48,"PHA",Imp),(0x08,"PHP",Imp),(0x68,"PLA",Imp),(0x28,"PLP",Imp),
    (0x2A,"ROL",Acc),(0x26,"ROL",Zp),(0x36,"ROL",Zpx),(0x2E,"ROL",Abs),(0x3E,"ROL",Abx),
    (0x6A,"ROR",Acc),(0x66,"ROR",Zp),(0x76,"ROR",Zpx),(0x6E,"ROR",Abs),(0x7E,"ROR",Abx),
    (0x40,"RTI",Imp),(0x60,"RTS",Imp),
    (0xE9,"SBC",Imm),(0xE5,"SBC",Zp),(0xF5,"SBC",Zpx),(0xED,"SBC",Abs),(0xFD,"SBC",Abx),(0xF9,"SBC",Aby),(0xE1,"SBC",Izx),(0xF1,"SBC",Izy),
    (0x85,"STA",Zp),(0x95,"STA",Zpx),(0x8D,"STA",Abs),(0x9D,"STA",Abx),(0x99,"STA",Aby),(0x81,"STA",Izx),(0x91,"STA",Izy),
    (0x86,"STX",Zp),(0x96,"STX",Zpy),(0x8E,"STX",Abs),
    (0x84,"STY",Zp),(0x94,"STY",Zpx),(0x8C,"STY",Abs),
    (0xAA,"TAX",Imp),(0xA8,"TAY",Imp),(0xBA,"TSX",Imp),(0x8A,"TXA",Imp),(0x9A,"TXS",Imp),(0x98,"TYA",Imp),
];

pub fn table() -> &'static [Op; 256] {
    static T: OnceLock<[Op; 256]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [ILLEGAL; 256];
        for &(b, m, mode) in LEGAL {
            t[b as usize] = Op { mnemonic: m, mode, len: mode.len(), illegal: false };
        }
        t
    })
}

pub fn op(byte: u8) -> &'static Op { &table()[byte as usize] }

pub const MNEMONICS: &[&str] = &[
    "ADC","AND","ASL","BCC","BCS","BEQ","BIT","BMI","BNE","BPL","BRK","BVC","BVS","CLC","CLD","CLI","CLV","CMP","CPX","CPY",
    "DEC","DEX","DEY","EOR","INC","INX","INY","JMP","JSR","LDA","LDX","LDY","LSR","NOP","ORA","PHA","PHP","PLA","PLP","ROL",
    "ROR","RTI","RTS","SBC","SEC","SED","SEI","STA","STX","STY","TAX","TAY","TSX","TXA","TXS","TYA",
];

#[derive(Debug, Clone, Serialize)]
pub struct Insn {
    pub opcode: u8,
    pub mnemonic: &'static str,
    pub mode: Mode,
    pub len: u8,
    pub operand: Option<u16>,
    pub illegal: bool,
    pub bytes: Vec<u8>,
}

/// Decode one instruction from `bytes` (must be non-empty). Truncated
/// instructions decode as a single `.byte`.
pub fn decode(bytes: &[u8]) -> Insn {
    let o = op(bytes[0]);
    if bytes.len() < o.len as usize {
        return Insn { opcode: bytes[0], mnemonic: ".byte", mode: Mode::Imp, len: 1, operand: None, illegal: true, bytes: vec![bytes[0]] };
    }
    let operand = match o.len {
        2 => Some(bytes[1] as u16),
        3 => Some(bytes[1] as u16 | ((bytes[2] as u16) << 8)),
        _ => None,
    };
    Insn { opcode: bytes[0], mnemonic: o.mnemonic, mode: o.mode, len: o.len, operand, illegal: o.illegal, bytes: bytes[..o.len as usize].to_vec() }
}

impl Insn {
    pub fn is_branch(&self) -> bool { self.mode == Mode::Rel }
    pub fn is_jsr(&self) -> bool { self.opcode == 0x20 }
    pub fn is_jmp_abs(&self) -> bool { self.opcode == 0x4C }
    pub fn is_jmp_ind(&self) -> bool { self.opcode == 0x6C }
    pub fn is_return(&self) -> bool { matches!(self.opcode, 0x60 | 0x40) }
    pub fn is_brk(&self) -> bool { self.opcode == 0x00 }
    pub fn is_store(&self) -> bool { matches!(self.mnemonic, "STA" | "STX" | "STY") }
    pub fn is_load(&self) -> bool { matches!(self.mnemonic, "LDA" | "LDX" | "LDY") }
    pub fn is_rmw(&self) -> bool { matches!(self.mnemonic, "INC" | "DEC" | "ASL" | "LSR" | "ROL" | "ROR") && self.mode != Mode::Acc }
    pub fn ends_flow(&self) -> bool { self.is_return() || self.is_jmp_abs() || self.is_jmp_ind() || self.is_brk() || self.illegal }

    /// Control-flow target for branches, JMP abs and JSR.
    pub fn target(&self, pc: u16) -> Option<u16> {
        match self.mode {
            Mode::Rel => {
                let off = self.operand? as u8 as i8 as i32;
                Some((pc as i32 + self.len as i32 + off) as u16)
            }
            Mode::Abs if self.is_jsr() || self.is_jmp_abs() => self.operand,
            _ => None,
        }
    }

    /// Effective memory operand for absolute/zero-page style access (the base address).
    pub fn mem_operand(&self) -> Option<u16> {
        match self.mode {
            Mode::Zp | Mode::Zpx | Mode::Zpy | Mode::Abs | Mode::Abx | Mode::Aby => {
                if self.is_jsr() || self.is_jmp_abs() { None } else { self.operand }
            }
            _ => None,
        }
    }

    pub fn indexed(&self) -> bool { matches!(self.mode, Mode::Zpx | Mode::Zpy | Mode::Abx | Mode::Aby | Mode::Izx | Mode::Izy) }

    pub fn format_operand(&self, pc: u16) -> String {
        let v = self.operand.unwrap_or(0);
        match self.mode {
            Mode::Imp => String::new(),
            Mode::Acc => "A".into(),
            Mode::Imm => format!("#${v:02X}"),
            Mode::Zp => format!("${v:02X}"),
            Mode::Zpx => format!("${v:02X},X"),
            Mode::Zpy => format!("${v:02X},Y"),
            Mode::Abs => format!("${v:04X}"),
            Mode::Abx => format!("${v:04X},X"),
            Mode::Aby => format!("${v:04X},Y"),
            Mode::Ind => format!("(${v:04X})"),
            Mode::Izx => format!("(${v:02X},X)"),
            Mode::Izy => format!("(${v:02X}),Y"),
            Mode::Rel => format!("${:04X}", self.target(pc).unwrap_or(0)),
        }
    }

    pub fn hex_bytes(&self) -> String {
        self.bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
    }
}

/// Hardware register name and behavioral role for $2000-$2007 and $4000-$4017.
pub fn hw_register(addr: u16) -> Option<(&'static str, &'static str)> {
    Some(match addr {
        0x2000 => ("PPUCTRL", "PPU control: NMI enable, sprite size, pattern table select, VRAM increment, nametable base"),
        0x2001 => ("PPUMASK", "PPU mask: rendering enable for background/sprites, color emphasis, left-column clipping"),
        0x2002 => ("PPUSTATUS", "PPU status: vblank flag, sprite 0 hit, sprite overflow; reading resets the address latch"),
        0x2003 => ("OAMADDR", "OAM address for sprite memory access"),
        0x2004 => ("OAMDATA", "OAM data port (sprite attribute memory)"),
        0x2005 => ("PPUSCROLL", "Scroll position (two writes: X then Y)"),
        0x2006 => ("PPUADDR", "VRAM address (two writes: high then low)"),
        0x2007 => ("PPUDATA", "VRAM data port: read/write nametables, pattern tables (CHR-RAM), palettes"),
        0x4000 => ("SQ1_VOL", "Pulse 1: duty, envelope/volume"),
        0x4001 => ("SQ1_SWEEP", "Pulse 1: sweep"),
        0x4002 => ("SQ1_LO", "Pulse 1: period low"),
        0x4003 => ("SQ1_HI", "Pulse 1: period high, length counter load"),
        0x4004 => ("SQ2_VOL", "Pulse 2: duty, envelope/volume"),
        0x4005 => ("SQ2_SWEEP", "Pulse 2: sweep"),
        0x4006 => ("SQ2_LO", "Pulse 2: period low"),
        0x4007 => ("SQ2_HI", "Pulse 2: period high, length counter load"),
        0x4008 => ("TRI_LINEAR", "Triangle: linear counter"),
        0x4009 => ("TRI_UNUSED", "Triangle: unused"),
        0x400A => ("TRI_LO", "Triangle: period low"),
        0x400B => ("TRI_HI", "Triangle: period high, length counter load"),
        0x400C => ("NOISE_VOL", "Noise: envelope/volume"),
        0x400D => ("NOISE_UNUSED", "Noise: unused"),
        0x400E => ("NOISE_LO", "Noise: period, mode"),
        0x400F => ("NOISE_HI", "Noise: length counter load"),
        0x4010 => ("DMC_FREQ", "DMC: IRQ enable, loop, rate"),
        0x4011 => ("DMC_RAW", "DMC: direct 7-bit output level"),
        0x4012 => ("DMC_START", "DMC: sample address"),
        0x4013 => ("DMC_LEN", "DMC: sample length"),
        0x4014 => ("OAMDMA", "OAM DMA: copy 256 bytes from CPU page to sprite memory"),
        0x4015 => ("SND_CHN", "APU status: channel enables / length counter status, DMC IRQ"),
        0x4016 => ("JOY1", "Controller 1: strobe (write) / serial read (read)"),
        0x4017 => ("JOY2", "Controller 2 serial read (read) / APU frame counter (write)"),
        _ => return None,
    })
}

pub fn apu_channel(addr: u16) -> Option<&'static str> {
    Some(match addr {
        0x4000..=0x4003 => "pulse1",
        0x4004..=0x4007 => "pulse2",
        0x4008..=0x400B => "triangle",
        0x400C..=0x400F => "noise",
        0x4010..=0x4013 => "dmc",
        0x4015 => "status",
        0x4017 => "frame_counter",
        _ => return None,
    })
}
