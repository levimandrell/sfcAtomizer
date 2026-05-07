# SFC Wave Compiler — Status

## Current milestone

**M2.6 — `.sfc` parity + m2-acceptance shipped.** New
`compile-sfc` v2 multi_voice_atom path (Phase A): mirrors
the M2.5 SPC compile pipeline (encode + atoms + voice setup
+ sequence + driver build + pack + module write), then
embeds the resulting `module.bin` into the same M1.6 LoROM
shell. Single-project clone mode, two-project mode, and
mixed v1/v2 / sample_basic+multi_voice combinations all
flow through the new dispatcher in
`core::sfc_export::compile_module_dispatched`. v1
sample_basic + v2-sample-only output is byte-identical to
the M1.6 baseline.

New per-channel oracle path on `verify-sfc-modules-audible`
(Phase D): driver_version is detected statically from the
embedded driver bytes (the `mov $f6, #imm` ready-signature
write), routing each module to either the legacy mono
gate (M1) or the new SPEC §21 stereo gate (M2). Per-channel
metrics + driver_version land in
`SfcModulesAudibleReport.module_a_audible` /
`module_b_audible`.

New `m2-acceptance` four-stage bundle (Phase E): mirror of
`m1-acceptance` for v2 projects. Stages are
**validation** (project schema), **compile** (compile-spc
per project), **oracle** (verify-spc-stereo per .spc with
source-step windows), and **infrastructure** (doctor +
compile-sfc + verify-sfc-structure +
verify-sfc-modules-audible). All stage reports + a
unified `bundle.json` land under `--out`. Behavioral
gates only (per the M2.5 policy); identity SHAs are
informational.

Loader byte-identity carry-forward: `m1_loader_65816.asm`
re-assembles to 588 bytes with SHA-256
`955f525c5304af5aea1b53fe12a1b0b469dc9a86bd74dff50c3ffb381b873f40`,
locked by `loader_byte_identity_at_m2_6`. Sentinel-collision
guard against the SPC700 driver's `$DE $AD $BE $EF` pattern
also clean.

32 KiB module cap (SPEC §15.6) near-boundary tests
(Phase B): three CLI tests exercise just-under (31 782 B),
near-boundary (32 763 B; 5 below the cap), and over (32
790 B → ModuleTooLarge hard error). The strict `> 32 768`
comparison in `core::module_writer` accepts `≤ 32 768`
exactly.

Two-distinct M2 swap end-to-end (Phase F): two distinct
multi_voice projects (one sine_128→sine_64 source-step,
one sustained sine_64) compile to a single SFC, both
modules clear per-channel audibility floors,
`modules_audio_identical = false`, and module A's
source-step zcr ratio ≥ 1.5 while module B's stays
< 1.5.

**M2.7 next.** GUI atom / sequence editing. PM to brief.

## Last pass

**Pass M2.6 — `.sfc` parity + m2-acceptance bundle.**

- **Phase A (compile-sfc v2 dispatch):**
  `core::sfc_export::compile_module_dispatched` routes
  projects to the right compile-module path based on
  schema version + driver profile. New
  `compile_module_v2_multi_voice` mirrors the M2.5
  compile-spc flow but emits `module.bin` instead of
  `.spc`. App-side `prepare_sfc_input` recognizes v2
  multi_voice and lets the path through unmodified; v1 /
  v2 sample-only continue through the existing v1-shim
  for byte-identity. New `SfcExportError::V2Compile`
  variant for typed v2-pipeline failures (voice_setup,
  sequence_compile).
- **Phase B (32 KiB cap near-boundary tests):** three CLI
  tests under `cli_compile_sfc_v2_module_*_32_kib_*` —
  90/24576 → 31 782 B (under), 96/24784 → 32 763 B
  (5 B from cap), 97/24576 → 32 790 B (over → error
  exit). Boundaries determined empirically because BRR
  encoding rounds frame counts up to 16-frame block
  boundaries (9 B per BRR block).
- **Phase C (loader byte-identity carry-forward):**
  `loader_byte_identity_at_m2_6` builds a fresh M1 SFC,
  extracts the loader bytes via the report's
  `loader_size_bytes` field, asserts `588 B` exactly +
  pinned SHA-256, and scans for the SPC700 driver-end
  sentinel pattern.
- **Phase D (verify-sfc-modules-audible stereo
  extension):** new `detect_driver_version_from_aram`
  scans the reconstructed ARAM `[0x0200..0x1200]` window
  for `8F xx F6` byte triples (the `mov $f6, #imm`
  ready-signature write). M1 modules report `driver_version=1`
  and run the legacy mono gate; M2 modules report
  `driver_version=2`, populate `report.left` / `right`
  with `PerChannelMetrics`, and pass when at least one
  channel clears the audibility floor (hard-panned voices
  intentionally silent on their off-side). Top-level
  stderr summary now prints `v2 stereo L=...{rms=...}
  R=...` for M2 modules and `v1 mono ...` for M1.
  `AudibleVerificationReport` gained three optional
  fields (`driver_version`, `left`, `right`) with serde
  `skip_serializing_if = "Option::is_none"` so M1 reports
  are still legible.
- **Phase E (m2-acceptance bundle):** new
  `cmd_m2_acceptance` four-stage aggregator. Each stage's
  sub-reports land under `--out`, plus a unified
  `bundle.json` with stage-level rollup statuses + key
  metrics (driver SHAs, SPC SHAs, per-channel metrics,
  module audibility). Bundle exit code = 2 on any stage
  fail; stderr summary mirrors `m1-acceptance`.
- **Phase F (two-distinct M2 swap end-to-end):**
  `cli_compile_sfc_two_distinct_m2_swap_audible_end_to_end`
  builds two distinct multi_voice projects sharing a
  WAV (so LEFT is comparable across modules); compiles
  both into one SFC; runs verify-sfc-modules-audible;
  asserts `modules_audio_identical = false`; runs
  per-project verify-spc-stereo with source-step
  windows; asserts module A's right-channel zcr ratio
  >= 1.5 and module B's stays < 1.5.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **488 tests
  workspace-wide** (was 482 at M2.5; +6 net delta —
  three boundary tests, one loader test, one swap-end-to-end
  test, one m2-acceptance test).

### M2.6 baselines locked

```
M1_LOADER_SIZE_BYTES               = 588                                ; M1.6 baseline; M2.6 re-asserted
M1_LOADER_SHA256                   = 955f525c5304af5aea1b53fe12a1b0b469dc9a86bd74dff50c3ffb381b873f40
                                    ; locked by loader_byte_identity_at_m2_6
M2_MODULE_CAP_BYTES                = 32768                              ; SPEC §15.6 hard cap; > 32768 errors
M2_CANONICAL_MODULE_BYTES          = 9458                               ; canonical M2 fixture this build (informational)
M2_CANONICAL_SFC_BYTES             = 262144                             ; LoROM minimum (256 KiB); same as M1
M2_MIN_SFC_FILE_BYTES              = 262144                             ; unchanged from M1.6
M2_DRIVER_VERSION                  = 2                                  ; embedded in `mov $f6, #$02` ready-signature write
```

Behavioral gates from M2.5 carry forward unchanged
(audibility floors `max_abs >= 1000`, `rms >= 200`;
silence ceiling `<= 50`; source-step zcr ratio `>= 1.5`).

### Decisions log additions (M2.6)

- `compile-sfc` dispatches v2 multi_voice_atom through the
  M2.6 path; sample_basic / v1 / v2-sample-only continue
  through the M1.6 path byte-identically.
- 32 KiB module cap enforced at `compile-sfc` with
  explicit `ModuleTooLarge` error; near-boundary tests
  cover the threshold.
- Loader byte-identical to M1.6 (locked via byte-equality
  test against `M1_LOADER_SIZE_BYTES = 588`).
- `verify-sfc-modules-audible` auto-detects driver_version
  from the embedded driver and dispatches mono (M1) vs
  stereo (M2) gates.
- `m2-acceptance` ships as the M2 analog of
  `m1-acceptance`; four-stage structure with `bundle.json`
  aggregator. Behavioral gates only; identity SHAs are
  informational.
- Two-distinct M2 swap pipeline validated end-to-end
  through the audio path.

## Previous passes

**Pass M2.5 — Multi-voice atom driver in `.spc` + M2.5 prelude.**

- **Layer 1 (block-M2 fixes, `core::sequence_compiler`):**
  Transitive capability dependency check at the top of
  `compile_sequence` — surfaces missing transitive deps as
  `CapabilityMissing { feature: "<dep> (transitive dep of <feature>)" }`.
  SET_SRC bounds-check: `src_index` over 255 errors with
  `SrcIndexTooLarge`; total source pool over 256 errors with
  `TooManySources`. Six new unit tests covering the per-feature
  transitive missing path, canonical-passes-after-check
  invariant, and SRCN bounds.
- **Layer 2 (SPEC / STATUS hygiene):** Five small patches —
  §5.2 mandatory-core wording fix (`core_pitch_table` no
  longer required for compile-time-seeded profiles); §21
  M2 oracle render frame count = 160 000 (5.0 s) with the
  pre-step (ticks 80..=120) / post-step (ticks 130..=249)
  windows; §14.3 slide first-write timing + last-write
  coincides-with-next-opcode prose; rounding-mode comment
  alignment in `driver_build::playback_to_voll_volr`; M2.4
  baseline retired in place to `_CURRENT` with `Supersedes:`
  block convention documented.
- **Layer 3 Phase A (`m2_multi_voice_atom.asm`):** Full SPC700
  driver — direct-page state at $00..$15 (token / status /
  seq_ptr / wait_counter / active_voice_mask / slide block /
  current_vol per voice); 22-byte voice-setup-table walk via
  ($20)+y indirect-indexed, skipping voices with
  `src_index = $FF`; T0 timer init (`T0TARGET = $85`);
  `process_tick` running slide-advance → wait-decrement →
  opcode-read in SPEC §14.3 order; 8-opcode interpreter using
  `cmp ; bne <skip>; jmp <handler>` chains (SPC700 relative
  branches are ±127 B); integer-Bresenham slide arithmetic
  via `mul ya / div ya, x` with carry-propagation through Y.
  Ends at `db $de, $ad, $be, $ef` sentinel.
- **Layer 3 Phase B (`core::driver_build::build_m2`):**
  `DriverConstantsM2` carries master vol / source-dir page /
  echo block / `flg_running` / `status_flags_initial` /
  `voice_setup_addr` / `sequence_addr` / `init_kon_mask`.
  `compute_constants_m2` derives `init_kon_mask` from
  `tracks[].kind == SampleSustain` (atom voices KON'd by
  bytecode). `render_constants_inc_m2` emits the .inc with
  `sequence_addr` shifted past the 8-byte SEQ2 header so the
  driver reads the payload directly. `build_m2` runs asar
  with the constants, slices through the sentinel, enforces
  the 4 KiB driver-code budget, and scans for sentinel
  collisions.
- **Layer 3 Phase C (`compile-spc` v2 dispatch):** New
  `cmd_compile_spc_v2_multi_voice` — validates v2, encodes
  samples + atoms, builds the voice-setup table, compiles
  the active sequence to SEQ2 bytecode, shadow-packs to learn
  voice_setup_addr / sequence_addr, builds the M2 driver,
  real-packs with driver bytes, wraps as .spc. `compile-spc`
  dispatches to this path when `driver.profile ==
  multi_voice_atom`; sample_basic continues through the M1
  shim.
- **Layer 3 Phase D (per-channel oracle harness):** New
  `verify-spc-stereo` and `m2-channel-acceptance` CLI
  commands. `OracleStereoReport` carries per-channel
  metrics + optional pre/post source-step windows.
  `per_channel_metrics` computes max_abs / rms / ZCR per L/R
  from interleaved s16le PCM. `m2-channel-acceptance` runs
  the three-fixture suite and emits a unified report with
  `gate: ok|error` and the SPEC §21 audibility / silence /
  source-step ratio findings.
- **Layer 3 Phase E (driver-build tests):** Four new
  driver_build integration tests pinning the M2 path —
  `build_driver_m2_assembles_within_budget` (deterministic
  build + budget check); `compute_constants_m2_init_kon_mask_only_sample_voices`
  (sample on v0 + atom on v1 → mask = 0b01);
  `render_constants_inc_m2_sequence_addr_skips_seq2_header`
  (lo/hi bytes pin `region_start + 8`); and
  `build_driver_m2_clears_koff_before_kon_in_op_kon` (counts
  the encoded `KOFF = $00 / KON = mask` pairs and asserts
  the op_kon body's clear precedes its KON).
- **Layer 3 Phase F (CLI tests):** Two new CLI tests
  exercising the full `.spc` → oracle path —
  `cli_verify_spc_stereo_combined_passes_spec_21_thresholds`
  (single fixture, asserts L max_abs ≥ 1000 + rms ≥ 200 + R
  same) and `cli_m2_channel_acceptance_canonical_suite`
  (three-project suite, asserts gate=ok and source-step zcr
  ratio ≥ 1.5). The retired
  `cli_compile_spc_on_v2_with_atoms_errors_with_m25_pending`
  is replaced by `cli_compile_spc_on_v2_with_atoms_succeeds_at_m25`,
  which now asserts a 66 048-byte .spc + driver bytes
  starting at `8F 6C F2`.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. 482 tests workspace-
  wide (was 471 at M2.4).

### M2.5 baselines locked

Canonical M2.5 fixtures (synthesized by the new CLI tests
in `app/tests/cli.rs::write_v2_combined_for_m25_gate` and
`cli_m2_channel_acceptance_canonical_suite`):

```
M2_DRIVER_CODE_BUDGET_BYTES        = 4096                                ; (unchanged from M2.0)
M2_DRIVER_CODE_OBSERVED_BYTES      = 1158                                ; this build, asar resolves locally
M2_DRIVER_INIT_KON_MASK_RULE       = OR over t in tracks where t.kind == sample_sustain of (1 << t.voice)

M2_SPC_BYTES                       = 66048                               ; standard SPC file (header + ARAM + DSP regs + ext)
M2_VERIFY_SPC_STEREO_FRAMES        = 32000                               ; 1.0 s at 32 kHz, used by the per-fixture CLI test
M2_CHANNEL_ACCEPTANCE_FRAMES       = 160000                              ; 5.0 s at 32 kHz, used by SPEC §21 canonical render
M2_AUDIBILITY_MAX_ABS_FLOOR        = 1000                                ; SPEC §21
M2_AUDIBILITY_RMS_FLOOR            = 200                                 ; SPEC §21
M2_SILENT_CHANNEL_MAX_ABS_CEIL     = 50                                  ; SPEC §21
M2_SOURCE_STEP_ZCR_RATIO_FLOOR     = 1.5                                 ; SPEC §21 (post.right.zcr / pre.right.zcr)
```

`m2-channel-acceptance` on the canonical suite (sample_only.v1
sine LEFT, atom_only.v2 sine_128 RIGHT, combined.v2 sample
LEFT + atom-sequence RIGHT with `fade_to_zero_retrigger`
source-step from sine_128 → sine_64) passes all seven §21
gates.

The exact .spc / ARAM / driver SHAs are not pinned literally
— they shift with asar version + snes_spc git pin and aren't
load-bearing. Tests pin behavior (audibility floors, silence
ceilings, source-step ratio) rather than identity.

### Multi-voice ARAM (post-M2.5)

```
M2_MULTI_VOICE_ARAM_SHA256_CURRENT = 8d59dabe2b9dd25cd06c09321b4872dbe639609096566fdda221ce4dfa488c04
                                  ; canonical combined fixture (1 sample LEFT,
                                  ; 2 sine atoms with source-step, voice 1 RIGHT);
                                  ; documentary only, no test pins this literal.

Supersedes:
- M2.4 pre-driver-fixes layout: 18861b8ad076ca170ca1d3d80cc73c8998c00c057d190b5164d17d243d216722
  (driver bytes shifted at M2.5: +KOFF-clear-before-KON in op_kon, +init_kon_mask
  KON write, +explicit jmp main_loop after init; sequence_addr now points at
  payload not header)
- M2.3 pre-sequence layout: b25f8d5e2c9579c603e5e06017e3f12540d9913b017c926636e76864435fb13e
  (retired at M2.4 when sequence_data region was added between source directory
  and sample BRR pool; kept for archaeology only)
