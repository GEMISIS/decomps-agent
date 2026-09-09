# Data-stage jobs (pinned for data_read / data_write)

Each lettered job below is one todo item. A read turn works on ONE job with ROM tools; the following write turn records what was learned.

A. DATA. Take every unreached range (rom_reach data_ranges, plus workers' data
items in `data_tables`) and classify it: pattern tables (CHR-ROM or the PRG
sources of CHR-RAM), palettes (runs of 4-byte groups with values below 64),
nametables/tilemaps, attribute tables, level layouts, metatile definitions,
text (look for a character mapping), lookup tables (sine, speed curves), music
sequences, compressed blobs. Use rom_entropy first, rom_bytes to look,
rom_decode_try to confirm a compression scheme. For each range context_append
to `data_tables` (key "<bank>:<start>", content: kind, length, format described
as a grammar in words, purpose, decoder name or "raw"). If you identify a
custom compression scheme, describe it in `compression` as a step-by-step
decoding grammar (control byte semantics, literal/repeat rules, terminators,
decoded size rule) and, if it is not rle/pb53/lz, write a decoder as
spec/decoders/<name>.rhai defining fn decode(bytes) that returns the decoded
bytes, then confirm it with rom_decode_try decoder="script:spec/decoders/<name>.rhai".

A1. FORMAT PROOF. Every data format you describe (sprite templates,
tilemap strips, text tables, song/instrument records, pointer lists) must be
proved before it goes in the spec: decode the first record BY HAND from
rom_bytes with your grammar and show that it equals something observed —
the OAM records rom_oamlog shows at the moment the template is loaded, the
tiles rom_observe shows at the rows/columns the strip is drawn to, the
register values rom_apu_trace shows for the first note. A format that does
not reproduce an observation is wrong; fix the grammar, not the observation.
A tilemap asset is named for the screen it PRODUCES, proved by drawing it in
your head at the placement the code uses and matching the observed grid of
that screen (the v4 spec called the title-screen block "game_screen_block"
and the writer uploaded the title over the court). A screen that the code
composes at runtime — border tiles in loops, text from constants, digits —
is NOT an asset: describe the composition (rows, columns, tile numbers,
character-to-tile mapping) as behavior in `video` / the system entry.
Every asset in the manifest must be located by a routine that READS it
(rom_xrefs on the pointer/table base): the manifest may not contain
"best-guess", "unconfirmed" or "not confirmed" locations — an asset you
cannot locate is not an asset, describe the data as behavior instead.

