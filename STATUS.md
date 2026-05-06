# SFC Wave Compiler — Status

## Current milestone

**M1 in progress — first audible `.spc` shipped.** `sfcwc
compile-spc` produces a 66,048-byte SPC700 v0.30 file that the
snes_spc oracle renders as audible PCM (max_abs=11072, rms=5520
for a synthesized 8000-amp sine fixture, well above the
min_max_abs=1000 / min_rms=200 thresholds). The
`sample_basic` driver assembles to 324 bytes — well under the
4 KiB budget. Real driver replaces the M1.4 zero placeholder
in the packer's driver_code region.

## Last pass

**Pass M1.5 — First real SPC700 driver + audible `.spc`.**

- Phase A: `core/fixtures/asm/m1_sample_basic.asm` — sample_basic
  driver per SPEC §20.1. Init writes FLG mute first, configures
  master vol, DIR, all echo regs (FIR + ESA + EDL before EON),
  then voice 0 (vol/pan/pitch/SRCN/ADSR/GAIN), then unmutes,
  sets dp state, KONs voice 0, writes the `$A5 $5A $01
  status_flags` ready signature. Polling main loop dispatches
  STOP / RESET_TO_IPL / PING / invalid commands. Sentinel
  `$DE $AD $BE $EF` after `driver_end` for length detection.
  Same asar mapper trick as M0.3 (`lorom + arch spc700 + org
  $008200 + base $0200`).
- Phase B: New `core::driver_build` module — generates
  `m1_constants.inc` from project + map, writes scratch
  `.asm` + `.inc` to a tempdir, invokes asar via the existing
  `AsarBackend`, locates the sentinel pattern in the 64 KB
  output, slices driver bytes, bound-checks against
  `DRIVER_CODE_BUDGET_M1` (4 KiB), SHA-256s the result. Driver
  source `include_str!`'d into the core crate so the host has
  no runtime workspace-layout dependency. Per-project constants
  cover voice 0 (constant-power pan formula, M1.0 pitch register,
  ADSR/GAIN tagged-union register mapping), master volume
  (hard-pinned to `$7F` for M1 — schema doesn't expose a
  master-vol field yet), source-directory page (from map),
  echo (FLG ECEN bit, ESA from map, EDL/FIR/EFB/EVOL from
  master_echo, EON gated by per-voice + master).
- Phase C: `compile_aram_image` orchestrates encode → shadow
  pack → driver_build → real pack. Both `sfcwc pack` and the
  new `sfcwc compile-spc` consume it. `--driver` flag on `pack`
  remains as an optional override for testing; default is the
  real driver.
- Phase D: New `sfcwc compile-spc` CLI subcommand. Loads +
  validates project, runs the full pipeline, builds an SPC v0.30
  via the new `core::spc::build_m1_image` (M1 contract: PC=$0200,
  GPRs=0, SP=$EF, DSP regs all zero — driver writes FLG=$60 mute
  on its first instruction). Writes image, map, SPC, and a
  `CompileSpcReport` carrying every SHA. Exit codes 0/1/2/3/4.
- Phase E: New `sfcwc verify-spc-audible` CLI subcommand. Renders
  the SPC through the snes_spc oracle, recomputes max_abs/rms in
  Rust (defensive, mirrors M0.5 calibrate-oracle pattern), writes
  an `AudibleVerificationReport` with `status: ok | silent_fail
  | oracle_error`. Exit 0 ok, 1 IO/oracle error, 2 silent_fail.
- Phase F: GUI "Compile SPC" + "Verify Audible" buttons in the
  project detail panel. Both run in-process. Compile-SPC writes
  to `<project_dir>/.sfcwc-build/<name>.spc`; Verify-Audible
  spawns the oracle and shows max_abs/rms inline.
- Phase G: 14 driver_build unit tests (pan formula, envelope
  mapping, FLG bits, status-flag bits, pitch, src-dir page,
  active-sample resolution, missing-source error, .inc text
  shape) + 5 integration tests (real asar invocation,
  determinism, sample-index encoding, asm-path existence) +
  5 CLI tests (compile-spc happy path, invalid-project,
  verify-audible-ok / silent_fail / oracle-missing) + 2
  round-trip tests for the new report types. **295 tests
  across the workspace; all green.**

### Audible verification result (M1.5 acceptance gate)

