// @tool rom_lint_spec
// @description Run the clean-room barrier lint on spec/behavioral_spec.json (the same check the handoff applies): reports every string that leaks a memory/ROM address ($XXXX or 0xXXXX), a 6502 mnemonic, a hex byte dump or a bank offset, with its JSON path. Run it after writing the spec and fix every violation before publishing; the handoff refuses a spec that fails.
// @requires shell
shell(`test -f spec/behavioral_spec.json && nesrom lint-spec spec/behavioral_spec.json --text 2>&1 | head -n 60 || echo "spec/behavioral_spec.json not found"`)
