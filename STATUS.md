# SFC Wave Compiler — Status

## Current milestone

**M3.7 — GUI polish.** Three small, independent additions that
sit cleanly on top of the M3.5/M3.5.1 methodology audit
(deliberately not exposing the gaussian characterization
surface to the GUI per consultant M3.5 audit #16). No encoder
change, no atom PCM change, no driver change, no SPEC contract
change, no M2 baseline change. Reports-only / GUI-only.

**Outcomes:**

- **`rename_sequence_id_cascade` lands** (consultant M3.2 audit
  #13, deferred through M3). `V2EditorModel` gains a cascade
  method mirroring M2.8's `rename_atom_id_cascade`: rename an
  `atom_sequences[idx].id` and propagate the change to every
  `tracks[].kind == AtomSequence { atom_sequence_id }` and to
  `m2.active_sequence_id` when it pointed at the old id.
  Rejects (returns `false`, no mutation) on out-of-range
  `idx`, SPEC §16.6 rule-40 pattern violation
  (`^[a-z0-9_]+$`, length 1..=64), or collision with another
  `atom_sequences[]` entry. The GUI sequence-id text field
  switches from direct mutation to a buffer + cascade call;
  rejected input visibly reverts on the next frame.
- **Atom preview surfaces M3.1 / M3.3 metric fields**
  (consultant M3.5 audit #15). The atom-edit panel grows a
  "Last preview metrics" readout showing `loop_click_abs`
  (color-graded green/yellow/orange/red on 0/≤1000/≤5000/>5000),
  `rotation_offset` ("offset / cycle_len"),
  `peak_abs_error_post_rotation`, and `rms_error_post_rotation`.
  Cached on the `SfcwcApp::last_atom_preview` field — only
  visible when the snapshot's atom_id matches the currently
  selected atom (switching atoms hides the readout until the
  user previews the new selection; self-cleaning across
  project loads since atom ids change).
- **Deliberately NOT surfaced in GUI:**
  - `loop_window_rms_delta` (consultant M3.5 audit #6 —
    diagnostic-only; stays available in the `AtomRenderReport`
    JSON for CLI consumers).
  - Gaussian characterization (consultant M3.5 audit #16 —
    methodology unresolved at M3.5.1; CLI/report-only).
- **Next pass: M3.8 — acceptance + release** (analog of M2.8).
  Tags `v0.3-rc1` after the integrity audit. Closes M3.

### M3.7 phase log

- **Phase A (commit `6a1fc56`)** — `V2EditorModel::rename_sequence_id_cascade(idx, new_id) -> bool`
  added next to `set_sequence_id`. SPEC §16.6 rule 40 pattern
  inlined (no `is_valid_id` import needed). Walks
  `project.tracks` and `project.m2.active_sequence_id` for
  cascade. GUI side: `draw_sequence_edit_panel` id field
  switches from direct-mutate to buffer+cascade; rejected
  edits self-revert on the next frame.
- **Phase B (commit `e8d7300`)** — `SfcwcApp::last_atom_preview:
  Option<AtomPreviewSnapshot>` field added; `do_preview_atom`
  populates it from `AtomBrrOutput` on a successful render.
  `V2ProjectDetailState` gains `last_atom_preview: Option<&'a
  AtomPreviewSnapshot>`; the reference is plumbed through
  `draw_atom_pool_editor` and `draw_atom_edit_panel`. Atom-edit
  panel grows a "Last preview metrics" readout (egui::Grid 2
  cols, 4 rows). `loop_click_color()` helper maps the metric
  to the consultant's green/yellow/orange/red bands.
- **Phase C (commit `56f79cc`)** — six new model-level tests for
  `rename_sequence_id_cascade` (tracks cascade, m2 cascade,
  collision-rejects, invalid-regex-rejects, idx-out-of-range,
  same-id no-op success) and one for the atom preview metric
  flow (`atom_preview_returns_brr_output_with_rotation_offset_populated`
  — block-alignment + finite-field invariants on the
  `AtomBrrOutput` the GUI snapshot reads from). Existing 3
  round-trip parity tests verified untouched.
- **Phase D (this entry).**
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **585 tests
  workspace-wide** (was 578 at M3.5.1 close; +7 new tests
  from Phase C; no existing tests broken).

### Decisions log additions (M3.7)

- `rename_sequence_id_cascade` model method + GUI wiring
  (consultant M3.2 audit #13). The cascade mirrors M2.8's
  atom rename but adds explicit regex / length validation
  inline since `is_valid_id` is `pub(crate)` in
  `core::project` and not visible to the `app` crate.
- GUI atom preview surfaces the four M3.1 / M3.3 metric fields
  (consultant M3.5 audit #15 addition).
- `loop_window_rms_delta` deliberately not surfaced in GUI
  (consultant M3.5 audit #6 — diagnostic-only).
- Gaussian characterization deliberately not surfaced in GUI
  (consultant M3.5 audit #16 — methodology unresolved at
  M3.5.1; CLI/report-only).
- No SPEC change; no encoder change; no atom render formula
  change; no M2 baseline change.
- **Engineer observation (informational, not blocking).** The
  M2.8 `rename_atom_id_cascade` GUI wiring also uses
  direct-mutate of `atom_pool[idx].id` rather than calling
  the cascade method. The cascade method exists on the model
  for CLI / test use but isn't invoked from the GUI today.
  M3.7 wires the *sequence* cascade into the GUI per this
  brief; the atom-side direct-mutate stays unchanged. PM may
  want to revisit the atom-side wiring at M3.8 prelude for
  consistency.

**Previous milestone (M3.5.1) — Gaussian characterization
methodology audit.**
Reports-only adjustments to M3.5. No encoder change, no atom
PCM change, no driver change, no M2 baseline change. Per
consultant M3.5 audit, the M3.5 absolute `gain_delta_db` curve
and ZCR ratios are too anomalous to trust as input for M3.6
pre-emphasis preset design.

**Outcome:**

- **M3.6 pre-emphasis preset implementation is DEFERRED to M4+.**
  M3.5 had returned `pending_preset_eval`; M3.5.1's
  precondition #0 re-evaluates the same characterization and
  returns `methodology_review` instead.
- **Schema bumped v2 → v3** (SPEC §10.9). `measurements[]`
  gains seven methodology diagnostic fields +
  `gain_delta_db_aligned`. Optional top-level
  `_methodology_audit_m3_5_1` records anomaly fingerprints
  when they fire.
- **Decision rule precondition #0** added: `zcr_ratio ∈
  [0.9, 1.1]` for all monotonicity-anchor signals
  (sine_cycle_64/128/256, harmonic_2/4/8/16_cycle_64), OR a
  documented methodology explanation. On failure
  `recommended_next` short-circuits to `"methodology_review"`
  without evaluating conditions #1–#4.
- **Re-run confirms anomalies:** `zcr_ratio` measures 1.93,
  1.93, 1.94 for sine_cycle_64/128/256 and 2.57, 2.20, 2.06,
  **1.00** for harmonic_2/4/8/**16**. Only the near-Nyquist
  harmonic_16 falls inside the sanity band — the rest are
  ~2×, the same ZCR-doubling pattern the M3.5 raw eprintln
  showed.
- **Next pass: M3.7 — GUI polish.** M3.6 is SKIPPED entirely.
  M3.7 covers `rename_sequence_id_cascade` + surfacing
  loop-click metrics + `rotation_offset` in atom preview /
  report (consultant audit #15 with one addition).
- **M4 prelude scope (informational):** the methodology
  resolution requires investigating the brute-force
  `align_oracle_to_raw` (`max_offset = 32` cannot resolve
  cycle lengths > 32 samples), the `+2.6 dB` low-frequency
  oracle boost (gaussian kernel coefficient sum vs DSP
  master-vol scaling), and the BRR encoder error magnitude
  (`peak_abs_raw_vs_source ≈ 18431` across all signals — the
  dominant atom-render artefact per consultant audit #14).

**M3.5.1 per-signal re-run table (frames = 16000, sample_rate
= 32 kHz):**

| Signal | f (Hz) | gain_delta_db | gain_delta_db_aligned | zcr_ratio | corr | peak_err | peak_after_norm | align_off |
|---|---|---|---|---|---|---|---|---|
| sine_cycle_64 | 500 | +2.645 | +2.645 | **1.93** | +0.056 | 36237 | 31025 | 13 |
| sine_cycle_128 | 250 | +2.663 | +2.663 | **1.93** | +0.024 | 36788 | 30995 | 11 |
| sine_cycle_256 | 125 | +2.673 | +2.673 | **1.94** | +0.013 | 39053 | 33047 | 32 |
| harmonic_2_cycle_64 | 1000 | +2.580 | +2.581 | **2.57** | +0.261 | 39053 | 33226 | 23 |
| harmonic_4_cycle_64 | 2000 | +2.334 | +2.335 | **2.20** | +0.492 | 39053 | 33710 | 7 |
| harmonic_8_cycle_64 | 4000 | +1.593 | +1.594 | **2.06** | +0.598 | 39053 | 35253 | 7 |
| harmonic_16_cycle_64 | 8000 | -1.231 | -1.231 | **1.00** | +0.983 | 16384 | 16384 | 31 |
| all_8_partials | 250 | +2.530 | +2.547 | 0.97 | +0.191 | 29349 | 24008 | 25 |
| normalize_false_clamp | 250 | +2.452 | +2.454 | 1.54 | +0.568 | 39053 | 33473 | 25 |

The bold `zcr_ratio` column is the precondition #0 trigger —
seven of nine signals land outside `[0.9, 1.1]`.

**Phases.**

- **Phase A (commit `4c28f68`)** — `core::characterize_gaussian`
  gains seven `Measurement` fields: `alignment_best_offset`,
  `aligned_raw_rms`, `aligned_oracle_rms`,
  `normalized_correlation`, `zcr_ratio`,
  `first_8_zero_crossings_raw`, `first_8_zero_crossings_oracle`,
  `peak_abs_error_after_gain_normalization`. Three new helpers:
  `first_n_zero_crossings`, `pearson_correlation`,
  `peak_abs_error_after_gain_normalization`. 10 new unit tests
  including a hermetic end-to-end
  (`methodology_diagnostics_populated_for_sine_cycle_128`) and
  the sanity-test `zcr_ratio_near_1_for_clean_sine_cycle_64`.
  Existing 3 decision-rule tests refactored to populate the new
  fields with `zcr_ratio = 1.0` (in-band) so they don't trip
  the Phase C precondition.
- **Phase B (commit `bfec32a`)** — `gain_delta_db_aligned` added
  to `Measurement`. Uses aligned-window RMS on both sides per
  consultant M3.5 audit #3; the original `gain_delta_db` stays
  alongside as documentary. Window-form bias proves small in
  practice (≤ 0.017 dB max across the signal set).
- **Phase C (commit `95490e0`)** — SPEC §10.9 schema bumped to
  `v3`; field semantics documented for the new diagnostics;
  decision rule precondition #0 added (zcr_ratio sanity band).
  `apply_m3_5_decision_rule` implements the short-circuit;
  `PRECONDITION_ANCHOR_SIGNALS` + `PRECONDITION_ZCR_RATIO_LOW/_HIGH`
  exposed publicly. `MethodologyAudit` struct added.
  `CharacterizationReport` gains optional
  `_methodology_audit_m3_5_1`. CLI's `cmd_characterize_gaussian`
  builds the audit when M3.5 anomaly fingerprints fire and
  emits schema_version=3 reports. 2 new precondition tests.
- **Phase E (commit `894ee83`)** — cosmetic: rename
  `signal_set_has_ten_signals` → `_nine_signals` and update the
  "Ten signals" doc-comment to "Nine signals" (consultant
  audit #13).
- **Phase D (commit `165cbc5`)** — re-ran `sfcwc characterize-gaussian`
  against the M3.5.1 implementation. 9 SPCs built, 9 oracle
  renders, 9 measurements. `baselines/m3.json` gains 45
  per-signal diagnostic entries + 4 summary entries
  (`M3_5_1_PRECONDITION_OUTCOME`,
  `M3_5_1_ANOMALIES_OBSERVED_COUNT`,
  `M3_5_1_M3_6_DECISION`, `M3_5_1_DOCUMENTARY_CLASS_NOTE`).
  `M3_5_RECOMMENDED_NEXT` shifts from `pending_preset_eval`
  to `methodology_review`.
- **Phase F (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **578 tests
  workspace-wide** (was 560 at M3.5 close; +12 from Phase A +
  Phase C tests; +6 from the smaller helper coverage —
  helpers added in Phase A as `pearson_correlation`,
  `peak_abs_error_after_gain_normalization`,
  `first_n_zero_crossings`).

### Engineer's interpretation of the re-run

- **ZCR doubling is intrinsic.** Confirmed across both M3.5 and
  M3.5.1 runs. The brute-force `align_oracle_to_raw` with
  `max_offset = 32` can't resolve cycle lengths > 32 samples
  (sine_cycle_128 has period 128 samples — 4× the search
  range), so it picks a phase that minimises aligned RMS at the
  cost of phase coherence. The `normalized_correlation` field
  exposes this: 0.013–0.056 for low-frequency sines vs 0.983
  for harmonic_16_cycle_64 (which has period 4 samples, well
  inside the search range).
- **gain_delta_db_aligned tells the same story as the raw form.**
  Max delta is 0.017 dB (all_8_partials_max_amp_harmonics_1_to_8).
  The window-form bias the new field was added to expose turns
  out to be small in absolute terms — the diagnostic value is
  in confirming it's not the cause of the +2.6 dB anomaly.
- **Shape vs gain.** `peak_abs_error_after_gain_normalization`
  ranges 30995–35253 across the low/mid signals vs
  `peak_abs_error_oracle_vs_raw` 36237–39053. Gain
  normalization reduces error by only ~14%. Raw and oracle
  differ predominantly in shape, not amplitude — consistent
  with the alignment-artefact hypothesis above.
- **harmonic_16 is the only clean measurement.** zcr_ratio =
  1.00, correlation = 0.983, gain_delta_db = -1.231 dB. The
  gaussian dulling at near-Nyquist is real and well-measured.
  If a future pass narrows the characterisation to
  high-frequency signals only (where the alignment search range
  is adequate), the data may be usable.

**Stop conditions hit:** none. All four stop conditions from the
brief (no anomaly, gain-norm doesn't help, precondition breaks
existing tests, M2 acceptance regression) checked; none fired.
The existing 3 decision-rule tests were preemptively refactored
in Phase A to set `zcr_ratio = 1.0` so they continue to
exercise conditions #1/#2 logic post-precondition. 560 → 578
test count progression confirms no test regressions.

**Spec ambiguities flagged:** none new beyond consultant M3.5
audit's framing.

**Previous milestone (M3.5) — Gaussian characterization.**
Reports-only pass (now superseded by M3.5.1). M3.5
`recommended_next = pending_preset_eval` was the initial
"go signal" for M3.6 preset design; M3.5.1 re-evaluates the
same measurement set under precondition #0 and downgrades to
`methodology_review`. M3.5 baselines stay in
`baselines/m3.json::documentary_snapshot::M3_5_*` for
reproducibility cross-reference.

**Previous milestone (M3.3) — Phase rotation implementation.**
First encoder-shifting pass of M3. SPEC §10.7 phase rotation
lands: block-aligned candidate offsets, lexicographic objective
`(loop_click_abs, peak_abs_error, rms_error, rotation_offset)`,
`f64::total_cmp` for the floating-point lex level, smallest-offset
tie-break. Rotation operates on a *transient* encoder input — the
stored atom PCM stays untouched per the SPEC §16.9 atom PCM
stability amendment.

**Improvement gate satisfied for all 11 atom fixtures.**
`loop_click_abs` post-rotation is `≤` the pre-M3 value for every
fixture, enforced by
`phase_rotation_loop_click_never_regresses_against_pre_m3` which
iterates the M3.0 + M3.1 + M3.2 fixture set against
`baselines/m3.json`. 9 of 11 fixtures dropped to
`loop_click_abs = 0`; 1 saw an 87% reduction
(`normalize_false_multi_partial_clamp_safety`: 16384 → 2048); 1
saw no improvement (`harmonic_16_cycle_64`: near-Nyquist content
where any block-aligned rotation produces the same seam — lex
correctly defaulted to `offset = 0`).

**Tie-breaker pinned.** `amplitude_zero_atom_phase_rotation_picks_offset_zero`
exercises the load-bearing case: all-zero PCM → every candidate
scores `(0, 0, 0.0, offset)` → smallest-offset tie-break selects
`rotation_offset = 0`. Block test per consultant M3.2 audit #16.

**M2 acceptance pre-check (consultant M3.2 audit #20): bundle.status=ok.**
Ran `m2-acceptance` against `fixtures/projects/canonical_m2/canonical_m2.sfcproj.json`
post-rotation; all four stage rollups green. The LEFT (sample)
channel reports `max_abs = 15624`, `rms = 11034` (≥ 1000/200
audibility floor); the RIGHT (atom) channel reports
`max_abs = 25706`, `rms = 20418` (also ≥ floor). Source-step ZCR
ratio: `pre.right.zcr = 1001.5` → `post.right.zcr = 2000.4` →
ratio ≈ **1.997** (≥ 1.5 minimum per
`M2_SOURCE_STEP_ZCR_RATIO_FLOOR`). No regression on any M2 gate.

545 tests workspace-wide (was 543 at M3.2; +2 from the
tie-breaker test + improvement-gate test). 3 ignored
(unchanged). 5 pre-existing M2.2 BRR-SHA-pinned tests updated to
the post-rotation values (`cli_render_atom_happy_path`,
`atom::tests::brr_loop_click_score_for_pure_sine_post_rotation_is_zero`,
`atom::tests::atom_render_baselines_post_rotation_pinned`,
`atom::tests::brr_round_trip_at_m1_reference_amp_within_atom_envelope`
— now compares decoded against rotated source per §10.7,
`render_canonical_atoms_match_locked_sha_baselines`).

`baselines/m3.json` expanded: +1 `behavior_gated`
(`M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE`) + 77 new
`documentary_snapshot` entries (7 per fixture × 11 fixtures —
`rotation_offset`, `loop_click_abs`, BRR SHA, decoded-BRR PCM
SHA, peak/rms error, windowed RMS delta). All 11 PCM SHAs in
`identity_gated` carry forward unchanged. `baselines/m2.json`
gets `_same_numeric_value_as` cross-reference fields on the
M2.2-era `M2_ATOM_*_LOOP_CLICK_SCORE` entries pointing at their
M3.1 counterparts (consultant M3.2 audit #21).

One on-disk project fixture committed:
`fixtures/projects/atom_edge_cases/harmonic_16_cycle_64.sfcproj.json`
+ `README.md` — atoms-only v2 project (empty `sample_pool` via
SPEC §16.6 M2.5 relaxation, one atom on voice 0). Reproduces the
M3.2 `harmonic_16_cycle_64` fixture end-to-end through
`sfcwc render-atom`; report fields match `baselines/m3.json`
documentary values byte-exactly. Per consultant M3 plan #12 the
remaining eight M3.2 edge-cases stay synthesized in tests.

**M3.4 next.** Predictor optimization (SPEC §10.8 conditional).
Goes ahead only if PM judges phase-rotation gains insufficient
against the M3.0 loop-click target AND the consultant M3 plan
beam-search proposal is expected to add measurable improvement
above and beyond rotation. With 9 of 11 fixtures already at
`loop_click_abs = 0`, the gain envelope for M3.4 is narrow —
mostly `harmonic_16_cycle_64` and
`normalize_false_multi_partial_clamp_safety` (the two fixtures
where rotation either didn't help or only got partway there).
PM go/defer decision at M3.4 entry brief.

## Last pass

**Pass M3.7 — GUI polish (Phases A–D).** Detail folded into the
"Current milestone" section above. Three independent additions:
sequence-id rename cascade with reference updates, atom preview
metric readout surfacing M3.1 / M3.3 fields, plus tests + STATUS.
No encoder / SPEC / baseline change.

---

**Pass M3.5.1 — Gaussian characterization methodology audit
(Phases A–F).** Detail folded into prior STATUS entries; the
bullets below capture the **decisions log additions** specific
to that pass.

### Decisions log additions (M3.5.1)

- **M3.6 pre-emphasis preset implementation DEFERRED to M4+** per
  consultant M3.5 audit #9, #19. Methodology audit M3.5.1
  documents why: the M3.5 raw gain curve is dominated by an
  alignment artefact at low frequencies (the brute-force
  `align_oracle_to_raw` with `max_offset = 32` cannot resolve
  cycles longer than 32 samples).
- **7 methodology diagnostic fields added to schema v3**
  (consultant audit #4): `alignment_best_offset`,
  `aligned_raw_rms`, `aligned_oracle_rms`,
  `normalized_correlation`, `zcr_ratio`,
  `first_8_zero_crossings_raw`, `first_8_zero_crossings_oracle`,
  `peak_abs_error_after_gain_normalization`.
- **`gain_delta_db_aligned` alternative form added** (consultant
  audit #3). Uses aligned-window RMS on both sides. Window-form
  bias proves small in practice (≤ 0.017 dB) — the diagnostic
  value is in confirming the +2.6 dB low-frequency anomaly is
  not a window-mismatch artefact.
- **Decision rule precondition #0 added** (consultant audit #8):
  `zcr_ratio ∈ [0.9, 1.1]` for all seven monotonicity-anchor
  signals. On failure the rule short-circuits to
  `recommended_next = "methodology_review"`.
- **Cosmetic** (consultant audit #13): `signal_set_has_ten_signals`
  → `_nine_signals` test name + matching doc-comment update.
- **ZCR-doubling anomaly persists in M3.5.1 re-run** (expected
  per consultant interpretation). 7 of 9 signals trip
  precondition #0; methodology resolution deferred to M4
  prelude investigation.
- **BRR encoder error magnitude confirmed as dominant
  atom-render artefact** (consultant audit #14):
  `peak_abs_raw_vs_source ≈ 18431` across all signals. This
  informs M4 scope — BRR encoder quality is the leading
  candidate for next-pass investment, not pre-emphasis.
- **Next pass: M3.7 — GUI polish.** `rename_sequence_id_cascade`
  + surface loop-click metrics and `rotation_offset` in atom
  preview/report (consultant audit #15 with one addition).
- **No M2 acceptance pre-check:** pass is reports-only; no
  encoder or driver bytes changed.

---

**Pass M3.5 — Gaussian characterization (Phases 2.5A/B/C + 3-6).**

Reports-only pass per consultant M3.3 audit #21 (M3.4 predictor
optimization deferred to M4+). Three audition-driven SPEC /
baseline amendments + new `characterize_gaussian` module + new
CLI subcommand + 9-signal characterization run + 112 new
`documentary_snapshot` entries + STATUS. No encoder change; no
atom PCM change; no driver change.

- **Phase 2.5A (commit `180a457`)** — SPEC §10.9 amendment.
  Expanded `m3_5_canonical` test signal set from 6 to 9 signals
  with a four-point cycle_64 harmonic gain curve
  (`harmonic_2/4/8/16_cycle_64`). Rewrote the M3.6 decision rule
  to four conditions: monotonic `gain_delta_db`, `harmonic_16`
  responds (≥25% reduction under proposed preset), anti-worsening
  on canonical sines (≤10% peak/rms error increase), no new
  clipping. Characterization report schema bumped to
  `schema_version: 2` with separated raw/oracle SHAs,
  `peak_abs_raw_vs_source` (BRR encoder error), ZCR and
  clipping counts.
- **Phase 2.5B (commit `388fc52`)** — `_audition_note` on all
  fourteen `_PHASE_ROTATION` entries for pairs 4/5
  (`NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY`,
  `ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8`). Pre-rotation
  metric improvements (87% / 100% loop_click reduction
  respectively) were real but perceptually masked at audition;
  notes distinguish these from pairs 1/2 where metric and
  perception aligned.
- **Phase 2.5C (commit `9cc23be`)** — optional top-level
  `subjective_audition` field in the characterization report
  schema. `perceived_change_axis` enum
  (`seam_click | harmonic_content | harshness | none`) +
  `masked_by_signal_content` bool. Lets future audition runs
  document metric-vs-perception mapping without contaminating
  the deterministic `measurements` array.
- **Phase 3 (commit `72fd005`)** — new `core/src/characterize_gaussian.rs`
  module: 9-signal `m3_5_canonical_signals()` builder
  matching the SPEC; raw-side metric helpers (`pcm_rms`,
  `pcm_zcr_per_sec`, `pcm_clipping_count`, `pcm_sha256_hex`,
  `decode_brr_flat`, `oracle_stereo_to_mono_left`,
  `tile_cycle_to_length`, `align_oracle_to_raw`);
  `compute_raw_side` + `finalize_measurement` combinators;
  `apply_m3_5_decision_rule` implementing conditions #1 (monotonicity)
  and #2 (raw form of `harmonic_16` responds); `CharacterizationReport`
  + `Measurement` + `SubjectiveAudition` + `Summary` types
  matching the SPEC §10.9 `schema_version: 2` shape. 14 new unit
  tests. Plus new `app/src/main.rs` `sfcwc characterize-gaussian`
  subcommand orchestrating: build single-atom V2 project per
  signal → spawn `compile-spc` → spawn oracle → host BRR decode
  → finalize measurement → write report.
- **Phase 3 fix-up (commit `90b05b6`)** — duration_ticks tuned
  from 600 to 240 to fit SPEC §16.6 u8 bound (caught running
  the command locally).
- **Phase 4 (commit `d0d0eee`)** — `sfcwc characterize-gaussian`
  invoked against the M2 driver + snes_spc oracle: 9 SPCs
  built, 9 oracle renders, 9 measurements computed.
  `baselines/m3.json` gains 112 `documentary_snapshot` entries
  in the `M3_5_*` namespace: 12 per signal (`FREQUENCY_HZ`,
  `RAW_DECODED_PCM_SHA256`, `ORACLE_PCM_SHA256`, `RAW_RMS`,
  `ORACLE_RMS`, `GAIN_DELTA_DB`, `PEAK_ABS_ERROR_ORACLE_VS_RAW`,
  `PEAK_ABS_RAW_VS_SOURCE`, `ZCR_RAW`, `ZCR_ORACLE`,
  `CLIPPING_COUNT_RAW`, `CLIPPING_COUNT_ORACLE`) plus four
  summary entries (`RECOMMENDED_NEXT`,
  `MONOTONICITY_HOLDS_ACROSS_CYCLE_64_HARMONIC_SERIES`,
  `HARMONIC_16_GAUSSIAN_ATTENUATION_DB`,
  `DOCUMENTARY_CLASS_NOTE`).
- **Phase 5 — decision rule outcome.** Recorded in the
  `M3_5_RECOMMENDED_NEXT` baseline entry. Outcome:
  `pending_preset_eval`. Conditions #1 (monotonicity) and #2
  (raw `harmonic_16` responds) both pass; conditions #3
  (anti-worsening) and #4 (no new clipping) require a proposed
  preset's outputs and stay unevaluated at M3.5 — the
  characterization-only pass does not design a preset.
  `M3.6` may now design a gentle preset and re-run
  `characterize-gaussian` against the preset's outputs.
- **Phase 6 (this entry).**
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **560 tests
  workspace-wide** (was 546 at M3.5 Phase 1-2 close; +14 from
  the new `core::characterize_gaussian` unit tests).
  4 ignored (unchanged from Phase 2: `m3_5_emit_audition_wavs`
  + the `m3_2_print` + 2 other pre-existing).

### Per-signal M3.5 characterization table

| Signal | f (Hz) | gain_delta_db | raw_rms | oracle_rms | peak_err | peak_raw_vs_src | zcr_raw | zcr_oracle | clip_raw / oracle |
|---|---|---|---|---|---|---|---|---|---|
| `sine_cycle_64` | 500 | +2.645 | 13774 | 18678 | 36237 | 18431 | 1000 | 1930 | 0 / 0 |
| `sine_cycle_128` | 250 | +2.663 | 13732 | 18659 | 36788 | 18431 | 500 | 965 | 0 / 0 |
| `sine_cycle_256` | 125 | +2.673 | 13807 | 18782 | 39053 | 18431 | 250 | 485 | 0 / 0 |
| `harmonic_2_cycle_64` | 1000 | +2.580 | 13687 | 18420 | 39053 | 18431 | 1500 | 3862 | 0 / 0 |
| `harmonic_4_cycle_64` | 2000 | +2.334 | 13633 | 17837 | 39053 | 18431 | 3500 | 7717 | 0 / 0 |
| `harmonic_8_cycle_64` | 4000 | +1.593 | 13332 | 16016 | 39053 | 18431 | 7500 | 15437 | 0 / 0 |
| `harmonic_16_cycle_64` | 8000 | **-1.231** | 10885 | 9446 | 16384 | 16384 | 15500 | 15460 | 0 / 0 |
| `all_8_partials_max_amp_harmonics_1_to_8` | 250 | +2.530 | 6970 | 9326 | 29349 | 18431 | 4000 | 3864 | 0 / 0 |
| `normalize_false_multi_partial_clamp_safety` | 250 | +2.452 | 12543 | 16634 | 39053 | 18431 | 3750 | 5791 | 0 / 0 |

Frame count: 16000 (0.5 s @ 32 kHz) per signal. `peak_abs_error_oracle_vs_raw`
values are large for the low-frequency signals because the gaussian's
~+2.6 dB boost on steady-state low-frequency content is a multiplicative
gain difference, not an alignment artifact — the aligned RMS of the
delta is still bounded.

### Decisions log additions (M3.5 Phase 2.5 + 3-6)

- SPEC §10.9 expanded `m3_5_canonical` to 9 signals + four-point
  cycle_64 harmonic gain curve (consultant audition audit #9).
- M3.6 decision rule rewritten to four conditions (consultant
  audit #10, #12). `harmonic_16` is the primary perceptual
  stress fixture and gets its own condition.
- Characterization report schema bumped to `schema_version: 2`
  with separated raw / oracle SHAs, BRR encoder error
  (`peak_abs_raw_vs_source`), ZCR, clipping count (consultant
  audit #13).
- Optional `subjective_audition` top-level field with
  `perceived_change_axis` enum + `masked_by_signal_content`
  (consultant audit #7). Tracks the pair-4/5 audition finding
  in a future-proof shape.
- `_audition_note` on 14 `_PHASE_ROTATION` baseline entries for
  pairs 4/5: real metric improvement, no audible difference
  (consultant audit #4, #5).
- M3.5 raw-form decision rule passes: monotonic, `harmonic_16`
  attenuates -1.231 dB. `recommended_next=pending_preset_eval`
  is the "go signal" for M3.6 preset design — but M3.6 ship
  still requires the full four-condition rule against a
  proposed preset's outputs.
- `characterize_gaussian` host-side raw decoder reuses
  `core::brr::decode_blocks` against the encoded BRR; oracle
  alignment is brute-force ≤ 32 sample skip with
  `f64`-precision aligned RMS. Documented in the
  `_phase_or_delay_note` per-measurement field when alignment
  was non-zero.
- Gaussian behavior at native pitch (default voice setup with
  root_midi_note = 60): low-frequency boost of ~+2.6 dB
  steady-state across all `sine_cycle_*` fixtures, falling to
  -1.231 dB at `harmonic_16_cycle_64`. ~3.9 dB total span
  across the cycle_64 harmonic series.
- No M2 acceptance pre-check: pass is reports-only, no encoder
  or driver bytes changed.

## Previous passes

**Pass M3.3 — Phase rotation implementation.**

Nine phases, large pass. SPEC amendment + core implementation +
report wiring + baseline expansion + tie-breaker + improvement
gate + M2 acceptance pre-check + committed project fixture +
STATUS. No predictor optimization (M3.4); no pre-emphasis (M3.5+);
no GUI (M3.7); atom PCM stays locked per SPEC §16.9.

- **Phase 0 (consultant M3.2 audit #13, #25):** SPEC §10.7
  amended to lock the error-comparison sources. `peak_abs_error`
  and `rms_error` compare decoded BRR PCM against the **rotated**
  source PCM (not the unrotated original) — otherwise rotation
  candidates would be penalized for phase displacement (which is
  the literal definition of rotation), making rotation appear
  artificially worse. Numeric types locked: `loop_click_abs:
  i32`, `peak_abs_error: i32`, `rms_error: f64` (from i64
  sum-of-squares, single final `sqrt`), `rotation_offset: u32`.
  `f64::total_cmp` for the floating-point lex level documents
  intent and removes any latent NaN-ordering footgun. Tie-break
  to offset 0 explicitly pinned by a regression test.
- **Phase A:** four new pure helpers in `core::atom`:
  `rotation_candidate_offsets(cycle_len) → Vec<usize>`,
  `rotate_pcm(source, offset) → Vec<i16>`,
  `peak_abs_error(rotated_source, decoded) → i32`,
  `rms_error(rotated_source, decoded) → f64`. Plus
  `RotationCandidate` struct and `pick_best_rotation` selector
  using `min_by` with the spec-locked lex tuple.
- **Phase B:** `render_to_brr` reworked. Source PCM is rendered
  once (identity-gated per §16.9). For each block-aligned
  candidate offset: rotate source → encode → decode → score lex
  tuple. Pick the lex-min candidate. `AtomBrrOutput` gains
  `rotation_offset: u32`, `peak_abs_error_post_rotation: i32`,
  `rms_error_post_rotation: f64`. Mirror fields land on
  `AtomRenderReport` with `#[serde(default)]` for pre-M3.3 report
  back-compat. Report round-trip tests in `core/src/report.rs`
  + the `atom_render_report` builder in `app/src/main.rs`
  updated. Five pre-existing M2.2 BRR-SHA-pinned tests updated to
  the post-rotation values; the M1 round-trip test
  (`brr_round_trip_at_m1_reference_amp_within_atom_envelope`)
  refactored to compare decoded against `rotate_pcm(&out.pcm,
  out.rotation_offset)` — the meaningful round-trip is against
  the encoder's actual input, not the unrotated source.
- **Phase C (consultant M3.2 audit #8, #19):** all 11 atom
  fixtures rendered through the rotation path. 77 new
  `documentary_snapshot` entries in `baselines/m3.json`:
  7 per fixture (`ROTATION_OFFSET`, `LOOP_CLICK_ABS`,
  `BRR_SHA256`, `DECODED_BRR_PCM_SHA256`, `PEAK_ABS_ERROR`,
  `RMS_ERROR`, `LOOP_WINDOW_RMS_DELTA` — all `_PHASE_ROTATION`
  suffix per consultant naming guidance). Documentary, not
  identity-gated: M3.4 predictor / M3.6 pre-emphasis may shift
  these further.
- **Phase D (consultant M3.2 audit #16):**
  `amplitude_zero_atom_phase_rotation_picks_offset_zero`
  (Block) — pins the tie-break to offset 0 for all-zero PCM.
  `phase_rotation_loop_click_never_regresses_against_pre_m3` —
  iterates all 11 fixtures, parses pre-M3 +
  PHASE_ROTATION entries from `baselines/m3.json`, asserts
  `post <= pre`. Both pass; the improvement gate is
  enforced by a single workspace test now.
- **Phase E (consultant M3.2 audit #20):** ran `m2-acceptance`
  against `fixtures/projects/canonical_m2/canonical_m2.sfcproj.json`
  post-rotation. `bundle.status=ok`; all four stage rollups
  green. Post-rotation canonical compile SHAs:
  - `driver_code_sha256_a`:
    `342ab3ec16a6dcbc2e6b8102b58d3b4f44412877af08201124d6c3a11d2f4804`
    (M2 multi_voice_atom driver — not the identity-gated M1
    driver SHA which still matches its locked baseline)
  - `spc_sha256_a`:
    `9f7f161054521c3550618adb3c090d98aa5fe56743cd7e385110b53eb478efc4`
  - LEFT channel (sample voice): `max_abs = 15624`,
    `rms = 11034`, `zcr = 999.8` (all ≥ M1/M2 audibility floors)
  - RIGHT channel (atom voice): `max_abs = 25706`,
    `rms = 20418`, `zcr = 1550.8`
  - Source-step ZCR ratio (consultant M2.5 §21):
    `post.right.zcr / pre.right.zcr ≈ 2000.4 / 1001.5 ≈ 1.997`
    (≥ 1.5 minimum `M2_SOURCE_STEP_ZCR_RATIO_FLOOR`)
- **Phase F (consultant M3.2 audit #21):**
  `baselines/m2.json` gains `_same_numeric_value_as` fields on
  `M2_ATOM_128_SINE_LOOP_CLICK_SCORE` and
  `M2_ATOM_64_SINE_LOOP_CLICK_SCORE` pointing at their M3.1
  pre-M3 counterparts. No retirement.
- **Phase G (consultant M3.2 audit #12):** one on-disk project
  fixture committed:
  `fixtures/projects/atom_edge_cases/harmonic_16_cycle_64.sfcproj.json`
  + `README.md`. Reproducible via `sfcwc render-atom --project
  … --atom harmonic_16_cycle_64 --out-report …`. Engineer chose
  `harmonic_16_cycle_64` over `amplitude_zero` because the
  near-Nyquist content is the most likely M3.4 / M3.6 stress
  vector and the boundary case where rotation correctly found
  no improvement (informative reproducer).
- **Phase H (this entry).**
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **545 tests
  workspace-wide** (was 543 at M3.2; +2 from
  `amplitude_zero_atom_phase_rotation_picks_offset_zero` +
  `phase_rotation_loop_click_never_regresses_against_pre_m3`).

### Per-fixture phase rotation table

| Fixture | offset | loop_click pre → post | improvement | post peak_abs_error | post rms_error |
|---|---|---|---|---|---|
| `128_SINE` | 96 | 1197 → 0 | -1197 (100%) | 9582 | 4795.19 |
| `64_SINE` | 48 | 2407 → 0 | -2407 (100%) | 10239 | 5108.55 |
| `AMPLITUDE_ZERO` | 0 | 0 → 0 | 0 (tie-break) | 0 | 0 |
| `ALL_PARTIALS_ZERO_NORMALIZE_TRUE` | 0 | 0 → 0 | 0 (tie-break) | 0 | 0 |
| `TWO_PARTIALS_CANCEL_PARTIALLY` | 32 | 1024 → 0 | -1024 (100%) | 10239 | 1532.33 |
| `MAX_AMPLITUDE_NO_NORMALIZE` | 16 | 16032 → 0 | -16032 (100%) | 18431 | 10576.55 |
| `NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY` | 16 | 16384 → 2048 | -14336 (87%) | 18431 | 10562.45 |
| `HARMONIC_16_CYCLE_64` | 0 | 16384 → 16384 | 0 (no improvement) | 18431 | 12329.89 |
| `ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8` | 16 | 10240 → 0 | -10240 (100%) | 18431 | 4574.39 |
| `PHASE_CYCLES_0_9999` | 96 | 1196 → 0 | -1196 (100%) | 9581 | 4797.03 |
| `CYCLE_256_CANONICAL_SINE` | 224 | 606 → 0 | -606 (100%) | 9995 | 4920.93 |

### Decisions log additions (M3.3)

- SPEC §10.7 amended (Phase 0): error-comparison sources locked
  to `decoded vs rotated_source`; numeric types locked
  (i32/i32/f64/u32); `f64::total_cmp` for the floating lex
  level; tie-break to offset 0 explicitly pinned by a
  regression test.
- Phase rotation implementation per SPEC §10.7: block-aligned
  candidate offsets `[0, 16, 32, ..., cycle_len - 16]`; lex
  `min_by` selector; `RotationCandidate` struct exposes the
  intermediate state for tests.
- 11 atom fixtures × 7 post-rotation entries (rotation_offset
  / loop_click_abs / BRR SHA / decoded-BRR PCM SHA / peak / rms
  / windowed RMS delta) added to
  `baselines/m3.json::documentary_snapshot`. Naming uses the
  `_PHASE_ROTATION` semantic suffix per consultant M3.2 audit
  #8; not yet identity-gated (M3.4 / M3.6 may shift further per
  consultant audit #19).
- Behavior gate added: `M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE`
  (post ≤ pre per fixture). Single workspace test enforces.
- Tie-breaker test pins all-tied lex tuples → smallest offset
  (consultant audit #16). Block-level test.
- M2 acceptance pre-check passes; all four stages green; no
  audibility / silence / source-step / module-cap regression
  (consultant audit #20). This becomes the basis for M3.8
  acceptance.
- `baselines/m2.json` gains `_same_numeric_value_as` cross-refs
  on M2.2 loop_click_score entries pointing at their M3.1
  counterparts (consultant audit #21). No retirement.
- One on-disk edge-case project fixture committed
  (`harmonic_16_cycle_64.sfcproj.json` + README) per consultant
  audit #12; remaining eight M3.2 fixtures stay synthesized.
- Atom PCM SHAs unchanged across M3.3: all 11 identity-pin
  tests pass unchanged; SPEC §16.9 stability amendment
  preserved.
- Five pre-existing M2.2 BRR-SHA-pinned tests updated to
  post-rotation values; the M1 BRR round-trip test refactored
  to compare decoded against `rotate_pcm(&out.pcm,
  out.rotation_offset)` — meaningful fidelity is against the
  encoder's actual input.
- Encode runtime: M3.3 encodes once per candidate offset (4 for
  cycle 64, 8 for cycle 128, 16 for cycle 256). The 16×
  worst-case for the cycle-256 fixture stays well under the 2×
  M2.2 ceiling per consultant SPEC §10.8 — for the canonical
  rendering paths atom encode is microseconds, not a runtime
  bottleneck.
- **M3.4 go/defer judgment, narrowed:** with 9 of 11 fixtures
  already at `loop_click_abs = 0` after rotation, the gain
  envelope for predictor beam-search is mostly
  `harmonic_16_cycle_64` (16384, unchanged) and
  `normalize_false_multi_partial_clamp_safety` (2048, ~87%
  already there). PM at M3.4 entry decides whether the residual
  is worth the SPEC §10.8 conditional ship.

**Pass M3.2 — Atom edge case fixture coverage.** Synthesized
fixture additions only; no encoder changes; no phase rotation
(M3.3); no committed-on-disk fixtures (deferred to M3.3 prelude
per consultant M3 plan #12). Nine new edge-case atoms
programmatically constructed in `core/tests/atom_edge_cases.rs`,
broadening the atom render → BRR encode → metric input space
before encoder optimization runs against it at M3.3+.

The nine fixtures and what each surfaces:

1. **amplitude_zero** — load-bearing for the M3.3
   phase-rotation tie-breaker. All-zero PCM →
   `loop_click_abs = 0`; all candidates tie at score zero; lex
   objective falls through to peak/rms/offset which must
   default to no-rotation.
2. **all_partials_zero_normalize_true** — exercises the
   normalize `max == 0` special case. Render skips the divide
   cleanly; no NaN. Same all-zero output as amplitude_zero.
3. **two_partials_cancel_partially** — surfaces an
   FP-noise-amplification path. `sin(θ) + sin(θ+π)` is not
   exactly zero in f64 → tiny noise floor → normalize divides
   by tiny max → noise amplified to ±1.0 → audible non-zero
   PCM. Render is graceful (deterministic, finite, no NaN); PM
   may revisit whether normalize should treat near-zero max as
   zero at M3+ (SPEC §16.9 amendment territory).
4. **max_amplitude_no_normalize** — 4 partials, no normalize,
   atom.amplitude=1.0. Raw sum exceeds 1.0; PCM clamps to
   ±32767. Tests the round-half-away-from-zero scaler's
   defensive clamp.
5. **normalize_false_multi_partial_clamp_safety** — most
   aggressive overflow path (8 partials × amp 1.0,
   normalize=false). f64 accumulator has plenty of headroom;
   verifies anyway.
6. **harmonic_16_cycle_64** — near-Nyquist content (harmonic
   16 over a 64-sample cycle = quarter sample rate). Critical
   for M3.4 predictor optimization + M3.6 pre-emphasis later.
   Renders cleanly; metric finite. `loop_window_rms_delta = 0`
   (decoded PCM has strong period-4 structure aligning the
   first-8 and last-8 windows).
7. **all_8_partials_max_amp_harmonics_1_to_8** — full
   partial-bank stress; bright high-harmonic content.
8. **phase_cycles_0_9999** — phase wraparound boundary.
   `loop_click_abs = 1196` (off by 1 from canonical sine_128's
   1197 — phase shift produces near-identical waveform).
9. **cycle_256_canonical_sine** — cycle-length parity with the
   existing 64/128 baselines. `loop_click_abs = 606` (smaller
   than 128's 1197: larger cycle → smaller last-sample
   magnitude).

Per-fixture coverage in `core/tests/atom_edge_cases.rs`:
- **PCM SHA identity-pin** against `baselines/m3.json::identity_gated`
  via `include_str!` (M2.8.1 / M3.1 pattern). 9 new tests.
- **Determinism** — single parameterized test renders all 9
  fixtures twice and asserts byte-equality on `pcm_sha256`,
  `brr_sha256`, `decoded_brr_pcm_sha256`, `loop_click_abs`,
  `loop_window_rms_delta` (compared by `f64::to_bits` for
  bit-exact equality, not float-equal).
- **Special-case assertions** — amplitude_zero produces
  all-zero PCM + `loop_click_abs = 0`; all_partials_zero
  renders cleanly with no NaN; two_partials_cancel renders
  bounded/finite (NOT all-zero — see fixture #3 above);
  harmonic_16_cycle_64 finite metric, no panic.

543 tests workspace-wide (was 529 at M3.1; +14 from the new
`core/tests/atom_edge_cases.rs`: 9 identity-pin + 1 determinism
+ 4 special-case). Plus 1 new `#[ignore]` print sentinel
(`m3_2_print_atom_edge_case_baselines`) — 3 ignored
workspace-wide.

`baselines/m3.json` expanded: +9 `identity_gated` (each
fixture's PCM SHA) + 36 `documentary_snapshot` (each fixture's
BRR SHA + decoded-BRR-PCM SHA + `loop_click_abs` +
`loop_window_rms_delta`). All existing M3.1 entries preserved.
PCM SHAs are identity-gated per the SPEC §16.9 amendment; BRR
/ decoded-BRR / metric values will shift at M3.3 phase
rotation.

No render path changes — atom PCM stays locked per §16.9.
amplitude_zero and all_partials_zero render cleanly via the
existing normalize `max > 0.0` guard; no defensive-coding fix
required for those.

**M3.3 next.** Phase rotation implementation per SPEC §10.7.
PM to brief at M3.3 entry.

## Last pass

**Pass M3.2 — Atom edge case fixture coverage.**

Four phases. Synthesized fixtures + baseline expansion. No
encoder changes; no render path changes.

- **Phase A:** synthesized nine atom edge-case fixtures in a
  new `core/tests/atom_edge_cases.rs` integration test file.
  Each fixture is built programmatically from `base(cycle)` +
  per-fixture mutations; all fed through `render_to_brr` for
  end-to-end metric capture. Names mirror the consultant's
  `M3_ATOM_<NAME>_*` convention.
- **Phase B:** captured pre-M3 baseline values via a new
  `#[ignore]`'d `m3_2_print_atom_edge_case_baselines` sentinel
  and populated `baselines/m3.json`:
  - 9 new `identity_gated` entries (PCM SHA per fixture; the
    SPEC §16.9 amendment classifies all atom PCM SHAs as
    identity-gated across milestones).
  - 36 new `documentary_snapshot` entries (4 per fixture: BRR
    SHA, decoded-BRR-PCM SHA, `loop_click_abs`,
    `loop_window_rms_delta`) — expected to shift at M3.3
    phase rotation.
- **Phase C:** per-fixture determinism verified by a single
  parameterized test that renders every fixture twice and
  asserts bit-equality on PCM, BRR, both SHAs, and both
  metrics (`f64::to_bits` for the windowed RMS delta — exact
  bit equality, not float-equal). Every fixture is
  deterministic; no f64 reduction-order or HashMap-iteration
  drift surfaced.
- **Phase D (this entry).**
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **543 tests
  workspace-wide** (was 529 at M3.1; +14 from
  `core/tests/atom_edge_cases.rs`).

### Decisions log additions (M3.2)

- Nine new atom edge-case fixtures synthesized: `amplitude_zero`
  (load-bearing for M3.3 tie-breaker), `all_partials_zero_normalize_true`
  (normalize special-case), `two_partials_cancel_partially`
  (FP-noise-amplification surface), `max_amplitude_no_normalize`
  (clamping), `normalize_false_multi_partial_clamp_safety`
  (overflow), `harmonic_16_cycle_64` (near-Nyquist),
  `all_8_partials_max_amp_harmonics_1_to_8` (full bank),
  `phase_cycles_0_9999` (boundary), `cycle_256_canonical_sine`
  (cycle length parity).
- Per-fixture metric values + PCM/BRR/decoded-BRR SHAs captured
  in `baselines/m3.json`. PCM SHAs identity_gated per SPEC §16.9
  amendment; BRR / decoded-BRR / metric values
  documentary_snapshot (will shift at M3.3 phase rotation).
- Determinism verified per fixture (two-run bit-identity on
  every output field).
- Render path handles `amplitude_zero` / `all_partials_zero` /
  the partial-cancellation case cleanly — no defensive-coding
  fix required. The existing normalize `if max > 0.0` guard
  catches both special cases.
- **Spec ambiguity flagged for M3+ consideration (not changed
  at M3.2):** the `two_partials_cancel_partially` fixture
  exposes that `f64::sin(θ + π) ≠ -f64::sin(θ)` exactly, so
  mathematically-cancelling partials produce a ULP-scale noise
  floor that normalize then amplifies to audible levels. The
  brief predicted all-zero PCM here; reality is non-zero
  noise-amplified PCM. Brief did not flag this as a stop
  condition (render is deterministic, finite, no NaN). PM may
  revisit whether normalize should treat near-zero max as
  zero — that is a SPEC §16.9 render-formula amendment and is
  out of M3.2 scope.
- **No committed fixture files** — synthesized in tests only;
  on-disk fixture under `fixtures/projects/atom_edge_cases/`
  deferred to M3.3 prelude per consultant M3 plan #12.
- **No reclassification of existing M3.1 baselines** — M3.1's
  M2 atom PCM SHAs stay identity_gated; M3.1's pre-M3
  loop-click / decoded-BRR scalars stay documentary_snapshot.

## Previous passes

**Pass M3.1 — Loop-click metric implementation + atom PCM
reclassification.**

Five phases, all metric-wiring / baseline reclassification.

- **Phase A:** wired `loop_click_abs` (i32, gated) +
  `loop_window_rms_delta` (f64, diagnostic) +
  `decoded_brr_pcm_sha256` (String) into `AtomBrrOutput` and
  `AtomRenderReport`. `render_to_brr` decodes the freshly
  encoded BRR bytes back to PCM via
  `crate::brr::decode_blocks` and computes the two SPEC §10.6
  metrics on the result. Atoms loop sample 0 .. cycle_len, so
  both metrics use the full decoded buffer with
  `loop_start = 0`. `AtomRenderReport`'s three new fields use
  `#[serde(default)]` for back-compat with pre-M3.1 serialized
  reports.
- **Phase B:** ran the canonical sine_128 / sine_64 fixtures
  through `render_to_brr` and captured M3.1 baseline values:
  - `loop_click_abs`: 128 = 1197, 64 = 2407 (i32).
  - `loop_window_rms_delta`: 128 ≈ 26745.84, 64 ≈ 51285.13 (f64).
  - `decoded_brr_pcm_sha256`: 128 = `de7c89ad...11880bb1`,
    64 = `9c4a231d...cc9b0ec4`.
- **Phase C:** moved `M2_ATOM_128_SINE_PCM_SHA256` and
  `M2_ATOM_64_SINE_PCM_SHA256` from
  `baselines/m2.json::documentary_snapshot` to
  `baselines/m3.json::identity_gated`. `baselines/m2.json` gains
  a `_migrated_to_m3` field documenting the move and the
  `_doc` field is updated. Two new integration tests in
  `core/tests/atom_render.rs`
  (`atom_pcm_sha_matches_locked_baseline_m3_canonical_128_sine`
  and `_64_sine`) read the locked SHA from `baselines/m3.json`
  via `include_str!` + `serde_json` and assert
  `render_to_brr` produces the same value — mirrors the M2.8.1
  `m1_driver_code_sha_matches_locked_baseline` pattern.
- **Phase D:** locked the Phase B values as
  `documentary_snapshot` entries in `baselines/m3.json`. Six
  entries: `M3_ATOM_{128,64}_SINE_LOOP_CLICK_ABS_PRE_M3`,
  `M3_ATOM_{128,64}_SINE_LOOP_WINDOW_RMS_DELTA_PRE_M3`,
  `M3_ATOM_{128,64}_SINE_DECODED_BRR_PCM_SHA256_PRE_M3`. M3.3
  phase rotation will compare against these — the
  `loop_click_abs` entries are the "must improve" target; the
  RMS deltas are diagnostic; the decoded-BRR-PCM SHAs are the
  surface phase rotation will shift intentionally.
- **Phase E (this entry).**
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **529 tests
  workspace-wide** (was 527 at M3.0; +2 from the two new
  atom PCM SHA identity tests).

### Decisions log additions (M3.1)

- M3.1 metric wiring: `loop_click_abs` (i32, gated per SPEC
  §10.6) + `loop_window_rms_delta` (f64, diagnostic) +
  `decoded_brr_pcm_sha256` (String) added to `AtomBrrOutput`
  and `AtomRenderReport`. Three new fields on
  `AtomRenderReport` use `#[serde(default)]` for back-compat.
- Pre-M3 baseline scores recorded for canonical sine_128 and
  sine_64 atoms in `baselines/m3.json::documentary_snapshot`.
  These are the "before phase rotation" measurements; M3.3
  phase rotation MUST produce `loop_click_abs <=` the pre-M3
  values for lexicographic improvement per SPEC §10.7.
- Atom PCM SHAs reclassified from `documentary_snapshot`
  (`baselines/m2.json`) to `identity_gated`
  (`baselines/m3.json`) per the SPEC §16.9 M3.0 amendment.
  Two `include_str!`-based identity tests added at
  integration-test scope (`core/tests/atom_render.rs`).
  `baselines/m2.json` gains a `_migrated_to_m3` field; the
  M2.2 in-module `m2_atom_render_baselines_locked` test
  (literal-string SHA assertion) is retained as defense in
  depth.
- M3.1 reconciliation report:
  - `loop_click_abs` values (1197, 2407) **match** the M2.2
    `loop_click_score` values exactly. Same formula; M3.1
    promotes it from encoder-internal to a SPEC-defined
    metric on the decoded BRR PCM. The M2.2 baselines remain
    in `baselines/m2.json::documentary_snapshot` (now
    redundant — same number, different name); they don't get
    retired this pass since the M2.2 entry covers the encoder
    internal field and the M3.1 entry covers the SPEC §10.6
    metric, and the brief forbids relaxing any M2 baseline.
  - Atom PCM SHAs **match** the M2.2-recorded values exactly.
    No render formula drift since M2.0. The §16.9 amendment
    is now enforced by include_str! tests.
- No encoder changes (phase rotation, predictor, pre-emphasis
  all defer to later sub-passes per SPEC §10.7-§10.9).

## Previous passes

**Pass M3.0 — M3 contracts freeze.**

Nine phases. Contracts only — no encoder, phase rotation,
predictor optimization, or pre-emphasis implementation.

- **Phase A (consultant M3 plan #4, #5, #6):** SPEC §10.6 —
  loop-click metric. Defines `loop_click_abs` (gated metric,
  integer `i32`) and `loop_window_rms_delta` (diagnostic,
  reports-only at M3.0). M3 sub-passes gate on
  `loop_click_abs` only. The squared-difference accumulation
  in the windowed metric is widened to `i64` to avoid overflow
  on i16-range inputs (max `(2 × 32767)^2 × 8 ≈ 3.4 × 10^10`
  for window=8) — the only adjustment to the consultant's
  formula; final `sqrt` produces an `f64` for report display.
  Pre-existing M2 atom loop-click scores
  (`M2_ATOM_128_SINE_LOOP_CLICK_SCORE = 1197`,
  `M2_ATOM_64_SINE_LOOP_CLICK_SCORE = 2407` in
  `baselines/m2.json`) carry forward as pre-M3 reference points.
- **Phase B (consultant M3 plan #7, #17):** SPEC §16.9 — atom
  PCM stability amendment. The atom render formula (f64
  additive sum, normalize-then-scale, round-half-away-from-zero,
  fixed cycle lengths {64, 128, 256}) is locked at M2.0 and
  MUST NOT change at M3+. Atom PCM SHAs are identity-gated
  across milestones; BRR SHAs derived from them MAY shift
  intentionally at M3 (phase rotation §10.7, predictor §10.8,
  pre-emphasis §10.9). M3.1 reclassifies the current M2 atom
  PCM SHAs from `documentary_snapshot` to `identity_gated` in
  `baselines/m3.json`.
- **Phase C (consultant M3 plan #8, #9, #10):** SPEC §10.7 —
  phase rotation M3 contract. Refines existing §10.3 with a
  concrete candidate set (block-aligned only:
  `[0, 16, 32, ..., cycle_len_samples - 16]`; 4/8/16 candidates
  for cycle 64/128/256) and a lexicographic objective
  `(loop_click_abs, peak_abs_error, rms_error, rotation_offset)`
  — not a weighted score. Final tie-breaker: smaller offset
  wins, defaulting to no-rotation. Atom PCM SHAs unaffected
  (rotation operates on a transient encoder input).
- **Phase D (consultant M3 plan #11, #12):** SPEC §10.8 —
  predictor optimization M3 conditional. Bounded beam search
  (recommended `beam_width = 4`) over per-block filter/shift
  selection. Conditional ship: M3.4 ships only if M3.3
  phase-rotation gains are insufficient AND the beam search
  produces measurable additional improvement AND runtime stays
  bounded (≤ 2× M2.2 encode time). Otherwise defers to M4.
- **Phase E (consultant M3 plan #13, #14):** SPEC §10.9 —
  pre-emphasis M3 stretch. Characterization required first at
  M3.5 (compare raw BRR decode vs snes_spc oracle render).
  Presets only (`off` | `gentle` | `strong`) at M3.6 land,
  conditional on M3.5 yielding a clear target. Per-atom
  `pre_emphasis` field; pre-emphasis runs BEFORE rotation /
  predictor search but does not change the stored atom PCM
  (the PCM stability rule gates the rendered PCM before any
  encoder filter, including pre-emphasis).
- **Phase F (consultant M3 plan #15, #16):** SPEC §21 — M3
  baseline classification under the existing M3 milestone
  entry. Three categories:
  - **Must NOT shift across M3:** all atom PCM SHAs;
    canonical SEQ2 bytecode SHA / voice setup table SHA / tick
    counts; M1 driver code SHA; M1 loader size + SHA.
  - **Expected to shift at M3:** atom BRR SHAs; loop-click
    score snapshots; decoded-BRR preview WAVs.
  - **Must remain behaviorally passing across M3:** M2
    audibility floors / silence ceiling / source-step ZCR
    ratio / 32 KiB module cap.

  M3 identity-gated baseline rule (carried from M2.8.1): every
  new `identity_gated` entry added to `baselines/m3.json` MUST
  ship with an `include_str!` + serde-parse test asserting the
  generated value matches.
- **Phase G:** `baselines/m3.json` scaffolded; `inherits_m2:
  true`; identity_gated empty (M3.1+ populates); three
  behavior_gated entries — `M3_LOOP_CLICK_METRIC_GATING`,
  `M3_PHASE_ROTATION_OBJECTIVE`,
  `M3_PHASE_ROTATION_CANDIDATE_SET`.
- **Phase H (consultant M3 plan #6):** `loop_click_abs` and
  `loop_window_rms_delta` implemented as pure functions in
  `core::audition` (no encoder dependency). Five fixture
  tests in `core/tests/loop_click_metric.rs` pin the metric
  formula on hand-constructed PCM vectors: simple seam (=100),
  perfect seam (=0), full-range negative-to-positive seam
  (=2000), windowed metric on all-zero seam (≈0), windowed
  metric on a linear-ramp wraparound (sqrt(5_120_000) ≈
  2262.74). Per consultant: "must be testable without
  rendering atoms or encoding BRR. That prevents circular
  validation."
- **Phase I (this entry).**
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **527 tests
  workspace-wide** (was 522 at M2.8.2; +5 from the
  loop-click-metric formula fixtures).

### Decisions log additions (M3.0)

- M3 entry approved per consultant M3 plan #37.
- M3.0 contracts frozen per consultant M3 plan #4–#16:
  loop-click metric (§10.6), atom PCM stability amendment
  (§16.9), phase rotation (§10.7), predictor optimization
  (§10.8), pre-emphasis (§10.9), baseline shift rules (§21 M3
  milestone entry).
- M3 contracts land in SPEC §10 (BRR encoder policy) as new
  subsections §10.6–§10.9, not §16.x — the consultant brief
  used §16.x as placeholders. §16.9 already exists as
  "Project file format v2 (M2)" and houses the atom render
  formula; the M3 encoder contracts are about the encoder, not
  the schema, so §10 is the structurally correct home. The
  §16.9 atom PCM stability rule does live inside §16.9 since
  it's about the render formula stored there.
- `baselines/m3.json` scaffolded; inherits M2 baselines by
  reference (`inherits_m2: true`).
- M3 identity-gated baseline rule carried from M2.8.1: every
  new `identity_gated` baseline added to `baselines/m3.json`
  must ship with an `include_str!` + serde-parse test
  asserting the value matches.
- Loop-click metric formula fixture-pinned at M3.0
  (independent of encoder); M3.1 implements applying it to
  atoms and records the pre-M3 atom loop-click baseline.
- M3 sub-pass plan locked: M3.1 metric implementation, M3.2
  atom edge cases, M3.3 phase rotation, M3.4 predictor
  optimization (conditional), M3.5 Gaussian characterization,
  M3.6 pre-emphasis presets (conditional), M3.7 GUI polish,
  M3.8 acceptance + release.
- Release tag policy: v0.3-rc1 only after M3.8 close +
  integrity audit per M2 lessons (M2.8.1 / M2.8.2 audit
  cycles).
- Spec ambiguity flagged for consultant/PM review (not
  changed at M3.0): the phase rotation candidate set scales
  with cycle length — cycle 256 yields 16 candidates vs cycle
  64 yielding 4. M3.3 may want to bound the candidate count
  rather than the offset stride. Deferred to M3.3 entry brief.
- Spec adjustment vs consultant brief: the windowed loop-click
  metric's squared-difference accumulation is `i64`, not `i32`
  — the consultant comment said i32 but with i16 inputs the
  per-sample diff^2 alone overflows i32. Final formula and
  determinism unchanged.

## Previous passes

**Pass M2.8.2 — identity-pin pattern standardization.**

Consultant M2.8.1 follow-up audit (audit-the-auditor):

- **Phase 1 (consultant M2.8.1 audit #3, #16):** the SEQ2
  bytecode SHA was identity-gated in `baselines/m2.json` but the
  test pointed at (`end_to_end_compile_sequence_canonical_byte_pinned`
  in `core/tests/sequence_compile.rs`) only asserted byte length
  (49) / payload length (41) / SEQ2 magic / END terminator —
  not the literal SHA value. Same failure mode as the
  M2.8.1 `M1_DRIVER_CODE_SHA256` discovery. Upgraded with an
  `include_str!`-based assertion mirroring the M2.8.1
  `m1_driver_code_sha_matches_locked_baseline` pattern. Asserted
  SHA passes on first run — the baseline value
  `f9fa6ea8...0fd24f0` is current; no stale-baseline condition
  triggered.
- **Phase 2 (consultant M2.8.1 audit #4):** standardized
  `voice_setup_table_byte_pinned_abi` — the byte-vector literal
  assertion is kept (it documents the ABI directly per
  consultant guidance), and a parallel SHA assertion against
  `baselines/m2.json` was added for drift-catching at the SHA
  layer.
- **Phase 3 (consultant M2.8.1 audit #5-#7):** standardized
  `total_ticks_matches_lowering` and
  `total_elapsed_ticks_includes_resume_tick_per_wait` in
  `core/src/sequence_compiler.rs::tests`. Hardcoded literals
  (`120 + 4 + 1 + 4 + 120`, `254`) were replaced with values
  read from `baselines/m2.json` via a small
  `baseline_scalar(name)` test helper in the same module. The
  literal sum-of-WAITs equation is preserved as a documentation
  cross-check inside the test.
- **Phase 4 — audit:** verified all 7 identity_gated baselines
  have a literal/include_str pin. Coverage:
  - `M1_LOADER_SIZE_BYTES` — `app/tests/cli.rs` literal `588`
  - `M1_LOADER_SHA256` — `app/tests/cli.rs` literal `const`
  - `M1_DRIVER_CODE_SHA256` — `core/tests/driver_build.rs`
    `include_str!` (M2.8.1)
  - `M2_CANONICAL_SEQUENCE_BYTECODE_SHA256` —
    `core/tests/sequence_compile.rs` `include_str!` (Phase 1)
  - `M2_CANONICAL_VOICE_SETUP_TABLE_SHA256` —
    `core/tests/sequence_compile.rs` byte-vector literal +
    `include_str!` (Phase 2)
  - `M2_CANONICAL_SEQUENCE_TOTAL_TICKS` —
    `core/src/sequence_compiler.rs` `include_str!` + literal
    sum-of-WAITs (Phase 3)
  - `M2_CANONICAL_SEQUENCE_ELAPSED_TICKS` —
    `core/src/sequence_compiler.rs` `include_str!` (Phase 3)
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **522 tests
  workspace-wide** (unchanged from M2.8.1; Phases 1-3 extended
  existing tests rather than spinning new sibling `_sha_pinned`
  test functions).

### Decisions log additions (M2.8.2)

- M2.8.1 follow-up audit (consultant audit-the-auditor pass):
  SEQ2 bytecode SHA was not literally pinned by
  `end_to_end_compile_sequence_canonical_byte_pinned` (length /
  shape only); upgraded with `include_str!`-based assertion
  mirroring the M2.8.1 M1 driver pattern. Voice setup table +
  `total_ticks` + `total_elapsed_ticks` standardized to the
  same baseline-parse pattern for uniformity.
- All 7 identity-gated baselines in `baselines/m2.json` now
  have at least one test that asserts the literal value via
  either `include_str!` parse OR literal byte-vector / scalar
  match.
- Pattern for M3 (per consultant's pending recommendation):
  every `identity_gated` baseline added to `baselines/m2.json`
  must ship with a test that includes `baselines/m2.json` via
  `include_str!` and asserts the generated value matches.
- v0.2-rc2 git tag points at the M2.8.2 close commit
  (annotated tag with
  `-m "v0.2-rc2: M2 release candidate (M2.8.2 identity-pin standardization)"`).
- v0.2-rc1 retained in tag history pointing at the M2.8.1
  close commit — kept as a documentary marker for "tagged
  before integrity audit" rather than retracted.

**Pass M2.8.1 — release-final patches before v0.2-rc1.**

Nine consultant M2-close-out items, all docs / baselines / one
test:

- **Phase 1A (consultant #1, #18):** narrowed `baselines/m2.json`
  `_doc` scope claim — current locked / snapshot values only;
  pre-M2 retired baselines stay in `docs/history/M0-M2-passes.md`.
- **Phase 1B (consultant #2):** added
  `M2_CANONICAL_SEQUENCE_ELAPSED_TICKS = 254` under
  `identity_gated` with `test:` pointer at the existing
  `total_elapsed_ticks_includes_resume_tick_per_wait`.
  Companion `test:` pointer added on
  `M2_CANONICAL_SEQUENCE_TOTAL_TICKS` (was missing).
- **Phase 2A (consultant #3):** committed canonical fixture
  under [`fixtures/projects/canonical_m2/`](fixtures/projects/canonical_m2/) —
  deterministic 32 kHz mono PCM16 WAV (8192 frames,
  `8000 * sin(2π n / 64)`, SHA `b42397b8...`) plus the v2
  multi_voice_atom project file referencing it. Same shape as
  the `core/tests/sequence_compile.rs::canonical_project()`
  helper. Verified end-to-end:
  `compile-sequence` reproduces the locked
  `M2_CANONICAL_SEQUENCE_BYTECODE_SHA256`; `m2-acceptance`
  reports `bundle.status: ok`.
- **Phase 2B (consultant #22):** asar version claim narrowed
  to "tested with 1.91, 1.81+ expected".
- **Phase 3A (consultant #29):** added
  `M2_CANONICAL_SEQUENCE_ELAPSED_TICKS = 254` to the
  identity-gated bullet of `RELEASE_NOTES_v0.2-rc.md`.
- **Phase 3B (consultant #5, #26):** separated atom PCM vs BRR
  shift expectations in release notes — atom PCM SHAs are
  M3-stable IF the render formula is unchanged; only atom BRR
  SHAs are expected to invalidate at M3.
- **Phase 3C (consultant #4):** annotated tag command in
  release notes (`git tag -a -m`); commit reference shifted
  from M2.8 to M2.8.1 close.
- **Phase 4 (consultant #16):** the historic
  `M1_DRIVER_CODE_SHA256` value `671ee21eb...` was
  identity-gated only *aspirationally* — no test pinned the
  literal. The actual driver SHA had drifted (probably at the
  M2.0 bootstrap-token fix `seed dp_last_token from $F4`,
  consultant M2.0 #1) to
  `22c5335e2dd889af14aec03e1792484ac71e13fb327d66431c712cdbcd826250`.
  M2.8.1 ships:
  - New test
    `core/tests/driver_build.rs::m1_driver_code_sha_matches_locked_baseline`
    — reads the locked SHA from `baselines/m2.json` via
    `include_str!` + `serde_json`, asserts the assembled driver
    SHA matches.
  - Updated baseline value in `baselines/m2.json` to the
    M2.8.1-current SHA with `_note` capturing the M2.0
    history. `locked_at` shifts from "M2.0 (rebase)" to
    "M2.8.1" — M2.8.1 is the actual enforcement boundary.
- **Phase 5 (this entry).**
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **522 tests
  workspace-wide** (was 521 at M2.8; +1 from the new literal
  pin test).

### Decisions log additions (M2.8.1)

- Release artifacts patched per consultant M2 close-out review
  (9 items): `baselines/m2.json` scope narrowed +
  `M2_CANONICAL_SEQUENCE_ELAPSED_TICKS` added; reproducer doc
  ships a concrete fixture (`fixtures/projects/canonical_m2/`);
  release notes elapsed-ticks promoted to identity-gated bullet
  + atom PCM vs BRR shift expectations separated; tag command
  annotated; asar-version claim narrowed to "tested with 1.91,
  1.81+ expected"; `M1_DRIVER_CODE_SHA256` test pinned literally.
- `M1_DRIVER_CODE_SHA256` re-locked at M2.8.1 (was historic
  M2.0-rebase value; drifted unobserved at the M2.0
  bootstrap-token fix). New
  `m1_driver_code_sha_matches_locked_baseline` test pulls the
  literal from `baselines/m2.json` so the source-of-truth is
  single.
- M3 deferrals captured (consultant #13, #25, #26, #27):
  `rename_sequence_id_cascade` GUI polish; M3 prelude must
  preserve atom PCM SHA unless the render formula changes;
  loop-click oracle metric must be implemented before BRR
  encoder optimization.
- v0.2-rc1 git tag points at the M2.8.1 close commit (annotated
  tag with `-m "v0.2-rc1: M2 release candidate"`).

**Pass M2.8 — M2 release prep.** [Full M2.8 entry preserved
below for the v0.2-rc1 record; ages out to the archive on the
next pass.]

Four release-prep layers covering the consultant's M2.7 review:

- **Layer 1 — WAIT timing alignment.** Consultant #1: SPEC §14.3
  pinned wait-decrement-before-opcode-read at M2.4 prelude; the
  M2.5 driver matched; the M2.4 sequence-compiler walker still
  used the older "WAIT n advances n ticks" semantic. Walker
  fixed: tick_cursor now tracks elapsed-tick under SPEC semantics
  (each WAIT advances n+1 ticks — n decrements + 1 resume tick).
  `SequenceCompileOutput` / `SequenceCompileReport` gain a new
  `total_elapsed_ticks` field; `total_ticks` keeps its M2.4
  semantic (`sum-of-WAIT-operands = 249` for canonical) for back-
  compat with the locked baseline. Canonical fixture's
  `total_elapsed_ticks = 254`. SPEC §21 source-step pre/post
  windows shift to `pre = ticks 80..=120`, `post = ticks 133..=254`
  to match the new walker / driver alignment.
- **Layer 2 — Five hardening surfaces.** Driver-version detection
  anchored on the full 12-byte ready-signature pattern (consultant
  #8) — was scanning for any `8F xx F6` triple, false-positive
  risk grows with driver size. v2 SFC compile path now enforces
  source-SHA refresh / mismatch (consultant #10) — was a hole in
  the v1 enforcement parity. GUI step reorder / remove auto-
  normalizes step transitions (consultant #11) — preserves SPEC
  §16.9 rules 47-48 across structural edits. New
  `rename_atom_id_cascade` method on `V2EditorModel` updates step
  references (consultant #12) — pre-M2.8 the GUI's `set_atom_id`
  setter left dangling references. Round-trip parity test
  extended with a non-trivial mutation cycle (consultant #16) —
  load-bears the "GUI editing produces byte-stable output" claim
  through real edit sessions, not just immediate-construction
  saves.
- **Layer 3 — Docs / baselines reorganization.** STATUS.md split
  into active (this file) + archive
  ([`docs/history/M0-M2-passes.md`](docs/history/M0-M2-passes.md))
  per consultant #20. Canonical SEQ2 bytecode + voice setup
  table fixtures extracted to
  [`baselines/m2_canonical_fixtures.md`](baselines/m2_canonical_fixtures.md)
  per consultant #21. Machine-readable baselines at
  [`baselines/m2.json`](baselines/m2.json) classify every M0–M2
  baseline as identity-gated / behavior-gated / documentary
  snapshot / retired (consultants #23, #25, #26, #27). Four
  prose hygiene patches: SPEC §5.4 GUI capability wording softened
  (consultant #19), STATUS slider-snap narration accuracy fix
  (consultant #15), profile-switch UX nudge added when
  `multi_voice_atom` lands without an atom_sequence track
  (consultant #13), `sample_pool 0..=128` relaxation noted in
  decisions log + release notes (consultant #31).
- **Layer 4 — Release proper.** Reproducer guide
  ([`docs/reproduce-m2.md`](docs/reproduce-m2.md)) walks
  fresh-clone-to-passing-`m2-acceptance` steps. Release notes at
  [`RELEASE_NOTES_v0.2-rc.md`](RELEASE_NOTES_v0.2-rc.md) cover
  highlights, locked-vs-snapshot baselines classification, schema
  notes, M3 deferrals, reproduction pointer, and the tag command.

521 tests workspace-wide at M2.8 (was 505 at M2.7; +16 net
delta from the new pin coverage across Layer 1 and Layer 2).
M2.8.1 added 1 more (`m1_driver_code_sha_matches_locked_baseline`)
for a total of 522 at the v0.2-rc1 tag.

Pre-M2.8 pass log archived at
[`docs/history/M0-M2-passes.md`](docs/history/M0-M2-passes.md) —
M0 through M2.7. STATUS.md keeps the current milestone, last
pass summary, decisions log additions for the current pass, and
current baselines. Historic entries land in the archive as they
age out.

## Current baselines

Machine-readable baselines + classification (identity-gated /
behavior-gated / documentary snapshot / retired) live in
[`baselines/m2.json`](baselines/m2.json). The canonical M2 SEQ2
bytecode + voice setup table hex dumps + per-byte breakdowns
live in [`baselines/m2_canonical_fixtures.md`](baselines/m2_canonical_fixtures.md).

Top-line locked values (mirror of `baselines/m2.json`):

```
M1_LOADER_SIZE_BYTES                  = 588
M1_LOADER_SHA256                      = 955f525c...873f40
M1_DRIVER_CODE_SHA256                 = 22c5335e...cd826250  (locked at M2.8.1)
M2_CANONICAL_SEQUENCE_TOTAL_TICKS     = 249   (sum-of-WAIT-operands)
M2_CANONICAL_SEQUENCE_ELAPSED_TICKS   = 254   (wall-elapsed under SPEC §14.3)
M2_CANONICAL_SEQUENCE_BYTECODE_SHA256 = f9fa6ea8...0fd24f0
M2_CANONICAL_VOICE_SETUP_TABLE_SHA256 = f2faaed8...089ad5
M2_AUDIBILITY_FLOORS                  = max_abs >= 1000, rms >= 200
M2_SILENCE_CEILING                    = max_abs <= 50  (hard-panned silent side)
M2_SOURCE_STEP_ZCR_RATIO_FLOOR        = 1.5  (post.right.zcr / pre.right.zcr)
M2_MODULE_CAP_BYTES                   = 32768  (SPEC §15.6, > triggers ModuleTooLarge)
```

Behavior-gated thresholds carry forward unchanged across M2 releases.
Identity-gated SHAs shift only on intentional driver / loader / fixture
edits and require coordinated baseline updates. Documentary snapshots
(M2 driver size, atom BRR SHAs) are informational and expected to
shift at M3 BRR-encoder-quality work.