```
verify-spc-audible: 16384 frames, max_abs=11072, rms=5519.6,
status=ok; report -> build/m1/<...>.audible-report.json
```

The same project's M0 smoke `.spc` (FLG=$60 mute) correctly
trips `silent_fail` (max_abs=0, rms=0, exit=2).

### M1.5 baseline-locked SHAs

For a one-sample project (8192-frame 8000-amp sine, 32 kHz,
echo off, GAIN=127):

```
M1_5_DRIVER_CODE_SHA256 = 52f9c26bad6df33875f16ae2ef4a6a4e0e5c265e29af74db103b393daef24955
M1_5_ARAM_IMAGE_SHA256  = b384617ff4d45c5da965d27bd74926070eb3b4985fb051bcda25165f7bfb54eb
M1_5_SPC_FILE_SHA256    = edd02b030e1c0f574c4ff1f64dc80f1c643d2ba18628a6f67a35dc93b8759315
```

Driver assembled size: **324 bytes** (out of 4 KiB budget).

### Mesen2 smoke (engineer self-check)

Mesen2 is **not installed on this host** (`SFCWC_MESEN2` unset,
no Mesen executable located). The oracle render is the formal
M1.5 acceptance gate per SPEC §17 / §18 — it confirms the SPC
plays audibly. **User audible audition is queued for return**;
when the user is back at their desktop, opening
`build/m1/<name>.spc` in Mesen2 should produce a sustained
sine tone matching the source (32 kHz at root pitch).

## Previous passes

**Pass M1.4 — ARAM packer v1 + ARAM meter.**

- Phase C: `AramMapReport` extended with four optional M1 meter
  fields — `echo: AramEchoSummary`, `source_directory:
  AramSourceDirSummary`, `samples: AramSamplesSummary`, `warnings`.
  `#[serde(default, skip_serializing_if)]` on all four keeps M0.6
  manifests parseable and round-tripping unchanged.
- Phase A/B/D/E/F: New `core::packer` module — single source of
  truth for M1 region layout. Per-sample BRR pool laid out in
  declaration order at the page after the source directory; each
  source-dir entry packs `(start_addr_le, loop_addr_le)` with
  `loop_addr = start_addr + loop_block * 9` for looped, `=
  start_addr` for one-shots. Returns a `PackOutput` carrying the
  64 KB `aram_image` and the populated `AramMapReport`.
- Phase G CLI: New `sfcwc pack` subcommand. Loads + validates the
  project, decodes audio, encodes BRR via the M1.3 encoder, packs,
  writes image + map. Exit codes 0 / 1 / 2 / 3 for ok / IO / project-
  invalid / pack-error. Stderr summary line lists sample count,
  total BRR bytes, echo state, free %, image SHA, map path.
- Phase G GUI: New ARAM meter view in the project detail panel —
  segmented horizontal bar with one color per region kind, echo
  callout (large text + warning when `writeback_safe=false`),
  numeric region breakdown, warnings list. Recomputes on project
  load, sample edits, loop-candidate apply, and a manual Refresh
  button. Encoder + packer run in-process for sub-second feedback;
  failed recomputes leave the previous meter intact and surface
  the error inline.
- Phase H: 14 packer unit tests (round-trip layout, loop addresses,
  echo math, overflow, alignment, bounds, fixed-region invariance,
  map partition, no-collisions, echo summary consistent with image),
  4 CLI integration tests (happy path, echo overflow → exit 3,
  invalid project → exit 2, default paths), 5 round-trip tests for
  the new report struct types + a pre-M1.4 backwards-parse test.
  269 tests across the workspace.
- Adjacent fix: `core::import` was treating bare-filename project
  paths (no slash) as having parent `Some("")`, which then failed
  to canonicalize and surfaced as a confusing "path traversal
  refused: " message. Now folds empty-parent and `None`-parent both
  to "." consistently.

`cargo check`, `cargo fmt --check`, `cargo clippy --all-targets`,
`cargo test` all green.

### Spec ambiguity flagged

The brief's "echo_end = 0xFFC0" math gives an ESA-misaligned
`echo_start = 0xDFC0` for EDL=4: `ESA*0x100` would point at
`0xDF00`, leaving 0xC0 bytes of the buffer unaddressable. The
correct M1-conservative interpretation is `echo_end = 0xFF00`
(largest page boundary at or below the IPL ROM shadow). The 192
bytes between `0xFF00` and `0xFFC0` are reported as
`ipl_rom_safe_pad`. ESA values in the brief's example (e.g.
`ESA=$DF` for EDL=4) are correct; only the start-address arithmetic
needed re-derivation.

