/* nes_hw.c - the only translation unit that touches PPU/APU/controller/
 * mapper registers (FIXED FILE: copied verbatim from the writer reference;
 * never edited by the architecture stage or a worker. A game needing a
 * hardware role that is missing here adds it in src/nes_hw_extra.c).
 *
 * Every rule in c_conventions that took a debugging session to learn is
 * baked in: NMI stays enabled (hw_wait_vblank spins on the NMI flag), the
 * latch is reset with `bit $2002` (cc65 drops a volatile read), the copy
 * loops are assembly through a real zero-page pointer, the OAM page lives
 * in the OAM segment, the APU frame image is written in a fixed order.
 */
#include "nes.h"

#ifndef NES_HOST_TEST

#ifndef NES_MAPPER
#define NES_MAPPER 0
#endif

#pragma code-name   ("CODE")
#pragma rodata-name ("RODATA")
#pragma bss-name    ("BSS")

#define PPU_CTRL    (*(volatile uint8_t *)0x2000)
#define PPU_MASK    (*(volatile uint8_t *)0x2001)
#define PPU_STATUS  (*(volatile uint8_t *)0x2002)
#define PPU_OAMADDR (*(volatile uint8_t *)0x2003)
#define PPU_SCROLL  (*(volatile uint8_t *)0x2005)
#define PPU_ADDR    (*(volatile uint8_t *)0x2006)
#define PPU_DATA    (*(volatile uint8_t *)0x2007)
#define OAM_DMA     (*(volatile uint8_t *)0x4014)
#define JOY1        (*(volatile uint8_t *)0x4016)
#define JOY2        (*(volatile uint8_t *)0x4017)
#define APU_BASE    ((volatile uint8_t *)0x4000)

#pragma bss-name (push, "OAM")
uint8_t oam_shadow_buffer[256];
#pragma bss-name (pop)

static uint8_t ppu_ctrl_shadow;
static uint8_t ppu_mask_shadow;
uint8_t prg_bank_current;
uint8_t hw_apu_frame[HW_APU_FRAME_SIZE];

void hw_init(void)
{
    static uint8_t i;
    ppu_ctrl_shadow = 0x80;          /* NMI ON (see nes.h): never cleared */
    ppu_mask_shadow = 0x00;
    PPU_CTRL = ppu_ctrl_shadow;
    PPU_MASK = ppu_mask_shadow;
    for (i = 0; i < HW_APU_FRAME_SIZE; i++) hw_apu_frame[i] = 0;
    i = 0;
    do { oam_shadow_buffer[i] = 0xF8; i++; } while (i != 0);   /* all sprites hidden */
    prg_bank_current = 0;
}

void hw_wait_vblank(void)
{
    nmi_flag = 0;
    while (!nmi_flag) { }
}

void hw_render_enable(uint8_t bg, uint8_t spr)
{
    ppu_mask_shadow = (uint8_t)(ppu_mask_shadow & 0xE7u);
    if (bg)  ppu_mask_shadow = (uint8_t)(ppu_mask_shadow | 0x08u);
    if (spr) ppu_mask_shadow = (uint8_t)(ppu_mask_shadow | 0x10u);
    PPU_MASK = ppu_mask_shadow;
}

void hw_render_disable(void)
{
    ppu_mask_shadow = (uint8_t)(ppu_mask_shadow & 0xE7u);
    PPU_MASK = ppu_mask_shadow;
}

void hw_set_leftmost_columns(uint8_t show_bg, uint8_t show_spr)
{
    ppu_mask_shadow = (uint8_t)(ppu_mask_shadow & 0xF9u);
    if (show_bg)  ppu_mask_shadow = (uint8_t)(ppu_mask_shadow | 0x02u);
    if (show_spr) ppu_mask_shadow = (uint8_t)(ppu_mask_shadow | 0x04u);
    PPU_MASK = ppu_mask_shadow;
}

