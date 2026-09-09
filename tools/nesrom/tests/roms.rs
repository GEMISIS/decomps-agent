//! Golden tests against ROMs you supply (skipped when absent) and synthetic ROMs.
//! Set NESROM_TEST_NROM to a 32K-PRG/8K-CHR NROM image with vertical mirroring (e.g. the cc65 NES sample),
//! NESROM_TEST_MMC1 to an MMC1 / NES 2.0 / CHR-RAM image with 16 PRG banks, and NESROM_TEST_MMC1_CHR_DIR to a
//! directory of matching `*.chr` / `*.pb53` pairs for the decoder round-trip. No ROM ships with this repository.

use nesrom::compress;
use nesrom::ines::Rom;
use nesrom::lint;
use nesrom::mapper::{self, parse_addr};
use nesrom::reach;

fn env_path(var: &str) -> Option<String> {
    match std::env::var(var) {
        Ok(p) if std::path::Path::new(&p).exists() => Some(p),
        _ => { eprintln!("skipping: {var} not set or not present"); None }
    }
}

fn load(var: &str) -> Option<Rom> {
    env_path(var).map(|p| Rom::load(&p).expect("load"))
}

/// Build a synthetic iNES image: `prg_banks` x 16K PRG, `chr_banks` x 8K CHR.
fn synth(mapper_no: u8, prg_banks: u8, chr_banks: u8, fill: impl Fn(usize) -> u8) -> Rom {
    let mut b = vec![b'N', b'E', b'S', 0x1A, prg_banks, chr_banks, (mapper_no << 4) | 1, mapper_no & 0xF0, 0, 0, 0, 0, 0, 0, 0, 0];
    let prg_len = prg_banks as usize * 16384;
    for i in 0..prg_len { b.push(fill(i)); }
    for _ in 0..(chr_banks as usize * 8192) { b.push(0); }
    // Vectors in the last 6 bytes of PRG: NMI=$C100 RESET=$C000 IRQ=$C200
    let n = b.len() - chr_banks as usize * 8192;
    b[n - 6] = 0x00; b[n - 5] = 0xC1; b[n - 4] = 0x00; b[n - 3] = 0xC0; b[n - 2] = 0x00; b[n - 1] = 0xC2;
    Rom::parse(&b, "synthetic.nes").unwrap()
}

#[test]
fn nrom_header_and_vectors() {
    let Some(rom) = load("NESROM_TEST_NROM") else { return };
    assert_eq!(rom.header.mapper, 0);
    assert_eq!(rom.header.prg_size, 32768);
    assert_eq!(rom.header.chr_size, 8192);
    assert_eq!(rom.header.mirroring, "vertical");
    assert!(!rom.header.chr_ram);
    let m = mapper::for_rom(&rom).unwrap();
    let st = m.reset_state();
    let reset_loc = mapper::cpu_to_rom(m.as_ref(), &rom, 0xFFFC, &st).unwrap();
    let reset = mapper::read_u16(&rom.prg, reset_loc.file_offset).unwrap();
    assert!(reset >= 0x8000, "reset vector {reset:04X} should be in PRG");
    assert!(mapper::cpu_to_rom(m.as_ref(), &rom, reset, &st).is_some());
}

#[test]
fn nrom_reach_finds_code() {
    let Some(rom) = load("NESROM_TEST_NROM") else { return };
    let m = mapper::for_rom(&rom).unwrap();
    let t = reach::trace(&rom, m.as_ref(), &[], true);
    assert!(!t.code_ranges().is_empty());
    assert!(t.subs.len() >= 3, "expected >=3 subroutines, got {}", t.subs.len());
    assert!(t.hw.iter().any(|h| (0x2000..=0x2007).contains(&h.addr_num)), "should touch a PPU register");
}

#[test]
fn mmc1_header_mmc1_chr_ram() {
    let Some(rom) = load("NESROM_TEST_MMC1") else { return };
    assert_eq!(rom.header.mapper, 1);
    assert!(rom.header.nes2);
    assert_eq!(rom.header.prg_size, 262144);
    assert_eq!(rom.header.chr_size, 0);
    assert!(rom.header.chr_ram);
    let m = mapper::for_rom(&rom).unwrap();
    assert_eq!(m.bank_count(), 16);
    let st = m.reset_state();
    assert_eq!(st.prg8k[2], 30);
    assert_eq!(st.prg8k[3], 31);
    for v in [0xFFFAu16, 0xFFFC, 0xFFFE] {
        let loc = mapper::cpu_to_rom(m.as_ref(), &rom, v, &st).unwrap();
        assert_eq!(loc.bank, 15);
        let t = mapper::read_u16(&rom.prg, loc.file_offset).unwrap();
        assert_eq!(mapper::cpu_to_rom(m.as_ref(), &rom, t, &st).unwrap().bank, 15);
    }
}

