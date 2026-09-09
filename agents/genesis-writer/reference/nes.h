/* nes.h - hardware abstraction API (FIXED FILE: copied verbatim from the
 * writer reference; never edited by the architecture stage or a worker).
 *
 * The only header that describes hardware roles. Register addresses live
 * solely in src/nes_hw.c. Under -DNES_HOST_TEST the same API is provided by
 * test/hw_nes_stub.c, which records intentions in arrays tests can inspect.
 */
#ifndef NES_H
#define NES_H

#include <stdint.h>

#define PAD_PORT_1 0
#define PAD_PORT_2 1

/* Button mask convention for the whole project, whatever the original used. */
#define PAD_A      0x80
#define PAD_B      0x40
#define PAD_SELECT 0x20
#define PAD_START  0x10
#define PAD_UP     0x08
#define PAD_DOWN   0x04
#define PAD_LEFT   0x02
#define PAD_RIGHT  0x01

/* Provided by nmi_shim.s (zero page) or test/hw_nes_stub.c. Declared first,
 * then marked zero-page: cc65's #pragma zpsym needs the extern to exist. */
extern volatile uint8_t nmi_flag;      /* set to 1 by every NMI */
extern volatile uint8_t frame_count;   /* increments every NMI (wraps) */
extern uint8_t nmi_enabled;            /* non-zero: the NMI calls nmi_handler() */
extern uint8_t irq_enabled;            /* non-zero: the IRQ calls irq_handler() */
#if !defined(NES_HOST_TEST) && defined(__CC65__)
#pragma zpsym ("nmi_flag")
#pragma zpsym ("frame_count")
#pragma zpsym ("nmi_enabled")
#pragma zpsym ("irq_enabled")
#endif

/* Implemented in main.c, called from the shim. */
void nmi_handler(void);
void irq_handler(void);

/* ---- boot / frame timing -------------------------------------------------
 * NMI is already ON when main() starts (the shim enables it after warm-up)
 * and hw_init keeps it on: hw_wait_vblank only ever returns because the NMI
 * sets nmi_flag. */
void hw_init(void);                 /* shadows, OAM page hidden, APU image cleared */
void hw_wait_vblank(void);          /* clear-then-wait on nmi_flag */

/* ---- PPU control ------------------------------------------------------- */
void hw_render_enable(uint8_t bg, uint8_t spr);
void hw_render_disable(void);
void hw_set_scroll(uint8_t x, uint8_t y, uint8_t nametable);   /* address-then-scroll tail of the NMI */
void hw_set_pattern_tables(uint8_t bg_table, uint8_t spr_table); /* 0 or 1 each */
void hw_set_sprite_size(uint8_t tall);                           /* 0 = 8x8, 1 = 8x16 */
void hw_set_leftmost_columns(uint8_t show_bg, uint8_t show_spr); /* leftmost 8 px */
void hw_oam_dma(void);              /* upload oam_shadow_buffer (513 cycles) */

/* 256-byte OAM shadow page (Y, tile, attr, X per sprite) in the OAM segment. */
extern uint8_t oam_shadow_buffer[256];

/* ---- VRAM access (rendering off, or from the NMI inside the budget) ----- */
void hw_vram_addr(uint16_t addr);   /* resets the latch first */
void hw_vram_write(uint8_t v);
void hw_vram_fill(uint8_t v, uint16_t n);
void hw_vram_copy(const uint8_t *src, uint16_t n);           /* asm, ~16 cycles/byte */
void hw_vram_write_run(uint16_t addr, const uint8_t *src, uint8_t len); /* one queue entry: ~45 + 16/byte */
void hw_vram_set_increment(uint8_t down);  /* 0 = +1 across, 1 = +32 down (columns) */
void hw_palette_load(const uint8_t *pal32); /* 32 bytes: 4 bg + 4 sprite sub-palettes */

/* ---- input ------------------------------------------------------------- */
uint8_t hw_read_pad(uint8_t port);  /* strobe-and-shift, PAD_* mask */

/* ---- audio ------------------------------------------------------------- */
void hw_apu_write(uint8_t reg, uint8_t v);   /* reg 0..0x17 relative to the APU base */
uint8_t hw_apu_read(uint8_t reg);             /* status read = 0x15 */
/* Per-frame register image for sound drivers that write a fixed set of
 * registers once per frame: [0]=$4000 [1]=$4002 [2]=$4003 [3]=$4004 [4]=$4006
 * [5]=$4007 [6]=$4008 [7]=$400A [8]=$400B [9]=$400C [10]=$400E; [11]/[12]
 * non-zero = also write the pulse 1/2 period MSB ($4003/$4007) this frame. */
#define HW_APU_FRAME_SIZE 13
extern uint8_t hw_apu_frame[HW_APU_FRAME_SIZE];
void hw_apu_flush_frame(void);

/* ---- mapper -------------------------------------------------------------
 * NES_MAPPER is defined by the Makefile (-DNES_MAPPER=0/2/3/1/4). NROM and
 * CNROM keep PRG fixed; the others switch the $8000 window. Shadows are kept
 * so callers can read prg_bank_current. */
extern uint8_t prg_bank_current;
void hw_set_prg_bank(uint8_t bank);
void hw_set_chr_bank(uint8_t slot, uint8_t bank);  /* slot 0/1 (4 KB halves) or MMC3 slot 0-5 */
void hw_set_mirroring(uint8_t mode);               /* 0 vertical, 1 horizontal, 2 one-screen low, 3 one-screen high */

#endif /* NES_H */