void hw_set_scroll(uint8_t x, uint8_t y, uint8_t nametable)
{
    ppu_ctrl_shadow = (uint8_t)((ppu_ctrl_shadow & 0xFCu) | (nametable & 0x03u));
    PPU_CTRL = ppu_ctrl_shadow;
    __asm__("bit $2002");
    PPU_SCROLL = x;
    PPU_SCROLL = y;
}

void hw_set_pattern_tables(uint8_t bg_table, uint8_t spr_table)
{
    ppu_ctrl_shadow = (uint8_t)(ppu_ctrl_shadow & 0xE7u);
    if (bg_table)  ppu_ctrl_shadow = (uint8_t)(ppu_ctrl_shadow | 0x10u);
    if (spr_table) ppu_ctrl_shadow = (uint8_t)(ppu_ctrl_shadow | 0x08u);
    PPU_CTRL = ppu_ctrl_shadow;
}

void hw_set_sprite_size(uint8_t tall)
{
    ppu_ctrl_shadow = (uint8_t)(ppu_ctrl_shadow & 0xDFu);
    if (tall) ppu_ctrl_shadow = (uint8_t)(ppu_ctrl_shadow | 0x20u);
    PPU_CTRL = ppu_ctrl_shadow;
}

void hw_vram_set_increment(uint8_t down)
{
    ppu_ctrl_shadow = (uint8_t)(ppu_ctrl_shadow & 0xFBu);
    if (down) ppu_ctrl_shadow = (uint8_t)(ppu_ctrl_shadow | 0x04u);
    PPU_CTRL = ppu_ctrl_shadow;
}

void hw_oam_dma(void)
{
    PPU_OAMADDR = 0;
    OAM_DMA = 0x02;                  /* OAMPAGE = $0200 in every linker config */
}

void hw_vram_addr(uint16_t addr)
{
    __asm__("bit $2002");
    PPU_ADDR = (uint8_t)(addr >> 8);
    PPU_ADDR = (uint8_t)(addr & 0xFFu);
}

void hw_vram_write(uint8_t v)
{
    PPU_DATA = v;
}

void hw_vram_fill(uint8_t v, uint16_t n)
{
    static uint16_t cnt;
    static uint8_t k, val;
    val = v; cnt = n;
    while (cnt >= 32u) {
        for (k = 0; k < 8u; ++k) { PPU_DATA = val; PPU_DATA = val; PPU_DATA = val; PPU_DATA = val; }
        cnt -= 32u;
    }
    for (k = 0; k < (uint8_t)cnt; ++k) { PPU_DATA = val; }
}

/* Zero-page source pointer for the assembly copy loops (vram_copy.s). It
 * must be a GLOBAL in the ZEROPAGE segment with #pragma zpsym AFTER the
 * declaration, or cc65 silently uses absolute addressing. */
#pragma bss-name (push, "ZEROPAGE")
const uint8_t *vram_src_zp;
#pragma bss-name (pop)
#pragma zpsym ("vram_src_zp")

uint8_t vram_copy_len;
uint8_t vram_copy_addr_hi, vram_copy_addr_lo;
void hw_vram_copy_run(void);
void hw_vram_write_entry(void);

void hw_vram_write_run(uint16_t addr, const uint8_t *src, uint8_t len)
{
    vram_copy_addr_hi = (uint8_t)(addr >> 8);
    vram_copy_addr_lo = (uint8_t)(addr & 0xFFu);
    vram_src_zp = src;
    vram_copy_len = len;
    hw_vram_write_entry();
}

void hw_vram_copy(const uint8_t *src, uint16_t n)
{
    static uint16_t cnt;
    vram_src_zp = src; cnt = n;
    while (cnt >= 255u) { vram_copy_len = 255; hw_vram_copy_run(); vram_src_zp += 255; cnt -= 255u; }
    if (cnt) { vram_copy_len = (uint8_t)cnt; hw_vram_copy_run(); }
}

