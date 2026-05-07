# SFC Wave Compiler — Status

## Current milestone

**M2.8 — M2 release prep shipped (v0.2-rc).** Four release-prep
layers covering the consultant's M2.7 review:

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

521 tests workspace-wide (was 505 at M2.7; +16 net delta from
the new pin coverage across Layer 1 and Layer 2).

**M3 next.** BRR encoder quality (phase rotation, predictor
optimization, pre-emphasis); loop-click oracle metric gating;
atom render edge cases beyond canonical fixture coverage. PM to
brief at M3 entry.

## Last pass

**Pass M2.8 — M2 release prep.**

- **Layer 1 (WAIT timing alignment, consultant #1):** SPEC §14.3
  / driver / compiler now agree. `walk_writes_per_tick` and the
  per-step lowering loop both advance by `n + 1` per WAIT — n
  decrement ticks + 1 resume tick where the next opcode_read
  fires. `total_ticks` (sum-of-WAIT-operands, M2.4 baseline) and
  `total_elapsed_ticks` (wall-elapsed under SPEC semantics, M2.8
  addition) reported separately on `SequenceCompileOutput` /
  `SequenceCompileReport`. Slide intervals for the canonical
  fixture: fade-out `[122..126)`, fade-in `[129..133)` (both
  shifted by +1 from the pre-fix windows). SPEC §21 source-step
  pre / post windows updated to the new walker output.
- **Layer 2A (driver-version anchor, consultant #8):** scan for
  the full 12-byte ready-signature pattern
  (`8F A5 F4 / 8F 5A F5 / 8F VV F6 / FA 01 F7`) instead of any
  `8F xx F6` byte triple. Synthetic-fixture regression test
  proves the pre-M2.8 detector would have false-positived; the
  M2.8 anchored detector returns `None`.
- **Layer 2B (v2 SFC source-SHA, consultant #10):**
  `compile_module_v2_multi_voice` now runs
  `check_or_refresh_source_hash` before sample decode. Mismatches
  surface as `SfcExportError::Decode { source: SourceHashMismatch }`;
  refreshes persist back to the project file via
  `ProjectV2::save_to_path` (parity with the v1 path). New
  `cli_compile_sfc_v2_multi_voice_source_hash_mismatch_errors`
  test mutates a WAV after staging and asserts non-zero exit.
- **Layer 2C (GUI step normalize, consultant #11):**
  `V2EditorModel::remove_step` / `move_step_up` / `move_step_down`
  now call `normalize_step_transitions` after every structural
  edit. Step 0 → InitialKon, steps 1+ → FadeToZeroRetrigger
  (preserving existing fade params when normalizing between fade
  slots). Four tests covering remove-step-0, move-up-to-0,
  move-down-from-0, fade-param preservation.
- **Layer 2D (atom rename cascade, consultant #12):**
  `V2EditorModel::rename_atom_id_cascade(idx, new_id)` updates
  the atom's id *and* every
  `atom_sequences[].steps[].atom_id` reference that pointed at
  the old id. Refuses on cross-pool collision (atom vs sample
  per SPEC §16.9 rule 30) or atom-pool self-collision. Four tests.
- **Layer 2E (round-trip nontrivial mutation, consultant #16):**
  `round_trip_parity_after_nontrivial_mutation_sequence` adds an
  atom + tweaks fields + adds a sequence with three steps, saves
  through the editor model, reloads from disk, saves again, and
  asserts byte-identical. Catches edit-session float drift /
  serde ordering drift the M2.7 immediate-construction round-trip
  test missed.
- **Layer 3 (docs reorg, consultants #15, #19, #20, #21, #23, #25,
  #26, #27, #31):** STATUS split into active + archive; canonical
  fixtures extracted; machine-readable baselines/m2.json shipped;
  four prose hygiene patches (SPEC §5.4 GUI wording, STATUS
  slider-snap narration, profile-switch UX nudge, sample_pool
  0..=128 release note).
- **Layer 4 (release proper):** `docs/reproduce-m2.md` +
  `RELEASE_NOTES_v0.2-rc.md`.
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **521 tests
  workspace-wide** (was 505 at M2.7; +16 net delta).

### Decisions log additions (M2.8)

- WAIT timing aligned across SPEC, driver, and compiler walker.
  `total_ticks` keeps M2.4 sum-of-WAIT-operands semantic;
  `total_elapsed_ticks` (new) is the wall-elapsed tick count.
- Driver-version detection anchored on the full
  `8F A5 F4 / 8F 5A F5 / 8F VV F6 / FA 01 F7` ready-signature
  pattern; isolated `8F xx F6` triples no longer false-positive.
- v2 SFC compile path enforces source-SHA refresh / mismatch
  via `check_or_refresh_source_hash` (parity with v1).
- GUI step reorder / remove auto-normalizes step transitions to
  satisfy SPEC §16.9 rules 47-48; structural edits no longer
  leave the project in an unsaveable state.
- New `rename_atom_id_cascade` method updates atom id + all
  step references in lockstep; the GUI rename UI uses cascade by
  default. The raw `set_atom_id` setter (no cascade) is reserved
  for tests / migrations / "I know what I'm doing" callers.
- Round-trip parity test extended with a non-trivial mutation
  cycle (add atom + tweak fields, add sequence + steps, save,
  reload, save again, assert byte-identical).
- STATUS split into active (`STATUS.md`, ~150 lines) + archive
  (`docs/history/M0-M2-passes.md`, ~2.6k lines).
- Canonical M2 SEQ2 bytecode + voice setup table fixtures
  extracted from STATUS to `baselines/m2_canonical_fixtures.md`.
- Machine-readable baselines shipped at `baselines/m2.json` with
  identity-gated / behavior-gated / documentary-snapshot / retired
  classification.
- SPEC §5.4 manifest-enforcement wording softened — GUI editor
  enforces profile / atom-data consistency through schema
  validation; the compile-time capability check is the source
  of truth.
- STATUS slider-snap narration corrected: model setters snap;
  direct `model.project.<field>` mutation does not snap and is
  reserved for tests / migrations.
- Profile-switch UX nudges the user when `multi_voice_atom` is
  selected with no atom_sequence track yet.
- Reproducer guide shipped (`docs/reproduce-m2.md`) plus
  release-candidate notes (`RELEASE_NOTES_v0.2-rc.md`).

## Previous passes

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
M1_DRIVER_CODE_SHA256                 = 671ee21e...4b2bcfe
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
