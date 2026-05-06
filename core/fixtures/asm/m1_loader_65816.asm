; ============================================================
; m1_loader_65816.asm — M1.6 .sfc test ROM loader.
;
; Boots a SNES into native mode, force-blanks the display,
; disables NMI/IRQ/HDMA/DMA, then runs the SPC700 IPL upload
; protocol (SPEC §19.2 + canonical fullsnes IPL behaviour) for
; module A, waits for the driver-ready signature, idles, sends
; RESET_TO_IPL (SPEC §20.1), uploads module B, then spins
; forever.
;
; Module data is embedded at fixed bank offsets so this loader
; doesn't need a relocation table. core::sfc_export writes the
; modules at file offsets $8000 (bank $01) and $10000 (bank $02).
;
; Fail-mode background colours (BGCOLOR / $2132 cumulative writes):
;   red    — IPL-ready timeout
;   green  — driver-ready timeout
;   blue   — command-ack timeout
;   white  — IPL upload byte-ack timeout
;
; Spin counts: SPEC §19.2 uses 32-bit values (0x0020_0000 etc).
; The loader uses 16-bit double-loops; the effective wait is
; outer * inner * ~few cycles, which gives ~50 ms at 21 MHz —
; well above the ~2 ms IPL ready time and ~1 ms command ack.
; ============================================================

arch 65816
lorom

; ---- LoROM header at $00:FFC0 (file offset $7FC0) ----
org $00FFC0
    db "SFCWC M1 TESTROM     "        ; exactly 21 chars (LoROM title field)
    db $20                            ; mode: LoROM SlowROM
    db $00                            ; type: ROM only
    db $08                            ; ROM size: 256 KB (1<<8 KB)
    db $00                            ; RAM size: none
    db $01                            ; country: US (0x01)
    db $33                            ; license: $33 = homebrew
    db $00                            ; version
    dw $0000                          ; checksum complement (asar fills)
    dw $0000                          ; checksum             (asar fills)

; ---- Native vectors at $00:FFE0 ----
org $00FFE4
    dw irq_unhandled                  ; native COP
    dw irq_unhandled                  ; native BRK
    dw irq_unhandled                  ; native ABORT
    dw irq_unhandled                  ; native NMI
    dw irq_unhandled                  ; native reserved
    dw irq_unhandled                  ; native IRQ

; ---- Emulation vectors at $00:FFF4 (COP / reserved / ABORT / NMI / RESET / IRQ) ----
org $00FFF4
    dw irq_unhandled                  ; emu COP   ($FFF4)
    dw irq_unhandled                  ; reserved  ($FFF6)
    dw irq_unhandled                  ; emu ABORT ($FFF8)
    dw irq_unhandled                  ; emu NMI   ($FFFA)
    dw reset                          ; emu RESET ($FFFC) — power-on entry
    dw irq_unhandled                  ; emu IRQ/BRK ($FFFE)

; ---- Code at $00:8000 ----
org $008000

reset:
    sei
    clc
    xce                               ; emulation -> native
    rep #$30                          ; A=16, X/Y=16
    sep #$20                          ; A=8 (we keep X/Y=16)
    ldx.w #$01FF
    txs

    ; Force-blank display (max brightness when we eventually
    ; un-blank; here the high bit means blank).
    lda #$80
    sta $2100

    ; Disable NMI / IRQ / VBlank interrupts and DMA channels.
    stz $4200                         ; NMITIMEN = 0
    stz $420C                         ; HDMAEN   = 0
    stz $420B                         ; MDMAEN   = 0

    ; Wait for SPC700 IPL ready signature ($AA on $2140, $BB on $2141).
    jsr wait_ipl_ready

    ; ----- Module A -----
    ; X = byte offset within module-bank (start at 0).
    ; B = bank number ($01 = bank 1, $02 = bank 2).
    sep #$20
    lda #$01
    sta $4
    rep #$10
    ldx.w #$0000
    jsr upload_module
    jsr wait_driver_ready

    ; Idle ~5 seconds (outer * inner * ~6 cycles ≈ several
    ; hundred million cycles at 21 MHz).
    jsr idle_5sec

    ; Send RESET_TO_IPL command and wait for $82 ack.
    jsr command_reset_to_ipl

    ; Wait for IPL ready again.
    jsr wait_ipl_ready

    ; ----- Module B -----
    sep #$20
    lda #$02
    sta $4
    rep #$10
    ldx.w #$0000
    jsr upload_module
    jsr wait_driver_ready

