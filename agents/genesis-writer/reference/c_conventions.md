# cc65 C Conventions for NES Reimplementations (Writer Side)

Pinned context for the Genesis writer, Remedy and Forge. It says how C for this
target is written so that every module a worker produces compiles, links, fits,
and runs on the original hardware. The toolchain is **cc65 2.18** (C89 with a
few extensions), `ca65` for assembly, `ld65` for linking.

---

## 1. Build

```
cc65 -t nes -O -Oirs -I include -o build/foo.s src/foo.c    # C -> asm
ca65 -t nes -I include -o build/foo.o build/foo.s           # asm -> obj
ca65 -t nes -o build/crt0.o reference/nmi_shim.s            # startup/NMI shim
ld65 -C linker/<mapper>.cfg -o game.nes build/*.o nes.lib -m build/game.map
```

- Always pass `-t nes` to both `cc65` and `ca65` (sets character set and target
  defines).
- `-O -Oirs` is the usual optimisation set. Do not use `-Os` on code with
  inline asm.
- The project ships its own startup + vector file (`nmi_shim.s`). Because it
  exports `__STARTUP__` and the `VECTORS` segment, the library's own `crt0` is
  never pulled from `nes.lib`; the library still supplies the runtime helpers
  (multiply, shifts, `memcpy`, `zerobss`, `copydata`).
- The Makefile targets: `all` (ROM), `stub-assets` (§10), `test` (§11),
  `clean`. `make` must succeed with no ROM present, using stub assets.
- Emit a map file and check segment sizes after every link. A bank that
  overflows is a link error, not a runtime surprise.

## 2. Language subset

| Rule | Why |
|---|---|
| C89: declarations at the top of a block, `/* */` comments accepted (cc65 also accepts `//`), no VLAs, no designated initialisers, no `inline`. | cc65 is a C89 compiler. |
| Use `<stdint.h>` types: `uint8_t`, `int8_t`, `uint16_t`. Prefer 8-bit everywhere; a 16-bit operation costs several times more. | The CPU is 8-bit. |
| **No floating point.** Fixed point: 8.8 or 4.4 (`uint16_t` pos, `int8_t` vel with a subpixel byte). | No FPU, huge soft-float. |
| **No recursion.** | 256-byte hardware stack, slow parameter stack. |
| No `malloc`, no `printf`, no `<stdio.h>`. | Size and speed. |
| No `struct` passing by value. Pass pointers or use globals. | Copies through the parameter stack. |
| Avoid `int`; write `uint8_t`. Cast loop counters to `uint8_t`. | cc65 promotes to 16-bit eagerly; explicit 8-bit types keep it in registers. |
| Arrays of structs are slow to index; prefer **structure of arrays** (`enemy_x[8]`, `enemy_y[8]`, `enemy_state[8]`). | 8-bit indexed addressing on parallel arrays is one instruction. |
| Prefer `static` globals over locals in hot paths; locals live on the software stack. | Locals cost indexed pointer accesses. |
| `switch` on a `uint8_t` compiles to a jump table when dense; keep case values small and contiguous. | |
| Constants as `#define` or `enum`; `const` arrays go in `RODATA` (they must be initialised). | |
| No `volatile` except in `nes_hw.c`. | Registers are touched only there. |

`__fastcall__` (the default calling convention for cc65 2.18 C functions) passes
the last argument in A/X. Functions with one `uint8_t` argument are cheap;
declare hot helpers with a single 8-bit parameter or none.

## 3. Segments and bank placement

cc65 places code and data with pragmas. Every module starts with a block that
says where it lives; the architecture stage assigns banks, workers copy them.

```c
/* physics.c - bank 2 */
#pragma code-name   ("PRG2")      /* functions in this file */
#pragma rodata-name ("PRG2")      /* const tables in this file */
#pragma data-name   ("DATA")      /* initialised RAM (rare) */
#pragma bss-name    ("BSS")       /* zero-initialised RAM */
```

