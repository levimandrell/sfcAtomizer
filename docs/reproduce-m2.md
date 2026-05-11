# Reproducing the M2 / M3 acceptance pipeline

This guide takes a fresh clone of the repository to a passing
`m2-acceptance` and `m3-acceptance` bundle. It exercises the
full M2 pipeline (sample encoding, atom rendering, sequence
compilation, M2 driver assembly, `.spc` / `.sfc` generation,
per-channel oracle gates, `m2-acceptance` aggregator) plus the
M3 release surface (atom PCM stability, loop-click improvement
gate, post-rotation documentary snapshots, baselines integrity
audit, gaussian characterization).

Tested on Windows 11 / msys2 bash; the same commands work on
Linux and macOS with the obvious path-separator adjustments.

## Prerequisites

- **Rust toolchain**: stable channel, pinned via
  `rust-toolchain.toml` at the repo root. `rustup` will install
  the right toolchain on first `cargo` invocation. No specific
  version pin yet.
- **`asar` SPC700 / 65816 assembler**: tested with 1.91. asar
  1.81+ is expected to work; older versions are unsupported
  unless verified. Build from
  https://github.com/RPGHacker/asar or download a release binary.
  Place on `PATH` or set the `SFCWC_ASAR` env var to the executable.
- **`snes_spc` oracle**: vendored under `tools/snes_spc_oracle/`.
  Build separately (see next section).

## Build the snes_spc oracle

The oracle is an out-of-process LGPL wrapper; the host binary
never links against it. Build it once:

```bash
cd tools/snes_spc_oracle
mkdir -p build && cd build
# CMakeLists.txt drives the build:
cmake -G "Visual Studio 17 2022" ..   # Windows
# OR:
cmake -G "Unix Makefiles" ..           # Linux/macOS
cmake --build . --config Release
```

The release binary lands at
`tools/snes_spc_oracle/build/Release/snes_spc_oracle.exe`
(Windows) or `tools/snes_spc_oracle/build/snes_spc_oracle`
(Unix). The host auto-resolves it from that conventional path,
or you can set `SFCWC_SNES_SPC_ORACLE=<path>` to point elsewhere.

`sfcwc doctor` reports oracle resolution:

```bash
cargo run --release --bin sfcwc -- doctor
```

## Run the test suite

From the repository root:

```bash
cargo test --workspace
```

Expected counts:
- v0.2-rc: **521 tests across the workspace, all green**.
- v0.3-rc: **579 tests across the workspace, all green** (+58
  from M3.0–M3.7: loop-click metric + atom edge cases + phase
  rotation + characterize_gaussian module + methodology
  diagnostics + rename_sequence_id_cascade + atom preview
  metric flow).

All four cargo gates must pass before any release tag:

```bash
cargo check --workspace
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

Tests that depend on `asar` or `snes_spc_oracle` skip with an
stderr note when the tools aren't resolved; on CI both must be
present for the full suite to run.

## Run `m2-acceptance` against the canonical fixture

The canonical M2 fixture is committed at
[`fixtures/projects/canonical_m2/`](../fixtures/projects/canonical_m2/) —
a deterministic 32 kHz mono PCM16 WAV at `audio/lead.wav` plus a
v2 multi_voice_atom project file referencing it with the WAV's
SHA. Same shape as the
`core/tests/sequence_compile.rs::canonical_project()` helper that
pins the canonical SEQ2 bytecode SHA in
`baselines/m2_canonical_fixtures.md`.

```bash
cargo run --release --bin sfcwc -- m2-acceptance \
    --project-a fixtures/projects/canonical_m2/canonical_m2.sfcproj.json \
    --out build/m2/acceptance/canonical \
    --frames 160000
```

Expected stderr summary:

```
m2-acceptance: project_a=canonical_m2.sfcproj.json, project_b=(clone)
  stage_1_validation: ok
  stage_2_compile: ok
  stage_3_oracle: ok
  stage_4_infrastructure: ok
  bundle.status: ok
  -> build/m2/acceptance/canonical/bundle.json
```

The full report lands in
`build/m2/acceptance/canonical/bundle.json` with stage rollups
and per-channel oracle metrics. SPEC §21 floors are checked:

- per-channel `max_abs >= 1000`, `rms >= 200` for audible channels
- `max_abs <= 50` on hard-panned silent channels
- post / pre source-step zero-crossing rate ratio `>= 1.5×`

## Run `m3-acceptance` against the canonical fixture

M3 acceptance extends M2 acceptance with M3-specific quality
gates: atom PCM stability (SPEC §16.9 identity pins), loop-click
improvement (SPEC §10.7 phase rotation post ≤ pre), encoder-
quality documentary snapshot (post-rotation BRR + decoded-BRR
SHAs), and baselines integrity audit (every identity_gated
entry carries a `test:` field).

```bash
cargo run --release --bin sfcwc -- m3-acceptance \
    --project-a fixtures/projects/canonical_m2/canonical_m2.sfcproj.json \
    --out build/m3/acceptance/canonical \
    --frames 160000
