# NES Hardware Reference (Reader Side)

This document is the pinned hardware context for the Genesis reader. It describes
the console the way a programmer thinks about it: what each part *does*, what
the software has to do to use it, and what timing rules constrain it. It is
written from public, community-documented knowledge of the hardware (the 6502
family CPU, the picture processor, the audio unit, the cartridge mappers). It
contains nothing about any particular game.

Two audiences read it:

1. **You, while tracing.** Use the addresses here to recognise what a piece of
   code is doing when it touches memory-mapped I/O.
2. **Nobody on the writer side.** The writer never sees this file. Anything you
   put in the behavioral spec must describe hardware use by *role*, not by
   address. See the final section, "Describing hardware use behaviorally".

---

## 1. CPU memory map, by role

The CPU is a 6502 derivative (no decimal mode) with a 16-bit address bus. The
cartridge, the picture processor (PPU), the audio unit (APU), and the controller
ports are all memory-mapped.

| Range | Role | Notes |
|---|---|---|
| `$0000-$00FF` | Zero page | Fast 8-bit-addressed RAM. Games keep their hottest variables and 16-bit pointers here (indirect addressing modes need a zero-page pointer pair). |
| `$0100-$01FF` | Hardware stack | Grows downward from `$01FF`. Subroutine return addresses and pushed registers live here. |
| `$0200-$07FF` | General RAM | 1.5 KB. One 256-byte page is almost always dedicated to a sprite (OAM) shadow buffer that gets DMA'd every frame. The rest holds game state, buffers, queues. |
| `$0800-$1FFF` | Mirrors of `$0000-$07FF` | Same 2 KB repeated. Treat an access here as an access to the low copy. |
| `$2000-$2007` | PPU registers | Eight registers; see §2. |
| `$2008-$3FFF` | Mirrors of `$2000-$2007` | Every 8 bytes. |
| `$4000-$4013` | APU channel registers | Pulse 1, pulse 2, triangle, noise, DMC; see §6. |
| `$4014` | OAM DMA trigger | Writing a page number copies that 256-byte RAM page to sprite memory. |
| `$4015` | APU status / channel enable | Write: enable channels. Read: which channels are still playing, plus interrupt flags. |
| `$4016` | Controller port 1 (write = strobe both ports) | See §3. |
| `$4017` | Controller port 2 (read) / APU frame counter (write) | Dual role. |
| `$4018-$401F` | Test/unused | Ignore. |
| `$4020-$5FFF` | Expansion area | Rarely used by the mappers in scope. Some mappers put registers here. |
| `$6000-$7FFF` | Cartridge RAM (PRG-RAM / WRAM) | 8 KB window. Battery-backed on save-game carts. Absent on plain NROM boards. |
| `$8000-$FFFF` | Cartridge program ROM (PRG) | 32 KB window. What appears here depends on the mapper (§8). The last six bytes are the interrupt vectors. |

### Interrupt vectors (always at the top of the CPU address space)

| Address | Vector | Fires when |
|---|---|---|
| `$FFFA-$FFFB` | NMI | The PPU enters vertical blank (if NMI is enabled). This is the frame heartbeat. |
| `$FFFC-$FFFD` | RESET | Power-on / reset button. Entry point of the program. |
| `$FFFE-$FFFF` | IRQ / BRK | Mapper scanline counters (MMC3), APU frame/DMC interrupts, or the `BRK` instruction. |

For a bank-switched cartridge the vectors sit in whichever bank is *fixed* at
the top of the window, so the reset and NMI handlers are always reachable.

### Typical reset sequence (what you should expect to see first)

Disable interrupts, clear decimal flag, set the stack pointer, disable NMI and
rendering, wait for the PPU to warm up (two vblank waits), clear RAM, possibly
initialise the mapper, initialise the APU, load palettes and tiles, then enable
NMI and rendering and drop into a main loop that waits for the NMI flag each
frame.

---

## 2. PPU registers (`$2000-$2007`) and their behavioral roles

