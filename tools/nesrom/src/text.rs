//! Compact, line-oriented text renderings of every subcommand's JSON result.

use serde_json::Value;

fn s<'a>(v: &'a Value, k: &str) -> &'a str { v.get(k).and_then(|x| x.as_str()).unwrap_or("") }
fn n(v: &Value, k: &str) -> i64 { v.get(k).and_then(|x| x.as_i64()).unwrap_or(0) }
fn f(v: &Value, k: &str) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) }
fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] { v.get(k).and_then(|x| x.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) }
fn b(v: &Value, k: &str) -> bool { v.get(k).and_then(|x| x.as_bool()).unwrap_or(false) }
fn strlist(v: &Value, k: &str) -> String { arr(v, k).iter().map(|x| x.as_str().unwrap_or("").to_string()).collect::<Vec<_>>().join(",") }

fn addr_list(items: &[Value], cap: usize) -> String {
    let all: Vec<String> = items.iter().map(|x| if x.is_string() { x.as_str().unwrap().to_string() } else { s(x, "addr").to_string() }).collect();
    if all.len() > cap { format!("[{},+{} more]", all[..cap].join(","), all.len() - cap) } else { format!("[{}]", all.join(",")) }
}

pub fn header(v: &Value) -> String {
    let h = &v["header"];
    let mut out = vec![format!("{} mapper {} ({}) nes2={} prg={} chr={}{} mirroring={} battery={} prg_banks={} bank_size={}",
        s(&v["rom"], "path"), n(h, "mapper"), s(v, "mapper_name"), b(h, "nes2"), n(h, "prg_size"), n(h, "chr_size"),
        if b(h, "chr_ram") { format!(" (CHR-RAM {})", n(h, "chr_ram_size")) } else { String::new() }, s(h, "mirroring"), b(h, "battery"), n(v, "prg_bank_count"), n(v, "prg_bank_size"))];
    out.push(format!("sha256={}", s(&v["rom"], "sha256")));
    for k in ["reset", "nmi", "irq"] {
        let e = &v["vectors"][k];
        if e.is_null() { out.push(format!("vector {k}: unresolved")); } else { out.push(format!("vector {k}: {} b{:02} file_offset={}", s(e, "addr"), n(e, "bank"), n(e, "file_offset"))); }
    }
    for w in arr(v, "bank_layout") {
        out.push(format!("window {} {} default_bank={} bank_count={}", s(w, "window"), s(w, "kind"), n(w, "default_bank"), n(w, "bank_count")));
    }
    if !b(v, "supported") { out.push("mapper NOT supported by nesrom (0,1,2,3,4 only)".into()); }
    out.join("\n")
}

pub fn bytes(v: &Value) -> String {
    arr(v, "rows").iter().map(|r| format!("{}  {}  {}", s(r, "addr"), s(r, "hex"), s(r, "ascii"))).collect::<Vec<_>>().join("\n")
}

fn range_lines(items: &[Value]) -> Vec<String> {
    items.iter().map(|r| format!("{}-{} b{:02} len={}", s(r, "start"), s(r, "end"), n(r, "bank"), n(r, "len"))).collect()
}

