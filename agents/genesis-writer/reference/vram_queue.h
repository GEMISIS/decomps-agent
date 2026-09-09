/* vram_queue.h - VRAM update queue (FIXED FILE: copied verbatim, never edited).
 * Game logic never touches VRAM while rendering is on; it queues records
 * here and nmi_handler() drains them inside the vblank budget.
 */
#ifndef VRAM_QUEUE_H
#define VRAM_QUEUE_H

#include <stdint.h>

#define VRAM_QUEUE_SIZE 192u          /* bytes of record storage */
#define VRAM_QUEUE_FLUSH_BUDGET 48u   /* data bytes drained per NMI (c_conventions "Vblank budget") */

/* Queue a run of `len` bytes (1..64) to be written at PPU address `addr`
 * with +1 increment (a row). Returns 1 if queued, 0 if the queue is full
 * (the caller retries next frame). Copies the bytes, so `src` may be a
 * temporary buffer. */
uint8_t vram_queue_add(uint16_t addr, const uint8_t *src, uint8_t len);
/* Same, but written downwards with the +32 increment (a column). */
uint8_t vram_queue_add_column(uint16_t addr, const uint8_t *src, uint8_t len);
/* Fill: `len` copies of one byte at `addr` (+1 increment). */
uint8_t vram_queue_add_fill(uint16_t addr, uint8_t value, uint8_t len);
/* Free bytes left (a record costs len + 3). */
uint8_t vram_queue_space(void);
/* Drop everything queued (use before a whole-screen redraw with rendering off). */
void vram_queue_clear(void);
/* Drain up to VRAM_QUEUE_FLUSH_BUDGET data bytes. Call ONLY from
 * nmi_handler(), first thing, before the scroll write and the OAM DMA. */
void vram_queue_flush(void);
/* 1 while records are still queued (wait for it to drop to 0 before
 * turning rendering off or queuing a whole screen's worth). */
uint8_t vram_queue_pending(void);

#endif /* VRAM_QUEUE_H */
