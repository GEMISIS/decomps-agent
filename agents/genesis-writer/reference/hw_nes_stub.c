/* hw_nes_stub.c - host-side implementation of nes.h for gcc unit tests
 * (FIXED FILE: copied verbatim to test/hw_nes_stub.c). Records every
 * register-level intention in arrays a test can inspect. Project-specific
 * doubles (asset arrays, queue stubs) go in test/hw_stub.c, not here. */
#include "nes.h"
#include <string.h>

volatile uint8_t nmi_flag; volatile uint8_t frame_count; uint8_t nmi_enabled; uint8_t irq_enabled;
uint8_t oam_shadow_buffer[256];
uint8_t hw_apu_frame[HW_APU_FRAME_SIZE];
uint8_t prg_bank_current;

uint8_t  stub_vram[0x4000];        /* everything written through PPUDATA */
uint16_t stub_vram_addr;
uint8_t  stub_vram_inc_down;
uint8_t  stub_scroll_x, stub_scroll_y, stub_scroll_nt;
uint8_t  stub_render_bg, stub_render_spr;
uint8_t  stub_pattern_bg, stub_pattern_spr, stub_sprite_tall;
uint8_t  stub_pad[2];              /* tests set these; hw_read_pad returns them */
uint8_t  stub_oam_dma_count;
uint16_t stub_vblank_waits;
#define STUB_APU_MAX 512
uint8_t  stub_apu_reg[STUB_APU_MAX], stub_apu_val[STUB_APU_MAX];
uint16_t stub_apu_calls;
uint8_t  stub_apu_status;
uint8_t  stub_chr_bank[8], stub_mirroring;

void stub_reset(void)
{
    memset(stub_vram, 0, sizeof(stub_vram)); stub_vram_addr = 0; stub_vram_inc_down = 0;
    stub_scroll_x = stub_scroll_y = stub_scroll_nt = 0; stub_render_bg = stub_render_spr = 0;
    stub_pad[0] = stub_pad[1] = 0; stub_oam_dma_count = 0; stub_vblank_waits = 0; stub_apu_calls = 0;
    memset(oam_shadow_buffer, 0xF8, sizeof(oam_shadow_buffer)); memset(hw_apu_frame, 0, sizeof(hw_apu_frame));
    nmi_flag = 0; frame_count = 0; nmi_enabled = 0; irq_enabled = 0; prg_bank_current = 0;
}

void hw_init(void) { stub_reset(); }
void hw_wait_vblank(void) { stub_vblank_waits++; frame_count++; }
void hw_render_enable(uint8_t bg, uint8_t spr) { stub_render_bg = bg; stub_render_spr = spr; }
void hw_render_disable(void) { stub_render_bg = 0; stub_render_spr = 0; }
void hw_set_leftmost_columns(uint8_t a, uint8_t b) { (void)a; (void)b; }
void hw_set_scroll(uint8_t x, uint8_t y, uint8_t nt) { stub_scroll_x = x; stub_scroll_y = y; stub_scroll_nt = nt; }
void hw_set_pattern_tables(uint8_t bg, uint8_t spr) { stub_pattern_bg = bg; stub_pattern_spr = spr; }
void hw_set_sprite_size(uint8_t tall) { stub_sprite_tall = tall; }
void hw_vram_set_increment(uint8_t down) { stub_vram_inc_down = down; }
void hw_oam_dma(void) { stub_oam_dma_count++; }
void hw_vram_addr(uint16_t addr) { stub_vram_addr = (uint16_t)(addr & 0x3FFFu); }
void hw_vram_write(uint8_t v) { stub_vram[stub_vram_addr] = v; stub_vram_addr = (uint16_t)((stub_vram_addr + (stub_vram_inc_down ? 32u : 1u)) & 0x3FFFu); }
void hw_vram_fill(uint8_t v, uint16_t n) { while (n--) hw_vram_write(v); }
void hw_vram_copy(const uint8_t *src, uint16_t n) { while (n--) hw_vram_write(*src++); }
void hw_vram_write_run(uint16_t addr, const uint8_t *src, uint8_t len) { hw_vram_addr(addr); hw_vram_copy(src, len); }
void hw_palette_load(const uint8_t *pal32) { hw_vram_addr(0x3F00); hw_vram_copy(pal32, 32); }
uint8_t hw_read_pad(uint8_t port) { return stub_pad[port & 1u]; }
void hw_apu_write(uint8_t reg, uint8_t v) { if (stub_apu_calls < STUB_APU_MAX) { stub_apu_reg[stub_apu_calls] = reg; stub_apu_val[stub_apu_calls] = v; } stub_apu_calls++; }
uint8_t hw_apu_read(uint8_t reg) { (void)reg; return stub_apu_status; }
void hw_apu_flush_frame(void)
{
    hw_apu_write(0x00, hw_apu_frame[0]); hw_apu_write(0x02, hw_apu_frame[1]);
    if (hw_apu_frame[11]) hw_apu_write(0x03, hw_apu_frame[2]);
    hw_apu_write(0x04, hw_apu_frame[3]); hw_apu_write(0x06, hw_apu_frame[4]);
    if (hw_apu_frame[12]) hw_apu_write(0x07, hw_apu_frame[5]);
    hw_apu_write(0x08, hw_apu_frame[6]); hw_apu_write(0x0A, hw_apu_frame[7]); hw_apu_write(0x0B, hw_apu_frame[8]);
    hw_apu_write(0x0C, hw_apu_frame[9]); hw_apu_write(0x0E, hw_apu_frame[10]);
}
void hw_set_prg_bank(uint8_t bank) { prg_bank_current = bank; }
void hw_set_chr_bank(uint8_t slot, uint8_t bank) { stub_chr_bank[slot & 7u] = bank; }
void hw_set_mirroring(uint8_t mode) { stub_mirroring = mode; }