pub fn reach(v: &Value) -> String {
    let mut out = Vec::new();
    let subs = arr(v, "subroutines");
    out.push(format!("{} instructions, {} subroutines, {} code ranges, {} data ranges, {} hw hits, {} bank-switch sites, {} unresolved",
        n(v, "instruction_count"), subs.len(), arr(v, "code_ranges").len(), arr(v, "data_ranges").len(), arr(v, "hw_hits").len(), arr(v, "bank_switch_sites").len(), arr(v, "unresolved_indirect_jumps").len()));
    out.push(format!("ENTRIES: {}", arr(v, "entries").iter().map(|e| format!("{}={} b{:02}", s(e, "name"), s(e, "addr"), n(e, "bank"))).collect::<Vec<_>>().join(" ")));
    out.push(format!("SUBROUTINES ({})", subs.len()));
    for sb in subs {
        out.push(format!("{} b{:02} {} insns={} callers={} calls={}", s(sb, "addr"), n(sb, "bank"), s(sb, "kind"), n(sb, "insn_count"), addr_list(arr(sb, "callers"), 8), addr_list(arr(sb, "calls"), 8)));
    }
    out.push("CODE RANGES".into());
    out.extend(range_lines(arr(v, "code_ranges")));
    out.push("DATA RANGES".into());
    out.extend(range_lines(arr(v, "data_ranges")));
    let hw = arr(v, "hw_hits");
    out.push(format!("HARDWARE ({})", hw.len()));
    // group by owning routine: hw hits carry `from`; find the sub whose insns include it via the subroutine list order.
    let mut by_from: std::collections::BTreeMap<String, Vec<&Value>> = std::collections::BTreeMap::new();
    for h in hw { by_from.entry(s(h, "routine").to_string()).or_default().push(h); }
    for (routine, hits) in by_from {
        out.push(format!("from {}:", if routine.is_empty() { "(unknown)" } else { &routine }));
        for h in hits {
            if h.get("vram_target").is_some() {
                let val = if s(h, "access") == "write" { format!(" <= {}", s(h, "vram_value")) } else { " read".to_string() };
                out.push(format!("  {} b{:02} {} {} VRAM[{}] {}{} at {}", s(h, "addr"), n(h, "from_bank"), s(h, "access"), s(h, "register"), s(h, "vram_target"), s(h, "vram_region"), val, s(h, "from")));
            } else {
                out.push(format!("  {} b{:02} {} {} {} at {}", s(h, "addr"), n(h, "from_bank"), s(h, "access"), s(h, "register"), s(h, "role"), s(h, "from")));
            }
        }
    }
    out.push("BANK SWITCH SITES".into());
    for x in arr(v, "bank_switch_sites") {
        let val = match &x["resolved_value"] {
            Value::String(st) => st.clone(),
            Value::Number(num) => num.to_string(),
            Value::Object(o) => {
                let reg = |k: &str| o.get(k).and_then(|v| v.as_i64()).map(|v| v.to_string()).unwrap_or_else(|| "-".into());
                format!("a={} x={} y={} prg8k=[{}]", reg("a"), reg("x"), reg("y"), o.get("prg8k").and_then(|v| v.as_array()).map(|a| a.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",")).unwrap_or_default())
            }
            other => other.to_string(),
        };
        out.push(format!("{} b{:02} {} -> {} {}", s(x, "addr"), n(x, "bank"), s(x, "register"), val, s(x, "description")));
    }
    out.push("UNRESOLVED".into());
    for u in arr(v, "unresolved_indirect_jumps") { out.push(format!("{} b{:02} {} {}", s(u, "addr"), n(u, "bank"), s(u, "kind"), s(u, "detail"))); }
    out.join("\n")
}

pub fn xrefs(v: &Value) -> String {
    let mut out = Vec::new();
    let reg = &v["register"];
    out.push(if reg.is_null() { format!("target {}", s(v, "target")) } else { format!("target {} ({} {})", s(v, "target"), s(reg, "name"), s(reg, "role")) });
    for (title, key) in [("READS:", "reads"), ("WRITES:", "writes"), ("JSR FROM:", "jsr_callers"), ("JMP FROM:", "jmp_sources"), ("BRANCH FROM:", "branch_sources")] {
        out.push(title.into());
        for e in arr(v, key) { out.push(format!("{} b{:02} {} {}{}", s(e, "addr"), n(e, "bank"), s(e, "mnemonic"), s(e, "operand"), if b(e, "speculative") { " ?" } else { "" })); }
    }
    out.join("\n")
}

pub fn entropy(v: &Value) -> String {
    arr(v, "windows").iter().map(|w| format!("{} len={} H={:.2} rep={:.2} zero={:.2} tile={:.2} text={:.2} code={:.2} guess={}", s(w, "start"), n(w, "len"), f(w, "entropy_bits"), f(w, "repetition_ratio"), f(w, "zero_ratio"), f(w, "tileness"), f(w, "text_ratio"), f(w, "code_plausibility"), s(w, "guess"))).collect::<Vec<_>>().join("\n")
}