| Addr | Common name | Access | Role |
|---|---|---|---|
| `$2000` | Control | W | Master configuration: enables NMI at vblank (bit 7), chooses 8×8 or 8×16 sprites (bit 5), selects which pattern table backgrounds and sprites draw from (bits 4 and 3), chooses the VRAM address auto-increment step of 1 (across a row) or 32 (down a column) (bit 2), and selects the base nametable — effectively the high bits of the scroll position (bits 1-0). |
| `$2001` | Mask | W | Turns rendering on/off for background (bit 3) and sprites (bit 4), hides the leftmost 8 pixel column of each (bits 1-2), greyscale (bit 0), colour emphasis (bits 5-7). Writing zero here blanks the screen and makes VRAM safely writable at any time. |
| `$2002` | Status | R | Bit 7 = currently in vblank (reading clears it), bit 6 = sprite 0 hit, bit 5 = sprite overflow. **Reading it also resets the address/scroll write latch**, so code reads it before writing `$2005`/`$2006` pairs. |
| `$2003` | OAM address | W | Selects which sprite-memory byte `$2004` accesses. Almost always written as 0 before a DMA. |
| `$2004` | OAM data | R/W | Byte access to sprite memory. Slow; games use DMA instead. |
| `$2005` | Scroll | W×2 | First write = horizontal scroll (fine X), second = vertical scroll. Combined with the nametable bits in `$2000` this places the camera. |
| `$2006` | VRAM address | W×2 | High byte then low byte of the PPU address to access next. Shares the latch with `$2005`. |
| `$2007` | VRAM data | R/W | Reads/writes the byte at the current VRAM address, then auto-increments by 1 or 32. Reads are buffered (first read after setting an address returns stale data, except for palette RAM). |

### Attribute table geometry (a frequent mistake)

The 64 attribute bytes at the end of each nametable form an 8x8 grid. Byte N (0-63) sits at attribute row N/8 and column N%8; attribute row R covers **tile rows 4R..4R+3** and attribute column C covers tile columns 4C..4C+3. Within a byte, bits 0-1 pick the palette of the top-left 2x2 tile block, bits 2-3 top-right, 4-5 bottom-left, 6-7 bottom-right. So bytes 32-47 (attribute rows 4-5) colour tile rows 16-23, not 8-15. `rom_observe` prints the table one attribute row per line with the tile rows it covers.

### The two-write latch

`$2005` and `$2006` share a single toggle. Reading `$2002` resets it. A routine
that reads `$2002`, writes `$2006` twice, then streams to `$2007` is *uploading
data to video memory*. A routine that writes `$2005` twice (and maybe `$2000`)
late in the NMI is *setting the camera for the next frame*. Writing `$2006` also
clobbers the scroll position, which is why scroll is set *after* uploads.

### What may only happen during vertical blank (or with rendering off)

- Any `$2007` read/write and any `$2006` address setting (VRAM is busy while
  rendering).
- OAM DMA (`$4014`).
- Palette changes.
- Turning rendering on with a consistent scroll.

Vblank on NTSC lasts about 2,270 CPU cycles (~20 scanlines). That is the
budget for all video uploads per frame, which is why games queue changes in RAM
during the frame and flush them in the NMI handler.

---

## 3. Controller protocol (strobe and shift)

Standard controllers are serial shift registers.

1. Write `1` then `0` to `$4016`. The rising edge latches the current button
   state into both controllers; the falling edge starts shifting.
2. Read `$4016` eight times for controller 1 (and `$4017` for controller 2).
   Bit 0 of each read is one button, in this order: **A, B, Select, Start, Up,
   Down, Left, Right**. The other bits are open bus / expansion and are masked.
3. Code typically rotates each bit into an accumulator byte so the result is an
   8-bit button mask with A in bit 7 (if shifting left) or bit 0 (if shifting
   right) — note which convention the game uses.

Games usually keep both the *current* mask and a *newly pressed* mask
(`current & ~previous`) so menus respond to presses, not holds. Some also
implement auto-repeat with a countdown. Reads are done once per frame,
ordinarily either at the start of the NMI or at the top of the main loop. DMC
audio can corrupt controller reads; robust games read twice and compare.

---

## 4. PPU frame model

- NTSC: 262 scanlines per frame, ~60.1 frames per second. 240 visible lines, 1
  post-render line, 20 vblank lines, 1 pre-render line. PAL: 312 lines, 50 Hz,
  longer vblank.
- CPU:PPU clock ratio is 1:3 on NTSC. One scanline is ~113.67 CPU cycles.
- **NMI** fires at the start of vblank if enabled in `$2000`. The NMI handler is
  the frame tick: flush queued VRAM writes, DMA sprites, set scroll, update
  audio, read controllers, then return. The main loop does game logic and
  waits for a flag the NMI sets.
