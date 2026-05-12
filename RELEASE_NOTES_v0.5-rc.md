# v0.5-rc — M5 release candidate

The M5 release candidate closes the M5 milestone: a methodology
pass targeting the gaussian characterization methodology gap
identified at M4.2 plus a BRR noise-floor strategy spike. **M5
shipped methodological clarity rather than user-facing feature
changes; production encoder behavior is byte-identical to v0.4-rc1.**

Per consultant M5 plan #33, M5 ships as `v0.5-rc1` even though
it is methodology-only — a trustworthy negative methodology
result (with the actual root cause refined and a forward path
documented) is project progress.

The release is recorded as `release: v0.5-pre` in
`baselines/m5.json` (will promote to `v0.5-rc` at tag time)
and tracks the M5.6 close commit on the `main` branch.

`baselines/m5.json` inherits `baselines/m4.json` by reference
(`inherits_m4: true`); M4 in turn inherits M3, M2. M5
acceptance runs M4 acceptance as its stage 1 regression gate.

## Highlights

- **M5.0 contracts freeze.** SPEC §10.11 (native-rate
  characterization contract — Option α, harness-scoped, no v2
  schema change), §10.9 reliable-alignment thresholds
  reaffirmed (M4.0 4-criterion predicate carries forward
  unchanged), §10.9 pre-emphasis preset report fields locked
  (conditional on M5.3 outcome), §24.1.1 methodology repair
  budget tightening (1+1 loops, stricter than M4's 2 loops),
  §16.9.1 atom PCM amendment procedure documented for forward
  visibility (NOT activated). `baselines/m5.json` scaffold
  with 6 behavior-gated contracts.
- **M5.1 native-rate harness verification.** Engineer's
  preflight surfaced a diagnosis-discrepancy in SPEC §10.11
  motivation: pitch register was already `0x1000` (unity) for
  every M3.5 canonical signal since M2.7 via
  `core::voice_setup`'s hardcoded `source_sample_rate_hz =
  32000` for `TrackKind::AtomSequence`. The earlier
  "fractional pitch stepping" attribution of the M4.2 shape
  divergence was therefore incorrect. SPEC §10.11 motivation
  rewritten before implementation (Phase 0 commit, prose-only).
  Harness contract delivered: `harness_meta` field on
  `CharacterizationReport` (schema `v4 → v5`), per-signal
  `atom_native_rates_hz` map, runtime regression guard
  `pitch_register_equals_4096_for_native_rate_signals`.
- **M5.1.1 historical-artifact corrections.** Annotation-style
  patches on v0.4-rc1-era artifacts retracting the M4.2
  fractional-stepping diagnosis:
  `baselines/m4.json::M4_2_PHASE_C_ZCR_DOUBLING_ROOT_CAUSE`
  gained a `_correction_m5_1` sibling field;
  `RELEASE_NOTES_v0.4-rc.md` and `docs/reproduce-m2.md` gained
  inline `[M5.1 correction: ...]` annotations adjacent to each
  retracted-claim sentence. **`v0.4-rc1` tag is unchanged at
  `1223606`**; annotations preserve released-artifact
  provenance while surfacing the correction.
- **M5.2 characterization re-run + decision: methodology_unresolved.**
  Re-running under the verified-unity-pitch harness produced
  **byte-identical metrics to M4.2**. The SPEC §10.9
  four-criterion reliable-alignment predicate fails 0/7 on
  anchor signals (criterion 4, `gain_separator_ok ≤ 80%`,
  fails universally). Outcome: `methodology_unresolved` per
  SPEC §10.11. **M5.3 pre-emphasis preset evaluation is
  permanently SKIPPED**; defers to M6+ unless a future
  milestone introduces a fundamentally different
  characterization methodology. 74 documentary baselines
  locked + `M5_2_RESIDUAL_DIVERGENCE_HYPOTHESIS` formalizes
  the inferential closure (`force_filter_0_loop_entry`
  zeroes BRR decoder state at each loop boundary; three-way
  comparison's (1) and (2) are equivalent for these atoms;
  residual divergence narrows to S-DSP gaussian 4-tap kernel
  non-impulse response at unity pitch).
- **M5.4 BRR noise-floor strategy spike: no production change.**
  Wider-beam scientific closure (widths 8/16 on 4 high-noise
  cluster fixtures): max RMS improvement 0.17%, max SNR
  improvement 0.015 dB; consultant M4.4 audit #2
  structural-ceiling claim confirmed empirically. Alt-shift
  objective on `HARMONIC_16_CYCLE_64`: byte-identical to
  production; consultant M4.4 audit #7 prediction confirmed.
  M6+ §16.9 amendment scope sketch documented in
  `baselines/m5.json::M5_4_SOURCE_DOMAIN_ATTENUATION_M6_SKETCH`
  per the §16.9.1 procedure (forward visibility; NOT activated).
- **M5.5 GUI/schema polish audit: clean.** Four-phase audit
  confirmed: zero new v2 schema cross-tree track-id
  references since M4.6; zero hits for characterization data
  field names in GUI surfaces (`app_main.rs`, `v2_editor.rs`);
  zero hits for `loop_window_rms_delta` in GUI; no incidental
  GUI debt. Single STATUS-only commit closed the pass.
- **`m5-acceptance` bundle.** Five-stage rollup analog of
  `m4-acceptance`: M4 regression (inherits its warn) +
  native-rate harness verification (M5.1) + characterization
  documentary integrity (M5.2 baselines snapshot inertia
  check) + M5.4 spike state (production byte-identity guard
  + feature-flag preservation) + M5 baselines integrity.

## What's locked

`baselines/m5.json` carries the M5 release baselines and
inherits `baselines/m4.json` by reference. Summary:

- **Identity-gated** (any drift = regression): **empty by
  design** per consultant M5 plan #10. M5's sub-passes
  produced methodology outcomes (native-rate harness
  verification, characterization re-run + decision, BRR
  noise-floor strategy spike), not new feature surfaces. M3's
  11 atom PCM SHAs remain the identity surface; M4 + M5 don't
  add to it.
- **Behavior-gated** (numeric / policy contracts) — 6 M5.0
  entries:
  - `M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER` = 4096
    (the M5.1 harness constant; locked at M5.0; runtime guard
    `pitch_register_equals_4096_for_native_rate_signals`
    added at M5.1 Phase D).
  - `M5_METHODOLOGY_REPAIR_BUDGET` = 1 main loop + 1
    correction (stricter than M4's 2-loop budget; M5.2.1 slot
    unburned).
  - `M5_RELIABLE_ALIGNMENT_THRESHOLD_INHERITED` (M4.0
    thresholds carry forward unchanged through M5).
  - `M5_PRE_EMPHASIS_FILTER_FORM_CONSTRAINT` (FIR ≤ 3 taps
    OR one-pole IIR shelf; no filter-design crate dependency
    unless explicit PM approval at M5.3 brief — moot since
    M5.3 is SKIPPED).
  - `M5_ATOM_PCM_STABILITY_HELD` (default M5 position: hold
    SPEC §16.9 line; no source-domain preprocessing without
    SPEC §16.9.1 amendment).
  - `M5_RUNTIME_BUDGET` = target 5 s / warning 10 s for the
    9-signal characterization (release build).
- **Documentary snapshot** (informational; expected to shift
  on declared milestones) — 77 entries:
  - 74 `M5_2_*`: 8 fields × 9 signals (`ALIGNMENT_BEST_OFFSET`,
    `NORMALIZED_CORRELATION`, `ZCR_RATIO`, `GAIN_DELTA_DB`,
    `GAIN_DELTA_DB_ALIGNED`, `PEAK_ABS_ERROR_ORACLE_VS_RAW`,
    `PEAK_ABS_ERROR_AFTER_GAIN_NORMALIZATION`,
    `ALIGNMENT_VALIDITY_ALL_PASS`) + `M5_2_CHARACTERIZATION_SUMMARY`
    + `M5_2_RESIDUAL_DIVERGENCE_HYPOTHESIS`.
  - 3 `M5_4_*`: `WIDER_BEAM_HIGH_NOISE_CLUSTER` (8 fixture ×
    width combinations), `ALT_SHIFT_OBJECTIVE_HARMONIC_16`
    (control + treatment + delta), `SOURCE_DOMAIN_ATTENUATION_M6_SKETCH`
    (three mechanism candidates + cost + benefit + risk + M5
    disposition).
- **Retired**: none.

## Reproduction

`docs/reproduce-m2.md` is the unified reproducer guide
covering M2 + M3 + M4 + M5 acceptance. Test count at v0.5-rc:
**620 tests workspace-wide, 12 ignored, 0 failed** (was 615 /
7 / 0 at v0.4-rc; +5 passing from M5.1 Phase D regression
guard + M5.4 Phase A/B sanity tests; +5 ignored from M5.4
Phase A/B measurement tests + their helpers).

## M5 measurement outcomes

M5 was a methodology milestone targeting the gaussian
characterization methodology gap identified at M4.2. **The
milestone shipped methodological clarity rather than
user-facing feature changes; production encoder behavior is
byte-identical to v0.4-rc1.**

### M5.1 preflight discovery: M4.2 fractional-stepping diagnosis retracted

M4.2 (and SPEC §10.11 motivation at M5.0) attributed the
raw-vs-oracle characterization shape divergence to DSP pitch
register fractional stepping. M5.1 preflight traced the M2
atom voice setup path (`core::voice_setup` hardcodes
`source_sample_rate_hz = 32000` for `TrackKind::AtomSequence`;
`pitch_register(32000, root, root, 0) = 0x1000`) and verified
pitch register has been `0x1000` (unity) since M2.7. The
"fractional-stepping" diagnosis was therefore incorrect.

Consultant verification confirmed the trace. SPEC §10.11
motivation was rewritten before M5.1 implementation landed
(Phase 0 commit, prose-only). **M5.1.1 added annotation-style
corrections to v0.4-rc1-era historical artifacts.**
`baselines/m4.json::M4_2_PHASE_C_ZCR_DOUBLING_ROOT_CAUSE`
gained a `_correction_m5_1` sibling field;
`RELEASE_NOTES_v0.4-rc.md` and `docs/reproduce-m2.md` gained
inline `[M5.1 correction: pitch register was verified at
0x1000 in M5.1 preflight; fractional stepping is NOT the
cause. See SPEC §10.11 (M5.1 update) for the retracted
diagnosis and current candidate-cause hypotheses.]`
annotations adjacent to each retracted-claim sentence.
**`v0.4-rc1` tag is unchanged at `1223606`** (annotated tag
object `4fc2f3e1`); the annotations preserve released-artifact
provenance.

### M5.2 characterization re-run: methodology_unresolved

M5.2 re-ran characterization under the verified-unity-pitch
harness and produced **byte-identical metrics to M4.2** —
correlations 0.117–0.274 on low-frequency anchors;
`zcr_ratio ≈ 2`; gain_delta_db_aligned of +2.6 dB at low/mid
frequencies dropping to -1.2 dB near Nyquist. The SPEC §10.9
four-criterion reliable-alignment predicate fails 0/7 on
anchor signals (criterion 4, `gain_separator_ok ≤ 80%`,
fails universally).

Outcome: `methodology_unresolved` per SPEC §10.11.
**M5.3 pre-emphasis preset evaluation is SKIPPED**; defers
permanently to M6+ unless a future milestone introduces a
fundamentally different characterization methodology that
can clear the four-criterion predicate. M5.2.1 correction
budget NOT burned — the limitation is structural under the
current comparison-surface design, not a clear-cause
implementation bug.

M5.2 also formalized M5.1's inferential closure
(`M5_2_RESIDUAL_DIVERGENCE_HYPOTHESIS`): every M3.5 canonical
atom sets `force_filter_0_loop_entry: true`, which zeroes
the BRR decoder state at each loop boundary. Three-way
comparison's "raw tiled no-state" and "raw with state-carry
across loops" produce byte-identical PCM for these atoms;
residual (1)/(2)-vs-(3) divergence narrows to **S-DSP
gaussian 4-tap kernel non-impulse response at unity pitch**.

### M5.4 BRR noise-floor strategy spike: no production change

M5.4 documented two empirical confirmations + one M6+ scope
sketch.

- **Wider-beam scientific closure** (widths 8/16 on 4
  high-noise cluster fixtures): max RMS improvement 0.17%,
  max SNR improvement 0.015 dB. None come close to the SPEC
  §24.1 10% exit-criterion threshold. Width 8 = width 16
  byte-identically across all 4 fixtures (the beam doesn't
  find further optima beyond width 4 / 8 within these
  signals' (filter, shift) trial spaces). **Consultant M4.4
  audit #2 structural-ceiling claim** (peak_abs ≈ 18431 =
  `i16::MAX − (7 << 12 >> 1) = 32767 − 14336`) **confirmed
  empirically.**

- **Alternative shift objective** (`RmsThenPeak` vs
  `PeakThenSumSq`): **byte-identical** output on
  `HARMONIC_16_CYCLE_64`. The 14336 per-block ceiling is the
  dominant constraint at the per-block grid; the lex
  tiebreak doesn't unlock a different (filter, shift)
  choice because the shift that minimizes peak also
  minimizes sum_sq under that ceiling. **Consultant M4.4
  audit #7 prediction confirmed.**

- **M6+ §16.9 amendment scope sketch** documented in
  `baselines/m5.json::M5_4_SOURCE_DOMAIN_ATTENUATION_M6_SKETCH`:
  three mechanism candidates (soft cap at ±14336, per-atom
  amplitude scaling, source-side spectral shaping), cost
  table (11 atom PCM SHA identity retirements + repins,
  M2/M3/M4 acceptance reruns, release-note warning,
  consultant close-out audit), ~50% SNR improvement estimate
  for high-noise cluster, three risk dimensions (audibility,
  compatibility, M2 voice-volume compensation viability).
  **M5 disposition: defer to M6+ for explicit PM
  authorization per SPEC §16.9.1 procedure.**

**Production encoder unchanged from M3.3.** The
`m5-acceptance` Stage 4 guard test
(`m5_4_alt_shift_peak_then_sum_sq_matches_production_path`)
locks this invariant; the M3 atom PCM SHA identity tests
(11 fixtures) also confirm `render_to_brr` output is
byte-for-byte unchanged.

## M6 prelude scope

Open questions identified during M5 that may be addressed
at M6+:

1. **Source-domain attenuation milestone (PM-authorized §16.9
   amendment).** Per
   `baselines/m5.json::M5_4_SOURCE_DOMAIN_ATTENUATION_M6_SKETCH`,
   the theoretical path to clear the SPEC §24.1 10%
   encoder-improvement gate requires reducing source PCM
   peaks below the BRR ADPCM current-sample ceiling of ±14336.
   Three mechanism candidates documented; M6 milestone scope
   sketch in place. **Activation requires explicit PM
   authorization per SPEC §16.9.1 amendment procedure.** M6
   milestone would: retire 11 M3 atom PCM identity SHAs, pin
   11 new M6 atom PCM identity SHAs, re-run M2/M3/M4/M5
   acceptance, release-note "BREAKING: render formula
   amended", consultant close-out audit.

2. **Alternative characterization methodology (research
   direction).** The M5.2 `methodology_unresolved` outcome
   stems from a structural mismatch between raw BRR decode
   and SPC playback at the gaussian-kernel level. M6+
   research direction (no committed milestone):
   characterization methodology that compares like-to-like
   at the gaussian interpolation layer. Approaches might
   include direct DSP-state introspection, gaussian-kernel-
   aware host simulation, or methodology that side-steps the
   comparison surface entirely. **No commitment; research
   scope only.**

3. **`baselines/m6.json`** (inherits M5 by reference;
   mirrors the M5-inherits-M4 pattern). To be created at
   M6.0 if M6 proceeds.

This is forward visibility, not a commitment. M6 entry is
conditional on PM authorization for either (1) or (2) above.
See SPEC §26 for the locked text.

## Tagging

This release-candidate is recorded as `v0.5-pre` in
`baselines/m5.json::release` (will promote to `v0.5-rc` at
tag time). Tag in git when ready to publish:

```bash
git tag -a v0.5-rc1 <m5.6-close-commit> -m "v0.5-rc1: M5 release candidate (methodology milestone; no user-facing production change)"
git push origin v0.5-rc1
```

Annotated tag (`-a`) carries the message + tagger metadata
so the release is fully self-describing from
`git show v0.5-rc1`. Tag the M5.6 close commit (final
release-prep patches including the `m5-acceptance` bundle +
reproducer doc update + this notes file + SPEC §26 M6
prelude scope) rather than M5.5 — the M5.6 patches deliver
the release surface that the `v0.5-rc1` label depends on.
