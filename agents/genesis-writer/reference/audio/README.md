# Audio references (writer side)

- `famitone2_player.c` — a cc65 C player for the FamiTone2 data format
  (public domain library by Shiru). Use it when the spec's `sound_engine`
  grammar matches: song header (song count, instrument/sample list pointers,
  song table with per-channel stream pointers and two 16-bit tempo words, PAL
  then NTSC), instrument records (6 bytes: volume, arpeggio, pitch envelope
  pointers), envelopes (value bytes, 0x7F repeat marker, loop offset),
  channel streams (note/rest/duration/instrument/reference/loop opcodes), a
  sound-effect pointer list of (NTSC, PAL) pairs and effect streams that set
  raw register images per frame. Verified against an emulator trace.
- `famitone2_layout_template.h` — the relocation constants the player needs
  (where each extracted block originally lived; pointers inside the data are
  absolute). Fill from the manifest/spec and copy to `src/audio_data_layout.h`.
- `test_audio_example.c` — the SHAPE of the host test (init writes, first
  frames' register images, stop behaviour, one full effect); every value is
  a placeholder to be filled from the spec's sound_engine section.

Hardware layer glue expected by the player (add to `nes.h`/`nes_hw.c`):

```c
#define HW_APU_FRAME_SIZE 13u
extern uint8_t hw_apu_frame[HW_APU_FRAME_SIZE];   /* [0..10] register image, [11],[12] "pulse hi changed" flags */
void hw_apu_flush_frame(void);                     /* writes the image in the driver's fixed order */
```

Cycle cost measured: ~1200 cycles per frame while music plays, ~300 when
silent, up to ~4300 on a row that starts notes on four channels. That fits
in the NMI after uploads for a 2-3 entry queue; if the queue is large, tick
from the main loop instead (the spec's `tick_behavior` decides).