- Segment names match the linker config (§5): `PRG0`..`PRGn` for switchable
  banks, `CODE`/`RODATA` for the fixed bank, `BSS` for RAM, `ZEROPAGE` for
  zero page, `OAM` for the sprite shadow page.
- Hot, small, shared code (hardware layer, NMI-time work, bank switch helper,
  audio driver tick) goes in the **fixed bank**. Everything that calls across
  banks goes through the helper in §6.
- Zero page: `#pragma bss-name ("ZEROPAGE")` before declaring a handful of
  the hottest variables (frame flags, camera, pointers used for indirect
  access). Budget ~200 bytes; the runtime reserves 26. Pointers used with
  `*ptr` in loops **must** be in zero page to get `(zp),y` addressing.
- `#pragma zpsym ("name")` tells the compiler an `extern` symbol is in zero
  page (used when the variable is declared in assembly).
- Put the `#pragma` lines *after* `#include`s so headers do not inherit them.
- Header files declare only; they contain no pragmas and no definitions.

Initialised RAM (`DATA`) costs ROM twice (load image + copy at boot); prefer
`const` tables in ROM plus explicit init code.

## 4. The hardware layer (`nes_hw.c` / `nes.h`)

**These two files are shipped, not written.** `reference/nes.h`,
`reference/nes_hw.c` and `reference/hw_nes_stub.c` are copied verbatim into
every project (with `nmi_shim.s` and `vram_copy.s`) and never edited; the
Makefile passes `-DNES_MAPPER=<n>`. Every rule below is already implemented
in them. What follows documents what they do.

`nes_hw.c` is the **only** translation unit that knows register addresses.
Everything else calls functions or uses macros from `include/nes.h` named by
role. When built with `-DNES_HOST_TEST` the same header maps to stubs (§11).

```c
/* nes.h - roles, not addresses */
void hw_init(void);                 /* warm-up, clear RAM, silence audio, rendering off */
void hw_wait_vblank(void);          /* spin until the NMI shim sets nmi_flag, then clear it */
void hw_render_enable(uint8_t bg, uint8_t spr);
void hw_render_disable(void);
void hw_set_scroll(uint8_t x, uint8_t y, uint8_t nametable);
void hw_oam_dma(void);              /* upload the OAM shadow page */
void hw_vram_addr(uint16_t addr);   /* only from NMI or with rendering off */
void hw_vram_write(uint8_t v);
void hw_vram_fill(uint8_t v, uint16_t n);
void hw_vram_copy(const uint8_t *src, uint16_t n);
void hw_palette_load(const uint8_t *pal32);
uint8_t hw_read_pad(uint8_t port);  /* strobe-and-shift, returns 8-bit mask */
void hw_apu_write(uint8_t reg, uint8_t v);   /* reg = 0..0x17 relative to APU base */
void hw_set_prg_bank(uint8_t bank);          /* §6 */
void hw_set_chr_bank(uint8_t slot, uint8_t bank);
void hw_set_mirroring(uint8_t mode);
```

Inside `nes_hw.c`:

```c
#define PPU_CTRL   (*(volatile uint8_t *)0x2000)
#define PPU_MASK   (*(volatile uint8_t *)0x2001)
#define PPU_STATUS (*(volatile uint8_t *)0x2002)
#define PPU_SCROLL (*(volatile uint8_t *)0x2005)
#define PPU_ADDR   (*(volatile uint8_t *)0x2006)
#define PPU_DATA   (*(volatile uint8_t *)0x2007)
#define OAM_DMA    (*(volatile uint8_t *)0x4014)
#define JOY1       (*(volatile uint8_t *)0x4016)
#define JOY2       (*(volatile uint8_t *)0x4017)
#define APU_BASE   ((volatile uint8_t *)0x4000)
```

Keep a shadow of write-only registers (`ppu_ctrl_shadow`, `ppu_mask_shadow`)
so the rest of the code can toggle single bits.

