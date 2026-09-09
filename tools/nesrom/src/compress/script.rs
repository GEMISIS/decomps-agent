//! Rhai-scripted decoder hook: the script must define `fn decode(bytes)` taking
//! and returning a Blob (byte array). Index the input with `bytes[i]` (yields an
//! integer); build the output with `let out = blob(); out.push(value);`.
//! Example:
//! ```rhai
//! fn decode(bytes) { let out = blob(); for i in 0..bytes.len() { out.push(bytes[i] ^ 0x55); } out }
//! ```

use super::Decoded;
use anyhow::{Context, Result};
use rhai::{Blob, Engine, Scope, AST};

pub fn decode(path: &str, input: &[u8], max_out: Option<usize>) -> Result<Decoded> {
    let src = std::fs::read_to_string(path).with_context(|| format!("reading decoder script {path}"))?;
    let mut engine = Engine::new();
    engine.set_max_operations(50_000_000);
    let ast: AST = engine.compile(&src).with_context(|| format!("compiling decoder script {path}"))?;
    let mut scope = Scope::new();
    let blob: Blob = input.to_vec();
    let result: Blob = engine
        .call_fn(&mut scope, &ast, "decode", (blob,))
        .map_err(|e| anyhow::anyhow!("decoder script {path} failed: {e}"))?;
    let mut data = result;
    if let Some(m) = max_out { data.truncate(m); }
    let ok = max_out.map(|m| data.len() >= m).unwrap_or(true);
    Ok(Decoded { data, consumed: input.len(), ok })
}