```

## Previous passes

**Pass M2.4 — Sequence bytecode compiler v0 + M2.4 prelude.**

- **Phase 0 (SPEC prelude, 9 edits):** §15.7 voice byte map
  with explicit byte offsets + `src_index=$FF` unused-voice
  sentinel as binary ABI; §14.3 WAIT execution model
  pinning the per-tick step order (slide-advance → wait-
  decrement → opcode-read until WAIT/END/budget); §14.3
  slide accumulator state with the integer-Bresenham
  round-half-AWAY-from-zero formula; §20.2 status-flags M2
  8-bit map locked (voice0_active / voice1_active /
  echo_enabled / stopped / reset_to_ipl_pending /
  timing_overrun / bytecode_error / write_budget_exceeded);
  §20.2 "ticks, not seconds" rule; §5.4 dependency graph
  promoted to a canonical sub-subsection; §5.4
  `core_tick_loop` polling-only-profile rule; §5.2
  mandatory-core line reconciled; §21 v2-sample-only bit-
  identity invariant promoted from STATUS to spec. Companion
  code fix: `CapabilityManifest::sample_basic()` drops
  `core_tick_loop` from the feature set, with the
  `sample_basic_does_not_claim_core_tick_loop` regression
  test.
- **Phase A/B (`core::sequence_compiler`):** SPEC §14.3
  lowering rules implemented as `compile_sequence`. First-
  step `initial_kon` → SET_SRC / SET_VOL / KON / WAIT.
  Subsequent-step `fade_to_zero_retrigger` → VOL_SLIDE
  (fade-out, ticks=fade_out_ticks) / WAIT / KOFF / WAIT 1
  (mandatory gap before SET_SRC on a sounding voice) /
  SET_SRC / SET_VOL 0,0 / KON / VOL_SLIDE (fade-in,
  ticks=fade_in_ticks) / WAIT / WAIT (sustain). END ($00)
  terminator. Output wrapped with the 8-byte SEQ2 header.
  vol_l / vol_r derived from `atom.playback.pan` and
  `step.target_volume * atom.playback.volume` via the
  shared constant-power `playback_to_voll_volr` helper.
  No SET_PITCH in default lowering — voice setup table
  seeds it.
- **Phase C (capability manifest enforcement):** First
  load-bearing capability check in the project. Compiler
  rejects when `core_sequence_wait` /
  `synth_atom_sequence` / `synth_source_step` (if
  fade_to_zero_retrigger present) / `volume_slide` (same)
  / `sample_runtime_src_change` (multi-step) are absent.
  Single-step pure-`initial_kon` sequences compile under
  fewer features. Returns `SequenceCompileError::CapabilityMissing`.
- **Phase D (per-tick write budget):** Walk the lowered
  bytecode tick-by-tick maintaining slide intervals; count
  DSP writes per tick (active-slide=2, KON=1, KOFF=1,
  SET_SRC=1, SET_VOL=2, VOL_SLIDE=0). For the canonical
  fixture, the observed max is **4 writes/tick** against
  a 24-write budget, far below ceiling. Compiler errors
  with `WriteBudgetExceeded` if the budget is breached on
  any tick.
- **Phase E (one-slide enforcement):** Maintain interval
  list `[tick_start, tick_end)` per active slide; sort and
  scan for overlap. Errors with `OverlappingSlides`
  describing both intervals. Slide tick-window is
  `[bytecode_read_tick + 1, ...)` — the slide is registered
  on the bytecode-read tick but its first write is on the
  NEXT tick (per SPEC §14.3 execution model: slide-advance
  runs before opcode-read).
- **Phase F (`SequenceCompileReport`):** Standard
  `{ schema_version, report_type }` envelope plus bytecode
  SHA-256, payload + total + region byte counts, max
  writes/tick estimate vs budget, total ticks, voice-setup
  / sequence-data ARAM addresses (filled by pack).
  Round-trip tests cover populated + minimal shapes.
- **Phase G (pack integration):** `cmd_pack_v2_multi_voice`
  resolves `m2.active_sequence_id`, builds the source-
  directory view, calls `compile_sequence`, and threads
  the bytecode through `pack_v2`'s `sequence_data` slot.
  After `pack_v2` returns, the post-pack
  `SequenceCompileReport` is written alongside the existing
  outputs, populated with the actual ARAM addresses.
  Sample-only-equivalent v2 packs unchanged (preserves the
  M2.1 bit-identity invariant — verified by
  `cli_pack_v2_sample_only_matches_v1_aram_sha`).
- **Phase H (`sfcwc compile-sequence` CLI):** New
  standalone command. Loads + validates v2, looks up
  active sequence (or `--sequence-id`), compiles, writes
  `.seq.bin` and structured report. Exit codes: 0 success,
  1 IO/parse, 2 project-invalid / sequence-not-found /
  capability-missing, 3 compile error (budget / overlap /
  too-large).
- **Phase I (tests, +18):** 11 sequence_compiler unit
  tests (byte-pinned canonical bytecode, KOFF+WAIT1+SET_SRC
  pattern preserved, no SET_PITCH in default lowering,
  capability gating per missing feature, dep graph
  edge-by-edge, determinism, total ticks invariant, max
  writes/tick≤4, invalid first/last step transition,
  unknown atom_id). 2 SequenceCompileReport round-trip
  tests. 4 sequence_compile integration tests (canonical
  byte-pinned, end-to-end pack with sequence, no-active-
  sequence pack, voice-setup-table byte-pinned ABI). 5
  atom edge-case tests (8 partials deterministic,
  amplitude=0 silent, unnormalised clamp safety, phase-
  cycles same-input-same-output, cycle 256 render). 4 CLI
  integration tests (compile-sequence happy path,
  missing-active error, unknown-id error, pack with
  sequence emits SEQ2 region). **470 tests across the
  workspace; all green.** Per-file new-test breakdown:
  sequence_compiler +6, sequence_compile +4, cli +4, atom
  +5, capability_manifest +1, report +2, plus assorted small
  deltas in pack_v2 / packer / project / project_v2 — gross
  adds ≈ +27 across the M2.4 prelude + Phase I, with a few
  M2.3-era scaffolding tests retired in the same window
  (consultant M2.4 #4 narration fix, applied at M2.7).
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green.

### M2.4 baselines locked

Canonical M2 sequence fixture (1 sample voice 0 panned
LEFT, 2 sine atoms voice 1 panned RIGHT, 2-step sequence
`initial_kon atom_a` then `fade_to_zero_retrigger` to
`atom_b` with fade_out=4 / fade_in=4 / duration=120 /
target_volume=0.8 / atom volume=1.0 pan=+1.0):

```
M2_CANONICAL_SEQUENCE_BYTECODE_SHA256 = f9fa6ea85a7197b662a3b386c9606bb25954708b9714629de7069c90f0fd24f0
M2_CANONICAL_SEQUENCE_BYTES           = 49                                  ; 8 header + 41 payload
M2_CANONICAL_SEQUENCE_TOTAL_TICKS     = 249                                 ; 120 + 4 + 1 + 4 + 120
M2_CANONICAL_MAX_WRITES_PER_TICK      = 4                                   ; budget 24, 17% utilization
M2_CANONICAL_VOICE_SETUP_TABLE_SHA256 = f2faaed8530fb82933e3c30b7537190ba150c38e00b82eee83517ac818089ad5
```

Bytecode hex dump (header + payload + END):

```
SEQ2 header: 53 45 51 32 02 00 29 00
Step 0 init: 10 01 01                                  ; SET_SRC v=1, src=1
             11 01 00 66                               ; SET_VOL v=1, l=0, r=102 ($66 = constant-power R at 0.8 vol)
             12 02                                     ; KON mask=0b10
             01 78                                     ; WAIT 120
Step 1 fade: 20 01 00 00 04                            ; VOL_SLIDE v=1, target (0,0), 4 ticks
             01 04                                     ; WAIT 4
             13 02                                     ; KOFF mask=0b10
             01 01                                     ; WAIT 1 (mandatory gap)
             10 01 02                                  ; SET_SRC v=1, src=2
             11 01 00 00                               ; SET_VOL v=1, 0, 0
             12 02                                     ; KON mask=0b10
             20 01 00 66 04                            ; VOL_SLIDE v=1, target (0,102), 4 ticks
             01 04                                     ; WAIT 4
             01 78                                     ; WAIT 120 (sustain)
End:         00                                        ; END
```

Voice setup table for the canonical fixture (22 bytes; LEFT
sample on voice 0, RIGHT atom on voice 1):

```
voice 0:  00 00 00 10 7f 00 00 00 7f 00 00            ; SRCN 0, pitch $1000, vol_l=127 / vol_r=0
voice 1:  01 01 00 10 00 7f 00 00 7f 00 00            ; SRCN 1, pitch $1000, vol_l=0   / vol_r=127
```

### Multi-voice ARAM (M2.4 snapshot — superseded by M2.5)

Same fixture as the M2.3 baseline plus the new
sequence_data region between source directory and sample
BRR pool. The added region shifts the sample/atom/voice-
table addresses by 256 B (one sequence_data page). Driver
bytes shifted again at M2.5; see "Multi-voice ARAM
(post-M2.5)" above for the current value.

**Convention.** When a baseline is superseded, replace in place with a `_CURRENT` suffix and add the retired value(s) under a `Supersedes:` block. Tests pin the `_CURRENT` SHA; old SHAs are documentation only.

Region order (M2.4):

```
$0000-$00EF  direct_page         (240 B,  fixed_runtime)
$00F0-$00FF  hardware_io         (16 B,   fixed_hardware)
$0100-$01FF  stack               (256 B,  fixed_runtime)
$0200-$11FF  driver_code         (4096 B)
$1200-$12FF  source_directory    (256 B; 3 entries × 4 B + page pad)
$1300-$13FF  sequence_data       (256 B; 49 SEQ2 + 207 page pad)
$1400-$148F  sample_brr_pool     (144 B)
$1490-$14FB  synth_atom_pool     (108 B)
$14FC-$1511  voice_setup_table   (22 B)
$1512-$FEFF  free                (59886 B)
$FF00-$FFBF  ipl_rom_safe_pad
$FFC0-$FFFF  ipl_rom_shadow
```

### v1↔v2 sample-only invariant — still bit-identical

```
M2_SAMPLE_ONLY_V2_ARAM_SHA256     = 9b14634a339359e837fae7ad65f2c077eb7f545bc6bf5e08de6186d1dda4eaf7
                                  ; identical between v1 pack and migrated-v2 pack of the same project
                                  ; preserved through M2.4 per cli_pack_v2_sample_only_matches_v1_aram_sha
```

## Previous passes

**Pass M2.3 — Multi-source ARAM packer + capability manifest.**

- **Phase A (multi-source packer layout):** New
  `core::packer::pack_v2(PackInputV2)` taking ProjectV2,
  encoded samples + atoms, optional sequence_data + voice
  setup table. M2 region order locked per SPEC §15.5: driver
  ($0200–$11FF, 4 KiB) → source directory ($1200, page-
  aligned, samples-first then atoms in declaration order)
  → optional sequence_data → sample BRR pool → synth atom
  pool → voice setup table → free → echo (top of usable,
  ending at $FF00). For sample-only-equivalent inputs the
  output is byte-identical to the v1 packer (locked by the
  `pack_v2_sample_only_matches_v1_layout` test and the CLI
  `cli_pack_v2_sample_only_matches_v1_aram_sha`).
- **Phase B (atom rendering at pack time):** The pack CLI
  v2 path renders every atom in the project's atom_pool via
  `core::atom::render_to_brr` — same code path as
  `render-atom`, deterministic across runs. Results land in
  `encoded_atoms` with `loop_entry_block: Some(0)`
  (single-cycle convention).
- **Phase C (voice setup table):** New `core::voice_setup`
  module emits a 22-byte M2 table per SPEC §15.7 — 11 bytes
  per voice, two voices: `voice / src_index / pitch_le /
  vol_l / vol_r / adsr1 / adsr2 / gain / flags`. Sample
  tracks pull params from `sample_pool[sample_id]`; atom
  tracks from the first step's atom in
  `atom_pool[steps[0].atom_id]`. Unused voices emit
  `src_index=0xFF` sentinel + zero everything else (driver
  reads the sentinel and leaves the voice silent). Five new
  unit tests cover layout, atom-vs-sample param sourcing,
  unused-voice sentinel, and reserved-flags zeros.
- **Phase D (capability manifest):** New
  `core::capability_manifest` module with two profile
  constructors (`sample_basic`, `multi_voice_atom`) and
  `validate_dependencies()` per SPEC §5.4 dependency graph.
  Feature lists from Appendix A.6 (`synth_static_atom →
  sample_playback + core_source_directory`,
  `synth_atom_sequence → synth_static_atom +
  core_sequence_wait`, `synth_source_step →
  synth_atom_sequence + sample_runtime_src_change +
  core_key_on_delay_safety`, `volume_slide → volume_set +
  core_tick_loop`). Limits per profile: max_music_voices=1
  for sample_basic, =2 for multi_voice_atom; atom + sequence
  limits zero for sample_basic. Seven new unit tests cover
  feature sets, limits, dependency consistency, and
  missing-dep error.
- **Phase E (AramMapReport extensions):** New
  `AramKind::VoiceSetupTable` variant + `AramAtomsSummary`
  / `PerAtomAramEntry` types. Non-breaking additions on the
  existing AramMapReport (`atoms: Option<...>` with
  skip-if-none); existing v1 callers keep working.
  GUI's `color_for_kind` and `module_writer`'s region routing
  match arms updated for the new variant.
- **Phase F (sfcwc pack CLI v2 path):** `cmd_pack`
  dispatches based on the loaded project's profile. v1 /
  v2-sample-only run the existing v1-shim path (M1.4
  bit-identical). v2-multi_voice_atom runs a new
  `cmd_pack_v2_multi_voice` path: encode samples, render
  atoms, build voice setup table, call `pack_v2`, emit
  capability manifest sidecar (`multi_voice_atom`). New
  `--out-capability-manifest` flag; defaults to
  `<out_map_stem>.capability-manifest.json`.
- **Phase G (tests):** 7 new packer unit tests
  (sample-only-matches-v1, multi-voice layout / region
  order, atom source-directory entries, voice table size /
  layout, count + size mismatches). 4 new integration
  tests in `core/tests/pack_v2.rs` (end-to-end multi-voice
  with real atom rendering, atom count mismatch, voice
  table size mismatch, 32 KiB module cap). 3 new CLI tests
  in `app/tests/cli.rs` (v2 multi-voice with capability
  manifest, v2 sample-only with sample_basic manifest, v1↔v2
  ARAM SHA equivalence under pack). **462 tests across the
  workspace; all green.** (was 422; +40, plus 1 ignored
  print-baselines sentinel.)
- **SPEC §15.6 32 KiB module cap:** `ModuleTooLarge`
  promoted to a hard error in `core::module_writer`. Existing
  M2.3 multi-voice fixtures fit comfortably (the
  `pack_module_size_within_32_kib_cap` integration test
  builds a typical 1-sample + 2-atom project and confirms
  the resulting module.bin is well under 32 KiB).
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green.

### M2.3 multi-voice pack baseline

For the canonical M2 multi-voice fixture (1 sample at amp 8000
period 64, 256 frames, plus the canonical 0.75-amp sine atoms
from M2.2 at cycle 128 and 64, multi_voice_atom profile, no
echo):

```
M2_MULTI_VOICE_ARAM_SHA256        = b25f8d5e2c9579c603e5e06017e3f12540d9913b017c926636e76864435fb13e
                                  ; M2.3 pre-sequence layout. Retired at M2.4 (sequence_data
                                  ; region added); see M2_MULTI_VOICE_ARAM_SHA256_CURRENT
                                  ; in the M2.4 baselines section above.
M2_MULTI_VOICE_SAMPLE_BRR_BYTES   = 144
M2_MULTI_VOICE_ATOM_BRR_BYTES     = 108                              ; 72 (atom_a, cycle 128) + 36 (atom_b, cycle 64)
M2_MULTI_VOICE_VOICE_TABLE_BYTES  = 22                               ; 2 entries × 11 bytes per SPEC §15.7
M2_MULTI_VOICE_FREE_BYTES         = 60142
```

Region order (multi_voice_atom):

```
$0000-$00EF  direct_page         (240 B, fixed_runtime)
$00F0-$00FF  hardware_io         (16 B,  fixed_hardware)
$0100-$01FF  stack               (256 B, fixed_runtime)
$0200-$11FF  driver_code         (4 KiB)
$1200-$12FF  source_directory    (3 entries × 4 B + page pad = 256 B)
$1300-$138F  sample_brr_pool     (144 B; 1 sample × 16 BRR blocks × 9 B)
$1390-$13FB  synth_atom_pool     (108 B; atom_a 72 B + atom_b 36 B)
$13FC-$1411  voice_setup_table   (22 B; SPEC §15.7)
$1412-$FEFF  free                (60142 B)
$FF00-$FFBF  ipl_rom_safe_pad
$FFC0-$FFFF  ipl_rom_shadow
```

Voice setup table hex dump (locked):

```
voice 0:  00 00 00 10 5a 5a 00 00 7f 00 00
voice 1:  01 01 00 10 48 48 00 00 7f 00 00
```

Voice 0 = sample track on SRCN 0, pitch $1000 (32 kHz at root
key), VOLL/VOLR = 0x5A (constant-power center at volume 1.0),
GAIN = $7F. Voice 1 = atom track on SRCN 1, pitch $1000, VOLL/
VOLR = 0x48 (constant-power center at volume 0.8), GAIN = $7F.
ADSR1/ADSR2 zero (GAIN-only envelope), flags reserved = 0.

For sample-only-equivalent v2 packs (matching M1.4 layout
byte-for-byte), the same fixture as M2.1's bit-identity demo
produces:

```
M2_SAMPLE_ONLY_V2_ARAM_SHA256     = 9b14634a339359e837fae7ad65f2c077eb7f545bc6bf5e08de6186d1dda4eaf7
                                  ; identical to v1 pack of the same project per
                                  ; cli_pack_v2_sample_only_matches_v1_aram_sha