## Previous passes

**Pass M1.3 — Loop selection + BRR encoder + audition WAV.**

- Phase A: `core::audio::decode_to_mono_pcm` wired through
  symphonia for WAV / AIFF / AIFC; chains the existing M0.2
  decoder for BRR. Symphonia stores every PCM sample
  left-aligned in i32 (i16 → `<< 16`, i24 → `<< 8`); we always
  recover i16 with `>> 16`. Stereo collapses to mono via
  per-frame average. Frame count cross-checked against probe.
- Phase B: New `core::brr_encoder` module — exhaustive per-block
  `(filter, shift)` search over 0..=3 × 0..=12; greedy across
  blocks; round-trip-correct by construction (the encoder's
  analytic predictor walk mirrors `core::brr::decode_block`,
  then re-decodes the produced block via the canonical decoder
  to score). Scoring: peak first, sum-of-squares as tiebreak —
  RMS-only scoring picks shifts that clip on signal peaks, and
  clipping is what makes BRR samples sound distorted.
  `force_filter_0_first_block` (default true) protects against
  uninitialised predictor history at KON; `loop_entry_block_index`
  forces filter 0 at the loop entry block where the predictor
  history at iteration ≥ 2 has no fixed value. Sine-wave
  round-trip peak < 256 LSBs (observed: 55).
- Phase C: New `core::loop_finder` module — windowed RMS
  difference + seam click magnitude, score = rms + 0.25 * click.
  Search restricted to first 25% / last 25%; iteration on
  multiples of 16 when `snap_to_brr_block` is on.
- Phase D: New `core::audition` module — hand-rolled 44-byte
  RIFF/WAVE writer for byte-stable output. Sample data is the
  raw 15-bit decoder output stored in i16; no gain compensation
  applied, so the audition reflects the S-DSP at unity voice gain.
- Phase E: Three new CLI subcommands (`encode-brr`,
  `preview-brr`, `find-loop-candidates`) plus three new report
  types (`BrrEncodeReport`, `LoopFinderReport`, `AuditionReport`)
  on the existing v1 envelope.
- Phase F: GUI gains loop start/end DragValues (auto-snap to
  multiples of 16), Find Loop Candidates button + modal with
  per-row Apply, Preview BRR button (writes to
  `<project_dir>/.sfcwc-preview/<sample_id>.audition.wav`).
  Sample detail panel switched from `&SampleSlot` to
  `&mut SampleSlot`; project re-validates after each edit.
- Phase G: 25 new tests — 10 encoder unit (round-trip,
  silence, force_filter_0, loop_entry, looped flags, alignment),
  6 loop_finder unit (sortedness, range, snap, top-N), 5
  audition unit (header layout, byte-stability, payload
  matches decoder), 4 decode-to-PCM integration (mono, stereo
  mix, zero-fill, BRR), 3 round-trip serde tests for the new
  report types, 3 CLI integration tests for the new
  subcommands. 246 tests across the workspace; `cargo check`,
  `cargo fmt --check`, `cargo clippy --all-targets`, and
  `cargo test` all green.

## Previous passes

**Pass M1.2 — WAV/AIFF/BRR import.**

- Phase A: New `core::audio` module with hand-rolled probes for
  WAV (RIFF/WAVE chunk walk), AIFF / AIFC (IFF FORM chunk walk
  with NONE/sowt compression validation), and BRR (file-size /
  9 = block count). 80-bit IEEE 754 extended-precision sample-
  rate decoder cross-pinned against four pre-computed byte
  tables (8000 / 22050 / 32000 / 44100 Hz). `sha256_of_file`
  streams 64 KiB chunks.
- Phase B: New `core::import` module wires the full pipeline:
  load → probe → SHA → copy (with same-SHA dedup + collision
  suffix) → derive id/name → default `SampleSlot` (root MIDI
  60, loop off, GainRaw 127, vol 1.0, pan 0.0, echo off) →
  append → seed `m1.active_sample_id` on first import →
  validate → save. Path-traversal guard refuses targets that
  resolve outside `<project_dir>/audio/`.