```

Expected stderr summary:

```
m3-acceptance: project_a=canonical_m2.sfcproj.json
  stage_1_m2_regression: ok
  stage_2_atom_pcm_stability: ok
  stage_3_loop_click_improvement_gate: ok
  stage_4_encoder_quality_snapshot: ok (ok)
  stage_5_baselines_integrity: ok
  bundle.status: ok
  -> build/m3/acceptance/canonical/bundle.json
```

The five stages and what each gates:

- **stage 1** — spawns `sfcwc m2-acceptance` and reads its
  `bundle.json::bundle.status`. M3 doesn't change M2; this is
  the regression guard.
- **stage 2** — runs the 11 atom-PCM-SHA identity-pin tests
  (2 from M3.1 + 9 from M3.2 atom edge cases). Per SPEC §16.9
  any drift here is an atom-render regression.
- **stage 3** — runs `phase_rotation_loop_click_never_regresses_against_pre_m3`.
  Asserts per-fixture `loop_click_abs` post-rotation `≤` the
  pre-M3 value (`M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE`,
  behavior-gated).
- **stage 4** — runs the M3.3 documentary post-rotation BRR
  and decoded-BRR PCM SHA tests. **Soft gate** (warn-not-fail):
  documentary entries are informational; drift is noted but
  doesn't fail the bundle. M3.4 (deferred) and M3.6 (deferred
  per M3.5.1 methodology audit) are the next surfaces where
  these would shift.
- **stage 5** — in-process audit of
  `baselines/m3.json::identity_gated`: every entry must carry a
  non-null `test:` field pointing at a real test (M2.8.1
  pattern). 11/11 identity_gated entries verified at v0.3-rc.

Also runs against the M3.3 committed edge-case fixture:

```bash
cargo run --release --bin sfcwc -- m3-acceptance \
    --project-a fixtures/projects/atom_edge_cases/harmonic_16_cycle_64.sfcproj.json \
    --out build/m3/acceptance/harmonic_16_cycle_64 \
    --frames 16000
```

Expected: `bundle.status=ok`. The fixture is an atoms-only V2
project (empty `sample_pool` per SPEC §16.6 M2.5 relaxation),
near-Nyquist content, validated end-to-end through
`sfcwc render-atom` against `baselines/m3.json` documentary
values byte-exactly.

## Reproduce the M3.5 gaussian characterization

The M3.5 characterization pass measures S-DSP gaussian
interpolation behavior against a 9-signal canonical set. M3.5.1
amended the report schema to v3 with seven methodology
diagnostic fields plus `gain_delta_db_aligned` and added a
decision-rule precondition (`zcr_ratio ∈ [0.9, 1.1]` for
monotonicity-anchor signals).

```bash
cargo run --release --bin sfcwc -- characterize-gaussian \
    --out-report build/m3/characterize_gaussian.json \
    --out-dir build/m3/characterize_gaussian \
    --frames 16000
```

Expected outcome at v0.3-rc: `recommended_next = "methodology_review"`.
Precondition #0 fires because the brute-force
`align_oracle_to_raw` (`max_offset = 32` samples) can't resolve
cycle lengths > 32 (the canonical signals use 64/128/256). M3.6
pre-emphasis preset implementation defers to M4+ until the
methodology is resolved.

The full per-signal numbers are captured under
`baselines/m3.json::documentary_snapshot` as `M3_5_*` entries
(12 per signal + 4 summary entries + the M3.5.1 diagnostics).
The report itself contains a `_methodology_audit_m3_5_1` field
documenting the anomalies, the audit actions taken, and the M4
deferral.

## Reproduce the M3.5 prelude A/B audition WAVs

The M3.5 prelude shipped ten WAV files at
`build/audition/m3.5-prelude/` (gitignored) — five pre/post
phase-rotation A/B pairs covering the canonical sines plus the
clipping-stress fixture. Regenerate locally with:

```bash
cargo test -p sfc-atomizer-core --test atom_edge_cases \
    m3_5_emit_audition_wavs -- --nocapture --ignored
