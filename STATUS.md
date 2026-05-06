# SFC Wave Compiler — Status

## Current milestone

**M1 in progress — data model + minimal app shell complete.**
`ProjectV1::validate` covers all 25 SPEC §16.6 rules; the project
file v1 round-trips byte-stably through `load_from_path` /
`save_to_path`; a read-only `sfcwc-app` GUI viewer renders the
Sample Pool with validation overlays; the CLI gains
`new-project` / `validate-project`. Next: M1.2 — WAV/AIFF/BRR
import.

**M0 artifacts are producer-side only.** M1 owns the first audible
driver. The NOP+BRA M0 smoke driver (`core/fixtures/asm/m0_smoke.asm`)
is intentionally non-functional and will be replaced wholesale at
M1.5 — do not reuse it as a base for M1.5 driver work.

## Last pass

**Pass M1.1 — Project v1 + Sample Pool data model + minimal app shell.**

- Phase A: `ProjectV1::validate` body lands. 25-rule SPEC §16.6
  coverage; multi-error collection (no bail-on-first); errors
  carry JSON-pointer paths. `ValidationError` redesigned from a
  flat enum to `{path, kind: ValidationErrorKind}`. Echo rules
  delegate to `core::echo_validation`. Convention: `name` allows
  spaces and non-ASCII; `id` is `^[a-z0-9_]+$`.
- Phase B: Project I/O. `load_from_path` / `save_to_path` /
  `load_and_validate` / `migrate_from_value` / `new_template`.
  `ProjectIoError` covers NotFound / Io / Parse / MalformedValue /
  UnsupportedSchemaVersion / Validation. Stable byte-for-byte
  round-trip verified by test (struct field declaration order
  preserved through serde).
- Phase C: New `core::report::ValidationReport` JSON envelope
  (status: ok | invalid | io_error; flat `{path, message}`
  errors). New `sfcwc new-project` and `sfcwc validate-project`
  CLI subcommands. Exit 0 / 2 / 1 for ok / invalid / io_error
  respectively. Project file extension chosen: `.sfcproj.json`
  (JSON-honest, project-prefixed).
- Phase D: New `sfcwc-app` GUI binary at `app/src/app_main.rs`.
  ~560 lines eframe/egui. Read-only viewer: File menu, Sample Pool
  list, sample/project detail panel, status bar with
  validation rollup, "Show errors" modal. Empty-pool placeholder
  ("No samples imported yet. (Import lands at M1.2.)"). Hand-
  rolled text-input modal stands in for native file pickers
  pending an authorized picker dep.
- Phase E: 50 new unit tests in `core::project` (every rule
  positive + negative; round-trip byte-stability; migration v1
  vs unsupported; new_template field parity). 5 new CLI
  integration tests. 183 tests total across the workspace.
- Phase F: SPEC §16.4 documents the `id` regex `^[a-z0-9_]+$`,
  the `name` UTF-8 + control-char + path-separator rules, and
  the strict `sha256` shape. SPEC §16.6 replaced with the 25
  rules grouped thematically (Root / project / driver /
  master_echo / sample_pool / Envelope / m1). LICENSING.md
  gains §4 covering eframe/egui (MIT-or-Apache, no obligations)
  and symphonia (MPL-2.0 file-scope copyleft, used unmodified).

`cargo check`, `cargo fmt --check`, `cargo clippy --all-targets
-- -D warnings`, `cargo test` all clean.

## Previous passes

**Pass M1.0 — M1 contracts + cleanup.**

- Phase A (M0 cleanup): SPEC §23 question 2 struck (resolved at
  M0.5/M0.6 — process-boundary oracle, host never links snes_spc).
  STATUS.md: M0 frozen acceptance SHAs promoted from prose into a
  named-constants block; producer-side caveat added to current
  milestone.
- Phase B1 (§16): Project file format restructured into v1 with
  subsections §16.1 format basics, §16.2 root, §16.3 master_echo,
  §16.4 sample_pool entries (source / loop / playback, ADSR-vs-
  GAIN tagged union, constant-power pan mapping formula), §16.5
  m1 block, §16.6 cross-field validation rules, §16.7 pitch +
  MIDI convention, §16.8 migration + acceptance.