- Phase C: New `sfcwc import` CLI subcommand. Exit codes 0 / 1
  / 2 / 3 for ok / IO-or-project-error / audio-format-rejected
  / resulting-project-invalid (defensive).
- Phase D: New File → Import Audio… menu item in `sfcwc-app`.
  Opens `rfd::FileDialog` with the wav/aif/aiff/aifc/brr
  extension filter, reloads the project on success, surfaces
  errors via the existing status bar.
- Phase E: 12 audio probe tests (all 9 brief cases plus extras
  for AIFC NONE/sowt and the SHA-256 streaming vector), 9
  import pipeline tests (happy path, dedup-by-SHA, collision
  suffix, --no-copy relative path, BRR sample-rate override,
  messy-filename id derivation, fallback id, missing-file,
  unsupported-extension), 3 CLI integration tests (exit 0 / 1
  / 2). Synth helpers in `core/tests/common/mod.rs`; no
  binary fixtures committed. 6 unit tests on the id-derivation
  logic.
- Phase F: LICENSING.md §4.2 documents `rfd` (dual MIT-OR-Apache,
  no obligations).

215 tests across the workspace — 157 core unit + 12 audio probe
integration + 10 BRR fixture integration + 9 import pipeline
integration + 27 app CLI integration. `cargo check`, `cargo fmt
--check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test` all green. `rfd` compiled clean on Windows with the
brief's feature set; no fallback to text-input modals required.

## Previous passes

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

### M1 driver / compile-spc / audible-verify decisions (M1.5)

- **Driver constants format: asar `name = $XX` (label assignment)**
  (M1.5). The `m1_constants.inc` file uses asar's `name = value`
  syntax (label numeric assignment); `mov $f3, #voice0_voll`
  resolves to `mov $f3, #$XX` at assemble time. Cleaner than
  `!define`, no usage-site `!` prefix.
- **Driver end marker: 4-byte sentinel `$DE $AD $BE $EF`**
  (M1.5). Single-byte sentinels collide with legitimate
  instruction bytes (the driver contains `$EE` and `$FF` in
  immediate fields). The 4-byte pattern is distinctive enough
  to find unambiguously by linear scan from offset $0200, and
  lets driver_build avoid teaching itself the SPC700 instruction
  encoding.
- **Master volume hard-pinned to `$7F`** (M1.5). The §16.4
  schema doesn't expose a project-level master_voll/master_volr
  field. M1 ships max-loud and documents the deviation; if a
  future pass adds project master volume, that's a non-breaking
  schema extension with default = $7F.
- **SPC initial DSP state: all zero, driver writes FLG=$60 first**
  (M1.5). Matches what happens on real hardware after IPL upload
  — the SPC starts with whatever DSP state the BIOS left, the
  driver mutes immediately on its first instruction, and unmutes
  later in init after voices/echo/source-dir are programmed.
  Simpler than computing DSP regs at compile time + identical
  observable behavior after the first ~4 instructions.
- **Audible verification thresholds: `min_max_abs=1000`,
  `min_rms=200`** (M1.5). For a unity-gain sine sample at MID
  amplitude these are easily exceeded (observed 11072 / 5520).
  `silent_fail` correctly catches the M0 muted smoke (0 / 0).
  Tunable via CLI flags. M1.6+ may tighten or add per-genre
  thresholds.
- **Driver-source embedding via `include_str!`** (M1.5). The
  canonical `m1_sample_basic.asm` is built into the host crate
  at compile time; runtime invocation writes it to a tempdir
  alongside the generated `m1_constants.inc` for asar. Means
  the host has zero filesystem-layout assumptions and the
  driver source ships in every binary.
- **`tempfile` promoted from dev-dep to regular dep** (M1.5).
  Host orchestration (CLI + GUI compile-spc paths) needs a
  scratch directory for asar; tempfile was already in the
  workspace, so this is a re-classification not a new crate.
- **Empirical DSP register write order** (M1.5): mute (FLG=$60)
  → clear KON/KOFF → master vol → DIR → echo params (FIR / ESA /
  EDL) → EON last → voice 0 regs → unmute (FLG=running) → DP
  state → KON → ready signature. Echo params before EON so the
  buffer pointer is set before any echo write can reach RAM.
  Mute around the whole init so DAC stays silent through the
  ~50 µs setup.

### M1 packer / ARAM meter decisions (M1.4)

