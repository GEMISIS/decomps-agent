//! Simple LZSS variant.
//!
//! Layout: a flag byte, then 8 items (MSB first). A set flag bit means one
//! literal byte follows. A clear bit means a 2-byte back-reference `[lo, hi]`:
//! `offset = (((hi & 0xF0) as usize) << 4 | lo) + 1` (1..=4096 bytes back),
//! `length = (hi & 0x0F) + 3`. Decoding stops at end of input or `max_out`.

use super::Decoded;

pub fn decode(input: &[u8], max_out: Option<usize>) -> Decoded {
    let limit = max_out.unwrap_or(usize::MAX);
    let mut out = Vec::new();
    let mut i = 0;
    'outer: while i < input.len() && out.len() < limit {
        let flags = input[i];
        i += 1;
        for bit in (0..8).rev() {
            if out.len() >= limit { break 'outer; }
            if i >= input.len() { break 'outer; }
            if flags & (1 << bit) != 0 {
                out.push(input[i]);
                i += 1;
            } else {
                if i + 1 >= input.len() { return Decoded { data: out, consumed: i, ok: false }; }
                let lo = input[i] as usize;
                let hi = input[i + 1] as usize;
                i += 2;
                let offset = (((hi & 0xF0) << 4) | lo) + 1;
                let len = (hi & 0x0F) + 3;
                if offset > out.len() { return Decoded { data: out, consumed: i, ok: false }; }
                let start = out.len() - offset;
                for k in 0..len {
                    if out.len() >= limit { break; }
                    let b = out[start + k];
                    out.push(b);
                }
            }
        }
    }
    let ok = max_out.map(|m| out.len() >= m).unwrap_or(true);
    Decoded { data: out, consumed: i, ok }
}

/// Greedy reference encoder (tests).
pub fn encode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let flag_pos = out.len();
        out.push(0);
        let mut flags = 0u8;
        for bit in (0..8).rev() {
            if i >= input.len() { break; }
            let mut best = (0usize, 0usize);
            let win_start = i.saturating_sub(4096);
            for s in win_start..i {
                let mut l = 0;
                while l < 18 && i + l < input.len() && input[s + l] == input[i + l] { l += 1; }
                if l > best.1 { best = (i - s, l); }
            }
            if best.1 >= 3 {
                let off = best.0 - 1;
                out.push((off & 0xFF) as u8);
                out.push((((off >> 4) & 0xF0) as u8) | ((best.1 - 3) as u8));
                i += best.1;
            } else {
                flags |= 1 << bit;
                out.push(input[i]);
                i += 1;
            }
        }
        out[flag_pos] = flags;
    }
    out
}