- Phase B2 (§17.2 new): Audition path locks decoded-BRR PCM16
  mono WAV at `build/m1/previews/<sample_id>_decoded_brr.wav`.
  Trap: must play decoded BRR, not the original source.
- Phase B3 (§19.2): IPL upload protocol byte-locked. Port direction
  convention, `$BB` ready signature, `$CC` kickoff, `$2141 = 0`
  jump-to-entrypoint, 8-bit-only writes, IRQ/NMI disabled around
  upload, six bounded host-side spin-count constants.
- Phase B4 (§19.4 new): `module.bin` sparse-block format. 64-byte
  header (magic `"SFCWCM1\0"`, schema_version 1, content SHA-256
  with the 32 SHA bytes themselves zeroed as the self-reference
  workaround), 8-byte block entries, block rules forbidding
  `$00F0..$00FF` and preferring `≥ $0200`.
- Phase B5 (§20.1 new): Driver command protocol for the
  `sample_basic` profile. Ready signature `$A5 $5A $01
  status_flags`; status-flags bit map (bits 0–4 used; 5–7 reserved);
  command_token discipline; commands `$01`/`$02`/`$7F`; acks
  `$81`/`$82`/`$FF`/`$EE`.
- Phase B6 (§21 M1): Acceptance bullets refined to cross-reference
  §16.7, §17.2, §19.4, §20.1, the bundle.status semantics from
  M0, and the .spc/.sfc parity rule (allocated regions
  bit-for-bit; free regions not parity-significant in `.sfc`).
- Phase C (Rust skeletons): Five new `core::` modules — `project`
  (full v1 type tree, `validate` is `todo!()`), `module_image`
  (`ModuleHeader` 64 B + `ModuleBlockEntry` 8 B with
  `std::mem::offset_of!` layout tests), `driver_proto`
  (`DriverCommand` / `DriverAck` enums, `StatusFlags` newtype,
  ready-signature constants, host_timeouts), `pitch`
  (real implementation of the §16.7 formula, 10 tests), and
  `echo_validation` (three SPEC §16.6 rules with multi-error
  collection, 8 tests).

133 tests across the workspace — 104 core unit + 10 BRR fixture
integration + 19 app CLI integration. `cargo check`,
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
and `cargo test` all green.

## Previous passes

**Pass M0.6 — Calibration report + M0 acceptance bundle.**

- Phase A: `core::report::M0Manifest` extended with a real
  `BundleSummary` — per-step `BundleSteps` (`StepStatus =
  ok|warnings|error|skipped`), aggregate `BundleStatus =
  ok|degraded|error`, three cross-reference SHAs
  (`aram_image_sha256`, `spc_file_sha256`, `oracle_pcm_sha256`),
  and a flattened `diagnostics` vector capped at 50 entries.
  `#[serde(default)]` on `bundle` keeps M0.4/M0.5 manifests
  parseable with a sentinel `Error` bundle.
- Phase B: New `core::manifest` module with `verify_bundle` that
  re-reads the on-disk manifest + every report and reports
  observed structure (file presence, parse, schema-version
  consistency, ARAM-SHA cross-reference, SPC-SHA cross-reference).
  Observation-only — never asserts.