- Two loop styles exist: *NMI-driven* (all per-frame work inside the NMI, main
  loop idles) and *flag-driven* (NMI only sets a flag and does uploads; main
  loop runs logic then spins on the flag). Identify which one a game uses — it
  determines the frame structure of the reimplementation.
- If game logic runs long, the NMI still fires; games either guard with a
  "logic busy" flag (skipping uploads that frame, i.e. lag) or accept
  corruption.

### Scrolling and split screens

Scroll is a 9-bit X and 9-bit Y position over a 2×2 arrangement of 256×240
nametables (only two are physically present; mirroring maps the others, §5).
Writing scroll and nametable select at vblank sets the camera for the whole
frame. Writing them *mid-frame* changes the camera for the remaining scanlines,
which is how status bars are kept fixed while the playfield scrolls. Games time
this by:

- **Sprite 0 hit**: put sprite 0 at the split line, poll `$2002` bit 6 in a
  busy loop, then write the new scroll. Look for a loop that reads `$2002` and
  tests bit 6.
- **Mapper scanline IRQ** (MMC3 and others): program a line count, the IRQ
  handler changes scroll. Look for the IRQ vector doing scroll writes.
- **Cycle counting**: a timed delay loop after NMI. Fragile, rarer.

A mid-frame scroll change can only cleanly change X and the coarse nametable
select through `$2005`/`$2000`; a full X/Y reposition needs the
`$2006`/`$2005`/`$2005`/`$2006` four-write trick.

### OAM DMA

Writing a page number `N` to `$4014` copies RAM `$N00-$NFF` into sprite memory,
stalling the CPU ~513 cycles. Games keep a 256-byte OAM shadow (commonly at
`$0200` or `$0700`), rewrite it during the frame, and DMA it in the NMI. Unused
sprites are hidden by setting Y to `$F0` or higher (off screen).

---

## 5. PPU memory (VRAM) layout

| PPU addr | Contents |
|---|---|
| `$0000-$0FFF` | Pattern table 0 (256 tiles × 16 bytes) |
| `$1000-$1FFF` | Pattern table 1 (256 tiles) |
| `$2000-$23FF` | Nametable 0 (960 bytes of tile indices, 32×30) + attribute table (64 bytes) |
| `$2400-$27FF` | Nametable 1 + attributes |
| `$2800-$2BFF` | Nametable 2 + attributes (mirrored unless 4-screen) |
| `$2C00-$2FFF` | Nametable 3 + attributes (mirrored unless 4-screen) |
| `$3000-$3EFF` | Mirror of `$2000-$2EFF` |
| `$3F00-$3F1F` | Palette RAM: 4 background palettes then 4 sprite palettes, 4 entries each |
| `$3F20-$3FFF` | Palette mirrors |

### Tiles (pattern tables)

A tile is 8×8 pixels at 2 bits per pixel = 16 bytes: the first 8 bytes are bit
plane 0 (one byte per row), the next 8 are bit plane 1. Pixel colour index =
`plane1_bit << 1 | plane0_bit`. Colour 0 is transparent for sprites and the
shared backdrop for backgrounds. Tiles carry no colour of their own; the
palette is chosen per 16×16 area (background) or per sprite.

Pattern tables come either from **CHR-ROM** on the cartridge (fixed or
bank-switched; the CPU never touches the bytes) or from **CHR-RAM**, which the
game must fill by streaming bytes through `$2006`/`$2007` from PRG-ROM,
usually at boot and on level transitions, often via a decompressor. A loop that
sets `$2006` to an address below `$2000` and writes many bytes to `$2007` is a
CHR-RAM upload; the source PRG range it reads from is tile data (possibly
compressed).

### Nametables and attributes

A nametable is 32 columns × 30 rows of tile indices (960 bytes). The 64-byte
attribute table that follows assigns a 2-bit palette number to each 16×16 pixel
block; each attribute byte covers a 32×32 area (four 16×16 quadrants, 2 bits
each, bottom-right quadrant in the top bits). Games frequently address the
screen in **metatiles** (16×16 = 2×2 tiles with one palette) precisely because
that is the attribute granularity; expect level formats built on metatiles.

### Palettes

Entries are indices into the fixed 64-colour master palette. Entry 0 of every
palette mirrors the universal backdrop colour at `$3F00`. Sprite palettes live
at `$3F11-$3F1F` (entry 0 of each is unused/transparent). A palette upload is
a `$2006` = `$3F00` (or `$3F10`) followed by 16 or 32 `$2007` writes; the source
bytes are a palette asset.

### Mirroring

