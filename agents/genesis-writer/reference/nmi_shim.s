; nmi_shim.s - startup, interrupt shim and iNES header for cc65 NES projects.
;
; Assembled as the project's own startup object. It exports __STARTUP__ and
; provides the VECTORS segment, so the nes.lib crt0 is never linked; the
; library still supplies the C runtime (zerobss, copydata, helpers).
;
; Exports to C (declare them in nes.h):
;   extern volatile uint8_t nmi_flag;      set to 1 by every NMI
;   extern volatile uint8_t frame_count;   increments every NMI (wraps)
;   extern uint8_t nmi_enabled;            when non-zero, NMI calls nmi_handler()
;   extern uint8_t irq_enabled;            when non-zero, IRQ calls irq_handler()
; Imports from C:
;   void nmi_handler(void);  void irq_handler(void);  void main(void);
;
; Build: ca65 -t nes -D MAPPER=<n> -D PRG16K=<n> -D CHR8K=<n> [-D MIRROR=<0|1>] [-D BATTERY=1] nmi_shim.s

.export     __STARTUP__ : absolute = 1
.export     _nmi_flag, _frame_count, _nmi_enabled, _irq_enabled
.import     _main, _nmi_handler, _irq_handler
.import     zerobss, copydata, initlib, donelib
.import     __SRAM_START__, __SRAM_SIZE__
.importzp   sp
ZPSPACE = 26                ; cc65 zero-page workspace: sp(2) sreg(2) regsave(4) regbank(6) tmp1-4(4) ptr1-4(8)

; ---------------------------------------------------------------- header --
; iNES header, parameterised at assembly time. Defaults: NROM, 2 x 16K, 1 x 8K, vertical.
.ifndef MAPPER
MAPPER  = 0
.endif
.ifndef PRG16K
PRG16K  = 2
.endif
.ifndef CHR8K
CHR8K   = 1
.endif
.ifndef MIRROR
MIRROR  = 1          ; 0 = horizontal, 1 = vertical
.endif
.ifndef BATTERY
BATTERY = 0
.endif

.macro INES_HEADER mapper, prg, chr, mirror, battery
    .byte "NES", $1A
    .byte prg
    .byte chr
    .byte ((mapper & $0F) << 4) | (mirror & 1) | ((battery & 1) << 1)
    .byte (mapper & $F0)
    .byte 0, 0, 0, 0, 0, 0, 0, 0
.endmacro

.segment "HEADER"
    INES_HEADER MAPPER, PRG16K, CHR8K, MIRROR, BATTERY

; --------------------------------------------------------------- zeropage --
.segment "ZEROPAGE"
_nmi_flag:      .res 1
_frame_count:   .res 1
_nmi_enabled:   .res 1
_irq_enabled:   .res 1

; ---------------------------------------------------------------- startup --
.segment "STARTUP"

PPU_CTRL   = $2000
PPU_MASK   = $2001
PPU_STATUS = $2002
APU_STATUS = $4015
APU_FRAME  = $4017
DMC_FREQ   = $4010

reset:
    sei
    cld
    ldx #$40
    stx APU_FRAME           ; frame IRQ off
    ldx #$FF
    txs
    inx                     ; X = 0
    stx PPU_CTRL            ; NMI off
    stx PPU_MASK            ; rendering off
    stx DMC_FREQ            ; DMC IRQ off
    stx APU_STATUS          ; all channels off

    bit PPU_STATUS          ; clear stale vblank flag
@vblank1:
    bit PPU_STATUS
    bpl @vblank1

    ; clear internal RAM $0000-$07FF
    lda #0
    tax
@clear:
    sta $0000, x
    sta $0100, x
    sta $0200, x
    sta $0300, x
    sta $0400, x
    sta $0500, x
    sta $0600, x
    sta $0700, x
    inx
    bne @clear

@vblank2:
    bit PPU_STATUS
    bpl @vblank2

    ; C runtime: parameter stack, BSS, DATA image, constructors
    lda #<(__SRAM_START__ + __SRAM_SIZE__)
    sta sp
    lda #>(__SRAM_START__ + __SRAM_SIZE__)
    sta sp + 1
    jsr zerobss
    jsr copydata
    jsr initlib

    ; NMI on before main(): hw_wait_vblank() spins on _nmi_flag, which only
    ; the NMI sets, so a main() that waits for vblank before enabling NMI
    ; would hang forever (seen 2026-09-04: blank screen, zero palette).
    ; _nmi_enabled still gates the C handler; the shim's own bookkeeping
    ; (_frame_count, _nmi_flag) runs from the first frame.
    lda #$80
    sta PPU_CTRL

    jsr _main
    jsr donelib
@halt:
    jmp @halt

; -------------------------------------------------------------------- NMI --
.segment "CODE"

nmi:
    pha
    txa
    pha
    tya
    pha

    inc _frame_count
    lda #1
    sta _nmi_flag

    lda _nmi_enabled
    beq @done
    ; The handler is C code, so it uses cc65's zero-page workspace and the C
    ; stack pointer. The interrupted main-thread C code may be mid-expression
    ; in exactly those locations, so save them all (26 bytes) and restore
    ; afterwards. The handler's own C-stack pushes land below the saved sp,
    ; i.e. in free space, so no separate NMI stack is needed.
    ldx #ZPSPACE-1
@zsave:
    lda sp,x
    pha
    dex
    bpl @zsave
    jsr _nmi_handler
    ldx #0
@zrest:
    pla
    sta sp,x
    inx
    cpx #ZPSPACE
    bne @zrest
@done:
    pla
    tay
    pla
    tax
    pla
    rti

; -------------------------------------------------------------------- IRQ --
irq:
    pha
    txa
    pha
    tya
    pha
    lda _irq_enabled
    beq @done
    jsr _irq_handler
@done:
    pla
    tay
    pla
    tax
    pla
    rti

; ---------------------------------------------------------------- vectors --
.segment "VECTORS"
    .addr nmi
    .addr reset
    .addr irq
