//! Recursive-descent reachability with bank-state propagation.

use crate::cpu6502::{self, Insn, Mode};
use crate::sim::{self, Regs};
use crate::vram::{self, Event, Queue, Vram, VramWrite};
use crate::ines::Rom;
use crate::mapper::{self, BankState, Mapper};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct TraceInsn {
    pub cpu_addr: u16,
    pub bank: usize,
    pub file_offset: usize,
    pub insn: Insn,
    /// File offset of the subroutine (or vector entry) this instruction was first reached through.
    pub sub: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Sub {
    pub cpu_addr: u16,
    pub bank: usize,
    pub file_offset: usize,
    pub kind: String,
    pub callers: BTreeSet<usize>,
    pub calls: BTreeSet<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub from: String,
    pub from_bank: usize,
    pub to: String,
    pub to_bank: usize,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_value: Option<usize>,
    #[serde(skip)]
    pub from_off: usize,
    #[serde(skip)]
    pub to_off: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HwHit {
    pub addr: String,
    pub register: &'static str,
    pub role: &'static str,
    pub access: &'static str,
    pub from: String,
    pub from_bank: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_value: Option<String>,
    #[serde(skip)]
    pub from_off: usize,
    #[serde(skip)]
    pub addr_num: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct SwitchSite {
    pub addr: String,
    pub bank: usize,
    pub register: String,
    pub resolved_value: serde_json::Value,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unresolved { pub addr: String, pub bank: usize, pub kind: String, pub detail: String }

#[derive(Debug, Clone, Serialize)]
pub struct Entry { pub name: String, pub addr: String, pub bank: usize, #[serde(skip)] pub off: usize }

#[derive(Debug, Default)]
pub struct Trace {
    pub insns: BTreeMap<usize, TraceInsn>,
    pub subs: BTreeMap<usize, Sub>,
    pub edges: Vec<Edge>,
    pub hw: Vec<HwHit>,
    pub switches: Vec<SwitchSite>,
    pub unresolved: Vec<Unresolved>,
    pub entries: Vec<Entry>,
    /// VRAM annotations keyed by file offset (from tracing and helper replay).
    pub notes: BTreeMap<usize, String>,
    pub vram_writes: Vec<VramWrite>,
    pub queues: Vec<Queue>,
    pub seq: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Range { pub bank: usize, pub start: String, pub end: String, pub len: usize, #[serde(skip)] pub file_start: usize, #[serde(skip)] pub file_end: usize }

struct Item { pc: u16, st: BankState, regs: Regs, sub: Option<usize>, vram: Vram }

#[derive(Debug, Clone)]
struct CallSite { jsr_pc: u16, jsr_off: usize, jsr_bank: usize, callee_off: usize, callee_pc: u16, regs: Regs, st: BankState, sub: Option<usize> }

struct Engine<'a> {
    rom: &'a Rom,
    m: &'a dyn Mapper,
    work: VecDeque<Item>,
    visited: HashSet<(usize, [usize; 4])>,
    call_sites: Vec<CallSite>,
    helper_subs: BTreeSet<usize>,
    processed_sites: HashSet<(usize, [usize; 4], Option<u8>, Option<u8>, Option<u8>)>,
    indirects: Vec<Indirect>,
    processed_indirects: HashSet<(usize, [usize; 4])>,
    queues: Vec<Queue>,
    steps: usize,
}

#[derive(Debug, Clone)]
enum IndKind { Ptr(u16), RtsTable { lo: u16, hi: u16 } }

#[derive(Debug, Clone)]
struct Indirect { pc: u16, off: usize, bank: usize, kind: IndKind, st: BankState }

pub fn hex4(v: u16) -> String { format!("${v:04X}") }

fn access_kind(insn: &Insn) -> &'static str {
    if insn.is_store() { "write" } else if insn.is_rmw() { "rmw" } else { "read" }
}

impl Trace {
    fn add_sub(&mut self, loc: &mapper::RomLoc, kind: &str) {
        let e = self.subs.entry(loc.file_offset).or_insert_with(|| Sub { cpu_addr: loc.cpu_addr, bank: loc.bank, file_offset: loc.file_offset, kind: kind.to_string(), ..Default::default() });
        if e.kind == "subroutine" && kind != "subroutine" { e.kind = kind.to_string(); }
    }

    fn edge(&mut self, from: &mapper::RomLoc, to: &mapper::RomLoc, st: &BankState, m: &dyn Mapper, base_kind: &str) {
        let switchable = m.windows().iter().any(|w| w.kind == "switchable" && to.cpu_addr >= w.start && to.cpu_addr <= w.end);
        let (kind, bank_value) = if st.dynamic && switchable && from.bank != to.bank {
            ("dynamic".to_string(), None)
        } else if from.bank != to.bank {
            ("cross_bank".to_string(), Some(to.bank))
        } else {
            (base_kind.to_string(), None)
        };
        self.edges.push(Edge { from: hex4(from.cpu_addr), from_bank: from.bank, to: hex4(to.cpu_addr), to_bank: to.bank, kind, bank_value, from_off: from.file_offset, to_off: to.file_offset });
    }

    /// Record a VRAM event: annotate the instruction and log resolved writes.
    pub fn record_vram(&mut self, ev: &Event, off: usize, pc: u16, bank: usize, routine: &str, via_call: Option<u16>, caller: Option<String>) {
        let note = vram::note_for(ev);
        self.notes.entry(off).or_insert(note);
        if let Event::Write { addr, value, reg, queue } = ev {
            let desc = value.describe(reg);
            let source = if queue.is_some() { format!("queue {}", value.source_kind()) } else { value.source_kind() };
            let via = via_call.map(hex4);
            if self.vram_writes.iter().any(|w| w.off == off && w.vram_n == *addr && w.value_desc == desc && w.via_call == via) { return; }
            self.seq += 1;
            self.vram_writes.push(VramWrite { addr: hex4(pc), bank, routine: routine.to_string(), caller, vram: hex4(*addr), region: vram::region(*addr), value: value.v, value_desc: desc, source, via_call: via, off, vram_n: *addr, seq: self.seq });
            if let Some(h) = self.hw.iter_mut().rev().find(|h| h.from_off == off) {
                if h.vram_target.is_none() {
                    h.vram_target = Some(hex4(*addr));
                    h.vram_region = Some(vram::region(*addr));
                    h.vram_value = Some(value.describe(reg));
                }
            }
        }
    }

    fn dedupe_edges(&mut self) {
        let mut seen: HashSet<(usize, usize, String, Option<usize>)> = HashSet::new();
        self.edges.retain(|e| seen.insert((e.from_off, e.to_off, e.kind.clone(), e.bank_value)));
        let mut seen_sw: HashSet<String> = HashSet::new();
        self.switches.retain(|s| seen_sw.insert(format!("{}|{}|{}|{}", s.addr, s.bank, s.register, s.resolved_value)));
    }

    pub fn code_ranges(&self) -> Vec<Range> {
        let mut out: Vec<Range> = Vec::new();
        for ti in self.insns.values() {
            let end_off = ti.file_offset + ti.insn.len as usize;
            if let Some(last) = out.last_mut() {
                if last.file_end == ti.file_offset && last.bank == ti.bank {
                    last.file_end = end_off;
                    last.end = hex4(ti.cpu_addr + ti.insn.len as u16 - 1);
                    last.len = last.file_end - last.file_start;
                    continue;
                }
            }
            out.push(Range { bank: ti.bank, start: hex4(ti.cpu_addr), end: hex4(ti.cpu_addr + ti.insn.len as u16 - 1), len: ti.insn.len as usize, file_start: ti.file_offset, file_end: end_off });
        }
        out
    }

    pub fn data_ranges(&self, rom: &Rom, m: &dyn Mapper) -> Vec<Range> {
        let size = m.prg_bank_size();
        let code = self.code_ranges();
        let mut out = Vec::new();
        let mut pos = 0usize;
        let total = rom.prg.len();
        let push = |s: usize, e: usize, out: &mut Vec<Range>| {
            // split at native bank boundaries
            let mut a = s;
            while a < e {
                let bank_end = (a / size + 1) * size;
                let b = e.min(bank_end);
                let cpu = mapper::file_offset_to_default_cpu(m, a);
                out.push(Range { bank: a / size, start: hex4(cpu), end: hex4(cpu + (b - a - 1) as u16), len: b - a, file_start: a, file_end: b });
                a = b;
            }
        };
        for r in &code {
            if r.file_start > pos { push(pos, r.file_start, &mut out); }
            pos = pos.max(r.file_end);
        }
        if pos < total { push(pos, total, &mut out); }
        out
    }

    /// Subroutines reachable (through `calls`) from the given entry file offsets.
    pub fn closure(&self, roots: &[usize]) -> BTreeSet<usize> {
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut q: VecDeque<usize> = roots.iter().copied().collect();
        while let Some(r) = q.pop_front() {
            if !seen.insert(r) { continue; }
            if let Some(s) = self.subs.get(&r) {
                for c in &s.calls { q.push_back(*c); }
            }
        }
        seen
    }

    pub fn entry_off(&self, name: &str) -> Option<usize> {
        self.entries.iter().find(|e| e.name == name).map(|e| e.off)
    }

    /// Sub file offsets that own each instruction, grouped.
    pub fn insns_of_sub(&self, sub_off: usize) -> Vec<&TraceInsn> {
        self.insns.values().filter(|i| i.sub == Some(sub_off)).collect()
    }

    pub fn to_json(&self, rom: &Rom, m: &dyn Mapper, only_bank: Option<usize>) -> serde_json::Value {
        let keep = |b: usize| only_bank.map(|ob| ob == b).unwrap_or(true);
        let subs: Vec<serde_json::Value> = self.subs.values().filter(|s| keep(s.bank)).map(|s| {
            serde_json::json!({
                "addr": hex4(s.cpu_addr), "bank": s.bank, "kind": s.kind,
                "callers": s.callers.iter().filter_map(|c| self.insns.get(c)).map(|i| serde_json::json!({"addr": hex4(i.cpu_addr), "bank": i.bank})).collect::<Vec<_>>(),
                "calls": s.calls.iter().filter_map(|c| self.subs.get(c)).map(|t| serde_json::json!({"addr": hex4(t.cpu_addr), "bank": t.bank})).collect::<Vec<_>>(),
                "insn_count": self.insns.values().filter(|i| i.sub == Some(s.file_offset)).count(),
            })
        }).collect();
        serde_json::json!({
            "rom": rom.info(),
            "mapper": {"number": m.number(), "name": m.name(), "prg_bank_size": m.prg_bank_size(), "bank_count": m.bank_count()},
            "entries": self.entries,
            "code_ranges": self.code_ranges().into_iter().filter(|r| keep(r.bank)).collect::<Vec<_>>(),
            "subroutines": subs,
            "call_graph": self.edges.iter().filter(|e| keep(e.from_bank) || keep(e.to_bank)).collect::<Vec<_>>(),
            "data_ranges": self.data_ranges(rom, m).into_iter().filter(|r| keep(r.bank)).collect::<Vec<_>>(),
            "hw_hits": self.hw.iter().filter(|h| keep(h.from_bank)).collect::<Vec<_>>(),
            "bank_switch_sites": self.switches.iter().filter(|s| keep(s.bank)).collect::<Vec<_>>(),
            "unresolved_indirect_jumps": self.unresolved,
            "instruction_count": self.insns.values().filter(|i| keep(i.bank)).count(),
        })
    }
}

/// Trace from the reset/NMI/IRQ vectors (reset-state mapping) plus extra entries.
pub fn trace(rom: &Rom, m: &dyn Mapper, extra: &[(u16, Option<usize>)], include_vectors: bool) -> Trace {
    let st0 = m.reset_state();
    let mut entries: Vec<(String, u16, BankState)> = Vec::new();
    if include_vectors {
        for (name, vec_addr) in [("reset", 0xFFFCu16), ("nmi", 0xFFFA), ("irq", 0xFFFE)] {
            if let Some(loc) = mapper::cpu_to_rom(m, rom, vec_addr, &st0) {
                if let Some(t) = mapper::read_u16(&rom.prg, loc.file_offset) {
                    entries.push((name.to_string(), t, st0.clone()));
                }
            }
        }
    }
    for (i, (addr, bank)) in extra.iter().enumerate() {
        let st = match bank { Some(b) => m.state_for_bank(*b, *addr, &st0), None => st0.clone() };
        entries.push((format!("entry{i}"), *addr, st));
    }
    trace_from(rom, m, entries)
}

pub fn trace_from(rom: &Rom, m: &dyn Mapper, entries: Vec<(String, u16, BankState)>) -> Trace {
    trace_from_with_queues(rom, m, entries, Vec::new())
}

/// Like `trace_from`, but seeds known VRAM queues (e.g. detected on a full trace from the vectors).
pub fn trace_from_with_queues(rom: &Rom, m: &dyn Mapper, entries: Vec<(String, u16, BankState)>, known: Vec<Queue>) -> Trace {
    let first = trace_pass(rom, m, entries.clone(), known.clone());
    let mut queues = vram::detect_queues(&first);
    for k in known { if !queues.iter().any(|q| q.hi_n == k.hi_n && q.lo_n == k.lo_n && q.val_n == k.val_n) { queues.push(k); } }
    if queues.is_empty() { return first; }
    let mut second = trace_pass(rom, m, entries, queues.clone());
    second.queues = queues;
    second
}

fn trace_pass(rom: &Rom, m: &dyn Mapper, entries: Vec<(String, u16, BankState)>, queues: Vec<Queue>) -> Trace {
    let mut t = Trace::default();
    let mut eng = Engine { rom, m, work: VecDeque::new(), visited: HashSet::new(), call_sites: Vec::new(), helper_subs: BTreeSet::new(), processed_sites: HashSet::new(), indirects: Vec::new(), processed_indirects: HashSet::new(), queues, steps: 0 };
    for (name, addr, st) in entries {
        if let Some(loc) = mapper::cpu_to_rom(m, rom, addr, &st) {
            t.entries.push(Entry { name: name.clone(), addr: hex4(addr), bank: loc.bank, off: loc.file_offset });
            t.add_sub(&loc, if name.starts_with("entry") { "entry" } else { "vector" });
            eng.work.push_back(Item { pc: addr, st, regs: Regs::default(), sub: Some(loc.file_offset), vram: Vram::default() });
        } else {
            t.unresolved.push(Unresolved { addr: hex4(addr), bank: 0, kind: "entry_outside_rom".into(), detail: format!("entry {name} at ${addr:04X} is not in PRG ROM") });
        }
    }
    loop {
        eng.drain(&mut t);
        let a = eng.replay_helpers(&mut t);
        let b = eng.resolve_indirects(&mut t);
        if !a && !b { break; }
    }
    t.dedupe_edges();
    fill_hw_vram(&mut t);
    t
}

/// Back-fill vram fields on hardware hits that were annotated through helper replay.
fn fill_hw_vram(t: &mut Trace) {
    let by_off: BTreeMap<usize, &VramWrite> = t.vram_writes.iter().map(|w| (w.off, w)).collect();
    for h in t.hw.iter_mut() {
        if h.vram_target.is_some() { continue; }
        if let Some(w) = by_off.get(&h.from_off) {
            h.vram_target = Some(w.vram.clone());
            h.vram_region = Some(w.region.clone());
            h.vram_value = Some(w.value_desc.clone());
        }
    }
}

impl<'a> Engine<'a> {
    /// Straight-line replay of a callee with the caller's registers, for VRAM
    /// tracking across calls. Returns the VRAM state at the callee's return.
    #[allow(clippy::too_many_arguments)]
    fn replay_vram(&mut self, t: &mut Trace, entry: u16, regs: &Regs, st: &BankState, vram: Vram, callee: &str, call_site: u16, caller: &str, depth: u8) -> Vram {
        let mut regs = regs.clone();
        let mut v = vram;
        let mut pc = entry;
        let mut st = st.clone();
        let queues = self.queues.clone();
        for _ in 0..160 {
            let Some(loc) = mapper::cpu_to_rom(self.m, self.rom, pc, &st) else { break };
            let end = (loc.file_offset + 3).min(self.rom.prg.len());
            let insn = cpu6502::decode(&self.rom.prg[loc.file_offset..end]);
            if insn.illegal || insn.is_return() || insn.is_brk() || insn.is_jmp_ind() { break; }
            if let Some(op) = insn.mem_operand() {
                if insn.is_store() && !insn.indexed() && op >= 0x8000 && self.m.is_mapper_reg(op) {
                    if let Some(val) = regs.stored(&insn).0.v { self.m.on_write(op, val, &mut st); }
                }
            }
            if let Some(ev) = vram::step(&insn, &regs, &mut v, &queues) {
                t.record_vram(&ev, loc.file_offset, pc, loc.bank, callee, Some(call_site), Some(caller.to_string()));
            }
            if insn.is_jsr() && depth < 3 {
                if let Some(tgt) = insn.operand {
                    if tgt >= 0x8000 { v = self.replay_vram(t, tgt, &regs, &st, v, &hex4(tgt), pc, caller, depth + 1); }
                }
            }
            if insn.is_jmp_abs() { pc = insn.operand.unwrap_or(pc); continue; }
            sim::update(&insn, &mut regs, self.rom, self.m, &st);
            pc = pc.wrapping_add(insn.len as u16);
        }
        v
    }

    /// Second pass: for every call into a bank-switch helper where the caller
    /// had a known register value, replay the helper straight-line with that
    /// value and continue tracing after the call under the resulting bank state.
    fn replay_helpers(&mut self, t: &mut Trace) -> bool {
        let mut added = false;
        let sites = self.call_sites.clone();
        for cs in sites {
            if !self.helper_subs.contains(&cs.callee_off) { continue; }
            let key = (cs.jsr_off, cs.st.prg8k, cs.regs.a.v, cs.regs.x.v, cs.regs.y.v);
            if !self.processed_sites.insert(key) { continue; }
            if cs.regs.a.v.is_none() && cs.regs.x.v.is_none() && cs.regs.y.v.is_none() { continue; }
            let mut st = cs.st.clone();
            let mut regs = cs.regs.clone();
            let mut pc = cs.callee_pc;
            let mut resolved: Vec<String> = Vec::new();
            for _ in 0..256 {
                let Some(loc) = mapper::cpu_to_rom(self.m, self.rom, pc, &st) else { break };
                let end = (loc.file_offset + 3).min(self.rom.prg.len());
                let insn = cpu6502::decode(&self.rom.prg[loc.file_offset..end]);
                if insn.illegal || insn.is_return() || insn.is_brk() || insn.is_jmp_ind() { break; }
                if let Some(op) = insn.mem_operand() {
                    if insn.is_store() && !insn.indexed() && op >= 0x8000 && self.m.is_mapper_reg(op) {
                        let (val, _) = regs.stored(&insn);
                        if let Some(v) = val.v {
                            if let Some(sw) = self.m.on_write(op, v, &mut st) { if sw.changed { resolved.push(sw.description); } }
                        } else { break; }
                    }
                }
                if insn.is_jmp_abs() { pc = insn.operand.unwrap_or(pc); continue; }
                sim::update(&insn, &mut regs, self.rom, self.m, &st);
                pc = pc.wrapping_add(insn.len as u16);
            }
            if st.prg8k != cs.st.prg8k {
                st.dynamic = false;
                let callee_addr = t.subs.get(&cs.callee_off).map(|s| hex4(s.cpu_addr)).unwrap_or_default();
                t.switches.push(SwitchSite {
                    addr: hex4(cs.jsr_pc), bank: cs.jsr_bank, register: format!("helper {callee_addr}"),
                    resolved_value: serde_json::json!({"a": cs.regs.a.v, "x": cs.regs.x.v, "y": cs.regs.y.v, "prg8k": st.prg8k}),
                    description: format!("bank switch via helper {callee_addr}: {}", resolved.join("; ")),
                });
                self.work.push_back(Item { pc: cs.jsr_pc.wrapping_add(3), st, regs: Regs::default(), sub: cs.sub, vram: Vram::default() });
                added = true;
            }
        }
        added
    }

    /// Resolve `JMP (ptr)` through stores that fill the pointer: immediate
    /// lo/hi pairs, or `LDA table,X` reads of a ROM pointer table.
    fn resolve_indirects(&mut self, t: &mut Trace) -> bool {
        let rom = self.rom;
        let m = self.m;
        let mut added = false;
        let pending = self.indirects.clone();
        for ind in pending {
            if !self.processed_indirects.insert((ind.off, ind.st.prg8k)) { continue; }
            let mut targets: Vec<(u16, &'static str)> = Vec::new();
            match ind.kind {
                IndKind::Ptr(ptr) => self.pointer_targets(t, ptr, &ind.st, &mut targets),
                IndKind::RtsTable { lo, hi } => Self::table_targets(rom, m, lo, hi, &ind.st, 1, "rts_table", &mut targets),
            }
            targets.sort();
            targets.dedup();
            let from = mapper::RomLoc { cpu_addr: ind.pc, bank: ind.bank, offset: 0, file_offset: ind.off };
            let mut resolved = false;
            for (tgt, kind) in targets {
                let Some(tl) = mapper::cpu_to_rom(m, rom, tgt, &ind.st) else { continue };
                resolved = true;
                t.add_sub(&tl, "indirect_target");
                t.edge(&from, &tl, &ind.st, m, kind);
                if self.visited.contains(&(tl.file_offset, ind.st.prg8k)) { continue; }
                self.work.push_back(Item { pc: tgt, st: ind.st.clone(), regs: Regs::default(), sub: Some(tl.file_offset), vram: Vram::default() });
                added = true;
            }
            if resolved {
                let a = hex4(ind.pc);
                t.unresolved.retain(|u| !((u.kind == "indirect_jump" || u.kind == "rts_jump_table") && u.addr == a));
            }
        }
        added
    }

    /// Targets of `JMP (ptr)`: stores that fill ptr/ptr+1 from immediates or ROM tables.
    fn pointer_targets(&self, t: &Trace, ptr: u16, st: &BankState, targets: &mut Vec<(u16, &'static str)>) {
        let lo_addr = ptr;
        let hi_addr = ptr.wrapping_add(1);
        let stores: Vec<(usize, bool)> = t.insns.values()
            .filter(|i| i.insn.mnemonic == "STA" && !i.insn.indexed())
            .filter_map(|i| i.insn.mem_operand().map(|op| (i.file_offset, op)))
            .filter(|(_, op)| *op == lo_addr || *op == hi_addr)
            .map(|(off, op)| (off, op == hi_addr))
            .collect();
        let mut imm_lo: Vec<u8> = Vec::new();
        let mut imm_hi: Vec<u8> = Vec::new();
        let mut tab_lo: Vec<u16> = Vec::new();
        let mut tab_hi: Vec<u16> = Vec::new();
        for (off, is_hi) in &stores {
            let src = t.insns.range(..*off).rev().take(6).find(|(_, i)| i.insn.mnemonic == "LDA");
            let Some((_, ld)) = src else { continue };
            match (ld.insn.mode, ld.insn.operand) {
                (Mode::Imm, Some(v)) => if *is_hi { imm_hi.push(v as u8) } else { imm_lo.push(v as u8) },
                (Mode::Abx | Mode::Aby, Some(tbl)) if tbl >= 0x8000 => if *is_hi { tab_hi.push(tbl) } else { tab_lo.push(tbl) },
                _ => {}
            }
        }
        for &l in &imm_lo { for &h in &imm_hi { targets.push(((h as u16) << 8 | l as u16, "indirect_immediate")); } }
        for &tl in &tab_lo { for &th in &tab_hi { Self::table_targets(self.rom, self.m, tl, th, st, 0, "indirect_table", targets); } }
    }

    /// Read a pointer table (split lo/hi tables, or interleaved words when hi == lo + 1).
    fn table_targets(rom: &Rom, m: &dyn Mapper, lo: u16, hi: u16, st: &BankState, plus: u16, kind: &'static str, targets: &mut Vec<(u16, &'static str)>) {
        let stride = if hi == lo.wrapping_add(1) { 2 } else { 1 };
        let (Some(ll), Some(hl)) = (mapper::cpu_to_rom(m, rom, lo, st), mapper::cpu_to_rom(m, rom, hi, st)) else { return };
        for i in 0..96usize {
            let lo_off = ll.file_offset + i * stride;
            let hi_off = hl.file_offset + i * stride;
            if lo_off >= rom.prg.len() || hi_off >= rom.prg.len() { break; }
            let tgt = ((rom.prg[hi_off] as u16) << 8 | rom.prg[lo_off] as u16).wrapping_add(plus);
            if tgt < 0x8000 { break; }
            targets.push((tgt, kind));
        }
    }

    fn drain(&mut self, t: &mut Trace) {
        let rom = self.rom;
        let m = self.m;
        while let Some(mut it) = self.work.pop_front() {
            let mut recent: Vec<cpu6502::Insn> = Vec::new();
            loop {
                self.steps += 1;
                if self.steps > 6_000_000 { return; }
                let loc = match mapper::cpu_to_rom(m, rom, it.pc, &it.st) {
                    Some(l) => l,
                    None => {
                        t.unresolved.push(Unresolved { addr: hex4(it.pc), bank: 0, kind: "flow_outside_rom".into(), detail: "execution flowed to an address outside PRG ROM (RAM code or unmapped)".into() });
                        break;
                    }
                };
                if !self.visited.insert((loc.file_offset, it.st.prg8k)) { break; }
                let end = (loc.file_offset + 3).min(rom.prg.len());
                let insn = cpu6502::decode(&rom.prg[loc.file_offset..end]);
                let len = insn.len as u16;
                let first_visit = !t.insns.contains_key(&loc.file_offset);
                if first_visit {
                    t.insns.insert(loc.file_offset, TraceInsn { cpu_addr: it.pc, bank: loc.bank, file_offset: loc.file_offset, insn: insn.clone(), sub: it.sub });
                }

                if let Some(op) = insn.mem_operand() {
                    if first_visit {
                        if let Some((name, role)) = cpu6502::hw_register(op) {
                            let routine = it.sub.and_then(|s0| t.subs.get(&s0)).map(|s0| hex4(s0.cpu_addr));
                            t.hw.push(HwHit { addr: hex4(op), register: name, role, access: access_kind(&insn), from: hex4(it.pc), from_bank: loc.bank, routine, vram_target: None, vram_region: None, vram_value: None, from_off: loc.file_offset, addr_num: op });
                        }
                    }
                    if insn.is_store() && op >= 0x8000 && m.is_mapper_reg(op) {
                        if let Some(s) = it.sub { self.helper_subs.insert(s); }
                        let (val, _) = it.regs.stored(&insn);
                        match (val.v, insn.indexed()) {
                            (Some(v), false) => {
                                if let Some(sw) = m.on_write(op, v, &mut it.st) {
                                    t.switches.push(SwitchSite { addr: hex4(it.pc), bank: loc.bank, register: hex4(op), resolved_value: serde_json::json!(v), description: sw.description });
                                }
                            }
                            _ => {
                                it.st.dynamic = true;
                                t.switches.push(SwitchSite { addr: hex4(it.pc), bank: loc.bank, register: hex4(op), resolved_value: serde_json::json!("dynamic"), description: m.write_note(op).unwrap_or_default() });
                            }
                        }
                    }
                }

                // VRAM target tracking.
                let queues = self.queues.clone();
                if let Some(ev) = vram::step(&insn, &it.regs, &mut it.vram, &queues) {
                    let routine = it.sub.and_then(|s0| t.subs.get(&s0)).map(|s0| hex4(s0.cpu_addr)).unwrap_or_default();
                    t.record_vram(&ev, loc.file_offset, it.pc, loc.bank, &routine, None, None);
                }

                if insn.illegal { break; }
                if insn.is_branch() {
                    if let Some(tgt) = insn.target(it.pc) {
                        if let Some(tl) = mapper::cpu_to_rom(m, rom, tgt, &it.st) {
                            t.edge(&loc, &tl, &it.st, m, "branch");
                            self.work.push_back(Item { pc: tgt, st: it.st.clone(), regs: it.regs.clone(), sub: it.sub, vram: it.vram.clone() });
                        }
                    }
                } else if insn.is_jsr() {
                    if let Some(tgt) = insn.operand {
                        if let Some(tl) = mapper::cpu_to_rom(m, rom, tgt, &it.st) {
                            t.add_sub(&tl, "subroutine");
                            t.edge(&loc, &tl, &it.st, m, "jsr");
                            t.subs.get_mut(&tl.file_offset).unwrap().callers.insert(loc.file_offset);
                            if let Some(s) = it.sub { if let Some(owner) = t.subs.get_mut(&s) { owner.calls.insert(tl.file_offset); } }
                            self.call_sites.push(CallSite { jsr_pc: it.pc, jsr_off: loc.file_offset, jsr_bank: loc.bank, callee_off: tl.file_offset, callee_pc: tgt, regs: it.regs.clone(), st: it.st.clone(), sub: it.sub });
                            self.work.push_back(Item { pc: tgt, st: it.st.clone(), regs: it.regs.clone(), sub: Some(tl.file_offset), vram: it.vram.clone() });
                            // Replay the callee straight-line so VRAM state survives the call.
                            let callee_name = hex4(tgt);
                            let caller_name = it.sub.and_then(|s0| t.subs.get(&s0)).map(|s0| hex4(s0.cpu_addr)).unwrap_or_default();
                            it.vram = self.replay_vram(t, tgt, &it.regs, &it.st, it.vram.clone(), &callee_name, it.pc, &caller_name, 0);
                        } else if first_visit {
                            t.unresolved.push(Unresolved { addr: hex4(it.pc), bank: loc.bank, kind: "jsr_outside_rom".into(), detail: format!("JSR ${tgt:04X} targets RAM or unmapped space") });
                        }
                    }
                } else if insn.is_jmp_abs() {
                    if let Some(tgt) = insn.operand {
                        if let Some(tl) = mapper::cpu_to_rom(m, rom, tgt, &it.st) {
                            t.edge(&loc, &tl, &it.st, m, "jmp");
                            self.work.push_back(Item { pc: tgt, st: it.st.clone(), regs: it.regs.clone(), sub: it.sub, vram: it.vram.clone() });
                        } else if first_visit {
                            t.unresolved.push(Unresolved { addr: hex4(it.pc), bank: loc.bank, kind: "jmp_outside_rom".into(), detail: format!("JMP ${tgt:04X} targets RAM or unmapped space") });
                        }
                    }
                    break;
                } else if insn.is_jmp_ind() {
                    let ptr = insn.operand.unwrap_or(0);
                    let resolved = mapper::cpu_to_rom(m, rom, ptr, &it.st).and_then(|pl| mapper::read_u16(&rom.prg, pl.file_offset));
                    match resolved.and_then(|tgt| mapper::cpu_to_rom(m, rom, tgt, &it.st).map(|tl| (tgt, tl))) {
                        Some((tgt, tl)) => {
                            t.edge(&loc, &tl, &it.st, m, "jmp");
                            self.work.push_back(Item { pc: tgt, st: it.st.clone(), regs: it.regs.clone(), sub: it.sub, vram: it.vram.clone() });
                        }
                        None => {
                            self.indirects.push(Indirect { pc: it.pc, off: loc.file_offset, bank: loc.bank, kind: IndKind::Ptr(ptr), st: it.st.clone() });
                            if first_visit { t.unresolved.push(Unresolved { addr: hex4(it.pc), bank: loc.bank, kind: "indirect_jump".into(), detail: format!("JMP (${ptr:04X}): pointer lives in RAM; target decided at runtime") }) }
                        }
                    }
                    break;
                } else if insn.is_return() || insn.is_brk() {
                    // RTS trick: LDA hi,idx / PHA / LDA lo,idx / PHA / RTS
                    if insn.opcode == 0x60 && recent.len() >= 4 {
                        let r = &recent[recent.len() - 4..];
                        let idx = |i: &cpu6502::Insn| matches!(i.mode, Mode::Abx | Mode::Aby);
                        if r[0].mnemonic == "LDA" && idx(&r[0]) && r[1].mnemonic == "PHA" && r[2].mnemonic == "LDA" && idx(&r[2]) && r[3].mnemonic == "PHA" {
                            if let (Some(hi), Some(lo)) = (r[0].operand, r[2].operand) {
                                self.indirects.push(Indirect { pc: it.pc, off: loc.file_offset, bank: loc.bank, kind: IndKind::RtsTable { lo, hi }, st: it.st.clone() });
                                if first_visit { t.unresolved.push(Unresolved { addr: hex4(it.pc), bank: loc.bank, kind: "rts_jump_table".into(), detail: format!("RTS-trick dispatch through pointer table lo=${lo:04X} hi=${hi:04X}") }); }
                            }
                        }
                    }
                    break;
                }
                recent.push(insn.clone());
                if recent.len() > 8 { recent.remove(0); }
                sim::update(&insn, &mut it.regs, rom, m, &it.st);
                it.pc = it.pc.wrapping_add(len);
                if it.pc < 0x8000 { break; }
            }
        }
    }
}