Only 2 KB of nametable RAM exists. **Horizontal mirroring** (nametables 0=1,
2=3, stacked vertically) suits vertical scrolling; **vertical mirroring**
(0=2, 1=3, side by side) suits horizontal scrolling. Some mappers switch
mirroring at run time (MMC1, MMC3) or offer single-screen modes (MMC1). A game
that scrolls horizontally with vertical mirroring streams new columns into the
nametable just off the right edge of the camera each frame — look for per-frame
uploads of 30-byte columns with the `$2000` increment-by-32 bit set.

---

## 6. Sprites (OAM)

- 64 sprites, 4 bytes each: Y (top minus 1), tile index, attributes, X.
- Attributes: bits 0-1 palette (sprite palettes 4-7), bit 5 priority (behind
  background), bit 6 horizontal flip, bit 7 vertical flip.
- 8×8 mode: tile index selects from the pattern table chosen in `$2000`.
  8×16 mode: bit 0 of the tile index selects the pattern table, the rest selects
  an even/odd tile pair stacked vertically.
- **8 sprites per scanline** hardware limit; extra ones vanish. Games rotate
  OAM order every frame ("flicker") so drops are shared rather than fixed.
- Sprite 0 is special only in that its overlap with an opaque background pixel
  sets the sprite-0-hit flag (used for split-screen timing).
- Larger game objects are assembled from several sprites using a metasprite
  table: a list of (dx, dy, tile, attr) records relative to an object origin,
  often terminated by a sentinel. Expect a routine that walks such a table and
  fills the OAM shadow.

---

## 7. APU (audio) — channels and register roles

Five channels. Each has 4 registers; the first byte of each group controls
volume/envelope/duty, the last two set the period (pitch) and length.

| Regs | Channel | Behavioral fields |
|---|---|---|
| `$4000-$4003` | Pulse 1 | Duty cycle (timbre, 4 choices), constant-volume vs envelope, volume/envelope rate, sweep (pitch slide up/down with period and shift), 11-bit period (pitch), length counter load (note duration). Writing the high period byte restarts the envelope and phase. |
| `$4004-$4007` | Pulse 2 | Same as pulse 1. |
| `$4008-$400B` | Triangle | Linear counter (duration control, no volume), 11-bit period; one octave lower than a pulse at the same period. Typically the bass line. |
| `$400C-$400F` | Noise | Volume/envelope, 4-bit period selecting one of 16 noise rates, "mode" bit for a metallic short-loop noise, length counter. Percussion. |
| `$4010-$4013` | DMC | Plays 1-bit delta-encoded samples from PRG-ROM (`$C000-$FFFF` window): sample rate select, IRQ enable, loop, direct 7-bit output level, sample address (`$C000 + addr*64`), sample length (`len*16 + 1`). Drum samples and voice. DMC fetches steal CPU cycles and can corrupt controller reads. |
| `$4015` | Status | Write: bit per channel enable (0-4). Disabling a channel silences it and zeroes its length counter. Read: channel active flags, frame IRQ, DMC IRQ. |
| `$4017` | Frame counter | Selects 4-step (~240 Hz, can raise IRQ) or 5-step (~192 Hz, no IRQ) sequencing of envelopes, sweeps and length counters. Bit 6 inhibits the frame IRQ. Reset code almost always writes `$40` here. |

### What a sound engine looks like in code

A per-frame routine (called from the NMI) that: advances one or more sequence
pointers per channel, decodes events (note on/off, volume, instrument, loop,
tempo), applies instruments/envelopes/arpeggios/vibrato from tables, resolves
note numbers to periods through a **period lookup table** (a ~64- or 96-entry
16-bit table of decreasing values), and finally writes the channel registers.
Sound effects usually pre-empt music on a channel with a priority rule and
restore it after. For the spec, describe: the channel usage, the *event
grammar* of the sequence data, tempo handling (ticks per row, frame counting),
instrument/envelope semantics, and the SFX priority policy — not the register
addresses.

---

## 8. Mappers in scope

The 32 KB PRG window and the 8 KB CHR space are too small for most games, so
cartridge hardware ("mappers") swaps banks in and out under software control.
Writes to the ROM address range (`$8000-$FFFF`) do not change ROM; they hit
mapper registers. **Any store to an address in `$8000-$FFFF` is a mapper
register write** — that is the signature to look for.

### Mapper 0 — NROM

