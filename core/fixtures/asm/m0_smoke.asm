; M0.3 asar pipeline smoke source.
;
; Exists only to prove the asar invocation pipeline produces a
; deterministic 64 KB ARAM image with our bytes at the address we
; asked for. Not a real driver; M0.4+ replaces this with something
; that drives audio.
;
; ============================================================
; Mapper trick (see core/src/asm.rs module docs for the whole
; story): asar's default LoROM/HiROM mapping treats `org` as a
; SNES bus address, so `org $0200` errors with
; Esnes_address_doesnt_map_to_rom. Workaround:
;
;   - `lorom` selects the LoROM mapper.
;   - `org $008200` lands at file offset 0x0200 (LoROM bank $00,
;     internal offset $8200 = file offset $0200).
;   - `base $0200` re-bases label resolution so SPC700 labels
;     resolve to ARAM addresses, not to $008200.
;
; The host driver invokes asar with `--no-title-check
; --fix-checksum=off` so asar doesn't try to inject a SNES title
; or LoROM checksum into our flat ARAM image.
; ============================================================

lorom
arch spc700

org $008200
base $0200

start:
    nop                ; opcode 0x00 at ARAM $0200 (file offset 0x0200)
    bra start          ; opcode 0x2F + signed disp 0xFD at $0201..$0202;
                       ; -3 from PC=$0203 returns to `start` ($0200).

; Expected sentinel bytes at file offset 0x0200..0x0202:
;   0x00 0x2F 0xFD
; Every other byte in the 64 KB image must be zero. The
; `assemble_smoke_when_asar_resolved` integration test in
; app/tests/cli.rs locks both halves of that contract.
