# SFC Wave Compiler — Status

## Current milestone

**M0 — Research harness** — in progress. The snes_spc oracle
wrapper boundary is in (M0.5): a thin C++ wrapper renders the
M0 smoke `.spc` to all-zero PCM (as the muted state contract
predicts), and `sfcwc calibrate-oracle` populates a real
calibration report. Next is M0.6 (the calibration report + M0
acceptance bundle).

## Last pass

**Pass M0.5 — snes_spc oracle wrapper boundary.**

- Phase A: Vendored `snes_spc 0.9.0` from the
  `blarggs-audio-libraries` mirror as an unmodified snapshot under
  `tools/snes_spc_oracle/vendor/snes_spc/`. Pinned commit
  `ec8ee2bbe30451614c1d02a83f7af1c97d497d45` (2020-10-24).
  License files preserved verbatim; provenance documented in
  `vendor/snes_spc/README-vendoring.md`. No upstream files
  modified.
- Phase B: New C++17 wrapper at `tools/snes_spc_oracle/main.cpp`,
  ~290 lines, no third-party dependencies. One subcommand:
  `render --input-spc --frames --output-pcm --report`. Locked CLI
  contract v1. Hand-rolled JSON output, inline FIPS 180-4 SHA-256.
  `spc_clear_echo()` after `spc_load_spc()` for determinism, per
  upstream's recommendation.
- Phase C: Build via CMake (3.16+, C++17). Output binary at
  `tools/snes_spc_oracle/build/Release/snes_spc_oracle.exe` on
  Windows/MSVC. `tools/snes_spc_oracle/build/` ignored by git.
- Phase D: `tools/snes_spc_oracle/README.md` documents the build,
  the locked CLI contract, the LGPL-2.1+ distribution obligations
  the wrapper inherits, and the process-boundary integration model.
- Phase E: `core::report::CalibrationReport` upgraded — replaced
  M0.1 placeholder inner shapes with M0.5 typed shapes
  (`ObservedInfo { voice_render_max_abs_lsb, voice_render_rms_lsb }`,
  same for `ProvisionalTolerances`), added
  `fixture_set: Option<FixtureSetInfo>`, `diagnostics: Vec<String>`,
  `error: Option<String>`. SCHEMA_VERSION stays at 1.
  `core::tools::resolve_snes_spc_oracle` extended to find the
  wrapper at its build output paths.
- Phase F: `sfcwc calibrate-oracle` rewired with `--oracle`,
  `--input-spc`, `--frames`, `--out`, `--out-pcm` flags. Spawns
  the wrapper, reads back PCM, recomputes max_abs/rms in Rust
  defensively, populates a real `CalibrationReport` with
  `status: provisional_not_ci_gate`. Failure-as-data on
  oracle-missing, input-missing, and wrapper-error branches.
- Phase G: `m0-acceptance` chain updated to invoke the real
  calibrate-oracle in step 6 (replacing the M0.4 stub). Oracle
  PCM lands at `<out>/oracle.pcm_s16le`. Three new CLI
  integration tests for oracle-resolved, oracle-missing, and
  input-spc-missing branches.

