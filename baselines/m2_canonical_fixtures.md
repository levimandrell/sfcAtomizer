# M2 canonical fixtures

The canonical M2 multi_voice_atom fixture is a 2-step atom_sequence
on voice 1 over a single sample-based voice 0. Used by the M2.4
sequence-compiler tests, the M2.5 driver tests, the M2.6 SFC
acceptance pipeline, and the M2.7 GUI editor's round-trip parity
test.

Originally documented inline in `STATUS.md` as part of the M2.4
"baselines locked" section; extracted here at M2.8 (consultant #21)
so STATUS stays scannable and the fixture has a stable home for
future reference.

## Fixture shape

```
project (v2):
  driver.profile = "multi_voice_atom"
  driver.bytecode_version = 2
  sample_pool: 1 entry — "lead", panned LEFT
  atom_pool: 2 entries — atom_a (sine_128), atom_b (sine_64)
  atom_sequences: 1 entry — atomseq_0001 (voice 1)
    step 0: atom_a, duration 120, target_volume 0.8, transition initial_kon
    step 1: atom_b, duration 120, target_volume 0.8,
            transition fade_to_zero_retrigger { fade_out=4, fade_in=4 }
```

## Locked SHAs

```
M2_CANONICAL_SEQUENCE_BYTECODE_SHA256 = f9fa6ea85a7197b662a3b386c9606bb25954708b9714629de7069c90f0fd24f0
M2_CANONICAL_VOICE_SETUP_TABLE_SHA256 = f2faaed8530fb82933e3c30b7537190ba150c38e00b82eee83517ac818089ad5
```

These are identity-gated: any drift = regression. They reflect the
M2.4 lowering rules + voice setup table ABI; both held through the
M2.5 driver bug fixes (driver bytes shifted, but the bytecode
*content* and the voice-setup-table *layout* are independent
artifacts).

## Tick / size scalars

```
M2_CANONICAL_SEQUENCE_BYTES         = 49        (8 SEQ2 header + 41 payload)
M2_CANONICAL_SEQUENCE_TOTAL_TICKS   = 249       (sum of WAIT operands)
M2_CANONICAL_SEQUENCE_ELAPSED_TICKS = 254       (M2.8: wall-elapsed under SPEC §14.3)
M2_CANONICAL_MAX_WRITES_PER_TICK    = 4         (budget 24; ~17% utilization)
```

The `total_ticks` field stays at 249 (sum-of-WAIT-operands) for
back-compat with the M2.4 baseline. The `total_elapsed_ticks` field
(added M2.8) is the wall-elapsed tick count under SPEC §14.3
semantics — each WAIT consumes one extra "resume tick" past the
n decrement ticks, so 5 WAITs in the canonical fixture add 5 to
the elapsed total: 249 + 5 = 254. END opcode reads on tick 254.

## Bytecode hex dump

8-byte SEQ2 header + 41-byte payload + END byte = 49 bytes total.

```
SEQ2 header: 53 45 51 32 02 00 29 00
                                ^^^^^ bytecode_len_le ($0029 = 41)
                          ^^^^^ reserved = 0
                       ^^ bytecode_version = 2
             ^^^^^^^^^^ magic = "SEQ2"

Step 0 init: 10 01 01                                  ; SET_SRC v=1, src=1
             11 01 00 66                               ; SET_VOL v=1, l=0, r=102
                                                       ;   ($66 = constant-power R at 0.8 vol × 1.0 atom vol)
             12 02                                     ; KON mask=0b10
             01 78                                     ; WAIT 120

Step 1 fade: 20 01 00 00 04                            ; VOL_SLIDE v=1, target (0,0), 4 ticks
             01 04                                     ; WAIT 4
             13 02                                     ; KOFF mask=0b10
             01 01                                     ; WAIT 1 (mandatory gap before SET_SRC on a sounding voice)
             10 01 02                                  ; SET_SRC v=1, src=2
             11 01 00 00                               ; SET_VOL v=1, 0, 0
             12 02                                     ; KON mask=0b10
             20 01 00 66 04                            ; VOL_SLIDE v=1, target (0,102), 4 ticks
             01 04                                     ; WAIT 4
             01 78                                     ; WAIT 120 (sustain)

End:         00                                        ; END
```

## Voice setup table hex dump

22 bytes — 11 bytes per voice, voices 0 and 1.

Per SPEC §15.7 byte map per voice:

```
byte 0  voice (informational; driver hard-codes addressing)
byte 1  src_index           ($FF = unused)
byte 2  pitch_l
byte 3  pitch_h
byte 4  vol_l
byte 5  vol_r
byte 6  adsr1
byte 7  adsr2
byte 8  gain
byte 9  flags_reserved      (= 0 in M2)
byte 10 pad_reserved        (= 0)
```

Canonical fixture (LEFT sample voice 0, RIGHT atom voice 1):

```
voice 0:  00 00 00 10 7f 00 00 00 7f 00 00
          ^^ voice = 0
             ^^ src_index = 0 (lead sample)
                ^^^^^ pitch = $1000 (1.0× root rate)
                      ^^ vol_l = 127 (hard left)
                         ^^ vol_r = 0
                            ^^^^^ adsr1=0, adsr2=0 (GAIN mode)
                                  ^^ gain = $7F (raw level full)
                                     ^^^^^ flags_reserved=0, pad_reserved=0

voice 1:  01 01 00 10 00 7f 00 00 7f 00 00
          ^^ voice = 1
             ^^ src_index = 1 (sine_128 atom)
                            ^^ vol_l = 0
                               ^^ vol_r = 127 (hard right)
                                                  (rest mirrors voice 0)
```

## Source

The fixture is synthesized programmatically by
`app/tests/cli.rs::write_v2_combined_for_m25_gate` (canonical
single-fixture form) and the test
`core/tests/sequence_compile.rs::end_to_end_compile_sequence_canonical_byte_pinned`
(byte-pinned round-trip). The same shape underlies the
two-distinct fixtures used by
`cli_compile_sfc_two_distinct_m2_swap_audible_end_to_end`.