```

## Previous passes

**Pass M2.2 — Synth atom v0 compiler.**

- **Phase A (`render_to_pcm`):** SPEC §16.9 single-cycle
  additive synth implemented in `core::atom`. f64 throughout
  (f32 partial sums aren't deterministic across architectures).
  Round-half-AWAY-from-zero per the spec — for `s = -0.5`,
  rounds to `-1`, not `0` (the round-half-up the pitch
  register uses). Module comments document the distinction.
- **Phase B (`render_to_brr`):** Chains `render_to_pcm`
  through `encode_looped` with `loop_entry_block_index =
  Some(0)` so the BRR's loop-flag is set on the final block
  and the S-DSP returns to block 0 on KOFF. Block 0 is also
  the first block, so `force_filter_0_first_block` covers
  both the kick-on safety and the loop-entry filter-0
  requirement at once. SHA-256 over PCM bytes (interpreted as
  s16 LE) and BRR bytes for downstream determinism gates.
  `AtomRenderError` reserved as an empty enum (render is
  pure-math infallible at M2; BRR encode is internally
  well-formed for valid atoms; future variants land alongside
  M3+ phase rotation / spectral scoring).
- **Phase C (`AtomRenderReport`):** New report type in
  `core::report` with the standard `{ schema_version,
  report_type }` envelope plus PCM/BRR SHAs, the embedded
  `EncodeSummary` (M1.3 type, now `Serialize/Deserialize`),
  and atom metadata. Round-trip tests cover populated +
  minimal shapes.
- **Phase D (`sfcwc render-atom`):** New CLI command. Loads
  the project via `load_project_versioned`, rejects v1 with
  the migrate-project hint, validates v2, finds the atom by
  `--atom <id>` (lists available IDs on miss), runs
  `render_to_brr`, writes BRR bytes to `build/m2/atoms/<id>.brr`
  and the structured report next to it. Optional `--out-pcm`
  writes raw s16le PCM. Exit codes: 0 success, 1 IO/parse,
  2 v1/atom-not-found/invalid, 3 render error (currently
  impossible).
- **Phase E (`sfcwc preview-atom`):** New CLI command.
  Renders the atom (same path as `render-atom`), decodes the
  BRR via `core::brr::decode_blocks`, repeats the cycle to
  fill `--duration-seconds` at 32 kHz, writes a 32 kHz mono
  PCM16 WAV under `<project_dir>/.sfcwc-preview/atoms/<id>.audition.wav`.
  Engineer-side audition only; not committed to the repo.
- **Phase F (tests):** 17 new core/lib unit tests covering
  the SPEC §16.9 render formula (single fundamental, phase
  offset, two-partial sum, normalize-then-scale, round-half-
  away-from-zero on negative half-values, all three cycle
  lengths, deterministic across calls), the BRR chain
  (round-trip envelope at M1-reference amp, filter-0-forced
  on block 0, encoded-byte-count = cycle/16*9, loop-click
  baseline match for both 64- and 128-sample, render-to-BRR
  determinism, distinct SHAs across cycle lengths, locked
  PCM/BRR SHA baselines). 2 new `AtomRenderReport`
  round-trip tests. 4 new core/integration tests in
  `core/tests/atom_render.rs`. 6 new CLI integration tests
  in `app/tests/cli.rs` (happy path, deterministic across
  runs, distinct 64-vs-128 BRR, v1 project errors with
  migrate hint, missing atom id lists available, preview
  WAV writes). **422 tests across the workspace; all
  green.** (was 395; +27, plus 1 ignored sentinel for
  printing fresh baselines.)
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green.

### M2 atom-render baselines (post-M2.2)

Canonical sine atoms (`amplitude=0.75` per SPEC §16.9
example, `partial.amplitude=1.0`, `phase_cycles=0.0`,
`force_filter_0_first_block=true`):

```
M2_ATOM_128_SINE_PCM_SHA256       = 7f9b274e9fa1c7088ba4d125a2899293bae79115bdd20824b2afb54116f9789a
M2_ATOM_128_SINE_BRR_SHA256       = 348c791449916e1f9169d0e229cd79bf97967b19e22db3c4a5be7dc9c69ac876
M2_ATOM_128_SINE_BRR_BYTES        = 72                                ; 8 BRR blocks
M2_ATOM_64_SINE_PCM_SHA256        = 0638ddfe8a2a8fb4c98ff6fed37ff3475c42dd257df893eca9e836d09d3e6565
M2_ATOM_64_SINE_BRR_SHA256        = 78da253b65a6a8d067102fe30ed90353c25b6981a71e3cafc6dd4f3041822e96
M2_ATOM_64_SINE_BRR_BYTES         = 36                                ; 4 BRR blocks
M2_ATOM_128_SINE_LOOP_CLICK_SCORE = 1197                              ; |first_loop_decoded - last_decoded|,
M2_ATOM_64_SINE_LOOP_CLICK_SCORE  = 2407                              ; M2.2 BRR encoder, force_filter_0_first_block
```

Loop-click scores are properties of the discrete-cycle wrap
(sample N-1 of a discrete sine `sin(2πk/N)` is non-zero, so
the wrap from N-1 back to 0 creates a non-zero
discontinuity). Lower than the M1 sample baseline
(`M1_LOOP_CLICK_SCORE = 795`) only because that fixture's
last sample lands closer to zero by coincidence; not a
quality-comparison baseline. M2.5/M2.6 oracle-loop-click
metric will gate against these locked values.

Locked by the in-module `m2_atom_render_baselines_locked`
test plus `core/tests/atom_render.rs::render_canonical_atoms_match_locked_sha_baselines`.
Any future change to the render formula, the M1 BRR encoder,
or the rounding mode that alters these SHAs is a
producer-side regression and must be flagged.

## Previous passes

**Pass M2.1 — Project schema v2 implementation + audition cleanup.**

- **Phase 0 (audition cleanup):** PM auditioned all three
  M2.0-postlude WAVs and the underlying `.spc` / `.sfc`
  artifacts on real Mesen2; M1 driver bootstrap-token fix
  (consultant #1) and loader ack-token + ack-code fix
  (consultant #10) confirmed working in real-hardware
  emulation. Audible periodic loop click at the fundamental
  cycle rate (~4 Hz for 8192-sample loops at 32 kHz)
  observed and expected per consultant finding #9. Locked
  `M1_LOOP_CLICK_SCORE = 795` in the post-M2.0 baselines
  block as the future M2.5/M2.6 oracle-loop-click metric
  baseline. Removed `audition/m2.0-postlude/` (audit
  artifact, not part of the long-term repo).
- **Phase A (`ProjectV2::validate`):** SPEC §16.9 rules
  26–57 implemented in `core::project_v2`. Carry-forward
  rules 1–25 reuse a new free function
  `core::project::validate_sample_slot` (extracted from the
  previous `ProjectV1::validate` method) so v1 + v2 share
  one source of truth for sample-slot rules. The
  `ValidationErrorKind` enum extended with v2-specific
  variants (atom partials, transitions, tracks, the
  cross-pool ID collision, profile / bytecode coupling,
  sample-basic-vs-multi-voice cross-cuts).
- **Phase B (`migrate_from_v1`):** Pure transformation per
  SPEC §16.10. schema_version 1 → 2; project / driver /
  master_echo / sample_pool carried forward; atom_pool +
  atom_sequences added empty; a single voice-0
  sample_sustain track `track_sample_0` mapped from
  `m1.active_sample_id`; `m1` dropped; `m2` added with
  `active_sequence_id: null`. Caller validates the result.
- **Phase C (v2 IO):** `ProjectV2::load_from_path /
  load_and_validate / save_to_path` mirror the v1 surface;
  `save_to_path` is validation-gated (refuses to write an
  invalid v2 project). v2 round-trip is byte-stable across
  save → load → save.
- **Phase D (versioned dispatch):** `LoadedProject::{V1, V2}`
  enum + `load_project_versioned(path)` reads JSON, peeks
  `schema_version`, dispatches. Other versions return
  `ProjectIoError::UnsupportedSchemaVersion`. No silent
  upgrades.
- **Phase E (`sfcwc migrate-project` CLI):** New subcommand
  with `--in / --out / --migration-report` flags. Exit
  codes 0 success, 1 IO/parse, 2 v1-validation OR
  already-at-v2, 3 post-migration v2 validation. Writes
  the migrated project + a structured `MigrationReport`
  next to it (`<out_stem>.migration-report.json` by
  default). `validate-project` now dispatches by
  `schema_version` so v1 and v2 projects validate through
  the same CLI surface. `compile-spc / compile-sfc / pack`
  gain a `prepare_v1_input` helper: v1 passes through;
  sample-only-equivalent v2 (empty atom data, all tracks
  sample_sustain on voice 0) is shimmed to a synthetic v1
  in a tempdir with absolute audio paths so the existing
  v1-only pipeline produces bit-identical output; v2 with
  atom data errors with the M2.5-pending message.
- **Phase F (GUI v1/v2 dual-load):** The eframe shell now
  dispatches File → Open through `load_project_versioned`.
  Two-variant state (`Option<ProjectV1>` /
  `Option<ProjectV2>`); the loaded schema appears in the
  status bar. File → Save writes back as the loaded
  version (no auto-migrate). New File → Migrate v1 → v2…
  menu item runs the explicit migration, writes a sibling
  migration report, and hot-reloads as v2; the v1 file is
  untouched. v2 detail panel renders read-only Atom Pool /
  Atom Sequences / Tracks sections; v2 sample entries are
  shown read-only (editing arrives at M2.7). Compile +
  Verify buttons work for v2 sample-only via the same
  in-memory v1 shim used by the CLI.
- **Phase G (tests):** 51 new core/lib v2 tests — positive
  baseline + targeted negative tests for each new
  validation rule (26–57, with cross-coupling cases for
  rule 30 sample/atom ID collision and rule 54 track-vs-
  sequence voice match). v2 round-trip byte-stability
  test. Migration round-trip stability (v1 → v2 → save →
  reload → save must be byte-identical). Migration field-
  mapping correctness vs SPEC §16.10. `MigrationReport`
  shape test. `load_project_versioned` dispatch tests for
  each schema version + unknown-version rejection. 5 new
  CLI integration tests in `app/tests/cli.rs` covering
  migrate-project happy path, already-v2 input,
  corrupted-v1 input, the round-trip migrate-then-compile-
  spc bit-identical baseline, and the v2-with-atoms error.
  **395 tests across the workspace; all green.** (was 347;
  +48.)
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --all-targets`, `cargo test --workspace`
  all green.

### M2.1 demo (this host)

Generated a canonical 8192-sample 8000-amp period-64 sine,
imported into a v1 project, migrated to v2, ran compile-spc
on both, and confirmed bit-identical output:

```
v1 driver_code = 671ee21ebb207302940075519e1ad0de557a97280038ab12aef7a22994b2bcfe
v2 driver_code = 671ee21ebb207302940075519e1ad0de557a97280038ab12aef7a22994b2bcfe
v1 aram_image = 8727dcbe6561a8af730096b18e0e6e746cccd03c47131ced6f975ed503d6068a
v2 aram_image = 8727dcbe6561a8af730096b18e0e6e746cccd03c47131ced6f975ed503d6068a
v1 spc_file   = 912ac507d62d90db93a42ff177fd842b7d6a65018f0b0f1453601a1531b09312
v2 spc_file   = 912ac507d62d90db93a42ff177fd842b7d6a65018f0b0f1453601a1531b09312
```

(SHAs differ from the locked M1 baselines because that
fixture has loop enabled at start_sample=0 / end_sample=8192 /
snap=brr_block_16; the demo here uses an auto-imported,
non-looped sine. The CLI integration test
`cli_migrate_project_then_compile_spc_matches_v1_baseline`
in `app/tests/cli.rs` covers the bit-identity claim
end-to-end against the CLI-built fixture.)

A v2 project with non-empty atom data correctly errors:

```
compile-spc: v2 project v2_atoms.json has atoms or atom
sequences. atom rendering lands at M2.2; sequence
compilation at M2.4; multi-voice driver at M2.5. v2
projects with atoms or sequences cannot yet be compiled.
Use a sample-only v2 project, or stay on v1 until M2.5
ships.
exit=2
```

## Previous passes

**Pass M2.0 — Contracts freeze + M1 hotfix.**

