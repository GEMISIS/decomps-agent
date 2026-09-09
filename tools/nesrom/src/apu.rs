//! APU register usage: group writes by subroutine, guess the sound-engine entry points.

use crate::cpu6502;
use crate::ines::Rom;
use crate::mapper::Mapper;
use crate::reach::{self, hex4};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn analyze(rom: &Rom, m: &dyn Mapper) -> Value {
    let t = reach::trace(rom, m, &[], true);
    let nmi_set = t.entry_off("nmi").map(|o| t.closure(&[o])).unwrap_or_default();
    let reset_set = t.entry_off("reset").map(|o| t.closure(&[o])).unwrap_or_default();
    let mut by_sub: BTreeMap<usize, (BTreeSet<u16>, usize)> = BTreeMap::new();
    for h in &t.hw {
        if !(0x4000..=0x4017).contains(&h.addr_num) || h.addr_num == 0x4014 || h.addr_num == 0x4016 { continue; }
        if h.access != "write" { continue; }
        let Some(sub) = t.insns.get(&h.from_off).and_then(|i| i.sub) else { continue };
        let e = by_sub.entry(sub).or_default();
        e.0.insert(h.addr_num);
        e.1 += 1;
    }
    let mut routines = Vec::new();
    let mut best_tick: Option<(usize, usize)> = None;
    let mut best_sfx: Option<(usize, usize)> = None;
    let mut best_init: Option<(usize, usize)> = None;
    for (sub_off, (regs, writes)) in &by_sub {
        let Some(sub) = t.subs.get(sub_off) else { continue };
        let channels: BTreeSet<&str> = regs.iter().filter_map(|r| cpu6502::apu_channel(*r)).collect();
        let from_nmi = nmi_set.contains(sub_off);
        let from_reset = reset_set.contains(sub_off);
        let insn_count = t.insns_of_sub(*sub_off).len();
        routines.push(json!({
            "addr": hex4(sub.cpu_addr), "bank": sub.bank,
            "registers": regs.iter().map(|r| hex4(*r)).collect::<Vec<_>>(),
            "channels": channels,
            "called_from_nmi": from_nmi, "called_from_reset": from_reset,
            "call_count": sub.callers.len(), "apu_write_count": writes, "insn_count": insn_count,
        }));
        let score = writes * 4 + channels.len() * 10 + insn_count / 8;
        if from_nmi && best_tick.map(|(_, s)| score > s).unwrap_or(true) { best_tick = Some((*sub_off, score)); }
        let sfx_score = sub.callers.len() * 5 + channels.len() * 3;
        if !from_nmi && sub.callers.len() >= 1 && best_sfx.map(|(_, s)| sfx_score > s).unwrap_or(true) { best_sfx = Some((*sub_off, sfx_score)); }
        if regs.contains(&0x4015) && (regs.contains(&0x4017) || from_reset) && best_init.map(|(_, s)| (regs.len()) > s).unwrap_or(true) { best_init = Some((*sub_off, regs.len())); }
    }
    let name = |o: Option<(usize, usize)>, conf: f64| o.and_then(|(off, _)| t.subs.get(&off)).map(|s| json!({"addr": hex4(s.cpu_addr), "bank": s.bank, "confidence": conf}));
    json!({
        "rom": rom.info(),
        "routines": routines,
        "candidates": {
            "music_tick": name(best_tick, if by_sub.len() > 1 { 0.6 } else { 0.4 }),
            "sfx_trigger": name(best_sfx, 0.4),
            "init": name(best_init, 0.5),
        },
        "frame_counter_writes": t.hw.iter().filter(|h| h.addr_num == 0x4017 && h.access == "write").map(|h| json!({"from": h.from, "bank": h.from_bank})).collect::<Vec<_>>(),
        "dmc_used": t.hw.iter().any(|h| (0x4010..=0x4013).contains(&h.addr_num) && h.access == "write"),
    })
}
