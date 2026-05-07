# v0.2-rc — M2 release candidate

This is the first release candidate covering the M2 multi-voice
atom pipeline. M0 through M2.8 deliverables are complete; the M3
BRR encoder quality pass is the next planned milestone.

The release is recorded as `release: v0.2-rc` in
`baselines/m2.json` and tracks the M2.8 close commit on the
`main` branch.

## Highlights

- **SPC700 multi_voice_atom driver** (1158 bytes, well under the
  4 KiB budget) implementing SPEC §20.2: T0 timer at 60.150 Hz
  nominal, voice-setup-table walker, full SEQ2 opcode interpreter,
  integer-Bresenham slide arithmetic, M2 status-flags 8-bit map.
  KOFF latch clear before KON in `op_kon` (M2.5 fix). M2.8
  consultant alignment landed the SPEC §14.3
  wait-decrement-before-opcode-read semantic across spec /
  driver / sequence compiler.
- **v2 project schema** with atoms (single-cycle additive
  synthesis), atom_sequences (declarative source-step
  transitions), tracks, and a capability manifest with
  dependency validation at compile time.
- **`compile-spc` and `compile-sfc` dispatch** by schema version
  + driver profile. v1 / v2-sample-only paths are byte-identical
  to M1 baselines; v2 multi_voice_atom uses the new compile path.
- **Per-channel oracle harness** (`verify-spc-stereo`,
  `verify-sfc-modules-audible` stereo extension) with SPEC §21
  acceptance gates: audibility floors (`max_abs >= 1000`,
  `rms >= 200`), silence ceiling (`max_abs <= 50` on hard-panned
  silent channel), source-step zero-crossing-rate ratio
  (`>= 1.5×`).
- **`m2-acceptance` four-stage bundle aggregator** (validation,
  compile, oracle, infrastructure). Analog of `m1-acceptance`.
- **GUI editor** (egui-based) for v2 atoms / sequences / tracks
  with live validation, profile switching with destructive-clear
  detection, atom audio preview, atom rename cascade with
  reference updates, step reorder/remove auto-normalization.
  Editing surface fully round-trip stable against the JSON path
  through nontrivial mutation cycles.

## What's locked

See `baselines/m2.json` for the full classification. Summary:

- **Identity-gated** (any drift = regression): M1 loader
  (588 bytes, SHA pinned), M1 driver SHA, M2 canonical SEQ2
  bytecode SHA, M2 canonical voice setup table SHA,
  `M2_CANONICAL_SEQUENCE_TOTAL_TICKS = 249`
  (sum-of-WAIT-operands semantic).
- **Behavior-gated**: M1 + M2 audibility floors, silence
  ceilings, source-step zero-crossing ratio, 32 KiB module cap.
- **Documentary snapshot** (expected to shift at M3): M2 driver
  code size (1158 B), atom BRR / PCM SHAs, loop click scores.

`baselines/m2_canonical_fixtures.md` carries the canonical SEQ2
bytecode + voice setup table hex dumps with per-byte breakdowns.

## Schema notes

- `sample_pool` length relaxed from `1..=128` to `0..=128` at
  M2.5. Empty pool is valid for atom-only `multi_voice_atom`
  fixtures and for `sample_basic` projects (silent SPC).
  Compile paths still require sample/track consistency:
  `multi_voice_atom` with empty `sample_pool` requires all
  tracks be `atom_sequence` (rule 57); `sample_basic` projects
  with empty `sample_pool` produce silent output.
- v1 ↔ v2 sample-only-equivalent migration is bit-identity-
  preserving through compile.
- `bytecode_version` auto-syncs with the active driver profile
  (1 for `sample_basic`, 2 for `multi_voice_atom`).
- M2.8 added `total_elapsed_ticks` to `SequenceCompileReport`
  alongside `total_ticks`. The former is the wall-elapsed tick
  count under SPEC §14.3 semantics; the latter is the
  sum-of-WAIT-operands and stays at the M2.4 baseline value
  (canonical fixture: 249 vs 254). Oracle frame-window math
  consumes `total_elapsed_ticks`.

## What's deferred to M3

- BRR encoder quality (phase rotation, predictor optimization,
  pre-emphasis). Atom BRR SHAs in `baselines/m2.json` are
  documentary snapshots and expected to shift.
- Loop-click oracle metric gating (currently only
  baseline-locked, not gated). The canonical atom fixtures have
  audible loop click expected per consultant #9.
- Atom render edge cases beyond the canonical fixture coverage.
- `bytecode_version` profile-version-table (consultant #14).
- Sequence preview in the GUI (would require runtime interpreter
  in the GUI process, forbidden per SPEC §0).

## Reproduction

`docs/reproduce-m2.md` walks fresh-clone-to-acceptance steps.

## Tagging

This release-candidate is recorded as `v0.2-rc` in
`baselines/m2.json::release`. Tag in git when ready to publish:

```bash
git tag v0.2-rc1 <m2.8-close-commit>
git push origin v0.2-rc1
```

PM's call on whether to tag — recommend tagging at the M2.8
close commit so the release is verifiable from-tag.
