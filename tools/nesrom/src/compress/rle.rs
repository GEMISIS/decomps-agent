//! Generic byte-oriented RLE.
//!
//! Layout: a control byte `n` followed by data.
//! * `n & 0x80 != 0`: repeat the next single byte `(n & 0x7F) + 1` times.
//! * otherwise: copy the next `n + 1` bytes literally.
//! Decoding stops when the input is exhausted or `max_out` bytes were produced.

use super::Decoded;

pub fn decode(input: &[u8], max_out: Option<usize>) -> Decoded {
    let limit = max_out.unwrap_or(usize::MAX);
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() && out.len() < limit {
        let n = input[i];
        i += 1;
        if n & 0x80 != 0 {
            let count = (n & 0x7F) as usize + 1;
            if i >= input.len() {
                return Decoded { data: out, consumed: i, ok: false };
            }
            let b = input[i];
            i += 1;
            for _ in 0..count {
                if out.len() >= limit { break; }
                out.push(b);
            }
        } else {
            let count = n as usize + 1;
            for _ in 0..count {
                if i >= input.len() {
                    return Decoded { data: out, consumed: i, ok: false };
                }
                if out.len() >= limit { break; }
                out.push(input[i]);
                i += 1;
            }
        }
    }
    let ok = max_out.map(|m| out.len() >= m).unwrap_or(true);
    Decoded { data: out, consumed: i, ok }
}

/// Reference encoder (used by tests and `decode-try` plausibility checks).
pub fn encode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let b = input[i];
        let mut run = 1;
        while i + run < input.len() && input[i + run] == b && run < 128 { run += 1; }
        if run >= 3 {
            out.push(0x80 | (run as u8 - 1));
            out.push(b);
            i += run;
        } else {
            let start = i;
            let mut lit = 0;
            while i < input.len() && lit < 128 {
                let b2 = input[i];
                let mut r = 1;
                while i + r < input.len() && input[i + r] == b2 && r < 3 { r += 1; }
                if r >= 3 { break; }
                i += 1;
                lit += 1;
            }
            out.push(lit as u8 - 1);
            out.extend_from_slice(&input[start..start + lit]);
        }
    }
    out
}