A2. VIDEO STATE. Call rom_observe (frames=120, then again with press="start"
or whatever the code's title screen waits for, and at a later frame) and
rom_vram, and record in `hardware_map` (context_write, merging) using the
OBSERVED values (the emulator shows the palette exactly as it ends up in
PPU RAM and the exact tile grid — trust it over static inference): every initial palette (all color values, AFTER any boot-time
overrides), what the boot code draws (text, borders, tile layout) with EXACT
placement — for EVERY drawn block (each strip of a tilemap asset, each text
string, each border run) give its top-left row and column and its width and
height in tiles as read off the nametable grid, never "roughly rows 4-13";
or the formula when computed at runtime (e.g. "column = (32 - text length)
/ 2, row = 15"); palette bands from the ATTRIBUTES lines, which are printed per
attribute row with the TILE ROWS they cover (attribute row N = tile rows
4N..4N+3; bytes 32-47 colour tile rows 16-23, never 8-15); and which tile numbers the border uses — which pattern table backgrounds and sprites use — copy rom_observe's "PPU CTRL=" line for EVERY screen state (title, gameplay, game over): the same tile ids drawn from the other pattern table are garbage, and this is the first thing the rebuilt title got wrong,
sprite size, scroll position, and how the screen is updated per frame.

A2b. RUNTIME TEXT AND NUMBERS. Anything the game writes to the screen at
runtime — score digits, HUD labels, prompts, win/lose words — must be
recorded with the exact row and column of every character and the TILE
INDEX used for each character/digit (from rom_observe at a frame where it is
visible, e.g. right after a point is scored; compare the tile grid before
and after). State the character-to-tile mapping as a rule ("digit d = tile
0x30+d", "letters use ASCII codes as tile indices") and prove it on two
examples. Without this the writer guesses positions and fonts.

A2c. LIMITS AND CLAMPS. Every clamp, threshold and extreme the systems
mention (player object top/bottom bounds, wall bounce rows, scoring edge, speed
caps) gets an observed value: rom_oamlog / rom_ramlog with `hold` (keep a
direction held for 100-200 frames) shows where a player-controlled object stops; a scoring
event shows the edge column. Write them as constants with the observed
numbers, never "clamp to screen".

A3. MOTION TIMELINES (frame-exact). Static reading never yields the frame a
thing first moves, the pixel it starts from, or how its sprite animates. For
every moving object and every state change, call rom_oamlog around the
event (e.g. from=495 to=560 press="start" press_at=500; then around the first
scoring/death/respawn event you saw in rom_observe) and record in
`hardware_map` under a heading TIMELINES: the runtime OAM slot order (which
slot holds which object — it often differs from any template in ROM), the
frame the object first appears and its position on that frame, its position
on each of the next ~8 frames (speed and direction on spawn), the
tile/attribute animation cycle (exact sequence of attribute bytes, period in
frames, phase relative to the event, whether it keeps cycling while the
object rests), rest positions and how many frames a pause lasts, and lag
frames (a frame where no sprite changed right after an event = the original
overran; note it, the writer reproduces the visible timing). Also log the
frames between the input and the next screen (title -> game) and state
exactly how many frames pass and what is visible on each. One rom_oamlog
call per event; keep ranges short.

A4. RANDOM NUMBERS (frame-exact). Anything the game places or chooses
"randomly" (star fields, enemy spawn columns and types, item drops, bounce
angles) is deterministic on the console: find the pseudo-random routine
(rom_xrefs on the RAM bytes the spawn/placement systems read, a routine built
from shift/rotate/exclusive-or/add chains on one to four RAM bytes) and
describe it as a DATA TRANSFORM, not as code: state width in bits, the exact
update rule in words (e.g. "16-bit linear feedback shift register, shift left
by one, feedback bit = bit 15 xor bit 13 xor bit 12 xor bit 10 into bit 0",
or "8-bit: value = value*5 + 1 modulo 256, then xor with the frame counter"),
how many bits or bytes one call returns and which part of the state, the
SEED at reset and every RESEED point (frame counter mixed in at the title,
controller bits, timer), and EVERY CALL SITE: which system calls it, how many
times per frame, and in what order. Then PROVE it with rom_ramlog: watch the
state bytes from reset through the first 32 frames of gameplay and list the
first 16 state values after the game starts (frame, value) in `hardware_map`
under RANDOM NUMBERS; that list becomes an acceptance table so the writer's
generator can be checked bit for bit. A spec without this section yields a
rebuild whose stars, spawns and bounces all differ from frame one.

B. SOUND ENGINE. First name the DRIVER FAMILY from its data layout and
call pattern (FamiTone2: five channel streams + instrument records + one
effect stream; FamiStudio engine: song table, per-channel note/instrument
envelopes with release points, arpeggio/pitch/duty envelopes, optional DPCM
sample table with address/length/pitch, tempo either per-song speed or a
tempo envelope, effect streams; Pently: pattern/instrument/drum tables; or a
custom driver) and then describe THAT family's grammar exactly — never assume
FamiTone2. Using rom_apu, rom_apu_trace and the routines already
catalogued, describe the audio driver behaviorally in `sound_engine`
(context_write): channels used, how often it ticks AND FROM WHERE (inside the
frame interrupt handler or the main loop — the trace tells: if writes stop on
exactly the frames where the main loop overran, the tick is skipped on lag
frames), the music sequence data grammar, how sound effects are triggered and
prioritized over music, tempo handling, and how tracks are selected. Check
every claim against the trace: list which songs/effects are ACTUALLY started
in the traced frames and when (a song that exists in the data but is never
played must be marked so); if the driver has region variants (NTSC/PAL
tables, tempo words per region) say which one the game uses and in which
order the variants are stored. THE GRAMMAR MUST BE EXACT AND
COMPLETE: every byte pattern (note, rest, duration, loop/repeat, call/return,
instrument/envelope, tempo, end) with its precise encoding — no "candidate"
or "possibly". Validate it: take the first 16 bytes of the title song from
rom_bytes, decode them by hand with your grammar, and check that the resulting
register values/timings match rom_apu_trace (frames 0-120). If they do not,
re-read the driver until they do. State the TICK TERMINATION RULE explicitly: which byte kinds consume a
frame (note, rest, duration) and which are processed inline (loop, jump,
instrument, tempo, effect), what bounds an inline chain (maximum control
bytes between two frame-consuming bytes in the actual data — count it), and
what the driver does at a loop/jump so a walker cannot spin forever. Also record: how many sound-register writes
the driver makes per frame while music plays (from the trace), the exact
register write ORDER per frame, which registers each sound effect touches,
and when music stops on its own (frame count after it started).
REGISTER TRACE HEAD: record, verbatim from rom_apu_trace, every sound
register write (register role and value) for the first 16 frames after each
music start (title, gameplay) and for the first 8 frames of each sound
effect, as `sound_engine.register_trace_head` — the writer validates its
driver against it frame by frame with build_apu_trace before it may claim
the build is good, so this table is the acceptance test for audio.
ASSET BOUNDARIES for sound data: derive them from the driver's pointer
tables (song list, instrument list, effect list), not from entropy guesses:
every pointer target must fall inside an extracted block, blocks must not
overlap, and each block's start must be the structure the grammar expects
(state what the first bytes of each block are: header, pointer list, stream).
Note the extracted-block base so absolute pointers inside the data can be
relocated (the manifest carries offsets; the spec only says "pointers are
absolute to the block's original location and must be rebased").

C0. INPUTS THAT TRIGGER TRANSITIONS. For every state transition an input
causes, list the exact buttons that work — verified by pressing each one
separately with rom_oamlog / rom_ramlog (press=start, then press=A, ...) at
two different frames (early and late in the state). Write "Start OR A" or
"Start only"; never "start/A" — a slash was read as one button and the
rebuilt game ignored Start.

C. ACCEPTANCE OBSERVATIONS. Using rom_observe (several frame counts, with and
without pressing the button the title waits for) and rom_apu_trace, write
8-15 concrete, checkable facts into `hardware_map` under a heading
ACCEPTANCE: e.g. "frame 120: title shows <text> on rows 4-7, palette bg0 =
[..]", "start pressed at frame 500 → by frame 700 the HUD row 2 reads
'Player 1: 0    Player 2: 0' and 7 sprites are visible", "with no input the
right player scores at about frame 650, 1150, ...; the match ends with LOSE/WIN
text by frame ~3200", "music: 9 sound-register writes per frame from frame 7",
and rom_observe's "VRAM WRITES" line for each observed screen ("by frame
120 the code has written 2816 nametable, 192 attribute and 32 palette
bytes") — an exact measure of how much each screen draws.
These become the writer's acceptance tests, together with the TIMELINES
(A3) which the spec carries verbatim as `timelines`. Add the driver's
routines to `functions` if missing. If the ROM has no audio code, write
"none" to `sound_engine`.


Never write addresses, mnemonics, or byte dumps into descriptions. Encodings
("bit 7 set means repeat", "values 0-95 are semitones above the lowest note")
are fine and necessary.

