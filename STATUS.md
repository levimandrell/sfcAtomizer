# SFC Wave Compiler — Status

## Current milestone

**M2.8.1 — release-final patches before tagging v0.2-rc1.**
Nine small fixes from the consultant's M2 close-out review: the
M2.8 implementation was signed off; the release artifacts
themselves needed narrowing of scope claims, identity-gating of
the new `total_elapsed_ticks` scalar, a literal-pin test for
`M1_DRIVER_CODE_SHA256` (the historic value had drifted
unobserved post-M2.0), a committed canonical fixture under
`fixtures/projects/canonical_m2/` so the reproducer guide isn't
hand-waving, asar-version-claim narrowing in the reproducer, and
prose hygiene on the release notes (PCM vs BRR shift expectations
separated; annotated-tag command). 522 tests workspace-wide
(was 521 at M2.8; +1 from the new
`m1_driver_code_sha_matches_locked_baseline` literal-SHA test).

The v0.2-rc1 git tag points at the M2.8.1 close commit (this
pass). M2 is officially closed once the tag pushes.

**M3 next.** BRR encoder quality (phase rotation, predictor
optimization, pre-emphasis); loop-click oracle metric
implementation (gated before encoder optimization per consultant
M2 close-out #25); atom render edge cases beyond canonical
fixture coverage; the deferred `rename_sequence_id_cascade`
GUI-polish surface (consultant #13). PM to brief at M3 entry.

## Last pass

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

## Previous passes

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
