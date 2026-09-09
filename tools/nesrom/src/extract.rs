//! Manifest-driven asset extraction.

use crate::compress;
use crate::ines::Rom;
use crate::mapper::{self, Mapper};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct RomFormat {
    pub mapper: u16,
    #[serde(default)]
    pub prg_banks: Option<usize>,
    #[serde(default)]
    pub chr_rom_banks: Option<usize>,
    #[serde(default)]
    pub chr_ram: Option<bool>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub rom_format: RomFormat,
    #[serde(default)]
    pub assets: serde_json::Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct Extracted { pub id: String, pub path: String, pub bytes: usize, pub decoder: String, pub category: String }

fn field_str<'a>(a: &'a Value, k: &str) -> Option<&'a str> { a.get(k).and_then(|v| v.as_str()) }
fn field_usize(a: &Value, k: &str) -> Option<usize> {
    match a.get(k) {
        Some(Value::Number(n)) => n.as_u64().map(|v| v as usize),
        Some(Value::String(s)) => mapper::parse_addr(s).ok().map(|v| v as usize),
        _ => None,
    }
}

/// Locate the raw bytes an asset entry points at.
fn locate<'a>(rom: &'a Rom, m: &dyn Mapper, a: &Value) -> Result<(&'a [u8], String)> {
    let source = field_str(a, "source").unwrap_or("prg");
    let bank = field_usize(a, "bank").unwrap_or(0);
    let offset_s = field_str(a, "offset").map(|s| s.to_string()).or_else(|| a.get("offset").and_then(|v| v.as_u64()).map(|v| v.to_string())).unwrap_or_else(|| "0".into());
    match source {
        "chr" => {
            let off = if offset_s.starts_with("0x") || offset_s.starts_with('$') { mapper::parse_addr(&offset_s)? as usize } else { offset_s.parse::<usize>().or_else(|_| mapper::parse_addr(&offset_s).map(|v| v as usize))? };
            let start = bank * 8192 + off;
            if start >= rom.chr.len() { bail!("CHR bank {bank} offset {offset_s} is outside CHR ROM ({} bytes)", rom.chr.len()); }
            Ok((&rom.chr[start..], format!("chr bank {bank} + {offset_s}")))
        }
        "prg" => {
            let addr = mapper::parse_addr(&offset_s)?;
            let loc = mapper::resolve(m, rom, addr, Some(bank), &m.reset_state())?;
            Ok((&rom.prg[loc.file_offset..], format!("prg bank {bank} ${addr:04X}")))
        }
        other => bail!("unknown asset source '{other}' (expected chr or prg)"),
    }
}

pub fn run(rom: &Rom, m: &dyn Mapper, manifest_path: &str, out_dir: &str) -> Result<Vec<Extracted>> {
    let text = std::fs::read_to_string(manifest_path).with_context(|| format!("reading manifest {manifest_path}"))?;
    let man: Manifest = serde_json::from_str(&text).context("parsing manifest JSON")?;
    let f = &man.rom_format;
    if f.mapper != rom.header.mapper { bail!("manifest expects mapper {} but ROM is mapper {}", f.mapper, rom.header.mapper); }
    if let Some(n) = f.prg_banks { if n * 16384 != rom.header.prg_size { bail!("manifest expects {n} x 16K PRG banks but ROM has {} bytes", rom.header.prg_size); } }
    if let Some(n) = f.chr_rom_banks { if n * 8192 != rom.header.chr_size { bail!("manifest expects {n} x 8K CHR banks but ROM has {} bytes", rom.header.chr_size); } }
    if let Some(s) = &f.sha256 { if !s.is_empty() && s != &rom.sha256 { bail!("ROM sha256 {} does not match manifest {s}", rom.sha256); } }
    std::fs::create_dir_all(out_dir)?;
    let mut report = Vec::new();
    for (category, list) in &man.assets {
        let Some(items) = list.as_array() else { continue };
        for a in items {
            let id = field_str(a, "id").ok_or_else(|| anyhow::anyhow!("asset in {category} has no id"))?.to_string();
            let (bytes, where_) = locate(rom, m, a).with_context(|| format!("asset {id}"))?;
            let decoder = field_str(a, "format").or_else(|| field_str(a, "decoder")).unwrap_or("raw");
            let decoder = if compress::is_known(decoder) { decoder.to_string() } else { "raw".to_string() };
            let (data, ext) = match category.as_str() {
                "pattern_tables" => {
                    let tiles = field_usize(a, "tile_count").unwrap_or(256);
                    if decoder == "raw" {
                        let n = (tiles * 16).min(bytes.len());
                        (bytes[..n].to_vec(), "chr")
                    } else {
                        let d = decode_bounded(&decoder, bytes, a, Some(tiles * 16), manifest_path)?;
                        (d, "chr")
                    }
                }
                "palettes" => {
                    // `byte_count` wins; else `count` is sub-palettes (<= 8) or colour bytes (> 8).
                    let n = if let Some(b) = field_usize(a, "byte_count") { b }
                        else { let c = field_usize(a, "count").unwrap_or(4); if c > 8 { c } else { c * 4 } };
                    let n = n.min(bytes.len());
                    (bytes[..n].to_vec(), "pal")
                }
                "tilemaps" => {
                    let n = field_usize(a, "byte_count").unwrap_or(960);
                    let mut d = decode_bounded(&decoder, bytes, a, Some(n), manifest_path)?;
                    if let (Some(ao), Some(ab)) = (field_str(a, "attribute_offset"), field_usize(a, "attribute_bytes")) {
                        let mut attr = a.clone();
                        attr["offset"] = Value::String(ao.to_string());
                        let (abytes, _) = locate(rom, m, &attr)?;
                        let n2 = ab.min(abytes.len());
                        let p = Path::new(out_dir).join(format!("{id}.attr"));
                        std::fs::write(&p, &abytes[..n2])?;
                        report.push(Extracted { id: format!("{id}.attr"), path: p.display().to_string(), bytes: n2, decoder: "raw".into(), category: category.clone() });
                    }
                    if d.len() > n { d.truncate(n); }
                    (d, "nam")
                }
                _ => {
                    let n = field_usize(a, "byte_count");
                    let d = decode_bounded(&decoder, bytes, a, n, manifest_path)?;
                    (d, "bin")
                }
            };
            let p = Path::new(out_dir).join(format!("{id}.{ext}"));
            std::fs::write(&p, &data).with_context(|| format!("writing {}", p.display()))?;
            report.push(Extracted { id: id.clone(), path: p.display().to_string(), bytes: data.len(), decoder: format!("{decoder} ({where_})"), category: category.clone() });
        }
    }
    Ok(report)
}

fn decode_bounded(decoder: &str, bytes: &[u8], a: &Value, max_out: Option<usize>, manifest_path: &str) -> Result<Vec<u8>> {
    let input = match field_usize(a, "compressed_len") { Some(n) => &bytes[..n.min(bytes.len())], None => bytes };
    if decoder == "raw" {
        let n = max_out.unwrap_or(input.len()).min(input.len());
        return Ok(input[..n].to_vec());
    }
    // script paths are relative to the manifest's directory
    let dec = if let Some(p) = decoder.strip_prefix("script:") {
        let base = Path::new(manifest_path).parent().unwrap_or(Path::new("."));
        format!("script:{}", base.join(p).display())
    } else { decoder.to_string() };
    let d = compress::decode(&dec, input, max_out)?;
    Ok(d.data)
}
