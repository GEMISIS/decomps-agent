; vram_copy.s - assembly VRAM writers for the NMI-time queue flush.
; Copy to src/vram_copy.s unchanged. C glue (in nes_hw.c):
;
;   #pragma bss-name (push, "ZEROPAGE")
;   const uint8_t *vram_src_zp;          /* GLOBAL, declared before the pragma below */
;   #pragma bss-name (pop)
;   #pragma zpsym ("vram_src_zp")        /* only works for a global declared above it */
;   uint8_t vram_copy_len, vram_copy_addr_hi, vram_copy_addr_lo;
;   void hw_vram_copy_run(void);         /* copies vram_copy_len bytes from vram_src_zp */
;   void hw_vram_write_entry(void);      /* latch reset + PPUADDR + copy, one call per entry */
;
;   void hw_vram_write_run(uint16_t addr, const uint8_t *src, uint8_t len) {
;       vram_copy_addr_hi = addr >> 8; vram_copy_addr_lo = addr & 0xFF;
;       vram_src_zp = src; vram_copy_len = len; hw_vram_write_entry();
;   }
;
; Cost: ~45 cycles per entry + 16 cycles per byte. A cc65 C loop doing the
; same is ~350 cycles per entry + 40-100 per byte and does not fit vblank.
.export _hw_vram_copy_run, _hw_vram_write_entry
.importzp _vram_src_zp
.import _vram_copy_len, _vram_copy_addr_hi, _vram_copy_addr_lo

.segment "CODE"

; void hw_vram_copy_run(void): PPUDATA <- vram_copy_len bytes from (vram_src_zp)
.proc _hw_vram_copy_run
    ldx _vram_copy_len
    beq done
    ldy #0
loop:
    lda (_vram_src_zp),y
    sta $2007
    iny
    dex
    bne loop
done:
    rts
.endproc

; void hw_vram_write_entry(void): bit $2002; PPUADDR = hi,lo; then the copy.
.proc _hw_vram_write_entry
    bit $2002
    lda _vram_copy_addr_hi
    sta $2006
    lda _vram_copy_addr_lo
    sta $2006
    jmp _hw_vram_copy_run
.endproc