**NMI is already on when `main()` starts** (the shim's reset enables it after
the warm-up), and `hw_wait_vblank()` only works while it stays on: the shim's
NMI sets `nmi_flag`; nothing else does. So `hw_init()` initialises
`ppu_ctrl_shadow = 0x80` (NMI bit kept, add pattern-table/sprite-size bits as
the spec says) and never writes 0 to PPUCTRL. A boot that writes PPUCTRL = 0
and then waits for vblank hangs forever — the symptom is a build whose
observe output shows an all-zero palette, an empty nametable, no sprites and
only two PPU register writes in the whole trace. `nmi_enabled` (the C flag)
gates only the C `nmi_handler`; leave it 0 until the first screen is drawn.

Button mask convention for the whole project (fixed, regardless of what the
original used): bit 7 A, bit 6 B, bit 5 Select, bit 4 Start, bit 3 Up, bit 2
Down, bit 1 Left, bit 0 Right. `input.c` derives `pad_held`, `pad_pressed`
(`held & ~prev`) and `pad_released`.

## 5. Frame structure

The NMI shim (`nmi_shim.s`, assembled as the startup object) provides:

- `reset`: the standard warm-up, clears RAM, calls `zerobss`/`copydata`, sets
  the C parameter stack, jumps to `_main`.
- `nmi`: saves A/X/Y, increments `_frame_count`, sets `_nmi_flag`, and if
  `_nmi_enabled` is non-zero calls the C function `nmi_handler()`, restores
  registers, `RTI`.
- `irq`: calls `irq_handler()` if `_irq_enabled` (mapper scanline IRQs), else
  `RTI`.

`main.c` owns the loop:

```c
void main(void) {
    hw_init();
    game_init();                       /* palettes, tiles (CHR-RAM), first screen */
    nmi_enabled = 1;
    hw_render_enable(1, 1);
    for (;;) {
        hw_wait_vblank();              /* returns right after the NMI ran */
        input_update();
        game_update();                 /* logic; may enqueue VRAM updates */
        render_build_oam();            /* fill the OAM shadow */
    }
}

/* runs inside the NMI: ONLY uploads and timing-critical work, IN THIS ORDER */
void nmi_handler(void) {
    if (logic_busy) return;            /* lag frame: skip everything (see "Lag frames") */
    vram_queue_flush();                /* §7 — first: must finish inside vblank */
    hw_set_scroll(camera_x, camera_y, camera_nt);   /* address-then-scroll */
    hw_oam_dma();                      /* 513 cycles the PPU does not care about */
    audio_tick();                      /* §8 — register writes need no vblank */
}
```

Rules:

- The NMI body's VRAM writes must finish inside vblank (2,273 cycles from
  the NMI, see "Vblank budget"). Keep it to queue flush, scroll, DMA, audio
  tick, controller read (optional). No game logic.
- `nmi_handler` and everything it calls live in the **fixed bank**, or the
  bank switch in progress on the main thread will be observed mid-call.
- If the spec says the original ran logic inside the NMI (NMI-driven loop),
  still use this structure; the observable behaviour (one logic step per
  frame) is what matters.
- `logic_busy` is set at the top of `game_update` and cleared at the bottom,
  so an over-long frame skips uploads rather than corrupting them.

## 6. Bank switching

`hw_set_prg_bank()` keeps a shadow (`prg_bank_current`) and performs the
mapper-specific write. Cross-bank calls go through a trampoline pattern:

```c
/* game_state.c (fixed bank) */
void run_level_engine(void) {
    uint8_t saved = prg_bank_current;
    hw_set_prg_bank(BANK_LEVEL_ENGINE);
    level_engine_frame();              /* lives in PRG2 */
    hw_set_prg_bank(saved);
}
```

- Never call from a switchable bank into a *different* switchable bank
  directly. Call up into the fixed bank, which switches and calls down.