```

Each WAV is 192,044 bytes (3.0 s @ 32 kHz, 16-bit mono PCM).
Filenames: `sine_64_{pre,post}_rotation.wav`,
`sine_128_{pre,post}_rotation.wav`,
`harmonic_16_cycle_64_{pre,post}_rotation.wav`,
`normalize_false_clamp_{pre,post}_rotation.wav`,
`all_8_partials_{pre,post}_rotation.wav`.

## Verify locked baselines

`baselines/m2.json` and `baselines/m3.json` together list every
release baseline classified into:

- `identity_gated` — drift = regression. M2: M1 loader + driver
  SHA, M2 canonical sequence + voice-setup-table SHA,
  total_ticks. M3: 11 atom PCM SHAs (2 M3.1 + 9 M3.2),
  identity-pinned per SPEC §16.9.
- `behavior_gated` — numeric thresholds documenting policy. M2:
  audibility floors, silence ceiling, source-step ratio, module
  size cap. M3: `M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE`
  (post ≤ pre per atom fixture); three policy entries for the
  metric / objective / candidate-set contracts.
- `documentary_snapshot` — informational only; expected to shift
  on declared milestones. M2: atom BRR / PCM SHAs (PCM SHAs
  promoted to identity at M3.1), loop click scores, M2 driver
  size. M3: post-rotation BRR + decoded-BRR PCM SHAs (11
  fixtures × 7 entries), 12 M3.5 characterization measurements
  per signal × 9 signals + 4 summary entries + the M3.5.1
  diagnostic re-snapshots (`GAIN_DELTA_DB_ALIGNED`,
  `ZCR_RATIO`, `NORMALIZED_CORRELATION`,
  `PEAK_ABS_ERROR_AFTER_GAIN_NORMALIZATION`,
  `ALIGNMENT_BEST_OFFSET` per signal).
- `retired` — superseded baselines kept for archaeology.

`baselines/m3.json` inherits M2 by reference
(`inherits: { m2: "baselines/m2.json" }`). The `m3-acceptance`
bundle's stage 1 (M2 regression) is the runtime check that the
inheritance still holds — any M2 baseline drift fails M3 too.

Tests pin identity-gated and behavior-gated values; documentary
snapshots are not gated. The canonical SEQ2 bytecode + voice
setup table hex dumps + per-byte breakdowns live at
`baselines/m2_canonical_fixtures.md`. Per-test mapping for the
M3 identity_gated entries lives in each entry's `test:` field
in `baselines/m3.json`.

## Real-hardware emulation audition (optional)

[Mesen2](https://www.mesen.ca/) plays both `.spc` and `.sfc`
files. Set `SFCWC_MESEN2=<path-to-Mesen.exe>` to make `sfcwc
doctor` resolve it; Mesen2 isn't auto-launched, the user opens
artifacts manually. Use Mesen2 to corroborate snes_spc oracle
output on a real-hardware-faithful emulator. snes_spc remains
the formal acceptance gate.

## Troubleshooting

**`asar not resolved on this host`**: install `asar` per the
prerequisites section, or set `SFCWC_ASAR` to the binary.

**`oracle wrapper not resolved`**: build the oracle per "Build
the snes_spc oracle" above, or set `SFCWC_SNES_SPC_ORACLE`.

**Tests pass locally but `m2-acceptance` reports `stage_3_oracle:
error` on the canonical fixture**: most likely the oracle binary
on `PATH` is older than the snes_spc git pin recorded in
`tools/snes_spc_oracle/main.cpp::SNES_SPC_PIN`. Rebuild from the
vendored source.

**Test failure `loader_byte_identity_at_m2_6` after editing the
loader source**: M1 loader bytes are identity-gated (588 B with
SHA pinned in `baselines/m2.json`). Any change to
`core/fixtures/asm/m1_loader_65816.asm` or the asar version
producing it shifts the SHA — investigate before relaxing the
test (likely an unintended regression).

**v1 / v2-sample-only `compile-sfc` produces non-byte-identical
output to M1.6 baseline**: M1 baseline preservation invariant
broken. Stop and diagnose; this is a regression of a locked
baseline, not a behavior to relax.

**Module too large at compile-sfc**: SPEC §15.6 caps `module.bin`
at 32 KiB (one LoROM bank). Reduce sample frames or atom count.
The 32 KiB cap is hard.

## Reference

- `SPEC.md` — full specification.
- `STATUS.md` — current milestone, last pass, current baselines.
- `docs/history/M0-M2-passes.md` — per-pass log archive (M0–M2.7).
- `baselines/m2.json` — machine-readable M2 release baselines.
- `baselines/m3.json` — machine-readable M3 release baselines
  (inherits M2 by reference).
- `baselines/m2_canonical_fixtures.md` — canonical SEQ2 + voice
  setup table fixture hex.
- `RELEASE_NOTES_v0.2-rc.md` — v0.2-rc release-candidate notes.
- `RELEASE_NOTES_v0.3-rc.md` — v0.3-rc release-candidate notes
  including the M3.5/M3.5.1 methodology deferral.
