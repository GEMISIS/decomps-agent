// report.rhai - output validator for the writer's final build report (markdown).
//
// Accepts when the report contains:
//   - a line starting with "BUILD: OK"
//   - a line starting with "ROM:" that names a .nes file
//   - a "## Tests" heading
//   - the deterministic build_check verdict line "SUMMARY: ALL PASS" (pasted from the tool)
//   - the build_hosttests summary line "HOSTTESTS: N pass, 0 fail"
// Returns () on success, else a message telling the agent what is missing.

fn validate(content) {
    let has_build_ok = false;
    let has_rom = false;
    let has_tests = false;
    let has_check = false;
    let has_hosttests = false;

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
        } else if line.contains("SUMMARY: ALL PASS") {
            has_check = true;
        } else if line.contains("HOSTTESTS:") && line.contains(" 0 fail") {
            has_hosttests = true;
        }
    }

    if has_build_ok && has_rom && has_tests && has_check && has_hosttests {
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
    if !has_check {
        missing.push("the build_check verdict line `SUMMARY: ALL PASS` pasted verbatim from the tool (run build_check; if it fails, the build is not OK - go back and fix it)");
    }
    if !has_hosttests {
        missing.push("the build_hosttests summary line `HOSTTESTS: N pass, 0 fail` pasted verbatim (run build_hosttests; every test binary must pass under the strict flags and every test/test_*.c must be wired into the Makefile's test target)");
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