forever:
    bra forever


; ============================================================
; wait_ipl_ready: spin until $2140 == $AA and $2141 == $BB.
; ============================================================
wait_ipl_ready:
    php
    rep #$10
    sep #$20
    ldx.w #$0000
.outer:
    ldy.w #$FFFF
.inner:
    lda $2140
    cmp #$AA
    bne .next
    lda $2141
    cmp #$BB
    beq .done
.next:
    dey
    bne .inner
    inx
    cpx.w #$0010
    bne .outer
    ; Timeout. Fail colour: red (R=$1F).
    lda #$1F
    sta $2132
    jmp halt_visible
.done:
    plp
    rts


; ============================================================
; wait_driver_ready: spin until $2140 == $A5 and $2141 == $5A.
; ============================================================
wait_driver_ready:
    php
    rep #$10
    sep #$20
    ldx.w #$0000
.outer:
    ldy.w #$FFFF
.inner:
    lda $2140
    cmp #$A5
    bne .next
    lda $2141
    cmp #$5A
    beq .done
.next:
    dey
    bne .inner
    inx
    cpx.w #$0010
    bne .outer
    ; Timeout. Fail colour: green (G=$3E0).
    rep #$20
    lda.w #$03E0
    sta $2132                         ; high+low write picks up green band
    sep #$20
    jmp halt_visible
.done:
    plp
    rts


; ============================================================
; idle_5sec: outer * inner spin (~5 sec at 21 MHz).
; ============================================================
idle_5sec:
    php
    rep #$10
    ldx.w #$0080                      ; outer
.o:
    ldy.w #$FFFF                      ; inner
.i:
    dey
    bne .i
    dex
    bne .o
    plp
    rts


