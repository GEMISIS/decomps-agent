//! Decoders for common NES data compression schemes plus a Rhai-scripted hook.

pub mod lz;
pub mod pb53;
pub mod rle;
pub mod script;

use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub struct Decoded {
    pub data: Vec<u8>,
    /// Number of input bytes consumed.
    pub consumed: usize,
    /// True when the decoder reached a well-defined end (limit hit or stream ended cleanly).
    pub ok: bool,
}

/// Decoder names: `raw`, `rle`, `pb53`, `lz`, `script:<path>`.
pub fn decode(decoder: &str, input: &[u8], max_out: Option<usize>) -> Result<Decoded> {
    if let Some(path) = decoder.strip_prefix("script:") {
        return script::decode(path, input, max_out);
    }
    match decoder {
        "raw" => {
            let n = max_out.unwrap_or(input.len()).min(input.len());
            Ok(Decoded { data: input[..n].to_vec(), consumed: n, ok: true })
        }
        "rle" => Ok(rle::decode(input, max_out)),
        "pb53" => Ok(pb53::decode(input, max_out.map(|n| n / 16))),
        "lz" => Ok(lz::decode(input, max_out)),
        other => bail!("unknown decoder '{other}' (expected raw, rle, pb53, lz, or script:<path>)"),
    }
}

pub fn is_known(decoder: &str) -> bool {
    decoder.starts_with("script:") || matches!(decoder, "raw" | "rle" | "pb53" | "lz")
}
