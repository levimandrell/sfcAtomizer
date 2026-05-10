# SFC Wave Compiler — Status

## Current milestone

**M3.2 — Atom edge case fixture coverage.** Synthesized
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