- Phase C: `cmd_m0_acceptance` rewritten to chain real steps,
  read each report back, map to `StepStatus` per the documented
  per-step rules, aggregate to `BundleStatus`, fold integrity
  findings into bundle diagnostics, and stamp the manifest with
  an RFC3339 `generated_at` (computed inline via Howard
  Hinnant's civil-from-days, no chrono dep).
- Phase D: New `sfcwc m0-status [--bundle <dir>] [--json]` —
  read-only summary of an existing bundle. Re-runs `verify_bundle`
  to catch on-disk drift, prints the per-step rollup, exits 0 on
  `ok`/`degraded` + clean integrity, 1 otherwise.
- Phase E: SPEC §21 acceptance bullets refined to reference
  `bundle.status` and `m0-status`. M0-completion paragraph
  updated.
- Phase F: Six new CLI integration tests cover bundle
  aggregation paths (all-tools, oracle-missing, asar-missing) and
  m0-status (valid, missing, corrupted bundle), plus four
  manifest unit tests for `verify_bundle`. Two new round-trip
  tests for the manifest schema.

91 tests across the workspace — 62 core unit + 10 BRR fixture
integration + 19 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` all green.

`sfcwc m0-acceptance` on this host produces `bundle.status =
degraded` (Mesen2 missing — `SFCWC_MESEN2` not set; bundle is
shippable but flagged for the optional manual smoke tool). The full
locked SHA constants live in the decisions log below.

## Previous passes

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
- **Bundle status aggregation** (M0.6): required steps are
  doctor, decode_fixtures, assemble, spc_export, aram_map.
  Optional: calibration. Bundle is `ok` iff every required step
  is `ok` AND calibration is `ok`; `error` if any required step
  is `error` or `skipped`; `degraded` otherwise (any required
  step `warnings` OR calibration `warnings`/`error`/`skipped`).
- **Bundle integrity cross-references** (M0.6):
  `assemble.output_image_sha256 == spc_export.input_aram_sha256`
  (driver image flowing into SPC export);
  `spc_export.spc_file_sha256 == calibration.fixture_set.sha256`
  (SPC flowing into oracle render). `verify_bundle` reports both;
  `m0-status` exits 1 if either disagrees.
- **`oracle_pcm_sha256` lives in `CalibrationReport`** (M0.6):
  M0.5 left a "engineer's call" between embed-in-report vs
  reading the wrapper's sidecar JSON. M0.6 chose embed —
  `cmd_calibrate_oracle` computes the SHA on the in-memory PCM
  bytes alongside max_abs/rms; the bundle pulls it from one place
  rather than parsing the wrapper's report.

### M1 implementation decisions (M1.1)

- **GUI binary `sfcwc-app` separate from CLI `sfcwc`** (M1.1):
  one workspace, two `[[bin]]` targets in `app/Cargo.toml`. CI/
  scripted workflows depend on `sfcwc` only and don't pay the
  eframe build cost; interactive use takes `sfcwc-app`.
- **Project file extension `.sfcproj.json`** (M1.1): JSON-honest
  filename plus a `.sfcproj` prefix that flags the file as
  project-shaped. Matches the brief's recommendation.
- **`load_from_path` does not auto-validate** (M1.1): the
  `(load, validate)` two-step lets the GUI render an invalid
  project alongside its errors instead of refusing to load it.
  CLI's `validate-project` and `load_and_validate()` chain the
  two for the happy path.
- **`ValidationError` shape: `{path, kind}`** (M1.1):
  JSON-pointer paths (`/sample_pool/0/loop/end_sample`) so the
  GUI can highlight the offending field. M1.0's flat enum
  replaced.
- **Validation collects every failed rule** (M1.1): no
  bail-on-first. The UI surfaces all problems at once so the
  user fixes everything in one editing pass.
- **Name vs id conventions** (M1.1, locked in SPEC §16.4):
  `name` allows spaces and non-ASCII letters but rejects
  control characters and path separators (`/`, `\`, `:`); `id`
  is `^[a-z0-9_]+$`. Justified in the spec's `id`/`name`
  paragraphs.
- **`ValidationReport` JSON shape** (M1.1, locked in
  `core::report`): `{ schema_version, report_type:
  "validation", project_path, status: ok|invalid|io_error,
  errors: [{path, message}] }`. `errors[]` is a flat
  `{path, message}` shape so JSON consumers don't need to know
  the typed `ValidationErrorKind` enum.
- **symphonia landed at M1.1, not M1.2** (M1.1): brief allowed
  either; landing it now means M1.2 wires only the WAV/AIFF
  probe modules without touching workspace deps. Currently
  declared in `core/Cargo.toml` but unused; the
  `unused_crate_dependencies` lint is allow-by-default so this
  is harmless.
- **Native file pickers deferred** (M1.1): no authorized
  file-picker crate (`rfd` etc.) in the M1.1 dep set, so File →
  Open / Save As / New use a hand-rolled single-line text-input
  modal. M1.2+ revisit with PM authorization.

### M1 contract decisions (M1.0)

These are the M1 contract surfaces frozen at M1.0. Implementation
details that change the byte values, formulas, or named fields are
spec changes, not implementation choices.

- **module.bin format** (SPEC §19.4): magic `"SFCWCM1\0"`,
  `schema_version = 1`, 64-byte header, 8-byte block entries,
  little-endian throughout. Self-reference workaround:
  `content_sha256_zeroed` lives in the header; the literal full-file
  SHA-256 lives in the M1 manifest as `module_file_sha256`.
- **Driver command bytes** (SPEC §20.1): commands `$01` STOP,
  `$02` RESET_TO_IPL, `$7F` PING. Acks `$81` STOP_ACK, `$82`
  RESET_ACK, `$FF` PING_ACK, `$EE` invalid. Ready signature
  `$A5 $5A $01 <status_flags>`.
- **Status flag bit map** (SPEC §20.1): bits 0–4 in use; bits 5–7
  reserved must be zero. Surface lives in `core::driver_proto::StatusFlags`.
- **Pitch formula and round-half-up** (SPEC §16.7): the SNES voice
  pitch is a 14-bit clamped register; rounding is `floor(x + 0.5)`,
  not banker's. M1 reference values: 32 kHz at root → `$1000`,
  22050 Hz at root → `$0B06`. Implemented in `core::pitch`.
- **MIDI convention** (SPEC §16.7): C4 = MIDI 60, A4 = MIDI 69 with
  A4 tuning = 440 Hz. Project files store integer MIDI numbers, not
  note strings.
- **Project v1 schema scope** (SPEC §16): single sample, ADSR or
  GAIN envelope, optional master echo + per-sample echo gate.
  No tracks, no clips, no sequencer — those land at M1.5+ in a
  forward-compatible schema bump.
- **`.spc` / `.sfc` parity rule** (SPEC §21 M1): both exports use
  the same canonical `aram_image` and the same driver entrypoint
  `$0200`. Allocated regions match bit-for-bit. Free regions are
  not parity-significant in `.sfc` because `$00F0–$00FF` is I/O,
  `$0100–$01FF` is stack, and free ARAM is not part of the module.
- **M1.3 audition path** (SPEC §17.2): decoded-BRR mono PCM16 WAV at
  `build/m1/previews/<sample_id>_decoded_brr.wav`. Cross-platform;
  no host audio engine in M1.
- **Bounded host-side spin counts** (SPEC §19.2): six named
  `WAIT_*_POLLS` constants so a missed handshake fails fast rather
  than deadlocks the host. Surface lives in
  `core::driver_proto::host_timeouts`.

### M0 frozen acceptance SHAs

These three SHA-256s identify the M0 acceptance artifact set.
Locked by `m0-acceptance` and re-checked by `m0-status`. Any future
change to the source `.asm`, the SPC exporter state contract, or
the oracle wrapper that alters these SHAs is a producer-side
regression.

```
M0_ARAM_IMAGE_SHA256  = ba728e6fb836e5da4d4d1abec94956f8d92304ce0ac8b768b4103b237c910298
M0_SPC_FILE_SHA256    = 0caba4a35c30c5cadce9585cea3140d17a048f4900721587457813827eea6f51
M0_ORACLE_PCM_SHA256  = 9f1dcbc35c350d6027f98be0f5c8b43b42ca52b7604459c0c42be3aa88913d47
                        (matches SHA-256 of 8,192 zero bytes)
```

## Open questions resolved at M0

- **Embed snes_spc for live preview vs keep oracle-only** (SPEC
  §23, question 2). **Resolved at M0.5/M0.6**: the wrapper is a
  separate-process oracle invoked across a process boundary, per
  `LICENSING.md` §3 and SPEC §17.1. The Apache-2.0 host never
  links snes_spc; embedding for live preview is forbidden on
  licensing grounds. Live preview at M3+ uses the internal Rust
  BRR decoder (§10.1 internal renderer mode), with the oracle
  remaining the calibration second-source.

## Open questions remaining

Cross-reference SPEC §23. Of the four questions there:

1. Final atom quality thresholds — empirical, deferred to calibration runs.
2. ~~Embed snes_spc for live preview vs keep oracle-only — decision target M0.~~ Resolved at M0.5/M0.6: separate-process oracle, host never links it.
3. Practical wavetable frame caps — empirical, may move from 32/64/96/128.
4. Whether a future version adds a free pre-emphasis EQ editor.

## Next pass

**M1.2 — WAV/AIFF/BRR import.** PM to brief. Wires the symphonia
crate (declared at M1.1, unused) for WAV/AIFF probing, plus a
direct BRR import path that consumes the §16.4 sample format. Adds
the import UI to `sfcwc-app` (replacing M1.1's read-only-viewer
mode) and a CLI `import` command.