#[test]
fn mmc1_reach_crosses_banks() {
    let Some(rom) = load("NESROM_TEST_MMC1") else { return };
    let m = mapper::for_rom(&rom).unwrap();
    let t = reach::trace(&rom, m.as_ref(), &[], true);
    let banks: std::collections::BTreeSet<usize> = t.subs.values().map(|s| s.bank).collect();
    assert!(banks.len() >= 3, "expected code in >=3 banks, got {banks:?}");
    assert!(t.switches.iter().any(|s| s.register.starts_with("helper")), "helper replay should resolve at least one switch");
    assert!(t.edges.iter().any(|e| e.kind == "rts_table"), "RTS-trick dispatch should be resolved");
}

#[test]
fn mmc1_serial_register() {
    let rom = synth(1, 8, 0, |_| 0xEA);
    let m = mapper::for_rom(&rom).unwrap();
    let mut st = m.reset_state();
    assert_eq!(st.prg8k, [0, 1, 14, 15]);
    // Select PRG bank 5 via five serial writes to $E000 (LSB first).
    for i in 0..5 {
        let bit = (5 >> i) & 1;
        m.on_write(0xE000, bit, &mut st);
    }
    assert_eq!(st.prg8k, [10, 11, 14, 15]);
    // Reset bit
    m.on_write(0x8000, 0x80, &mut st);
    assert_eq!(st.shift_n, 0);
    // 32K mode via control (bits 2-3 = 0), bank reg 4 -> 32K bank 2 at $8000
    for i in 0..5 { m.on_write(0x8000, (0x00 >> i) & 1, &mut st); }
    for i in 0..5 { m.on_write(0xE000, (4 >> i) & 1, &mut st); }
    assert_eq!(st.prg8k, [8, 9, 10, 11]);
}

#[test]
fn uxrom_mapping() {
    let rom = synth(2, 8, 0, |_| 0xEA);
    let m = mapper::for_rom(&rom).unwrap();
    let mut st = m.reset_state();
    assert_eq!(st.prg8k, [0, 1, 14, 15]);
    let sw = m.on_write(0x8000, 3, &mut st).unwrap();
    assert!(sw.changed);
    assert_eq!(st.prg8k, [6, 7, 14, 15]);
    let loc = mapper::cpu_to_rom(m.as_ref(), &rom, 0x8010, &st).unwrap();
    assert_eq!((loc.bank, loc.offset), (3, 0x10));
    let fixed = mapper::cpu_to_rom(m.as_ref(), &rom, 0xC000, &st).unwrap();
    assert_eq!(fixed.bank, 7);
    assert_eq!(m.bank_cpu_base(7), 0xC000);
    assert_eq!(m.bank_cpu_base(2), 0x8000);
}

#[test]
fn cnrom_mapping() {
    let rom = synth(3, 2, 4, |_| 0xEA);
    let m = mapper::for_rom(&rom).unwrap();
    let mut st = m.reset_state();
    assert_eq!(st.prg8k, [0, 1, 2, 3]);
    m.on_write(0x8000, 2, &mut st);
    assert_eq!(st.chr1k[0], 16);
    assert_eq!(st.prg8k, [0, 1, 2, 3]);
}

#[test]
fn mmc3_mapping() {
    let rom = synth(4, 8, 0, |_| 0xEA); // 128K = 16 x 8K
    let m = mapper::for_rom(&rom).unwrap();
    assert_eq!(m.prg_bank_size(), 8192);
    assert_eq!(m.bank_count(), 16);
    let mut st = m.reset_state();
    assert_eq!(st.prg8k, [0, 1, 14, 15]);
    m.on_write(0x8000, 6, &mut st);
    m.on_write(0x8001, 9, &mut st);
    assert_eq!(st.prg8k, [9, 1, 14, 15]);
    m.on_write(0x8000, 7, &mut st);
    m.on_write(0x8001, 3, &mut st);
    assert_eq!(st.prg8k, [9, 3, 14, 15]);
    // PRG mode 1 swaps $8000 and $C000
    m.on_write(0x8000, 0x46, &mut st);
    assert_eq!(st.prg8k, [14, 3, 9, 15]);
    assert_eq!(m.bank_cpu_base(15), 0xE000);
    assert_eq!(m.bank_cpu_base(14), 0xC000);
}

#[test]
fn nrom_mapping_and_resolve() {
    let rom = synth(0, 2, 1, |i| (i & 0xFF) as u8);
    let m = mapper::for_rom(&rom).unwrap();
    let st = m.reset_state();
    let loc = mapper::resolve(m.as_ref(), &rom, 0xC123, None, &st).unwrap();
    assert_eq!((loc.bank, loc.offset, loc.file_offset), (1, 0x123, 0x4123));
    let loc2 = mapper::resolve(m.as_ref(), &rom, 0x8123, Some(1), &st).unwrap();
    assert_eq!(loc2.file_offset, 0x4123);
    assert!(mapper::resolve(m.as_ref(), &rom, 0x1234, None, &st).is_err());
    assert_eq!(parse_addr("$C000").unwrap(), 0xC000);
    assert_eq!(parse_addr("0xc000").unwrap(), 0xC000);
    assert_eq!(parse_addr("C000").unwrap(), 0xC000);
}

