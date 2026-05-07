# Reproducing the M2 acceptance pipeline

This guide takes a fresh clone of the repository to a passing
`m2-acceptance` bundle. It exercises the full M2 pipeline: sample
encoding, atom rendering, sequence compilation, M2 driver
assembly, `.spc` / `.sfc` generation, per-channel oracle gates,
and the `m2-acceptance` aggregator.

Tested on Windows 11 / msys2 bash; the same commands work on
Linux and macOS with the obvious path-separator adjustments.

## Prerequisites

- **Rust toolchain**: stable channel, pinned via
  `rust-toolchain.toml` at the repo root. `rustup` will install
  the right toolchain on first `cargo` invocation. No specific
  version pin yet.
- **`asar` SPC700 / 65816 assembler**: 1.81 or later. Engineer's
  machine ships with 1.91. Build from
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

Expected at v0.2-rc: **521 tests across the workspace, all green**.

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

The canonical M2 fixture is synthesized by the test
`app/tests/cli.rs::write_v2_combined_for_m25_gate`. To run
`m2-acceptance` outside a test, point at any v2 multi_voice_atom
project:

```bash
cargo run --release --bin sfcwc -- m2-acceptance \
    --project-a path/to/canonical_m2.sfcproj.json \
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

## Verify locked baselines

`baselines/m2.json` lists every release baseline classified into:

- `identity_gated` — drift = regression. M1 loader + driver SHA,
  M2 canonical sequence + voice-setup-table SHA, total_ticks.
- `behavior_gated` — numeric thresholds documenting policy
  (audibility floors, silence ceiling, source-step ratio,
  module size cap).
- `documentary_snapshot` — informational only; expected to shift
  on declared milestones (M2 driver size, atom BRR / PCM SHAs,
  loop click scores).
- `retired` — superseded baselines kept for archaeology.

Tests pin identity-gated and behavior-gated values; documentary
snapshots are not gated. The canonical SEQ2 bytecode + voice
setup table hex dumps + per-byte breakdowns live at
`baselines/m2_canonical_fixtures.md`.

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
- `baselines/m2.json` — machine-readable release baselines.
- `baselines/m2_canonical_fixtures.md` — canonical SEQ2 + voice
  setup table fixture hex.
- `RELEASE_NOTES_v0.2-rc.md` — release-candidate notes.