No banking. PRG is 16 KB (mirrored into both halves, "NROM-128") or 32 KB
("NROM-256"). CHR is 8 KB ROM. Mirroring fixed by a solder pad (header flag).
CPU address maps directly to ROM offset: `offset = addr - $8000` (mod 16 KB for
NROM-128).

### Mapper 1 — MMC1 (SxROM)

Registers are written **serially**: five consecutive writes of one bit each
(bit 0 of the written value, least-significant first) to any address in
`$8000-$FFFF`; the *address of the fifth write* selects which of four internal
registers receives the assembled 5-bit value. Writing a value with bit 7 set
resets the shift register and forces PRG mode 3. Look for the idiom
`STA reg / LSR / STA reg / LSR / STA reg / LSR / STA reg / LSR / STA reg`.

| 5th-write addr | Register | Fields |
|---|---|---|
| `$8000-$9FFF` | Control | bits 0-1 mirroring (0 one-screen lower, 1 one-screen upper, 2 vertical, 3 horizontal); bits 2-3 PRG mode (0/1: 32 KB switch at `$8000`; 2: fix first bank at `$8000`, switch `$C000`; 3: fix **last** bank at `$C000`, switch `$8000`); bit 4 CHR mode (0: 8 KB, 1: two 4 KB) |
| `$A000-$BFFF` | CHR bank 0 | 4 KB bank at PPU `$0000` (or 8 KB in 8 KB mode; low bit ignored). On CHR-RAM boards these bits are often reused for PRG-RAM/256 KB PRG selection. |
| `$C000-$DFFF` | CHR bank 1 | 4 KB bank at PPU `$1000` (ignored in 8 KB mode) |
| `$E000-$FFFF` | PRG bank | bits 0-3 select the 16 KB bank; bit 4 disables PRG-RAM |

The overwhelmingly common configuration is PRG mode 3: the last 16 KB bank is
fixed at `$C000-$FFFF` (vectors, NMI, common routines, bank-switch trampoline)
and `$8000-$BFFF` is switchable. Games keep a **shadow copy of the current
bank** in RAM because the register is write-only; a routine that saves the
shadow, switches, calls, and restores is a cross-bank call trampoline. PRG-RAM
at `$6000-$7FFF` is often present and battery backed.

### Mapper 2 — UxROM

A single write anywhere in `$8000-$FFFF` selects the 16 KB bank at `$8000`;
the **last** 16 KB bank is fixed at `$C000`. CHR is 8 KB RAM filled by the
program. Because the write also reads the ROM byte at that address on real
hardware (bus conflict), games write to a location whose ROM contents already
equal the bank number (a small identity table `.byte 0,1,2,3...`). Look for
`TAX / STA table,X`. Mirroring is fixed.

### Mapper 3 — CNROM

PRG is 32 KB, no banking. A write anywhere in `$8000-$FFFF` selects one of up
to four 8 KB CHR-ROM banks (bits 0-1), with the same bus-conflict idiom as
UxROM. Used for tile-set swaps between levels or simple animation.

### Mapper 4 — MMC3 (TxROM)

8 KB PRG granularity, 1 KB CHR granularity, and a scanline counter.

| Addr (even/odd) | Register | Fields |
|---|---|---|
| `$8000` (even) | Bank select | bits 0-2: which of registers R0-R7 the next `$8001` write sets; bit 6 PRG mode; bit 7 CHR inversion (swap the 2 KB and 1 KB halves of the pattern tables) |
| `$8001` (odd) | Bank data | Value for the selected register. R0-R1: 2 KB CHR banks; R2-R5: 1 KB CHR banks; R6: 8 KB PRG at `$8000` (or `$C000` in mode 1); R7: 8 KB PRG at `$A000` |
| `$A000` (even) | Mirroring | bit 0: 0 vertical, 1 horizontal |
| `$A001` (odd) | PRG-RAM protect | enable/write-protect the `$6000` RAM |
| `$C000` (even) | IRQ latch | scanline count to reload |
| `$C001` (odd) | IRQ reload | request reload of the counter |
| `$E000` (even) | IRQ disable | also acknowledges a pending IRQ |
| `$E001` (odd) | IRQ enable | |

The last 8 KB bank is always fixed at `$E000-$FFFF`; the second-to-last is fixed
at either `$C000` or `$8000` depending on PRG mode. The IRQ counter decrements
once per scanline while rendering (driven by pattern-table address line A12),
so a game that writes a latch value N and enables the IRQ is asking for an
interrupt N+1 scanlines later — the standard split-screen and raster-effect
mechanism. Look for the IRQ handler writing `$2005`/`$2000` or swapping CHR
banks.

