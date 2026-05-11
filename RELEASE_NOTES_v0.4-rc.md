# v0.4-rc — M4 release candidate

The M4 release candidate closes the M4 milestone: a quality
pass targeting two open questions from M3 (gaussian
characterization methodology and BRR encoder noise-floor
investigation). **Both investigations produced documented
negative-or-deferred outcomes rather than feature ships;
this is the right outcome for a research-spike-heavy
milestone.**

The release is recorded as `release: v0.4-rc` in
`baselines/m4.json` and tracks the M4.7 close commit on the
`main` branch.

`baselines/m4.json` inherits `baselines/m3.json` by reference
(`inherits_m3: true`); M3 in turn inherits M2. M4 acceptance
runs M3 acceptance as its stage 1 regression gate.

## Highlights

- **M4.0 — M4 contracts freeze.** SPEC §10.9 amendment locks
  reliable-alignment criteria (4 conditions: `zcr_ratio`,
  `normalized_correlation ≥ 0.90`, `alignment_best_offset <
  alignment_search_limit`, gain-vs-shape separator) and the
  alignment search range
  (`max_offset = max_i(cycle_len_samples_i)`). SPEC §10.10
  locks the four BRR encoder noise-floor metrics
  (`peak_abs_raw_vs_source`, `rms_raw_vs_source`, `snr_db`,
  `clipping_count_raw`) with formulas fixture-pinned. SPEC
  §10.9 locks the pre-emphasis pipeline order
  (`render → pre_emphasis → rotation → encode`). SPEC §24.1
  locks per-spike exit criteria + §24.2 locks baseline shift
  rules. Schema bumped to `v3` (M3.5.1 diagnostics) then `v4`
  (M4.0 alignment fields).
- **M4.1 — alignment plumbing.** `align_oracle_to_raw`'s
  search range expanded from M3.5.1's hard-coded
  `max_offset = 32` to per-signal `cycle_len_samples` (for
  `m3_5_canonical`: 64 / 128 / 256). Four schema v4 alignment
  fields populated on the gaussian characterization report
  (`alignment_search_limit`, `alignment_boundary_hit`,
  `alignment_valid`, `methodology_precondition_passed`).
  `is_alignment_reliable_for_signal` + per-signal
  `AlignmentValidity` struct landed for debuggability.
- **M4.2 — characterization re-run + zcr_ratio investigation.**
  Re-ran the 9-signal characterization under M4.1 alignment;
  74 documentary baselines locked. Phase C investigation
  found the cause as intrinsic to SPC playback at non-native
  pitch (atom MIDI-60 = 261.63 Hz; native rate for cycle_len
  128 = 33489 Hz; project rate 32 kHz forces pitch-register
  fractional stepping). **Outcome 3 per SPEC §24.1:
  pre-emphasis preset implementation defers permanently to
  M5+.** M4.5 will be SKIPPED at its time. See "M4 measurement
  outcomes" below.
- **M4.3 — BRR noise-floor measurement.** Four SPEC §10.10
  metrics wired through `render_to_brr` and
  `AtomRenderReport`. 80 documentary baselines locked (44
  atom-fixture + 36 characterization-signal × 4 metrics).
  Bimodal noise-floor distribution surfaced: low-noise
  cluster (SNR > 10 dB) for canonical sines + silent
  fixtures; high-noise cluster (SNR 5.5–7.4 dB, peak ≈
  18431) for max-amplitude / clipping / dense-harmonic /
  near-Nyquist fixtures.
- **M4.4 — encoder improvement spike (SKIP).** Cross-block
  beam search (`width = 4`) tested against the high-noise
  cluster. Best RMS improvement -2.41% on `64_SINE` (mixed
  peak +1.4%); high-noise cluster sees 0% to -0.17% RMS
  shift; runtime 7.73× M3.3 baseline. **Two of four SPEC
  §24.1 exit conditions fail; no production encoder change
  ships.** Spike implementation preserved feature-flagged in
  `core::brr_encoder` for M5+ reference. Acceptable close per
  consultant M4 plan #17.
- **M4.6 — GUI polish.** `rename_track_id_cascade` defensive
  landing (third v2-schema rename cascade for symmetry with
  M2.8 atom + M3.7 sequence). Plus M4.4 arithmetic wording
  patch per consultant M4.4 audit #2 / #4 / #9 (narrower
  current-sample-term claim + canonical-sine amplitude=0.75
  explanation).