- **Phase A (consultant #1):** M1 driver
  `core/fixtures/asm/m1_sample_basic.asm` now seeds
  `dp_last_token` from `$F4` at init via `mov a, $f4 ; mov
  $00, a` (encoded `E4 F4 C4 00`) before writing the
  ready signature. Pre-fix, the driver wrote zero into the
  token slot, then read `$F4` in main loop — the IPL exec
  path leaves the kick byte on `$F4`, so the first poll
  fired the invalid-command path and stomped the ready
  signature with `$EE`, tripping the loader's
  `wait_driver_ready` timeout. Driver size: 324 → 325 bytes.
- **Phase B (consultant #10):** 65816 loader's
  `command_reset_to_ipl` wait loop now verifies BOTH the
  ack code (`$82` on `$2141`) AND the round-trip token
  (`$42` on `$2140`). Stale acks from a prior driver run
  no longer pass the gate. Loader size: 581 → 588 bytes.
- **Phase C (consultant #3):** New
  `core::audio::check_or_refresh_source_hash` enforces the
  declared `source.sha256` on every compile path. New
  `--refresh-source-hash` flag on `compile-spc`,
  `compile-sfc`, `pack` updates the project in place when
  the user explicitly asks. New
  `AudioDecodeError::SourceHashMismatch` variant.
- **Phase D:** Baseline rebase. New constants locked under
  `M1 baseline locked SHAs (post-M2.0 rebase)`. M1
  acceptance still passes end-to-end with
  `bundle.status=ok`. All audible numbers unchanged
  (max_abs=11072, rms=5519.6) — confirms the fixes only
  changed the driver/loader instruction stream, not the
  audio output.
- **Phase E (consultant #7):** `core::driver_build` post-
  slice scan catches the case where the chosen sentinel is
  inside the driver and the real `driver_end` is later in
  the image (the failure mode silently truncates the
  driver). New `DriverBuildError::SentinelCollision`. The
  current M1 driver does not collide.
- **Phase F:** 8 STATUS items promoted into SPEC: asar
  invocation split (§17), M2 packer policy (§15.5),
  module.bin 32 KiB cap (§15.6), M2 fixture-asset path
  rule (§16.6), loader fail-mode colour codes (§19.2.1),
  module v1 magic stability (§19.4), spin-count semantics
  + ack-verification rule (§19.2), audible threshold floor
  + sfc verification scope clarification (§21), oracle
  process-boundary rule (§17).
- **Phase G:** 7 M2 architectural contract sections from
  Appendix A added to SPEC: §5.4 capability manifest M2
  extensions + enforcement rule, §14.3 sequence bytecode
  v2 (`SEQ2`) opcodes/operand lengths/source-step
  lowering, §15.7 voice setup table byte format, §16.9
  project schema v2 + atom v0 design, §16.10 migration
  v1→v2, §20.2 driver `multi_voice_atom` profile (T0
  timer, init/main loop, zero-page state, status-flags
  reservation), §21 M2 acceptance per-channel thresholds
  + source-step observability.
- **Phase I:** Cheap Rust type skeletons. `core::bytecode`
  locks the SEQ2 opcode bytes + region header. `core::atom`
  declares `AtomSlot` / `AtomKind::AdditiveSingleCycleV0` /
  `AtomPartial` / `AtomRenderOptions`; `render` body is
  `todo!()`. `core::project_v2` declares the full v2 type
  tree (`ProjectV2`, `AtomSequence`, `AtomSequenceStep`,
  `AtomTransition` tagged-union, `Track` /
  `TrackKind` tagged-union, `M2Block`); `validate` and
  `migrate_from_v1` are `todo!()`. All round-trip cleanly
  through serde.
- **Phase J:** Regression tests for each M2.0 fix. **347
  tests across the workspace** (was 326; +21).
  Token-bootstrap regression locks the new bytes order;
  sentinel-collision regression confirms a synthetic
  collision trips `SentinelCollision`; source-SHA tests
  cover intact / drifted / refreshed paths.

`cargo check`, `cargo fmt --check`, `cargo clippy
--all-targets`, `cargo test` all green.

### M1 baseline locked SHAs (post-M2.0 rebase)

For the canonical one-sample reference project (8192-frame
8000-amp sine, 32 kHz, echo off, GAIN=127, single-project
clone mode for the `.sfc` swap):

```
M1_DRIVER_CODE_SHA256  = 671ee21ebb207302940075519e1ad0de557a97280038ab12aef7a22994b2bcfe
M1_DRIVER_CODE_BYTES   = 325                                ; was 324 pre-rebase
M1_ARAM_IMAGE_SHA256   = 336a6745d0930816ec59a18cd6b5c45ed2f1ed0cb3962621e56ebf8d142bfaff
M1_SPC_FILE_SHA256     = 264b1eff2dc2fee7d4e36be6f5c3924123d3307eddb059dc04583bae871c4d8e
M1_SFC_FILE_SHA256     = 263b230a652e0fe05157b0f6c89b1099ac2f8b413802b81ab2c3bba3b1a5610e
M1_MODULE_A_SHA256     = 456b5f806af384efdc03f07274ce3c4b51cc6437933acd9af8552c2efb7f79cb
M1_LOADER_SIZE_BYTES   = 588                                ; was 581 pre-rebase
M1_AUDIBLE_MAX_ABS     = 11072                              ; unchanged from M1.7
M1_AUDIBLE_RMS         = 5519.6                             ; unchanged from M1.7
M1_AUDIBLE_THRESHOLDS  = min_max_abs=1000, min_rms=200      ; frozen at M1.7
M1_LOOP_CLICK_SCORE    = 795                                ; |first_loop_decoded - last_decoded|,
                                                            ; M1.3 BRR encoder, canonical
                                                            ; 8192-sample 8000-amp period-64
                                                            ; sine, loop_start=0,
                                                            ; force_filter_0_first_block=true.
                                                            ; Locked at M2.1 as the future
                                                            ; M2.5/M2.6 oracle-loop-click
                                                            ; baseline (consultant #9).
```

Pre-rebase commit (M1.7 baseline): `4bf286f` (top of
`docs: STATUS — M1 complete; lock baseline SHAs;
consolidate audition queue`).

### M2.0 user audition result (M2.1 phase 0)

PM auditioned all three M2.0-postlude WAVs and the
underlying `.spc` / `.sfc` artifacts on real Mesen2.
Outcome: M1 driver bootstrap-token fix (consultant #1) and
loader ack-token + ack-code fix (consultant #10) both
confirmed working in real-hardware emulation. Audible
periodic loop click at the fundamental cycle rate (~4 Hz
for 8192-sample loops at 32 kHz) observed and expected per
consultant finding #9 — the loop-click oracle metric is
deferred to M2.5/M2.6 and `M1_LOOP_CLICK_SCORE = 795` is
locked above as its baseline.

`audition/m2.0-postlude/` removed in M2.1 phase 0 (audit
artifact, not part of the long-term repo).

## Previous passes

**Pass M1.7 — M1 acceptance bundle.**

- Phase A: `M1Manifest` + `M1BundleSummary` + `M1BundleSteps`
  in `core::report`. Mirrors the M0.6 design (Status enum
  reused). Carries cross-reference SHAs (aram / spc / sfc /
  module_a / module_b / driver_code), audible max_abs values
  for both .spc and .sfc paths, `modules_audio_identical` flag.
- Phase B: `core::manifest::verify_m1_bundle` reads the bundle
  from disk and cross-checks: `compile_spc.spc_file_sha256 ==
  audible_spc.spc_sha256`, `compile_sfc.module_a_in_file_sha256
  == structure_sfc.module_a.in_file_sha256`, `compile_sfc.sfc_path
  ↔ structure_sfc.sfc_path`, schema version consistency. The
  legacy `read_typed` helper was generalised to a trait-based
  `read_typed_with` so M0 and M1 verifiers share the path.
- Phase C: `sfcwc m1-acceptance` chains the M1 sub-commands by
  spawning the sfcwc binary for each step (failure-as-data —
  every step writes a report regardless of outcome). Reads
  reports back to compute `M1BundleSteps`; runs `verify_m1_bundle`
  to populate diagnostics; writes `manifest.json`. Always exits 0;
  bundle status reflects the run.
- Phase D: `sfcwc m1-status` read-only summary. Exit 0 on
  bundle status `ok`/`degraded` AND clean integrity; exit 1
  otherwise (drift detection mirrors `m0-status`).
- Phase E: SPEC §21 M1 acceptance bullet expanded with the
  required-vs-optional step list, doctor mapping, frozen
  thresholds, and cross-reference invariants. §23 question 1
  partially resolved (voice/module audible-render thresholds
  frozen; atom-quality remains open for M3+).
- Phase F: STATUS — milestone marked complete; M1 baseline
  SHAs locked under named constants; Awaiting-user-audition
  section consolidated.
- Phase G: 5 new `verify_m1_bundle` unit tests + 2
  `M1Manifest` round-trip tests + 5 CLI integration tests
  (`m1-acceptance` one-project, two-projects, `m1-status`
  valid / missing / corrupted). **326 tests across the
  workspace; all green.**

### M1 acceptance gate result on this host

```
m1-acceptance: bundle.status=ok; wrote 9 reports + manifest -> bundle\manifest.json
```
- `doctor`: ok (asar resolved on PATH; oracle resolved via env;
  Mesen2 missing — informational only, doesn't downgrade)
- `validate_a`: ok
- `validate_b`: skipped (no project B)
- `compile_spc`: ok — driver 324 B, image 65 KB, spc 66 KB
- `audible_spc`: ok — max_abs=11072, rms=5519.6
- `compile_sfc`: ok — sfc 256 KB, module_a 9048 B, clone-of-a
- `structure_sfc`: ok — 0 findings
- `audible_sfc`: ok — A.max_abs=11072, B.max_abs=11072, identical=true

All cross-reference SHA invariants pass; integrity bools all
true. Mangling any one report's SHA correctly trips
`m1-status` to exit 1 with a finding.

## Awaiting user audible audition

When you're back at your desktop with a working Mesen2 install:

- **`<project>.spc`** from `compile-spc` (or
  `bundle/project_a.spc` from `m1-acceptance`): sustained sine,
  no clicks, no silence. Audible immediately on Mesen2 load.
- **`<project>.sfc`** from `compile-sfc` (or
  `bundle/project.sfc` from `m1-acceptance`): ~5 seconds of
  sine → brief gap (RESET_TO_IPL handshake) → ~5 seconds of
  sine again (module B = clone of A in single-project mode) →
  repeats. With two distinct projects, modules A and B should
  sound different.
- **On Mesen2 load failure**: copy any error dialog text and
  report.

## Previous passes

**Pass M1.6 — `.sfc` test ROM + module swap.**

- Phase 0: `core::tools::resolve_mesen2` gains a PATH fallback
  per PM authorization. Order: `SFCWC_MESEN2` env → PATH lookup
  for `Mesen.exe` / `Mesen2.exe` / `Mesen` (POSIX: `Mesen` /
  `Mesen2` / lowercase) → missing. SPEC §17.1 table updated.
  New sentinel-based unit test exercises the PATH branch.
- Phase A: New `core::module_writer` — converts a packed ARAM
  image + map into a `module.bin` per SPEC §19.4. Sparse-block
  format, header bytes `$00..$40`, self-zeroed SHA-256 in
  `$20..$40`, block table starts at `$40`. Skips runtime/
  free/IPL-pad regions; emits zero-filled echo buffer block
  only when echo is enabled. Round-trip helpers
  `parse_module_header` / `parse_module_blocks` /
  `project_blocks_to_aram` for verifiers.
- Phase B: New `core/fixtures/asm/m1_loader_65816.asm` — 65816
  loader. Native mode setup, force-blank, IPL upload protocol
  (canonical fullsnes interpretation), driver-ready wait,
  ~5-second idle, RESET_TO_IPL (SPEC §20.1) command, second
  upload, infinite spin. Fail-mode background colors: red =
  IPL timeout, green = driver-ready timeout, blue = command
  ack timeout, white = IPL byte-ack timeout. Assembles to
  ~580 bytes; pads ROM to 256 KB to match the LoROM header
  size byte.
- Phase C: New `core::sfc_export`. Compiles project A (and
  optional project B) through the encode → driver_build →
  packer → module_writer pipeline; assembles the embedded
  loader source via the existing `AsarBackend`; embeds modules
  at fixed bank offsets ($01:8000, $02:8000); re-fixes the
  LoROM checksum after embedding (asar's pre-embed checksum
  goes stale once the module bytes land). When only project A
  is provided, module B is a clone of A so the swap mechanism
  still gets exercised. `AsarBackend::AssembleInput` now
  carries `expected_output_size` + `extra_args` so the SFC
  path can request 256 KB output and `--fix-checksum=on`
  (opposite of the SPC700 `--fix-checksum=off`).
- Phase D: `sfcwc compile-sfc` CLI — load+validate, encode,
  driver-build, pack, write modules, assemble loader,
  embed, re-checksum. Exit 0/1/2/3/4/5 across the failure
  modes. `CompileSfcReport` carries SFC SHA + per-module SHAs
  + loader size + clone flag.
- Phase E: `sfcwc verify-sfc-structure` — power-of-two file
  size, mode byte = `$20`, country = `$01`, ASCII title,
  checksum + complement = `$FFFF`, reset vector ≥ `$8000`,
  embedded `module.bin` parses, blocks sorted ascending,
  in-file SHA matches recomputed. Exit 0 on ok, 2 on any
  finding.
- Phase F: `sfcwc verify-sfc-modules-audible` — for each
  module, parse blocks → project to 64 KB ARAM → wrap as M1
  SPC → render via snes_spc oracle → compute max_abs/rms.
  Reports `modules_audio_identical` (SHA on rendered PCM) so
  single-project clone runs are explicitly recognized.
- Phase G: GUI Compile SFC + Verify SFC buttons in the
  project detail panel; both run in-process. Status badge
  (green/red/gray) reflects last verify outcome.
- Phase H: 3 round-trip tests for the new report types, 7
  module_writer unit tests, 6 CLI integration tests
  (compile-sfc happy path, two-projects, invalid project,
  verify-sfc-structure pass/corruption, verify-sfc-modules-
  audible distinct/silent). 1 new tools-resolve unit test
  for the Mesen2 PATH fallback. **314 tests across the
  workspace; all green.**

### M1.6 baseline-locked SHAs

For a one-sample project (8192-frame 8000-amp sine, 32 kHz,
echo off, single-project clone mode):

```
M1_6_SFC_FILE_SHA256        = fca07fb5f505f1f6e74c4e37a89d576d81ab94a75878131b12406c1f38f1b2ce
M1_6_MODULE_A_SHA256        = d138f81fe1c23f5340a426d3e08b3e79e365e7d787bc6cd522f1a3082dc0da86
M1_6_MODULE_B_SHA256        = (= M1_6_MODULE_A_SHA256, single-project clone)
M1_6_LOADER_SIZE_BYTES      = 581
M1_6_SFC_SIZE_BYTES         = 262144
```

Audible cross-check via oracle: both modules render to
`max_abs=11072, rms=5519.6` (matching M1.5 baseline exactly,
since the same project's ARAM round-trips through module.bin
unchanged), `status=ok`, `modules_audio_identical=true`.

### Mesen2 self-check (engineer)

Mesen2 PATH fallback works as designed. On this host the user
has `C:\tools\Mesen2\` containing `MesenCore.dll` (and other
files) but **no `Mesen.exe`** in the directory. The directory
is also not on PATH for the engineer process. Doctor reports
mesen2 missing, which is accurate. **User audible audition is
queued for return** — load `build/m1/<name>.sfc` (or
`<project_dir>/.sfcwc-build/<name>.sfc` from the GUI) into a
proper Mesen2 install and listen for: sustained sine ~5
seconds, brief gap (RESET_TO_IPL), sustained sine again
(module B = clone of A), repeats.

## Previous passes

**Pass M1.5 — First real SPC700 driver + audible `.spc`.**

- Phase A: `core/fixtures/asm/m1_sample_basic.asm` — sample_basic
  driver per SPEC §20.1. Init writes FLG mute first, configures
  master vol, DIR, all echo regs (FIR + ESA + EDL before EON),
  then voice 0 (vol/pan/pitch/SRCN/ADSR/GAIN), then unmutes,
  sets dp state, KONs voice 0, writes the `$A5 $5A $01
  status_flags` ready signature. Polling main loop dispatches
  STOP / RESET_TO_IPL / PING / invalid commands. Sentinel
  `$DE $AD $BE $EF` after `driver_end` for length detection.
  Same asar mapper trick as M0.3 (`lorom + arch spc700 + org
  $008200 + base $0200`).
- Phase B: New `core::driver_build` module — generates
  `m1_constants.inc` from project + map, writes scratch
  `.asm` + `.inc` to a tempdir, invokes asar via the existing
  `AsarBackend`, locates the sentinel pattern in the 64 KB
  output, slices driver bytes, bound-checks against
  `DRIVER_CODE_BUDGET_M1` (4 KiB), SHA-256s the result. Driver
  source `include_str!`'d into the core crate so the host has
  no runtime workspace-layout dependency. Per-project constants
  cover voice 0 (constant-power pan formula, M1.0 pitch register,
  ADSR/GAIN tagged-union register mapping), master volume
  (hard-pinned to `$7F` for M1 — schema doesn't expose a
  master-vol field yet), source-directory page (from map),
  echo (FLG ECEN bit, ESA from map, EDL/FIR/EFB/EVOL from
  master_echo, EON gated by per-voice + master).
- Phase C: `compile_aram_image` orchestrates encode → shadow
  pack → driver_build → real pack. Both `sfcwc pack` and the
  new `sfcwc compile-spc` consume it. `--driver` flag on `pack`
  remains as an optional override for testing; default is the
  real driver.
- Phase D: New `sfcwc compile-spc` CLI subcommand. Loads +
  validates project, runs the full pipeline, builds an SPC v0.30
  via the new `core::spc::build_m1_image` (M1 contract: PC=$0200,
  GPRs=0, SP=$EF, DSP regs all zero — driver writes FLG=$60 mute
  on its first instruction). Writes image, map, SPC, and a
  `CompileSpcReport` carrying every SHA. Exit codes 0/1/2/3/4.
- Phase E: New `sfcwc verify-spc-audible` CLI subcommand. Renders
  the SPC through the snes_spc oracle, recomputes max_abs/rms in
  Rust (defensive, mirrors M0.5 calibrate-oracle pattern), writes
  an `AudibleVerificationReport` with `status: ok | silent_fail
  | oracle_error`. Exit 0 ok, 1 IO/oracle error, 2 silent_fail.
- Phase F: GUI "Compile SPC" + "Verify Audible" buttons in the
  project detail panel. Both run in-process. Compile-SPC writes
  to `<project_dir>/.sfcwc-build/<name>.spc`; Verify-Audible
  spawns the oracle and shows max_abs/rms inline.
- Phase G: 14 driver_build unit tests (pan formula, envelope
  mapping, FLG bits, status-flag bits, pitch, src-dir page,
  active-sample resolution, missing-source error, .inc text
  shape) + 5 integration tests (real asar invocation,
  determinism, sample-index encoding, asm-path existence) +
  5 CLI tests (compile-spc happy path, invalid-project,
  verify-audible-ok / silent_fail / oracle-missing) + 2
  round-trip tests for the new report types. **295 tests
  across the workspace; all green.**

### Audible verification result (M1.5 acceptance gate)

```
verify-spc-audible: 16384 frames, max_abs=11072, rms=5519.6,
status=ok; report -> build/m1/<...>.audible-report.json
```

The same project's M0 smoke `.spc` (FLG=$60 mute) correctly
trips `silent_fail` (max_abs=0, rms=0, exit=2).

### M1.5 baseline-locked SHAs

For a one-sample project (8192-frame 8000-amp sine, 32 kHz,
echo off, GAIN=127):

```
M1_5_DRIVER_CODE_SHA256 = 52f9c26bad6df33875f16ae2ef4a6a4e0e5c265e29af74db103b393daef24955
M1_5_ARAM_IMAGE_SHA256  = b384617ff4d45c5da965d27bd74926070eb3b4985fb051bcda25165f7bfb54eb
M1_5_SPC_FILE_SHA256    = edd02b030e1c0f574c4ff1f64dc80f1c643d2ba18628a6f67a35dc93b8759315
```

Driver assembled size: **324 bytes** (out of 4 KiB budget).

### Mesen2 smoke (engineer self-check)

Mesen2 is **not installed on this host** (`SFCWC_MESEN2` unset,
no Mesen executable located). The oracle render is the formal
M1.5 acceptance gate per SPEC §17 / §18 — it confirms the SPC
plays audibly. **User audible audition is queued for return**;
when the user is back at their desktop, opening
`build/m1/<name>.spc` in Mesen2 should produce a sustained
sine tone matching the source (32 kHz at root pitch).

## Previous passes

**Pass M1.4 — ARAM packer v1 + ARAM meter.**

- Phase C: `AramMapReport` extended with four optional M1 meter
  fields — `echo: AramEchoSummary`, `source_directory:
  AramSourceDirSummary`, `samples: AramSamplesSummary`, `warnings`.
  `#[serde(default, skip_serializing_if)]` on all four keeps M0.6
  manifests parseable and round-tripping unchanged.
- Phase A/B/D/E/F: New `core::packer` module — single source of
  truth for M1 region layout. Per-sample BRR pool laid out in
  declaration order at the page after the source directory; each
  source-dir entry packs `(start_addr_le, loop_addr_le)` with
  `loop_addr = start_addr + loop_block * 9` for looped, `=
  start_addr` for one-shots. Returns a `PackOutput` carrying the
  64 KB `aram_image` and the populated `AramMapReport`.
- Phase G CLI: New `sfcwc pack` subcommand. Loads + validates the
  project, decodes audio, encodes BRR via the M1.3 encoder, packs,
  writes image + map. Exit codes 0 / 1 / 2 / 3 for ok / IO / project-
  invalid / pack-error. Stderr summary line lists sample count,
  total BRR bytes, echo state, free %, image SHA, map path.
- Phase G GUI: New ARAM meter view in the project detail panel —
  segmented horizontal bar with one color per region kind, echo
  callout (large text + warning when `writeback_safe=false`),
  numeric region breakdown, warnings list. Recomputes on project
  load, sample edits, loop-candidate apply, and a manual Refresh
  button. Encoder + packer run in-process for sub-second feedback;
  failed recomputes leave the previous meter intact and surface
  the error inline.
- Phase H: 14 packer unit tests (round-trip layout, loop addresses,
  echo math, overflow, alignment, bounds, fixed-region invariance,
  map partition, no-collisions, echo summary consistent with image),
  4 CLI integration tests (happy path, echo overflow → exit 3,
  invalid project → exit 2, default paths), 5 round-trip tests for
  the new report struct types + a pre-M1.4 backwards-parse test.
  269 tests across the workspace.
- Adjacent fix: `core::import` was treating bare-filename project
  paths (no slash) as having parent `Some("")`, which then failed
  to canonicalize and surfaced as a confusing "path traversal
  refused: " message. Now folds empty-parent and `None`-parent both
  to "." consistently.

`cargo check`, `cargo fmt --check`, `cargo clippy --all-targets`,
`cargo test` all green.

### Spec ambiguity flagged

The brief's "echo_end = 0xFFC0" math gives an ESA-misaligned
`echo_start = 0xDFC0` for EDL=4: `ESA*0x100` would point at
`0xDF00`, leaving 0xC0 bytes of the buffer unaddressable. The
correct M1-conservative interpretation is `echo_end = 0xFF00`
(largest page boundary at or below the IPL ROM shadow). The 192
bytes between `0xFF00` and `0xFFC0` are reported as
`ipl_rom_safe_pad`. ESA values in the brief's example (e.g.
`ESA=$DF` for EDL=4) are correct; only the start-address arithmetic
needed re-derivation.

## Previous passes

**Pass M1.3 — Loop selection + BRR encoder + audition WAV.**

- Phase A: `core::audio::decode_to_mono_pcm` wired through
  symphonia for WAV / AIFF / AIFC; chains the existing M0.2
  decoder for BRR. Symphonia stores every PCM sample
  left-aligned in i32 (i16 → `<< 16`, i24 → `<< 8`); we always
  recover i16 with `>> 16`. Stereo collapses to mono via
  per-frame average. Frame count cross-checked against probe.
- Phase B: New `core::brr_encoder` module — exhaustive per-block
  `(filter, shift)` search over 0..=3 × 0..=12; greedy across
  blocks; round-trip-correct by construction (the encoder's
  analytic predictor walk mirrors `core::brr::decode_block`,
  then re-decodes the produced block via the canonical decoder
  to score). Scoring: peak first, sum-of-squares as tiebreak —
  RMS-only scoring picks shifts that clip on signal peaks, and
  clipping is what makes BRR samples sound distorted.
  `force_filter_0_first_block` (default true) protects against
  uninitialised predictor history at KON; `loop_entry_block_index`
  forces filter 0 at the loop entry block where the predictor
  history at iteration ≥ 2 has no fixed value. Sine-wave
  round-trip peak < 256 LSBs (observed: 55).
- Phase C: New `core::loop_finder` module — windowed RMS
  difference + seam click magnitude, score = rms + 0.25 * click.
  Search restricted to first 25% / last 25%; iteration on
  multiples of 16 when `snap_to_brr_block` is on.
- Phase D: New `core::audition` module — hand-rolled 44-byte
  RIFF/WAVE writer for byte-stable output. Sample data is the
  raw 15-bit decoder output stored in i16; no gain compensation
  applied, so the audition reflects the S-DSP at unity voice gain.
- Phase E: Three new CLI subcommands (`encode-brr`,
  `preview-brr`, `find-loop-candidates`) plus three new report
  types (`BrrEncodeReport`, `LoopFinderReport`, `AuditionReport`)
  on the existing v1 envelope.
- Phase F: GUI gains loop start/end DragValues (auto-snap to
  multiples of 16), Find Loop Candidates button + modal with
  per-row Apply, Preview BRR button (writes to
  `<project_dir>/.sfcwc-preview/<sample_id>.audition.wav`).
  Sample detail panel switched from `&SampleSlot` to
  `&mut SampleSlot`; project re-validates after each edit.
- Phase G: 25 new tests — 10 encoder unit (round-trip,
  silence, force_filter_0, loop_entry, looped flags, alignment),
  6 loop_finder unit (sortedness, range, snap, top-N), 5
  audition unit (header layout, byte-stability, payload
  matches decoder), 4 decode-to-PCM integration (mono, stereo
  mix, zero-fill, BRR), 3 round-trip serde tests for the new
  report types, 3 CLI integration tests for the new
  subcommands. 246 tests across the workspace; `cargo check`,
  `cargo fmt --check`, `cargo clippy --all-targets`, and
  `cargo test` all green.

## Previous passes

**Pass M1.2 — WAV/AIFF/BRR import.**

- Phase A: New `core::audio` module with hand-rolled probes for
  WAV (RIFF/WAVE chunk walk), AIFF / AIFC (IFF FORM chunk walk
  with NONE/sowt compression validation), and BRR (file-size /
  9 = block count). 80-bit IEEE 754 extended-precision sample-
  rate decoder cross-pinned against four pre-computed byte
  tables (8000 / 22050 / 32000 / 44100 Hz). `sha256_of_file`
  streams 64 KiB chunks.
- Phase B: New `core::import` module wires the full pipeline:
  load → probe → SHA → copy (with same-SHA dedup + collision
  suffix) → derive id/name → default `SampleSlot` (root MIDI
  60, loop off, GainRaw 127, vol 1.0, pan 0.0, echo off) →
  append → seed `m1.active_sample_id` on first import →
  validate → save. Path-traversal guard refuses targets that
  resolve outside `<project_dir>/audio/`.
- Phase C: New `sfcwc import` CLI subcommand. Exit codes 0 / 1
  / 2 / 3 for ok / IO-or-project-error / audio-format-rejected
  / resulting-project-invalid (defensive).
- Phase D: New File → Import Audio… menu item in `sfcwc-app`.
  Opens `rfd::FileDialog` with the wav/aif/aiff/aifc/brr
  extension filter, reloads the project on success, surfaces
  errors via the existing status bar.
- Phase E: 12 audio probe tests (all 9 brief cases plus extras
  for AIFC NONE/sowt and the SHA-256 streaming vector), 9
  import pipeline tests (happy path, dedup-by-SHA, collision
  suffix, --no-copy relative path, BRR sample-rate override,
  messy-filename id derivation, fallback id, missing-file,
  unsupported-extension), 3 CLI integration tests (exit 0 / 1
  / 2). Synth helpers in `core/tests/common/mod.rs`; no
  binary fixtures committed. 6 unit tests on the id-derivation
  logic.
- Phase F: LICENSING.md §4.2 documents `rfd` (dual MIT-OR-Apache,
  no obligations).

215 tests across the workspace — 157 core unit + 12 audio probe
integration + 10 BRR fixture integration + 9 import pipeline
integration + 27 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` all green. `rfd` compiled clean on Windows with the
brief's feature set; no fallback to text-input modals required.

## Previous passes

**Pass M1.1 — Project v1 + Sample Pool data model + minimal app shell.**

- Phase A: `ProjectV1::validate` body lands. 25-rule SPEC §16.6
  coverage; multi-error collection (no bail-on-first); errors
  carry JSON-pointer paths. `ValidationError` redesigned from a
  flat enum to `{path, kind: ValidationErrorKind}`. Echo rules
  delegate to `core::echo_validation`. Convention: `name` allows
  spaces and non-ASCII; `id` is `^[a-z0-9_]+$`.
- Phase B: Project I/O. `load_from_path` / `save_to_path` /
  `load_and_validate` / `migrate_from_value` / `new_template`.
  `ProjectIoError` covers NotFound / Io / Parse / MalformedValue /
  UnsupportedSchemaVersion / Validation. Stable byte-for-byte
  round-trip verified by test (struct field declaration order
  preserved through serde).
- Phase C: New `core::report::ValidationReport` JSON envelope
  (status: ok | invalid | io_error; flat `{path, message}`
  errors). New `sfcwc new-project` and `sfcwc validate-project`
  CLI subcommands. Exit 0 / 2 / 1 for ok / invalid / io_error
  respectively. Project file extension chosen: `.sfcproj.json`
  (JSON-honest, project-prefixed).
- Phase D: New `sfcwc-app` GUI binary at `app/src/app_main.rs`.
  ~560 lines eframe/egui. Read-only viewer: File menu, Sample Pool
  list, sample/project detail panel, status bar with
  validation rollup, "Show errors" modal. Empty-pool placeholder
  ("No samples imported yet. (Import lands at M1.2.)"). Hand-
  rolled text-input modal stands in for native file pickers
  pending an authorized picker dep.
- Phase E: 50 new unit tests in `core::project` (every rule
  positive + negative; round-trip byte-stability; migration v1
  vs unsupported; new_template field parity). 5 new CLI
  integration tests. 183 tests total across the workspace.
- Phase F: SPEC §16.4 documents the `id` regex `^[a-z0-9_]+$`,
  the `name` UTF-8 + control-char + path-separator rules, and
  the strict `sha256` shape. SPEC §16.6 replaced with the 25
  rules grouped thematically (Root / project / driver /
  master_echo / sample_pool / Envelope / m1). LICENSING.md
  gains §4 covering eframe/egui (MIT-or-Apache, no obligations)
  and symphonia (MPL-2.0 file-scope copyleft, used unmodified).

`cargo check`, `cargo fmt --check`, `cargo clippy --all-targets
-- -D warnings`, `cargo test` all clean.

## Previous passes

**Pass M1.0 — M1 contracts + cleanup.**

- Phase A (M0 cleanup): SPEC §23 question 2 struck (resolved at
  M0.5/M0.6 — process-boundary oracle, host never links snes_spc).
  STATUS.md: M0 frozen acceptance SHAs promoted from prose into a
  named-constants block; producer-side caveat added to current
  milestone.
- Phase B1 (§16): Project file format restructured into v1 with
  subsections §16.1 format basics, §16.2 root, §16.3 master_echo,
  §16.4 sample_pool entries (source / loop / playback, ADSR-vs-
  GAIN tagged union, constant-power pan mapping formula), §16.5
  m1 block, §16.6 cross-field validation rules, §16.7 pitch +
  MIDI convention, §16.8 migration + acceptance.
- Phase B2 (§17.2 new): Audition path locks decoded-BRR PCM16
  mono WAV at `build/m1/previews/<sample_id>_decoded_brr.wav`.
  Trap: must play decoded BRR, not the original source.
- Phase B3 (§19.2): IPL upload protocol byte-locked. Port direction
  convention, `$BB` ready signature, `$CC` kickoff, `$2141 = 0`
  jump-to-entrypoint, 8-bit-only writes, IRQ/NMI disabled around
  upload, six bounded host-side spin-count constants.
- Phase B4 (§19.4 new): `module.bin` sparse-block format. 64-byte
  header (magic `"SFCWCM1\0"`, schema_version 1, content SHA-256
  with the 32 SHA bytes themselves zeroed as the self-reference
  workaround), 8-byte block entries, block rules forbidding
  `$00F0..$00FF` and preferring `≥ $0200`.
- Phase B5 (§20.1 new): Driver command protocol for the
  `sample_basic` profile. Ready signature `$A5 $5A $01
  status_flags`; status-flags bit map (bits 0–4 used; 5–7 reserved);
  command_token discipline; commands `$01`/`$02`/`$7F`; acks
  `$81`/`$82`/`$FF`/`$EE`.
- Phase B6 (§21 M1): Acceptance bullets refined to cross-reference
  §16.7, §17.2, §19.4, §20.1, the bundle.status semantics from
  M0, and the .spc/.sfc parity rule (allocated regions
  bit-for-bit; free regions not parity-significant in `.sfc`).
- Phase C (Rust skeletons): Five new `core::` modules — `project`
  (full v1 type tree, `validate` is `todo!()`), `module_image`
  (`ModuleHeader` 64 B + `ModuleBlockEntry` 8 B with
  `std::mem::offset_of!` layout tests), `driver_proto`
  (`DriverCommand` / `DriverAck` enums, `StatusFlags` newtype,
  ready-signature constants, host_timeouts), `pitch`
  (real implementation of the §16.7 formula, 10 tests), and
  `echo_validation` (three SPEC §16.6 rules with multi-error
  collection, 8 tests).

133 tests across the workspace — 104 core unit + 10 BRR fixture
integration + 19 app CLI integration. `cargo check`,
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
and `cargo test` all green.

## Previous passes

**Pass M0.6 — Calibration report + M0 acceptance bundle.**

- Phase A: `core::report::M0Manifest` extended with a real
  `BundleSummary` — per-step `BundleSteps` (`StepStatus =
  ok|warnings|error|skipped`), aggregate `BundleStatus =
  ok|degraded|error`, three cross-reference SHAs
  (`aram_image_sha256`, `spc_file_sha256`, `oracle_pcm_sha256`),
  and a flattened `diagnostics` vector capped at 50 entries.
  `#[serde(default)]` on `bundle` keeps M0.4/M0.5 manifests
  parseable with a sentinel `Error` bundle.
- Phase B: New `core::manifest` module with `verify_bundle` that
  re-reads the on-disk manifest + every report and reports
  observed structure (file presence, parse, schema-version
  consistency, ARAM-SHA cross-reference, SPC-SHA cross-reference).
  Observation-only — never asserts.
- Phase C: `cmd_m0_acceptance` rewritten to chain real steps,
  read each report back, map to `StepStatus` per the documented
  per-step rules, aggregate to `BundleStatus`, fold integrity
  findings into bundle diagnostics, and stamp the manifest with
  an RFC3339 `generated_at` (computed inline via Howard
  Hinnant's civil-from-days, no chrono dep).
- Phase D: New `sfcwc m0-status [--bundle <dir>] [--json]` —
  read-only summary of an existing bundle. Re-runs `verify_bundle`
  to catch on-disk drift, prints the per-step rollup, exits 0 on
  `ok`/`degraded` + clean integrity, 1 otherwise.
- Phase E: SPEC §21 acceptance bullets refined to reference
  `bundle.status` and `m0-status`. M0-completion paragraph
  updated.
- Phase F: Six new CLI integration tests cover bundle
  aggregation paths (all-tools, oracle-missing, asar-missing) and
  m0-status (valid, missing, corrupted bundle), plus four
  manifest unit tests for `verify_bundle`. Two new round-trip
  tests for the manifest schema.

91 tests across the workspace — 62 core unit + 10 BRR fixture
integration + 19 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` all green.

`sfcwc m0-acceptance` on this host produces `bundle.status =
degraded` (Mesen2 missing — `SFCWC_MESEN2` not set; bundle is
shippable but flagged for the optional manual smoke tool). The full
locked SHA constants live in the decisions log below.

## Previous passes

**Pass M0.5 — snes_spc oracle wrapper boundary.**

- Phase A: Vendored `snes_spc 0.9.0` from the
  `blarggs-audio-libraries` mirror as an unmodified snapshot under
  `tools/snes_spc_oracle/vendor/snes_spc/`. Pinned commit
  `ec8ee2bbe30451614c1d02a83f7af1c97d497d45` (2020-10-24).
  License files preserved verbatim; provenance documented in
  `vendor/snes_spc/README-vendoring.md`. No upstream files
  modified.
- Phase B: New C++17 wrapper at `tools/snes_spc_oracle/main.cpp`,
  ~290 lines, no third-party dependencies. One subcommand:
  `render --input-spc --frames --output-pcm --report`. Locked CLI
  contract v1. Hand-rolled JSON output, inline FIPS 180-4 SHA-256.
  `spc_clear_echo()` after `spc_load_spc()` for determinism, per
  upstream's recommendation.
- Phase C: Build via CMake (3.16+, C++17). Output binary at
  `tools/snes_spc_oracle/build/Release/snes_spc_oracle.exe` on
  Windows/MSVC. `tools/snes_spc_oracle/build/` ignored by git.
- Phase D: `tools/snes_spc_oracle/README.md` documents the build,
  the locked CLI contract, the LGPL-2.1+ distribution obligations
  the wrapper inherits, and the process-boundary integration model.
- Phase E: `core::report::CalibrationReport` upgraded — replaced
  M0.1 placeholder inner shapes with M0.5 typed shapes
  (`ObservedInfo { voice_render_max_abs_lsb, voice_render_rms_lsb }`,
  same for `ProvisionalTolerances`), added
  `fixture_set: Option<FixtureSetInfo>`, `diagnostics: Vec<String>`,
  `error: Option<String>`. SCHEMA_VERSION stays at 1.
  `core::tools::resolve_snes_spc_oracle` extended to find the
  wrapper at its build output paths.
- Phase F: `sfcwc calibrate-oracle` rewired with `--oracle`,
  `--input-spc`, `--frames`, `--out`, `--out-pcm` flags. Spawns
  the wrapper, reads back PCM, recomputes max_abs/rms in Rust
  defensively, populates a real `CalibrationReport` with
  `status: provisional_not_ci_gate`. Failure-as-data on
  oracle-missing, input-missing, and wrapper-error branches.
- Phase G: `m0-acceptance` chain updated to invoke the real
  calibrate-oracle in step 6 (replacing the M0.4 stub). Oracle
  PCM lands at `<out>/oracle.pcm_s16le`. Three new CLI
  integration tests for oracle-resolved, oracle-missing, and
  input-spc-missing branches.

79 tests across the workspace: 55 core unit + 10 BRR fixture
integration + 14 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` all green. Wrapper builds clean with one harmless
struct/class warning from upstream's vendored code.

Locked observation: rendering the M0.4 muted smoke `.spc` through
the wrapper produces 8,192 zero PCM bytes (max_abs=0, rms=0),
matching the SPEC §19.3 contract. PCM SHA-256 is reproducible
across runs.

## Previous passes

**Pass M0.4 — Minimal `.spc` exporter smoke.**

- Phase A: New `core::spc` module — SPC v0.30 layout constants for
  the 66,048-byte file (header / ARAM / DSP / 64 B unused gap /
  Extra RAM), `SpcCpuState` / `SpcImage` / `SpcBuildError` types,
  and `SpcImage::to_bytes`/`write_to_path`. Layout cross-checked
  against fullsnes and vspcplay; 0x10180..0x101C0 is the canonical
  unused gap, 0x101C0..0x10200 is Extra RAM.
- Phase B: New SPEC §19.3 SPC smoke state contract — PC=$0200,
  GPRs zero, SP=$EF, FLG=$60 (Mute amp + Echo write disable), all
  other DSP regs zero, ID666 indicator absent (0x1B). Mirrored as
  `SMOKE_CPU_STATE`, `SMOKE_FLG`, `smoke_dsp_regs`,
  `build_smoke_image` constants/functions in `core::spc`.
- Phase C: `core::spc::verify_structure` — observation-only parse
  of any v0.30 SPC, returns magic_ok / minor_version /
  id666_present / cpu / per-region SHA-256s. Never asserts; the
  caller decides what's fatal.
- Phase D: New `core::aram` module with
  `map_from_image(&[u8; 65536]) -> AramMapReport`. Walks for
  first/last nonzero byte in `$0200..$FFC0` to identify
  driver_code; everything else is `free`. Documented as a stopgap
  valid for the M0 smoke output; the real packer lands in M3+.
  The new partition invariant: regions sum to total_aram exactly,
  free_bytes equals the sum of `Free` regions.
- Phase E: `SpcExportReport` extended with four new optional
  fields — `input_aram_sha256`, `dsp_state_sha256`,
  `spc_file_sha256`, `error`. Same `skip_serializing_if` pattern
  as M0.3; pre-M0.4 reports still parse.
- Phase F: `sfcwc export-spc-smoke` rewired with `--aram`,
  `--out-spc`, `--verify-structure` flags. Reads driver.bin,
  builds smoke image, writes 66 KB .spc, optionally re-verifies
  structure. ARAM-missing/wrong-size is failure-as-data.
- Phase G: `sfcwc m0-acceptance` chained end-to-end — doctor →
  decode-fixtures → assemble-smoke → export-spc-smoke →
  aram::map_from_image → calibrate-oracle (still stub) →
  manifest. Failures in one step don't halt the chain; the
  manifest still gets produced.
- Phase H: 76 tests passing — 54 core unit + 10 BRR fixture
  integration + 12 app CLI integration. New tests cover SPC byte
  layout, ARAM partition invariant, the export-spc happy/sad
  paths, and a full m0-acceptance chain run.

`cargo check`, `cargo fmt --check`, `cargo clippy --all-targets --
-D warnings`, and `cargo test` all green.

## Previous passes

**Pass M0.3 — Asar backend smoke.**

- Phase A: New `core::asm` module with `AssemblerBackend` trait
  (name, version, assemble) and supporting types — `AssembleInput`,
  `AssembleOutput`, `AssembleError`. Trait keeps the door open for
  WLA-DX without rewiring the build pipeline; M0.3 ships only
  `AsarBackend`.
- Phase B: `AsarBackend::assemble` — pre-creates a 64 KB
  zero-filled scratch file, invokes asar with the locked
  `--no-title-check --fix-checksum=off` form, verifies exact 64 KB
  size, computes SHA-256 of the full image. Asar quirks discovered
  in probing and locked in the module docs: `org $0200` errors
  without `lorom + org $008200 + base $0200`; without
  `--fix-checksum=off` asar writes 4 LoROM checksum bytes into our
  flat ARAM image at `$7FDC..$7FDF`.
- Phase C: `core/fixtures/asm/m0_smoke.asm` — trivial NOP + BRA
  loop yielding sentinel bytes `00 2F FD` at file offset `0x0200`,
  with every other byte zero. Brief sketched `00 2F FE` but the
  actual displacement is `-3` (BRA targets `start: nop` at $0200,
  not the BRA itself), so 0xFD is correct.
- Phase D: Two new optional fields on `AssembleReport` —
  `output_image_sha256` and `error`. Both `skip_serializing_if`,
  so the schema extension is non-breaking; pre-M0.3 reports still
  parse. `SCHEMA_VERSION` stays at 1.
- Phase E: `sfcwc assemble-smoke` rewired to invoke the asar
  pipeline. New `--out-image` flag (default
  `build/m0/driver.bin`); existing `--out` keeps the JSON report
  path. Failure-as-data: asar-missing or asar-error writes a
  populated `AssembleReport` with `status: error` and exit 0.
- Phase F: Two CLI integration tests — asar-present (gated on
  `resolve_asar().resolved`, asserts exit 0, 64 KB image, exact
  sentinel bytes, sha256 in stderr) and asar-missing (forces
  failure via `SFCWC_ASAR=<bogus>` + isolated `PATH`).

53 tests across the workspace — 32 core unit + 10 BRR fixture
integration + 11 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and `cargo
test` all green.

## Previous passes

**Pass M0.2 — Raw BRR decoder + deterministic fixture suite.**

- Phase A: New `core::brr` module — `BrrHeader`, `BrrDecoderState`,
  `decode_block`, `decode_blocks`. Pipeline: 4-bit sign extension,
  shift (with documented 13..=15 path → -2048 / 0), one of four
  filter formulas in i32 integer arithmetic with hardware-matching
  arithmetic-right-shift rounding, optional 16-bit clamp on filters
  2/3, final 15-bit wrap via `int16(s << 1) >> 1`. Six unit tests
  cover header parsing, sign extension, predictor mutation, filter-0
  identity, header-flag isolation, and the shift-13 path.
- Phase B: Nine-fixture corpus under `core/fixtures/brr/` — every
  `expected_pcm` is independent ground truth (hand-walked or
  spec-derived via `_reference.py`, never decoder output). Coverage:
  filter 0 basic, filter 0 shift/clamp incl. shift=12 and shift=13,
  filter 1 zero/nonzero history, filter 2 nonzero, filter 3 nonzero,
  multi-block predictor continuity, SPEC §10.2 loop-entry seeding,
  and END/LOOP-flag isolation. `_reference.py` is a deliberately
  separate Python implementation of the same fullsnes/snesbrr
  formulas, used as a second-source check.
- Phase C: `core/fixtures/brr/README.md` documents the provenance
  discipline ("expected_pcm is not decoder output"), per-fixture
  audit table, the SnesLab vs SNESdev END/LOOP bit-ordering
  disagreement (we follow SNESdev / boldowa-snesbrr), and the rule
  that any change to expected_pcm requires a provenance_notes update.
- Phase D: New `core/tests/brr_fixtures.rs` runs each fixture in its
  own #[test] for individual failure granularity. `core::brr_fixtures`
  module embeds the corpus via `include_str!` and exposes
  `run_fixture` returning `BrrFixtureResult`.
- Phase E: `sfcwc decode-fixtures` now runs the real corpus and emits
  per-fixture pass/fail data with a `9/9 passed; wrote <path>`
  stderr summary. Failed fixtures are data, not process errors —
  command exits 0. Other M0.1 stubs untouched per brief.

44 tests across the workspace — 24 core unit + 10 core fixture
integration + 10 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and `cargo
test` all green.

## Previous passes

**Pass M0.1 — CLI surface, JSON report schemas, tool resolution.**

- Phase A: Workspace deps (`serde`, `serde_json`, `clap`,
  `thiserror`, `sha2`, `tempfile`) declared in
  `[workspace.dependencies]`; per-crate dependency tables wired in
  `core/` and `app/`. Binary name pinned to `sfcwc` via `[[bin]]` in
  `app/Cargo.toml`.
- Phase B: Seven stable report types in `core::report`
  (`DoctorReport`, `BrrFixtureReport`, `AramMapReport`,
  `AssembleReport`, `SpcExportReport`, `CalibrationReport`,
  `M0Manifest`), each carrying the
  `{ schema_version: 1, report_type: "...", ... }` envelope and a
  `stub()` constructor for placeholder bodies. Round-trip serde tests
  guard every shape.
- Phase C: Tool resolution in `core::tools` per SPEC §17.1.
  `resolve_asar`, `resolve_snes_spc_oracle`, `resolve_mesen2` follow
  env → (PATH or workspace-default, per tool) → missing; asar and the
  oracle wrapper are best-effort version-probed via `--version`.
  Mesen2 is env-only with no PATH fallback and no version probe per
  spec.
- Phase D: `sfcwc` CLI binary in `app/src/main.rs` with six
  clap-derive subcommands. `doctor [--json] [--out]` does real
  resolution and emits `DoctorStatus::{Ok, Warnings, Errors}`. Five
  stubs (`decode-fixtures`, `assemble-smoke`, `export-spc-smoke`,
  `calibrate-oracle`, `m0-acceptance`) write valid placeholder
  reports to `build/m0/` (default) or a `--out` path. Errors flow
  through a `thiserror`-derived `CliError`.
- Phase E: 22 tests across the workspace — 12 round-trip unit tests
  in `core` and 10 CLI integration tests in `app/tests/cli.rs`
  exercising every subcommand including the `SFCWC_ASAR=<sentinel>`
  env-resolution path.

**Pass 2.0 — Pre-M0 spec cleanup + M0-readiness additions.**

- Phase A: §21 M0 acceptance reconciled — removed the contradictory
  closing paragraph and three superseded acceptance bullets (snes_spc
  render gate, SPC-player playback, WAV-fixture round-trip).
- Phase B: `.sfc` exporter deferred to M1 alongside the 65816
  SPC-upload contract (SPEC §19.2); Mesen2 added as the manual M0
  `.spc` verification path.
- Phase C: New §17.1 "Tool discovery" subsection; §10.1
  cross-reference typo fixed (snes_spc external validation lives in
  §17 and §18, not §16); §4 architecture diagram cosmetic.
- Phase D: Rust toolchain pinned to the stable channel via
  `rust-toolchain.toml`; `CLAUDE.md` "External tools" section added.

**Pass 1 — Repo hygiene + spec truth.**

- Phase A: SPEC.md unescaped (markdown backslash-escapes and `&#x20;` HTML
  entities stripped; tables/code fences/lists tightened).
- Phase B: 14 consultant-review patches applied (M0 readiness).
- Phase C: Cargo workspace skeleton created (`core/`, `app/`); STATUS,
  CLAUDE, LICENSE, LICENSING docs added; `.gitignore` covering Rust,
  Windows, editors, `build/`, `.claude/`.
- Phase D: `cargo check` and `cargo fmt --check` clean; pushed to `main`.

## Decisions log

### M1 baseline locked SHAs (post-M2.0 rebase)

For the canonical one-sample reference project (8192-frame
8000-amp sine, 32 kHz, echo off, GAIN=127, single-project
clone mode for the `.sfc` swap). M2.0 rebased every SHA after
the M1 driver token-bootstrap and loader ack-verify hotfixes
(consultant findings #1 and #10) altered the driver and
loader instruction streams.

```
M1_DRIVER_CODE_SHA256  = 671ee21ebb207302940075519e1ad0de557a97280038ab12aef7a22994b2bcfe
M1_DRIVER_CODE_BYTES   = 325
M1_ARAM_IMAGE_SHA256   = 336a6745d0930816ec59a18cd6b5c45ed2f1ed0cb3962621e56ebf8d142bfaff
M1_SPC_FILE_SHA256     = 264b1eff2dc2fee7d4e36be6f5c3924123d3307eddb059dc04583bae871c4d8e
M1_SFC_FILE_SHA256     = 263b230a652e0fe05157b0f6c89b1099ac2f8b413802b81ab2c3bba3b1a5610e
M1_MODULE_A_SHA256     = 456b5f806af384efdc03f07274ce3c4b51cc6437933acd9af8552c2efb7f79cb
M1_LOADER_SIZE_BYTES   = 588
M1_AUDIBLE_MAX_ABS     = 11072      ; oracle render unchanged from M1.7 —
                                    ; the hotfixes only changed the
                                    ; driver/loader instruction stream,
                                    ; not the audio output
M1_AUDIBLE_RMS         = 5519.6
M1_AUDIBLE_THRESHOLDS  = min_max_abs=1000, min_rms=200 (frozen at M1.7)
M1_LOOP_CLICK_SCORE    = 795
                       ; |first_loop_decoded - last_decoded| from the M1.3 BRR
                       ; encoder for the canonical 8192-sample 8000-amp period-64
                       ; sine with loop_start=0 and force_filter_0_first_block=true.
                       ; Locked at M2.1 as the future M2.5/M2.6 oracle-loop-click
                       ; metric baseline (consultant finding #9). Audible
                       ; periodic loop click at the fundamental cycle rate
                       ; (~4 Hz for 8192-sample loops at 32 kHz) was confirmed
                       ; in the M2.0-postlude Mesen2 audition; treating this
                       ; level as acceptable until the metric ships.
```

Pre-rebase commit (M1.7 baseline): `4bf286f`. Old SHAs are
preserved in commit history; do not pin against them.

Locked by `m1-acceptance` and re-checked by `m1-status`. Any
future change to the source `.asm`, the BRR encoder, the
packer, the driver_build constants generator, the
module_writer, the SFC export, or the SPC contract that alters
these SHAs is a producer-side regression and must be flagged.

- **Migration is explicit** (M2.1): v1 → v2 migration runs only via
  `sfcwc migrate-project` (CLI) or File → Migrate v1 → v2 (GUI).
  Load-time silent upgrades remain forbidden per SPEC §16.10.
- **v2 sample-only compile path** (M2.1): a v2 project with empty
  `atom_pool`, empty `atom_sequences`, and only voice-0
  `sample_sustain` tracks compiles via the existing v1 pipeline
  (in-memory v1 shim). v2 with atom data errors with the
  M2.5-pending message; atom rendering lands at M2.2, sequence
  compilation at M2.4, multi-voice driver at M2.5.
- **GUI v2 read-only at M2.1:** atom_pool / atom_sequences / tracks
  panels are listings only; editing arrives at M2.7.
- **v2 round-trip byte-stability locked** (M2.1): `ProjectV2::save_to_path
  → load_from_path → save_to_path` produces byte-identical output;
  enforced by the `v2_save_load_round_trip_byte_stable` test.
- **Migration is bit-preservation-clean** (M2.1): compile-spc on a
  migrated sample-only v2 produces the same ARAM / SPC SHAs as
  compile-spc on the original v1. Enforced by the
  `cli_migrate_project_then_compile_spc_matches_v1_baseline` test.
- **Atom render uses round-half-AWAY-from-zero** (M2.2): SPEC §16.9
  rounding mode for atom PCM quantisation. Differs from the
  round-half-up used in pitch-register conversion (§16.7); for
  `s = -0.5` the atom path rounds to `-1`, the pitch-register path
  rounds to `0`. Both modes are correct in their context; the
  distinction matters only when a future caller picks the wrong
  one. Locked by the `render_round_half_away_from_zero_negative`
  test.
- **Single-cycle atoms have block 0 = first block = loop entry**
  (M2.2): the `loop_entry_block_index` passed to the M1 BRR
  encoder is `Some(0)`, and `force_filter_0_first_block` covers
  both the kick-on safety AND the loop-entry filter-0 requirement
  in one pass. Encoder runs at filter 0 on block 0 unconditionally
  for atoms; the filter freedom on subsequent blocks is preserved.
- **`render-atom` emits to `build/m2/atoms/<atom_id>.brr`** (M2.2):
  default output path; M2.3 pack will consume these alongside
  the existing M1 sample BRRs. `--out-brr` overrides; `--out-pcm`
  writes raw s16le bytes for offline inspection.
- **Audition WAVs under `<project_dir>/.sfcwc-preview/atoms/`**
  (M2.2): not committed to the repo. PM doesn't audition this
  pass — the deterministic SHAs are the primary gate.
- **`AtomRenderError` is uninhabited at M2** (M2.2): atom render
  is pure-math infallible (no NaN paths after validated input;
  the optional normalise divide is guarded against zero-max), and
  the M1 BRR encoder is internally well-formed at `loop_start=0`
  for the validated 64/128/256 cycle lengths. Future M3+ work —
  phase rotation, spectral scoring, pre-emphasis — may add
  variants.
- **`f64` throughout the atom render** (M2.2): `f32` accumulating
  partial sums isn't precise enough to make the output deterministic
  across architectures. Locked by the
  `render_deterministic_across_calls` test.
- **Multi-source ARAM layout: samples first, atoms second**
  (M2.3): SRCN ordering follows declaration order WITHIN each kind,
  with all samples preceding all atoms. Cross-pool ID validation
  (rule 30 from M2.1) means sample IDs and atom IDs cannot collide.
  Locked by `pack_v2_atom_source_directory_entries_correct` and the
  end-to-end multi-voice integration test.
- **Atom rendering at pack time** (M2.3): the v2 multi-voice pack
  path renders every atom in `project.atom_pool` via
  `core::atom::render_to_brr` — same code as the standalone
  `render-atom` CLI, deterministic across runs. Sample-only-v2 and
  v1 don't render atoms (atom_pool is empty).
- **Voice setup table always 22 bytes for M2** (M2.3): two voices ×
  11 bytes per SPEC §15.7. Unused voices (single-voice projects)
  emit `src_index=0xFF` sentinel + zeros; driver reads the sentinel
  and leaves the voice silent. Always-22-bytes keeps driver
  constants stable across single- and dual-voice projects.
- **Capability manifest emitted as sidecar JSON** (M2.3): not in the
  ARAM image. Default path
  `<out_map_stem>.capability-manifest.json`; overrideable via
  `--out-capability-manifest`. Per SPEC §5.4 every compile-time entry
  point should consult the manifest, not just `driver.profile`. The
  manifest's `validate_dependencies()` runs at pack time for the
  multi_voice_atom path.
- **32 KiB module.bin cap enforced as compile error** (M2.3):
  `core::module_writer::ModuleTooLarge`, hard error per SPEC §15.6.
  Existing M1 fixtures and M2.3 typical multi-voice fixtures are
  comfortably under the cap; the
  `pack_module_size_within_32_kib_cap` integration test confirms a
  1-sample + 2-atom project produces a sub-32 KiB module.
- **Sample-only v2 packs bit-identical to v1** (M2.3): preserved
  from the M2.1 migration→compile-spc invariant, now extended to
  the pack output directly. `pack_v2` with empty atom_pool / empty
  sequence_data / no voice setup table reduces to the v1 layout
  byte-for-byte. Locked by `pack_v2_sample_only_matches_v1_layout`
  and the CLI-level `cli_pack_v2_sample_only_matches_v1_aram_sha`.
- **Step duration semantics** (M2.4): `duration_ticks` is the
  sustain time at target volume AFTER any transition completes.
  Total step time = `transition_ticks + duration_ticks`. For
  `initial_kon`, transition is 0. For `fade_to_zero_retrigger`,
  transition is `fade_out_ticks + 1 + fade_in_ticks` (the +1 is
  the mandatory KOFF→SET_SRC gap). Compiler tests
  `total_ticks_matches_lowering` lock this.
- **No SET_PITCH in default M2.4 lowering** (M2.4): voice setup
  table seeds the pitch register at driver init from the atom's
  `root_midi_note`. The SET_PITCH ($30) opcode stays in §14.3 for
  forward compat (M3+ pitch sweeps); M2.4 compiler simply doesn't
  emit it. Locked by `canonical_lowering_emits_no_set_pitch`.
- **Capability manifest enforcement first lands in compile_sequence**
  (M2.4): per SPEC §5.4 / consultant #4, every compile entry point
  must consult the manifest. compile_sequence rejects when
  `synth_source_step` / `volume_slide` / `core_sequence_wait` etc.
  are absent, with `SequenceCompileError::CapabilityMissing` naming
  the missing feature. Validated by
  `rejects_when_synth_source_step_missing` and
  `dep_graph_each_edge_produces_missing_dep_when_dropped`.
- **Slide tick-window starts at +1** (M2.4 / SPEC §14.3): the
  bytecode-read tick that issues `VOL_SLIDE` does NOT itself
  contribute slide writes. Per the per-tick step order (slide-
  advance → wait-decrement → opcode-read), the slide is registered
  during opcode-read and first writes on the NEXT tick. The
  compiler's `ActiveSlideInterval { tick_start, tick_end }` is
  therefore `[bytecode_read_tick + 1, ...)`. Locked by
  `max_writes_per_tick_estimate_is_at_most_4` (which would observe
  6 writes on the bytecode-read tick if the interval were off-by-
  one).
- **Voice setup table byte-pinned ABI test** (M2.4 /
  consultant #28): `voice_setup_table_byte_pinned_abi` extracts
  the 22-byte table from the canonical multi-voice fixture's
  packed ARAM and asserts byte-exact match against the expected
  vector. Locks the §15.7 ABI between packer and the future M2.5
  driver.
- **WAIT 0 invalid; M2.4 compiler never emits it** (M2.4 / SPEC
  §14.3): schema validation rule 43 already disallows
  `duration_ticks=0`, and the compiler's `emit_wait` debug-asserts
  on it. Locked at the source level.
- **License model:** Apache-2.0 for the host application source; 0BSD for
  generated outputs (`.spc`, `.sfc`, driver/module blobs); snes_spc kept
  out-of-process to avoid LGPL propagation. See `LICENSING.md`.
- **Assembler:** asar primary for M0–M2, behind an `AssemblerBackend`
  interface so WLA-DX can be added later without rewiring the compiler
  (SPEC §4).
- **`max_sources`:** 128 (SPEC §5.4); ARAM packer enforces actual
  source-directory footprint.
- **M0 acceptance split** (resolved per consultant Patch 5): raw BRR
  decode is bit-identical at M0; voice-render and full-module-render
  tolerances are provisional at M0 and frozen at M1 (SPEC §10.1, §21,
  §23).
- **`.sfc` deferred to M1** (Pass 2.0): the 65816 SPC-upload contract is
  M1 work (SPEC §19.2); M0 ships `.spc` only.
- **Tool discovery via `SFCWC_*` env vars** (Pass 2.0): asar, snes_spc
  oracle, Mesen2 (SPEC §17.1).
- **Rust toolchain pinned to `stable` channel** (Pass 2.0): no specific
  version pin yet; revisit if regressions appear.
- **CLI binary name `sfcwc`** (M0.1): pinned in `app/Cargo.toml` so the
  executable is `sfcwc` regardless of the `sfc-atomizer` package name.
- **Default report output dir `build/m0/`** (M0.1): every stub command
  writes there unless `--out` overrides; `m0-acceptance` writes a
  sibling `manifest.json` next to the six reports.
- **Report envelope `{ schema_version, report_type }`** (M0.1): every
  JSON report carries these two top-level fields. `SCHEMA_VERSION = 1`
  in `core::report` gates breaking shape changes; the `report_type`
  string lets generic JSON consumers dispatch without a wrapping
  discriminator.
- **BRR fixture provenance discipline** (M0.2): every primary fixture
  must have independent ground truth — hand-walked, spec-derived, or
  external-tool. Decoder output as ground truth ("regression-baseline"
  provenance) is forbidden in the M0 corpus. If we ever add an
  audit-only baseline, it lives in a separate directory and is
  excluded from the M0 acceptance gate.
- **BRR fixture asset path `core/fixtures/brr/`** (M0.2): fixtures are
  first-class project assets, not test-only data. They live under
  `core/fixtures/`, are embedded via `include_str!` so they ship in
  the binary, and are consumed by both `core::brr_fixtures` (tests)
  and the `sfcwc decode-fixtures` CLI command.
- **Header bit ordering: END = bit 0, LOOP = bit 1** (M0.2): SnesLab's
  wiki disagrees (claims end=bit 1, loop=bit 0). We follow the
  SNESdev wiki and boldowa/snesbrr convention, locked by
  `flags_end_loop_ignored_by_raw_decode` and the `header_parse_layout`
  unit test.
- **AssemblerBackend trait shape** (M0.3): `name` + `version` +
  `assemble(&AssembleInput) -> Result<AssembleOutput, AssembleError>`.
  Version probing is informational, never gating — failures yield
  `Ok("unknown")`. `WrongOutputSize` is a hard error so a 64 KB
  image is always exactly 64 KB.
- **Asar invocation locked**: `asar --no-title-check
  --fix-checksum=off <source.asm> <output.bin>` (M0.3). Plus the
  smoke .asm must declare `lorom + arch spc700 + org $008200 +
  base $0200` to coax asar into a flat 64 KB ARAM workflow without
  expanding the file or injecting SNES-rom checksum bytes.
- **Smoke .asm location** `core/fixtures/asm/m0_smoke.asm` (M0.3):
  first-class fixture, not test-only. Sentinel bytes `00 2F FD` at
  offset `0x0200` are locked by an integration test.
- **SPC v0.30 file size 66,048 bytes, no extended ID666** (M0.4):
  Header (256 B) + ARAM (64 KB) + DSP (128 B) + unused gap (64 B at
  0x10180) + Extra RAM (64 B at 0x101C0). ID666 indicator = absent
  (0x1B); the 210-byte tag region is zero-filled. xid6/extended
  ID666 deferred until a use case appears.
- **M0 smoke state contract** (M0.4, SPEC §19.3): PC=$0200, GPRs=0,
  SP=$EF, DSP FLG=$60 (Mute amp + Echo write disable), every other
  DSP reg = 0. Result: SPC700 runs the driver from $0200 while the
  DSP produces no audio. M1+ smoke profiles will add an
  audible-but-deterministic state.
- **ARAM map approach for M0** (M0.4): `core::aram::map_from_image`
  walks the assembled image for nonzero driver-code extent. Valid
  for "single contiguous driver, no sample pool, no atom pool yet."
  The M3+ ARAM packer replaces this whole module; M0.4's stopgap
  reports the new invariant that regions partition total_aram
  exactly (rather than the M0.1 stub's "fixed regions only" model).
- **snes_spc fork: blarggs-audio-libraries mirror** (M0.5): pinned
  to commit `ec8ee2bbe30451614c1d02a83f7af1c97d497d45` (2020-10-24,
  unmodified Blargg 0.9.0 snapshot). Vendored at
  `tools/snes_spc_oracle/vendor/snes_spc/`; not a submodule.
- **Oracle wrapper build system: CMake** (M0.5): ~30-line
  CMakeLists.txt globs `vendor/snes_spc/*.cpp` plus the wrapper's
  `main.cpp`. C++17, no third-party deps, no package manager. The
  alternative `build.bat` fallback was unnecessary — CMake worked
  cleanly with the on-host Visual Studio 18 Community generator.
- **C++ toolchain on this host** (M0.5): no install needed —
  Visual Studio 18 Community (cl.exe 14.50.35717), Visual Studio
  2019 BuildTools (cl.exe 14.29.30133), and CMake 4.3.2 were all
  already present. CMake auto-selected the VS 18 generator.
- **Oracle wrapper CLI contract v1** (M0.5): ONE subcommand
  `render --input-spc <path> --frames <N> --output-pcm <path>
  --report <path>` plus `--version`. Locked; do not extend without
  a PM brief. Output PCM is `frames*4` bytes of s16le interleaved
  stereo at 32 kHz; oracle-side report mirrors the schema described
  in the M0.5 brief. `spc_clear_echo()` after load enforces
  determinism.
- **Oracle smoke baseline** (M0.5): rendering the M0.4 muted
  smoke `.spc` through the wrapper produces 8,192 zero PCM bytes
  (max_abs=0, rms=0). M0.5 provisional tolerances are `max_abs=1`,
  `rms=0.25`; both informational only (`ci_gate: false`). M1
  freezes the first accepted tolerance table per SPEC §10.1, §21.
- **Bundle status aggregation** (M0.6): required steps are
  doctor, decode_fixtures, assemble, spc_export, aram_map.
  Optional: calibration. Bundle is `ok` iff every required step
  is `ok` AND calibration is `ok`; `error` if any required step
  is `error` or `skipped`; `degraded` otherwise (any required
  step `warnings` OR calibration `warnings`/`error`/`skipped`).
- **Bundle integrity cross-references** (M0.6):
  `assemble.output_image_sha256 == spc_export.input_aram_sha256`
  (driver image flowing into SPC export);
  `spc_export.spc_file_sha256 == calibration.fixture_set.sha256`
  (SPC flowing into oracle render). `verify_bundle` reports both;
  `m0-status` exits 1 if either disagrees.
- **`oracle_pcm_sha256` lives in `CalibrationReport`** (M0.6):
  M0.5 left a "engineer's call" between embed-in-report vs
  reading the wrapper's sidecar JSON. M0.6 chose embed —
  `cmd_calibrate_oracle` computes the SHA on the in-memory PCM
  bytes alongside max_abs/rms; the bundle pulls it from one place
  rather than parsing the wrapper's report.

### M2.0 postlude — audition artifacts generated

`build/audition/` carries three Mesen2-ready artefacts for
user audible verification on return:

- `canonical_sine.spc` — 500 Hz sustained sine (.spc baseline,
  unchanged from M1.5).
- `canonical_sine.sfc` — single-project clone, exercises the
  `.sfc` IPL upload + RESET_TO_IPL + re-upload flow with
  audibly-identical modules. **Pre-M2.0 this would
  green-screen on driver-ready timeout; the M2.0 hotfixes
  unblocked it.**
- `swap_220_to_440.sfc` — two distinct projects
  (~219 Hz / ~438 Hz, exact 2:1 octave). Audible pitch jump
  at the swap point proves module B was actually uploaded.

Oracle cross-check on `swap_220_to_440.sfc`:
`modules_audio_identical=false`, A/B both pass the audible
threshold, `status=ok`. The two projects produce
distinguishable rendered PCM (different SHAs) which is the
positive signal we want for the swap exerciser.

### Host-specific tool resolution

- **Mesen2 on this host** (set during M2.0 follow-up): User-scope
  `SFCWC_MESEN2 = C:\Users\Spencer\Documents\Mesen_2.1.1_Windows\Mesen.exe`.
  Doctor resolves via `source: "env"` from any fresh terminal. The
  M1.6 PATH fallback finds nothing because the install dir isn't on
  PATH. Doctor going from "missing" to "resolved" doesn't change
  `m1-acceptance` bundle status — Mesen2 has always been
  informational, not required (SPEC §17.1 / §21 doctor mapping).

### M2.0 — Contracts freeze + M1 hotfix decisions

- **M1 driver token-bootstrap fix** (consultant #1, M2.0).
  Driver seeds `dp_last_token` from `$F4` at init via
  `mov a, $f4 ; mov $00, a` (encoded `E4 F4 C4 00`) before
  writing the ready signature. The `.spc` / oracle path was
  unaffected because `spc_load_spc` initialises `$F4` from
  the embedded ARAM image (zero); only the `.sfc` IPL exec
  path tripped the bug.
- **Loader ack token+code verify** (consultant #10, M2.0).
  `command_reset_to_ipl` wait loop now checks both the ack
  code on `$2141` AND the round-trip token on `$2140`. Stale
  acks no longer pass.
- **Compile-time source SHA enforcement** (consultant #3,
  M2.0). Every compile path verifies `source.sha256` against
  the live file before decode. `--refresh-source-hash` is the
  opt-in escape hatch; default is hard error on mismatch.
  Never auto-on during `m1-acceptance` — that would mask the
  drift it's meant to detect.
- **Driver size detection: sentinel post-scan collision check**
  (consultant #7, M2.0). After slicing at the first
  `$DE $AD $BE $EF` occurrence, the driver_build scans the
  rest of the image (`$0204..$FFC0`) for nonzero bytes — any
  trailing content means the slicer truncated mid-driver and
  the real `driver_end` lies past an accidental sentinel. The
  M1 driver does not collide; the check is a canary for M2+.
- **Audible thresholds remain frozen at M1.7 floor** (M2.0).
  The M1 thresholds (`min_max_abs=1000`, `min_rms=200`) are
  non-silence gates, not quality gates. M2 adds per-channel
  checks AND combined-energy consistency AND source-step
  observability (Appendix A.7 / SPEC §21 M2); none relax the
  M1 floor.
- **M1 baselines rebased; pre-M2.0 SHAs not pinned** (M2.0).
  The M1.7 baseline SHA block is replaced verbatim with the
  post-rebase values. Pre-rebase commit `4bf286f` is the
  canonical reference for the prior baselines; tests use
  computed-not-asserted SHAs throughout, so no test code
  hardcoded the old values.
- **M2 architectural contracts frozen** (M2.0). Per Appendix
  A: capability manifest M2 (§5.4), sequence bytecode v2
  (§14.3), voice setup table (§15.7), project schema v2
  (§16.9), migration v1→v2 (§16.10), driver
  `multi_voice_atom` profile (§20.2), M2 oracle thresholds
  + source-step observability (§21 M2). Implementation lands
  M2.1+ — these are SPEC-canonical, types-follow.
- **M2 status flags reservation deferred to M2.5** (M2.0).
  Two surface options reserved: sibling status byte on `$F6`
  (replacing `driver_version`) vs `driver_version` bump to
  `$02` with reinterpreted `driver_out_3`. Either way the
  ready signature on `driver_out_0..1` stays `$A5 $5A`. M2.5
  picks one when shipping the multi-voice driver.

### M1 acceptance bundle decisions (M1.7)

- **Audible verification thresholds frozen** (M1.7).
  `min_max_abs = 1000`, `min_rms = 200`. Both the M1.5
  `verify-spc-audible` and M1.6 `verify-sfc-modules-audible`
  CLI defaults locked here. Regressions become CI failures
  from M2 onward (parallel to the M0.6 → M1 calibration
  freeze pattern).
- **M1 bundle aggregation rules** (M1.7). Required steps:
  doctor, validate_a, compile_spc, audible_spc, compile_sfc,
  structure_sfc, audible_sfc. Optional: validate_b (Skipped is
  fine when no project_b given). `BundleStatus::Ok` iff every
  required step is `Ok`. `Error` if any required step is
  `Error` or `Skipped`. `Degraded` if any required step is
  `Warnings`. Mirrors the M0.6 aggregation pattern, adapted
  to the M1 step set.
- **Doctor mapping carves out Mesen2** (M1.7). Per the brief's
  guidance: asar missing → Error; oracle missing → Warnings
  (audible steps then Skip and the bundle drops to Error
  there); Mesen2 missing → Ok (informational only). The
  doctor status enum's blanket "Warnings" — which counts
  Mesen2-missing — would otherwise downgrade a healthy
  bundle to Degraded; we look at individual tool-resolved
  flags instead.
- **Cross-reference SHA invariants enforced by
  `verify_m1_bundle`** (M1.7):
  `compile_spc.spc_file_sha256 == audible_spc.spc_sha256`
  (the same .spc file fed to the oracle), `compile_sfc.
  module_a_in_file_sha256 == structure_sfc.module_a.
  in_file_sha256` (the same module.bin embedded in the .sfc),
  `compile_sfc.sfc_path` and `structure_sfc.sfc_path` resolve
  to the same filename, all reports share `schema_version`.
  `m1-status` re-runs these against the on-disk bundle so
  drift after generation is surfaced.
- **`m1-acceptance` chains by spawning sfcwc subprocesses**
  (M1.7). The M1 cmd_* functions exit-on-failure (designed as
  top-level CLI commands); rather than refactor each into a
  Result-returning library function, m1-acceptance spawns the
  current binary for each step and reads the produced report
  back. Same fail-as-data shape as M0.6. Trade-off: ~50 ms
  of process spawn overhead per step (×8 steps); acceptable
  for an acceptance gate.

### M1 .sfc / module.bin / Mesen2 decisions (M1.6)

- **LoROM minimum size 256 KB** (M1.6). The loader pads to
  bank 7 end (file offset `$3FFFF`) so the file matches the
  header ROM-size byte `$08`. Smaller sizes (`$07` = 128 KB)
  also valid LoROM but the brief mandates 256 KB.
- **65816 code at $00:8000..$00:7FBF; module A at $01:8000;
  module B at $02:8000** (M1.6). LoROM bank → file offset
  mapping puts loader code in file `$0..$7FBF`, module A at
  `$8000`, module B at `$10000`. Banks 3..7 are pad.
- **asar invocation differs between SPC700 and 65816** (M1.6).
  SPC700 driver build uses `--fix-checksum=off` (flat ARAM,
  no SNES checksum). 65816 ROM build uses `--fix-checksum=on`
  so asar fills LoROM checksum + complement at `$7FDC..$7FDF`.
  `AssembleInput::extra_args` now lets each backend caller
  set this independently.
- **Single-project compile-sfc emits module B as a duplicate
  of module A** (M1.6). The loader still exercises the swap
  flow; user hears the same audio twice with a brief gap, which
  positively confirms RESET_TO_IPL + re-upload worked. The
  CLI report's `module_b_is_clone_of_a` flag and the audible
  report's `modules_audio_identical` make the clone explicit.
- **65816 loader spin counts use 16-bit double-loops** (M1.6).
  SPEC §19.2 specifies 32-bit values like `WAIT_IPL_READY_POLLS
  = 0x0020_0000`; the loader uses `outer × inner = 0x10 × 0xFFFF`
  ≈ 1M iterations, ~50 ms at 21 MHz — ample for ~2 ms IPL ready.
  Multi-loop upgrades reserved for a future tighter spec.
- **Module swap timing: ~5 sec module A, RESET_TO_IPL,
  module B forever** (M1.6). Configurable via spin-count
  constants in `m1_loader_65816.asm` (`idle_5sec`).
- **Loader fail-mode background colours**: red = IPL-ready
  timeout, green = driver-ready timeout, blue = command-ack
  timeout, white = IPL byte-ack timeout. Documented inline
  at the top of `m1_loader_65816.asm`.
- **Sentinel byte sequence locks emulator vector layout**:
  RESET vector at `$00:FFFC` (NOT `$00:FFFA`) — caught during
  Phase B development. SPEC §19 didn't expand the LoROM vector
  table, so this is the empirical layout per fullsnes.
- **Re-fix LoROM checksum after embedding modules** (M1.6).
  asar fixes the checksum during initial assemble; embedding
  module bytes after that invalidates it. `core::sfc_export`
  recomputes by zeroing the four checksum bytes, summing all
  bytes mod `$10000`, and writing complement + sum.
- **module.bin self-zeroed SHA workaround** (M1.6, SPEC §19.4
  reaffirmation). The full-file SHA can't live inside the file,
  so `$20..$40` carries SHA-of-file-with-those-32-bytes-zeroed.
  The literal full-file SHA lives only in the M1 manifest.
  `parse_module_header::content_sha256_in_file` round-trips
  cleanly through `recompute_in_file_sha`.
- **Mesen2 PATH fallback** (M1.6, Phase 0). `resolve_mesen2`
  now tries env → PATH → missing, mirroring the asar pattern.
  Locked binary names: `Mesen.exe` / `Mesen2.exe` / `Mesen` on
  Windows; `Mesen` / `Mesen2` (and lowercase) on POSIX. SPEC
  §17.1 table updated.

### M1 driver / compile-spc / audible-verify decisions (M1.5)

- **Driver constants format: asar `name = $XX` (label assignment)**
  (M1.5). The `m1_constants.inc` file uses asar's `name = value`
  syntax (label numeric assignment); `mov $f3, #voice0_voll`
  resolves to `mov $f3, #$XX` at assemble time. Cleaner than
  `!define`, no usage-site `!` prefix.
- **Driver end marker: 4-byte sentinel `$DE $AD $BE $EF`**
  (M1.5). Single-byte sentinels collide with legitimate
  instruction bytes (the driver contains `$EE` and `$FF` in
  immediate fields). The 4-byte pattern is distinctive enough
  to find unambiguously by linear scan from offset $0200, and
  lets driver_build avoid teaching itself the SPC700 instruction
  encoding.
- **Master volume hard-pinned to `$7F`** (M1.5). The §16.4
  schema doesn't expose a project-level master_voll/master_volr
  field. M1 ships max-loud and documents the deviation; if a
  future pass adds project master volume, that's a non-breaking
  schema extension with default = $7F.
- **SPC initial DSP state: all zero, driver writes FLG=$60 first**
  (M1.5). Matches what happens on real hardware after IPL upload
  — the SPC starts with whatever DSP state the BIOS left, the
  driver mutes immediately on its first instruction, and unmutes
  later in init after voices/echo/source-dir are programmed.
  Simpler than computing DSP regs at compile time + identical
  observable behavior after the first ~4 instructions.
- **Audible verification thresholds: `min_max_abs=1000`,
  `min_rms=200`** (M1.5). For a unity-gain sine sample at MID
  amplitude these are easily exceeded (observed 11072 / 5520).
  `silent_fail` correctly catches the M0 muted smoke (0 / 0).
  Tunable via CLI flags. M1.6+ may tighten or add per-genre
  thresholds.
- **Driver-source embedding via `include_str!`** (M1.5). The
  canonical `m1_sample_basic.asm` is built into the host crate
  at compile time; runtime invocation writes it to a tempdir
  alongside the generated `m1_constants.inc` for asar. Means
  the host has zero filesystem-layout assumptions and the
  driver source ships in every binary.
- **`tempfile` promoted from dev-dep to regular dep** (M1.5).
  Host orchestration (CLI + GUI compile-spc paths) needs a
  scratch directory for asar; tempfile was already in the
  workspace, so this is a re-classification not a new crate.
- **Empirical DSP register write order** (M1.5): mute (FLG=$60)
  → clear KON/KOFF → master vol → DIR → echo params (FIR / ESA /
  EDL) → EON last → voice 0 regs → unmute (FLG=running) → DP
  state → KON → ready signature. Echo params before EON so the
  buffer pointer is set before any echo write can reach RAM.
  Mute around the whole init so DAC stays silent through the
  ~50 µs setup.

### M1 packer / ARAM meter decisions (M1.4)

- **Driver code budget M1.4: 4 KiB at $0200..$1200** (`DRIVER_CODE_BUDGET_M1`).
  M1.4 fills with zeros (placeholder). M1.5 ships the real
  assembled `sample_basic` driver in the same window. The budget
  is round-aligned to the page boundary so the source directory
  always lands at `$1200` (`DIR` register = `$12`) regardless of
  the actual driver size.
- **Source directory placement: page-aligned, padded to next page**
  (M1.4): 4 bytes/entry (start_addr_le, loop_addr_le). The page-
  pad rolls into the `source_directory` region in the map so the
  BRR pool always starts on a clean page boundary.
- **Echo placement: page-aligned, ends at `$FF00`** (M1.4): largest
  page boundary at or below the `$FFC0` IPL ROM shadow. ESA =
  `echo_start >> 8` is well-defined for any EDL ∈ 1..=15. The 192
  bytes `[$FF00, $FFC0)` are reported as `ipl_rom_safe_pad`. The
  brief's "echo_end = 0xFFC0" was ESA-misaligned and corrected
  here.
- **Multi-sample layout: single contiguous BRR pool** (M1.4). No
  inter-sample gaps, no per-sample regions in the map (only one
  `sample_brr_pool` region; per-sample bounds live in
  `samples.per_sample`). M2+ may split when atom pools land.
- **Loop addresses: `start_addr + loop_block * 9` for looped;
  `loop_addr = start_addr` for one-shots** (M1.4). The S-DSP's
  END flag in the last block handles non-loop termination; loop
  flag handles wrap-back.
- **Map report extensions are non-breaking** (M1.4): all four new
  fields (`echo`, `source_directory`, `samples`, `warnings`) are
  `Option`/`Vec` with `serde(default, skip_serializing_if)`.
  M0.6 manifests still parse, M0 acceptance + `m0-status`
  unaffected. `SCHEMA_VERSION` stays at 1.
- **Meter recompute is synchronous, in-process** (M1.4): the GUI
  re-runs decode + encode + pack on the UI thread when the project
  changes. Sub-second for realistic projects; no `Command::new`
  overhead. Failed recomputes leave the prior meter visible and
  surface the error inline.

### M1 encoder / loop / audition decisions (M1.3)

- **Encoder scoring: peak-first, RMS-tiebreak** (M1.3): brief
  said exhaustive `(filter, shift)` search; consultant didn't pin
  the cost function. Pure RMS scoring picks shifts that
  cleanly encode the bulk of the block but clip at signal peaks
  (4-bit nibbles × shift step can't span large amplitudes), and
  clipping at peaks is what makes BRR samples sound distorted.
  Peak first, sum-of-squares as tiebreak, avoids that cleanly.
  Empirical: amp-8000 sine round-trip peak drops from 793 to 55
  LSBs.
- **`force_filter_0_first_block` default true** (M1.3):
  conservative safety against predictor-history glitches at
  KON. The S-DSP documents prev1=prev2=0 reset on KON, so
  filters 1..=3 on block 0 are normally safe; default-on is
  the consultant's belt-and-suspenders advice. The `EncodeOptions`
  toggle lets callers (round-trip tests, advanced users)
  disable it when block-0 quality matters more than safety.
- **Symphonia i32 path: always `>> 16`** (M1.3):
  `SampleBuffer<i32>` left-aligns every PCM sample in i32
  regardless of source bit depth (i16 → `<< 16`, i24 → `<< 8`,
  i32 → identity per the symphonia-core::conv impl_convert
  table). To recover an i16 sample, drop the low 16 bits — the
  shift is fixed, not heuristic.
- **Loop finder: first-25% / last-25% search ranges** (M1.3):
  matches typical sustained-instrument loop shapes (attack →
  loop region → release implicit). Score = rms_window_diff +
  0.25 × seam_click — the click weight is intentionally below
  the RMS so a high-RMS / zero-click pair never beats a
  low-RMS / small-click pair.
- **Audition WAV: raw 15-bit output, no gain compensation**
  (M1.3): `core::audition` writes the S-DSP's natural 15-bit
  output stored in i16 (range -16384..=16383), no scaling. The
  audition reflects what playback would emit at unity voice
  gain; user applies makeup gain in their DAW if needed.
- **Encoder pads to multiples of 16 internally** (M1.3): brief
  allowed either "encoder errors on unaligned input" or
  "encoder pads with zeros." Pads internally so the CLI and
  GUI don't have to repeat the rounding logic. `encode_looped`
  still rejects unaligned `loop_start_sample` because that's a
  meaningful contract (BRR loop alignment), unlike trailing-pad
  which is just bookkeeping.

### M1 import decisions (M1.2)

- **Copy-into-project default ON** (M1.2): `sfcwc import` and
  the GUI both default `copy_into_project = true`. CLI exposes
  `--no-copy` for the opt-out. Rationale: a project is a
  self-contained source-of-truth; relying on external paths is
  fragile across machines.
- **SHA-match dedup on re-import** (M1.2): if `<project_dir>/
  audio/<filename>` already exists with the same SHA-256 as the
  freshly-computed source, the existing file is reused — no `_2`
  copy. Different SHA with the same filename suffixes `_2`, `_3`,
  … on the file. Either way, the new sample slot gets a unique
  `id` (also suffixed on collision).
- **Path-traversal guard** (M1.2): the canonicalized `<project_dir>/
  audio/` target must start with the canonicalized project
  directory; anything else (symlink-out, absolute-path injection)
  refuses with `ImportError::PathTraversal`. Honest about Windows
  UNC paths: canonicalize handles `\\?\` prefixes uniformly on
  both sides of the comparison.
- **Default `SampleSlot` fields** (M1.2): root MIDI 60 (C4),
  loop disabled with `start_sample` / `end_sample` / `snap` all
  `None`, `playback.envelope = GainRaw { gain_byte: 127 }`,
  volume 1.0, pan 0.0, echo off. ADSR is reserved for M1.3+ once
  loop selection lands; M1.2 deliberately keeps the envelope the
  raw-byte form to avoid premature commitment.
- **First-sample auto-binds `m1.active_sample_id`** (M1.2): if
  the pool was empty before the import, the new sample's id is
  written into `m1.active_sample_id` so the project validates
  immediately. Subsequent imports leave `active_sample_id`
  alone.
- **`id` derivation rule** (M1.2): filename stem → ASCII
  lowercase → any non-`[a-z0-9]` codepoint folds to `_` (runs of
  separators collapse) → leading/trailing `_` trimmed →
  truncate to 64 chars → fall back to `sample_<N>` if empty →
  uniquify with `_2`, `_3`, … on collision. Matches the SPEC
  §16.4 `^[a-z0-9_]+$` constraint.
- **WAV bit-depth: 8 / 16 / 24 int only** (M1.2): float WAV
  (`WAVE_FORMAT_IEEE_FLOAT = 0x0003`) and 32-bit int rejected.
  WAVE_FORMAT_EXTENSIBLE follows the SubFormat GUID's leading
  two bytes back to a legacy tag.
- **AIFF compression: NONE and sowt only** (M1.2): plain AIFF
  (no compression field) accepted as uncompressed BE PCM.
  AIFC accepted only when the 4-byte compression code is
  `NONE` or `sowt`. ima4, ulaw, MAC3/MAC6, etc. rejected.
- **BRR file-size invariant** (M1.2): file size must be a
  positive multiple of 9 bytes. `sample_rate_hz` defaults to
  32000 Hz (the canonical S-DSP rate); CLI exposes
  `--brr-sample-rate <Hz>` and `ImportOptions::brr_sample_rate_hz`
  to override.
- **No binary audio fixtures committed** (M1.2): every test
  synthesizes its WAV/AIFF/AIFC/BRR fixture in a `tempfile::
  TempDir` at runtime via `core/tests/common/mod.rs`. The
  AIFF synth uses the four pre-computed 80-bit IEEE 754
  extended-precision byte tables for the rates the tests
  exercise; runtime extended-80 encoding is deferred to M1.3+
  if a future test needs it.
- **`rfd` authorized** (M1.2): native file dialogs across
  Windows/macOS/Linux with `xdg-portal + tokio` features; the
  M1.1 hand-rolled text-input modals stay in place for File →
  Open / Save As / New (those still need addressing) but
  Import goes straight to the native picker.

### M1 implementation decisions (M1.1)

- **GUI binary `sfcwc-app` separate from CLI `sfcwc`** (M1.1):
  one workspace, two `[[bin]]` targets in `app/Cargo.toml`. CI/
  scripted workflows depend on `sfcwc` only and don't pay the
  eframe build cost; interactive use takes `sfcwc-app`.
- **Project file extension `.sfcproj.json`** (M1.1): JSON-honest
  filename plus a `.sfcproj` prefix that flags the file as
  project-shaped. Matches the brief's recommendation.
- **`load_from_path` does not auto-validate** (M1.1): the
  `(load, validate)` two-step lets the GUI render an invalid
  project alongside its errors instead of refusing to load it.
  CLI's `validate-project` and `load_and_validate()` chain the
  two for the happy path.
- **`ValidationError` shape: `{path, kind}`** (M1.1):
  JSON-pointer paths (`/sample_pool/0/loop/end_sample`) so the
  GUI can highlight the offending field. M1.0's flat enum
  replaced.
- **Validation collects every failed rule** (M1.1): no
  bail-on-first. The UI surfaces all problems at once so the
  user fixes everything in one editing pass.
- **Name vs id conventions** (M1.1, locked in SPEC §16.4):
  `name` allows spaces and non-ASCII letters but rejects
  control characters and path separators (`/`, `\`, `:`); `id`
  is `^[a-z0-9_]+$`. Justified in the spec's `id`/`name`
  paragraphs.
- **`ValidationReport` JSON shape** (M1.1, locked in
  `core::report`): `{ schema_version, report_type:
  "validation", project_path, status: ok|invalid|io_error,
  errors: [{path, message}] }`. `errors[]` is a flat
  `{path, message}` shape so JSON consumers don't need to know
  the typed `ValidationErrorKind` enum.
- **symphonia landed at M1.1, not M1.2** (M1.1): brief allowed
  either; landing it now means M1.2 wires only the WAV/AIFF
  probe modules without touching workspace deps. Currently
  declared in `core/Cargo.toml` but unused; the
  `unused_crate_dependencies` lint is allow-by-default so this
  is harmless.
- **Native file pickers deferred** (M1.1): no authorized
  file-picker crate (`rfd` etc.) in the M1.1 dep set, so File →
  Open / Save As / New use a hand-rolled single-line text-input
  modal. M1.2+ revisit with PM authorization.

### M1 contract decisions (M1.0)

These are the M1 contract surfaces frozen at M1.0. Implementation
details that change the byte values, formulas, or named fields are
spec changes, not implementation choices.

- **module.bin format** (SPEC §19.4): magic `"SFCWCM1\0"`,
  `schema_version = 1`, 64-byte header, 8-byte block entries,
  little-endian throughout. Self-reference workaround:
  `content_sha256_zeroed` lives in the header; the literal full-file
  SHA-256 lives in the M1 manifest as `module_file_sha256`.
- **Driver command bytes** (SPEC §20.1): commands `$01` STOP,
  `$02` RESET_TO_IPL, `$7F` PING. Acks `$81` STOP_ACK, `$82`
  RESET_ACK, `$FF` PING_ACK, `$EE` invalid. Ready signature
  `$A5 $5A $01 <status_flags>`.
- **Status flag bit map** (SPEC §20.1): bits 0–4 in use; bits 5–7
  reserved must be zero. Surface lives in `core::driver_proto::StatusFlags`.
- **Pitch formula and round-half-up** (SPEC §16.7): the SNES voice
  pitch is a 14-bit clamped register; rounding is `floor(x + 0.5)`,
  not banker's. M1 reference values: 32 kHz at root → `$1000`,
  22050 Hz at root → `$0B06`. Implemented in `core::pitch`.
- **MIDI convention** (SPEC §16.7): C4 = MIDI 60, A4 = MIDI 69 with
  A4 tuning = 440 Hz. Project files store integer MIDI numbers, not
  note strings.
- **Project v1 schema scope** (SPEC §16): single sample, ADSR or
  GAIN envelope, optional master echo + per-sample echo gate.
  No tracks, no clips, no sequencer — those land at M1.5+ in a
  forward-compatible schema bump.
- **`.spc` / `.sfc` parity rule** (SPEC §21 M1): both exports use
  the same canonical `aram_image` and the same driver entrypoint
  `$0200`. Allocated regions match bit-for-bit. Free regions are
  not parity-significant in `.sfc` because `$00F0–$00FF` is I/O,
  `$0100–$01FF` is stack, and free ARAM is not part of the module.
- **M1.3 audition path** (SPEC §17.2): decoded-BRR mono PCM16 WAV at
  `build/m1/previews/<sample_id>_decoded_brr.wav`. Cross-platform;
  no host audio engine in M1.
- **Bounded host-side spin counts** (SPEC §19.2): six named
  `WAIT_*_POLLS` constants so a missed handshake fails fast rather
  than deadlocks the host. Surface lives in
  `core::driver_proto::host_timeouts`.

### M0 frozen acceptance SHAs

These three SHA-256s identify the M0 acceptance artifact set.
Locked by `m0-acceptance` and re-checked by `m0-status`. Any future
change to the source `.asm`, the SPC exporter state contract, or
the oracle wrapper that alters these SHAs is a producer-side
regression.

```
M0_ARAM_IMAGE_SHA256  = ba728e6fb836e5da4d4d1abec94956f8d92304ce0ac8b768b4103b237c910298
M0_SPC_FILE_SHA256    = 0caba4a35c30c5cadce9585cea3140d17a048f4900721587457813827eea6f51
M0_ORACLE_PCM_SHA256  = 9f1dcbc35c350d6027f98be0f5c8b43b42ca52b7604459c0c42be3aa88913d47
                        (matches SHA-256 of 8,192 zero bytes)
```

## Open questions resolved at M0

- **Embed snes_spc for live preview vs keep oracle-only** (SPEC
  §23, question 2). **Resolved at M0.5/M0.6**: the wrapper is a
  separate-process oracle invoked across a process boundary, per
  `LICENSING.md` §3 and SPEC §17.1. The Apache-2.0 host never
  links snes_spc; embedding for live preview is forbidden on
  licensing grounds. Live preview at M3+ uses the internal Rust
  BRR decoder (§10.1 internal renderer mode), with the oracle
  remaining the calibration second-source.

## Open questions remaining

Cross-reference SPEC §23. Of the four questions there:

1. Final atom quality thresholds — empirical, deferred to calibration runs.
2. ~~Embed snes_spc for live preview vs keep oracle-only — decision target M0.~~ Resolved at M0.5/M0.6: separate-process oracle, host never links it.
3. Practical wavetable frame caps — empirical, may move from 32/64/96/128.
4. Whether a future version adds a free pre-emphasis EQ editor.

## Next pass

**M2.1 — Project schema v2 + migration.**
`ProjectV2::validate` body, `migrate_from_v1` body, host
load/save plumbing. The type tree, validation rules, and
migration table are all already locked in SPEC §16.9 / §16.10
and the `core::project_v2` skeleton — M2.1 just fills bodies.
