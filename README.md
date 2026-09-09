# NES Clean-Room Reimplementation Agents

Leviath agents that turn a NES ROM you own into clean, buildable C source for
the cc65 toolchain, through an enforced clean-room barrier, then keep that
source healthy (Remedy) and extend it (Forge). One command runs the whole
pipeline unattended and stops itself on budget or provider problems.

```
ROM ──▶ genesis-reader (Clean Room A) ──▶ behavioral spec ──▶ handoff.sh ──▶ genesis-writer (Clean Room B) ──▶ C source ──▶ build_byorom.sh ──▶ game.nes
                     │                                                                                 ▲
                     └──▶ asset manifest (stays on side A; used only by the extractor at build time) ──┘
                                                                                                        └──▶ smoke test vs the original ──▶ Remedy loop
```

No ROM, extracted asset, or game-specific data ships with this repository.
You bring your own ROM (BYOROM); everything the agents learn about it stays in
the git-ignored `workspace/` tree on your machine.

## Setup

```bash
scripts/setup.sh                  # rust, cc65, jq, python jsonschema, Leviath, nesrom, sandbox image
scripts/setup.sh --with-emulator  # adds FCEUX (used for observation and acceptance)
scripts/preflight.sh path/to/game.nes
```

`setup.sh` sets `[tool_script_permissions] shell = "allow"` in
`~/.leviath/config.toml` (a backup is kept). That is what lets the reader's
typed `rom_*` tools call `nesrom` while the reader's model itself has no shell.
Models are addressed through OpenRouter; the blueprints name a per-stage mix
(a cheap model for tool-heavy stages, a strong one for judgement stages).

## Run

```bash
scripts/pipeline.sh mygame path/to/mygame.nes                  # full unattended run
scripts/pipeline.sh mygame path/to/mygame.nes --budget 250 --rounds 4
scripts/pipeline.sh mygame path/to/mygame.nes --skip-reader --skip-writer   # rebuild + acceptance loop only
```

Outputs, all under `workspace/mygame/`:

- `writer/src`, `writer/include`, `writer/Makefile`, `writer/test` — the C project (a local git repo; Remedy commits fixes there)
- `writer/build/game.nes` — the rebuilt ROM, built from the C source plus assets extracted from your ROM
- `REPORT.md` — acceptance table, strict comparison, code stats, spend, run ids
- `provenance.json` — reader and writer run ids, ROM and spec hashes, barrier lint result
- `shots/` — screenshots, side-by-side comparisons and observation dumps

What "unattended" means:

- `preflight.sh` refuses to start if a tool, the Docker daemon, the Leviath daemon, a blueprint, the mapper, or the OpenRouter balance (must cover `--budget`) is not right.
- Every agent run is watched with a wall clock (`RUN_TIMEOUT_MIN`, default 240) and a provider watchdog (`PROVIDER_DOWN_MIN`, default 10). A stuck run, or one whose provider reports exhausted credits, is cancelled rather than left waiting to resume and spend later.
- A spend cap (`--budget`, default $400, read from the daemon's own ledgers) is checked before the reader, the writer and every Remedy round. The Remedy loop has its own cap (`LOOP_BUDGET`, default $80).
- The Remedy loop keeps the best tree on its own: a round is kept only if it is a net improvement, or fixes its ticket while breaking only lower-priority areas; an earlier divergence from the original counts as a regression; reverted candidates stay as `roundN-candidate` tags; the areas already attempted persist in `remedy_attempted.txt` so a relaunch never pays twice for the same wall (`ATTEMPTED_RESET=1` clears it, `BASELINE_COMMIT=<sha>` restarts from a chosen commit).
- Exit codes: 0 acceptance passes; 1 still differs; 2 provider down or timeout; 3 budget; 4 preflight or build failure. `pipeline.status` and `remedy_loop.status` hold the same verdicts.

The game name is only a label. The reader gets the ROM as `rom/game.nes` in
its own workdir through the `rom_*` tools; the writer gets nothing but
`spec/behavioral_spec.json` seeded into its context.

Step by step, if you want to drive it yourself:

```bash
scripts/run_reader.sh mygame path/to/mygame.nes   # Clean Room A → workspace/mygame/reader/spec/
scripts/handoff.sh mygame                         # lint + schema check, copies ONLY the spec across
scripts/run_writer.sh mygame                      # Clean Room B → workspace/mygame/writer/
scripts/build_byorom.sh mygame path/to/mygame.nes # extract assets from YOUR rom + make → build/game.nes
scripts/smoke_realasset.sh mygame path/to/mygame.nes   # acceptance vs the original; files Remedy tickets
scripts/remedy_loop.sh mygame path/to/mygame.nes 4     # one Remedy run per ticket, guarded
scripts/compare_full.sh mygame path/to/mygame.nes      # strict PASS/FAIL table + side-by-side PNG
scripts/forge.sh mygame "remappable controls with an options screen"
```

Emulator helpers (`scripts/emu_*.sh`): screenshot, nametable and palette
dumps, per-frame sprite and RAM logs, sound-register traces, cycle profiles.
The same scripts are copied into each workdir as the agents' `rom_*` /
`build_*` observation tools.

## The clean-room barrier

| Side | Agent | Sees | Cannot |
|---|---|---|---|
| A (reader) | `agents/genesis-reader` | `rom/game.nes` through typed `rom_*` tools that wrap `nesrom` | run arbitrary shell, reach the network |
| — | `scripts/handoff.sh` | the spec JSON | copy anything but the spec; fails on `nesrom lint-spec` (addresses, mnemonics, byte dumps) or schema errors |
| B (writer, Remedy, Forge) | `agents/genesis-writer`, `agents/remedy`, `agents/forge` | `spec/behavioral_spec.json` plus the cc65 conventions | see the ROM or the manifest; shells run in a container (`docker/cc65.Dockerfile`) with only a C toolchain |

The spec describes behavior: state machines, timelines, physics constants,
palette values, tile placements, the sound engine's data grammar. It never
carries addresses, opcodes or byte runs, and the lint enforces that. Audit any
run with `lev context <run-id>`.

The writer ships a fixed board-support layer verbatim into every project
(`agents/genesis-writer/reference/`: `nes.h`, `nes_hw.c`, `nmi_shim.s`,
`vram_queue.c`, `vram_copy.s`, a host stub for tests). It is the part every
NES program shares and is not generated per game.

## Layout

- `agents/*/agent.leviath` — the seven blueprints (reader and its worker, writer and its worker, Remedy, Forge and its worker)
- `agents/*/tools/*.rhai` — typed tools: `rom_*` (reader) and `build_*` (writer side)
- `agents/*/validators/*.rhai` — output validators (barrier lint for the spec, build report check)
- `agents/*/reference/` — hardware reference in our own words (reader); C conventions, linker configs, board-support layer, a FamiTone2-format player written from the public format description (writer)
- `tools/nesrom/` — Rust CLI: header, disasm, reach, xrefs, entropy, chr, apu, vram, decode-try, extract, lint-spec (mappers 0/1/2/3/4; decoders rle/pb53/lz/rhai-script)
- `schemas/` — behavioral spec, asset manifest, Remedy report
- `scripts/` — pipeline, preflight, acceptance, Remedy loop, emulator helpers
- `tests/fixtures/` — a synthetic spec and manifest for `scripts/selftest.sh`
- `workspace/` — per-game working trees (git-ignored)

## Testing

```bash
(cd tools/nesrom && cargo test --release)   # synthetic ROMs always; golden tests run when NESROM_TEST_NROM / NESROM_TEST_MMC1 / NESROM_TEST_MMC1_CHR_DIR point at images you own
scripts/selftest.sh path/to/small_nrom.nes  # extractor + handoff + build plumbing without any agent run
lev validate agents/<name> --graph          # blueprint checks
```

## Status

The pipeline has completed end to end on a small NROM homebrew title
(MIT-licensed, FamiTone2 audio) with no hand edits to the generated code: the
rebuilt ROM's title, gameplay and game-over screens are identical to the
original (palettes, pattern tables, every tile and attribute row), every
sprite record matches at each checkpoint, and object motion matches through
the first collision. Two sub-visible differences remained when the run
stopped on budget: one sound-register write at the title music stop, and one
host test that only compiles with lenient flags. A full run costs on the order
of $230 with the default model mix; most of that is the reader and writer.

A second, larger NROM title (a vertical shooter with scrolling, enemy
waves and a FamiStudio-engine soundtrack, MIT-licensed) ran through the same
unattended pipeline with no hand edits: the title screen is identical, the
gameplay screen has the right court, ship, HUD and lives, but star and enemy
positions diverge because the spec did not pin the random-number generator,
the rebuilt ship dies earlier than the original, and the sound engine
differs from the first frame. Total $252. The run produced the guard's
severity rule and the sandbox process fixes described above; the next agent
work is pinning RNG behaviour and the FamiStudio data grammar in the reader.

Mappers beyond NROM, CHR-RAM, compressed tile data and the bank fan-out are
implemented in `nesrom` and the blueprints but have not yet been exercised by
a complete run.

## Local only

Nothing here talks to GitHub or any remote by default. Leviath's `publish`
stages are local output steps (`lev result`). Remedy commits to a local branch
and writes the PR text to `.remedy/pr-<n>.md`; Forge commits locally. The only
online action is `scripts/remedy_poll.sh --github`, which you must opt into,
and even then it only reads issues and adds a label.

## License and notices

This repository is released under the MIT License (see `LICENSE`).

- It contains no ROM images, no extracted game assets and no code taken from any game. Everything the agents produce about a specific game lives in your git-ignored `workspace/`.
- Reimplementing someone else's game may still be restricted by copyright, trademark or the game's own license, whatever the technique used. Use the pipeline on software you own the rights to, or whose license permits it; a copyleft license on the original carries over to the reimplementation. Consult counsel before publishing a reimplementation of a commercial title.
- "NES" and "Nintendo Entertainment System" are trademarks of Nintendo Co., Ltd. This project is not affiliated with or endorsed by Nintendo; the names are used only to identify the hardware platform targeted.
- Third-party software is used, not redistributed: cc65 (zlib license; the linker configurations in `agents/genesis-writer/reference/linker/` are adapted from its `nes.cfg`), FCEUX (GPL, as an external emulator process), Leviath, Docker. The FamiTone2 data format is described from its author's public documentation; the player in `reference/audio/` is an original implementation.
