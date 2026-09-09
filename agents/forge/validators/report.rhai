// report.rhai - output validator for the writer's final build report (markdown).
//
// Accepts when the report contains:
//   - a line starting with "BUILD: OK"
//   - a line starting with "ROM:" that names a .nes file
//   - a "## Tests" heading
// Returns () on success, else a message telling the agent what is missing.

fn validate(content) {
    let has_build_ok = false;
    let has_rom = false;
    let has_tests = false;

    let text = content;
    text.replace("\r", "");
    let lines = text.split("\n");
    for raw in lines {
        let line = raw;
        line.trim();
        if line.starts_with("BUILD: OK") {
            has_build_ok = true;
        } else if line.starts_with("ROM:") {
            if line.contains(".nes") { has_rom = true; }
        } else if line.starts_with("## Tests") {
            has_tests = true;
        }
    }

    if has_build_ok && has_rom && has_tests {
        return ();
    }

    let missing = [];
    if !has_build_ok {
        missing.push("a line starting with `BUILD: OK` (only submit once `make` succeeds; if it does not, keep fixing instead of submitting)");
    }
    if !has_rom {
        missing.push("a line starting with `ROM:` giving the path of the produced .nes file");
    }
    if !has_tests {
        missing.push("a `## Tests` section summarising the host unit test results");
    }
    let msg = "The build report is missing: ";
    let i = 0;
    for m in missing {
        if i > 0 { msg += "; "; }
        msg += m;
        i += 1;
    }
    msg + "."
}