- **Driver code budget M1.4: 4 KiB at $0200..$1200** (`DRIVER_CODE_BUDGET_M1`).
  M1.4 fills with zeros (placeholder). M1.5 ships the real
  assembled `sample_basic` driver in the same window. The budget
  is round-aligned to the page boundary so the source directory
  always lands at `$1200` (`DIR` register = `$12`) regardless of
  the actual driver size.
- **Source directory placement: page-aligned, padded to next page**
  (M1.4): 4 bytes/entry (start_addr_le, loop_addr_le). The page-
  pad rolls into the `source_directory` region in the map so the
  BRR pool always starts on a clean page boundary.
- **Echo placement: page-aligned, ends at `$FF00`** (M1.4): largest
  page boundary at or below the `$FFC0` IPL ROM shadow. ESA =
  `echo_start >> 8` is well-defined for any EDL ∈ 1..=15. The 192
  bytes `[$FF00, $FFC0)` are reported as `ipl_rom_safe_pad`. The
  brief's "echo_end = 0xFFC0" was ESA-misaligned and corrected
  here.
- **Multi-sample layout: single contiguous BRR pool** (M1.4). No
  inter-sample gaps, no per-sample regions in the map (only one
  `sample_brr_pool` region; per-sample bounds live in
  `samples.per_sample`). M2+ may split when atom pools land.
- **Loop addresses: `start_addr + loop_block * 9` for looped;
  `loop_addr = start_addr` for one-shots** (M1.4). The S-DSP's
  END flag in the last block handles non-loop termination; loop
  flag handles wrap-back.
- **Map report extensions are non-breaking** (M1.4): all four new
  fields (`echo`, `source_directory`, `samples`, `warnings`) are
  `Option`/`Vec` with `serde(default, skip_serializing_if)`.
  M0.6 manifests still parse, M0 acceptance + `m0-status`
  unaffected. `SCHEMA_VERSION` stays at 1.
- **Meter recompute is synchronous, in-process** (M1.4): the GUI
  re-runs decode + encode + pack on the UI thread when the project
  changes. Sub-second for realistic projects; no `Command::new`
  overhead. Failed recomputes leave the prior meter visible and
  surface the error inline.

### M1 encoder / loop / audition decisions (M1.3)