- Data accessed through pointers (level maps, tile blobs) must be in the bank
  that is mapped while it is read; the module that owns the data owns the
  switch.
- Interrupts: the NMI must not switch banks unless it restores them; audio
  data referenced from `audio_tick` therefore lives in the fixed bank or a
  bank the audio driver switches and restores itself.

Mapper specifics implemented only in `nes_hw.c`:

| Mapper | `hw_set_prg_bank` | Notes |
|---|---|---|
| NROM | no-op | |
| UxROM | write `bank` to `bank_table[bank]` (a `const uint8_t` identity table in the fixed bank) | avoids bus conflict |
| CNROM | `hw_set_chr_bank(0, bank)` same identity-table write | PRG fixed |
| MMC1 | reset bit then five serial writes of `bank` to the PRG register address; also `hw_set_mirroring` and CHR regs via the same 5-write sequence; shadow every register | write sequence must not be interrupted by an NMI that also writes MMC1 — set a `mapper_busy` flag or do all mapper writes from the main thread |
| MMC3 | write slot selector then bank data; keep shadows for all 8 registers; `hw_irq_scanline(n)` helper | fixed banks at top |

## 7. VRAM update queue

**Shipped, not written:** `reference/vram_queue.h` / `vram_queue.c` are
copied verbatim (`vram_queue_add`, `vram_queue_add_column`,
`vram_queue_add_fill`, `vram_queue_space`, `vram_queue_pending`,
`vram_queue_clear`, `vram_queue_flush`). What follows documents them.

Game logic never touches VRAM directly. It appends to a queue in RAM; the NMI
flushes it.

```c
/* renderer.c */
#define VRAM_QUEUE_SIZE 128
static uint8_t vq[VRAM_QUEUE_SIZE];     /* records: addr_hi, addr_lo, len, data... */
static uint8_t vq_len;
void vram_queue_add(uint16_t addr, const uint8_t *src, uint8_t len);
void vram_queue_add_column(uint16_t addr, const uint8_t *src, uint8_t len); /* +32 step */
void vram_queue_flush(void);            /* called from nmi_handler */
```

- The queue is bounded; the logic side checks `vram_queue_space()` and defers.
- Whole-screen loads (level start) are done with rendering **off**, streaming
  directly with `hw_vram_copy`, not through the queue.
- Column/row streaming for scrolling: 30-byte columns via the +32 increment
  mode, plus the 8 attribute bytes for that column when a 32-pixel boundary
  is crossed.

## 8. Audio driver

`audio.c` implements the sound engine from the spec's `sound_engine` grammar.

- `audio_init()`, `audio_play_music(uint8_t track)`, `audio_play_sfx(uint8_t id)`,
  `audio_stop(void)`, `audio_tick(void)`.
- `audio_tick` runs **once per frame from the NMI**, after uploads, and must be
  bounded (~300-600 cycles). It advances sequences and writes registers via
  `hw_apu_write`. Nothing else writes the APU.
- Music and SFX state are separate; the SFX priority policy from the spec
  decides which one owns a channel this frame; the driver restores the music
  channel state (period, volume) when an SFX ends.
- Note-to-period table: a `const uint16_t` table in ROM, one entry per
  semitone, generated by the architecture stage from the standard NTSC
  formula, unless the spec supplies specific values.
- Sequence data is an asset (`assets/music.bin` etc.) included via `assets.s`
  (§9); the driver walks it with zero-page pointers.
