/* vram_queue.c - VRAM update queue (FIXED FILE: copied verbatim, never edited).
 * Record layout: [addr_hi | 0x80 if column] [addr_lo] [len] [len data bytes].
 * Writes go through hw_vram_write_run (assembly, ~45 + 16 cycles per byte),
 * so 48 data bytes per frame leave room for the scroll write and OAM DMA.
 */
#include "vram_queue.h"
#include "nes.h"

#ifndef NES_HOST_TEST
#pragma code-name   ("CODE")
#pragma rodata-name ("RODATA")
#pragma bss-name    ("BSS")
#endif

static uint8_t vq[VRAM_QUEUE_SIZE];
static uint8_t vq_len;        /* bytes used */
static uint8_t vq_read;       /* flush position within vq (records are consumed front to back) */
static uint8_t vq_spent;      /* data bytes written this NMI */

static uint8_t vq_put(uint16_t addr, const uint8_t *src, uint8_t value, uint8_t len, uint8_t column)
{
    static uint8_t i, w;
    if (len == 0u) return 1u;
    if ((uint8_t)(VRAM_QUEUE_SIZE - vq_len) < (uint8_t)(len + 3u)) return 0u;
    w = vq_len;
    vq[w++] = (uint8_t)(((addr >> 8) & 0x3Fu) | (column ? 0x80u : 0u));
    vq[w++] = (uint8_t)(addr & 0xFFu);
    vq[w++] = len;
    if (src) { for (i = 0; i < len; i++) vq[w++] = src[i]; }
    else     { for (i = 0; i < len; i++) vq[w++] = value; }
    vq_len = w;
    return 1u;
}

uint8_t vram_queue_add(uint16_t addr, const uint8_t *src, uint8_t len)        { return vq_put(addr, src, 0, len, 0); }
uint8_t vram_queue_add_column(uint16_t addr, const uint8_t *src, uint8_t len) { return vq_put(addr, src, 0, len, 1); }
uint8_t vram_queue_add_fill(uint16_t addr, uint8_t value, uint8_t len)        { return vq_put(addr, 0, value, len, 0); }
uint8_t vram_queue_space(void)   { return (uint8_t)(VRAM_QUEUE_SIZE - vq_len); }
uint8_t vram_queue_pending(void) { return (uint8_t)(vq_len != 0u); }
void vram_queue_clear(void)      { vq_len = 0; vq_read = 0; }

void vram_queue_flush(void)
{
    static uint8_t len, hi;
    static uint16_t addr;
    vq_spent = 0;
    while (vq_read < vq_len) {
        len = vq[vq_read + 2u];
        if (vq_spent != 0u && (uint8_t)(vq_spent + len) > VRAM_QUEUE_FLUSH_BUDGET) return;  /* rest next frame */
        hi = vq[vq_read];
        addr = (uint16_t)(((uint16_t)(hi & 0x3Fu) << 8) | vq[vq_read + 1u]);
        hw_vram_set_increment((uint8_t)(hi & 0x80u ? 1u : 0u));
        hw_vram_write_run(addr, &vq[vq_read + 3u], len);
        vq_read = (uint8_t)(vq_read + 3u + len);
        vq_spent = (uint8_t)(vq_spent + len);
    }
    hw_vram_set_increment(0);
    vq_len = 0; vq_read = 0;
}