- **Encoder scoring: peak-first, RMS-tiebreak** (M1.3): brief
  said exhaustive `(filter, shift)` search; consultant didn't pin
  the cost function. Pure RMS scoring picks shifts that
  cleanly encode the bulk of the block but clip at signal peaks
  (4-bit nibbles × shift step can't span large amplitudes), and
  clipping at peaks is what makes BRR samples sound distorted.
  Peak first, sum-of-squares as tiebreak, avoids that cleanly.
  Empirical: amp-8000 sine round-trip peak drops from 793 to 55
  LSBs.
- **`force_filter_0_first_block` default true** (M1.3):
  conservative safety against predictor-history glitches at
  KON. The S-DSP documents prev1=prev2=0 reset on KON, so
  filters 1..=3 on block 0 are normally safe; default-on is
  the consultant's belt-and-suspenders advice. The `EncodeOptions`
  toggle lets callers (round-trip tests, advanced users)
  disable it when block-0 quality matters more than safety.
- **Symphonia i32 path: always `>> 16`** (M1.3):
  `SampleBuffer<i32>` left-aligns every PCM sample in i32
  regardless of source bit depth (i16 → `<< 16`, i24 → `<< 8`,
  i32 → identity per the symphonia-core::conv impl_convert
  table). To recover an i16 sample, drop the low 16 bits — the
  shift is fixed, not heuristic.
- **Loop finder: first-25% / last-25% search ranges** (M1.3):
  matches typical sustained-instrument loop shapes (attack →
  loop region → release implicit). Score = rms_window_diff +
  0.25 × seam_click — the click weight is intentionally below
  the RMS so a high-RMS / zero-click pair never beats a
  low-RMS / small-click pair.
- **Audition WAV: raw 15-bit output, no gain compensation**
  (M1.3): `core::audition` writes the S-DSP's natural 15-bit
  output stored in i16 (range -16384..=16383), no scaling. The
  audition reflects what playback would emit at unity voice
  gain; user applies makeup gain in their DAW if needed.
- **Encoder pads to multiples of 16 internally** (M1.3): brief
  allowed either "encoder errors on unaligned input" or
  "encoder pads with zeros." Pads internally so the CLI and
  GUI don't have to repeat the rounding logic. `encode_looped`
  still rejects unaligned `loop_start_sample` because that's a
  meaningful contract (BRR loop alignment), unlike trailing-pad
  which is just bookkeeping.

### M1 import decisions (M1.2)

- **Copy-into-project default ON** (M1.2): `sfcwc import` and
  the GUI both default `copy_into_project = true`. CLI exposes
  `--no-copy` for the opt-out. Rationale: a project is a
  self-contained source-of-truth; relying on external paths is
  fragile across machines.
- **SHA-match dedup on re-import** (M1.2): if `<project_dir>/
  audio/<filename>` already exists with the same SHA-256 as the
  freshly-computed source, the existing file is reused — no `_2`
  copy. Different SHA with the same filename suffixes `_2`, `_3`,
  … on the file. Either way, the new sample slot gets a unique
  `id` (also suffixed on collision).
- **Path-traversal guard** (M1.2): the canonicalized `<project_dir>/
  audio/` target must start with the canonicalized project
  directory; anything else (symlink-out, absolute-path injection)
  refuses with `ImportError::PathTraversal`. Honest about Windows
  UNC paths: canonicalize handles `\\?\` prefixes uniformly on
  both sides of the comparison.
- **Default `SampleSlot` fields** (M1.2): root MIDI 60 (C4),
  loop disabled with `start_sample` / `end_sample` / `snap` all
  `None`, `playback.envelope = GainRaw { gain_byte: 127 }`,
  volume 1.0, pan 0.0, echo off. ADSR is reserved for M1.3+ once
  loop selection lands; M1.2 deliberately keeps the envelope the
  raw-byte form to avoid premature commitment.
- **First-sample auto-binds `m1.active_sample_id`** (M1.2): if
  the pool was empty before the import, the new sample's id is
  written into `m1.active_sample_id` so the project validates
  immediately. Subsequent imports leave `active_sample_id`
  alone.
- **`id` derivation rule** (M1.2): filename stem → ASCII
  lowercase → any non-`[a-z0-9]` codepoint folds to `_` (runs of
  separators collapse) → leading/trailing `_` trimmed →
  truncate to 64 chars → fall back to `sample_<N>` if empty →
  uniquify with `_2`, `_3`, … on collision. Matches the SPEC
  §16.4 `^[a-z0-9_]+$` constraint.
- **WAV bit-depth: 8 / 16 / 24 int only** (M1.2): float WAV
  (`WAVE_FORMAT_IEEE_FLOAT = 0x0003`) and 32-bit int rejected.
  WAVE_FORMAT_EXTENSIBLE follows the SubFormat GUID's leading
  two bytes back to a legacy tag.
- **AIFF compression: NONE and sowt only** (M1.2): plain AIFF
  (no compression field) accepted as uncompressed BE PCM.
  AIFC accepted only when the 4-byte compression code is
  `NONE` or `sowt`. ima4, ulaw, MAC3/MAC6, etc. rejected.
- **BRR file-size invariant** (M1.2): file size must be a
  positive multiple of 9 bytes. `sample_rate_hz` defaults to
  32000 Hz (the canonical S-DSP rate); CLI exposes
  `--brr-sample-rate <Hz>` and `ImportOptions::brr_sample_rate_hz`
  to override.
- **No binary audio fixtures committed** (M1.2): every test
  synthesizes its WAV/AIFF/AIFC/BRR fixture in a `tempfile::
  TempDir` at runtime via `core/tests/common/mod.rs`. The
  AIFF synth uses the four pre-computed 80-bit IEEE 754
  extended-precision byte tables for the rates the tests
  exercise; runtime extended-80 encoding is deferred to M1.3+
  if a future test needs it.
- **`rfd` authorized** (M1.2): native file dialogs across
  Windows/macOS/Linux with `xdg-portal + tokio` features; the
  M1.1 hand-rolled text-input modals stay in place for File →
  Open / Save As / New (those still need addressing) but
  Import goes straight to the native picker.

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

**M1.6 — `.sfc` test ROM + module swap.** PM to brief.
With audible `.spc` shipped, the next surface is the 65816 IPL
upload contract (SPEC §19.2) so the same compiled ARAM image
boots through a real `.sfc` cartridge container.