### Recognising bank state while tracing

Track, per traced path, the current contents of each switchable window. A
`JSR` into a switchable window is only meaningful together with the bank
selected at that moment. If the bank number comes from a register loaded from
a table (`LDA table,X`), the edge is *dynamic*: enumerate the table if it is
small, otherwise record the edge as unresolved with the table's identity so a
later pass can fan out on it. The fixed bank is always safe to trace first;
its trampolines tell you the calling convention every other bank uses.

---

## 9. Common data structures to expect in ROM

| Data | Recognisable by |
|---|---|
| Palette sets | Runs of 16 or 32 bytes with values `< $40`, `$0F`/`$0D` (black) recurring at every 4th position |
| Pattern tiles in PRG (CHR-RAM games) | 16-byte alignment, plane-pair structure, moderate entropy; or a decompressor's input (high entropy, then a `$2007` loop) |
| Nametables / screens | 960-byte or 1024-byte blocks of tile indices, often RLE-compressed |
| Metatile definitions | Tables of 4 bytes per entry (TL, TR, BL, BR tile) plus a parallel palette/collision byte table |
| Level maps | Column- or screen-major arrays of metatile indices, frequently RLE or dictionary compressed; object lists as (x, y, type) triples with a terminator |
| Jump tables | Pairs of low/high byte tables indexed by a state or object type, used via `JMP (ptr)` or the `PHA/PHA/RTS` trick (address minus 1) |
| Period table | 64-96 16-bit values, monotonically decreasing, first entries around `$07xx` |
| Text | Runs of bytes in a narrow range mapped to a font; often with a custom encoding and control codes |
| Pointer tables | Little-endian 16-bit values all in `$8000-$FFFF` |

Entropy hints: code ~5-6 bits/byte with characteristic opcode frequency;
compressed data near 8; tiles 3-6; sparse tables/palettes low.

---

## 10. Describing hardware use behaviorally (barrier rules)

Everything that crosses to the writer must be phrased in terms of *what the
game does*, never *what bytes it pokes*. The writer has its own hardware layer
and will map roles to registers on its own.

| Do not write | Write instead |
|---|---|
| "writes $80 to $2000" | "enables the vblank interrupt with 8×8 sprites, backgrounds from the second pattern table, sprites from the first" |
| "STA $2005 twice in NMI" | "sets the camera scroll for the next frame at the end of the frame handler" |
| "reads $4016 eight times" | "reads controller 1 with the standard strobe-and-shift protocol, producing an 8-bit button mask, A in the low bit" |
| "writes $02 to $4014" | "uploads the 256-byte sprite shadow buffer to sprite memory every frame via DMA" |
| "loop writing $2007 from $C400" | "uploads a 4 KB tile set to the background pattern table at boot (asset `bg_tiles`)" |
| "STA $E000 (MMC1 PRG reg)" | "switches the swappable program bank to the one holding the level engine before calling it, and restores the previous bank afterwards" |
| "$4000 = $3F, $4002/$4003 = period" | "plays a pulse note at full volume with a 12.5% duty; pitch from the note-to-period table" |
| "$0300 holds player X" | a RAM variable named `player_x_pos` (u8, subpixel companion `player_x_sub`) with its description |

Rules:

1. **No addresses, no bank numbers, no opcodes, no hex byte listings** in the
   spec's prose or identifiers. The barrier lint rejects them.
2. Name RAM variables by **role** (`camera_x`, `enemy_state[8]`), and describe
   type, size and meaning.
3. Describe data formats as **grammars** ("a run byte: high bit set means
   repeat the next byte `(n & $7F)` times ..." — note even `$7F` here is
   fine in the *manifest* but should be written as "127" or "the low 7 bits"
   in the spec).
4. Timing is described in **frames** and **scanlines**, not cycles, unless a
   cycle-exact behaviour is genuinely part of the gameplay.
5. Asset references use **IDs** (`bg_tiles`, `level_1_map`) plus their
   dimensions/counts. Where each asset lives in the ROM goes in the asset
   manifest, which stays on the reader side.
6. Numeric gameplay constants (gravity, speeds, timers, damage) are recorded
   as decimal values with units (pixels/frame, frames, subpixels).
7. When something is unknown or unresolved, say so explicitly in the spec
   rather than guessing; the writer can leave a hook, and Remedy can revisit.