void hw_palette_load(const uint8_t *pal32)
{
    hw_vram_addr(0x3F00);
    vram_src_zp = pal32; vram_copy_len = 32; hw_vram_copy_run();
}

uint8_t hw_read_pad(uint8_t port)
{
    static uint8_t result, i;
    volatile uint8_t *reg = (port == PAD_PORT_1) ? &JOY1 : &JOY2;
    JOY1 = 1;
    JOY1 = 0;
    result = 0;
    for (i = 0; i < 8; i++) {
        result = (uint8_t)((result << 1) | (uint8_t)(*reg & 0x01u));
    }
    return result;
}

void hw_apu_write(uint8_t reg, uint8_t v)
{
    APU_BASE[reg] = v;
}

uint8_t hw_apu_read(uint8_t reg)
{
    return APU_BASE[reg];
}

void hw_apu_flush_frame(void)
{
    APU_BASE[0x00] = hw_apu_frame[0];
    APU_BASE[0x02] = hw_apu_frame[1];
    if (hw_apu_frame[11]) APU_BASE[0x03] = hw_apu_frame[2];
    APU_BASE[0x04] = hw_apu_frame[3];
    APU_BASE[0x06] = hw_apu_frame[4];
    if (hw_apu_frame[12]) APU_BASE[0x07] = hw_apu_frame[5];
    APU_BASE[0x08] = hw_apu_frame[6];
    APU_BASE[0x0A] = hw_apu_frame[7];
    APU_BASE[0x0B] = hw_apu_frame[8];
    APU_BASE[0x0C] = hw_apu_frame[9];
    APU_BASE[0x0E] = hw_apu_frame[10];
}

/* ---- mappers ----------------------------------------------------------- */
#if NES_MAPPER == 2 || NES_MAPPER == 3
/* Identity table so the bus-conflict write puts the same value on both sides. */
static const uint8_t bank_table[32] = {
    0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31 };
#endif

void hw_set_prg_bank(uint8_t bank)
{
#if NES_MAPPER == 2
    prg_bank_current = bank;
    *(volatile uint8_t *)&bank_table[bank] = bank_table[bank];   /* write the value to a ROM byte holding it */
#elif NES_MAPPER == 1
    static uint8_t v, k;
    prg_bank_current = bank;
    v = bank;
    for (k = 0; k < 5; k++) { *(volatile uint8_t *)0xE000 = v; v >>= 1; }
#elif NES_MAPPER == 4
    prg_bank_current = bank;
    *(volatile uint8_t *)0x8000 = 6; *(volatile uint8_t *)0x8001 = bank;
#else
    (void)bank;
#endif
}

void hw_set_chr_bank(uint8_t slot, uint8_t bank)
{
#if NES_MAPPER == 3
    (void)slot;
    *(volatile uint8_t *)&bank_table[bank] = bank_table[bank];
#elif NES_MAPPER == 1
    static uint8_t v, k;
    v = bank;
    for (k = 0; k < 5; k++) { if (slot) *(volatile uint8_t *)0xC000 = v; else *(volatile uint8_t *)0xA000 = v; v >>= 1; }
#elif NES_MAPPER == 4
    *(volatile uint8_t *)0x8000 = slot; *(volatile uint8_t *)0x8001 = bank;
#else
    (void)slot; (void)bank;
#endif
}

void hw_set_mirroring(uint8_t mode)
{
#if NES_MAPPER == 1
    static uint8_t v, k;
    v = (uint8_t)((mode == 0) ? 0x0E : (mode == 1) ? 0x0F : (mode == 2) ? 0x0C : 0x0D);
    for (k = 0; k < 5; k++) { *(volatile uint8_t *)0x8000 = v; v >>= 1; }
#elif NES_MAPPER == 4
    *(volatile uint8_t *)0xA000 = (uint8_t)(mode == 1 ? 1 : 0);
#else
    (void)mode;
#endif
}

#endif /* NES_HOST_TEST */
