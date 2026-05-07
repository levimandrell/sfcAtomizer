; ============================================================
; m2_multi_voice_atom.asm — M2.5 driver (SPEC §20.2,
; multi_voice_atom profile).
;
; Implements the full SPEC §14.3 / §15.7 / §20.2 contract:
;
;   - Init reads the 22-byte voice setup table (SPEC §15.7);
;     voices with src_index = $FF are skipped.
;   - T0 timer drives a 60.150 Hz nominal tick (T0TARGET=$85).
;   - Each tick: slide-advance → wait-decrement → opcode-read
;     (SPEC §14.3 WAIT execution model).
;   - 8 SEQ2 opcodes: END, WAIT, SET_SRC, SET_VOL, KON, KOFF,
;     VOL_SLIDE, SET_PITCH.
;   - Slide accumulator state per SPEC §14.3 with integer
;     Bresenham + round-half-AWAY-from-zero.
;
; M1 holdovers preserved: dp_last_token bootstrap fix (M2.0
; consultant #1), STOP / RESET_TO_IPL / PING host commands
; (single host poll per main-loop pass; tick processing is a
; separate poll on T0OUT).
;
; Direct-page layout (zero-page, driver-private; SPEC §20.2):
;   $00  dp_last_token          ; M1 bootstrap fix
;   $01  status_flags           ; M2 8-bit map (SPEC §20.2)
;   $02  seq_ptr_lo
;   $03  seq_ptr_hi
;   $04  wait_counter
;   $05  active_voice_mask
;   $06  slide_voice            ; $FF = no active slide
;   $07  slide_ticks_total
;   $08  slide_ticks_done       ; advances 1..=total
;   $09  slide_start_l
;   $0A  slide_start_r
;   $0B  slide_target_l
;   $0C  slide_target_r
;   $0D  slide_dl_abs           ; |target_l - start_l|
;   $0E  slide_dl_sign          ; $00 if dl>=0, $FF if dl<0
;   $0F  slide_dr_abs
;   $10  slide_dr_sign
;   $11  slide_half_total       ; total/2 (precomputed)
;   $12  current_vol_l_v0
;   $13  current_vol_r_v0
;   $14  current_vol_l_v1
;   $15  current_vol_r_v1
;
; Sentinel: 4 bytes `$de $ad $be $ef` immediately after the
; last instruction. core::driver_build does a post-slice scan
; to catch sentinel collisions.
; ============================================================

incsrc "m2_constants.inc"

lorom
arch spc700

org $008200
base $0200

; ============================================================
; Entry — runs once at boot, then falls into main_loop.
; ============================================================
driver_entry:
    ; --- 1. Mute amp + echo write disable while configuring DSP.
    mov $f2, #$6c                   ; FLG
    mov $f3, #$60

    ; --- 2. Clear KON / KOFF.
    mov $f2, #$4c
    mov $f3, #$00
    mov $f2, #$5c
    mov $f3, #$00

    ; --- 3. Master volumes.
    mov $f2, #$0c
    mov $f3, #master_voll
    mov $f2, #$1c
    mov $f3, #master_volr

    ; --- 4. Source directory page.
    mov $f2, #$5d
    mov $f3, #src_dir_page

    ; --- 5. Echo configuration (FIR + ESA + EDL before EON).
    mov $f2, #$0d
    mov $f3, #echo_efb
    mov $f2, #$2c
    mov $f3, #echo_evoll
    mov $f2, #$3c
    mov $f3, #echo_evolr
    mov $f2, #$6d
    mov $f3, #echo_esa
    mov $f2, #$7d
    mov $f3, #echo_edl
    mov $f2, #$0f
    mov $f3, #echo_fir_0
    mov $f2, #$1f
    mov $f3, #echo_fir_1
    mov $f2, #$2f
    mov $f3, #echo_fir_2
    mov $f2, #$3f
    mov $f3, #echo_fir_3
    mov $f2, #$4f
    mov $f3, #echo_fir_4
    mov $f2, #$5f
    mov $f3, #echo_fir_5
    mov $f2, #$6f
    mov $f3, #echo_fir_6
    mov $f2, #$7f
    mov $f3, #echo_fir_7
    mov $f2, #$4d                   ; EON
    mov $f3, #echo_eon

    ; --- 6. Voice setup table walk (SPEC §15.7).
    ; The table sits at `voice_setup_addr` (a u16 constant from
    ; m2_constants.inc). We read 11 bytes per voice for
    ; voices 0 and 1; if src_index=$FF, skip the voice.
    ;
    ; Use direct-page pointer at $20-$21 to walk the table.
    mov $20, #voice_setup_addr_lo
    mov $21, #voice_setup_addr_hi

    ; Voice 0 entry — read 11 bytes via ($20)+y.
    call setup_voice_0
    ; Advance pointer by 11 bytes.
    mov a, #11
    clrc
    adc a, $20
    mov $20, a
    mov a, #0
    adc a, $21
    mov $21, a
    ; Voice 1 entry.
    call setup_voice_1

    ; --- 7. FLG: unmute amp; ECEN per master_echo policy.
    mov $f2, #$6c
    mov $f3, #flg_running

    ; --- 7.1 KON sample_sustain voices (atom voices are KON'd by
    ;     bytecode). init_kon_mask is computed by core::driver_build
    ;     from project tracks.
    mov $f2, #$4c                   ; KON
    mov $f3, #init_kon_mask

    ; --- 8. Direct-page state.
    mov $01, #status_flags_initial
    mov $02, #sequence_addr_lo
    mov $03, #sequence_addr_hi
    mov $04, #$00                   ; wait_counter
    mov $05, #$00                   ; active_voice_mask
    mov $06, #$ff                   ; slide_voice = no active slide
    mov $12, #$00                   ; current_vol_l_v0
    mov $13, #$00                   ; current_vol_r_v0
    mov $14, #$00                   ; current_vol_l_v1
    mov $15, #$00                   ; current_vol_r_v1

    ; --- 8.1 Bootstrap-token fix (M2.0 consultant #1).
    mov a, $f4
    mov $00, a

    ; --- 9. T0 timer setup. T0TARGET=$85 → 8000/133 ≈ 60.150 Hz.
    mov $fa, #$85                   ; T0TARGET
    mov $f1, #$01                   ; CONTROL: enable T0

    ; --- 10. Driver-ready signature.
    mov $f4, #$a5
    mov $f5, #$5a
    mov $f6, #$02                   ; driver_version (M2 = 2)
    mov $f7, $01                    ; status_flags

    jmp main_loop

; ============================================================
; setup_voice_0 / setup_voice_1 — read 11-byte voice setup
; table entry from [$20] and program voice DSP regs.
; If src_index ($FF in entry byte 1) sentinel, skip.
;
; Per SPEC §15.7 byte map:
;   byte 0  voice (informational; we hard-code addressing)
;   byte 1  src_index           ($FF = unused)
;   byte 2  pitch_l
;   byte 3  pitch_h
;   byte 4  vol_l
;   byte 5  vol_r
;   byte 6  adsr1
;   byte 7  adsr2
;   byte 8  gain
;   byte 9  flags_reserved
;   byte 10 pad_reserved
; ============================================================
setup_voice_0:
    mov y, #1
    mov a, ($20)+y
    cmp a, #$ff
    beq .skip
    ; Write voice 0 DSP regs ($00 base for VOLL/VOLR/PITCHL/PITCHH/SRCN/ADSR1/ADSR2/GAIN).
    ; SRCN.
    mov $f2, #$04
    mov $f3, a
    ; PITCHL.
    mov y, #2
    mov a, ($20)+y
    mov $f2, #$02
    mov $f3, a
    ; PITCHH.
    mov y, #3
    mov a, ($20)+y
    mov $f2, #$03
    mov $f3, a
    ; VOL_L.
    mov y, #4
    mov a, ($20)+y
    mov $f2, #$00
    mov $f3, a
    mov $12, a                      ; current_vol_l_v0
    ; VOL_R.
    mov y, #5
    mov a, ($20)+y
    mov $f2, #$01
    mov $f3, a
    mov $13, a                      ; current_vol_r_v0
    ; ADSR1.
    mov y, #6
    mov a, ($20)+y
    mov $f2, #$05
    mov $f3, a
    ; ADSR2.
    mov y, #7
    mov a, ($20)+y
    mov $f2, #$06
    mov $f3, a
    ; GAIN.
    mov y, #8
    mov a, ($20)+y
    mov $f2, #$07
    mov $f3, a
.skip:
    ret

setup_voice_1:
    mov y, #1
    mov a, ($20)+y
    cmp a, #$ff
    beq .skip
    ; Voice 1 DSP regs ($10 base).
    mov $f2, #$14
    mov $f3, a
    mov y, #2
    mov a, ($20)+y
    mov $f2, #$12
    mov $f3, a
    mov y, #3
    mov a, ($20)+y
    mov $f2, #$13
    mov $f3, a
    mov y, #4
    mov a, ($20)+y
    mov $f2, #$10
    mov $f3, a
    mov $14, a                      ; current_vol_l_v1
    mov y, #5
    mov a, ($20)+y
    mov $f2, #$11
    mov $f3, a
    mov $15, a                      ; current_vol_r_v1
    mov y, #6
    mov a, ($20)+y
    mov $f2, #$15
    mov $f3, a
    mov y, #7
    mov a, ($20)+y
    mov $f2, #$16
    mov $f3, a
    mov y, #8
    mov a, ($20)+y
    mov $f2, #$17
    mov $f3, a
.skip:
    ret

; ============================================================
; Main loop — polls host commands AND T0OUT each iteration.
; ============================================================
main_loop:
    ; 1. Host command poll.
    mov a, $f4
    cmp a, $00
    beq .no_host
    mov $00, a
    mov a, $f5
    cmp a, #$01
    beq handle_stop
    cmp a, #$02
    beq handle_reset_to_ipl
    cmp a, #$7f
    beq handle_ping
    bra reject_command
.no_host:
    ; 2. T0OUT poll.
    mov a, $fd                      ; T0OUT (clears on read)
    beq main_loop
    ; A = number of ticks elapsed since last poll. Process one tick
    ; per main-loop pass; if A >= 4 set timing_overrun bit.
    cmp a, #$04
    bcc .no_overrun
    mov a, $01
    or a, #$20                      ; timing_overrun
    mov $01, a
.no_overrun:
    call process_tick
    bra main_loop

; ============================================================
; Host-command handlers (mirrors M1).
; ============================================================
reject_command:
    mov a, $01
    or a, #$40                      ; bytecode_error == invalid command
    mov $01, a
    mov $f4, $00
    mov $f5, #$ee
    mov $f6, #$02
    mov $f7, $01
    bra main_loop

handle_stop:
    mov $f2, #$5c                   ; KOFF all voices
    mov $f3, #$03
    mov a, $01
    and a, #$fc                     ; clear voice0_active + voice1_active
    or a, #$08                      ; stopped
    mov $01, a
    mov $f4, $00
    mov $f5, #$81
    mov $f6, #$02
    mov $f7, $01
    bra main_loop

handle_ping:
    mov $f4, $00
    mov $f5, #$ff
    mov $f6, #$02
    mov $f7, $01
    bra main_loop

handle_reset_to_ipl:
    mov a, $01
    or a, #$10                      ; reset_to_ipl_pending
    mov $01, a
    mov $f4, $00
    mov $f5, #$82
    mov $f6, #$02
    mov $f7, $01
    mov $f2, #$5c
    mov $f3, #$03                   ; KOFF both voices
    mov $f2, #$6c
    mov $f3, #$60                   ; FLG mute
    mov $f2, #$4d
    mov $f3, #$00                   ; EON = 0
    mov $f1, #$80                   ; CONTROL bit 7: map IPL ROM
    jmp $ffc0

; ============================================================
; process_tick — one tick of slide-advance → wait-decrement →
; opcode-read.
; ============================================================
process_tick:
    call advance_slide
    ; wait-decrement.
    mov a, $04
    beq .read
    dec a
    mov $04, a
    ret
.read:
    call opcode_read
    ret

; ============================================================
; opcode_read — execute opcodes until WAIT, END, or budget.
; Reads from seq_ptr ($02-$03) and advances on each consume.
; ============================================================
opcode_read:
    mov y, #0
    mov a, ($02)+y
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    ; SPC700 relative branches are ±127 bytes; the opcode
    ; handlers are too far for direct `beq`. Use the
    ; `bne <skip>; jmp <handler>` pattern (3 bytes per opcode
    ; skip block).
    cmp a, #$00                     ; END
    bne .not_end
    jmp op_end
.not_end:
    cmp a, #$01                     ; WAIT
    bne .not_wait
    jmp op_wait
.not_wait:
    cmp a, #$10                     ; SET_SRC
    bne .not_set_src
    jmp op_set_src
.not_set_src:
    cmp a, #$11                     ; SET_VOL
    bne .not_set_vol
    jmp op_set_vol
.not_set_vol:
    cmp a, #$12                     ; KON
    bne .not_kon
    jmp op_kon
.not_kon:
    cmp a, #$13                     ; KOFF
    bne .not_koff
    jmp op_koff
.not_koff:
    cmp a, #$20                     ; VOL_SLIDE
    bne .not_vol_slide
    jmp op_vol_slide
.not_vol_slide:
    cmp a, #$30                     ; SET_PITCH
    bne .not_set_pitch
    jmp op_set_pitch
.not_set_pitch:
    jmp on_bytecode_error

op_end:
    ; Set stopped bit and halt.
    mov a, $01
    or a, #$08                      ; stopped
    mov $01, a
    ret

op_wait:
    mov y, #0
    mov a, ($02)+y
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    mov $04, a                      ; wait_counter
    ret

; SET_SRC voice, src_index
op_set_src:
    mov y, #0
    mov a, ($02)+y                  ; voice
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    push a                          ; save voice for after src read
    mov y, #0
    mov a, ($02)+y                  ; src_index
    inc $02
    bne .no_carry2
    inc $03
.no_carry2:
    ; Stash src in $30.
    mov $30, a
    pop a                           ; voice back into A
    cmp a, #$00
    beq .voice0
    ; voice 1
    mov $f2, #$14                   ; VxSRCN voice 1
    mov $f3, $30
    jmp opcode_read
.voice0:
    mov $f2, #$04                   ; VxSRCN voice 0
    mov $f3, $30
    jmp opcode_read

; SET_VOL voice, vol_l, vol_r
op_set_vol:
    mov y, #0
    mov a, ($02)+y                  ; voice
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    push a
    mov y, #0
    mov a, ($02)+y                  ; vol_l
    inc $02
    bne .no_carry2
    inc $03
.no_carry2:
    mov $30, a                      ; vol_l in scratch
    mov y, #0
    mov a, ($02)+y                  ; vol_r
    inc $02
    bne .no_carry3
    inc $03
.no_carry3:
    mov $31, a                      ; vol_r in scratch
    pop a                           ; voice
    cmp a, #$00
    beq .voice0
    ; voice 1
    mov $f2, #$10
    mov $f3, $30
    mov $14, $30                    ; current_vol_l_v1
    mov $f2, #$11
    mov $f3, $31
    mov $15, $31                    ; current_vol_r_v1
    jmp opcode_read
.voice0:
    mov $f2, #$00
    mov $f3, $30
    mov $12, $30                    ; current_vol_l_v0
    mov $f2, #$01
    mov $f3, $31
    mov $13, $31                    ; current_vol_r_v0
    jmp opcode_read

; KON voice_mask
op_kon:
    mov y, #0
    mov a, ($02)+y
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    ; Clear KOFF latches first — S-DSP holds voices in release while
    ; KOFF bits stay set. Without this, a KON immediately following a
    ; KOFF reads as silence (voice keeps getting key-off'd every DSP
    ; cycle).
    push a
    mov $f2, #$5c                   ; KOFF
    mov $f3, #$00
    pop a
    mov $f2, #$4c                   ; KON
    mov $f3, a
    ; Update active_voice_mask = (active | new).
    mov $30, a
    mov a, $05
    or a, $30
    mov $05, a
    ; Update status_flags voice0_active / voice1_active.
    mov a, $01
    or a, $30                       ; bits 0+1 align with voice_mask 0b01/0b10
    mov $01, a
    jmp opcode_read

; KOFF voice_mask
op_koff:
    mov y, #0
    mov a, ($02)+y
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    mov $f2, #$5c                   ; KOFF
    mov $f3, a
    ; Clear matching bits in active_voice_mask.
    mov $30, a
    eor a, #$ff                     ; complement
    mov $31, a                      ; ~mask
    mov a, $05
    and a, $31
    mov $05, a
    ; Clear voice0_active / voice1_active in status_flags.
    mov a, $01
    and a, $31
    mov $01, a
    jmp opcode_read

; VOL_SLIDE voice, target_l, target_r, ticks
op_vol_slide:
    mov y, #0
    mov a, ($02)+y                  ; voice
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    mov $06, a                      ; slide_voice
    mov y, #0
    mov a, ($02)+y                  ; target_l
    inc $02
    bne .no_carry2
    inc $03
.no_carry2:
    mov $0b, a                      ; slide_target_l
    mov y, #0
    mov a, ($02)+y                  ; target_r
    inc $02
    bne .no_carry3
    inc $03
.no_carry3:
    mov $0c, a                      ; slide_target_r
    mov y, #0
    mov a, ($02)+y                  ; ticks
    inc $02
    bne .no_carry4
    inc $03
.no_carry4:
    mov $07, a                      ; slide_ticks_total
    mov $08, #$00                   ; slide_ticks_done = 0
    ; slide_half_total = ticks / 2 (LSR).
    lsr a
    mov $11, a
    ; Capture slide_start_l / slide_start_r from current voice volumes.
    mov a, $06
    cmp a, #$00
    beq .v0
    ; voice 1
    mov a, $14
    mov $09, a
    mov a, $15
    mov $0a, a
    bra .compute_deltas
.v0:
    mov a, $12
    mov $09, a
    mov a, $13
    mov $0a, a
.compute_deltas:
    ; dl = target_l - start_l. abs + sign. Use cmp a, dp (no
    ; cmp a, x in SPC700).
    mov a, $0b
    cmp a, $09
    bcs .dl_pos
    ; target < start → dl negative → abs = start - target, sign=$ff.
    mov a, $09
    setc
    sbc a, $0b
    mov $0d, a                      ; |dl|
    mov $0e, #$ff
    bra .dr
.dl_pos:
    ; A is target_l ($0b); compute |dl| = target_l - start_l.
    setc
    sbc a, $09
    mov $0d, a
    mov $0e, #$00
.dr:
    mov a, $0c
    cmp a, $0a
    bcs .dr_pos
    mov a, $0a
    setc
    sbc a, $0c
    mov $0f, a
    mov $10, #$ff
    jmp opcode_read
.dr_pos:
    ; A is target_r ($0c).
    setc
    sbc a, $0a
    mov $0f, a
    mov $10, #$00
    jmp opcode_read

; SET_PITCH voice, pitch_l, pitch_h
op_set_pitch:
    mov y, #0
    mov a, ($02)+y                  ; voice
    inc $02
    bne .no_carry1
    inc $03
.no_carry1:
    push a
    mov y, #0
    mov a, ($02)+y                  ; pitch_l
    inc $02
    bne .no_carry2
    inc $03
.no_carry2:
    mov $30, a
    mov y, #0
    mov a, ($02)+y                  ; pitch_h
    inc $02
    bne .no_carry3
    inc $03
.no_carry3:
    mov $31, a
    pop a
    cmp a, #$00
    beq .voice0
    mov $f2, #$12
    mov $f3, $30
    mov $f2, #$13
    mov $f3, $31
    jmp opcode_read
.voice0:
    mov $f2, #$02
    mov $f3, $30
    mov $f2, #$03
    mov $f3, $31
    jmp opcode_read

on_bytecode_error:
    mov a, $01
    or a, #$48                      ; bytecode_error + stopped
    mov $01, a
    ret

; ============================================================
; advance_slide — runs at the start of each tick. If a slide
; is active (slide_voice != $FF), increment slide_ticks_done
; and write VOLL/VOLR per SPEC §14.3 integer-Bresenham
; round-half-AWAY-from-zero formula.
; ============================================================
advance_slide:
    mov a, $06
    cmp a, #$ff
    bne .active
    ret
.active:
    ; slide_ticks_done += 1
    mov a, $08
    inc a
    mov $08, a
    ; Compute new_l = start_l + signed( |dl|*done + half_total ) / total ; sign(dl)
    ;   Step 1: compute |dl|*done → 16-bit Y:A.
    mov y, $08
    mov a, $0d                      ; |dl|
    mul ya
    ;   Step 2: add half_total to YA (low byte) with carry into Y.
    ;   SPC700: adc a, dp; on carry propagate to Y via inc.
    clrc
    adc a, $11
    bcc .no_y_inc_l
    inc y
.no_y_inc_l:
    ;   Step 3: divide YA / total → A = quotient.
    mov x, $07
    div ya, x
    ;   Step 4: apply sign(dl). $0e is $00 if dl>=0, $ff if dl<0.
    mov y, $0e
    beq .pos_dl
    eor a, #$ff
    inc a
.pos_dl:
    ;   Step 5: new_l = start_l + signed_step.
    clrc
    adc a, $09
    mov $30, a                      ; new_l in scratch
    ;   Same for R.
    mov y, $08
    mov a, $0f
    mul ya
    clrc
    adc a, $11
    bcc .no_y_inc_r
    inc y
.no_y_inc_r:
    mov x, $07
    div ya, x
    mov y, $10
    beq .pos_dr
    eor a, #$ff
    inc a
.pos_dr:
    clrc
    adc a, $0a
    mov $31, a                      ; new_r
    ; Write to DSP based on slide_voice.
    mov a, $06
    cmp a, #$00
    beq .v0
    mov $f2, #$10
    mov $f3, $30
    mov $14, $30
    mov $f2, #$11
    mov $f3, $31
    mov $15, $31
    bra .check_done
.v0:
    mov $f2, #$00
    mov $f3, $30
    mov $12, $30
    mov $f2, #$01
    mov $f3, $31
    mov $13, $31
.check_done:
    ; If slide_ticks_done == slide_ticks_total, end the slide.
    mov a, $08
    cmp a, $07
    bne .not_done
    mov $06, #$ff                   ; slide_voice = no active
.not_done:
    ret

driver_end:
    db $de, $ad, $be, $ef           ; sentinel — see file header