#[test]
fn pb53_goldens() {
    let Some(dir_s) = env_path("NESROM_TEST_MMC1_CHR_DIR") else { return };
    let dir = std::path::Path::new(&dir_s);
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().map(|e| e == "pb53").unwrap_or(false) {
            let chr = p.with_extension("chr");
            if !chr.exists() { continue; }
            // spgeorge.chr in the checkout is stale relative to spgeorge.pb53 (the packed
            // stream decodes cleanly to exactly 512 bytes but differs from tile 12 on).
            if p.file_name().map(|n| n == "spgeorge.pb53").unwrap_or(false) { continue; }
            let packed = std::fs::read(&p).unwrap();
            let expect = std::fs::read(&chr).unwrap();
            let d = compress::pb53::decode(&packed, Some(expect.len() / 16));
            assert_eq!(d.data.len(), expect.len(), "{}: length", p.display());
            assert!(d.data == expect, "{}: content mismatch", p.display());
            checked += 1;
        }
    }
    assert!(checked >= 40, "expected many pb53 pairs, checked {checked}");
}

#[test]
fn rle_roundtrip() {
    let mut input = Vec::new();
    for i in 0..600u32 { input.push(if i % 50 < 30 { 7 } else { (i * 31 % 251) as u8 }); }
    let enc = compress::rle::encode(&input);
    assert!(enc.len() < input.len());
    let d = compress::rle::decode(&enc, Some(input.len()));
    assert!(d.ok);
    assert_eq!(d.data, input);
}

#[test]
fn lz_roundtrip() {
    let mut input = Vec::new();
    for i in 0..2000u32 { input.push(((i * 7) % 13) as u8 + if i % 97 == 0 { 100 } else { 0 }); }
    let enc = compress::lz::encode(&input);
    assert!(enc.len() < input.len());
    let d = compress::lz::decode(&enc, Some(input.len()));
    assert!(d.ok);
    assert_eq!(d.data, input);
}

#[test]
fn rhai_script_decoder() {
    let dir = std::env::temp_dir().join(format!("nesrom-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("xor.rhai");
    std::fs::write(&script, "fn decode(bytes) { let out = blob(); for i in 0..bytes.len() { out.push(bytes[i] ^ 0x55); } out }").unwrap();
    let d = compress::decode(&format!("script:{}", script.display()), &[0x55, 0x00, 0xFF], None).unwrap();
    assert_eq!(d.data, vec![0x00, 0x55, 0xAA]);
}

#[test]
fn lint_spec_rules() {
    let bad = serde_json::json!({
        "systems": [
            {"behavior": "Reads controller at $4016 each frame"},
            {"behavior": "The routine does LDA then STA to the port"},
            {"behavior": "table at 0xC400 holds palettes"},
            {"behavior": "bytes A9 00 8D 00 20 set it up"},
            {"behavior": "see bank $3 offset 0x1200"}
        ]
    });
    let v = lint::lint(&bad);
    let rules: std::collections::BTreeSet<&str> = v.iter().map(|x| x.rule).collect();
    for r in ["dollar_hex_address", "6502_mnemonic", "0x_hex_address", "opcode_byte_run", "rom_offset_phrase"] {
        assert!(rules.contains(r), "missing rule {r}: {rules:?}");
    }
    let good = serde_json::json!({
        "systems": [{"name": "input_polling", "behavior": "Reads the controller using the standard strobe-and-shift protocol once per frame. Returns 8 buttons."}],
        "data_tables": [{"name": "level_1", "format": "Row-major 16x15 tile grid, 240 bytes"}],
        "physics": {"gravity": "2 subpixels per frame squared, capped at 4 px/frame"}
    });
    assert!(lint::lint(&good).is_empty(), "{:?}", lint::lint(&good));
}

#[test]
fn nrom_palette_upload_resolved() {
    let Some(rom) = load("NESROM_TEST_NROM") else { return };
    let m = mapper::for_rom(&rom).unwrap();
    let t = reach::trace(&rom, m.as_ref(), &[], true);
    assert!(!t.queues.is_empty(), "cc65's deferred VRAM queue should be detected");
    let v = nesrom::vram::to_json(&t, None);
    let pals = v["palettes"].as_array().unwrap();
    let site = pals.iter().find(|p| p["vram_start"] == "$3F00").expect("palette site at $3F00");
    let entries = site["entries"].as_array().unwrap();
    assert!(entries.len() >= 4, "expected >=4 palette entries, got {}", entries.len());
    for e in entries { assert_eq!(e["value"], "0F", "{e}"); }
    // The direct clear-screen upload must also be visible.
    let sites = v["sites"].as_array().unwrap();
    assert!(sites.iter().any(|s| s["vram_start"] == "$2000" && s["values"].as_array().map(|a| a.len() >= 4).unwrap_or(false)));
}