79 tests across the workspace: 55 core unit + 10 BRR fixture
integration + 14 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` all green. Wrapper builds clean with one harmless
struct/class warning from upstream's vendored code.

Locked observation: rendering the M0.4 muted smoke `.spc` through
the wrapper produces 8,192 zero PCM bytes (max_abs=0, rms=0),
matching the SPEC §19.3 contract. PCM SHA-256 is reproducible
across runs.

## Previous passes

**Pass M0.4 — Minimal `.spc` exporter smoke.**

- Phase A: New `core::spc` module — SPC v0.30 layout constants for
  the 66,048-byte file (header / ARAM / DSP / 64 B unused gap /
  Extra RAM), `SpcCpuState` / `SpcImage` / `SpcBuildError` types,
  and `SpcImage::to_bytes`/`write_to_path`. Layout cross-checked
  against fullsnes and vspcplay; 0x10180..0x101C0 is the canonical
  unused gap, 0x101C0..0x10200 is Extra RAM.
- Phase B: New SPEC §19.3 SPC smoke state contract — PC=$0200,
  GPRs zero, SP=$EF, FLG=$60 (Mute amp + Echo write disable), all
  other DSP regs zero, ID666 indicator absent (0x1B). Mirrored as
  `SMOKE_CPU_STATE`, `SMOKE_FLG`, `smoke_dsp_regs`,
  `build_smoke_image` constants/functions in `core::spc`.
- Phase C: `core::spc::verify_structure` — observation-only parse
  of any v0.30 SPC, returns magic_ok / minor_version /
  id666_present / cpu / per-region SHA-256s. Never asserts; the
  caller decides what's fatal.
- Phase D: New `core::aram` module with
  `map_from_image(&[u8; 65536]) -> AramMapReport`. Walks for
  first/last nonzero byte in `$0200..$FFC0` to identify
  driver_code; everything else is `free`. Documented as a stopgap
  valid for the M0 smoke output; the real packer lands in M3+.
  The new partition invariant: regions sum to total_aram exactly,
  free_bytes equals the sum of `Free` regions.
- Phase E: `SpcExportReport` extended with four new optional
  fields — `input_aram_sha256`, `dsp_state_sha256`,
  `spc_file_sha256`, `error`. Same `skip_serializing_if` pattern
  as M0.3; pre-M0.4 reports still parse.
- Phase F: `sfcwc export-spc-smoke` rewired with `--aram`,
  `--out-spc`, `--verify-structure` flags. Reads driver.bin,
  builds smoke image, writes 66 KB .spc, optionally re-verifies
  structure. ARAM-missing/wrong-size is failure-as-data.
- Phase G: `sfcwc m0-acceptance` chained end-to-end — doctor →
  decode-fixtures → assemble-smoke → export-spc-smoke →
  aram::map_from_image → calibrate-oracle (still stub) →
  manifest. Failures in one step don't halt the chain; the
  manifest still gets produced.
- Phase H: 76 tests passing — 54 core unit + 10 BRR fixture
  integration + 12 app CLI integration. New tests cover SPC byte
  layout, ARAM partition invariant, the export-spc happy/sad
  paths, and a full m0-acceptance chain run.

`cargo check`, `cargo fmt --check`, `cargo clippy --all-targets --
-D warnings`, and `cargo test` all green.

## Previous passes

**Pass M0.3 — Asar backend smoke.**

- Phase A: New `core::asm` module with `AssemblerBackend` trait
  (name, version, assemble) and supporting types — `AssembleInput`,
  `AssembleOutput`, `AssembleError`. Trait keeps the door open for
  WLA-DX without rewiring the build pipeline; M0.3 ships only
  `AsarBackend`.
- Phase B: `AsarBackend::assemble` — pre-creates a 64 KB
  zero-filled scratch file, invokes asar with the locked
  `--no-title-check --fix-checksum=off` form, verifies exact 64 KB
  size, computes SHA-256 of the full image. Asar quirks discovered
  in probing and locked in the module docs: `org $0200` errors
  without `lorom + org $008200 + base $0200`; without
  `--fix-checksum=off` asar writes 4 LoROM checksum bytes into our
  flat ARAM image at `$7FDC..$7FDF`.
- Phase C: `core/fixtures/asm/m0_smoke.asm` — trivial NOP + BRA
  loop yielding sentinel bytes `00 2F FD` at file offset `0x0200`,
  with every other byte zero. Brief sketched `00 2F FE` but the
  actual displacement is `-3` (BRA targets `start: nop` at $0200,
  not the BRA itself), so 0xFD is correct.
- Phase D: Two new optional fields on `AssembleReport` —
  `output_image_sha256` and `error`. Both `skip_serializing_if`,
  so the schema extension is non-breaking; pre-M0.3 reports still
  parse. `SCHEMA_VERSION` stays at 1.
- Phase E: `sfcwc assemble-smoke` rewired to invoke the asar
  pipeline. New `--out-image` flag (default
  `build/m0/driver.bin`); existing `--out` keeps the JSON report
  path. Failure-as-data: asar-missing or asar-error writes a
  populated `AssembleReport` with `status: error` and exit 0.
- Phase F: Two CLI integration tests — asar-present (gated on
  `resolve_asar().resolved`, asserts exit 0, 64 KB image, exact
  sentinel bytes, sha256 in stderr) and asar-missing (forces
  failure via `SFCWC_ASAR=<bogus>` + isolated `PATH`).

53 tests across the workspace — 32 core unit + 10 BRR fixture
integration + 11 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and `cargo
test` all green.

## Previous passes

**Pass M0.2 — Raw BRR decoder + deterministic fixture suite.**

- Phase A: New `core::brr` module — `BrrHeader`, `BrrDecoderState`,
  `decode_block`, `decode_blocks`. Pipeline: 4-bit sign extension,
  shift (with documented 13..=15 path → -2048 / 0), one of four
  filter formulas in i32 integer arithmetic with hardware-matching
  arithmetic-right-shift rounding, optional 16-bit clamp on filters
  2/3, final 15-bit wrap via `int16(s << 1) >> 1`. Six unit tests
  cover header parsing, sign extension, predictor mutation, filter-0
  identity, header-flag isolation, and the shift-13 path.
- Phase B: Nine-fixture corpus under `core/fixtures/brr/` — every
  `expected_pcm` is independent ground truth (hand-walked or
  spec-derived via `_reference.py`, never decoder output). Coverage:
  filter 0 basic, filter 0 shift/clamp incl. shift=12 and shift=13,
  filter 1 zero/nonzero history, filter 2 nonzero, filter 3 nonzero,
  multi-block predictor continuity, SPEC §10.2 loop-entry seeding,
  and END/LOOP-flag isolation. `_reference.py` is a deliberately
  separate Python implementation of the same fullsnes/snesbrr
  formulas, used as a second-source check.
- Phase C: `core/fixtures/brr/README.md` documents the provenance
  discipline ("expected_pcm is not decoder output"), per-fixture
  audit table, the SnesLab vs SNESdev END/LOOP bit-ordering
  disagreement (we follow SNESdev / boldowa-snesbrr), and the rule
  that any change to expected_pcm requires a provenance_notes update.
- Phase D: New `core/tests/brr_fixtures.rs` runs each fixture in its
  own #[test] for individual failure granularity. `core::brr_fixtures`
  module embeds the corpus via `include_str!` and exposes
  `run_fixture` returning `BrrFixtureResult`.
- Phase E: `sfcwc decode-fixtures` now runs the real corpus and emits
  per-fixture pass/fail data with a `9/9 passed; wrote <path>`
  stderr summary. Failed fixtures are data, not process errors —
  command exits 0. Other M0.1 stubs untouched per brief.

44 tests across the workspace — 24 core unit + 10 core fixture
integration + 10 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and `cargo
test` all green.

## Previous passes

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
- **BRR fixture provenance discipline** (M0.2): every primary fixture
  must have independent ground truth — hand-walked, spec-derived, or
  external-tool. Decoder output as ground truth ("regression-baseline"
  provenance) is forbidden in the M0 corpus. If we ever add an
  audit-only baseline, it lives in a separate directory and is
  excluded from the M0 acceptance gate.
- **BRR fixture asset path `core/fixtures/brr/`** (M0.2): fixtures are
  first-class project assets, not test-only data. They live under
  `core/fixtures/`, are embedded via `include_str!` so they ship in
  the binary, and are consumed by both `core::brr_fixtures` (tests)
  and the `sfcwc decode-fixtures` CLI command.
- **Header bit ordering: END = bit 0, LOOP = bit 1** (M0.2): SnesLab's
  wiki disagrees (claims end=bit 1, loop=bit 0). We follow the
  SNESdev wiki and boldowa/snesbrr convention, locked by
  `flags_end_loop_ignored_by_raw_decode` and the `header_parse_layout`
  unit test.
- **AssemblerBackend trait shape** (M0.3): `name` + `version` +
  `assemble(&AssembleInput) -> Result<AssembleOutput, AssembleError>`.
  Version probing is informational, never gating — failures yield
  `Ok("unknown")`. `WrongOutputSize` is a hard error so a 64 KB
  image is always exactly 64 KB.
- **Asar invocation locked**: `asar --no-title-check
  --fix-checksum=off <source.asm> <output.bin>` (M0.3). Plus the
  smoke .asm must declare `lorom + arch spc700 + org $008200 +
  base $0200` to coax asar into a flat 64 KB ARAM workflow without
  expanding the file or injecting SNES-rom checksum bytes.
- **Smoke .asm location** `core/fixtures/asm/m0_smoke.asm` (M0.3):
  first-class fixture, not test-only. Sentinel bytes `00 2F FD` at
  offset `0x0200` are locked by an integration test.
- **SPC v0.30 file size 66,048 bytes, no extended ID666** (M0.4):
  Header (256 B) + ARAM (64 KB) + DSP (128 B) + unused gap (64 B at
  0x10180) + Extra RAM (64 B at 0x101C0). ID666 indicator = absent
  (0x1B); the 210-byte tag region is zero-filled. xid6/extended
  ID666 deferred until a use case appears.
- **M0 smoke state contract** (M0.4, SPEC §19.3): PC=$0200, GPRs=0,
  SP=$EF, DSP FLG=$60 (Mute amp + Echo write disable), every other
  DSP reg = 0. Result: SPC700 runs the driver from $0200 while the
  DSP produces no audio. M1+ smoke profiles will add an
  audible-but-deterministic state.
- **ARAM map approach for M0** (M0.4): `core::aram::map_from_image`
  walks the assembled image for nonzero driver-code extent. Valid
  for "single contiguous driver, no sample pool, no atom pool yet."
  The M3+ ARAM packer replaces this whole module; M0.4's stopgap
  reports the new invariant that regions partition total_aram
  exactly (rather than the M0.1 stub's "fixed regions only" model).
- **snes_spc fork: blarggs-audio-libraries mirror** (M0.5): pinned
  to commit `ec8ee2bbe30451614c1d02a83f7af1c97d497d45` (2020-10-24,
  unmodified Blargg 0.9.0 snapshot). Vendored at
  `tools/snes_spc_oracle/vendor/snes_spc/`; not a submodule.
- **Oracle wrapper build system: CMake** (M0.5): ~30-line
  CMakeLists.txt globs `vendor/snes_spc/*.cpp` plus the wrapper's
  `main.cpp`. C++17, no third-party deps, no package manager. The
  alternative `build.bat` fallback was unnecessary — CMake worked
  cleanly with the on-host Visual Studio 18 Community generator.
- **C++ toolchain on this host** (M0.5): no install needed —
  Visual Studio 18 Community (cl.exe 14.50.35717), Visual Studio
  2019 BuildTools (cl.exe 14.29.30133), and CMake 4.3.2 were all
  already present. CMake auto-selected the VS 18 generator.
- **Oracle wrapper CLI contract v1** (M0.5): ONE subcommand
  `render --input-spc <path> --frames <N> --output-pcm <path>
  --report <path>` plus `--version`. Locked; do not extend without
  a PM brief. Output PCM is `frames*4` bytes of s16le interleaved
  stereo at 32 kHz; oracle-side report mirrors the schema described
  in the M0.5 brief. `spc_clear_echo()` after load enforces
  determinism.
- **Oracle smoke baseline** (M0.5): rendering the M0.4 muted
  smoke `.spc` through the wrapper produces 8,192 zero PCM bytes
  (max_abs=0, rms=0). M0.5 provisional tolerances are `max_abs=1`,
  `rms=0.25`; both informational only (`ci_gate: false`). M1
  freezes the first accepted tolerance table per SPEC §10.1, §21.

## Open questions remaining

Cross-reference SPEC §23. Of the four questions there:

1. Final atom quality thresholds — empirical, deferred to calibration runs.
2. Embed snes_spc for live preview vs keep oracle-only — decision target M0.
3. Practical wavetable frame caps — empirical, may move from 32/64/96/128.
4. Whether a future version adds a free pre-emphasis EQ editor.

## Next pass

**M0.6 — Calibration report + M0 acceptance bundle.** PM to
brief. Bundles the seven M0 reports into a frozen acceptance
artifact, populates `M0Manifest.generated_at`, and pins the M0
exit conditions to a reproducible build artifact set.
