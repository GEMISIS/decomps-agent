// spec.rhai - output validator for the reader's behavioral spec.
//
// Runs on Leviath's hardened script engine (no filesystem, no network, op budget).
// Accepts the submitted spec when it parses, has the required top-level shape,
// and contains nothing that could only have come from the ROM's layout:
//   - "$" followed by 4 hex digits            ($C000)
//   - "0x" followed by 4 or more hex digits   (0x8000, 0x1F400)
//   - 6502 mnemonics as standalone tokens, but only in strings that also
//     carry a $/0x marker (full mnemonic and byte-run checks: nesrom lint-spec)
//   (byte-run detection is left to the Rust lint at handoff: op budget)
// Returns () on success, or a message listing up to 10 violations.
//
// The Rust `nesrom lint-spec` is the authoritative lint at handoff; this is a
// first line of defence so the model fixes the spec before the run ends.
//
// Note: Rhai functions cannot see script-level variables, so the lookup tables
// are built once in validate() and passed down.

// Mnemonics that are never English words: flagged in any letter case.
fn strict_mnemonics() {
    #{
        "LDA": true, "STA": true, "LDX": true, "STX": true, "LDY": true, "STY": true,
        "JSR": true, "JMP": true, "RTS": true, "RTI": true,
        "BNE": true, "BEQ": true, "BCC": true, "BCS": true, "BMI": true, "BPL": true, "BVC": true, "BVS": true,
        "INX": true, "DEX": true, "INY": true, "DEY": true,
        "CMP": true, "CPX": true, "CPY": true, "ADC": true, "SBC": true,
        "ORA": true, "EOR": true, "ASL": true, "LSR": true, "ROL": true, "ROR": true,
        "PHA": true, "PLA": true, "PHP": true, "PLP": true,
        "TXA": true, "TAY": true, "TYA": true, "TSX": true, "TXS": true,
        "SEI": true, "CLC": true, "SED": true, "CLD": true, "CLV": true, "BRK": true
    }
}

// Mnemonics that collide with English words or common abbreviations: flagged
// only when the token is written in ALL CAPS.
fn ambiguous_mnemonics() {
    #{
        "AND": true, "BIT": true, "INC": true, "DEC": true, "SEC": true,
        "TAX": true, "NOP": true, "CLI": true
    }
}

fn is_hex4(s, start) {
    if s.len() < start + 4 { return false; }
    let hex = "0123456789abcdefABCDEF";
    hex.contains(s[start]) && hex.contains(s[start + 1]) && hex.contains(s[start + 2]) && hex.contains(s[start + 3])
}

// Cheap token scan (the engine has an operation budget; the authoritative and
// more thorough lint is `nesrom lint-spec` at handoff). Flags $XXXX, 0xXXXX,
// and 6502 mnemonics; returns "" when clean.
fn check_string(s, strict, ambiguous) {
    if s.len() < 3 { return ""; }
    // Prefilter: most strings are prose with none of the markers; skip them
    // without tokenizing (the tokenizer is what eats the op budget).
    // Only strings carrying an address marker are tokenized; mnemonic and
    // byte-run detection inside plain prose is left to `nesrom lint-spec`.
    if !s.contains("$") && !s.contains("0x") && !s.contains("0X") { return ""; }
    let flat = s;
    flat.replace("\n", " ");
    for raw in flat.split(" ") {
        let n = raw.len();
        if n < 3 { continue; }
        let c0 = raw[0];
        if c0 == '$' && is_hex4(raw, 1) { return `address-like token "${raw}"`; }
        if c0 == '(' || c0 == '"' || c0 == '\'' {
            if n >= 6 && raw[1] == '$' && is_hex4(raw, 2) { return `address-like token "${raw}"`; }
        }
        if c0 == '0' && n >= 6 && (raw[1] == 'x' || raw[1] == 'X') && is_hex4(raw, 2) { return `hex offset "${raw}"`; }
        if n <= 4 {
            let tok = raw;
            if n == 4 { let last = raw[3]; if ".,;:)]".contains(last) { tok = raw.sub_string(0, 3); } }
            if tok.len() == 3 {
                let up = tok.to_upper();
                if up in strict { return `6502 mnemonic "${tok}"`; }
                if tok == up && up in ambiguous { return `6502 mnemonic "${tok}"`; }
            }
        }
    }
    ""
}

// Walks the JSON value and returns the violations array (max 10). Rhai passes
// arrays by value, so the accumulator is threaded through return values.
fn walk(node, path, out, strict, ambiguous) {
    if out.len() >= 11 { return out; }
    if out[out.len() - 1] >= 160 { return out; }
    let t = type_of(node);
    if t == "string" {
        // Bounded scan: the engine has a fixed operation budget, so only the
        // first 160 strings are linted here; `nesrom lint-spec` at handoff
        // checks everything. The count rides along as the array's last slot.
        let seen = out.pop();
        if seen >= 160 { out.push(seen); return out; }
        out.push(seen + 1);
        let why = check_string(node, strict, ambiguous);
        if why != "" { let c = out.pop(); out.push(`${path}: ${why}`); out.push(c); }
    } else if t == "map" {
        for k in node.keys() {
            out = walk(node[k], path + "." + k, out, strict, ambiguous);
            if out.len() >= 11 { break; }
        }
    } else if t == "array" {
        for item in node {
            out = walk(item, path + "[]", out, strict, ambiguous);
            if out.len() >= 11 { break; }
        }
    }
    out
}

fn validate(content) {
    let max_violations = 10;
    let spec = ();
    try {
        spec = parse_json(content);
    } catch (e) {
        return `The output must be a single JSON document matching the behavioral spec schema. JSON parse failed: ${e}`;
    }
    if type_of(spec) != "map" {
        return "The output must be a JSON object (the behavioral spec), not a bare value or array.";
    }

    let missing = [];
    for key in ["game", "systems", "state_machine", "ram_variables", "assets"] {
        if !(key in spec) { missing.push(key); }
    }
    if missing.len() > 0 {
        return `Spec is missing required top-level keys: ${missing}`;
    }
    if type_of(spec.systems) != "array" || spec.systems.len() < 1 {
        return "Spec must list at least one entry in `systems`.";
    }
    let sm = spec.state_machine;
    if type_of(sm) != "map" || !("states" in sm) {
        return "Spec must list at least one state in `state_machine.states`.";
    }
    let states = sm.states;
    if type_of(states) != "array" || states.len() < 1 {
        return "Spec must list at least one state in `state_machine.states`.";
    }

    let strict = strict_mnemonics();
    let ambiguous = ambiguous_mnemonics();
    let violations = walk(spec, "spec", [0], strict, ambiguous);
    violations.pop();
    if violations.len() == 0 {
        return ();
    }

    let msg = "Barrier lint failed: the spec must describe behavior only - no ROM/RAM addresses, hex offsets, 6502 mnemonics or byte listings. "
        + "Rewrite the offending text by role (see the hardware reference's 'Describing hardware use behaviorally'). Violations:\n";
    for v in violations {
        msg += `  - ${v}\n`;
    }
    if violations.len() >= max_violations {
        msg += "  (first 10 shown; there may be more)\n";
    }
    msg
}