; ============================================================
; command_reset_to_ipl: write packet (token=$42, code=$02) and
; wait for both ack code ($82 in $2141) AND ack token ($42 in
; $2140). Verifying both fields prevents stale acks from a
; prior driver run from passing this gate (M2.0 / consultant #10).
; ============================================================
command_reset_to_ipl:
    php
    sep #$20
    rep #$10
    stz $2143
    stz $2142
    lda #$02
    sta $2141
    lda #$42
    sta $2140
    ; Wait for ack — code AND token must both match.
    ldx.w #$0000
.o:
    ldy.w #$FFFF
.i:
    lda $2141
    cmp #$82                           ; ack code: RESET_TO_IPL ack
    bne .next
    lda $2140
    cmp #$42                           ; token we sent
    beq .done
.next:
    dey
    bne .i
    inx
    cpx.w #$0010
    bne .o
    ; Timeout — blue.
    rep #$20
    lda.w #$7C00
    sta $2132
    sep #$20
    jmp halt_visible
.done:
    plp
    rts


; ============================================================
; upload_module: parse the module.bin at bank=DP $0004, offset=X
; (16-bit) and run the IPL upload protocol.
;
; The module bytes live at long address ($00:0004:X). We use
; long absolute addressing (lda $XX:offset, X) to read the
; ROM-resident module.
;
; Strategy:
;   1. Read block_count from header offset $0C (u16).
;   2. For each block (8-byte entry at offset $40 + i*8):
;        - dest_addr (u16)
;        - length    (u16)
;        - data_off  (u32)
;      Upload via IPL.
;   3. After last block, finalize with EXEC (entrypoint = $0200).
;
; Register conventions inside this routine:
;   DP:$0004 = module bank (8 bits, but DP $4 word reads the bank+0)
;   DP:$0006 = current block index (u16)
;   DP:$0008 = current block dest_addr (u16)
;   DP:$000A = current block length    (u16)
;   DP:$000C = current block data_offset (u32)
;   DP:$0010 = ipl_counter (u16; low 8 bits land on $2140)
; ============================================================
upload_module:
    php
    rep #$30

    ; Save module bank into DP $0005 (high byte), zero $0004.
    ; Actually, we store a 24-bit pointer in DP $4..$6 = (lo, mid, bank).
    ; X already holds the offset (byte index within the bank).
    ; lda long $00:0004 form requires (ml,mh,bank) operand wiring;
    ; instead we use B (data bank) register to address module bytes.
    sep #$20
    lda $4                            ; module bank
    pha
    plb                               ; data bank = module bank

    ; --- Send IPL kickoff for first block ---
    rep #$20
    ; Read block_count from header offset $0C.
    ; Module is at bank-DBR base $0000 (LoROM bank=$01 maps to ROM
    ; offset $8000, but the DBR-relative address is $8000 too).
    ldx.w #$0000                      ; reset offset to start of module
    ; Read block_count at offset $0C.
    lda $800C, x
    sta $6                            ; DP $6 = block_count

    ; --- IPL: send "first byte" kickoff with first block's dest ---
    ; The IPL protocol after AA/BB:
    ;   $2143 = dest_high; $2142 = dest_low
    ;   $2141 = $01 (any nonzero, NOT $CC); $2140 = $CC
    ;   wait for SPC echo $2140 = $CC
    ; Then per-byte:
    ;   $2141 = byte; $2140 = (counter & $FF)
    ;   wait for SPC echo $2140 = (counter & $FF)
    ; Counter starts at 0 for first byte of FIRST block.
    ; To start the next block within the same upload session:
    ;   $2143 = new_dest_high; $2142 = new_dest_low
    ;   $2141 = nonzero != $CC != $00
    ;   $2140 = (counter+2) & $FF; wait for ack
    ; To execute (after final block):
    ;   $2143 = entry_high; $2142 = entry_low
    ;   $2141 = $00; $2140 = (counter+2) & $FF; wait for ack.

    ; Read first block (at header offset $40).
    lda $8040, x                      ; first block dest_addr (u16)
    sta $8                            ; DP $8 = dest_addr
    lda $8042, x                      ; length (u16)
    sta $A                            ; DP $A = length
    lda $8044, x                      ; data_offset low 16
    sta $C                            ; DP $C = data_off lo
    lda $8046, x                      ; data_offset high 16
    sta $E                            ; DP $E = data_off hi

    sep #$20
    ; Write dest to $2142/$2143.
    lda $9                            ; high byte of dest_addr
    sta $2143
    lda $8                            ; low byte
    sta $2142
    lda #$01                          ; nonzero, not $CC, not $00
    sta $2141
    lda #$CC
    sta $2140
    jsr wait_ipl_ack_cc

    ; Initialize counter to 0.
    rep #$20
    stz $10                           ; DP $10 = counter

    ; --- Upload first block bytes ---
    jsr upload_current_block_bytes

    ; --- Subsequent blocks (block index 1..block_count-1) ---
    rep #$20
    lda #$0001
    sta $12                           ; DP $12 = current block index

.next_block:
    rep #$20
    lda $12
    cmp $6                            ; block_count
    bcs .all_blocks_done

    ; Compute block-table entry offset = $40 + index*8.
    asl                               ; *2
    asl                               ; *4
    asl                               ; *8
    clc
    adc #$0040
    tax
    lda $8000, x                      ; dest_addr
    sta $8
    lda $8002, x                      ; length
    sta $A
    lda $8004, x                      ; data_off lo
    sta $C
    lda $8006, x                      ; data_off hi
    sta $E

    ; NEXT_BLOCK transition:
    sep #$20
    lda $9
    sta $2143
    lda $8
    sta $2142
    lda #$02                          ; "next block" marker (any nonzero != $CC, $00)
    sta $2141
    rep #$20
    lda $10
    clc
    adc #$0002
    sta $10
    sep #$20
    lda $10                           ; low byte of counter
    sta $2140
    jsr wait_ipl_ack_low

    jsr upload_current_block_bytes

    rep #$20
    inc $12
    bra .next_block

.all_blocks_done:
    ; --- EXEC: jump to entrypoint $0200 ---
    sep #$20
    lda #$02
    sta $2143
    stz $2142
    stz $2141                         ; $00 = exec
    rep #$20
    lda $10
    clc
    adc #$0002
    sta $10
    sep #$20
    lda $10
    sta $2140
    jsr wait_ipl_ack_low

    ; Restore DBR to bank 0.
    lda #$00
    pha
    plb
    plp
    rts


; ============================================================
; upload_current_block_bytes: stream DP $A bytes from module
; offset DP $C (32-bit) to the SPC, advancing DP $10 counter.
; Bank-relative read using long ROM addressing.
; ============================================================
upload_current_block_bytes:
    php
    rep #$30
    ; Source byte pointer in X (16-bit; assume length+offset stays
    ; within bank — modules are < 32 KB so this holds).
    lda $C
    tax
    ; Length counter in Y.
    lda $A
    tay

.byte_loop:
    cpy #$0000
    beq .done

    sep #$20
    lda $8000, x                      ; read module byte at bank-relative offset
    sta $2141
    rep #$20
    inc $10                           ; counter += 1
    sep #$20
    lda $10
    sta $2140
    jsr wait_ipl_ack_low
    rep #$20
    inx
    dey
    bra .byte_loop

.done:
    ; Persist updated source pointer (low 16) back to DP $C.
    txa
    sta $C
    plp
    rts


; ============================================================
; wait_ipl_ack_cc: poll $2140 for $CC.
; ============================================================
wait_ipl_ack_cc:
    php
    sep #$20
    rep #$10
    ldx.w #$0000
.o:
    ldy.w #$FFFF
.i:
    lda $2140
    cmp #$CC
    beq .done
    dey
    bne .i
    inx
    cpx.w #$0010
    bne .o
    ; Timeout — white.
    rep #$20
    lda.w #$7FFF
    sta $2132
    sep #$20
    jmp halt_visible
.done:
    plp
    rts


; ============================================================
; wait_ipl_ack_low: poll $2140 for the low byte of DP $10.
; ============================================================
wait_ipl_ack_low:
    php
    sep #$20
    rep #$10
    ldx.w #$0000
.o:
    ldy.w #$FFFF
.i:
    lda $2140
    cmp $10
    beq .done
    dey
    bne .i
    inx
    cpx.w #$0010
    bne .o
    rep #$20
    lda.w #$7FFF
    sta $2132
    sep #$20
    jmp halt_visible
.done:
    plp
    rts


; ============================================================
; halt_visible: enable display so the user sees the fail colour.
; ============================================================
halt_visible:
    sep #$20
    lda #$0F
    sta $2100
.h:
    bra .h


irq_unhandled:
    rti


; ============================================================
; Module A and B placement.
;
; LoROM bank 1 = file offset $8000. core::sfc_export overwrites
; this region with the actual module.bin contents at .sfc build
; time; we just reserve a label here so the loader's reads make
; sense in asar.
; ============================================================
org $018000
module_a_marker:

org $028000
module_b_marker:

; Pad ROM up to 256 KB so the .sfc file size matches the header
; ROM-size byte ($08 = 256 KB). Bank 7 ends at SNES $07:FFFF =
; file offset $3FFFF.
org $07FFFF
    db $00
