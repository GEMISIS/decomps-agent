//! PB53 tile decompressor (format by Damian Yerrick; this is an independent
//! implementation from the published format description).
//!
//! Data is a stream of 16-byte tiles (two 8-byte bit planes). Each tile starts
//! with a control byte:
//! * `$84-$87`: solid tile; bit 0 = plane 0 fill ($00/$FF), bit 1 = plane 1 fill.
//! * `$82`: repeat the previous tile (16 bytes).
//! * `$83`: repeat the tile 4096 bytes back (same tile of the previous 4 KB table).
//! * `$80/$81`: this plane is solid $00/$FF.
//! * `$00-$7F`: PB8 plane: the byte is a flag word; the first plane byte follows
//!   literally, then for bits 6..0 (MSB first) a set bit repeats the previous
//!   byte and a clear bit reads a new byte.
//! After plane 0, the second plane's control byte may also be `$82` (copy of
//! plane 0) or `$83` (complement of plane 0).

use super::Decoded;

fn plane(ctrl: u8, input: &[u8], i: &mut usize, out: &mut Vec<u8>) -> bool {
    if ctrl >= 0x80 {
        let fill = if ctrl & 1 != 0 { 0xFF } else { 0x00 };
        out.extend(std::iter::repeat(fill).take(8));
        return true;
    }
    if *i >= input.len() { return false; }
    let mut last = input[*i];
    *i += 1;
    out.push(last);
    let mut flag = ctrl;
    for _ in 1..8 {
        flag <<= 1;
        if flag & 0x80 != 0 {
            out.push(last);
        } else {
            if *i >= input.len() { return false; }
            last = input[*i];
            *i += 1;
            out.push(last);
        }
    }
    true
}

pub fn decode(input: &[u8], max_tiles: Option<usize>) -> Decoded {
    let limit = max_tiles.map(|t| t * 16).unwrap_or(usize::MAX);
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < input.len() && out.len() < limit {
        let ctrl = input[i];
        i += 1;
        match ctrl {
            0x84..=0x87 => {
                let p0 = if ctrl & 1 != 0 { 0xFF } else { 0x00 };
                let p1 = if ctrl & 2 != 0 { 0xFF } else { 0x00 };
                out.extend(std::iter::repeat(p0).take(8));
                out.extend(std::iter::repeat(p1).take(8));
            }
            0x82 => {
                if out.len() < 16 { return Decoded { data: out, consumed: i, ok: false }; }
                let s = out.len() - 16;
                for k in 0..16 { let b = out[s + k]; out.push(b); }
            }
            0x83 => {
                if out.len() < 4096 { return Decoded { data: out, consumed: i, ok: false }; }
                let s = out.len() - 4096;
                for k in 0..16 { let b = out[s + k]; out.push(b); }
            }
            _ => {
                if !plane(ctrl, input, &mut i, &mut out) { return Decoded { data: out, consumed: i, ok: false }; }
                if i >= input.len() { return Decoded { data: out, consumed: i, ok: false }; }
                let c2 = input[i];
                i += 1;
                if c2 == 0x82 || c2 == 0x83 {
                    let x = if c2 & 1 != 0 { 0xFF } else { 0x00 };
                    let s = out.len() - 8;
                    for k in 0..8 { let b = out[s + k] ^ x; out.push(b); }
                } else if !plane(c2, input, &mut i, &mut out) {
                    return Decoded { data: out, consumed: i, ok: false };
                }
            }
        }
    }
    let ok = max_tiles.map(|t| out.len() >= t * 16).unwrap_or(true);
    Decoded { data: out, consumed: i, ok }
}