- The reference player in `reference/audio/` is for the FamiTone2 family
  only. When `sound_engine.driver_family` names another engine (FamiStudio,
  Pently, custom), implement that grammar from the spec — envelope kinds,
  release points, tempo model, DPCM sample table, effect streams — and treat
  `sound_engine.register_trace_head` as the unit test: a host test feeds the
  real data (or the spec's quoted bytes) through the driver and asserts the
  first frames' register writes match the table in order; on the real build
  `build_apu_trace` must match it too. "Writes registers every frame" is
  not evidence the engine is right.

## 8b. Random numbers

Every "random" choice in an NES game is a deterministic generator. Implement
the spec's `random_number_generator` system exactly: same state width, same
update rule, same seed, same reseed points, and the same number of calls per
frame from the same call sites in the same order (the generator's outputs are
a shared sequence; an extra call in one module shifts every later choice in
all of them). Put it in `rng.c` with `rng_seed()`, `rng_next()` returning
exactly what the spec says one call returns, and no other module may advance
it. Host test: seed as the spec says and assert the spec's `first_values`
list verbatim. Never "improve" the generator (a better polynomial is a
divergence from the original).

## 9. Assets

The repository contains **no game data**. Assets are extracted at build time
from the user's ROM into `assets/` (gitignored) by the extractor, using the
asset manifest. C code references assets only by identifier through
`include/assets.h`; the linkage is a generated assembly file.

```
; assets.s (generated by the architecture stage from the spec's asset list)
.segment "CHR"             ; CHR-ROM boards only
.incbin "assets/bg_tiles.chr"
.incbin "assets/sprite_tiles.chr"

.segment "PRG3"            ; data banks for CHR-RAM boards, levels, music
.export _asset_bg_tiles
_asset_bg_tiles: .incbin "assets/bg_tiles.bin"
.export _asset_level_maps
_asset_level_maps: .incbin "assets/level_maps.bin"
```

```c
/* assets.h */
extern const uint8_t asset_bg_tiles[];     /* 4096 bytes, 256 tiles */
extern const uint8_t asset_level_maps[];   /* runtime format "rle_v1" */
#define ASSET_BG_TILES_BANK 3
```

- Sizes in comments come from the spec (`tile_count`, `byte_count`,
  `dimensions`); they are the contract with `make stub-assets`.
- If the spec says a blob is in a runtime-decoded format (e.g. an RLE
  variant), `levels.c`/`renderer.c` implement the decoder from the spec's
  grammar and the asset is included raw.
- Assets never carry addresses or bank numbers from the original; the writer
  chooses banks freely.

## 10. `make stub-assets`

So the tree links without a ROM (in the writer sandbox, in CI, for tests), the
Makefile can synthesise placeholders:

```make
ASSETS := assets/bg_tiles.chr:4096 assets/sprite_tiles.chr:4096 assets/level_maps.bin:2048
stub-assets:
	@mkdir -p assets
	@for spec in $(ASSETS); do \
	  f=$${spec%%:*}; n=$${spec##*:}; \
	  [ -f $$f ] || head -c $$n /dev/zero > $$f; \
	done
```

The architecture stage writes the `ASSETS` list from the spec. Real extracted
files are never overwritten by stubs (`[ -f ]` guard). `make all` depends on
`stub-assets`.

## 11. Host-side unit tests

Pure logic (state machine, physics, input edge detection, runtime decoders,
sequence parsing) is tested on the host with gcc:

```
gcc -std=c89 -Wall -Wextra -Werror -Wimplicit-function-declaration -DNES_HOST_TEST -I include -I test test/test_physics.c src/physics.c test/hw_nes_stub.c test/hw_stub.c src/vram_queue.c -o build/test_physics && build/test_physics
```

- `include/nes.h` selects `test/hw_stub.c` implementations when
  `NES_HOST_TEST` is defined; the stub records register-level intentions in
  arrays the tests can inspect (`stub_scroll_x`, `stub_apu[0x18]`,
  `stub_vram[0x4000]`).
- Pragmas are wrapped: `#ifndef NES_HOST_TEST #pragma code-name(...) #endif`
  (gcc ignores unknown pragmas but warns; wrapping keeps output clean).
- Tests are plain C with an `ASSERT(cond)` macro that counts failures and a
  `main` that returns non-zero on any failure. No framework dependency.
- Host tests compile with `-Wall -Wextra -Werror -Wimplicit-function-declaration`
  in the Makefile: the sandbox's gcc treats a missing include as a warning,
  a user's clang treats it as an error, and a test suite that does not build
  on the user's machine is a defect. Every test file includes every header
  whose functions it calls.
- Every module worker adds at least one test for the behaviour the spec
  quantifies (constants, transitions, decoder round-trips on synthetic data).

## 12. Common cc65 pitfalls

1. **`int` arithmetic everywhere.** `a + b` on two `uint8_t` becomes 16-bit
   unless assigned back to `uint8_t`; use `(uint8_t)(a + b)` in expressions
   used as indices.
2. **Signed comparisons on `uint8_t`** silently promote; write `(int8_t)` casts
   for velocities.
3. **Locals cost.** A function with six locals spends most of its time
   shuffling the parameter stack. Use `static` file-scope variables.
4. **Pointer arithmetic on non-zero-page pointers** generates library calls.
   Copy the pointer into a zero-page pointer first.
5. **`const` missing** puts a table in RAM (`DATA`), doubling its footprint and
   overflowing RAM. Every lookup table is `const`.
6. **Interrupts and 16-bit variables.** The NMI can fire between the two
   bytes of a 16-bit write; variables shared with `nmi_handler` are 8-bit, or
   guarded with a flag.
7. **Reading `PPU_STATUS` clears vblank** and the scroll latch; only the
   hardware layer reads it.
8. **`switch` with sparse cases** becomes a compare chain. Renumber states to
   0..N.
9. **`memset`/`memcpy` on tiny sizes** are slower than a loop; use them only
   for ≥ 32 bytes.
10. **Stack depth.** Deeply nested calls plus interrupts can exhaust the 256
    byte hardware stack; keep call depth ≤ 6 and never `JSR` from the NMI
    into deep code.
11. **Bank confusion.** A `const` table used from bank A but placed (by the
    file's pragma) in bank B reads garbage. Data goes in the bank of its
    user, or in the fixed bank.
12. **Uninitialised `BSS`.** `zerobss` runs at reset only; re-entering a state
    needs explicit re-initialisation.
13. **`char` is unsigned** in cc65 by default; do not rely on `char` being
    signed.
14. **Bit fields** are supported but slow and large; use masks.
15. **Boot hang.** PPUCTRL written with the NMI bit clear followed by
    `hw_wait_vblank()` never returns (see §4). Observe output: zero palette,
    blank screen, no sprites. Fix the shadow, not the wait.


## NMI handlers written in C (mandatory pattern)

`nmi_shim.s` saves A/X/Y **and cc65's zero-page workspace** (`sp`, `sreg`,
`regsave`, `regbank`, `tmp1-4`, `ptr1-4`, 26 bytes via `zpspace`) before
calling `nmi_handler()` and restores them afterwards. Without that, a C
handler silently corrupts whatever main-thread C expression was interrupted
(symptom: queue entries with wrong addresses/values once every few frames,
screens that only become correct after a repaint). Keep the handler short
(drain a bounded number of queued writes, set scroll, tick audio) and never
call anything that allocates or uses the heap from it. Main-thread code that
touches PPU registers directly must set `nmi_enabled = 0` around the access
so the flush cannot interleave with it, and `hw_vram_addr()` must read
PPU_STATUS first to reset the address latch.

`hw_wait_vblank()` must be clear-then-wait (`nmi_flag = 0; while (!nmi_flag);`).
Wait-then-clear returns immediately whenever an NMI already fired during the
caller's work, and the following PPU writes then land mid-frame (symptom:
stray palette/tile bytes scattered over the lower nametable rows).

