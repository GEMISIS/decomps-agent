//! Every subcommand's --text output on the NESROM_TEST_NROM image is non-empty and JSON-free (skipped when unset).
use std::process::Command;

fn rom() -> Option<String> { std::env::var("NESROM_TEST_NROM").ok().filter(|p| std::path::Path::new(p).exists()) }

fn run(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_nesrom")).args(args).output().expect("run nesrom");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn text_outputs_are_compact() {
    let Some(rom) = rom() else { eprintln!("skipping: NESROM_TEST_NROM not set"); return };
    let HELLO: &str = &rom;
    let dir = std::env::temp_dir().join(format!("nesrom-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = dir.join("m.json");
    std::fs::write(&manifest, r#"{"rom_format":{"mapper":0},"assets":{"pattern_tables":[{"id":"font","source":"chr","bank":0,"offset":"0x0000","tile_count":256}]}}"#).unwrap();
    let spec = dir.join("spec.json");
    std::fs::write(&spec, r#"{"systems":[{"behavior":"Reads the controller each frame."}]}"#).unwrap();
    let out_s = dir.display().to_string();
    let cases: Vec<Vec<&str>> = vec![
        vec!["--text", "header", HELLO],
        vec!["--text", "bytes", HELLO, "--addr", "$8000", "--len", "32"],
        vec!["--text", "disasm", HELLO, "--start", "$8000", "--count", "10"],
        vec!["--text", "disasm", HELLO, "--start", "$8000", "--follow", "--count", "50"],
        vec!["--text", "reach", HELLO],
        vec!["--text", "xrefs", HELLO, "--addr", "$2007"],
        vec!["--text", "entropy", HELLO, "--start", "$8000", "--len", "2048"],
        vec!["--text", "chr", HELLO],
        vec!["--text", "chr", HELLO, "--from-prg"],
        vec!["--text", "apu", HELLO],
        vec!["--text", "vram", HELLO],
        vec!["--text", "decode-try", HELLO, "--addr", "$8000", "--len", "64", "--decoder", "rle", "--max-out", "64"],
        vec!["--text", "extract", "--rom", HELLO, "--manifest", manifest.to_str().unwrap(), "--out", &out_s],
        vec!["--text", "lint-spec", spec.to_str().unwrap()],
    ];
    for c in &cases {
        let o = run(c);
        assert!(!o.trim().is_empty(), "{c:?}: empty output");
        assert!(!o.contains('{'), "{c:?}: output contains JSON: {o}");
    }
    let capped = run(&["--text", "--max-lines", "3", "reach", HELLO]);
    assert_eq!(capped.lines().count(), 4, "{capped}");
    assert!(capped.contains("more lines, narrow the query"));
    if let Some(banked) = std::env::var("NESROM_TEST_MMC1").ok().filter(|p| std::path::Path::new(p).exists()) {
        let banked: &str = &banked;
        for c in [vec!["--text", "reach", banked], vec!["--text", "chr", banked, "--from-prg"], vec!["--text", "apu", banked]] {
            let o = run(&c);
            assert!(!o.contains('{'), "{c:?}: output contains JSON");
        }
    }
    let json = run(&["header", HELLO]);
    assert!(json.trim_start().starts_with('{'), "default output must stay JSON");
}
