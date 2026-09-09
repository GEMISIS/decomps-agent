//! iNES / NES 2.0 header parsing and ROM loading.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct Header {
    pub mapper: u16,
    pub submapper: u8,
    pub nes2: bool,
    pub prg_size: usize,
    pub chr_size: usize,
    pub chr_ram: bool,
    pub chr_ram_size: usize,
    pub prg_ram_size: usize,
    pub prg_nvram_size: usize,
    pub mirroring: String,
    pub battery: bool,
    pub trainer: bool,
}

#[derive(Debug, Clone)]
pub struct Rom {
    pub path: String,
    pub header: Header,
    pub prg: Vec<u8>,
    pub chr: Vec<u8>,
    pub sha256: String,
    /// File offset at which PRG data begins (16, or 528 with a trainer).
    pub prg_file_base: usize,
}

fn nes2_size(lsb: u8, msb_nibble: u8, unit: usize) -> usize {
    if msb_nibble == 0xF {
        // Exponent-multiplier form: 2^E * (MM*2+1)
        let e = (lsb >> 2) as u32;
        let mm = (lsb & 3) as usize;
        (1usize << e) * (mm * 2 + 1)
    } else {
        (((msb_nibble as usize) << 8) | lsb as usize) * unit
    }
}

fn shift_size(v: u8) -> usize {
    if v == 0 {
        0
    } else {
        64usize << v
    }
}

pub fn parse_header(b: &[u8]) -> Result<Header> {
    if b.len() < 16 || &b[0..4] != b"NES\x1a" {
        bail!("not an iNES file (missing NES<EOF> magic)");
    }
    let nes2 = (b[7] & 0x0C) == 0x08;
    let mirroring = if b[6] & 0x08 != 0 {
        "four_screen"
    } else if b[6] & 0x01 != 0 {
        "vertical"
    } else {
        "horizontal"
    };
    let battery = b[6] & 0x02 != 0;
    let trainer = b[6] & 0x04 != 0;
    let mut mapper = ((b[6] >> 4) as u16) | ((b[7] & 0xF0) as u16);
    let mut submapper = 0u8;
    let (prg_size, chr_size, chr_ram_size, prg_ram_size, prg_nvram_size);
    if nes2 {
        mapper |= ((b[8] & 0x0F) as u16) << 8;
        submapper = b[8] >> 4;
        prg_size = nes2_size(b[4], b[9] & 0x0F, 16 * 1024);
        chr_size = nes2_size(b[5], b[9] >> 4, 8 * 1024);
        prg_ram_size = shift_size(b[10] & 0x0F);
        prg_nvram_size = shift_size(b[10] >> 4);
        chr_ram_size = shift_size(b[11] & 0x0F) + shift_size(b[11] >> 4);
    } else {
        prg_size = (b[4] as usize) * 16 * 1024;
        chr_size = (b[5] as usize) * 8 * 1024;
        prg_ram_size = if b[8] == 0 { 8192 } else { b[8] as usize * 8192 };
        prg_nvram_size = if battery { prg_ram_size } else { 0 };
        chr_ram_size = if chr_size == 0 { 8192 } else { 0 };
    }
    let chr_ram = chr_size == 0;
    Ok(Header {
        mapper,
        submapper,
        nes2,
        prg_size,
        chr_size,
        chr_ram,
        chr_ram_size: if chr_ram && chr_ram_size == 0 { 8192 } else { chr_ram_size },
        prg_ram_size,
        prg_nvram_size,
        mirroring: mirroring.to_string(),
        battery,
        trainer,
    })
}

impl Rom {
    pub fn parse(bytes: &[u8], path: &str) -> Result<Rom> {
        let header = parse_header(bytes)?;
        let prg_file_base = 16 + if header.trainer { 512 } else { 0 };
        let prg_end = prg_file_base + header.prg_size;
        if bytes.len() < prg_end {
            bail!(
                "file too short: header declares {} PRG bytes but only {} bytes follow the header",
                header.prg_size,
                bytes.len().saturating_sub(prg_file_base)
            );
        }
        let prg = bytes[prg_file_base..prg_end].to_vec();
        let chr_end = prg_end + header.chr_size;
        let chr = if bytes.len() >= chr_end {
            bytes[prg_end..chr_end].to_vec()
        } else {
            bail!("file too short for declared CHR size {}", header.chr_size);
        };
        let mut h = Sha256::new();
        h.update(bytes);
        let sha256 = format!("{:x}", h.finalize());
        Ok(Rom {
            path: path.to_string(),
            header,
            prg,
            chr,
            sha256,
            prg_file_base,
        })
    }

    pub fn load(path: &str) -> Result<Rom> {
        let bytes = std::fs::read(path).with_context(|| format!("reading ROM {path}"))?;
        Rom::parse(&bytes, path)
    }

    /// The `rom` stanza every output object carries.
    pub fn info(&self) -> serde_json::Value {
        serde_json::json!({ "sha256": self.sha256, "mapper": self.header.mapper, "path": self.path })
    }
}