- **`m4-acceptance` bundle (M4.7).** Five-stage rollup:
  M3 regression (subprocess `m3-acceptance`); alignment
  validity (M4.1 plumbing); BRR noise-floor baseline (M4.3
  fixture-pin); M4.4 spike state (feature-flag preservation
  + decode + determinism + loop_click invariants); M4
  baselines integrity audit. `bundle.status = warn` on both
  the canonical M2 fixture and the M3.3 edge-case fixture —
  the only non-clean signal is stage 2's `alignment_valid:
  false` outcome, documented as warn (not fail) per the
  intentional M4.2 outcome 3.

## What's locked

`baselines/m4.json` carries the M4 release baselines and
inherits `baselines/m3.json` by reference. Summary:

- **Identity-gated** (any drift = regression): **empty by
  design**. M4's sub-passes produced measurement + research
  outcomes (alignment plumbing, noise-floor metrics, encoder
  spike skip), not new feature surfaces. M3's 11 atom PCM
  SHAs remain the identity surface; M4 doesn't add to it.
- **Behavior-gated** (numeric / policy contracts) — 6 entries:
  - `M4_RELIABLE_ALIGNMENT_CRITERIA` — 4-condition validity
    predicate; correlation threshold 0.90.
  - `M4_ALIGNMENT_SEARCH_LIMIT` —
    `max_cycle_len_samples_in_signal_set`.
  - `M4_BRR_NOISE_FLOOR_METRICS` — array of 4 metric names;
    formulas fixture-pinned in
    `core/tests/brr_noise_floor_metric.rs` (14 tests).
  - `M4_ENCODER_SPIKE_EXIT_CRITERION` — ≥10% RMS or peak
    improvement on at least one fixture + no loop_click
    regression + no M2 gate regression + ≤2× M3.3 runtime.
  - `M4_METHODOLOGY_REPAIR_BUDGET` — 2 loops max; if
    alignment still anomalous after M4.2 + M4.2.1,
    pre-emphasis defers to M5+.
  - `M4_PRE_EMPHASIS_PIPELINE_ORDER` —
    `render → pre_emphasis → rotation → encode`.
- **Documentary snapshot** (informational; expected to shift
  on declared milestones):
  - 74 M4.2 entries: per-signal alignment_offset,
    normalized_correlation, zcr_ratio, gain_delta_db,
    gain_delta_db_aligned, peak/rms metrics,
    alignment_validity.all_pass; plus
    `M4_2_CHARACTERIZATION_SUMMARY` and
    `M4_2_PHASE_C_ZCR_DOUBLING_ROOT_CAUSE`.
  - 80 M4.3 entries: 44 `M4_3_ATOM_<NAME>_*` + 36
    `M4_3_CHARSIG_<NAME>_*` (4 metrics each).
  - 13 M4.4 entries: 11 per-fixture spike-delta records + 1
    `M4_4_SPIKE_OUTCOME` + 1
    `M4_4_BRR_NEAR_LOCAL_OPTIMUM_FINDING` (M4.6-patched
    wording).
- **Retired**: none.

## Reproduction

`docs/reproduce-m2.md` is the unified reproducer guide covering
M2 + M3 + M4 acceptance. Test count at v0.4-rc: **615 tests
workspace-wide**, all green under `cargo test --workspace`
(was 579 at v0.3-rc; was 521 at v0.2-rc).

## M4 measurement outcomes

M4 was a quality milestone targeting two open questions from
M3: the gaussian characterization methodology limit identified
at M3.5/M3.5.1, and the BRR encoder noise floor that user
audition identified as the dominant atom-render artifact.
**Both investigations produced documented negative-or-deferred
outcomes rather than feature ships; this is the right outcome.**

### Gaussian characterization (M4.1, M4.2)

M3.5.1 identified `align_oracle_to_raw`'s `max_offset = 32` as
inadequate for cycle lengths 64 / 128 / 256. M4.1 expanded the
search range to per-signal `cycle_len_samples`. M4.2 re-ran
the 9-signal characterization with the new alignment.

**Outcome:** the alignment offset for low-frequency signals
shifted from 11–32 (capped at M3.5.1's boundary) to ~55 (the
true DSP delay). However, the four-criterion reliable-alignment
predicate from SPEC §10.9 still fails on 7 of 7
monotonicity-anchor signals — criterion 4
(`gain_separator_ok`) fails universally. Phase C investigation
traced the residual divergence to an intrinsic mismatch in the
characterization design: raw BRR decode is sample-aligned 1:1
at 32 kHz, while oracle render goes through the DSP pitch
register's fractional stepping (atoms at MIDI 60 have native
sample rate ~33489 Hz for `cycle_len = 128`, but project
master rate is 32 kHz). The resulting waveform comparison is
between two physically different processes; the shape
divergence is real, not artifact.

**M4.5 pre-emphasis preset implementation defers permanently
to M5+.** A clean characterization methodology requires
aligning project sample rate with each atom's native rate
(eliminates pitch-register fractional stepping); this is
outside M4 scope. See `docs/reproduce-m2.md` and SPEC §25 M5
prelude scope for the redesign sketch.

### BRR encoder noise floor (M4.3, M4.4)

M4.3 measured the four SPEC §10.10 noise-floor metrics across
11 atom fixtures + 9 characterization signals. The data shows
a bimodal distribution:

- **Low-noise cluster** (SNR > 10 dB) for canonical sines and
  special-case-zero fixtures.
- **High-noise cluster** (SNR 5.5–7.4 dB, peak ≈ 18431) for
  max-amplitude, clipping, dense-harmonic, and near-Nyquist
  fixtures.

The peak = 18431 ceiling is explained by `i16::MAX − (7 << 12
>> 1) = 32767 − 14336` — the BRR ADPCM current-sample term at
the highest non-degenerate shift in filter-0 /
forced-loop-entry / current-term-dominated cases. M4.6 patched
the original "universal mathematical upper bound" wording to
this narrower current-sample-term framing per consultant
M4.4 audit #2; the canonical sines do not hit 18431 because
their atom amplitude is `0.75` (source peak ~24575, peak
error ~10239 = `24575 − 14336`).

M4.4 spike tested cross-block beam search (`beam_width = 4`)
as the deferred M3.4 hypothesis.

**Outcome:** best RMS improvement -2.41% on `64_SINE` (with
peak going +1.4%, a mixed outcome); high-noise cluster sees
0% to -0.17% RMS shift; runtime 7.73× the 2× exit-criterion
ceiling. **SKIP per SPEC §24.1 exit criteria; no production
encoder change ships.** Spike implementation preserved
feature-flagged in `core::brr_encoder::encode_looped_m4_4_spike`
for M5+ reference.

Filters 1–3 add predictor terms from previous decoded samples
and can in principle exceed the current-sample term ceiling,
but the M4.4 beam search did not find such a path within the
2× runtime budget. Future improvements would require either
source-domain preprocessing (forbidden by SPEC §16.9 atom PCM
stability), pre-emphasis (gated on the M5+ methodology
redesign), or a deliberate playback contract change (outside
SPEC scope) — or a wider beam / alternative scoring at a
follow-up M5+ spike.

## M5 prelude scope

Open questions identified during M4 that may be addressed at
M5+:

1. **Characterization methodology redesign.** Align project
   sample rate with each atom's native rate so the DSP pitch
   register operates at `0x1000` (no fractional stepping).
   Requires either a per-atom sample-rate declaration in the
   v2 schema OR a re-tooled characterization harness that
   synthesizes test signals at varying project rates.
2. **Conditional pre-emphasis** (depends on item 1).
3. **BRR encoder noise-floor compensation strategies.** Per
   consultant M4.4 audit #13, productive work operates before
   encoding (source-domain attenuation; requires SPEC §16.9
   amendment) OR pre-emphasis (gated on item 2) OR outside
   current SPEC (BRR shift 13–15 with Mesen2 / `snes_spc`
   validation).
4. **`rename_track_id_cascade` cross-tree wiring.** Currently
   defensive (v2 schema doesn't reference `tracks[].id`
   cross-tree). M5+ schema growth that introduces such
   references should activate the existing cascade body.
5. **`baselines/m5.json`** (inherits M4 by reference; mirrors
   the M4-inherits-M3 pattern).

This is forward visibility, not a commitment. See SPEC §25 for
the locked text.

## Tagging

This release-candidate is recorded as `v0.4-rc` in
`baselines/m4.json::release`. Tag in git when ready to
publish:

```bash
git tag -a v0.4-rc1 <m4.7-close-commit> -m "v0.4-rc1: M4 release candidate"
git push origin v0.4-rc1
```

Annotated tag (`-a`) carries the message + tagger metadata so
the release is fully self-describing from `git show v0.4-rc1`.
Tag the M4.7 close commit (final release-prep patches
including the `m4-acceptance` bundle + reproducer doc + this
notes file + SPEC §25 M5 prelude) rather than M4.6 — the M4.7
patches deliver the release surface that the `v0.4-rc1` label
depends on.