cc65 silently drops `(void)PPU_STATUS;` even though the pointer is volatile.
Reset the address latch with `__asm__("bit $2002");` (guarded by `#ifdef __CC65__`)
and check the generated `.s` for the `$2002` access when in doubt.

End the frame handler with the address-then-scroll pair: after the VRAM
queue drain write PPUADDR = nametable base ($2000 + table*0x400) as two
writes, then PPUSCROLL x and y. Leaving PPUADDR at the last queued write makes
the PPU re-emit that tile at the next address (visible as a duplicated tile
after every batch).

## Draining the VRAM queue fast enough

Per-entry C function calls cost ~350 cycles in cc65, so five entries fill
vblank and the last write lands as rendering restarts (symptom: the last tile
of every batch duplicated at the next address). Drain with one loop in
`nes_hw.c` that indexes the queue's **global** arrays with a `static uint8_t`
index (`ldy idx / lda arr,y`), never through pointer parameters, and reset the
latch with `bit $2002` per entry. Budget ~60 cycles per entry; 16-24 entries
per frame is safe. Send the boot palette through the same queue instead of
writing it directly after waiting for vblank (the handler's drain already used
part of vblank). Hide sprites once (OAM persists), not every pass.


## Ownership of shared variables

Every RAM variable that more than one module touches is DEFINED in exactly
one place — `src/game_state.c` — and declared `extern` in `include/game.h`.
A module never defines a variable another header declares, and never
declares a second copy of another module's state; it includes `game.h`. Two
definitions link on the NES (cc65 merges them silently into two different
addresses) and break every host test with `multiple definition` errors, and
the fix-up costs an integrate round. Module-private state is `static`.