pub fn chr(v: &Value) -> String {
    let mut out = vec![format!("chr_size={} chr_banks={} chr_ram={}", n(v, "chr_size"), n(v, "chr_banks"), b(v, "chr_ram"))];
    for t in arr(v, "tables") {
        if t.get("files").is_some() {
            for fl in arr(t, "files") { out.push(format!("file {}", fl.as_str().unwrap_or(""))); }
        } else {
            out.push(format!("chr bank {} table {} @{}: {} tiles, {} nonblank, H={:.2}", n(t, "chr_bank"), n(t, "table"), s(t, "ppu_addr"), n(t, "tile_count"), n(t, "nonblank_tiles"), f(t, "entropy_bits")));
        }
    }
    out.join("\n")
}

pub fn chr_from_prg(v: &Value) -> String {
    let mut out = vec![format!("chr_ram={}", b(v, "chr_ram")), "PPUDATA WRITER ROUTINES:".into()];
    for r in arr(v, "ppudata_writer_routines") { out.push(format!("{} b{:02} how={} pointer_zp=[{}] callers={}", s(r, "routine"), n(r, "bank"), s(r, "how"), strlist(r, "pointer_zp"), addr_list(arr(r, "callers"), 8))); }
    out.push("CANDIDATE PRG SOURCES:".into());
    for c in arr(v, "candidate_prg_sources") { out.push(format!("PRG {} b{:02} len={} conf={:.2} how={} routine={}{}", s(c, "start"), n(c, "bank"), n(c, "len"), f(c, "confidence"), s(c, "how"), s(c, "routine"), if c.get("set_up_at").is_some() { format!(" set_up_at={}", s(c, "set_up_at")) } else { String::new() })); }
    out.join("\n")
}

pub fn apu(v: &Value) -> String {
    let mut out = vec!["ROUTINES:".to_string()];
    for r in arr(v, "routines") {
        out.push(format!("{} b{:02} channels=[{}] regs=[{}] nmi={} reset={} calls={} writes={} insns={}", s(r, "addr"), n(r, "bank"), strlist(r, "channels"), strlist(r, "registers"), if b(r, "called_from_nmi") { "yes" } else { "no" }, if b(r, "called_from_reset") { "yes" } else { "no" }, n(r, "call_count"), n(r, "apu_write_count"), n(r, "insn_count")));
    }
    let c = &v["candidates"];
    let cand = |k: &str| { let x = &c[k]; if x.is_null() { "none".to_string() } else { format!("{}({:.1})", s(x, "addr"), f(x, "confidence")) } };
    out.push(format!("CANDIDATES: music_tick={} sfx_trigger={} init={}", cand("music_tick"), cand("sfx_trigger"), cand("init")));
    out.push(format!("frame_counter_writes=[{}] dmc={}", arr(v, "frame_counter_writes").iter().map(|w| s(w, "from").to_string()).collect::<Vec<_>>().join(","), if b(v, "dmc_used") { "yes" } else { "no" }));
    out.join("\n")
}

pub fn decode_try(v: &Value) -> String {
    format!("decoder={} ok={} in={} consumed={} out={} ratio={:.2} tile={:.2} H={:.2}\nsample: {}", s(v, "decoder"), b(v, "ok"), n(v, "input_len"), n(v, "consumed"), n(v, "output_len"), f(v, "ratio"), f(v, "tileness"), f(v, "entropy_bits"), s(v, "sample_hex"))
}

pub fn extract(v: &Value) -> String {
    arr(v, "assets").iter().map(|e| format!("{} {} bytes {} -> {}", s(e, "id"), n(e, "bytes"), s(e, "decoder"), s(e, "path"))).collect::<Vec<_>>().join("\n")
}

pub fn lint(v: &Value) -> String {
    if b(v, "ok") { return "OK: no barrier violations".into(); }
    arr(v, "violations").iter().map(|x| format!("{} [{}]: ...{}...", s(x, "path"), s(x, "rule"), s(x, "snippet"))).collect::<Vec<_>>().join("\n")
}

/// Truncate to `max_lines`, appending a hint.
pub fn cap(text: String, max_lines: Option<usize>) -> String {
    let Some(m) = max_lines else { return text };
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= m { return text; }
    let mut out = lines[..m].join("\n");
    out.push_str(&format!("\n... (+{} more lines, narrow the query)", lines.len() - m));
    out
}
