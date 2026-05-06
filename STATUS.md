# SFC Wave Compiler — Status

## Current milestone

**M0 — Research harness** — in progress. CLI surface and report
schemas are in (M0.1); raw BRR decoder and the deterministic fixture
suite are next (M0.2).

## Last pass

**Pass M0.1 — CLI surface, JSON report schemas, tool resolution.**

- Phase A: Workspace deps (`serde`, `serde_json`, `clap`,
  `thiserror`, `sha2`, `tempfile`) declared in
  `[workspace.dependencies]`; per-crate dependency tables wired in
  `core/` and `app/`. Binary name pinned to `sfcwc` via `[[bin]]` in
  `app/Cargo.toml`.
- Phase B: Seven stable report types in `core::report`
  (`DoctorReport`, `BrrFixtureReport`, `AramMapReport`,
  `AssembleReport`, `SpcExportReport`, `CalibrationReport`,
  `M0Manifest`), each carrying the
  `{ schema_version: 1, report_type: "...", ... }` envelope and a
  `stub()` constructor for placeholder bodies. Round-trip serde tests
  guard every shape.
- Phase C: Tool resolution in `core::tools` per SPEC §17.1.
  `resolve_asar`, `resolve_snes_spc_oracle`, `resolve_mesen2` follow
  env → (PATH or workspace-default, per tool) → missing; asar and the
  oracle wrapper are best-effort version-probed via `--version`.
  Mesen2 is env-only with no PATH fallback and no version probe per
  spec.
- Phase D: `sfcwc` CLI binary in `app/src/main.rs` with six
  clap-derive subcommands. `doctor [--json] [--out]` does real
  resolution and emits `DoctorStatus::{Ok, Warnings, Errors}`. Five
  stubs (`decode-fixtures`, `assemble-smoke`, `export-spc-smoke`,
  `calibrate-oracle`, `m0-acceptance`) write valid placeholder
  reports to `build/m0/` (default) or a `--out` path. Errors flow
  through a `thiserror`-derived `CliError`.
- Phase E: 22 tests across the workspace — 12 round-trip unit tests
  in `core` and 10 CLI integration tests in `app/tests/cli.rs`
  exercising every subcommand including the `SFCWC_ASAR=<sentinel>`
  env-resolution path.

`cargo check`, `cargo fmt --check`, `cargo clippy --all-targets --
-D warnings`, and `cargo test` all green.

## Previous passes

**Pass 2.0 — Pre-M0 spec cleanup + M0-readiness additions.**

- Phase A: §21 M0 acceptance reconciled — removed the contradictory
  closing paragraph and three superseded acceptance bullets (snes_spc
  render gate, SPC-player playback, WAV-fixture round-trip).
- Phase B: `.sfc` exporter deferred to M1 alongside the 65816
  SPC-upload contract (SPEC §19.2); Mesen2 added as the manual M0
  `.spc` verification path.
- Phase C: New §17.1 "Tool discovery" subsection; §10.1
  cross-reference typo fixed (snes_spc external validation lives in
  §17 and §18, not §16); §4 architecture diagram cosmetic.
- Phase D: Rust toolchain pinned to the stable channel via
  `rust-toolchain.toml`; `CLAUDE.md` "External tools" section added.

**Pass 1 — Repo hygiene + spec truth.**

- Phase A: SPEC.md unescaped (markdown backslash-escapes and `&#x20;` HTML
  entities stripped; tables/code fences/lists tightened).
- Phase B: 14 consultant-review patches applied (M0 readiness).
- Phase C: Cargo workspace skeleton created (`core/`, `app/`); STATUS,
  CLAUDE, LICENSE, LICENSING docs added; `.gitignore` covering Rust,
  Windows, editors, `build/`, `.claude/`.
- Phase D: `cargo check` and `cargo fmt --check` clean; pushed to `main`.

## Decisions log

- **License model:** Apache-2.0 for the host application source; 0BSD for
  generated outputs (`.spc`, `.sfc`, driver/module blobs); snes_spc kept
  out-of-process to avoid LGPL propagation. See `LICENSING.md`.
- **Assembler:** asar primary for M0–M2, behind an `AssemblerBackend`
  interface so WLA-DX can be added later without rewiring the compiler
  (SPEC §4).
- **`max_sources`:** 128 (SPEC §5.4); ARAM packer enforces actual
  source-directory footprint.
- **M0 acceptance split** (resolved per consultant Patch 5): raw BRR
  decode is bit-identical at M0; voice-render and full-module-render
  tolerances are provisional at M0 and frozen at M1 (SPEC §10.1, §21,
  §23).
- **`.sfc` deferred to M1** (Pass 2.0): the 65816 SPC-upload contract is
  M1 work (SPEC §19.2); M0 ships `.spc` only.
- **Tool discovery via `SFCWC_*` env vars** (Pass 2.0): asar, snes_spc
  oracle, Mesen2 (SPEC §17.1).
- **Rust toolchain pinned to `stable` channel** (Pass 2.0): no specific
  version pin yet; revisit if regressions appear.
- **CLI binary name `sfcwc`** (M0.1): pinned in `app/Cargo.toml` so the
  executable is `sfcwc` regardless of the `sfc-atomizer` package name.
- **Default report output dir `build/m0/`** (M0.1): every stub command
  writes there unless `--out` overrides; `m0-acceptance` writes a
  sibling `manifest.json` next to the six reports.
- **Report envelope `{ schema_version, report_type }`** (M0.1): every
  JSON report carries these two top-level fields. `SCHEMA_VERSION = 1`
  in `core::report` gates breaking shape changes; the `report_type`
  string lets generic JSON consumers dispatch without a wrapping
  discriminator.

## Open questions remaining

Cross-reference SPEC §23. Of the four questions there:

1. Final atom quality thresholds — empirical, deferred to calibration runs.
2. Embed snes_spc for live preview vs keep oracle-only — decision target M0.
3. Practical wavetable frame caps — empirical, may move from 32/64/96/128.
4. Whether a future version adds a free pre-emphasis EQ editor.

## Next pass

**M0.2 — Raw BRR decoder + deterministic fixture suite.** PM to brief.
The fixture report shape from M0.1 (`BrrFixtureReport`,
`fixture_set: "m0_raw_decode"`, results array) gets filled in.