## Vblank budget (why screens garble)

Vblank is 2,273 CPU cycles from the NMI. Everything that writes PPUADDR /
PPUDATA must be done by then; the pre-render line reloads the PPU's address
from the last PPUADDR value, so writes that spill past vblank land at the
wrong place. Symptoms: a text row whose tail restarts at its own first
column ("tinue.An" instead of "Press A to continue."), a tile duplicated at
the next address, or later queue entries missing while earlier ones land.

Budget every frame like this:

| item | cycles |
|---|---|
| NMI entry + shim (register + 26-byte zero-page save) | ~400 |
| each queue entry via `hw_vram_write_entry` (asm) | 45 + 16 × bytes |
| the same via C calls (`hw_vram_addr` + per-byte loop) | 350 + 40..100 × bytes — never in the NMI |
| OAM DMA | 513 (**not shown by FCEUX's cycle counter**; add it by hand) |
| scroll writes | ~50 |

So: VRAM writes first, then scroll, then DMA, then audio. Drain at most
~48 data bytes per frame from the queue (the flush keeps a running byte
count and stops before the entry that would exceed it; the rest goes next
frame). Whole-screen draws (title, level start, game-over screen) are done
with rendering off (`nmi_enabled = 0`, `hw_render_disable()`), in ONE pass,
with the unrolled `hw_vram_fill` / the asm copy — never through the queue.

`reference/vram_copy.s` (copy to `src/vram_copy.s`) provides
`hw_vram_copy_run` and `hw_vram_write_entry`. The C glue needs a zero-page
source pointer: declare it as a **global** inside
`#pragma bss-name (push,"ZEROPAGE")` and put `#pragma zpsym ("vram_src_zp")`
*after* the declaration (zpsym on a symbol declared later, or on a static,
is silently ignored and the copy loop reads garbage).

## Frame-exact behaviour (timelines)

The spec's `timelines` say on which frame, relative to an input, an object
first appears, where it is on each of the following frames, how its
attribute bytes cycle, how long it rests and where. Reproduce them exactly:

- State transitions happen in the frame that detects the input: when the
  update changes state, dispatch the new state in the SAME frame (loop the
  dispatcher, bounded to 3 passes) unless the timeline shows an idle frame.
- A screen draw that must precede the new state (clear + palette + full
  nametable) runs with rendering off inside that same frame; measure it —
  a 1 KB clear plus a 1 KB copy with the asm routines is ~35k cycles, i.e.
  more than one frame. If the timeline shows the original also lost a frame
  there ("lag frame"), that is fine; otherwise split the draw across the
  preceding frames or draw only what changed.
- Every reset/spawn/respawn reloads EVERY variable the spec lists for it:
  positions, speeds, direction flags, spin/animation counters. A reset that
  forgets a direction flag sends the object the wrong way and fires the wrong
  sound effect on the very next frame.
- Sprite animation cycles (attribute flip bits, tile swaps) are driven by a
  counter that advances once per game update — including while the object
  rests, if the timeline shows the cycle continuing — with the phase the
  timeline gives.
- OAM slot order is behaviour (the timeline lists slots): fill the shadow in
  that order. States whose timeline shows no sprites (title screens) hide
  all sprites (`y = 0xF0+`) rather than leaving the template in place.
- Verify with `build_oamlog` over the same frame range as the spec's
  timeline; a one-frame lag or a two-pixel offset is a bug.

## Lag frames and the sound tick

`logic_busy` skips the whole NMI body, including `audio_tick()`, when the
main loop overran — the same thing the original does when its driver ticks
from the frame interrupt (the spec's `tick_behavior` says where the original
ticks; follow it: NMI after uploads, or main loop after logic). The spec's
timelines mark the original's lag frames; matching them is not required,
but never introduce extra ones: measure the transition frames with the
emulator (`build_oamlog` shows a lag frame as a frame with no sprite
change).


## Inputs named in the spec

A transition the spec attributes to "start/A", "A or Start", "any button"
means EVERY listed button triggers it; implement the test as a mask of all
of them and cover each button in the host test. Verify presses the button
the acceptance observations name — usually Start — so a module that only
listens for A fails the real-build smoke test while passing its own test.

## Bounded data walkers (sequencers, decoders, tilemap strips)

Any loop that walks data — a music/sfx sequencer processing control bytes
until the next note, a decompressor, a tilemap strip reader, a pointer-table
follower — is bounded by an explicit iteration cap, never by a terminator
byte alone. Real data contains loop/jump opcodes that a naive walker follows
forever inside one tick; with zero-filled stub assets that never happens, so
the sandbox cannot catch it, and on the real build the NMI never returns:
the screen freezes with the last frame drawn, the controller reads zero,
`build_profile` shows the driver taking ~97% of every frame.
- Per tick, a channel processes at most N inline control bytes (N = 16 or
  the spec's stated maximum chain, whichever is larger); at the cap it
  stops the channel (silence, pointer parked) rather than spinning.
- Loop/jump targets are range-checked against the asset's size; an
  out-of-range pointer parks the channel.
- The module's host test FUZZES the walker: fill the data array with
  pseudo-random bytes (a fixed LCG seed), run 2,000 ticks, and assert every
  tick returned and touched at most N bytes. This test is mandatory for
  audio drivers and runtime decoders.

## Per-frame CPU budget

One NTSC frame is 29,780 CPU cycles, and the NMI (uploads, DMA, sound tick)
takes 2,000-6,000 of them. The main-loop logic therefore has ~20,000 cycles.
When it takes more, `logic_busy` is still set at the next NMI, that frame's
uploads and sound are skipped and the object positions repeat for a frame
(visible in `build_oamlog` as advance, advance, repeat). Rules:

- One-shot setup routines ("draw the title once", "init sound once") must
  return in a few cycles after their first run: test one flag and return.
- Never redraw a whole screen, re-upload a palette or rebuild a full OAM
  page from a template every frame; only what changed.
- Sound drivers: ~1,000-3,000 cycles per frame is normal; a row with new
  notes on every channel may reach 4,000. Anything higher is a data-walking
  bug (re-parsing from the song start each frame).
- cc65 costs: a C function call with pointer parameters ~350 cycles, a
  16-bit multiply ~200, a `uint8_t` array index ~10. Structure-of-arrays,
  `static` locals, 8-bit types.
- Measure, never guess: `build_profile` attributes cycles per frame to
  routines from the map file.
