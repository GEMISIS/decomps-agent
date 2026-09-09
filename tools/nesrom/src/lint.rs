//! Barrier lint: refuse a behavioral spec that leaks implementation details
//! (addresses, opcodes, mnemonics, ROM offsets) across the clean-room boundary.

use crate::cpu6502::MNEMONICS;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct Violation { pub path: String, pub snippet: String, pub rule: &'static str }

fn is_hex(c: char) -> bool { c.is_ascii_hexdigit() }

fn snippet(s: &str, at: usize) -> String {
    let start = s[..at].char_indices().rev().nth(20).map(|(i, _)| i).unwrap_or(0);
    let end = s[at..].char_indices().nth(40).map(|(i, _)| at + i).unwrap_or(s.len());
    s[start..end].replace('\n', " ")
}

pub fn check_string(path: &str, s: &str, out: &mut Vec<Violation>) {
    let chars: Vec<char> = s.chars().collect();
    let lower = s.to_ascii_lowercase();
    // `$XXXX`
    for (i, &c) in chars.iter().enumerate() {
        if c == '$' && i + 4 < chars.len() + 0 && chars[i + 1..].iter().take(4).filter(|c| is_hex(**c)).count() == 4 {
            let byte_at = s.char_indices().nth(i).map(|(b, _)| b).unwrap_or(0);
            out.push(Violation { path: path.into(), snippet: snippet(s, byte_at), rule: "dollar_hex_address" });
            break;
        }
    }
    // `0xXXXX`
    if let Some(pos) = lower.find("0x") {
        let mut p = pos;
        while let Some(rel) = lower[p..].find("0x") {
            let at = p + rel;
            let digits = lower[at + 2..].chars().take_while(|c| c.is_ascii_hexdigit()).count();
            if digits >= 4 {
                out.push(Violation { path: path.into(), snippet: snippet(s, at), rule: "0x_hex_address" });
                break;
            }
            p = at + 2;
        }
    }
    // standalone mnemonics (case-sensitive upper-case words)
    let words: Vec<&str> = s.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').collect();
    for w in &words {
        if MNEMONICS.contains(w) {
            out.push(Violation { path: path.into(), snippet: snippet(s, s.find(w).unwrap_or(0)), rule: "6502_mnemonic" });
            break;
        }
    }
    // runs of >= 4 hex byte pairs separated by spaces, e.g. "A9 00 8D 00 20"
    // A byte dump is space-separated hex pairs with at least one A-F digit in the run; a comma-separated
    // list of decimal numbers ("notes 20, 22, 24, 25") is legitimate behaviour text, not a dump.
    let toks: Vec<&str> = s.split_whitespace().collect();
    let mut run = 0;
    let mut has_letter = false;
    for tk in &toks {
        let comma = tk.ends_with(',') || tk.ends_with(';');
        let t = tk.trim_matches(|c: char| c == ',' || c == ';');
        if !comma && t.len() == 2 && t.chars().all(is_hex) && t.chars().any(|c| c.is_ascii_digit() || c.is_ascii_uppercase()) {
            run += 1;
            if t.chars().any(|c| c.is_ascii_uppercase()) { has_letter = true; }
            if run >= 4 && has_letter {
                out.push(Violation { path: path.into(), snippet: snippet(s, s.find(t).unwrap_or(0)), rule: "opcode_byte_run" });
                break;
            }
        } else {
            run = 0; has_letter = false;
        }
    }
    for phrase in ["bank $", "offset 0x", "offset $"] {
        if let Some(at) = lower.find(phrase) {
            out.push(Violation { path: path.into(), snippet: snippet(s, at), rule: "rom_offset_phrase" });
        }
    }
}

fn walk(v: &Value, path: String, out: &mut Vec<Violation>) {
    match v {
        Value::String(s) => check_string(&path, s, out),
        Value::Array(a) => for (i, x) in a.iter().enumerate() { walk(x, format!("{path}[{i}]"), out) },
        Value::Object(o) => for (k, x) in o {
            check_string(&format!("{path}.{k} (key)"), k, out);
            walk(x, if path.is_empty() { k.clone() } else { format!("{path}.{k}") }, out)
        },
        _ => {}
    }
}

pub fn lint(v: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    walk(v, String::new(), &mut out);
    out
}
