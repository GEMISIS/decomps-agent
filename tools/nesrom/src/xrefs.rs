//! Cross-references to an address: reads, writes, callers, jump sources.

use crate::cpu6502;
use crate::ines::Rom;
use crate::mapper::{self, Mapper};
use crate::reach::{self, hex4};
use serde_json::{json, Value};

fn entry(addr: u16, bank: usize, insn: &cpu6502::Insn, speculative: bool) -> Value {
    json!({ "addr": hex4(addr), "bank": bank, "mnemonic": insn.mnemonic, "mode": insn.mode.as_str(), "operand": insn.format_operand(addr), "speculative": speculative })
}

pub fn find(rom: &Rom, m: &dyn Mapper, target: u16) -> Value {
    let t = reach::trace(rom, m, &[], true);
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut jsr = Vec::new();
    let mut jmp = Vec::new();
    let mut branch = Vec::new();
    let mut classify = |addr: u16, bank: usize, insn: &cpu6502::Insn, spec: bool| {
        if let Some(op) = insn.mem_operand() {
            let hit = op == target || (insn.indexed() && op < target && target - op < 0x100 && op != 0);
            if hit {
                let e = entry(addr, bank, insn, spec);
                if insn.is_store() { writes.push(e) } else if insn.is_rmw() { writes.push(e) } else { reads.push(e) }
            }
        }
        if let (Some(tg), true) = (insn.target(addr), insn.is_jsr() || insn.is_jmp_abs() || insn.is_branch()) {
            if tg == target {
                let e = entry(addr, bank, insn, spec);
                if insn.is_jsr() { jsr.push(e) } else if insn.is_jmp_abs() { jmp.push(e) } else { branch.push(e) }
            }
        }
        if insn.is_jmp_ind() && insn.operand == Some(target) { jmp.push(entry(addr, bank, insn, spec)); }
    };
    for ti in t.insns.values() {
        classify(ti.cpu_addr, ti.bank, &ti.insn, false);
    }
    // Speculative linear sweep over unreached ranges.
    for r in t.data_ranges(rom, m) {
        let mut off = r.file_start;
        while off < r.file_end {
            let end = (off + 3).min(rom.prg.len());
            let insn = cpu6502::decode(&rom.prg[off..end]);
            let addr = mapper::file_offset_to_default_cpu(m, off);
            if !insn.illegal { classify(addr, r.bank, &insn, true); }
            off += insn.len as usize;
        }
    }
    json!({
        "rom": rom.info(),
        "target": hex4(target),
        "register": cpu6502::hw_register(target).map(|(n, r)| json!({"name": n, "role": r})),
        "reads": reads, "writes": writes, "jsr_callers": jsr, "jmp_sources": jmp, "branch_sources": branch,
    })
}
