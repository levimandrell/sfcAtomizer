# SFC Wave Compiler — Status

## Current milestone

**M5.2 — Characterization re-run + decision.** Second M5
research-spike per SPEC §24.1.1 (M5 budget: 1+1 loops; M5.2
is the second of the main loops; M5.2.1 conditional
correction remains unburned). Given M5.1's empirical finding
that Phase E metrics were numerically identical to M4.2,
M5.2's scope was narrower than originally anticipated: lock
M5.2 baselines under the corrected-mechanism framing, apply
the SPEC §10.9 four-criterion validity predicate, decide
outcome per the three SPEC §10.11 alternatives.

**Outcome: methodology_unresolved per SPEC §10.11.** 0/7
anchor signals satisfy all four reliable-alignment criteria.
The methodology cannot characterize gaussian-kernel
non-impulse response at unity pitch with the current
comparison surfaces (raw BRR decode tile vs SPC oracle
render). **M5.3 pre-emphasis preset evaluation is SKIPPED;
defers permanently to M6+** unless a future milestone
introduces fundamentally different characterization
methodology that can clear the four-criterion predicate.
**M5.2.1 correction budget NOT burned** — the limitation is
structural under the current comparison-surface design, not
a clear-cause implementation bug.

### M5.2 strategy

Phase A re-ran `sfcwc characterize-gaussian` (release build)
and confirmed byte-identity against M5.1 Phase E across every
measurement field and top-level value: zero measurement-row
mismatches; `schema_version` / `fixture_set` /
`alignment_search_limit` / `alignment_valid` / `harness_meta`
/ `summary` / `tool` all match exactly. Runtime: 0.605 s (vs
M5.1's 0.539 s; ~12% system jitter; both well under
M5.0-locked 5 s target / 10 s warning).

Phase B locked 74 documentary snapshots in `baselines/m5.json`
mirroring the M4.2 surface shape with M5.2 framing: 72
`M5_2_GAUSSIAN_*_<SIGNAL>` per-signal entries (8 fields × 9
signals) + 1 `M5_2_CHARACTERIZATION_SUMMARY` (`decision_summary`
kind, carries the harness_meta block + anchor rollups +
validity-per-criterion counts + decision outcome) + 1
`M5_2_RESIDUAL_DIVERGENCE_HYPOTHESIS` (`investigation_result`
kind, formalizes the M5.1 inferential closure).

Phase C applied the SPEC §10.9 four-criterion predicate to
the 7 monotonicity-anchor signals:

| Criterion | Anchor pass count | Detail |
|---|---|---|
| 1. `zcr_ratio ∈ [0.9, 1.1]` | 1/7 | `harmonic_16_cycle_64` only |
| 2. `normalized_correlation ≥ 0.90` | 1/7 | `harmonic_16_cycle_64` only |
| 3. `offset_in_range < 256` | 7/7 | universal pass under M4.1 expanded search range |
| 4. `gain_separator_ok ≤ 80%` | **0/7** | universal fail (peak_after_norm ≈ peak_err for every anchor) |
| **All four pass** | **0/7** | criterion 4 alone forces failure on every anchor |

Phase E adopted Path 1 (inferential closure) per brief
recommendation. The M5.1 STATUS argument is formalized as
the `M5_2_RESIDUAL_DIVERGENCE_HYPOTHESIS` baseline entry.
Empirical confirmation via Path 2 (~30-line state-carrying
decode helper) is deferred to M6+ as its result is
predictable from the algebraic argument: `force_filter_0_loop_entry: true`
on every M3.5 canonical atom zeroes the BRR decoder state
at each loop boundary, so a three-way comparison's (1) raw
tiled no-state and (2) raw-with-state-carry produce
byte-identical PCM. The residual (1)/(2)-vs-(3) divergence
narrows to **S-DSP gaussian 4-tap kernel non-impulse
response at unity pitch** — the only remaining candidate
after the inferential closure eliminates BRR-predictor/loop-
state.

### M5.2 phase log

- **Phase A** — re-run characterization (release build,
  oracle from
  `tools/snes_spc_oracle/build/Release/snes_spc_oracle.exe`).
  Diff vs M5.1 Phase E: 0 measurement mismatches across 9
  signals; all top-level fields match. Runtime 0.605 s.
- **Phase B (commit `58e87a5`)** — 74 documentary snapshots
  locked in `baselines/m5.json`. 9 `alignment_offset` + 18
  `gain_delta_db` + 45 `metric_value` + 1 `decision_summary`
  + 1 `investigation_result`. UTF-8 encoding preserved.
- **Phase C** — SPEC §10.9 four-criterion predicate applied;
  0/7 anchors pass all four. Counts encoded in
  `M5_2_CHARACTERIZATION_SUMMARY.value.anchor_validity_per_criterion`.
- **Phase D** — `methodology_unresolved` adopted; `M5.3`
  pre-emphasis SKIPPED; defers permanently to M6+.
- **Phase E** — Path 1 inferential closure formalized in
  `M5_2_RESIDUAL_DIVERGENCE_HYPOTHESIS`. No code change. Path
  2 deferred to M6+.
