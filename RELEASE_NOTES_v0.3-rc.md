# v0.3-rc — M3 release candidate

The M3 release candidate closes the M3 milestone: BRR-encoder
quality work (loop-click metric + phase rotation), atom render
stability identity-pins, gaussian characterization plumbing, GUI
polish, and the M3 acceptance bundle.

The release is recorded as `release: v0.3-rc` in
`baselines/m3.json` and tracks the M3.8 close commit on the
`main` branch.

`baselines/m3.json` inherits `baselines/m2.json` by reference;
M3 acceptance includes M2 acceptance as its stage 1 regression
gate.

## Highlights

- **M3.0 — M3 contracts.** SPEC §10.6 locks the loop-click metric
  formula (`loop_click_abs = |decoded[loop_start] -
  decoded[loop_end - 1]|`). SPEC §10.7 locks the phase rotation
  contract (block-aligned candidate offsets, lexicographic
  objective `(loop_click_abs, peak_abs_error, rms_error,
  rotation_offset)`, `f64::total_cmp` for the float lex level,
  smallest-offset tie-break). SPEC §16.9 amendment locks atom
  PCM stability across milestones.
- **M3.1 — loop-click metric implementation against M2 atoms.**
  `AtomBrrOutput` and `AtomRenderReport` gain `loop_click_abs`,
  `loop_window_rms_delta`, and `decoded_brr_pcm_sha256`. Atom
  PCM SHAs reclassified from documentary to identity-gated.
- **M3.2 — atom edge case fixture coverage.** Nine new
  synthesized fixtures broaden the atom render → BRR encode →
  metric input space: `amplitude_zero`,
  `all_partials_zero_normalize_true`,
  `two_partials_cancel_partially`,
  `max_amplitude_no_normalize`,
  `normalize_false_multi_partial_clamp_safety`,
  `harmonic_16_cycle_64`,
  `all_8_partials_max_amp_harmonics_1_to_8`,
  `phase_cycles_0_9999`, `cycle_256_canonical_sine`. 11 atom
  PCM SHA identity pins in `baselines/m3.json`.
- **M3.3 — phase rotation.** First encoder-shifting M3 pass.
  9 of 11 atom fixtures drop to `loop_click_abs = 0`
  post-rotation; 1 sees an 87% reduction; 1 (near-Nyquist
  `harmonic_16_cycle_64`) finds no improvement and lex-defaults
  to `offset = 0`. The `M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE`
  behavior-gated baseline enforces "post ≤ pre" per fixture.
  Atom PCM stays untouched per §16.9 (rotation is a transient
  encoder input). Committed on-disk reproducer fixture:
  `fixtures/projects/atom_edge_cases/harmonic_16_cycle_64.sfcproj.json`.
- **M3.5 — gaussian characterization plumbing.** `sfcwc
  characterize-gaussian` CLI subcommand: builds a single-atom
  V2 project per signal in the `m3_5_canonical` 9-signal set,
  compiles to SPC, runs the snes_spc oracle, computes per-signal
  metrics (RMS, gain delta, ZCR, clipping count, peak error
  against source). 112 documentary snapshots in
  `baselines/m3.json::M3_5_*`. Optional `subjective_audition`
  field in the report schema captures PM A/B audition results
  separately from deterministic measurements.
- **M3.5.1 — methodology audit.** Seven methodology diagnostic
  fields added to the measurement schema (`alignment_best_offset`,
  `aligned_raw_rms`, `aligned_oracle_rms`,
  `normalized_correlation`, `zcr_ratio`,
  `first_8_zero_crossings_raw`/`_oracle`,
  `peak_abs_error_after_gain_normalization`) plus
  `gain_delta_db_aligned`. Decision rule grows a precondition #0
  (`zcr_ratio ∈ [0.9, 1.1]` for monotonicity-anchor signals).
  Re-run confirms anomalies; outcome shifts to
  `recommended_next = "methodology_review"`. See "Pre-emphasis
  deferred to M4" below.
