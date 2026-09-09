//! Static PPU VRAM-target resolution: track $2006 address latches, $2007
//! data writes, PPUCTRL increment mode, and deferred VRAM write queues.

use crate::cpu6502::{Insn, Mode};
use crate::reach::{self, hex4, Trace};
use crate::sim::{Regs, Val};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Queue { pub hi: String, pub lo: String, pub val: String, pub popper: String, #[serde(skip)] pub hi_n: u16, #[serde(skip)] pub lo_n: u16, #[serde(skip)] pub val_n: u16 }

#[derive(Debug, Clone, Default)]
pub struct Vram {
    pub expect_lo: bool,
    pub hi: Option<u8>,
    pub addr: Option<u16>,
    pub inc: Option<u16>,
    pub pending: BTreeMap<usize, [Option<Val>; 3]>,
}

#[derive(Debug, Clone)]
pub enum Event {
    AddrSet { addr: u16 },
    Write { addr: u16, value: Val, reg: &'static str, queue: Option<usize> },
    Read { addr: u16 },
}

/// Human name for a VRAM address.
pub fn region(addr: u16) -> String {
    let a = addr & 0x3FFF;
    match a {
        0x0000..=0x0FFF => "pattern table 0".into(),
        0x1000..=0x1FFF => "pattern table 1".into(),
        0x2000..=0x3EFF => {
            let o = (a - 0x2000) % 0x1000;
            let k = o / 0x400;
            let w = o % 0x400;
            if w >= 0x3C0 { format!("attribute table {k} byte {}", w - 0x3C0) } else { format!("nametable {k} row {} col {}", w / 32, w % 32) }
        }
        0x3F00..=0x3FFF => {
            let p = a & 0x1F;
            if p < 0x10 { format!("palette bg {p}") } else { format!("palette sprite {}", p - 0x10) }
        }
        _ => "unknown".into(),
    }
}

fn mem_target(insn: &Insn) -> Option<u16> {
    match insn.mode { Mode::Abs | Mode::Abx | Mode::Aby => insn.operand, _ => None }
}

/// Apply one instruction (registers as they were BEFORE the instruction).
pub fn step(insn: &Insn, regs: &Regs, v: &mut Vram, queues: &[Queue]) -> Option<Event> {
    let Some(op) = mem_target(insn) else { return None };
    let is_store = insn.is_store();
    if is_store {
        let (val, reg) = regs.stored(insn);
        match op {
            0x2000 => { if let Some(c) = val.v { v.inc = Some(if c & 0x04 != 0 { 32 } else { 1 }); } return None; }
            0x2006 => {
                if !v.expect_lo { v.hi = val.v; v.expect_lo = true; v.addr = None; return None; }
                v.expect_lo = false;
                v.addr = match (v.hi, val.v) { (Some(h), Some(l)) => Some(((h as u16) << 8 | l as u16) & 0x3FFF), _ => None };
                return v.addr.map(|a| Event::AddrSet { addr: a });
            }
            0x2007 => {
                let a = v.addr?;
                v.addr = Some((a + v.inc.unwrap_or(1)) & 0x3FFF);
                return Some(Event::Write { addr: a, value: val, reg, queue: None });
            }
            _ => {}
        }
        // Deferred VRAM queue: parallel arrays hi/lo/value.
        for (qi, q) in queues.iter().enumerate() {
            let slot = if op == q.hi_n { 0 } else if op == q.lo_n { 1 } else if op == q.val_n { 2 } else { continue };
            let e = v.pending.entry(qi).or_insert([None, None, None]);
            e[slot] = Some(val.clone());
            if let [Some(h), Some(l), Some(d)] = e.clone() {
                v.pending.remove(&qi);
                if let (Some(h), Some(l)) = (h.v, l.v) {
                    return Some(Event::Write { addr: ((h as u16) << 8 | l as u16) & 0x3FFF, value: d, reg: "queue", queue: Some(qi) });
                }
            }
            return None;
        }
        return None;
    }
    if insn.is_load() || insn.mnemonic == "BIT" || insn.mnemonic == "CMP" {
        if op == 0x2002 { v.expect_lo = false; v.hi = None; }
        if op == 0x2007 {
            let a = v.addr?;
            v.addr = Some((a + v.inc.unwrap_or(1)) & 0x3FFF);
            return Some(Event::Read { addr: a });
        }
    }
    None
}

pub fn note_for(ev: &Event) -> String {
    match ev {
        Event::AddrSet { addr } => format!("VRAM address = ${addr:04X} ({})", region(*addr)),
        Event::Write { addr, value, reg, queue } => format!("VRAM[${addr:04X}] <= {}{} ({})", value.describe(reg), if queue.is_some() { " via queue" } else { "" }, region(*addr)),
        Event::Read { addr } => format!("VRAM[${addr:04X}] read ({})", region(*addr)),
    }
}

/// Find "pop a VRAM queue" routines: LDA hi,idx / STA $2006 / LDA lo,idx / STA $2006 / LDA val,idx / STA $2007.
pub fn detect_queues(t: &Trace) -> Vec<Queue> {
    let mut out: Vec<Queue> = Vec::new();
    let subs: Vec<usize> = t.subs.keys().copied().collect();
    for so in subs {
        let mut insns: Vec<&reach::TraceInsn> = t.insns_of_sub(so);
        insns.sort_by_key(|i| i.file_offset);
        let mut pairs: Vec<(u16, u16)> = Vec::new(); // (source array, ppu reg)
        let mut last_load: Option<u16> = None;
        for i in &insns {
            let ins = &i.insn;
            if ins.mnemonic == "LDA" && matches!(ins.mode, Mode::Abx | Mode::Aby) && ins.operand.map(|o| o < 0x2000).unwrap_or(false) {
                last_load = ins.operand;
            } else if ins.mnemonic == "STA" && ins.mode == Mode::Abs && matches!(ins.operand, Some(0x2006) | Some(0x2007)) {
                if let Some(src) = last_load { pairs.push((src, ins.operand.unwrap())); }
                last_load = None;
            } else if ins.is_load() || ins.is_store() {
                last_load = None;
            }
        }
        for w in pairs.windows(3) {
            if w[0].1 == 0x2006 && w[1].1 == 0x2006 && w[2].1 == 0x2007 {
                let popper = t.subs.get(&so).map(|s| hex4(s.cpu_addr)).unwrap_or_default();
                let q = Queue { hi: hex4(w[0].0), lo: hex4(w[1].0), val: hex4(w[2].0), popper, hi_n: w[0].0, lo_n: w[1].0, val_n: w[2].0 };
                if !out.iter().any(|x| x.hi_n == q.hi_n && x.lo_n == q.lo_n && x.val_n == q.val_n) { out.push(q); }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct VramWrite {
    pub addr: String,
    pub bank: usize,
    pub routine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    pub vram: String,
    pub region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<u8>,
    pub value_desc: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_call: Option<String>,
    #[serde(skip)]
    pub off: usize,
    #[serde(skip)]
    pub vram_n: u16,
    #[serde(skip)]
    pub seq: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Site {
    pub addr: String,
    pub bank: usize,
    pub routine: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    pub vram_start: String,
    pub vram_region: String,
    pub count: usize,
    pub stride: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_call: Option<String>,
    #[serde(skip)]
    pub vram_n: u16,
    #[serde(skip)]
    pub addrs: Vec<u16>,
    #[serde(skip)]
    pub key: String,
}

/// Group consecutive writes (same context, ascending VRAM addresses with a fixed stride) into
/// sites, then drop duplicates produced by re-tracing the same code in another context.
pub fn sites(writes: &[VramWrite]) -> Vec<Site> {
    let mut ws: Vec<&VramWrite> = writes.iter().collect();
    ws.sort_by_key(|w| w.seq);
    let mut out: Vec<Site> = Vec::new();
    for w in ws {
        let ctx = w.caller.clone().unwrap_or_else(|| w.routine.clone());
        if let Some(last) = out.last_mut() {
            let prev = *last.addrs.last().unwrap();
            let delta = w.vram_n.wrapping_sub(prev);
            let ok_stride = if last.count == 1 { (1..=32).contains(&delta) } else { delta == last.stride };
            let same_ctx = last.key == ctx && last.source == w.source;
            if same_ctx && ok_stride && w.vram_n > prev {
                if last.count == 1 { last.stride = delta; }
                last.count += 1;
                last.addrs.push(w.vram_n);
                match (&mut last.values, w.value) { (Some(vs), Some(v)) => vs.push(format!("{v:02X}")), (vs, _) => *vs = None }
                continue;
            }
        }
        out.push(Site { addr: w.addr.clone(), bank: w.bank, routine: w.routine.clone(), caller: w.caller.clone(), vram_start: w.vram.clone(), vram_region: w.region.clone(), count: 1, stride: 1, values: w.value.map(|v| vec![format!("{v:02X}")]), source: w.source.clone(), via_call: w.via_call.clone(), vram_n: w.vram_n, addrs: vec![w.vram_n], key: ctx });
    }
    // Dedupe: same instruction, same VRAM range, same values.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    out.retain(|s| seen.insert(format!("{}|{}|{}|{}|{:?}", s.addr, s.routine, s.vram_start, s.count, s.values)));
    out
}

pub fn to_json(t: &Trace, only_bank: Option<usize>) -> Value {
    let keep = |b: usize| only_bank.map(|ob| ob == b).unwrap_or(true);
    let sites: Vec<Site> = sites(&t.vram_writes).into_iter().filter(|s| keep(s.bank)).collect();
    let palettes: Vec<Value> = sites.iter().filter(|s| (0x3F00..=0x3F1F).contains(&s.vram_n) && s.count >= 4 && s.values.is_some()).map(|s| {
        json!({ "addr": s.addr, "bank": s.bank, "routine": s.routine, "caller": s.caller, "vram_start": s.vram_start, "stride": s.stride,
                "entries": s.addrs.iter().zip(s.values.as_ref().unwrap()).map(|(a, v)| json!({"vram": hex4(*a), "region": region(*a), "value": v})).collect::<Vec<_>>(),
                "via_call": s.via_call })
    }).collect();
    json!({ "sites": sites, "palettes": palettes, "queues": t.queues, "write_count": t.vram_writes.len() })
}

pub fn text(v: &Value) -> String {
    let s = |x: &Value, k: &str| x.get(k).and_then(|y| y.as_str()).unwrap_or("").to_string();
    let n = |x: &Value, k: &str| x.get(k).and_then(|y| y.as_i64()).unwrap_or(0);
    let mut out = vec![format!("{} resolved VRAM writes, {} sites, {} palette sites, {} queues", n(v, "write_count"), v["sites"].as_array().map(|a| a.len()).unwrap_or(0), v["palettes"].as_array().map(|a| a.len()).unwrap_or(0), v["queues"].as_array().map(|a| a.len()).unwrap_or(0))];
    out.push("SITES:".into());
    for st in v["sites"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let vals = st.get("values").and_then(|x| x.as_array()).map(|a| format!(" values=[{}]", a.iter().map(|x| x.as_str().unwrap_or("")).collect::<Vec<_>>().join(","))).unwrap_or_default();
        let via = if st.get("via_call").map(|x| !x.is_null()).unwrap_or(false) { format!(" via_call={}", s(st, "via_call")) } else { String::new() };
        let caller = if st.get("caller").map(|x| !x.is_null()).unwrap_or(false) { format!(" caller={}", s(st, "caller")) } else { String::new() };
        out.push(format!("{} b{:02} routine={}{} VRAM {} {} count={} stride={} source={}{}{}", s(st, "addr"), n(st, "bank"), s(st, "routine"), caller, s(st, "vram_start"), s(st, "vram_region"), n(st, "count"), n(st, "stride"), s(st, "source"), vals, via));
    }
    out.push("PALETTES:".into());
    for p in v["palettes"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        let entries = p["entries"].as_array().map(|a| a.iter().map(|e| format!("{}={}", s(e, "vram"), s(e, "value"))).collect::<Vec<_>>().join(" ")).unwrap_or_default();
        let caller = if p.get("caller").map(|x| !x.is_null()).unwrap_or(false) { format!(" caller={}", s(p, "caller")) } else { String::new() };
        out.push(format!("{} routine={}{} at {} b{:02}: {}", s(p, "vram_start"), s(p, "routine"), caller, s(p, "addr"), n(p, "bank"), entries));
    }
    out.push("QUEUES:".into());
    for q in v["queues"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        out.push(format!("hi={} lo={} value={} popped_by={}", s(q, "hi"), s(q, "lo"), s(q, "val"), s(q, "popper")));
    }
    out.join("\n")
}