- **Phase F (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` all green. **617 tests
  workspace-wide** (unchanged from M5.1 close; M5.2 is
  data + docs only, no new code).

### Decisions log additions (M5.2)

- M5.2 characterization re-run: byte-identical to M5.1 Phase
  E (which was byte-identical to M4.2). Locks 74 documentary
  snapshots under corrected-mechanism framing.
- SPEC §10.9 four-criterion predicate: 0/7 anchors satisfy
  all four; `alignment_valid = false`; `recommended_next =
  methodology_unresolved`.
- **Decision: outcome `methodology_unresolved` per SPEC
  §10.11.** Pre-emphasis preset evaluation (M5.3) SKIPPED
  entirely; defers permanently to M6+ unless a future
  milestone introduces a fundamentally different
  characterization methodology.
- M5.2.1 correction budget **not burned** — no clear-cause
  implementation bug exists; the methodology limitation is
  structural under the current comparison-surface design.
- Three-way comparison Path 1 (inferential closure) adopted
  and formalized as `M5_2_RESIDUAL_DIVERGENCE_HYPOTHESIS`.
- Refined residual hypothesis: **S-DSP gaussian 4-tap kernel
  non-impulse response at unity pitch** is the only remaining
  candidate after the M5.1 inferential closure eliminated
  BRR-predictor/loop-state for atoms with
  `force_filter_0_loop_entry: true`.
- **M5 trajectory:** M5.3 skipped; **M5.4 BRR noise-floor
  strategy spike** becomes the next substantive work. M5 still
  produces a tagged release (`v0.5-rc1`) per consultant M5
  plan #33 — a trustworthy negative methodology result is
  valuable project progress.
- All M3.3 phase rotation, M2/M3/M4 acceptance, and 11 atom
  PCM SHA identity tests pass unchanged.

**Next pass: M5.4 — BRR noise-floor strategy spike.**
Per consultant M5 plan #29. With pre-emphasis off the table,
the remaining productive substantive work is investigating
BRR encoder noise-floor compensation strategies (source-domain
attenuation per SPEC §16.9.1 amendment procedure, OR shift-13–15
exploration, OR something else outside the current SPEC).
M5.5 GUI/schema polish + M5.6 acceptance/release follow. PM to
brief.

**Previous milestone (M5.1) — Native-rate characterization harness
verification + investigation.** First M5 research-spike per
SPEC §24.1.1 (M5's tightened 1+1 budget; M5.1 occupies the
main loop, M5.2.1 reserves the conditional correction slot).
The M5.1 brief was reframed post-PM-consultation when
engineer's preflight surfaced a diagnosis-discrepancy in SPEC
§10.11 motivation: pitch register was already programmed at
`0x1000` (unity) for every M3.5 canonical signal since M2.7
via `core::voice_setup`'s hardcoded
`source_sample_rate_hz = 32000` for `TrackKind::AtomSequence`.
The earlier "fractional pitch stepping" attribution of the
M4.2 shape divergence is therefore incorrect. Consultant
verified the preflight finding (audit #1, #2). M5.1 reframes
from "removing fractional pitch stepping" to "investigation of
unity-pitch DSP playback vs host raw decode" per consultant M5
plan #6.

**Outcome:** harness contract verified end-to-end; SPEC §10.11
motivation prose corrected; runtime regression guard in place;
informal characterization run produces metrics numerically
identical to M4.2 (confirming preflight diagnosis). The actual
root cause of the `zcr_ratio ≈ 2` / low-correlation pattern
narrows to candidate (a) S-DSP gaussian 4-tap kernel
non-impulse response at unity pitch (since
`force_filter_0_loop_entry: true` makes candidate (b)
BRR-predictor/loop-state divergence inferentially equivalent
to the no-state path for these atoms). **M5.2 will run the
characterization re-run + decision against this refined
hypothesis space.**

### M5.1 strategy (Path X — already in place)

Engineer's preflight determined that the M5.1 harness contract
(`pitch_register == 0x1000`) was already produced by the
existing M2.7 voice-setup path: `core::voice_setup::build_voice_entry`
calls `pitch_register(source_sample_rate_hz, root, root, 0)`
with `source_sample_rate_hz` hardcoded to 32000 for
`TrackKind::AtomSequence`; with `desired == root` and
`cents == 0` the formula collapses to
`round(4096 × 32000/32000 × 2^0) = 4096 = 0x1000`. The M2 ASM
driver (`core/fixtures/asm/m2_multi_voice_atom.asm`) writes the
resulting bytes directly to `$F2/$F3` with no run-time
override.

M5.1 therefore ships **no code change** to achieve the unity-
pitch contract. The M5.1 deliverables are:

1. **Prose correction** — SPEC §10.11 motivation paragraph
   replaced (consultant audit #7 wording); STATUS M4.2
   narrative gains a blockquote correction note pointing to
   the SPEC update; `baselines/m5.json` rule prose updated to
   drop the "eliminates fractional-stepping" framing.
2. **`harness_meta` field on `CharacterizationReport`** with
   `pitch_register: u32`, `rate_strategy: String`,
   `atom_native_rates_hz: BTreeMap<String, u32>` per SPEC
   §10.11.4. Populated via `build_harness_meta(&signals)`.
3. **Schema bump `v4 → v5`** on the characterization report.
4. **Runtime regression guard test**
   `pitch_register_equals_4096_for_native_rate_signals` drives
   every `m3_5_canonical` signal through
   `build_voice_setup_table` and asserts the encoded
   `pitch_register` equals `0x1000`.

### Phase E informal characterization observation

Release-build run (oracle from
`tools/snes_spc_oracle/build/Release/snes_spc_oracle.exe`, 9
signals, 16000 frames). Runtime: **0.539 s** (well under the
M5.0-locked target of 5 s; warning threshold 10 s).

```
schema_version: 5
report_type: gaussian_characterization
fixture_set: m3_5_canonical
alignment_search_limit: 256
alignment_boundary_hit: false
alignment_valid: false
methodology_precondition_passed: false
recommended_next: methodology_review

harness_meta:
  pitch_register: 4096
  rate_strategy: spc_rate_matches_atom_native_at_project_32k
  atom_native_rates_hz: { 9 signals → 32000 }
```

| Signal | f (Hz) | align_off | corr | zcr_ratio | gain_db | peak_err | peak_after_norm |
|---|---|---|---|---|---|---|---|
| sine_cycle_64 | 500 | 55 | 0.153 | 1.93 | +2.657 | 39053 | 33080 |
| sine_cycle_128 | 250 | 55 | 0.147 | 1.93 | +2.675 | 39053 | 33043 |
| sine_cycle_256 | 125 | 55 | 0.117 | 1.94 | +2.679 | 39053 | 33034 |
| harmonic_2_cycle_64 | 1000 | 55 | 0.274 | 2.58 | +2.589 | 39053 | 33210 |
| harmonic_4_cycle_64 | 2000 | 7 | 0.492 | 2.20 | +2.334 | 39053 | 33710 |
| harmonic_8_cycle_64 | 4000 | 7 | 0.598 | 2.06 | +1.593 | 39053 | 35253 |
| harmonic_16_cycle_64 | 8000 | 63 | **0.984** | **1.00** | -1.223 | 16384 | 16384 |
| all_8_partials | 250 | 40 | 0.613 | 0.97 | +2.534 | 35493 | 30107 |
| normalize_false_clamp | 250 | 40 | 0.597 | 1.55 | +2.456 | 39053 | 33463 |

**Numerically identical to M4.2** (same `align_off` / `corr` /
`zcr_ratio` / `peak_err` on every signal — compare to M4.2
data block above). This confirms the preflight diagnosis: the
M4.2 shape divergence does not depend on the pitch register
since that register was already `0x1000` then. The unity-pitch
contract is **verified active** in both runs; only the
interpretation of the comparison changes.

### Phase E three-way comparison — deferred to M5.2

The three-way comparison consultant #4 recommends (raw tiled /
BRR-decoded-with-predictor-state-carried / oracle) is
**deferred to M5.2**. Engineer's inferential finding makes the
comparison's (2) reducible to (1) for these atoms:

- Every M3.5 canonical atom sets
  `render.force_filter_0_loop_entry: true`
  (`core::characterize_gaussian::atom_base` line 69).
  Filter 0 BRR blocks contain no prior-sample predictor term,
  so the decoder state is **reset** at each loop boundary.
  Within one cycle iteration the decoder state evolves
  through filters 0–3; at the loop wrap, filter 0 on block 0
  zeroes that state again. The PCM output of cycle N+1 is
  therefore byte-identical to cycle N.
- Consequently, "raw tiled (no state-carry)" and "BRR-decoded
  with state-carry across loop iterations" produce the SAME
  PCM buffer for these atoms. The three-way comparison's (1)
  and (2) are equivalent.
- The remaining candidate cause for the (1)/(2)-vs-(3)
  divergence narrows to **gaussian-kernel non-impulse response
  at unity pitch** (the S-DSP 4-tap kernel applies non-impulse
  weights at integer offsets too). M5.2 investigates this
  hypothesis.

The engineering effort to *implement* a state-carrying decode
helper would still be small (~30 lines), but the comparison
itself would not add signal beyond what the inferential
argument establishes. Per the M5.1 brief's feasibility caveat
("if it's larger, defer to M5.2"), deferral is the right call.

### M5.1 phase log

- **Phase 0 (commit `900f6ad`)** — `docs(spec+baselines):
  retract fractional-stepping diagnosis at SPEC §10.11`. SPEC
  §10.11 motivation paragraph + definition item 1 reframed
  per consultant audit #7. STATUS M4.2 narrative gains a
  blockquote correction note (Option A; archived narrative
  preserved). `baselines/m5.json:16`
  `M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER` rule prose
  updated. Numeric value (4096), `locked_at` (M5.0), and
  `kind` (harness_constant) unchanged.
- **Chore (commit `637b20b`)** — `chore: track rust 1.95.0
  clippy advances`. Three new lints
  (`manual_is_multiple_of`, `doc_lazy_continuation`,
  `items_after_test_module`) fired on pre-existing code under
  the stable channel advance to 1.95.0. Minimum-impact fixes
  applied so the M5.1 commit sequence keeps clippy green; not
  part of the M5.1 narrative.
- **Phase C (commit `2b0383a`)** — `feat(core): add
  HarnessMeta struct + field on CharacterizationReport`. New
  `HarnessMeta` struct + `build_harness_meta` helper in
  `core::characterize_gaussian`. Field added with
  `#[serde(default)]`; `main.rs` constructs with
  `HarnessMeta::default()` (Phase B swaps this).
- **Phase B (commit `9fd858e`)** — `feat(core): harness_meta
  emission + schema v5 bump`. CLI now populates `harness_meta`
  via `build_harness_meta(&signals)`; `schema_version` bumped
  `4 → 5`.
- **Phase D (commit `1a6496c`)** — `test(core):
  pitch_register_equals_4096_for_native_rate_signals
  regression guard`. Drives every M3.5 canonical signal
  through `build_voice_setup_table` and asserts
  `pitch_register == 0x1000` on voice 0. **617 tests
  workspace-wide** (was 616 at M5.0 close; +1 from this
  test).
- **Phase E** — informal release-build characterization run;
  numbers identical to M4.2; three-way comparison deferred to
  M5.2 with inferential rationale. **No baseline update.**
  Runtime 0.539 s.
- **Phase F (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` all green throughout. **617 tests
  workspace-wide** (was 616 at M5.0 close; +1 from Phase D).

### Decisions log additions (M5.1)

- **SPEC §10.11 motivation paragraph corrected** per
  consultant audit #1, #2, #5, #7. Fractional-stepping
  diagnosis retracted; pitch register verified at `0x1000`
  since M2.7 via `core::voice_setup`. Implementation contract
  (definition item 1) preserved; only the motivation prose
  changes.
- **M5.1 strategy: Path X** — unity pitch already in place
  via `voice_setup.rs` M2.7 hardcoding. No code changes
  needed for the harness contract itself; M5.1 work is
  reporting + verification.
- **`harness_meta` field** added to `CharacterizationReport`;
  populated per SPEC §10.11.4 with `pitch_register = 4096`,
  `rate_strategy = "spc_rate_matches_atom_native_at_project_32k"`,
  and per-signal `atom_native_rates_hz` (all 32000 for the
  M3.5 canonical set).
- **Schema bump `v4 → v5`** on the characterization report.
- **`pitch_register_equals_4096_for_native_rate_signals`**
  runtime test passes for all 9 signals (regression guard).
- **Phase E informal observation:** `alignment_best_offset` /
  `normalized_correlation` / `zcr_ratio` / `peak_err`
  **numerically identical to M4.2** — confirming preflight
  finding that pitch register was already `0x1000` in M4.2.
  The M4.2 shape divergence is therefore not
  pitch-register-related; the M5.2 investigation focuses on
  gaussian-kernel behavior at unity pitch.
- **Phase E three-way comparison deferred to M5.2** with
  inferential rationale: `force_filter_0_loop_entry: true`
  makes the BRR predictor-state-carry path equivalent to the
  no-state path for these atoms, so the residual hypothesis
  is gaussian-kernel-at-unity (candidate (a) of SPEC §10.11
  motivation).
- **Runtime: 0.539 s** vs M5.0-locked 5 s target / 10 s
  warning.
- **M5.1 does NOT re-run characterization for
  baseline-locking; that's M5.2.** Phase E run is informal;
  no `baselines/m5.json` changes.
- **M5.1.1 docs-only patch follows this commit** to annotate
  v0.4-rc1-era historical artifacts (surfaces 4–6 per the
  M5.1 supplement disposition):
  `baselines/m4.json::M4_2_PHASE_C_ZCR_DOUBLING_ROOT_CAUSE`
  gains a `_correction_m5_1` sibling field;
  `RELEASE_NOTES_v0.4-rc.md` and `docs/reproduce-m2.md` gain
  inline `[M5.1 correction: ...]` annotations next to each
  retracted claim. Annotation-style (not rewriting) preserves
  released-artifact provenance. No tag change; `v0.4-rc1`
  stays at `1223606`.
- **M5.2.1 conditional correction slot remains unburned.**
  M5 repair budget is 1+1; M5.1 is loop 1, M5.2.1 is the
  conditional loop 2 if M5.2 surfaces a clear-cause fix.

**Previous milestone (M5.0) — M5 Contracts Freeze.** No implementation. Contracts
only. Same shape as M2.0 / M3.0 / M4.0: lock the contracts
that M5.1+ sub-passes build against. Per consultant M5 plan
#36. No encoder change; no atom render formula change (SPEC
§16.9 stability held by default per §16.9.1 forward-visibility
amendment); no v2 schema change (per consultant #5 — schema
changes are too permanent for a methodology experiment); no
M2 / M3 / M4 numeric baseline change.

**Outcomes:**

- **SPEC §10.11 — native-rate characterization contract**
  (commit `f5e7a96`). Locks the M5 harness contract that
  M5.1 implements: each characterization SPC MUST configure
  the DSP pitch register at exactly `0x1000` (4096); new
  top-level report field `harness_meta` records pitch
  register + rate strategy + per-signal native rates. M5.1
  implementation is Option α — scoped to the harness only;
  the v2 schema's `atom_pool[]` does NOT gain a permanent
  `native_sample_rate_hz` field at M5 per consultant #5.
  Three M5.2 outcomes locked: `reliable_preset_eval` (M5.3
  unlocked), `reliable_no_preset_needed` (M5.3 skipped,
  proceed to M5.4), `methodology_unresolved` (pre-emphasis
  defers permanently to M6+, proceed to M5.4 without
  preset).
- **SPEC §10.9 — M4.0 thresholds reaffirmed** (commit
  `44d3c27`). The four-criterion reliable-alignment
  predicate carries forward unchanged through M5
  (`zcr_ratio ∈ [0.9, 1.1]`, `normalized_correlation ≥ 0.90`,
  `alignment_best_offset < alignment_search_limit`,
  `peak_abs_error_after_gain_normalization ≤ 80%` of
  unnormalized peak). No relaxation in M5.0; relaxation at
  M5.2 requires explicit PM review per consultant #7.
- **SPEC §10.9 — pre-emphasis preset report fields**
  (commit `0db148d`). M5.3 conditional preset evaluation
  gains optional report fields (`pre_emphasis_applied`,
  `pre_emphasis_preset_id`, `pre_emphasis_filter_form`,
  `rotation_offset_with_pre_emphasis`,
  `loop_click_abs_with_pre_emphasis`,
  `noise_floor_metrics_with_pre_emphasis`). Filter form
  must be hand-derivable (FIR up to 3 taps OR one-pole IIR
  shelf); **no filter-design crate dependency** unless
  explicit PM approval at M5.3 brief time per consultant
  #13.
- **SPEC §24.1.1 — M5 methodology repair budget tightening**
  (commit `66edab5`). M5 inherits M4.0's research-spike
  pattern but tightens to **1+1 loops**: M5.1 native-rate
  harness + M5.2 characterization re-run + at most one
  M5.2.1 correction. Stricter than M4's 2-loop budget per
  consultant #4 — M4 surfaced that methodology investigation
  can iterate indefinitely without converging; M5 commits to
  a decision (positive or negative) within tighter bounds.
- **SPEC §16.9.1 — atom PCM stability amendment procedure**
  (commit `2980cab`). Forward-visibility documentation only;
  NOT activated in M5.0. Six-step procedure for IF M5 ever
  chooses source-domain attenuation work: authorization gate,
  old SHA retirement, new SHA pinning, acceptance regression,
  release-note warning, close-out audit. M5 default position
  per consultant #10 / #19: hold the §16.9 line.
- **`baselines/m5.json` scaffolded** (commit `2950305`).
  Six behavior_gated contract entries:
  `M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER` (4096
  with `test:` field), `M5_METHODOLOGY_REPAIR_BUDGET`
  (1+1 loops), `M5_RELIABLE_ALIGNMENT_THRESHOLD_INHERITED`
  (M4.0 thresholds carry forward), `M5_PRE_EMPHASIS_FILTER_FORM_CONSTRAINT`
  (hand-derivable forms only), `M5_ATOM_PCM_STABILITY_HELD`
  (no §16.9 amendment unless authorized),
  `M5_RUNTIME_BUDGET` (target 5 s / warning 10 s for the
  9-signal characterization). `identity_gated` empty by
  design (M5 default holds §16.9). `inherits_m4: true`.
- **Pitch-register fixture pin test** (commit `ef87359`).
  Mirrors the M4.0 Phase G pattern: pin the baseline value
  4096 (= 0x1000) at compile time via the M2.8.1
  `include_str!` pattern. M5.0 fixture-pin is a structural
  drift-guard; M5.1 will add the runtime-behavior test
  `pitch_register_equals_4096_for_native_rate_signals`.

### M5.0 phase log

- **Phase A (commit `f5e7a96`)** — SPEC §10.11 native-rate
  characterization contract (Option α, harness-scoped; no v2
  schema change; three M5.2 outcomes locked).
- **Phase B (commit `44d3c27`)** — SPEC §10.9 reaffirms M4.0
  thresholds; no relaxation.
- **Phase C (commit `0db148d`)** — SPEC §10.9 pre-emphasis
  preset report fields; hand-derivable filter form
  constraint.
- **Phase D (commit `66edab5`)** — SPEC §24.1.1 M5 repair
  budget tightening (1+1 vs M4's 2 loops).
- **Phase E (commit `2980cab`)** — SPEC §16.9.1 amendment
  procedure forward-visibility documentation.
- **Phase F (commit `2950305`)** — `baselines/m5.json`
  scaffold; six behavior_gated contracts; inherits_m4.
- **Phase G (commit `ef87359`)** — pitch register fixture
  pin (`m5_pitch_register_constant_pinned_at_4096`).
- **Phase H (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **616 tests
  workspace-wide** (was 615 at M4.7 close; +1 from the
  fixture-pin test).

### Decisions log additions (M5.0)

- M5 entry approved per consultant M5 plan #35; no M4 surface
  blocks entry.
- M5.0 contracts frozen per consultant M5 plan #4, #5, #6, #7,
  #10, #11, #12, #13, #16, #19, #22:
  - Native-rate characterization harness contract
    (SPEC §10.11); Option α scoped to harness, NO v2 schema
    change.
  - M4.0 reliable-alignment thresholds reaffirmed without
    relaxation.
  - Pre-emphasis preset report fields locked (coefficients
    TBD at M5.3).
  - Methodology repair budget tightened to 1+1 loops; stricter
    than M4's 2-loop budget.
  - SPEC §16.9 amendment procedure (§16.9.1) documented for
    forward visibility; NOT activated.
  - `baselines/m5.json` scaffolded; inherits M4 by reference.
  - Pitch-register fixture pin at 4096 (`0x1000`).
  - Runtime budget locked at target < 5 s / warning > 10 s.
- Research-spike vs implementation-pass split carries forward
  (per consultant #2–#4): **M5.1 / M5.2 / M5.4 are
  research-spikes** with exit criteria; **M5.0 / M5.3 / M5.5
  / M5.6 are contracted implementation** (M5.3 conditional on
  M5.2 outcome).
- M5 sub-pass plan: M5.0 contracts (this pass), M5.1
  native-rate harness implementation, M5.2 characterization
  re-run + decision, M5.2.1 conditional correction (at most
  one), M5.3 conditional pre-emphasis evaluation, M5.4 BRR
  noise-floor strategy spike, M5.5 GUI / schema polish, M5.6
  acceptance + release.
- Release tag policy: `v0.5-rc1` only after final M5.6 close
  + integrity audit per M2 / M3 / M4 lessons.
- **M5 default position: hold the §16.9 line.** Source-domain
  preprocessing defers to M6+ unless M5 data demands it AND
  PM authorizes amendment per the §16.9.1 procedure.

**Previous milestone (M4.7) — M4 release prep + acceptance
+ tag `v0.4-rc1`.** Final M4 sub-pass. Mirrors M3.8 structure
with the same baselines-inheritance + literal-pin patterns.
No encoder change; no SPEC contract change; no M2 / M3 / M4
numeric baseline change. Release notes are the load-bearing
piece — M4.2 outcome 3, M4.4 SKIP, and M4.5 permanently
skipped are all explicit, not obscured.

**Outcomes:**

- **`m4-acceptance` bundle CLI** (Phase 1, commit `e3f0e5e`).
  Five-stage rollup analog of `m3-acceptance`: M3 regression
  (subprocess `m3-acceptance`) → alignment validity (M4.1
  plumbing tests) → BRR noise-floor baseline (M4.3 fixture-pin)
  → M4.4 spike state (feature-flag preservation + decode +
  determinism + loop_click invariants) → baselines integrity
  audit (identity_gated empty at M4 by design). Stage 2
  reports `warn` for the M4.2-outcome-3 `alignment_valid:
  false` reality (documented and intentional); stages 1/3/4/5
  report `ok`. Bundle JSON built via `Map::insert` pattern per
  the M3.8 Windows-stack-overflow fix. `bundle.status = warn`
  on both `fixtures/projects/canonical_m2/canonical_m2.sfcproj.json`
  and `fixtures/projects/atom_edge_cases/harmonic_16_cycle_64.sfcproj.json`.
- **Reproducer doc updated for M4** (Phase 2, commit
  `b2e6d82`). Option A continues — `docs/reproduce-m2.md` is
  the unified guide. New sections: `m4-acceptance` invocation
  + expected stderr summary + per-stage description; "M4-
  specific reproduction notes" covering the
  `alignment_valid: false`, no-production-encoder-change, and
  M4.5-permanently-skipped realities; "Reproduce the M4.3 BRR
  noise-floor measurement" with the ignored print-helper
  invocations. Test count updated 579 → 615.
- **`RELEASE_NOTES_v0.4-rc.md` shipped** (Phase 3, commit
  `82d0522`). Highlights covering M4.0–M4.7, locked-baseline
  summary, the load-bearing "M4 measurement outcomes" section
  documenting M4.2 outcome 3 + M4.4 SKIP + M4.5 permanent
  defer per consultant M4.4 audit #11, M5 prelude scope
  sketch, tagging instructions.
- **M4 baseline classification audit complete** (Phase 4,
  commit `9129ce9`). `identity_gated` empty by design (M4
  was measurement / research, not feature surfaces).
  `behavior_gated` 6 entries: three M4.0 policy contracts
  with `test: null` (acceptable per M3.8 pattern —
  `M4_ALIGNMENT_SEARCH_LIMIT`, `M4_METHODOLOGY_REPAIR_BUDGET`,
  `M4_PRE_EMPHASIS_PIPELINE_ORDER`); three patched with
  `test:` fields pointing at the actual tests
  (`M4_RELIABLE_ALIGNMENT_CRITERIA` →
  `m4_1_validity_predicate_*`,
  `M4_BRR_NOISE_FLOOR_METRICS` →
  `core/tests/brr_noise_floor_metric.rs::*`,
  `M4_ENCODER_SPIKE_EXIT_CRITERION` →
  `m4_4_spike_does_not_worsen_loop_click_vs_m3_3_production`).
  `documentary_snapshot` 167 entries (74 M4.2 + 80 M4.3 + 13
  M4.4) — no `test:` required per pattern.
- **M5 prelude scope documented in SPEC §25** (Phase 5,
  commit `78a952f`). Five forward questions: characterization
  methodology redesign (project-rate alignment with atom
  native rate), conditional pre-emphasis (gated on item 1),
  BRR noise-floor compensation strategies (source-domain
  attenuation / pre-emphasis / BRR-spec extension /
  wider-beam follow-up), `rename_track_id_cascade` cross-tree
  wiring on future schema growth, `baselines/m5.json` with
  inherits-M4 pattern.
- **`v0.4-rc1` annotated tag** at the M4.7 close commit
  (Phase 6, this entry; tagged after STATUS push).

### M4.7 phase log

- **Phase 1 (commit `e3f0e5e`)** — `sfcwc m4-acceptance`
  subcommand. `cmd_m4_acceptance` + `M4AcceptanceBundleArgs` +
  `build_m4_acceptance_bundle_json` mirroring the M3.8 split.
  ~350 lines.
- **Phase 2 (commit `b2e6d82`)** — `docs/reproduce-m2.md`
  extended with M4 reproduction section. Test count updated.
- **Phase 3 (commit `82d0522`)** —
  `RELEASE_NOTES_v0.4-rc.md` (new file, 264 lines).
  Mirrors `RELEASE_NOTES_v0.3-rc.md` shape. Explicit
  outcome documentation.
- **Phase 4 (commit `9129ce9`)** — `baselines/m4.json`: added
  `test:` fields to 3 of 6 behavior_gated entries; the
  remaining 3 stay `test: null` as policy contracts per M3.8
  pattern. `identity_gated` confirmed empty by design.
- **Phase 5 (commit `78a952f`)** — `SPEC.md` §25 M5 prelude
  scope.
- **Phase 6 (this entry)** — STATUS rewrite + `v0.4-rc1`
  annotated tag at the M4.7 close commit.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **615 tests
  workspace-wide** (same as M4.6 close; M4.7 added no new
  test functions — release prep is implementation +
  documentation work).
- **m4-acceptance runtime confirmation (final tag-eve run):**

  ```
  m4-acceptance: project_a=fixtures/projects/canonical_m2/canonical_m2.sfcproj.json
    stage_1_m3_regression: ok
    stage_2_alignment_validity: warn (alignment_valid=false expected; M4.2 outcome 3)
    stage_3_brr_noise_floor_baseline: ok
    stage_4_m4_4_spike_state: ok
    stage_5_baselines_integrity: ok
    bundle.status: warn
  ```

  Both the canonical M2 fixture and the M3.3
  `harmonic_16_cycle_64.sfcproj.json` reproducer fixture
  return the same `bundle.status = warn` end-to-end — the
  only non-clean signal is the M4.2-outcome-3
  `alignment_valid: false`, documented and intentional.

### Decisions log additions (M4.7)

- `m4-acceptance` bundle shipped; 5-stage rollup using
  `Map::insert` (NOT `serde_json::json!{}` — M3.8 stack-overflow
  lesson carried forward).
- Reproducer doc updated for M4 (single-doc Option A
  continues).
- `RELEASE_NOTES_v0.4-rc.md` shipped with explicit M4.2
  outcome 3 + M4.4 SKIP + M4.5 permanently skipped
  documentation per consultant M4.4 audit #11.
- Baseline classification audit complete: 0 identity_gated,
  6 behavior_gated (3 with test: fields added, 3 policy
  contracts with test: null), 167 documentary_snapshot.
- M5 prelude scope documented in SPEC §25 (5 forward
  questions). Includes the wider-beam-follow-up M5 candidate
  per consultant M4.4 audit #6.
- `v0.4-rc1` annotated tag at the M4.7 close commit.
- **Next pass: M5 prelude.** PM may consult before M5 entry
  (M3-style cadence — consultant planning pass produces M5
  sub-pass structure), or proceed directly to an M5 entry
  brief.

**Previous milestone (M4.6) — GUI polish + M4.4 arithmetic
wording patch.** Small pass; two independent layers. Per
consultant M4.4 audit close-out (layer 1) plus consultant
M3 close-out audit #19 item 5 (layer 2). No encoder change;
no SPEC contract change; no M2 / M3 / M4 numeric baseline
change.

**Layer 1 — M4.4 arithmetic wording patch** (commit `392dd04`).
Consultant M4.4 audit #2, #4, #9: the "no filter/shift
trajectory can exceed 14336" claim in STATUS + baselines/m4.json
overstated the math. Patched to the narrower "current-sample
term at shift = 12 explains the plateau in filter-0 /
forced-loop-entry / current-term-dominated cases" wording.
Filters 1–3 add predictor terms from previous decoded samples,
so the decoded value **can** exceed the raw shifted-nibble
current-sample term — the 14336 is the practical
current-encoder ceiling under the beam-search-width-4 spike,
not a universal mathematical upper bound. Also added the
canonical-sine amplitude-factor explanation (atom amplitude =
0.75, so source peak ~24575 ≈ 0.75 × 32767; peak error ~10239 =
24575 − 14336). Block-M4.7 release-prep wording fix.

**Layer 2 — `rename_track_id_cascade` defensive landing**
(commits `726c036` model + GUI, `d7ec00b` tests). Mirrors M2.8's
`rename_atom_id_cascade` and M3.7's
`rename_sequence_id_cascade`. The third v2-schema rename
cascade lands defensively for symmetry; the body does the
self-update plus `mark_dirty()`. **No cross-tree cascade runs**
because the v2 schema does not currently reference
`tracks[].id` from anywhere else (verified by grep across
core + app sources). A "future-schema-growth site" comment
marks where cascade logic would live if a later v2 rev
introduces fields that reference track ids. GUI side: the track
edit panel's id field switches from direct mutation to buffer +
cascade call with self-revert on rejection — same M3.7
mechanics as the sequence-id field.

### M4.6 phase log

- **Phase 0 (commit `392dd04`)** — `STATUS.md` M4.4
  "Investigation finding" reworded; `baselines/m4.json`
  `M4_4_BRR_NEAR_LOCAL_OPTIMUM_FINDING` `value` and `_note`
  rewritten per consultant audit #2 / #4 / #9.
- **Phase A + B (commit `726c036`)** —
  `V2EditorModel::rename_track_id_cascade(idx, new_id) -> bool`
  added next to `set_track_id` in `app/src/v2_editor.rs`; SPEC
  §16.6 rule 49 pattern inlined. GUI side: `app/src/app_main.rs`
  `draw_track_edit_panel` id text field swapped from direct
  mutate to buffer + cascade.
- **Phase C (commit `d7ec00b`)** — five model-level tests in
  `app/src/v2_editor.rs::tests` mirroring M3.7's
  sequence-rename test shape: updates_track_id, rejects
  collision, rejects invalid regex, out-of-range no-op, same-id
  no-op success. All five pass.
- **Phase D (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **615 tests
  workspace-wide** (was 610 at M4.4 close; +5 new
  track-rename tests).

### Decisions log additions (M4.6)

- M4.4 arithmetic wording patched per consultant M4.4 audit
  #2, #4, #9. The "structural ceiling" claim narrowed to
  "current-sample term at shift=12, dominated in filter-0 /
  forced-loop-entry cases." Canonical-sine amplitude factor
  (0.75 → source peak ~24575 → peak error ~10239) added to
  explanation. Filter-1/2/3 predictor surface explicitly
  named as a follow-up direction not exhausted by
  beam-width-4.
- `rename_track_id_cascade` lands defensively per consultant
  M3 close-out audit #19 item 5. v2 schema does not currently
  reference `tracks[].id` cross-tree, so no actual cascade
  work needed; method mirrors M2.8 atom + M3.7 sequence
  patterns for symmetry and future schema growth.
- Consultant signed off on M4.4 SKIP per audit #14. M4.5 was
  already permanently skipped at M4.2 outcome 3. M4.6 confirms
  both holds.
- Three v2-schema id surfaces (atom, sequence, track) now have
  parallel `rename_*_id_cascade` methods + GUI wirings + test
  coverage. Pattern is consistent across M2.8 / M3.7 / M4.6.
- No encoder changes; no SPEC contract changes; no numeric
  baseline changes; no new crate dependencies.

**Next pass: M4.7 — Acceptance + release prep + tag `v0.4-rc1`.**
Analog of M3.8. PM to brief.

**Previous milestone (M4.4) — Encoder improvement spike (SKIP).**
Research-spike per SPEC §24.1; consultant M4 plan #4, #17.
**Decision: skip; no production encoder change ships.** Two of
four SPEC §24.1 exit conditions failed; the spike's beam-search
strategy didn't clear the threshold, and the residual finding is
documented for M5+ reference. Per consultant plan #17 ("negative
finding is an acceptable M4.4 outcome"), M4.4 closed cleanly.

**Strategy:** Hypothesis A from the brief — **cross-block beam
search** (`beam_width = 4`), the M3.4-deferred predictor
optimization per consultant M3.3 audit #21. Implementation lives
in `core::brr_encoder::encode_looped_m4_4_spike` behind an
`M44SpikeConfig { strategy: M44Strategy::BeamSearch { beam_width } }`
feature flag; tests invoke it directly; the M2.2 greedy
`encode_looped` stays as the production encoder unchanged.
Rationale per M4.3 STATUS: the bimodal noise floor at
`peak = 18431` across the high-noise atom cluster suggested the
greedy per-block search was leaving cross-block gains on the
table.

**Outcome: SKIP per SPEC §24.1 exit criterion application.**

- **Condition 1 — ≥10% improvement on RMS or peak for at least
  one fixture: FAIL.** Best RMS improvement across all 11 atom
  fixtures was **-2.41% on `64_SINE`** (with peak going +1.4%, a
  mixed outcome). Best peak improvement was **-0.16% on
  `CYCLE_256_CANONICAL_SINE`**. The 10% gate is nowhere near
  approached on any fixture, and the high-noise cluster
  (`MAX_AMPLITUDE_NO_NORMALIZE`, `NORMALIZE_FALSE_*`,
  `HARMONIC_16_CYCLE_64`, `ALL_8_PARTIALS_*`) sees 0% to -0.17%
  RMS improvement and identical `peak = 18431`. **The structural
  ceiling is arithmetic, not a search artifact** — see
  "Investigation finding" below.
- **Condition 2 — no `loop_click_abs` worsening: PASS.** The
  in-process test
  `m4_4_spike_does_not_worsen_loop_click_vs_m3_3_production`
  asserts the M3.3 phase-rotation improvement gate holds against
  the spike encoder for every atom fixture. All 11 pass; the
  spike picks the same lex-optimal rotation as M3.3.
- **Condition 3 — no M2 behavioral regression: PASS (trivially).**
  No production encoder swap = no M2 risk surface. Confirmed
  by leaving the canonical `m2-acceptance` fixture untouched.
- **Condition 4 — encode runtime ≤ 2× M3.3 baseline: FAIL.**
  Release-build wall-clock measurement on the 11-atom suite:
  M3.3 production **13.709 ms**, M4.4 spike (beam_width=4)
  **105.928 ms**, ratio **7.73×**. Way over the 2× ceiling.
  Wider beams (16, 64) would scale ~4× and ~16× worse and were
  not pursued — runtime alone disqualifies even if the
  improvement gate cleared.

**Investigation finding (the structural ceiling — narrowed per
consultant M4.4 audit #2, #4, #9).** The high-noise
atom-fixture cluster's `peak_abs_raw_vs_source = 18431` is
explained by the current-sample contribution of 4-bit ADPCM at
the highest non-degenerate shift:

```
max BRR-decoded magnitude from current-sample term at shift = 12
  = max_nibble × 2^shift / 2
  = 7 × 4096 / 2
  = 14336

i16::MAX − current_sample_term_max
  = 32767 − 14336
  = 18431  ← the observed plateau for full-scale peaks
```

The `18431` ceiling is **structural for samples whose critical
peak is encoded through the current-sample term at shift = 12**,
especially filter-0 / forced-loop-entry cases. It is **NOT** a
universal mathematical upper bound on every decoded BRR sample
under filters 1–3 — those filters add predictor terms from
previous decoded samples, so the decoded value can exceed the
raw shifted-nibble current-sample term. However, **for the M4.4
spike's actual measurements** — full-scale peaks in filter-0,
at the forced-loop-entry block (always filter-0 here), and at
current-term-dominated cases for the high-noise cluster — the
14336 current-sample ceiling explains the observed `18431`
plateau, and the beam-search-width-4 spike did not find a
trajectory around it.

The **canonical sines do not hit 18431** because their atom
amplitude is `0.75`, so their source peaks are below full-scale
(`~24575 ≈ 0.75 × 32767`). Peak error of `~10239` for those
fixtures matches `24575 − 14336 = 10239` — the same
current-sample-term ceiling applied to a lower source peak.

Shift values 13–15 trigger the special-case
`negative → −2048, positive → 0` path that's degraded enough to
be unusable for music (and M1's encoder contract excludes them
per `brr_encoder.rs` doc-comment).

Productive future directions would have to operate either
**before** the encoder (e.g. source-PCM attenuation, which §16.9
forbids without atom amplitude-field change; or pre-emphasis,
permanently deferred to M5+ per M4.2 outcome 3), **outside the
spec** (e.g. extending into shift 13+, which would change the
playback contract and require Mesen2 / `snes_spc` oracle
validation), or **inside the filter-1/2/3 prediction surface**
(which the beam-search-width-4 spike did NOT exhaust — wider
beams or alternative scoring may find predictor trajectories
that lift decoded peaks beyond the current-sample ceiling). The
first two are out of M4 scope; the third is a candidate for a
follow-up spike if PM decides to revisit at M5+.

### M4.4 per-fixture measurement table (11 atom fixtures)

| Fixture | M4.3 peak | M4.4 peak | Δ peak % | M4.3 rms | M4.4 rms | Δ rms % | Δ SNR dB |
|---|---|---|---|---|---|---|---|
| 128_SINE | 9582 | 9580 | -0.02 | 4795.19 | 4792.09 | -0.06 | +0.01 |
| 64_SINE | 10239 | 10380 | **+1.38** | 5108.55 | 4985.33 | **-2.41** | +0.21 |
| AMPLITUDE_ZERO | 0 | 0 | — | 0.00 | 0.00 | — | 0.00 |
| ALL_PARTIALS_ZERO_NORMALIZE_TRUE | 0 | 0 | — | 0.00 | 0.00 | — | 0.00 |
| TWO_PARTIALS_CANCEL_PARTIALLY | 10239 | 10239 | 0.00 | 1532.33 | 1532.15 | -0.01 | 0.00 |
| MAX_AMPLITUDE_NO_NORMALIZE | 18431 | 18431 | 0.00 | 10576.55 | 10558.60 | -0.17 | +0.01 |
| NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY | 18431 | 18431 | 0.00 | 10562.45 | 10562.45 | 0.00 | 0.00 |
| HARMONIC_16_CYCLE_64 | 18431 | 18431 | 0.00 | 12329.89 | 12329.89 | 0.00 | 0.00 |
| ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8 | 18431 | 18431 | 0.00 | 4574.39 | 4574.38 | -0.00 | 0.00 |
| PHASE_CYCLES_0_9999 | 9581 | 9581 | 0.00 | 4797.03 | 4796.69 | -0.01 | 0.00 |
| CYCLE_256_CANONICAL_SINE | 9995 | 9979 | -0.16 | 4920.93 | 4910.45 | -0.21 | +0.02 |

### M4.4 phase log

- **Phase A (commit `4f895b4`)** — feature-flagged
  `encode_looped_m4_4_spike` with `M44Strategy::BeamSearch`.
  ~230 lines. Score by `(sum_sq, peak, bytes)` lex order;
  bytes tie-break ensures determinism.
- **Phase B (commit `f09eeb3`)** — 3 hard tests + 4 ignored
  helpers in `core/tests/m4_4_spike_measurement.rs`:
  decode-roundtrip clean, deterministic two-run identity,
  `loop_click` doesn't worsen vs production; plus
  print-helpers for atom fixtures / characterization signals /
  runtime / `beam_width = 16` follow-up.
- **Phase C** — exit criterion applied; 2/4 conditions fail.
- **Phase D (commit `e8265c4`)** — skip path. 13 new
  `M4_4_*` documentary entries in `baselines/m4.json`:
  11 per-fixture delta records + 1
  `M4_4_SPIKE_OUTCOME` + 1
  `M4_4_BRR_NEAR_LOCAL_OPTIMUM_FINDING`.
- **Phase E (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **610 tests
  workspace-wide** (was 607 at M4.3 close; +3 hard M4.4
  tests; 4 new `#[ignore]` helpers not counted).

### Confirmations (stop-condition checks)

- ✓ Spike decode-roundtrip clean for every fixture (no invalid
  BRR).
- ✓ Spike deterministic across two runs for all 11 fixtures
  (peak / rms-via-`to_bits` / snr-via-`to_bits` / clipping /
  loop_click / rotation_offset all bit-identical).
- ✓ All 11 atom PCM SHA identity tests pass unchanged
  (SPEC §16.9 stability preserved — spike doesn't touch
  render formula).
- ✓ M3.3 phase-rotation improvement gate test
  (`phase_rotation_loop_click_never_regresses_against_pre_m3`)
  passes unchanged.
- ✓ No new crate dependencies.
- ✓ No SPEC contract changes.
- ✓ No M2 / M3 baseline changes.
- ✓ Spike implementation preserved in `core::brr_encoder`
  for M5+ reference; not wired into `render_to_brr` (no
  production-code surface change beyond two new public types
  `M44SpikeConfig` / `M44Strategy`).

### Decisions log additions (M4.4)

- M4.4 spike strategy chosen: cross-block beam search
  (Hypothesis A from brief). `beam_width = 4` based on the
  M4.3 bimodal-noise-floor interpretation. The greedy M2.2
  encoder is mathematically `beam_width = 1`.
- Spike scoring targets RMS primary (cumulative `sum_sq`) and
  peak secondary, matching SPEC §24.1 exit criterion phrasing
  (`≥ 10% on rms OR peak`).
- Phase C: 2 of 4 exit conditions FAIL (criterion 1
  improvement, criterion 4 runtime). 2 of 4 PASS (criterion 2
  loop_click, criterion 3 M2 regression). Decision: SKIP.
- Phase D skip path: 13 documentary entries recorded.
  Spike implementation stays feature-flagged in
  `core::brr_encoder` for M5+ reference.
- Investigation finding: peak_abs_raw_vs_source = 18431 is
  arithmetically structural at shift=12 (= i16::MAX − max
  BRR-decoded). Beam search cannot fix arithmetic ceilings.
- Acceptable close per consultant M4 plan #17 ("negative
  finding is an acceptable M4.4 outcome"). M4 encoder surface
  for `v0.4-rc1` is the M2.2 greedy encoder + M3.3 phase
  rotation, unchanged at M4.
- M5+ scope informal note: improving the high-noise cluster
  needs either source-PCM preprocessing (currently forbidden
  by SPEC §16.9 atom stability + M4.5-deferred pre-emphasis)
  or BRR-spec extension into shift 13+. Neither is M4 scope.

**Next pass: M4.6 — GUI polish.** Unconditional per SPEC §24.1
(independent of research-spike outcomes). M4.5 is permanently
SKIPPED per M4.2 outcome 3. PM to brief.

**Previous milestone (M4.3) — BRR noise-floor measurement.**
Contracted implementation pass per SPEC §24.1 (not a
research-spike). Wires the four SPEC §10.10 noise-floor metrics
(`peak_abs_raw_vs_source`, `rms_raw_vs_source`, `snr_db`,
`clipping_count_raw`) through the `render_to_brr` path and
locks documentary baselines for all 11 atom fixtures + the 9
`m3_5_canonical` characterization signals. No encoder change;
no phase-rotation change; no atom render formula change.

**Outcomes:**

- **Metrics wired** (commit `ddc35ab`). `AtomBrrOutput` and
  `AtomRenderReport` each gain four `#[serde(default)]`
  fields; `render_to_brr` populates them after the M3.3 lex
  rotation picks `best`. Comparison source is `best.rotated_source`
  (the encoder INPUT per SPEC §10.7), not the pre-rotation
  original. `cmd_render_atom`'s JSON output propagates the
  fields.
- **Atom fixture coverage** (commit `488751d`). Three new
  hard tests in `core/tests/atom_edge_cases.rs`:
  `m4_3_atom_fixture_noise_floor_metrics_deterministic` (two
  renders, bit-identical including `f64::to_bits()`),
  `m4_3_atom_fixture_noise_floor_metrics_finite` (sanity
  invariants), and `m4_3_atom_fixture_noise_floor_baselines_pinned`
  (M2.8.1 `include_str!` pattern; reads
  `baselines/m4.json::documentary_snapshot::M4_3_ATOM_*` and
  asserts byte-exact match). Plus one `#[ignore]` print helper
  per file (`atom_edge_cases.rs` + `characterize_gaussian.rs`)
  for one-off baseline capture.
- **Baselines locked** (commit `4edae98`). 80 new
  `documentary_snapshot` entries in `baselines/m4.json`:
  44 `M4_3_ATOM_<NAME>_<METRIC>` (11 fixtures × 4 metrics)
  + 36 `M4_3_CHARSIG_<NAME>_<METRIC>` (9 signals × 4
  metrics).

### M4.3 atom-fixture noise-floor table (11 fixtures)

| Fixture | peak_abs | rms | snr_db | clip |
|---|---|---|---|---|
| 128_SINE | 9582 | 4795.19 | **11.18** | 0 |
| 64_SINE | 10239 | 5108.55 | 10.63 | 0 |
| AMPLITUDE_ZERO | 0 | 0.00 | 0.00 | 0 |
| ALL_PARTIALS_ZERO_NORMALIZE_TRUE | 0 | 0.00 | 0.00 | 0 |
| TWO_PARTIALS_CANCEL_PARTIALLY | 10239 | 1532.33 | **13.36** | 0 |
| MAX_AMPLITUDE_NO_NORMALIZE | 18431 | 10576.55 | 6.40 | 0 |
| NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY | 18431 | 10562.45 | 6.33 | 0 |
| HARMONIC_16_CYCLE_64 | 18431 | 12329.89 | **5.48** | 0 |
| ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8 | 18431 | 4574.39 | 7.41 | 0 |
| PHASE_CYCLES_0_9999 | 9581 | 4797.03 | 11.18 | 0 |
| CYCLE_256_CANONICAL_SINE | 9995 | 4920.93 | 10.96 | 0 |

### M4.3 characterization-signal noise-floor table (9 signals)

| Signal | peak_abs | rms | snr_db | clip |
|---|---|---|---|---|
| sine_cycle_64 | 18431 | 10442.46 | 6.92 | 0 |
| sine_cycle_128 | 18431 | 10439.41 | 6.92 | 0 |
| sine_cycle_256 | 18328 | 10353.63 | 7.00 | 0 |
| harmonic_2_cycle_64 | 18431 | 10454.40 | 6.91 | 0 |
| harmonic_4_cycle_64 | 18431 | 10459.21 | 6.91 | 0 |
| harmonic_8_cycle_64 | 18431 | 10345.79 | 7.00 | 0 |
| harmonic_16_cycle_64 | 18431 | 12329.89 | **5.48** | 0 |
| all_8_partials_max_amp_harmonics_1_to_8 | 18431 | 4574.39 | 7.41 | 0 |
| normalize_false_multi_partial_clamp_safety | 18431 | 10562.45 | 6.33 | 0 |

### Interpretation (M4.4 scoping input)

**Noise floor is BIMODAL across the atom-fixture set.** Two
distinct clusters:

- **"Low-noise" cluster** (peak ≤ 10239, SNR > 10 dB): the
  canonical sines (`128_SINE`, `64_SINE`, `CYCLE_256_CANONICAL_SINE`),
  `PHASE_CYCLES_0_9999` (a slightly-phase-shifted canonical
  sine), and the special-case-zero fixtures
  (`AMPLITUDE_ZERO`, `ALL_PARTIALS_ZERO_NORMALIZE_TRUE` — both
  literally silent). Plus `TWO_PARTIALS_CANCEL_PARTIALLY` —
  the FP-noise-amplification fixture, where RMS is 1532
  because the post-normalize signal IS FP noise dominated by
  the dominant FFT/sin-error component, so most of the BRR
  "error" is encoding tiny non-musical values.
- **"High-noise" cluster** (peak = 18431, SNR 5.5–7.4 dB):
  `MAX_AMPLITUDE_NO_NORMALIZE`, `NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY`,
  `HARMONIC_16_CYCLE_64`, `ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8`.
  The `peak_abs = 18431` ceiling matches the M3.5.1 finding;
  it appears to be a structural ADPCM limit at the
  full-scale-input edge.

**Across the characterization-signal set** the noise floor is
**near-uniform**: 8 of 9 signals at peak = 18431, SNR 6.3–7.0
dB. `harmonic_16_cycle_64` is the lone outlier with SNR 5.48
dB — consistent with near-Nyquist content being the hardest
case for any prediction-based encoder.

**No clipping** (`clipping_count_raw = 0` across all 20
fixtures + signals). Host BRR decode stays within ±16384,
well below i16 saturation.

**M4.4 scope recommendation.** The bimodal split suggests
**M4.4 should target the high-noise cluster specifically** —
those are where the 10% improvement gate is easiest to clear
and where any improvement matters most for real music
content (clamped / max-amp / dense-harmonic / near-Nyquist
fixtures). The low-noise fixtures already encode near-optimum
at the current shift+filter selection; aggressive
optimization there risks regression for sub-LSB improvement.

The M4.4 spike's most promising candidates:
- **`HARMONIC_16_CYCLE_64`** (SNR 5.48 dB, the worst). Near
  the structural limit at peak=18431 = `9 * 2048 + 1023` ≈
  half BRR full-scale; suggests filter-2/3 prediction
  exhausted its dynamic range. Cross-block beam search
  (deferred from M3.4) may find a better shift+filter
  trajectory.
- **`MAX_AMPLITUDE_NO_NORMALIZE` + `NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY`**
  (SNR 6.33–6.40 dB). High-RMS-error fixtures where the
  source itself clips the i16 range during render; encoder
  has no room to improve the peak but might cut RMS.
- **`ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8`** (SNR 7.41 dB,
  RMS 4574). High peak but low RMS — the encoder is doing
  well on average but spiking at certain samples.

### M4.3 phase log

- **Phase A (commit `ddc35ab`)** — wire 4 noise-floor metrics
  through `core::atom::render_to_brr`; same fields on
  `AtomRenderReport` with `#[serde(default)]`. CLI
  `atom_render_report` propagates.
- **Phase B (test commit `488751d`)** — 3 new hard tests
  (determinism, finite, baseline-pin) + 1 ignored print
  helper for atom fixtures.
- **Phase C (test commit `488751d`, baselines commit
  `4edae98`)** — 1 ignored print helper for characterization
  signals + 80 `M4_3_*` documentary baselines.
- **Phase D (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **607 tests
  workspace-wide** (was 604 at M4.2 close; +3 from the
  three new hard tests; 2 new `#[ignore]` print helpers not
  counted).

### Decisions log additions (M4.3)

- 4 M4.0 noise-floor metrics wired through `render_to_brr`
  and `AtomRenderReport`. Comparison source = rotated source
  PCM per SPEC §10.7.
- 11 atom fixtures × 4 metrics = 44 documentary baselines
  locked.
- 9 characterization signals × 4 metrics = 36 documentary
  baselines locked.
- Determinism verified: two-run byte-identity for all 4
  metrics on all 11 atom fixtures (via `f64::to_bits()` for
  rms / snr_db; integer equality for peak / clipping).
- All 11 atom PCM SHA identity tests pass unchanged
  (SPEC §16.9 preserved).
- M3.3 phase-rotation improvement gate test passes unchanged
  (`phase_rotation_loop_click_never_regresses_against_pre_m3`).
- No M2 / M3 baseline changes; no encoder-algorithm changes;
  no SPEC contract changes.
- M4.4 entry recommendation: the noise floor is **bimodal**
  across atom fixtures (low-noise cluster SNR > 10 dB vs
  high-noise cluster SNR 5.5–7.4 dB at peak = 18431) and
  **near-uniform** across characterization signals (8 of 9
  at peak = 18431, SNR 6.3–7.0 dB). Target the high-noise
  atom fixtures first; the structural `peak = 18431`
  ceiling suggests cross-block beam search (M3.4 deferred
  work) is the most likely productive intervention.

**Next pass: M4.4 — Encoder improvement spike (conditional
production).** Research-spike per SPEC §24.1 with exit
criterion (≥10% improvement on at least one fixture's
`rms_raw_vs_source` OR `peak_abs_raw_vs_source` AND no
`loop_click_abs` worsening AND no M2 regression AND encode
runtime ≤ 2× M3.3). A negative finding ("BRR encoder near
local optimum under current constraints") is an acceptable
close. PM to brief.

**Previous milestone (M4.2) — Characterization re-run with
reliable alignment.** Second M4 research-spike per SPEC §24.1
(methodology repair budget loop 2 of 2). Runs the M4.1
alignment plumbing end-to-end against `m3_5_canonical`;
investigates the persistent `zcr_ratio` doubling that
alignment alone couldn't fix; decides M4 trajectory. No
encoder change; no atom PCM change; no M2/M3 baseline change.

> **[M5.1 correction note — read before the archived narrative
> below.]** M5.1 preflight verified that the M2 atom-sequence
> voice path programs `pitch_register == 0x1000` (unity) for
> every M3.5 canonical signal: `core::voice_setup` hardcodes
> `source_sample_rate_hz = 32000` for `TrackKind::AtomSequence`
> (since M2.7); `pitch_register(32000, root, root, 0) = 0x1000`;
> the M2 ASM driver writes those bytes directly to `$F2/$F3`.
> The "DSP pitch register fractionally stepping through input
> samples per output sample" mechanism described in the Phase C
> root-cause paragraph below (and echoed in the M5+ scope notes)
> is **NOT** the actual cause of the M4.2 shape divergence —
> pitch register was already unity. The zcr_ratio ≈ 2 / low-
> correlation pattern has a different origin among: (a) S-DSP
> gaussian 4-tap kernel non-impulse response at unity pitch;
> (b) BRR predictor/loop-state divergence between host raw-tile
> decode and DSP playback. See **SPEC §10.11 motivation (M5.1
> update)** for the corrected framing. M5.2 investigates the
> actual root cause; the archived M4.2 narrative below is
> preserved verbatim for provenance.

**Outcome: Phase D outcome 3 — pre-emphasis defers
permanently to M5+.** All 7 monotonicity-anchor signals fail
at least one of the four SPEC §10.9 reliable-alignment
criteria (criterion 4, `gain_separator_ok`, fails universally
across all 9 signals including `harmonic_16_cycle_64` which
otherwise passes ZCR + correlation). Phase C investigation
identified the cause as **intrinsic to the SPC playback
pipeline**, not a fixable methodology bug. M4.2.1 correction
budget was NOT burned because no clear-cause fix exists for
this characterization comparison. **M4.5 conditional
pre-emphasis evaluation is now SKIPPED.** M4 proceeds to M4.3
BRR noise-floor measurement without expecting
characterization-driven decisions.

### M4.2 characterization data (locked as documentary)

```
schema_version: 4
alignment_search_limit: 256
alignment_boundary_hit: false
alignment_valid: false
methodology_precondition_passed: false
recommended_next: methodology_review
runtime: 0.566 s (release build, 9 signals)
```

| Signal | f (Hz) | align_off | corr | zcr_ratio | gain_db | peak_err | peak_after_norm | validity |
|---|---|---|---|---|---|---|---|---|
| sine_cycle_64 | 500 | 55 | 0.153 | 1.93 | +2.657 | 39053 | 33080 | fail (zcr, corr, gain) |
| sine_cycle_128 | 250 | 55 | 0.147 | 1.93 | +2.675 | 39053 | 33043 | fail (zcr, corr, gain) |
| sine_cycle_256 | 125 | 55 | 0.117 | 1.94 | +2.679 | 39053 | 33034 | fail (zcr, corr, gain) |
| harmonic_2_cycle_64 | 1000 | 55 | 0.274 | 2.58 | +2.589 | 39053 | 33210 | fail (zcr, corr, gain) |
| harmonic_4_cycle_64 | 2000 | 7 | 0.492 | 2.20 | +2.334 | 39053 | 33710 | fail (zcr, corr, gain) |
| harmonic_8_cycle_64 | 4000 | 7 | 0.598 | 2.06 | +1.593 | 39053 | 35253 | fail (zcr, corr, gain) |
| harmonic_16_cycle_64 | 8000 | 63 | **0.984** | **1.00** | -1.223 | 16384 | 16384 | fail (gain only) |
| all_8_partials | 250 | 40 | 0.613 | 0.97 | +2.534 | 35493 | 30107 | fail (corr, gain) |
| normalize_false_clamp | 250 | 40 | 0.597 | 1.55 | +2.456 | 39053 | 33463 | fail (zcr, corr, gain) |

**Pass/fail tallies across the 7 anchors:**
- `zcr_in_band ∈ [0.9, 1.1]`: 1/7 pass (`harmonic_16` only).
- `correlation ≥ 0.90`: 1/7 pass (`harmonic_16` only).
- `offset_in_range < 256`: 7/7 pass.
- `gain_separator ≤ 80% of peak_err`: 0/7 pass.

74 new `M4_2_GAUSSIAN_*` documentary_snapshot entries in
`baselines/m4.json` (8 fields × 9 signals + 2 summary entries).

### Phase C zcr_ratio doubling investigation

Three concrete hypotheses tested.

- **Test 1 — sample-rate doubling.** RULED OUT.
  `raw_pcm_length == oracle_pcm_length == 16000 mono samples`
  for every signal. Oracle invocation confirms
  `sample_rate_hz: 32000, channels: 2, frames_rendered: 16000`
  per the oracle-side report.
- **Test 2 — waveform shape inspection.** ROOT CAUSE FOUND.
  Dumped oracle output for `sine_cycle_128` to
  `build/m4.2-zcr-debug/sine_cycle_128_first200.txt`
  (gitignored). Findings:
  - First **568 samples are pure silence** (SPC startup
    transient — the M2 driver's key-on cycle takes that long
    to drive the voice's GAIN register up to the configured
    127 byte).
  - Once the voice plays, the waveform is **not a sine**.
    It's an alternating-amplitude plateau pattern: ~10
    samples flat at +19853, descent through 0 to flat at
    -22182, **discontinuous jump** to +22669, ~40 samples
    flat at +22669, descent through 0 to flat at -19341,
    discontinuous jump back to +19853, repeat.
  - Each discontinuous jump from negative to positive (or
    vice versa) crosses zero, adding **2 zero crossings per
    fundamental cycle beyond a clean sine** → `zcr_ratio ≈ 2`
    for the low/mid-frequency anchors.
- **Test 3 — oracle invocation review.** RULED OUT.
  `cmd_characterize_gaussian` spawns the oracle with
  `--frames`, `--input-spc`, `--output-pcm`, `--report` —
  no sample-rate or channel flags that could alter output
  shape. The oracle's own report confirms 32 kHz stereo
  s16le at the requested frame count.

**Root cause identified.** The shape divergence is intrinsic
to the SPC playback path. The atom is a 128-sample cycle
played at MIDI 60 (`= 261.63 Hz` fundamental); the atom's
native sample rate for `cycle_len = 128` is
`128 × 261.63 ≈ 33489 Hz`. The SPC plays at the project's
32 kHz master rate, so the DSP pitch register fractionally
steps through input samples per output sample. Combined with
BRR quantization (15-bit decode range ±16384) and gaussian
4-tap interpolation, this produces the alternating-amplitude
shape — the gaussian kernel's center-weight overshoots when
the fractional accumulator lands near a BRR-decoded sample's
peak, then averages down when between samples.

**Why M4.2.1 correction was NOT burned.** A "clear-cause fix"
in the consultant-#16 sense would be an oracle-side or
methodology-pipeline bug. The discovered cause is a
methodological mismatch: the characterization compares
"raw BRR decode tile-repeated at 1:1 sample alignment"
against "SPC pitch-shifted DSP-gaussian playback". These are
**different physical processes**; the shape divergence is
real, not artifact. A cleaner methodology would align the
project sample rate with each atom's native rate (so pitch
register = `0x1000`, no fractional stepping), but that's a
fundamentally different characterization design — far beyond
M4.2.1 scope. Per consultant M4 plan #16, the M4.2.1 budget
is reserved for clear-cause fixes; this finding doesn't
qualify.

### M4.2 phase log

- **Phase A** — fresh release-build characterization run.
  Captured full schema v4 report at
  `build/m3/characterize_gaussian.json` (gitignored). 9
  signals processed in 0.566 s.
- **Phase B (commit `d8e74cd`)** — `baselines/m4.json` gains
  74 `M4_2_*` documentary_snapshot entries + summary +
  Phase C root-cause record.
- **Phase C** — three hypothesis tests run on
  `sine_cycle_128`. Test 1 + Test 3 ruled out. Test 2
  identified intrinsic SPC-playback artifact. Diagnostic
  sample dump at `build/m4.2-zcr-debug/` (gitignored).
- **Phase D** — applied the SPEC §10.9 four-criterion
  validity predicate to all 7 anchors. 0/7 satisfy all four
  criteria; criterion 4 (`gain_separator_ok`) fails on all
  9 signals. Decision: outcome 3 — pre-emphasis defers
  permanently to M5+.
- **Phase E (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **604 tests
  workspace-wide** (unchanged from M4.1; M4.2 is data + docs
  only, no new code).

### Decisions log additions (M4.2)

- M4.2 characterization re-run executed under M4.1's expanded
  alignment search range. 0/9 signals satisfy the
  reliable-alignment predicate; 1/7 anchors satisfy 3 of 4
  criteria (`harmonic_16_cycle_64` — fails only criterion 4).
- Phase C zcr_ratio investigation: 3 hypotheses tested in 1
  hour combined. Test 1 (sample-rate doubling) + Test 3
  (oracle invocation) ruled out. Test 2 (waveform shape)
  identified the cause as intrinsic SPC playback artifact
  (BRR quant + non-native-pitch fractional stepping +
  gaussian) — not a methodology bug.
- **Decision: outcome 3 per Phase D.** Pre-emphasis preset
  implementation (M4.5) defers permanently to M5+ per
  consultant M4 plan #16. M4.5 will be SKIPPED at its time.
- M4.2.1 correction iteration was **not burned**: the
  Phase C finding is a methodological-design issue, not a
  clear-cause bug fixable inside the 2-loop repair budget.
  Per SPEC §24.1, M4 proceeds to M4.3 (BRR noise-floor) and
  M4.6 (GUI polish) without expecting characterization-driven
  decisions.
- All 11 atom PCM SHA identity tests pass unchanged
  (SPEC §16.9 atom PCM stability preserved).
- All M2 / M3 identity / behavior baselines unchanged.
- M5+ scope: revisit the characterization methodology by
  aligning project sample rate with each atom's native rate
  (eliminates pitch-register fractional stepping). Outside
  M4 scope per SPEC §24.

**Next pass: M4.3 — BRR noise-floor measurement.** Contracted
implementation pass per SPEC §24.1. Wires the four SPEC §10.10
noise-floor metric helpers (locked at M4.0, formula-pinned in
`core/tests/brr_noise_floor_metric.rs`) through the
`render_to_brr` path. Populates per-fixture documentary
snapshots. No decision criterion — pure measurement layer.
M4.4 acts on the data. PM to brief.

**Previous milestone (M4.1) — Alignment search range
expansion.** First M4 research-spike per SPEC §24.1.
Mechanically lands the alignment-search fix locked at M4.0
(SPEC §10.9 amendment): `align_oracle_to_raw` now uses
per-signal `max_offset = signal.atom.cycle_len_samples` (was
hard-coded to 32 at M3.5.1). Plus the schema v4 alignment
fields and the reliable-alignment validity predicate. No
encoder change; no atom PCM change; no M2/M3 baseline change.

**Outcomes:**

- **Per-signal `max_offset` plumbing** (commit `42ed122`).
  `finalize_measurement` passes
  `signal.atom.cycle_len_samples` as the alignment search
  range. For `m3_5_canonical`: 64 / 128 / 256 across the
  signals; effective max = 256 vs M3.5.1's flat 32.
- **Schema v4 alignment fields populated** (commits
  `7a032ee` for the core types + helpers, `6b3bfac` for the
  CLI wiring). Four top-level fields:
  `alignment_search_limit`, `alignment_boundary_hit`,
  `alignment_valid`, `methodology_precondition_passed`.
  Per-measurement `alignment_validity: AlignmentValidity`
  surfaces the four-criterion breakdown for debuggability.
- **Reliable-alignment validity predicate** (commit
  `7a032ee`). `is_alignment_reliable_for_signal(measurement,
  search_limit) -> AlignmentValidity`,
  `compute_alignment_valid_for_report` (AND across the seven
  anchor signals), `compute_alignment_boundary_hit` (any
  anchor within 4 samples of the limit). Constants locked
  at M4.0: `RELIABLE_ALIGNMENT_CORRELATION_THRESHOLD = 0.90`,
  `RELIABLE_ALIGNMENT_GAIN_SEPARATOR_THRESHOLD = 0.80`,
  `RELIABLE_ALIGNMENT_BOUNDARY_TOLERANCE = 4`.
- **11 fixture tests** (commit `7530eda`). Synthesized-signal
  cases: alignment finds 64-sample shift / 128-sample shift /
  zero offset; validity predicate fails each of the four
  criteria independently; silent signal accepts; aggregate
  predicate requires all anchors. Workspace test count
  593 → 604.

### M4.1 informal characterization run (Phase 6)

Re-ran `sfcwc characterize-gaussian` against the M4.1
implementation; **outcome and runtime captured below**.
Baselines NOT updated — that's M4.2's deliverable. This is
informational only.

**Runtime:** `0.566 s` wall-clock for the full 9-signal
characterization (release build). Search-range expansion
from 32 → 256 (8× more candidates per alignment call) is
not a bottleneck.

**Top-level outcome:** `recommended_next = "methodology_review"`
(precondition #0 still fires). `alignment_valid` would be
`false` for the same reason — `zcr_ratio` remains anomalous
for most anchor signals.

**Anchor signal cluster shift** (M3.5.1 → M4.1):

| Signal | align_off | corr | zcr_ratio |
|---|---|---|---|
| sine_cycle_64 | 13 → **55** | 0.056 → 0.153 | 1.93 → 1.93 |
| sine_cycle_128 | 11 → **55** | 0.024 → 0.147 | 1.93 → 1.93 |
| sine_cycle_256 | 32 → **55** | 0.013 → 0.117 | 1.94 → 1.94 |
| harmonic_2_cycle_64 | 23 → **55** | 0.261 → 0.274 | 2.57 → 2.58 |
| harmonic_4_cycle_64 | 7 → 7 | 0.492 → 0.492 | 2.20 → 2.20 |
| harmonic_8_cycle_64 | 7 → 7 | 0.598 → 0.598 | 2.06 → 2.06 |
| harmonic_16_cycle_64 | 31 → **63** | 0.983 → 0.984 | 1.00 → 1.00 |
| all_8_partials | 25 → 40 | 0.191 → **0.613** | 0.97 → 0.97 |
| normalize_false_clamp | 25 → 40 | 0.568 → 0.597 | 1.54 → 1.55 |

**Engineer's interpretation (informal, M4.2 verifies):**

- Several low-frequency signals (`sine_cycle_64/128/256`,
  `harmonic_2_cycle_64`) now cluster at `align_off = 55`,
  suggesting the **true gaussian + DSP delay is ~55 samples**
  — which M3.5.1's `max_offset = 32` could never reach.
  Worth M4.2 investigating whether 55 is consistent across
  signals or a coincidence.
- `harmonic_16_cycle_64` now resolves at `align_off = 63`
  (cycle_len = 64 → boundary-adjacent within the
  4-sample tolerance). `alignment_boundary_hit` will fire
  for this signal. M4.2 may need to expand the search range
  further for harmonic_16 specifically, or accept that
  near-Nyquist signals have an intrinsic 1-sample-near-boundary
  alignment.
- **Correlations improve modestly but stay well below 0.90
  for low-frequency sines** (0.117–0.274 vs 0.013–0.261).
  Mechanical alignment fix is insufficient on its own — the
  shape divergence between raw BRR decode and oracle render
  persists. M4.2 will need to investigate whether this is
  intrinsic gaussian behavior (real shape difference, not
  alignment artefact) or an additional methodology issue
  (e.g. the host BRR decoder vs the oracle's BRR decoder
  diverge on the same BRR bytes due to predictor-state
  initialization, sample-rate handling, or DSP envelope).
- **`all_8_partials` correlation jumped 0.191 → 0.613** —
  significant improvement; alignment was clearly the
  bottleneck for this signal. Still below 0.90 but on the
  trajectory.
- `zcr_ratio` values are essentially unchanged across the
  board — they measure zero-crossing rate in absolute terms,
  not phase, so alignment doesn't move them. The
  ~2× ZCR doubling on low-frequency signals is a separate
  symptom (possibly oracle sample-rate doubling or DSP
  envelope adding zero crossings).

**Implication for M4.2:** the alignment-search fix alone
does NOT clear the M3.5.1 anomaly. M4.2 will need to either
(a) accept that the remaining anomalies are real DSP
behavior and proceed under that assumption, or (b) burn the
single M4.2.1 correction iteration on investigating the
shape / ZCR-doubling root cause. Per SPEC §24.1, if the
anomaly persists after the M4.2.1 budget, characterization
is declared unreliable and pre-emphasis defers to M5+.

### M4.1 phase log

- **Phase A (commit `42ed122`)** — `finalize_measurement`
  passes per-signal `max_offset = cycle_len_samples` to
  `align_oracle_to_raw`.
- **Phase B + C (commit `7a032ee`)** — Schema v4 top-level
  fields + `AlignmentValidity` per-measurement field +
  `is_alignment_reliable_for_signal` /
  `compute_alignment_valid_for_report` /
  `compute_alignment_boundary_hit` helpers. Constants locked.
- **CLI wiring (commit `6b3bfac`)** — `cmd_characterize_gaussian`
  computes `alignment_search_limit` from the signal set,
  populates the four top-level fields, and emits
  `schema_version: 4`.
- **Phase D (commit `7530eda`)** — 11 fixture tests covering
  alignment search-range expansion and each validity
  criterion independently.
- **Phase 6 (informal run, not committed)** — re-ran
  characterize-gaussian; results captured inline above. No
  baseline update.
- **Phase E (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **604 tests
  workspace-wide** (was 593 at M4.0 close; +11 from Phase D).

### Decisions log additions (M4.1)

- M4.1 research-spike landed mechanically. `align_oracle_to_raw`
  search range = `max_cycle_len_samples` per signal; was 32
  flat. For `m3_5_canonical`: per-signal 64/128/256, effective
  max 256.
- Schema v4 fields populated: `alignment_search_limit`,
  `alignment_boundary_hit`, `alignment_valid`,
  `methodology_precondition_passed`. Plus per-measurement
  `alignment_validity` struct (4 booleans + `all_pass`)
  exposed for debuggability.
- Reliable-alignment validity predicate implemented per
  SPEC §10.9 4-condition contract. The 80% gain-separator
  threshold (`RELIABLE_ALIGNMENT_GAIN_SEPARATOR_THRESHOLD`)
  was an engineer call within the SPEC's "materially lower"
  prose; M4.2 will exercise it and engineer may revisit.
- Informal Phase 6 characterization run: per-signal
  `align_off` clusters shifted (low-frequency anchors
  converging on ~55; `harmonic_16` at boundary-adjacent 63);
  correlations improve modestly but stay below 0.90 for
  canonical sines; `zcr_ratio` essentially unchanged. M4.1
  fix is necessary but not sufficient.
- Runtime is fine: 0.566 s for full 9-signal run, well under
  any concern (consultant stop condition was ~30s).
- M4.1 does NOT update baselines — M4.2's deliverable.

**Next pass: M4.2 — Characterization re-run with reliable
alignment + decision.** Per SPEC §24.1: M4.2 validates whether
M4.1's fix produces reliable measurements (`alignment_valid:
true` for all anchors), records the new measurements in
baselines/m4.json::documentary_snapshot, and decides whether
to proceed to M4.3 (BRR noise-floor) or burn the M4.2.1
correction iteration on the remaining anomaly. Per the M4.1
informal observation, M4.2.1 may be needed. PM to brief.

**Previous milestone (M4.0) — M4 Contracts Freeze.** No
implementation beyond the SPEC §10.10 noise-floor metric
helpers and their fixture-pin tests. Same shape as M2.0 / M3.0:
lock the contracts that M4.1+ sub-passes build against. No
encoder change, no atom PCM change (SPEC §16.9 reaffirmed at
M4 boundary), no M2 / M3 baseline change.

**Outcomes:**

- **SPEC §10.9 — reliable-alignment criteria + search range**
  (commit `961ca2f`). Four conditions (`zcr_ratio ∈ [0.9, 1.1]`,
  `normalized_correlation ≥ 0.90`, `alignment_best_offset <
  alignment_search_limit`, gain-vs-shape separator) must hold
  for every monotonicity-anchor signal. Search range set to
  `max_i(cycle_len_samples_i) = 256` for `m3_5_canonical`;
  supersedes M3.5.1's `max_offset = 32`. Schema bumped to v4
  with four new top-level fields (`alignment_search_limit`,
  `alignment_boundary_hit`, `alignment_valid`,
  `methodology_precondition_passed`); populated at M4.1 / M4.2.
- **SPEC §10.10 — BRR encoder noise-floor metrics**
  (commit `ed989c5`). Four metrics:
  `peak_abs_raw_vs_source` (i32 widened-abs delta),
  `rms_raw_vs_source` (f64 from i64 sum-of-squares),
  `snr_db` (f64; +inf when err_rms < 1e-12; 0.0 when source
  is silent), `clipping_count_raw` (u32; widened-i32
  `|x| ≥ 32767`; counts ±32767 AND -32768). Compared against
  the rotated source (SPEC §10.7), not the pre-rotation
  original.
- **SPEC §10.9 — pre-emphasis pipeline ordering contract**
  (commit `006d3a9`). Locked order:
  `render → pre_emphasis → rotation → encode`. Atom PCM SHA
  refers to the pre-filter render output (identity-gated per
  §16.9). Rotation and noise-floor metrics compare against
  rotated-filtered source when pre-emphasis is active. Schema
  example block updated from v3 to v4.
- **SPEC §24 amendments — M4 research-spike exit criteria +
  baseline shift rules** (commit `8b30174`). Per-spike
  contracts: M4.1 + M4.2 + M4.2.1 methodology repair budget
  (2-loop cap), M4.3 contracted implementation, M4.4 encoder
  improvement spike (≥10% rms-or-peak gate + no regressions
  + ≤2× encode-runtime ceiling), M4.5 conditional
  pre-emphasis (skipped if M4.2 yields unreliable
  measurements), M4.6 GUI polish (unconditional), M4.7
  acceptance + release. Baseline shift rules: must-not-shift
  M1/M2 + atom PCM SHAs + M2 gates; expected-to-shift
  conditional on M4.4 (BRR/decoded-BRR SHAs, loop_click,
  rms/peak metrics, gaussian characterization snapshots).
  M2.8.1 identity-gated rule carried forward.
- **`baselines/m4.json` scaffolded** (commit `beb1412`).
  Six behavior_gated contract entries (alignment criteria,
  search range, noise-floor metric names, encoder spike
  exit criterion, methodology repair budget, pre-emphasis
  pipeline order). identity_gated and documentary_snapshot
  empty until M4.1+ populates. `inherits_m3: true`.
- **Noise-floor metric helpers + fixture-pin tests**
  (commits `0848d84`, `5513108`). Four public functions in
  `core::audition` (same module as the M3.0 loop-click
  helpers so the encoder-independent measurement surface
  stays in one place). 14 fixture tests on hand-constructed
  PCM vectors pin the formulas BEFORE M4.3 applies them to
  atoms (M3.0 Phase H pattern — prevents the circular-
  validation trap where M4.4 encoder changes retroactively
  tweak the metric to look better).

### M4.0 phase log

- **Phase A (commit `961ca2f`)** — SPEC §10.9 reliable-alignment
  criteria + search range. Schema v3 → v4.
- **Phase B (commit `ed989c5`)** — SPEC §10.10 BRR noise-floor
  metrics. Four metric definitions + comparison source +
  M4.4 exit criterion.
- **Phase C (commit `006d3a9`)** — SPEC §10.9 pre-emphasis
  pipeline ordering contract. Render → pre_emphasis →
  rotation → encode. Atom PCM SHA pinned to pre-filter
  render.
- **Phase D + E (commit `8b30174`)** — SPEC §24.1
  research-spike exit criteria + §24.2 baseline shift rules.
  Six sub-pass contracts. Must-not-shift / expected-to-shift
  classification.
- **Phase F (commit `beb1412`)** — `baselines/m4.json`
  scaffold. Six behavior_gated contract entries; inherits_m3.
- **Phase G (commits `0848d84`, `5513108`)** — Four pure
  metric functions in `core::audition`; 14 fixture tests in
  `core/tests/brr_noise_floor_metric.rs`.
- **Phase H (this entry)** — STATUS rewrite.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **593 tests
  workspace-wide** (was 579 at M3.8.1 close; +14 from the
  fixture-pin tests).

### Decisions log additions (M4.0)

- M4 entry approved. M3 closed at `v0.3-rc1` (commit
  `7a2329f`) with `579 tests` verified at M3.8.1 audit; the
  consultant brief's "595 tests" claim was a PM-side
  miscount, not a docs drift.
- M4.0 contracts frozen per consultant M4 plan #5–#21:
  reliable-alignment criteria (4 conditions), alignment
  search range (= max_cycle_len), characterization signal
  set (same 9 as M3.5), BRR noise-floor metrics (peak / rms
  / snr_db / clipping_count_raw), pre-emphasis ordering
  (render → pre_emphasis → rotation → encode),
  research-spike hard-caps (2-loop methodology budget,
  10% encoder improvement gate), baseline shift rules.
- `baselines/m4.json` scaffolded; inherits M3 by reference.
- M4 identity-gated baseline rule carried from M2.8.1: every
  new `identity_gated` entry ships with an `include_str!` +
  `serde-parse` + `assert_eq!` test asserting the value.
  M4.7 `m4-acceptance` stage 5 will enforce at runtime.
- Research-spike vs implementation-pass split (consultant
  M4 plan #4): M4.1, M4.4, M4.5 are research-spikes with
  exit criteria; M4.0, M4.2, M4.3, M4.6, M4.7 are
  contracted-implementation passes. Spike passes may exit
  with "no production change" and still close successfully.
- BRR noise-floor metric formulas fixture-pinned at M4.0
  (independent of encoder, 14 tests); M4.3 wires them through
  the atom encode path and the gaussian characterization
  report.
- M4 sub-pass plan: M4.1 alignment fix, M4.2 characterization
  re-run, M4.2.1 conditional correction, M4.3 noise-floor
  metric wiring, M4.4 encoder improvement spike (conditional
  production), M4.5 pre-emphasis evaluation (conditional;
  only if M4.2 valid), M4.6 GUI polish, M4.7 acceptance +
  release.
- Release tag policy: `v0.4-rc1` only after final M4.7 close
  + integrity audit per M2 / M3 lessons.

### Spec ambiguity flagged

The brief's stop-condition resolution for
`clipping_count_raw` proposed `x == i16::MAX || x ==
i16::MIN` to dodge the `i16::MIN.abs()` overflow. That
definition would NOT count `-32767` (since `i16::MIN` is
`-32768`), conflicting with the brief's own test expectation
of count = 3 for `[32767, -32767, 32766, 0, 32767]`. SPEC
§10.10 and the implementation adopt the widened-i32 form
(`(x as i32).abs() >= 32767`) which matches the test
contract and counts `±32767` AND `-32768`. Reported in the
Phase B commit message; PM may want to confirm SPEC §10.10
matches intent.

**Next pass: M4.1 — Alignment search range expansion +
reliable-alignment criteria implementation.** Research-spike
with exit criteria from SPEC §24.1. PM to brief.

**Previous milestone (M3.8.1) — Release-final test-count
reconciliation. M3 closed at `v0.3-rc1`.** Documentation-only
pass. Consultant M3 close-out audit flagged a single
block-M3-close item: PM's pre-audit summary claimed "595
tests workspace-wide" while every release artifact (STATUS,
`RELEASE_NOTES_v0.3-rc.md`, `docs/reproduce-m2.md`)
consistently records 579. The 595 figure was a PM error in
the consultant brief, not a real claim from any release
artifact.

**M3.8.1 audit verification (Phase 1):** ran
`cargo test --workspace` against `main` at the v0.3-rc1
commit. Captured output to `build/m3.8.1-test-count.txt`
(gitignored). Tally across 15 test binaries:

```
passed=579 failed=0 ignored=4
```

`579` matches the figure in `STATUS.md`,
`RELEASE_NOTES_v0.3-rc.md`, and `docs/reproduce-m2.md`
byte-for-byte. **Phase 2A applies: no patch, no rc2 retag.**
`v0.3-rc1` stays as the canonical M3 release-candidate tag.

**Consultant M3 close-out audit signed off** on technical
substance; the 18 Acceptable confirmations are recorded as
such. Single block finding (test-count reconciliation)
resolved as a PM-side miscount, not a docs drift.

**M3 is officially closed at `v0.3-rc1`** (annotated tag,
commit `7a2329f`).

**M4 entry ordering** (per consultant close-out audit #19;
forward visibility, not a commitment):

1. **Gaussian alignment search expansion.** Resolve the
   `align_oracle_to_raw` `max_offset = 32` limit so cycle
   lengths up to 256 align deterministically. Pre-emphasis
   decisions depend on trustworthy measurement — alignment
   first.
2. **Re-run characterization** with reliable alignment.
   Precondition #0 (`zcr_ratio ∈ [0.9, 1.1]` for
   monotonicity anchors) becomes evaluable.
3. **BRR encoder noise floor reduction.**
   `peak_abs_raw_vs_source ≈ 18431` LSBs is the dominant
   atom-render artefact per M3.5.1; M4 may investigate
   per-block filter refinements / predictor optimization
   (the M3.4-deferred work).
4. **Conditional pre-emphasis presets.** Only if items 1–3
   yield a clear frequency-response target the M3.5.1
   precondition + four-condition rule can clear.
5. **GUI / schema polish** (`rename_track_id_cascade` etc.,
   currently unnecessary; revisit if schema grows track-id
   references).
6. **`baselines/m4.json`** (inherits M3 by reference;
   mirrors the M3-inherits-M2 pattern).

PM drafts the M4.0 contracts brief next. Engineer may consult
before scoping, or proceed directly when briefed.

### Decisions log additions (M3.8.1)

- **M3.8.1 audit** (consultant M3 close-out audit, single
  block finding): release docs test count "579 tests
  workspace-wide" verified against `cargo test --workspace`
  runner output at the `v0.3-rc1` commit (`7a2329f`). PM's
  pre-audit summary claim of "595 tests" was a PM error in
  the consultant brief; not present in any release artifact.
  `v0.3-rc1` stays as the canonical tag; no rc2 required.
  No code changes; no SPEC changes; no baseline changes.
- Consultant M3 close-out audit signed off on technical
  substance; M3 officially closed at `v0.3-rc1`.
- M4 entry ordering recommendation recorded (alignment first,
  then characterization re-run, BRR noise, conditional
  presets, polish, `baselines/m4.json`).

**Previous milestone (M3.8) — M3 release prep + acceptance +
tag `v0.3-rc1`.** Final
M3 sub-pass. Mirrors M2.8 in structure but with smaller
scope — most release-prep patterns (literal SHA pins, STATUS
split, machine-readable baselines) were established at M2.8
and applied proactively through M3.0–M3.7. No encoder change,
no atom PCM change, no driver change, no M2 baseline change.

**Outcomes:**

- **`m3-acceptance` bundle CLI** (Phase 1, commit `86fec96`).
  Five-stage rollup analog of `m2-acceptance`: M2 regression
  (subprocess `m2-acceptance`) → atom PCM stability (11
  identity-pin tests) → loop-click improvement gate (post ≤
  pre per fixture) → encoder-quality snapshot
  (post-rotation documentary BRR / decoded-BRR SHAs,
  soft-gate per brief) → baselines integrity audit
  (every identity_gated entry carries a `test:` field).
  `bundle.status = ok` on both
  `fixtures/projects/canonical_m2/canonical_m2.sfcproj.json`
  and
  `fixtures/projects/atom_edge_cases/harmonic_16_cycle_64.sfcproj.json`.
- **Reproducer doc updated for M3** (Phase 2, commit
  `7e1367b`). `docs/reproduce-m2.md` is the unified
  reproducer — Option A per brief; single doc beats two
  separate docs because the actual fresh-clone flow is M2
  then M3. New sections cover `m3-acceptance` invocation,
  `characterize-gaussian` invocation + expected
  `methodology_review` outcome, and prelude audition WAV
  regeneration. Test count updated 521 → 579.
- **`RELEASE_NOTES_v0.3-rc.md` shipped** (Phase 3, commit
  `3f873a6`). Highlights covering M3.0–M3.8, locked-baseline
  summary, the M3.6-deferred-to-M4 methodology note per
  consultant M3.5 audit #17, and forward M4 prelude scope.
- **M3 baseline classification audit complete** (Phase 4,
  commit `b3255b5`). 11/11 `identity_gated` entries verified
  with literal-pin `include_str!` + `serde-parse` +
  `assert_eq!` tests. One `behavior_gated` gap patched:
  `M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE` now carries
  a `test:` field pointing at
  `phase_rotation_loop_click_never_regresses_against_pre_m3`.
  The three remaining `behavior_gated` entries are policy
  contracts asserted by implementation code (lex comparison,
  candidate-set generator) and follow M2's pattern of
  `test: null` for policy-only entries.
- **M4 prelude scope documented in SPEC §24** (Phase 5,
  commit `bb34938`). Five forward-visibility questions:
  alignment search expansion, BRR encoder noise floor
  reduction, conditional pre-emphasis presets,
  `rename_track_id_cascade`, `baselines/m4.json` creation
  with M3-inherits-M2 pattern.
- **`v0.3-rc1` annotated tag** at the M3.8 close commit
  (Phase 6, this entry; tagged after STATUS push).

**Stack-frame hardening (incidental).** The original
`cmd_m3_acceptance` used a monolithic `serde_json::json!{}`
that pushed the Windows debug `sfcwc` binary past the
1 MiB PE-header default main-thread stack at startup
(reproduced with `target/debug/sfcwc.exe --help` → stack
overflow). Two-pronged fix landed in Phase 1:
extracted bundle JSON construction into a separate
`build_m3_acceptance_bundle_json` helper using explicit
`serde_json::Map::insert` calls, and added
`.cargo/config.toml` linking the `x86_64-pc-windows-msvc`
target with `/STACK:8388608` (8 MiB, matching typical
Linux soft default). Scope: Windows MSVC only.

### M3.8 phase log

- **Phase 1 (commit `86fec96`)** — `sfcwc m3-acceptance`
  subcommand. ~300 lines across `app/src/main.rs`:
  `cmd_m3_acceptance` orchestrator,
  `build_m3_acceptance_bundle_json` helper +
  `M3AcceptanceBundleArgs` struct, `Command::M3Acceptance`
  enum variant + dispatch arm. Plus
  `.cargo/config.toml` Windows MSVC stack-flag.
- **Phase 2 (commit `7e1367b`)** — `docs/reproduce-m2.md`
  extended from "Reproducing M2" to "Reproducing M2 / M3"
  with three new sections (m3-acceptance run, M3.5
  characterization, M3.5 prelude audition WAVs) and
  expanded "Verify locked baselines" + "Reference"
  sections.
- **Phase 3 (commit `3f873a6`)** — `RELEASE_NOTES_v0.3-rc.md`
  (new file, 230 lines). Mirrors `RELEASE_NOTES_v0.2-rc.md`
  shape. Includes the explicit M3.6 deferral note per
  consultant M3.5 audit #17 and the M4 prelude scope.
- **Phase 4 (commit `b3255b5`)** — `baselines/m3.json`:
  added `test:` field to
  `M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE` pointing
  at the existing test. 11/11 identity_gated audit clean.
- **Phase 5 (commit `bb34938`)** — `SPEC.md` §24 M4 prelude
  scope.
- **Phase 6 (this entry)** — STATUS rewrite + `v0.3-rc1`
  annotated tag at the M3.8 close commit.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **579 tests
  workspace-wide** (same as M3.7 close; M3.8 added no new
  test functions — release prep is implementation +
  documentation work).
- **m3-acceptance runtime confirmation (final tag-eve run):**
  ```
  m3-acceptance: project_a=fixtures/projects/canonical_m2/canonical_m2.sfcproj.json
    stage_1_m2_regression: ok
    stage_2_atom_pcm_stability: ok
    stage_3_loop_click_improvement_gate: ok
    stage_4_encoder_quality_snapshot: ok (ok)
    stage_5_baselines_integrity: ok
    bundle.status: ok
  ```
  Both the canonical M2 fixture and the M3.3
  `harmonic_16_cycle_64.sfcproj.json` reproducer fixture
  return `bundle.status = ok` end-to-end.

### Decisions log additions (M3.8)

- `m3-acceptance` bundle shipped; 5-stage rollup (M2
  regression + atom PCM stability + loop-click gate +
  encoder-quality snapshot + baselines integrity).
- Reproducer doc updated for M3 (Option A — single doc).
- `RELEASE_NOTES_v0.3-rc.md` shipped with explicit M3.6
  deferral methodology note per consultant M3.5 audit #17.
- Baseline classification audit complete: 11/11
  identity_gated entries verified with literal-pin tests;
  one behavior_gated `test:` field added.
- M4 prelude scope documented in SPEC §24 (5 forward
  questions).
- `.cargo/config.toml` adds Windows MSVC `/STACK:8388608`
  linker flag to handle the debug-build main-thread stack
  growth from the m3-acceptance code path. Scope: Windows
  MSVC only.
- `v0.3-rc1` annotated tag at the M3.8 close commit.
- **Next pass: M4 prelude.** PM may brief at M4 entry.
  Open questions enumerated in SPEC §24.

**Previous milestone (M3.7) — GUI polish.** Three small,
independent additions that sit cleanly on top of the
M3.5/M3.5.1 methodology audit (deliberately not exposing the
gaussian characterization surface to the GUI per consultant
M3.5 audit #16). No encoder change, no atom PCM change, no
driver change, no SPEC contract change, no M2 baseline change.
Reports-only / GUI-only.

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

**Pass M5.0 — M5 Contracts Freeze (Phases A–H).** Eight commits
covering SPEC §10.11 native-rate characterization (Option α,
no v2 schema change), §10.9 M5 threshold reaffirmation, §10.9
pre-emphasis preset report fields, §24.1.1 M5 methodology
repair budget (1+1 loops), §16.9.1 atom PCM stability
amendment procedure (forward visibility), `baselines/m5.json`
scaffold with 6 behavior_gated entries, pitch-register
fixture-pin test, STATUS rewrite. No encoder change; no
render formula change; no v2 schema change; no M2/M3/M4
baseline change. Workspace test count 615 → 616.

---

**Pass M4.7 — M4 release prep + acceptance + tag `v0.4-rc1`
(Phases 1–6).** Final M4 sub-pass. Six commits: `m4-acceptance`
5-stage bundle CLI, reproducer doc update, release notes with
M4.2 / M4.4 / M4.5 deferral documentation, baseline
classification audit, SPEC §25 M5 prelude scope, STATUS
rewrite. Workspace test count unchanged at 615. `m4-acceptance`
bundle.status = `warn` end-to-end on both fixtures; only
stage 2's `alignment_valid: false` (documented M4.2 outcome 3)
is non-clean. M4 closed at `v0.4-rc1`.

---

**Pass M4.6 — GUI polish + M4.4 arithmetic wording patch
(Phases 0, A–D).** Small two-layer pass. Phase 0
(`392dd04`): patched M4.4 "structural ceiling" wording per
consultant M4.4 audit #2 / #4 / #9. Phases A–D
(`726c036`, `d7ec00b`): defensive `rename_track_id_cascade`
landing for symmetry with M2.8 atom + M3.7 sequence cascades.
Workspace test count 610 → 615.

---

**Pass M4.4 — Encoder improvement spike (Phases A–E) — SKIP
outcome.** Research-spike per SPEC §24.1. Three commits: spike
implementation, test infrastructure, skip-path documentary
baselines. Workspace test count 607 → 610. **Decision: skip; no
production change ships.** Two of four exit conditions failed
(≥10% improvement gate and ≤2× runtime ceiling). Investigation
finding: the high-noise cluster's peak=18431 plateau is
explained by the current-sample-term ceiling at shift=12 in
filter-0 / forced-loop-entry cases (M4.6 wording patch
narrowed the original "universal ceiling" claim). Spike
implementation preserved feature-flagged in
`core::brr_encoder` for M5+ reference. Acceptable close per
consultant plan #17.

---

**Pass M4.3 — BRR noise-floor measurement (Phases A–D).**
Contracted implementation. Four commits: noise-floor metrics
wired through `render_to_brr` and `AtomRenderReport`, three new
hard tests + two ignored print helpers, 80 documentary baselines
(`M4_3_ATOM_*` + `M4_3_CHARSIG_*`), STATUS rewrite. Workspace
test count 604 → 607. **Bimodal noise floor surfaced across
atom fixtures** (low-noise SNR > 10 dB vs high-noise SNR
5.5–7.4 dB at peak = 18431); fed directly into M4.4 spike
scope.

---

**Pass M4.2 — Characterization re-run with reliable alignment
(Phases A–E).** Second M4 research-spike. Two commits: 74
`M4_2_*` documentary baselines from the M4.1-aligned
characterization run plus STATUS. Phase C zcr_ratio doubling
investigation identified intrinsic SPC playback (BRR +
non-native pitch + gaussian) — not a fixable methodology bug.
M4.2.1 budget NOT burned. **Outcome 3:** pre-emphasis defers
permanently to M5+; M4.5 will be SKIPPED.

---

**Pass M4.1 — Alignment search range expansion (Phases A–E).**
First M4 research-spike. Five commits: per-signal
`max_offset = cycle_len_samples` plumbing, schema v4
alignment fields + reliable-alignment validity predicate,
CLI wiring for the four top-level alignment fields, 11
fixture tests, STATUS rewrite. Workspace test count 593 → 604.
Informal characterization run captured (unchanged
`recommended_next = methodology_review` — alignment fix is
necessary but not sufficient; M4.2 confirmed).

---

**Pass M4.0 — M4 Contracts Freeze (Phases A–H).** Eight commits
covering SPEC §10.9 reliable-alignment + search-range
amendment (schema v3 → v4), SPEC §10.10 BRR noise-floor
metrics, SPEC §10.9 pre-emphasis ordering, SPEC §24.1
research-spike exit criteria + §24.2 baseline shift rules,
`baselines/m4.json` scaffold, four `core::audition` metric
helpers, 14 fixture-pin tests, STATUS rewrite. No encoder
change; no atom render change; no M2 / M3 baseline change.
Workspace test count 579 → 593.

---

**Pass M3.8.1 — Release-final test-count reconciliation.**
Documentation-only verification per consultant M3 close-out
audit's single block finding. `cargo test --workspace` runner
reports 579 passed / 0 failed / 4 ignored across 15 test
binaries at the v0.3-rc1 commit. `v0.3-rc1` (`7a2329f`)
remains the canonical M3 tag.

---

**Pass M3.8 — M3 release prep + acceptance + tag v0.3-rc1
(Phases 1–6).** Final M3 sub-pass: `m3-acceptance` 5-stage
bundle CLI, updated reproducer doc, release notes with M3.6
deferral methodology note, baseline classification audit, SPEC
§24 M4 prelude scope, and the `v0.3-rc1` annotated tag.

---

**Pass M3.7 — GUI polish (Phases A–D).** Three independent
additions: sequence-id rename cascade with reference updates,
atom preview metric readout surfacing M3.1 / M3.3 fields, plus
tests + STATUS. No encoder / SPEC / baseline change.

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