- **M3.7 — GUI polish.** `rename_sequence_id_cascade` model
  method cascading through `tracks[].atom_sequence_id` and
  `m2.active_sequence_id` (mirrors M2.8's atom rename cascade);
  GUI sequence-id text field switched from direct mutation to
  buffer + cascade. Atom-edit panel grows a "Last preview
  metrics" readout showing `loop_click_abs` (color-graded green
  / yellow / orange / red on the 0 / ≤1000 / ≤5000 / >5000
  bands), `rotation_offset`, `peak_abs_error_post_rotation`,
  `rms_error_post_rotation`. `loop_window_rms_delta` and
  gaussian characterization deliberately NOT surfaced in GUI
  (consultant M3.5 audit #6 and #16).
- **m3-acceptance bundle (M3.8).** Five-stage rollup:
  M2 regression (subprocess `m2-acceptance`); atom PCM
  stability (cargo test `atom_pcm_sha_matches_locked_baseline_m3`);
  loop-click improvement gate (cargo test
  `phase_rotation_loop_click_never_regresses_against_pre_m3`);
  encoder-quality snapshot (post-rotation BRR + decoded-BRR
  SHAs — soft gate); baselines integrity audit (every
  identity_gated entry carries a `test:` field).
  `bundle.status = ok` on the canonical M2 fixture and on the
  M3.3 committed edge-case fixture.

## What's locked

`baselines/m3.json` carries the M3 release baselines and
inherits `baselines/m2.json` by reference. Summary:

- **Identity-gated** (any drift = regression).
  - 11 atom PCM SHAs (2 from M3.1: `M2_ATOM_128_SINE_PCM_SHA256`,
    `M2_ATOM_64_SINE_PCM_SHA256` — promoted from M2 documentary
    to M3 identity per SPEC §16.9 amendment; 9 from M3.2 atom
    edge-case fixtures).
- **Behavior-gated** (numeric / policy contracts):
  - `M3_LOOP_CLICK_METRIC_GATING` (SPEC §10.6 reports-only at
    M3, gate at M4+).
  - `M3_PHASE_ROTATION_OBJECTIVE` (SPEC §10.7 lex tuple
    contract).
  - `M3_PHASE_ROTATION_CANDIDATE_SET` (SPEC §10.7 block-aligned
    offsets only).
  - `M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE`
    (per-fixture post ≤ pre).
- **Documentary snapshot** (informational; expected to shift on
  declared milestones):
  - Pre-M3 atom BRR + decoded-BRR + loop-click + windowed
    delta values (11 fixtures × 4 entries).
  - Post-M3.3 phase-rotation BRR + decoded-BRR + loop-click +
    peak/rms error + windowed-delta values (11 fixtures × 7
    entries with `_PHASE_ROTATION` suffix).
  - M3.5 gaussian characterization measurements (9 signals × 12
    fields + 4 summary entries).
  - M3.5.1 methodology diagnostic re-snapshots (9 signals × 5
    fields + 4 summary entries).
  - M3.5.1 `_audition_note` annotations on pairs 4 / 5
    `_PHASE_ROTATION` entries.

`baselines/m2_canonical_fixtures.md` (M2.5) is unchanged.

## Reproduction

`docs/reproduce-m2.md` is the unified reproducer guide covering
both M2 and M3 acceptance — fresh clone, build oracle, run
test suite, run `m2-acceptance` + `m3-acceptance`, regenerate
characterization, regenerate M3.5 prelude audition WAVs.

Test count at v0.3-rc: **579 tests workspace-wide**, all green
under `cargo test --workspace` (was 521 at v0.2-rc).

## Pre-emphasis deferred to M4

M3 originally planned a pre-emphasis preset milestone (M3.6)
after gaussian characterization (M3.5). M3.5 characterization
measured the S-DSP gaussian curve across a 9-signal test set
but surfaced two methodology anomalies:

1. `zcr_ratio` (oracle zero-crossing rate / raw zero-crossing
   rate) measured ~2× for low/mid-frequency sine signals,
   indicating the oracle and raw decode outputs are not
   phase-aligned — the `align_oracle_to_raw` helper uses a
   `max_offset = 32` sample search window, which cannot resolve
   cycle lengths greater than 32 samples (canonical signals
   use cycle lengths 64 / 128 / 256).

2. The absolute `gain_delta_db` curve shows a +2.6 dB low/mid
   boost that the gaussian alone shouldn't produce. This may
   be a methodology artifact of the misaligned comparison
   rather than a real S-DSP characteristic.

The M3.5.1 methodology audit added diagnostic fields to the
characterization report (`zcr_ratio`, `normalized_correlation`,
`first_8_zero_crossings_*`, `peak_abs_error_after_gain_normalization`,
etc.) and a decision-rule precondition (`zcr_ratio ∈ [0.9, 1.1]`
for monotonicity anchors). The precondition fires on 7 of 9
signals (only `harmonic_16_cycle_64` and
`all_8_partials_max_amp_harmonics_1_to_8` land in band); the
characterization returns
`recommended_next = "methodology_review"` instead of a preset
recommendation.

Designing a pre-emphasis filter against potentially-misaligned
measurements would compensate for the methodology pipeline
rather than the S-DSP. **M3.6 defers to M4+** where the
alignment search range will be expanded to ≥ max_cycle_len
(256 samples) and the characterization re-run.

Independently, M3.5.1 confirmed that BRR encoder distortion
(`peak_abs_raw_vs_source ≈ 18431` LSBs across all 9 signals —
over half of i16 dynamic range) is the dominant atom-render
artifact, not gaussian coloration. M4 scope: broader BRR noise
/ DSP-coloration compensation, not just HF pre-emphasis.

The audition WAVs that drove the M3.5 phase 2 user audition
(under `build/audition/m3.5-prelude/`, gitignored) remain
reproducible via the ignored
`m3_5_emit_audition_wavs` test; see `docs/reproduce-m2.md` for
the invocation.

## M4 prelude scope

Open questions identified during M3 that may be addressed at
M4+:

1. **Gaussian characterization methodology resolution.** Expand
   `align_oracle_to_raw` search range to ≥ max_cycle_len. Re-run
   characterization with reliable alignment. M3.5.1
   precondition #0 will pass when `zcr_ratio` settles in
   `[0.9, 1.1]` for the anchor signals.
2. **BRR encoder noise floor reduction.**
   `peak_abs_raw_vs_source ≈ 18431` LSBs is the dominant
   atom-render distortion. M4 may investigate per-block filter
   selection refinements or predictor optimization (M3.4
   deferred per consultant M3.3 audit #21).
3. **Pre-emphasis presets** (conditional on item 1). If M4
   methodology audit confirms a real frequency-response curve
   worth compensating, ship `gentle` and / or `strong` presets
   per SPEC §10.9. Decision rule conditions #3 and #4 require
   evaluation against a proposed preset's outputs.
4. **`rename_track_id_cascade`.** Currently no cascade needed
   since track IDs aren't referenced elsewhere in the v2
   schema. Revisit if schema additions reference track IDs.
5. **M4 baselines file structure.** Create `baselines/m4.json`;
   inherit M3 by reference (mirror the M3 inherit-M2 pattern).

This is forward visibility, not a commitment. M4 can pick and
choose.

## Tagging

This release-candidate is recorded as `v0.3-rc` in
`baselines/m3.json::release`. Tag in git when ready to publish:

```bash
git tag -a v0.3-rc1 <m3.8-close-commit> -m "v0.3-rc1: M3 release candidate"
git push origin v0.3-rc1
```

Annotated tag (`-a`) carries the message + tagger metadata so
the release is fully self-describing from `git show v0.3-rc1`.
Tag the M3.8 close commit (final release-prep patches including
the m3-acceptance bundle + reproducer doc + this notes file)
rather than M3.7 — the M3.8 patches deliver the release surface
that the `v0.3-rc1` label depends on.
