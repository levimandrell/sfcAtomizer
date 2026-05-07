//! `sfcwc` — host CLI for the SFC Wave Compiler M0 harness.
//!
//! M0.1 ships shape only: real tool resolution in `doctor`; stub
//! reports for the other subcommands. Substance lands in M0.2–M0.6.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sfc_atomizer_core::aram::{map_from_image, ARAM_LEN};
use sfc_atomizer_core::asm::{
    sha256_hex, sha256_hex_file, AsarBackend, AssembleError, AssembleInput, AssemblerBackend,
};
use sfc_atomizer_core::atom::{render_to_brr, AtomBrrOutput, AtomKind, AtomSlot};
use sfc_atomizer_core::audio::{decode_to_mono_pcm, probe, AudioDecodeError, AudioFormat};
use sfc_atomizer_core::audition::export_decoded_brr_wav;
use sfc_atomizer_core::brr_encoder::{encode as brr_encode, encode_looped, EncodeOptions};
use sfc_atomizer_core::brr_fixtures::{run_fixture, M0_RAW_DECODE_FIXTURES};
use sfc_atomizer_core::driver_build::{build as driver_build, DriverBuildInput};
use sfc_atomizer_core::import::{import_audio, ImportError, ImportOptions};
use sfc_atomizer_core::loop_finder::{find_loop_candidates, LoopFinderOptions};
use sfc_atomizer_core::manifest::{verify_bundle, verify_m1_bundle, M1BundleIntegrity};
use sfc_atomizer_core::module_writer::{
    parse_module_blocks, parse_module_header, project_blocks_to_aram, recompute_in_file_sha,
    MODULE_MAGIC,
};
use sfc_atomizer_core::packer::{pack as packer_pack, EncodedSample, PackInput};
use sfc_atomizer_core::project::{ProjectIoError, ProjectV1, ValidationError};
use sfc_atomizer_core::project_v2::{
    load_project_versioned, migrate_from_v1, LoadedProject, MigrationReport,
};
use sfc_atomizer_core::report::{
    AramKind, AramMapReport, AssembleReport, AssembleStatus, AtomRenderReport, AudibleStatus,
    AudibleThresholds, AudibleVerificationReport, AuditionReport, BrrEncodeBlock, BrrEncodeReport,
    BrrFixtureReport, BundleStatus, BundleSteps, BundleSummary, CalibrationReport,
    CalibrationStatus, CompileSfcReport, CompileSpcReport, DoctorReport, DoctorStatus, DoctorTools,
    FixtureSetInfo, LoopCandidateJson, LoopFinderReport, M0Manifest, M1BundleSteps,
    M1BundleSummary, M1Manifest, ObservedAudio, ObservedInfo, OracleInfo, ProvisionalTolerances,
    RenderInfo, RustInfo, SequenceCompileReport, SfcFinding, SfcHeaderSummary, SfcModuleSummary,
    SfcModulesAudibleReport, SfcStructureReport, SfcStructureStatus, SpcExportReport,
    SpcInitialState, SpcStatus, StepStatus, ToolStatus, ValidationErrorJson, ValidationReport,
    ValidationStatus, SCHEMA_VERSION,
};
use sfc_atomizer_core::sfc_export::{
    export_sfc, SfcExportInput, LOROM_HEADER_BASE, LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET,
    LOROM_HEADER_CHECKSUM_OFFSET, LOROM_HEADER_MODE_OFFSET, LOROM_HEADER_RESET_VECTOR_OFFSET,
    LOROM_HEADER_TITLE_LEN, MODULE_A_FILE_OFFSET, MODULE_B_FILE_OFFSET,
};
use sfc_atomizer_core::spc::{
    build_m1_image, build_smoke_image, verify_structure, SpcCpuState, SpcImage, SMOKE_CPU_STATE,
    SPC_ARAM_SIZE, SPC_FILE_SIZE,
};
use sfc_atomizer_core::tools::{self, ResolvedTool, ToolSource};
use thiserror::Error;

#[derive(Parser)]
#[command(name = "sfcwc", version, about = "SFC Wave Compiler — M0 host CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Resolve external tools and emit a doctor report.
    Doctor {
        /// Print the doctor report as JSON to stdout.
        #[arg(long)]
        json: bool,
        /// Also write the JSON report to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run the BRR fixture suite (M0.1: empty stub report).
    DecodeFixtures {
        #[arg(long, default_value = "build/m0/brr-fixture-report.json")]
        out: PathBuf,
    },
    /// Smoke-test the asar backend: assemble `--source` into a 64 KB
    /// ARAM image at `--out-image`, write the report to `--out`.
    AssembleSmoke {
        #[arg(long)]
        source: PathBuf,
        #[arg(long, default_value = "build/m0/assemble-report.json")]
        out: PathBuf,
        #[arg(long, default_value = "build/m0/driver.bin")]
        out_image: PathBuf,
    },
    /// Wrap an assembled 64 KB ARAM image in an SPC v0.30 file with
    /// the M0 smoke initial-state contract (SPEC §19.3).
    ExportSpcSmoke {
        #[arg(long, default_value = "build/m0/driver.bin")]
        aram: PathBuf,
        #[arg(long, default_value = "build/m0/spc-export-report.json")]
        out: PathBuf,
        #[arg(long, default_value = "build/m0/smoke.spc")]
        out_spc: PathBuf,
        /// Re-read the produced SPC and assert structural invariants.
        #[arg(long)]
        verify_structure: bool,
    },
    /// Render the M0 smoke `.spc` through the snes_spc oracle wrapper
    /// and emit a calibration report.
    CalibrateOracle {
        /// Override `SFCWC_SNES_SPC_ORACLE` and the workspace defaults.
        #[arg(long)]
        oracle: Option<PathBuf>,
        #[arg(long, default_value = "build/m0/smoke.spc")]
        input_spc: PathBuf,
        #[arg(long, default_value_t = 2048u32)]
        frames: u32,
        #[arg(long, default_value = "build/m0/calibration-report.json")]
        out: PathBuf,
        #[arg(long, default_value = "build/m0/oracle.pcm_s16le")]
        out_pcm: PathBuf,
    },
    /// Run all M0 acceptance steps and write a manifest pointing at the reports.
    M0Acceptance {
        #[arg(long, default_value = "build/m0")]
        out: PathBuf,
    },
    /// Read-only summary of an existing M0 acceptance bundle.
    ///
    /// Re-runs the integrity check against the on-disk bundle, prints
    /// the per-step rollup, and exits 0 if `bundle.status` is `ok` or
    /// `degraded`, 1 otherwise.
    M0Status {
        #[arg(long, default_value = "build/m0")]
        bundle: PathBuf,
        /// Print the manifest as JSON to stdout instead of the
        /// human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Write a minimal pre-import M1 project template (SPEC §16 v1).
    ///
    /// The template fails validation by design: empty `sample_pool`
    /// (rule #9 wants 1..=128) and empty `m1.active_sample_id`
    /// (rule #25). The user runs `import` (M1.2) to add samples
    /// before the project validates.
    NewProject {
        #[arg(long, default_value = "project.sfcproj.json")]
        out: PathBuf,
        /// Project name. Defaults to the `--out` filename stem.
        #[arg(long)]
        name: Option<String>,
    },
    /// Validate a project file (SPEC §16.6 rules).
    ///
    /// Exits 0 when valid, 2 when validation errors are present, 1
    /// on IO/parse errors.
    ValidateProject {
        #[arg(long)]
        project: PathBuf,
        /// Print a structured `ValidationReport` to stdout.
        #[arg(long)]
        json: bool,
        /// Also write the report JSON to a file.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run the full M1 acceptance pipeline (M1.7) and write a
    /// bundle manifest with aggregate `BundleStatus` semantics
    /// analogous to `m0-acceptance`. Always exits 0; bundle status
    /// reflects each step's outcome.
    M1Acceptance {
        #[arg(long)]
        project_a: PathBuf,
        #[arg(long)]
        project_b: Option<PathBuf>,
        #[arg(long, default_value = "build/m1")]
        out: PathBuf,
        #[arg(long, default_value_t = 16384u32)]
        frames: u32,
    },
    /// Read-only summary of an existing M1 acceptance bundle. Exits
    /// 0 on `bundle.status` ok/degraded + clean integrity, 1 otherwise.
    M1Status {
        #[arg(long, default_value = "build/m1")]
        bundle: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Compile one or two projects into a LoROM `.sfc` test ROM (M1.6).
    ///
    /// With only `--project-a`, the loader uploads a single module
    /// twice (module B is a clone of A) so the swap mechanism is
    /// still exercised. Exit codes: 0 ok, 1 IO/parse, 2 project-
    /// invalid, 3 pack error, 4 module-write error, 5 sfc-build
    /// error.
    CompileSfc {
        #[arg(long)]
        project_a: PathBuf,
        #[arg(long)]
        project_b: Option<PathBuf>,
        #[arg(long)]
        out_sfc: Option<PathBuf>,
        #[arg(long)]
        out_report: Option<PathBuf>,
        /// M2.0 (consultant #3): when set, recompute and persist
        /// each sample's `source.sha256` in the project file
        /// before compiling. Default off — without this flag, a
        /// SHA mismatch between project file and live source is
        /// a hard error.
        #[arg(long)]
        refresh_source_hash: bool,
    },
    /// Verify the structural integrity of a LoROM `.sfc`: header,
    /// vectors, embedded `module.bin` parsing + in-file SHA match.
    /// Exit codes: 0 ok, 1 IO error, 2 structural failure.
    VerifySfcStructure {
        #[arg(long)]
        sfc: PathBuf,
        #[arg(long)]
        out_report: Option<PathBuf>,
    },
    /// Cross-check audio: parse each embedded module, project blocks
    /// back into a 64 KB ARAM image, wrap as M1 SPC, render via
    /// snes_spc oracle, assert non-silent. Exit codes: 0 ok, 1
    /// IO/oracle error, 2 silent_fail (any module silent).
    VerifySfcModulesAudible {
        #[arg(long)]
        sfc: PathBuf,
        #[arg(long, default_value_t = 16384u32)]
        frames: u32,
        #[arg(long)]
        out_report: Option<PathBuf>,
        #[arg(long, default_value_t = 1000u32)]
        min_max_abs: u32,
        #[arg(long, default_value_t = 200.0f64)]
        min_rms: f64,
        #[arg(long)]
        oracle: Option<PathBuf>,
        /// Optional: write module A's rendered PCM as a 32 kHz mono
        /// PCM16 WAV alongside the report. Useful for offline
        /// audition / commit-visible artifacts.
        #[arg(long)]
        out_wav_a: Option<PathBuf>,
        /// Same for module B.
        #[arg(long)]
        out_wav_b: Option<PathBuf>,
    },
    /// Compile a project end-to-end into an audible `.spc` file (M1.5).
    ///
    /// Encode → driver_build → pack → SPC export with the M1
    /// state contract (PC=$0200, GPRs=0, SP=$EF, DSP regs=0;
    /// driver writes FLG=$60 mute on its first instruction).
    /// Exit codes: 0 ok, 1 IO/parse, 2 project-invalid, 3 pack
    /// error, 4 driver-build error.
    CompileSpc {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        out_spc: Option<PathBuf>,
        #[arg(long)]
        out_image: Option<PathBuf>,
        #[arg(long)]
        out_map: Option<PathBuf>,
        #[arg(long)]
        out_report: Option<PathBuf>,
        /// M2.0 (consultant #3): refresh + persist `source.sha256`
        /// before compile when the live SHA differs from the
        /// project's recorded SHA. Default off.
        #[arg(long)]
        refresh_source_hash: bool,
    },
    /// Render a `.spc` through the snes_spc oracle and assert
    /// audible (non-silent) output (M1.5).
    ///
    /// Exit codes: 0 ok, 1 IO / oracle error, 2 silent_fail
    /// (driver muted or KON missed).
    VerifySpcAudible {
        #[arg(long)]
        spc: PathBuf,
        #[arg(long, default_value_t = 16384u32)]
        frames: u32,
        #[arg(long)]
        out_report: Option<PathBuf>,
        #[arg(long)]
        out_pcm: Option<PathBuf>,
        #[arg(long, default_value_t = 1000u32)]
        min_max_abs: u32,
        #[arg(long, default_value_t = 200.0f64)]
        min_rms: f64,
        /// Override `SFCWC_SNES_SPC_ORACLE` and the workspace
        /// default (same shape as `calibrate-oracle`).
        #[arg(long)]
        oracle: Option<PathBuf>,
        /// Optional: write the rendered PCM as a 32 kHz mono PCM16
        /// WAV. The oracle output is interleaved-stereo 16-bit at
        /// 32 kHz; the WAV-side helper averages L+R per frame.
        #[arg(long)]
        out_wav: Option<PathBuf>,
    },
    /// Pack a project into a 64 KB ARAM image + map report (M1.4).
    ///
    /// Loads + validates the project, encodes each sample's BRR via
    /// the M1.3 encoder, runs `core::packer::pack`, writes the
    /// resulting image and map JSON.
    ///
    /// Exit codes: 0 ok, 1 IO/parse, 2 validation, 3 pack error.
    Pack {
        #[arg(long)]
        project: PathBuf,
        #[arg(long, default_value = "build/m1/aram-image.bin")]
        out_image: PathBuf,
        #[arg(long, default_value = "build/m1/aram-map.json")]
        out_map: PathBuf,
        /// Optional pre-built driver-code blob. Defaults to a 4 KB
        /// zero-filled placeholder (M1.4 only — M1.5 ships a real
        /// driver).
        #[arg(long)]
        driver: Option<PathBuf>,
        /// M2.0 (consultant #3): refresh + persist `source.sha256`
        /// before pack. Default off.
        #[arg(long)]
        refresh_source_hash: bool,
        /// M2.3: write the capability manifest sidecar to this path.
        /// Defaults to `<out_map>.capability-manifest.json` next to
        /// the ARAM map.
        #[arg(long)]
        out_capability_manifest: Option<PathBuf>,
    },
    /// Encode a WAV / AIFF / BRR audio file to BRR bytes (M1.3).
    ///
    /// Decodes via the same path as `import` (mono mix, 16-bit PCM),
    /// runs the M1 BRR encoder, and writes both the `.brr` byte file
    /// and a structured `BrrEncodeReport`.
    EncodeBrr {
        #[arg(long)]
        audio: PathBuf,
        #[arg(long)]
        out_brr: PathBuf,
        #[arg(long)]
        out_report: Option<PathBuf>,
        /// If set, encode as a looped sample with the loop entry at
        /// this sample index (must be a multiple of 16).
        #[arg(long)]
        loop_start_sample: Option<u32>,
        /// Allow filter 1..=3 on block 0. Default forces filter 0 for
        /// safety against predictor history at KON.
        #[arg(long)]
        no_force_filter_0_first_block: bool,
    },
    /// Decode a BRR file to a 16-bit mono PCM WAV for offline preview.
    PreviewBrr {
        #[arg(long)]
        brr: PathBuf,
        #[arg(long)]
        out_wav: PathBuf,
        #[arg(long)]
        out_report: Option<PathBuf>,
        #[arg(long, default_value_t = 32000u32)]
        sample_rate_hz: u32,
    },
    /// Search for sustain-loop candidates in a sample.
    FindLoopCandidates {
        #[arg(long)]
        audio: PathBuf,
        #[arg(long)]
        out_report: PathBuf,
        #[arg(long, default_value_t = 32u32)]
        window_samples: u32,
        #[arg(long, default_value_t = 8u32)]
        max_candidates: u32,
        #[arg(long)]
        no_snap_to_brr_block: bool,
    },
    /// Import a WAV / AIFF / BRR audio file as a new sample-pool entry.
    ///
    /// Default behaviour copies the source into `<project_dir>/audio/`
    /// and rewrites the project. Pass `--no-copy` to skip the copy
    /// (project records the source path as-is).
    Import {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        audio: PathBuf,
        /// Override the auto-derived sample id.
        #[arg(long)]
        id: Option<String>,
        /// Override the auto-derived sample name.
        #[arg(long)]
        name: Option<String>,
        /// Don't copy the audio into `<project_dir>/audio/`.
        #[arg(long)]
        no_copy: bool,
        /// For BRR imports, override the default 32 kHz sample rate.
        #[arg(long)]
        brr_sample_rate: Option<u32>,
    },
    /// Migrate a v1 project to v2 (SPEC §16.10).
    ///
    /// Validates the v1 input, transforms per the §16.10 mapping,
    /// validates the resulting v2, and writes both the migrated
    /// project and a migration report. Migrations are explicit and
    /// one-way; load-time silent upgrades are forbidden (§16.10).
    ///
    /// Exit codes: 0 success, 1 IO/parse, 2 v1 validation OR input
    /// already at v2, 3 post-migration v2 validation.
    MigrateProject {
        /// Path to the v1 input project.
        #[arg(long, value_name = "PATH")]
        r#in: PathBuf,
        /// Path to write the v2 output project.
        #[arg(long)]
        out: PathBuf,
        /// Optional path to write the migration report. Default:
        /// `<out_stem>.migration-report.json` next to `--out`.
        #[arg(long)]
        migration_report: Option<PathBuf>,
    },
    /// Render a single atom from a v2 project to BRR (M2.2).
    ///
    /// Loads + validates the v2 project, finds the atom by id,
    /// runs the SPEC §16.9 render formula, encodes through the M1
    /// BRR encoder, writes the BRR bytes and a structured
    /// `AtomRenderReport`. Optional `--out-pcm` writes the raw
    /// pre-encode PCM as s16le bytes.
    ///
    /// Exit codes: 0 success, 1 IO/parse, 2 v1 project / atom not
    /// found / project invalid, 3 render error (currently impossible).
    RenderAtom {
        #[arg(long)]
        project: PathBuf,
        /// Atom id to render (must exist in `atom_pool[]`).
        #[arg(long)]
        atom: String,
        #[arg(long)]
        out_brr: Option<PathBuf>,
        #[arg(long)]
        out_report: Option<PathBuf>,
        /// Optional: write raw pre-encode PCM as s16le bytes.
        #[arg(long)]
        out_pcm: Option<PathBuf>,
    },
    /// Compile an atom_sequence to SEQ2 bytecode (M2.4).
    ///
    /// Loads + validates the v2 project, looks up the active
    /// sequence (or `--sequence-id`), runs the SPEC §14.3 lowering,
    /// writes the .seq.bin and a structured `SequenceCompileReport`.
    /// The compile path is the same as the one M2.3's pack uses;
    /// this CLI exists for engineer / CI inspection.
    ///
    /// Exit codes: 0 success, 1 IO/parse, 2 project-invalid /
    /// sequence-not-found / capability-missing, 3 compile error
    /// (budget, overlap, too-large).
    CompileSequence {
        #[arg(long)]
        project: PathBuf,
        /// Atom sequence id to compile. Defaults to
        /// `m2.active_sequence_id` from the project.
        #[arg(long)]
        sequence_id: Option<String>,
        #[arg(long)]
        out_bin: Option<PathBuf>,
        #[arg(long)]
        out_report: Option<PathBuf>,
    },
    /// Decode a rendered atom and write a looped audition WAV (M2.2).
    ///
    /// Renders the atom (same path as `render-atom`), decodes the
    /// BRR bytes, repeats the cycle to fill `--duration-seconds` at
    /// 32 kHz, writes a 32 kHz mono PCM16 WAV. Engineer-side audition
    /// only; not committed to the repo.
    PreviewAtom {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        atom: String,
        #[arg(long, default_value_t = 2.0_f64)]
        duration_seconds: f64,
        #[arg(long)]
        out_wav: Option<PathBuf>,
    },
}

#[derive(Debug, Error)]
enum CliError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not determine current directory: {0}")]
    Cwd(std::io::Error),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Doctor { json, out } => cmd_doctor(json, out.as_deref()),
        Command::DecodeFixtures { out } => cmd_decode_fixtures(&out),
        Command::AssembleSmoke {
            source,
            out,
            out_image,
        } => cmd_assemble_smoke(&source, &out, &out_image),
        Command::ExportSpcSmoke {
            aram,
            out,
            out_spc,
            verify_structure,
        } => cmd_export_spc_smoke(&aram, &out, &out_spc, verify_structure),
        Command::CalibrateOracle {
            oracle,
            input_spc,
            frames,
            out,
            out_pcm,
        } => cmd_calibrate_oracle(oracle.as_deref(), &input_spc, frames, &out, &out_pcm),
        Command::M0Acceptance { out } => cmd_m0_acceptance(&out),
        Command::M0Status { bundle, json } => cmd_m0_status(&bundle, json),
        Command::NewProject { out, name } => cmd_new_project(&out, name.as_deref()),
        Command::ValidateProject { project, json, out } => {
            cmd_validate_project(&project, json, out.as_deref())
        }
        Command::Pack {
            project,
            out_image,
            out_map,
            driver,
            refresh_source_hash,
            out_capability_manifest,
        } => cmd_pack(
            &project,
            &out_image,
            &out_map,
            driver.as_deref(),
            refresh_source_hash,
            out_capability_manifest.as_deref(),
        ),
        Command::M1Acceptance {
            project_a,
            project_b,
            out,
            frames,
        } => cmd_m1_acceptance(&project_a, project_b.as_deref(), &out, frames),
        Command::M1Status { bundle, json } => cmd_m1_status(&bundle, json),
        Command::CompileSfc {
            project_a,
            project_b,
            out_sfc,
            out_report,
            refresh_source_hash,
        } => cmd_compile_sfc(
            &project_a,
            project_b.as_deref(),
            out_sfc.as_deref(),
            out_report.as_deref(),
            refresh_source_hash,
        ),
        Command::VerifySfcStructure { sfc, out_report } => {
            cmd_verify_sfc_structure(&sfc, out_report.as_deref())
        }
        Command::VerifySfcModulesAudible {
            sfc,
            frames,
            out_report,
            min_max_abs,
            min_rms,
            oracle,
            out_wav_a,
            out_wav_b,
        } => cmd_verify_sfc_modules_audible(
            &sfc,
            frames,
            out_report.as_deref(),
            min_max_abs,
            min_rms,
            oracle.as_deref(),
            out_wav_a.as_deref(),
            out_wav_b.as_deref(),
        ),
        Command::CompileSpc {
            project,
            out_spc,
            out_image,
            out_map,
            out_report,
            refresh_source_hash,
        } => cmd_compile_spc(
            &project,
            out_spc.as_deref(),
            out_image.as_deref(),
            out_map.as_deref(),
            out_report.as_deref(),
            refresh_source_hash,
        ),
        Command::VerifySpcAudible {
            spc,
            frames,
            out_report,
            out_pcm,
            min_max_abs,
            min_rms,
            oracle,
            out_wav,
        } => cmd_verify_spc_audible(
            &spc,
            frames,
            out_report.as_deref(),
            out_pcm.as_deref(),
            min_max_abs,
            min_rms,
            oracle.as_deref(),
            out_wav.as_deref(),
        ),
        Command::EncodeBrr {
            audio,
            out_brr,
            out_report,
            loop_start_sample,
            no_force_filter_0_first_block,
        } => cmd_encode_brr(
            &audio,
            &out_brr,
            out_report.as_deref(),
            loop_start_sample,
            !no_force_filter_0_first_block,
        ),
        Command::PreviewBrr {
            brr,
            out_wav,
            out_report,
            sample_rate_hz,
        } => cmd_preview_brr(&brr, &out_wav, out_report.as_deref(), sample_rate_hz),
        Command::FindLoopCandidates {
            audio,
            out_report,
            window_samples,
            max_candidates,
            no_snap_to_brr_block,
        } => cmd_find_loop_candidates(
            &audio,
            &out_report,
            window_samples as usize,
            max_candidates as usize,
            !no_snap_to_brr_block,
        ),
        Command::Import {
            project,
            audio,
            id,
            name,
            no_copy,
            brr_sample_rate,
        } => cmd_import(
            &project,
            &audio,
            ImportOptions {
                id,
                name,
                copy_into_project: !no_copy,
                brr_sample_rate_hz: brr_sample_rate,
            },
        ),
        Command::MigrateProject {
            r#in,
            out,
            migration_report,
        } => cmd_migrate_project(&r#in, &out, migration_report.as_deref()),
        Command::RenderAtom {
            project,
            atom,
            out_brr,
            out_report,
            out_pcm,
        } => cmd_render_atom(
            &project,
            &atom,
            out_brr.as_deref(),
            out_report.as_deref(),
            out_pcm.as_deref(),
        ),
        Command::PreviewAtom {
            project,
            atom,
            duration_seconds,
            out_wav,
        } => cmd_preview_atom(&project, &atom, duration_seconds, out_wav.as_deref()),
        Command::CompileSequence {
            project,
            sequence_id,
            out_bin,
            out_report,
        } => cmd_compile_sequence(
            &project,
            sequence_id.as_deref(),
            out_bin.as_deref(),
            out_report.as_deref(),
        ),
    }
}

// =============================================================================
// doctor
// =============================================================================

fn cmd_doctor(json: bool, out: Option<&Path>) -> Result<(), CliError> {
    let workspace_root = std::env::current_dir().map_err(CliError::Cwd)?;
    let report = build_doctor_report(&workspace_root);

    if json {
        let s = serde_json::to_string_pretty(&report)?;
        println!("{s}");
    } else {
        print_doctor_human(&report);
    }

    if let Some(path) = out {
        write_json(path, &report)?;
        eprintln!("doctor: wrote {}", path.display());
    }

    Ok(())
}

fn build_doctor_report(workspace_root: &Path) -> DoctorReport {
    let asar = tools::resolve_asar();
    let oracle = tools::resolve_snes_spc_oracle(workspace_root);
    let mesen2 = tools::resolve_mesen2();

    let status = doctor_status(&asar, &oracle, &mesen2);
    let diagnostics = doctor_diagnostics(&asar, &oracle, &mesen2);

    DoctorReport {
        schema_version: SCHEMA_VERSION,
        report_type: DoctorReport::REPORT_TYPE.to_string(),
        tools: DoctorTools {
            asar: tool_status(&asar),
            snes_spc_oracle: tool_status(&oracle),
            mesen2: tool_status(&mesen2),
        },
        rust: rust_info(),
        status,
        diagnostics,
    }
}

fn tool_status(r: &ResolvedTool) -> ToolStatus {
    ToolStatus {
        resolved: r.resolved,
        path: r.path.as_ref().map(|p| p.display().to_string()),
        version: r.version.clone(),
        source: r.source,
        searched: if r.resolved {
            Vec::new()
        } else {
            r.searched.clone()
        },
    }
}

/// asar required for M0; missing asar is `errors`. Missing oracle or
/// Mesen2 alone is `warnings` (oracle is non-gating at M0; Mesen2 is
/// only used for manual verification).
fn doctor_status(
    asar: &ResolvedTool,
    oracle: &ResolvedTool,
    mesen2: &ResolvedTool,
) -> DoctorStatus {
    if !asar.resolved {
        DoctorStatus::Errors
    } else if !oracle.resolved || !mesen2.resolved {
        DoctorStatus::Warnings
    } else {
        DoctorStatus::Ok
    }
}

fn doctor_diagnostics(
    asar: &ResolvedTool,
    oracle: &ResolvedTool,
    mesen2: &ResolvedTool,
) -> Vec<String> {
    let mut d = Vec::new();
    if !asar.resolved {
        d.push("asar not found at SFCWC_ASAR or on PATH; assemble-smoke will fail".to_string());
    }
    if !oracle.resolved {
        d.push(
            "snes_spc oracle wrapper not found at SFCWC_SNES_SPC_ORACLE or tools/snes_spc_oracle"
                .to_string(),
        );
    }
    if !mesen2.resolved {
        d.push("Mesen2 not configured (set SFCWC_MESEN2 to enable manual smoke tests)".to_string());
    }
    d
}

fn rust_info() -> RustInfo {
    RustInfo {
        channel: "stable".to_string(),
        version: probe_rustc_version().unwrap_or_else(|| "unknown".to_string()),
    }
}

fn probe_rustc_version() -> Option<String> {
    let out = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    s.split_whitespace().nth(1).map(str::to_string)
}

fn print_doctor_human(r: &DoctorReport) {
    println!("doctor: status = {}", status_label(r.status));
    print_one_tool("asar", &r.tools.asar);
    print_one_tool("snes_spc_oracle", &r.tools.snes_spc_oracle);
    print_one_tool("mesen2", &r.tools.mesen2);
    println!("  rust: {} {}", r.rust.channel, r.rust.version);
    if !r.diagnostics.is_empty() {
        println!("diagnostics:");
        for d in &r.diagnostics {
            println!("  - {d}");
        }
    }
}

fn print_one_tool(label: &str, t: &ToolStatus) {
    let src = source_label(t.source);
    if t.resolved {
        let path = t.path.as_deref().unwrap_or("?");
        match &t.version {
            Some(v) => println!("  {label}: resolved via {src} -> {path} ({v})"),
            None => println!("  {label}: resolved via {src} -> {path}"),
        }
    } else {
        println!("  {label}: missing (searched: {})", t.searched.join(", "));
    }
}

fn source_label(s: ToolSource) -> &'static str {
    match s {
        ToolSource::Env => "env",
        ToolSource::Path => "path",
        ToolSource::Default => "default",
        ToolSource::Missing => "missing",
    }
}

fn status_label(s: DoctorStatus) -> &'static str {
    match s {
        DoctorStatus::Ok => "ok",
        DoctorStatus::Warnings => "warnings",
        DoctorStatus::Errors => "errors",
    }
}

// =============================================================================
// stubs: decode-fixtures, assemble-smoke, export-spc-smoke, calibrate-oracle
// =============================================================================

fn cmd_decode_fixtures(out: &Path) -> Result<(), CliError> {
    let results: Vec<_> = M0_RAW_DECODE_FIXTURES.iter().map(run_fixture).collect();
    let total = results.len() as u32;
    let passed = results.iter().filter(|r| r.passed).count() as u32;
    let failed = total - passed;

    let report = BrrFixtureReport {
        schema_version: SCHEMA_VERSION,
        report_type: BrrFixtureReport::REPORT_TYPE.to_string(),
        fixture_set: "m0_raw_decode".to_string(),
        total,
        passed,
        failed,
        skipped: 0,
        results,
    };
    write_json(out, &report)?;
    if failed == 0 {
        eprintln!(
            "decode-fixtures: {passed}/{total} passed; wrote {}",
            out.display()
        );
    } else {
        eprintln!(
            "decode-fixtures: {passed}/{total} passed ({failed} failed); wrote {}",
            out.display()
        );
    }
    Ok(())
}

fn cmd_assemble_smoke(source: &Path, report_out: &Path, image_out: &Path) -> Result<(), CliError> {
    let working_dir = std::env::current_dir().map_err(CliError::Cwd)?;
    let input_sha = sha256_hex_file(source).ok();
    let input_path_str = source.display().to_string();

    let mut report = AssembleReport::stub();
    report.input_path = Some(input_path_str.clone());
    report.input_sha256 = input_sha;
    report.output_path = Some(image_out.display().to_string());

    match AsarBackend::from_resolution() {
        Err(AssembleError::NotResolved { hint }) => {
            report.status = AssembleStatus::Error;
            report.error = Some(format!("assembler not resolved: {hint}"));
            write_json(report_out, &report)?;
            eprintln!(
                "assemble-smoke: asar not resolved (set SFCWC_ASAR); report -> {}",
                report_out.display()
            );
            Ok(())
        }
        Err(other) => {
            report.status = AssembleStatus::Error;
            report.error = Some(format!("backend init: {other}"));
            write_json(report_out, &report)?;
            eprintln!(
                "assemble-smoke: backend init failed: {other}; report -> {}",
                report_out.display()
            );
            Ok(())
        }
        Ok(backend) => assemble_with_backend(
            &backend,
            source,
            report_out,
            image_out,
            &working_dir,
            report,
        ),
    }
}

fn assemble_with_backend(
    backend: &AsarBackend,
    source: &Path,
    report_out: &Path,
    image_out: &Path,
    working_dir: &Path,
    mut report: AssembleReport,
) -> Result<(), CliError> {
    report.backend = backend.name().to_string();

    let input = AssembleInput::for_spc700_aram(
        source.to_path_buf(),
        image_out.to_path_buf(),
        working_dir.to_path_buf(),
    );

    match backend.assemble(&input) {
        Ok(out) => {
            report.backend_version = out.version;
            report.output_bytes = out.output_bytes;
            report.exit_code = Some(out.exit_code);
            report.stdout_lines = count_lines(&out.stdout);
            report.stderr_lines = count_lines(&out.stderr);
            report.output_image_sha256 = Some(out.output_image_sha256.clone());
            report.status = AssembleStatus::Ok;
            report.error = None;

            write_json(report_out, &report)?;
            eprintln!(
                "assemble-smoke: asar OK; wrote {} ({} B, sha256={}); report -> {}",
                image_out.display(),
                out.output_bytes,
                out.output_image_sha256,
                report_out.display()
            );
            Ok(())
        }
        Err(err) => {
            // Failure-as-data: populate what we have, status=error,
            // exit 0 so callers see the report.
            report.backend_version = backend.version().unwrap_or_else(|_| "unknown".to_string());
            if let AssembleError::NonZeroExit { code, ref stderr } = err {
                report.exit_code = Some(code);
                report.stderr_lines = count_lines(stderr);
            }
            report.status = AssembleStatus::Error;
            report.error = Some(format!("{err}"));

            write_json(report_out, &report)?;
            let summary = match &err {
                AssembleError::NonZeroExit { code, stderr } => {
                    format!("asar exited {code}: {}", first_line(stderr))
                }
                other => format!("{other}"),
            };
            eprintln!(
                "assemble-smoke: {summary}; report -> {}",
                report_out.display()
            );
            Ok(())
        }
    }
}

fn count_lines(s: &str) -> u32 {
    if s.is_empty() {
        0
    } else {
        s.lines().count() as u32
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

fn cmd_export_spc_smoke(
    aram_path: &Path,
    report_out: &Path,
    spc_out: &Path,
    verify: bool,
) -> Result<(), CliError> {
    let mut report = SpcExportReport::stub();
    report.output_path = Some(spc_out.display().to_string());

    // Read the ARAM input.
    let aram_bytes = match std::fs::read(aram_path) {
        Ok(b) => b,
        Err(e) => {
            report.status = SpcStatus::Error;
            report.error = Some(format!(
                "aram input missing at {}: {e} (run assemble-smoke first)",
                aram_path.display()
            ));
            write_json(report_out, &report)?;
            eprintln!(
                "export-spc-smoke: aram input missing at {} (run assemble-smoke first); report -> {}",
                aram_path.display(),
                report_out.display()
            );
            return Ok(());
        }
    };

    if aram_bytes.len() != SPC_ARAM_SIZE {
        report.status = SpcStatus::Error;
        report.error = Some(format!(
            "aram input wrong size at {}: expected {} bytes, got {}",
            aram_path.display(),
            SPC_ARAM_SIZE,
            aram_bytes.len()
        ));
        write_json(report_out, &report)?;
        eprintln!(
            "export-spc-smoke: aram input wrong size ({} B, expected {}); report -> {}",
            aram_bytes.len(),
            SPC_ARAM_SIZE,
            report_out.display()
        );
        return Ok(());
    }

    let aram_sha = sha256_hex(&aram_bytes);
    report.input_aram_sha256 = Some(aram_sha.clone());
    report.aram_image_sha256 = Some(aram_sha.clone());

    // Build the smoke SPC image (same ARAM, smoke CPU state, smoke DSP).
    let img: SpcImage =
        build_smoke_image(aram_bytes).expect("build_smoke_image rejected size we just checked");
    let spc_bytes = img.to_bytes().expect("to_bytes on validated image");

    // Write the .spc file.
    if let Some(parent) = spc_out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
    }
    std::fs::write(spc_out, &spc_bytes).map_err(|e| CliError::Io {
        path: spc_out.to_path_buf(),
        source: e,
    })?;

    let dsp_sha = sha256_hex(&img.dsp_regs);
    let spc_file_sha = sha256_hex(&spc_bytes);

    report.file_size_bytes = spc_bytes.len() as u64;
    report.dsp_state_sha256 = Some(dsp_sha.clone());
    report.spc_file_sha256 = Some(spc_file_sha.clone());
    report.initial_state = cpu_to_initial_state(&img.cpu);

    if verify {
        match verify_structure(&spc_bytes) {
            Ok(s) => {
                let aram_match = s.aram_sha256 == aram_sha;
                let cpu_match = s.cpu == SMOKE_CPU_STATE;
                let dsp_match = s.dsp_sha256 == dsp_sha;
                let size_match = s.file_size == SPC_FILE_SIZE;
                if aram_match && cpu_match && dsp_match && size_match && s.magic_ok {
                    report.verified_structure = true;
                } else {
                    report.verified_structure = false;
                    report.error = Some(format!(
                        "verify_structure mismatch (aram_match={aram_match}, cpu_match={cpu_match}, dsp_match={dsp_match}, size_match={size_match}, magic_ok={})",
                        s.magic_ok
                    ));
                }
            }
            Err(e) => {
                report.verified_structure = false;
                report.error = Some(format!("verify_structure failed: {e}"));
            }
        }
    }

    let status = if report.error.is_none() {
        SpcStatus::Ok
    } else {
        SpcStatus::Error
    };
    report.status = status;

    write_json(report_out, &report)?;
    let summary_tail = if verify {
        if report.verified_structure {
            "; structure verified".to_string()
        } else {
            "; structure verify FAILED".to_string()
        }
    } else {
        String::new()
    };
    eprintln!(
        "export-spc-smoke: wrote {} ({} B){}; report -> {}",
        spc_out.display(),
        spc_bytes.len(),
        summary_tail,
        report_out.display()
    );

    Ok(())
}

fn cpu_to_initial_state(cpu: &SpcCpuState) -> SpcInitialState {
    SpcInitialState {
        pc: cpu.pc,
        a: cpu.a,
        x: cpu.x,
        y: cpu.y,
        psw: cpu.psw,
        sp: cpu.sp,
    }
}

fn cmd_calibrate_oracle(
    explicit_oracle: Option<&Path>,
    input_spc: &Path,
    frames: u32,
    report_out: &Path,
    pcm_out: &Path,
) -> Result<(), CliError> {
    let workspace_root = std::env::current_dir().map_err(CliError::Cwd)?;

    let mut report = CalibrationReport::stub();
    report.fixture_set = Some(FixtureSetInfo {
        name: "m0_smoke".to_string(),
        sha256: sha256_hex_file(input_spc).unwrap_or_default(),
    });
    report.render = Some(RenderInfo {
        sample_rate_hz: 32000,
        frames,
        channels: 2,
    });
    report.provisional_tolerances = Some(ProvisionalTolerances {
        voice_render_max_abs_lsb: 1,
        voice_render_rms_lsb: 0.25,
    });

    // Oracle resolution: explicit --oracle wins, then env / workspace
    // defaults via core::tools.
    let oracle_path = match resolve_oracle(explicit_oracle, &workspace_root) {
        Some(p) => p,
        None => {
            report.status = CalibrationStatus::Error;
            report.error = Some(
                "oracle wrapper not resolved (set SFCWC_SNES_SPC_ORACLE or build it under tools/snes_spc_oracle/build/Release)".to_string(),
            );
            write_json(report_out, &report)?;
            eprintln!(
                "calibrate-oracle: oracle wrapper not resolved (set SFCWC_SNES_SPC_ORACLE); report -> {}",
                report_out.display()
            );
            return Ok(());
        }
    };

    let oracle_version = probe_oracle_version(&oracle_path);
    report.oracle = Some(OracleInfo {
        backend: "snes_spc_wrapper".to_string(),
        version: oracle_version.clone(),
        path: oracle_path.display().to_string(),
    });

    if !input_spc.is_file() {
        report.status = CalibrationStatus::Error;
        report.error = Some(format!(
            "input SPC missing or not a file: {}",
            input_spc.display()
        ));
        write_json(report_out, &report)?;
        eprintln!(
            "calibrate-oracle: input SPC missing at {}; report -> {}",
            input_spc.display(),
            report_out.display()
        );
        return Ok(());
    }

    // Wrapper writes its own report next to ours.
    let mut wrapper_report_path = report_out.to_path_buf();
    let wrapper_report_name = match wrapper_report_path.file_name() {
        Some(n) => format!("{}.oracle-side.json", n.to_string_lossy()),
        None => "oracle-side.json".to_string(),
    };
    wrapper_report_path.set_file_name(wrapper_report_name);

    if let Some(parent) = pcm_out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
    }

    let output = std::process::Command::new(&oracle_path)
        .arg("render")
        .arg("--input-spc")
        .arg(input_spc)
        .arg("--frames")
        .arg(frames.to_string())
        .arg("--output-pcm")
        .arg(pcm_out)
        .arg("--report")
        .arg(&wrapper_report_path)
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            report.status = CalibrationStatus::Error;
            report.error = Some(format!("spawn oracle: {e}"));
            write_json(report_out, &report)?;
            eprintln!(
                "calibrate-oracle: cannot spawn oracle ({e}); report -> {}",
                report_out.display()
            );
            return Ok(());
        }
    };

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let first = first_line(&stderr);
        report.status = CalibrationStatus::Error;
        report.error = Some(format!("oracle exited {code}: {first}"));
        write_json(report_out, &report)?;
        eprintln!(
            "calibrate-oracle: oracle exited {code}: {first}; report -> {}",
            report_out.display()
        );
        return Ok(());
    }

    // Verify PCM defensively in Rust — don't trust the wrapper's
    // self-reported max_abs/rms without recomputing.
    let pcm_bytes = match std::fs::read(pcm_out) {
        Ok(b) => b,
        Err(e) => {
            report.status = CalibrationStatus::Error;
            report.error = Some(format!("read oracle PCM: {e}"));
            write_json(report_out, &report)?;
            eprintln!(
                "calibrate-oracle: cannot read oracle PCM at {}: {e}; report -> {}",
                pcm_out.display(),
                report_out.display()
            );
            return Ok(());
        }
    };
    let expected_pcm_bytes = (frames as usize) * 4;
    if pcm_bytes.len() != expected_pcm_bytes {
        report.status = CalibrationStatus::Error;
        report.error = Some(format!(
            "oracle PCM wrong size: expected {} bytes ({} frames), got {}",
            expected_pcm_bytes,
            frames,
            pcm_bytes.len()
        ));
        write_json(report_out, &report)?;
        eprintln!(
            "calibrate-oracle: oracle PCM wrong size ({} B, expected {}); report -> {}",
            pcm_bytes.len(),
            expected_pcm_bytes,
            report_out.display()
        );
        return Ok(());
    }

    let (max_abs, rms) = pcm_stats_from_bytes(&pcm_bytes);
    report.observed = Some(ObservedInfo {
        voice_render_max_abs_lsb: max_abs,
        voice_render_rms_lsb: rms,
    });
    report.oracle_pcm_sha256 = Some(sha256_hex(&pcm_bytes));

    if max_abs != 0 {
        report.diagnostics.push(format!(
            "M0 smoke is muted via DSP FLG=$60; oracle render produced max_abs={max_abs} (UNEXPECTED). \
             Investigate: the smoke contract or the wrapper is wrong."
        ));
    }

    report.status = CalibrationStatus::ProvisionalNotCiGate;
    report.error = None;

    write_json(report_out, &report)?;

    if max_abs == 0 {
        eprintln!(
            "calibrate-oracle: snes_spc_wrapper rendered {frames} frames; max_abs=0; rms=0; report -> {}",
            report_out.display()
        );
    } else {
        eprintln!(
            "calibrate-oracle: snes_spc_wrapper rendered {frames} frames; max_abs={max_abs} (UNEXPECTED for muted smoke); report -> {}",
            report_out.display()
        );
    }

    Ok(())
}

fn resolve_oracle(explicit: Option<&Path>, workspace_root: &Path) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        return None;
    }
    let r = tools::resolve_snes_spc_oracle(workspace_root);
    if r.resolved {
        r.path
    } else {
        None
    }
}

fn probe_oracle_version(oracle: &Path) -> String {
    match std::process::Command::new(oracle).arg("--version").output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        _ => "unknown".to_string(),
    }
}

fn pcm_stats_from_bytes(pcm: &[u8]) -> (i32, f64) {
    let n = pcm.len() / 2;
    if n == 0 {
        return (0, 0.0);
    }
    let mut max_abs: i32 = 0;
    let mut sum_sq: f64 = 0.0;
    for chunk in pcm.chunks_exact(2) {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
        let a = s.unsigned_abs() as i32;
        if a > max_abs {
            max_abs = a;
        }
        sum_sq += (s as f64) * (s as f64);
    }
    let rms = (sum_sq / (n as f64)).sqrt();
    (max_abs, rms)
}

// =============================================================================
// m0-acceptance
// =============================================================================

fn cmd_m0_acceptance(out_dir: &Path) -> Result<(), CliError> {
    create_dir(out_dir)?;
    let workspace_root = std::env::current_dir().map_err(CliError::Cwd)?;

    let doctor_path = out_dir.join("doctor.json");
    let brr_path = out_dir.join("brr-fixture-report.json");
    let assemble_path = out_dir.join("assemble-report.json");
    let driver_bin = out_dir.join("driver.bin");
    let spc_path = out_dir.join("spc-export-report.json");
    let smoke_spc = out_dir.join("smoke.spc");
    let aram_map_path = out_dir.join("aram-map.json");
    let calibration_path = out_dir.join("calibration-report.json");
    let oracle_pcm = out_dir.join("oracle.pcm_s16le");
    let manifest_path = out_dir.join("manifest.json");

    // Run each step, writing its report. Failure-as-data is the
    // contract throughout — every step writes a report regardless of
    // success/failure, and we read them back to compute the bundle.

    // 1. Doctor (also kept in memory for step-status mapping).
    let doctor = build_doctor_report(&workspace_root);
    write_json(&doctor_path, &doctor)?;
    eprintln!("m0-acceptance: doctor -> {}", doctor_path.display());

    // 2. BRR fixtures.
    cmd_decode_fixtures(&brr_path)?;

    // 3. Assemble.
    let smoke_asm = workspace_root
        .join("core")
        .join("fixtures")
        .join("asm")
        .join("m0_smoke.asm");
    cmd_assemble_smoke(&smoke_asm, &assemble_path, &driver_bin)?;

    // 4. SPC export.
    cmd_export_spc_smoke(&driver_bin, &spc_path, &smoke_spc, true)?;

    // 5. ARAM map: real walk if driver.bin is the right size,
    // otherwise the M0.1 stub (kept so the report file always exists).
    let (aram_report, aram_real) = match read_aram_image(&driver_bin) {
        Some(img) => (map_from_image(&img), true),
        None => (AramMapReport::stub(), false),
    };
    write_json(&aram_map_path, &aram_report)?;
    eprintln!("m0-acceptance: aram-map -> {}", aram_map_path.display());

    // 6. Calibrate oracle.
    cmd_calibrate_oracle(None, &smoke_spc, 2048, &calibration_path, &oracle_pcm)?;

    // 7. Read each report back to compute the bundle. We don't trust
    // in-memory state because the per-cmd functions are the source of
    // truth for what's on disk, and m0-status needs to reproduce the
    // computation from the same on-disk files.
    let brr_report = read_report::<BrrFixtureReport>(&brr_path);
    let assemble_report = read_report::<AssembleReport>(&assemble_path);
    let spc_report = read_report::<SpcExportReport>(&spc_path);
    let calibration_report = read_report::<CalibrationReport>(&calibration_path);

    let steps = BundleSteps {
        doctor: doctor_step_status(&doctor),
        decode_fixtures: brr_step_status(brr_report.as_ref()),
        assemble: assemble_step_status(assemble_report.as_ref(), &doctor),
        spc_export: spc_step_status(spc_report.as_ref()),
        aram_map: aram_step_status(&aram_report, aram_real),
        calibration: calibration_step_status(calibration_report.as_ref(), &doctor),
    };
    let bundle_status = aggregate_bundle_status(&steps);

    let mut diagnostics = aggregate_diagnostics(
        &doctor,
        brr_report.as_ref(),
        assemble_report.as_ref(),
        spc_report.as_ref(),
        calibration_report.as_ref(),
    );

    // Cross-check via verify_bundle on the fresh bundle.
    // Anything it flags becomes a bundle-level diagnostic too.
    let manifest_pre = M0Manifest {
        schema_version: SCHEMA_VERSION,
        report_type: M0Manifest::REPORT_TYPE.to_string(),
        generated_at: Some(rfc3339_now()),
        doctor_report: doctor_path.display().to_string(),
        brr_fixture_report: brr_path.display().to_string(),
        aram_map_report: aram_map_path.display().to_string(),
        assemble_report: assemble_path.display().to_string(),
        spc_export_report: spc_path.display().to_string(),
        calibration_report: calibration_path.display().to_string(),
        bundle: BundleSummary::default(),
    };
    write_json(&manifest_path, &manifest_pre)?;
    let integrity = verify_bundle(out_dir);
    for f in &integrity.findings {
        diagnostics.push(format!("integrity: {f}"));
    }
    truncate_diagnostics(&mut diagnostics);

    let bundle = BundleSummary {
        steps,
        status: bundle_status,
        aram_image_sha256: assemble_report
            .as_ref()
            .and_then(|r| r.output_image_sha256.clone()),
        spc_file_sha256: spc_report.as_ref().and_then(|r| r.spc_file_sha256.clone()),
        oracle_pcm_sha256: calibration_report
            .as_ref()
            .and_then(|r| r.oracle_pcm_sha256.clone()),
        diagnostics,
    };
    let manifest = M0Manifest {
        bundle,
        ..manifest_pre
    };
    write_json(&manifest_path, &manifest)?;

    eprintln!(
        "m0-acceptance: bundle.status={}; wrote 7 reports + manifest -> {}",
        bundle_status_label(bundle_status),
        manifest_path.display()
    );

    Ok(())
}

fn cmd_m0_status(bundle_dir: &Path, json: bool) -> Result<(), CliError> {
    let manifest_path = bundle_dir.join("manifest.json");
    let manifest_bytes = match std::fs::read(&manifest_path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!(
                "m0-status: no bundle at {} (run `sfcwc m0-acceptance` first)",
                bundle_dir.display()
            );
            std::process::exit(1);
        }
    };
    let manifest: M0Manifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("m0-status: cannot parse {}: {e}", manifest_path.display());
            std::process::exit(1);
        }
    };

    let integrity = verify_bundle(bundle_dir);

    if json {
        let s = serde_json::to_string_pretty(&manifest)?;
        println!("{s}");
    } else {
        print_m0_status_human(&manifest, &integrity);
    }

    let bundle_ok = matches!(
        manifest.bundle.status,
        BundleStatus::Ok | BundleStatus::Degraded
    );
    let integrity_ok = integrity.all_reports_present
        && integrity.reports_parse
        && integrity.schema_versions_consistent
        && integrity.aram_sha_matches_across_reports;

    if bundle_ok && integrity_ok {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn print_m0_status_human(m: &M0Manifest, integrity: &sfc_atomizer_core::manifest::BundleIntegrity) {
    println!("m0-status:");
    println!(
        "  bundle.status   = {}",
        bundle_status_label(m.bundle.status)
    );
    println!(
        "  generated_at    = {}",
        m.generated_at.as_deref().unwrap_or("<unknown>")
    );
    println!("  steps:");
    let s = &m.bundle.steps;
    println!("    doctor          = {}", step_status_label(s.doctor));
    println!(
        "    decode_fixtures = {}",
        step_status_label(s.decode_fixtures)
    );
    println!("    assemble        = {}", step_status_label(s.assemble));
    println!("    spc_export      = {}", step_status_label(s.spc_export));
    println!("    aram_map        = {}", step_status_label(s.aram_map));
    println!("    calibration     = {}", step_status_label(s.calibration));
    println!("  cross-references:");
    println!(
        "    aram_image_sha256  = {}",
        m.bundle.aram_image_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    spc_file_sha256    = {}",
        m.bundle.spc_file_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    oracle_pcm_sha256  = {}",
        m.bundle.oracle_pcm_sha256.as_deref().unwrap_or("<absent>")
    );
    println!("  integrity:");
    println!(
        "    all_reports_present              = {}",
        integrity.all_reports_present
    );
    println!(
        "    reports_parse                    = {}",
        integrity.reports_parse
    );
    println!(
        "    schema_versions_consistent       = {}",
        integrity.schema_versions_consistent
    );
    println!(
        "    aram_sha_matches_across_reports  = {}",
        integrity.aram_sha_matches_across_reports
    );
    if !integrity.findings.is_empty() {
        println!("  integrity findings:");
        for f in integrity.findings.iter().take(10) {
            println!("    - {f}");
        }
        if integrity.findings.len() > 10 {
            println!("    ... ({} more truncated)", integrity.findings.len() - 10);
        }
    }
    if !m.bundle.diagnostics.is_empty() {
        println!("  diagnostics (top 5):");
        for d in m.bundle.diagnostics.iter().take(5) {
            println!("    - {d}");
        }
    }
}

/// Read a 64 KB ARAM image into a fixed array. Returns `None` if the
/// file is missing or not exactly the right size.
fn read_aram_image(path: &Path) -> Option<[u8; ARAM_LEN]> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() != ARAM_LEN {
        return None;
    }
    let mut img = [0u8; ARAM_LEN];
    img.copy_from_slice(&bytes);
    Some(img)
}

fn read_report<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

// =============================================================================
// Bundle aggregation
// =============================================================================

fn doctor_step_status(d: &DoctorReport) -> StepStatus {
    match d.status {
        DoctorStatus::Ok => StepStatus::Ok,
        DoctorStatus::Warnings => StepStatus::Warnings,
        DoctorStatus::Errors => StepStatus::Error,
    }
}

fn brr_step_status(r: Option<&BrrFixtureReport>) -> StepStatus {
    match r {
        Some(r) if r.failed == 0 && r.total > 0 => StepStatus::Ok,
        Some(_) => StepStatus::Error,
        None => StepStatus::Skipped,
    }
}

fn assemble_step_status(r: Option<&AssembleReport>, doctor: &DoctorReport) -> StepStatus {
    if !doctor.tools.asar.resolved {
        return StepStatus::Skipped;
    }
    match r {
        Some(r) => match r.status {
            AssembleStatus::Ok => StepStatus::Ok,
            AssembleStatus::Error => StepStatus::Error,
            AssembleStatus::NotRun => StepStatus::Skipped,
        },
        None => StepStatus::Skipped,
    }
}

fn spc_step_status(r: Option<&SpcExportReport>) -> StepStatus {
    match r {
        Some(r) if r.status == SpcStatus::Ok && r.verified_structure => StepStatus::Ok,
        Some(r) if r.status == SpcStatus::NotRun => StepStatus::Skipped,
        Some(_) => StepStatus::Error,
        None => StepStatus::Skipped,
    }
}

fn aram_step_status(r: &AramMapReport, real_walk: bool) -> StepStatus {
    if !real_walk {
        return StepStatus::Skipped;
    }
    if !r.collisions.is_empty() {
        return StepStatus::Error;
    }
    let sum: u32 = r.regions.iter().map(|x| x.bytes).sum();
    if sum != r.total_aram {
        return StepStatus::Error;
    }
    let claimed_free: u32 = r
        .regions
        .iter()
        .filter(|x| x.kind == AramKind::Free)
        .map(|x| x.bytes)
        .sum();
    if claimed_free != r.free_bytes {
        return StepStatus::Error;
    }
    StepStatus::Ok
}

fn calibration_step_status(r: Option<&CalibrationReport>, doctor: &DoctorReport) -> StepStatus {
    if !doctor.tools.snes_spc_oracle.resolved {
        return StepStatus::Skipped;
    }
    match r {
        Some(r) => match r.status {
            CalibrationStatus::ProvisionalNotCiGate => match r.observed.as_ref() {
                Some(o) if o.voice_render_max_abs_lsb == 0 => StepStatus::Ok,
                Some(_) => StepStatus::Warnings, // smoke contract violation
                None => StepStatus::Error,
            },
            CalibrationStatus::Frozen => StepStatus::Ok,
            CalibrationStatus::NotRun => StepStatus::Skipped,
            CalibrationStatus::Error => StepStatus::Error,
        },
        None => StepStatus::Skipped,
    }
}

/// Aggregation rules — see SPEC §21 M0 acceptance.
///
/// Required steps: doctor, decode_fixtures, assemble, spc_export,
/// aram_map. Calibration is optional at M0 (oracle missing is
/// acceptable; bundle drops to `degraded` rather than `error`).
///
/// - Any required step `Error` or `Skipped` → bundle `Error`.
/// - All required `Ok` AND calibration `Ok`               → bundle `Ok`.
/// - Otherwise (required has `Warnings`, OR calibration is
///   `Warnings`/`Error`/`Skipped`)                         → bundle `Degraded`.
fn aggregate_bundle_status(steps: &BundleSteps) -> BundleStatus {
    let required = [
        steps.doctor,
        steps.decode_fixtures,
        steps.assemble,
        steps.spc_export,
        steps.aram_map,
    ];
    if required
        .iter()
        .any(|s| matches!(s, StepStatus::Error | StepStatus::Skipped))
    {
        return BundleStatus::Error;
    }
    let all_required_ok = required.iter().all(|s| matches!(s, StepStatus::Ok));
    let calibration_ok = matches!(steps.calibration, StepStatus::Ok);
    if all_required_ok && calibration_ok {
        BundleStatus::Ok
    } else {
        BundleStatus::Degraded
    }
}

fn aggregate_diagnostics(
    doctor: &DoctorReport,
    brr: Option<&BrrFixtureReport>,
    assemble: Option<&AssembleReport>,
    spc: Option<&SpcExportReport>,
    calibration: Option<&CalibrationReport>,
) -> Vec<String> {
    let mut out = Vec::new();
    for d in &doctor.diagnostics {
        out.push(format!("doctor: {d}"));
    }
    if let Some(b) = brr {
        if b.failed > 0 {
            out.push(format!(
                "decode_fixtures: {} of {} fixtures failed",
                b.failed, b.total
            ));
        }
    }
    if let Some(a) = assemble {
        if let Some(e) = a.error.as_deref() {
            out.push(format!("assemble: {e}"));
        }
    }
    if let Some(s) = spc {
        if let Some(e) = s.error.as_deref() {
            out.push(format!("spc_export: {e}"));
        }
        if !s.verified_structure && s.status == SpcStatus::Ok {
            out.push("spc_export: structure verification skipped".to_string());
        }
    }
    if let Some(c) = calibration {
        if let Some(e) = c.error.as_deref() {
            out.push(format!("calibration: {e}"));
        }
        for d in &c.diagnostics {
            out.push(format!("calibration: {d}"));
        }
    }
    out
}

const MAX_DIAGNOSTICS: usize = 50;

fn truncate_diagnostics(d: &mut Vec<String>) {
    if d.len() > MAX_DIAGNOSTICS {
        let extra = d.len() - MAX_DIAGNOSTICS;
        d.truncate(MAX_DIAGNOSTICS);
        d.push(format!("... ({extra} more truncated)"));
    }
}

fn bundle_status_label(s: BundleStatus) -> &'static str {
    match s {
        BundleStatus::Ok => "ok",
        BundleStatus::Degraded => "degraded",
        BundleStatus::Error => "error",
    }
}

fn step_status_label(s: StepStatus) -> &'static str {
    match s {
        StepStatus::Ok => "ok",
        StepStatus::Warnings => "warnings",
        StepStatus::Error => "error",
        StepStatus::Skipped => "skipped",
    }
}

/// RFC3339 timestamp using only `std::time` + Howard Hinnant's
/// civil-from-days algorithm. UTC, second precision, 'Z' suffix.
fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    rfc3339_from_unix(secs)
}

fn rfc3339_from_unix(secs: u64) -> String {
    let s = (secs % 60) as u32;
    let m = ((secs / 60) % 60) as u32;
    let h = ((secs / 3600) % 24) as u32;
    let days = (secs / 86400) as i64;

    // Howard Hinnant's civil_from_days. Valid for 0001-01-01 onward.
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if mo <= 2 { y + 1 } else { y };

    format!("{year:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

// =============================================================================
// io helpers
// =============================================================================

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    let mut s = serde_json::to_string_pretty(value)?;
    s.push('\n');
    std::fs::write(path, s).map_err(|source| CliError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir(dir: &Path) -> Result<(), CliError> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|source| CliError::Io {
        path: dir.to_path_buf(),
        source,
    })
}

// =============================================================================
// new-project / validate-project (M1.1)
// =============================================================================

fn cmd_new_project(out: &Path, explicit_name: Option<&str>) -> Result<(), CliError> {
    let name = explicit_name
        .map(str::to_string)
        .unwrap_or_else(|| derive_project_name_from_path(out));
    let project = ProjectV1::new_template(&name);
    project
        .save_to_path(out)
        .map_err(|e| project_io_to_cli(e, out))?;
    eprintln!(
        "new-project: wrote {} (template; pre-import — `validate-project` will report empty sample_pool until samples are added)",
        out.display()
    );
    Ok(())
}

fn cmd_validate_project(
    project: &Path,
    emit_json: bool,
    report_out: Option<&Path>,
) -> Result<(), CliError> {
    let project_path_s = project.display().to_string();
    let mut report = ValidationReport {
        project_path: project_path_s.clone(),
        ..ValidationReport::stub()
    };

    let load_result = load_project_versioned(project);
    let (status, errors) = match load_result {
        Ok(LoadedProject::V1(p)) => match p.validate() {
            Ok(()) => (ValidationStatus::Ok, Vec::new()),
            Err(verrors) => (
                ValidationStatus::Invalid,
                verrors.into_iter().map(validation_error_to_json).collect(),
            ),
        },
        Ok(LoadedProject::V2(p)) => match p.validate() {
            Ok(()) => (ValidationStatus::Ok, Vec::new()),
            Err(verrors) => (
                ValidationStatus::Invalid,
                verrors.into_iter().map(validation_error_to_json).collect(),
            ),
        },
        Err(e) => (
            ValidationStatus::IoError,
            vec![ValidationErrorJson {
                path: project_path_s.clone(),
                message: format!("{e}"),
            }],
        ),
    };
    report.status = status;
    report.errors = errors;

    if emit_json {
        let s = serde_json::to_string_pretty(&report)?;
        println!("{s}");
    }
    if let Some(p) = report_out {
        write_json(p, &report)?;
    }

    print_validate_summary(&report);

    let exit = match report.status {
        ValidationStatus::Ok => 0,
        ValidationStatus::Invalid => 2,
        ValidationStatus::IoError => 1,
    };
    if exit != 0 {
        std::process::exit(exit);
    }
    Ok(())
}

/// Resolved project path for downstream compile commands. v1 projects
/// are returned as-is; sample-only-equivalent v2 projects are
/// "shimmed" into a synthetic v1 JSON (in a tempdir, with absolute
/// audio paths so relative-path resolution still works) so the
/// existing `compile_aram_image` / `export_sfc` plumbing — both v1-
/// only — keeps working unchanged. v2 projects with atom data error
/// with the M2.5-pending message per the brief.
struct V1Input {
    /// Path the caller should hand to compile_aram_image / export_sfc.
    path: PathBuf,
    /// Holds the synthetic JSON's tempdir alive until compile is
    /// done. `None` for the pure-v1 path.
    _guard: Option<tempfile::TempDir>,
}

fn prepare_v1_input(label: &str, project_path: &Path) -> V1Input {
    let loaded = match load_project_versioned(project_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{label}: load failed for {}: {e}", project_path.display());
            std::process::exit(match e {
                ProjectIoError::Validation(_) => 2,
                _ => 1,
            });
        }
    };
    match loaded {
        LoadedProject::V1(_) => V1Input {
            path: project_path.to_path_buf(),
            _guard: None,
        },
        LoadedProject::V2(v2) => {
            // Up-front v2 validation so we catch atom data even when
            // compile_aram_image's v1 path would otherwise just see
            // a synthetic-v1-equivalent.
            if let Err(verrors) = v2.validate() {
                eprintln!(
                    "{label}: v2 project invalid — {} ({} error{})",
                    project_path.display(),
                    verrors.len(),
                    if verrors.len() == 1 { "" } else { "s" }
                );
                for e in &verrors {
                    eprintln!("  {} : {}", e.path, e.kind);
                }
                std::process::exit(2);
            }
            v1_shim_from_sample_only_v2(label, project_path, &v2)
        }
    }
}

fn v1_shim_from_sample_only_v2(
    label: &str,
    project_path: &Path,
    v2: &sfc_atomizer_core::project_v2::ProjectV2,
) -> V1Input {
    use sfc_atomizer_core::project::M1Block;
    use sfc_atomizer_core::project_v2::TrackKind;
    // Sample-only equivalence: empty atom data, every track is
    // sample_sustain on voice 0, and there's at least one such
    // track. Anything else routes to the M2.5-pending error.
    let any_atom_data = !v2.atom_pool.is_empty() || !v2.atom_sequences.is_empty();
    let only_sample_voice_0 = !v2.tracks.is_empty()
        && v2
            .tracks
            .iter()
            .all(|t| t.voice == 0 && matches!(t.kind, TrackKind::SampleSustain { .. }));
    if any_atom_data || !only_sample_voice_0 {
        eprintln!(
            "{label}: v2 project {} has atoms or atom sequences. atom rendering lands at M2.2; sequence compilation at M2.4; multi-voice driver at M2.5. v2 projects with atoms or sequences cannot yet be compiled. Use a sample-only v2 project, or stay on v1 until M2.5 ships.",
            project_path.display()
        );
        std::process::exit(2);
    }

    // Pick the first sample_sustain track's sample_id as the
    // synthetic-v1 active_sample_id. (Validation guarantees the
    // referenced sample exists in sample_pool.)
    let active_sample_id = v2
        .tracks
        .iter()
        .find_map(|t| match &t.kind {
            TrackKind::SampleSustain { sample_id } => Some(sample_id.clone()),
            _ => None,
        })
        .unwrap_or_default();

    // Build a v1 with absolute audio paths (the synthetic file lives
    // in a tempdir, away from the user's project_dir; relative
    // resolution would otherwise be wrong).
    //
    // Canonicalize the project path first so the parent is a real
    // absolute path even when the user passed a relative project arg
    // (`v2.json` from cwd). Falls back to joining cwd if canonicalize
    // fails (e.g. broken symlinks).
    let canonical = std::fs::canonicalize(project_path)
        .ok()
        .unwrap_or_else(|| project_path.to_path_buf());
    let project_dir = canonical
        .parent()
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut sample_pool = v2.sample_pool.clone();
    for s in &mut sample_pool {
        let raw = Path::new(&s.source.path);
        if !raw.is_absolute() {
            let abs = project_dir.join(raw);
            s.source.path = abs.display().to_string();
        }
    }
    let v1 = ProjectV1 {
        schema_version: ProjectV1::SCHEMA_VERSION_M1,
        project: v2.project.clone(),
        driver: v2.driver.clone(),
        master_echo: v2.master_echo.clone(),
        sample_pool,
        m1: M1Block { active_sample_id },
    };

    // Write to a tempdir; the guard keeps the dir alive across the
    // compile.
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{label}: v2 shim tempdir: {e}");
            std::process::exit(1);
        }
    };
    let stem = project_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    let shim_path = dir.path().join(format!("{stem}.v1-shim.json"));
    if let Err(e) = v1.save_to_path(&shim_path) {
        eprintln!("{label}: writing v2 shim {}: {e}", shim_path.display());
        std::process::exit(1);
    }
    V1Input {
        path: shim_path,
        _guard: Some(dir),
    }
}

/// `sfcwc migrate-project --in <v1> --out <v2>` — explicit one-way
/// migration per SPEC §16.10. Validates the v1 input, runs
/// `migrate_from_v1`, validates the resulting v2, writes both the
/// migrated project and a structured migration report.
///
/// Exit codes (set via `std::process::exit` to bypass the generic
/// `CliError` exit-1 path):
///
/// - 0 success
/// - 1 IO / parse error (also for input that's not a JSON project)
/// - 2 v1 validation failure OR input already at schema_version 2
/// - 3 post-migration v2 validation failure (real bug — flag and stop)
fn cmd_migrate_project(
    in_path: &Path,
    out_path: &Path,
    migration_report: Option<&Path>,
) -> Result<(), CliError> {
    // Step 1 — load + dispatch by schema_version. v2 input is a hard
    // error; the user is being asked to "migrate v1 to v2" but they
    // pointed at a v2 file.
    let loaded = match load_project_versioned(in_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("migrate-project: {e}");
            std::process::exit(match e {
                ProjectIoError::NotFound { .. }
                | ProjectIoError::Io { .. }
                | ProjectIoError::Parse { .. }
                | ProjectIoError::MalformedValue
                | ProjectIoError::UnsupportedSchemaVersion { .. } => 1,
                ProjectIoError::Validation(_) => 2,
            });
        }
    };
    let v1 = match loaded {
        LoadedProject::V1(p) => p,
        LoadedProject::V2(_) => {
            eprintln!(
                "migrate-project: input {} is already at schema_version 2 (no migration needed)",
                in_path.display()
            );
            std::process::exit(2);
        }
    };

    // Step 2 — v1 validation gate.
    if let Err(verrors) = v1.validate() {
        eprintln!(
            "migrate-project: v1 input {} fails validation ({} error(s)):",
            in_path.display(),
            verrors.len()
        );
        for e in &verrors {
            eprintln!("  {}: {}", e.path, e.kind);
        }
        std::process::exit(2);
    }

    // Step 3 — pure transformation.
    let v2 = migrate_from_v1(&v1);

    // Step 4 — post-migration validation gate.
    if let Err(verrors) = v2.validate() {
        eprintln!(
            "migrate-project: post-migration v2 fails validation ({} error(s)) — this is a real bug:",
            verrors.len()
        );
        for e in &verrors {
            eprintln!("  {}: {}", e.path, e.kind);
        }
        std::process::exit(3);
    }

    // Step 5 — write the migrated project.
    if let Err(e) = v2.save_to_path(out_path) {
        eprintln!("migrate-project: writing {}: {}", out_path.display(), e);
        std::process::exit(1);
    }

    // Step 6 — derive + write the migration report.
    let report_path = match migration_report {
        Some(p) => p.to_path_buf(),
        None => default_migration_report_path(out_path),
    };
    let report = MigrationReport::for_v1_to_v2(in_path.to_path_buf(), out_path.to_path_buf(), &v1);
    if let Err(e) = write_json(&report_path, &report) {
        eprintln!(
            "migrate-project: writing migration report {}: {e}",
            report_path.display()
        );
        std::process::exit(1);
    }

    eprintln!(
        "migrate-project: {} -> {}; {} sample(s) preserved; m1.active_sample_id={:?} mapped to track_sample_0 on voice 0; report -> {}",
        in_path.display(),
        out_path.display(),
        v1.sample_pool.len(),
        v1.m1.active_sample_id,
        report_path.display()
    );
    Ok(())
}

/// `sfcwc render-atom --project <v2> --atom <id>` — SPEC §16.9
/// atom render → M1.3 BRR encode chain. Writes the BRR bytes and a
/// structured `AtomRenderReport` so downstream M2.3 pack consumes
/// the deterministic output. Optional `--out-pcm` writes the raw
/// pre-encode PCM as s16le bytes for offline inspection.
fn cmd_render_atom(
    project_path: &Path,
    atom_id: &str,
    out_brr: Option<&Path>,
    out_report: Option<&Path>,
    out_pcm: Option<&Path>,
) -> Result<(), CliError> {
    let v2 = match load_project_versioned(project_path) {
        Ok(LoadedProject::V1(_)) => {
            eprintln!(
                "render-atom: {} is a v1 project; render-atom requires v2. Run `sfcwc migrate-project --in {} --out <v2>` first.",
                project_path.display(),
                project_path.display()
            );
            std::process::exit(2);
        }
        Ok(LoadedProject::V2(p)) => p,
        Err(e) => {
            eprintln!("render-atom: {e}");
            std::process::exit(match e {
                ProjectIoError::Validation(_) => 2,
                _ => 1,
            });
        }
    };
    if let Err(verrors) = v2.validate() {
        eprintln!(
            "render-atom: project invalid — {} ({} error{})",
            project_path.display(),
            verrors.len(),
            if verrors.len() == 1 { "" } else { "s" }
        );
        for e in &verrors {
            eprintln!("  {} : {}", e.path, e.kind);
        }
        std::process::exit(2);
    }

    let atom = match v2.atom_pool.iter().find(|a| a.id == atom_id) {
        Some(a) => a.clone(),
        None => {
            let available: Vec<&str> = v2.atom_pool.iter().map(|a| a.id.as_str()).collect();
            eprintln!(
                "render-atom: atom id {atom_id:?} not found in atom_pool. available: [{}]",
                available.join(", ")
            );
            std::process::exit(2);
        }
    };

    let render = render_to_brr(&atom)
        .expect("AtomRenderError is uninhabited at M2.2 — render is infallible");

    let out_brr_owned = out_brr
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m2/atoms").join(format!("{atom_id}.brr")));
    let out_report_owned = out_report.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        PathBuf::from("build/m2/atoms").join(format!("{atom_id}.atom-render-report.json"))
    });

    if let Some(parent) = out_brr_owned.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir(parent)?;
        }
    }
    std::fs::write(&out_brr_owned, &render.brr_bytes).map_err(|source| CliError::Io {
        path: out_brr_owned.clone(),
        source,
    })?;

    if let Some(p) = out_pcm {
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir(parent)?;
            }
        }
        let mut bytes = Vec::with_capacity(render.pcm.len() * 2);
        for s in &render.pcm {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(p, &bytes).map_err(|source| CliError::Io {
            path: p.to_path_buf(),
            source,
        })?;
    }

    let report = atom_render_report(&atom, &render);
    write_json(&out_report_owned, &report)?;

    let normalize_label = if atom.render.normalize {
        "true"
    } else {
        "false"
    };
    let partial_count = match &atom.kind {
        AtomKind::AdditiveSingleCycleV0 { partials } => partials.len(),
    };
    eprintln!(
        "render-atom: {} ({}) — cycle={}, partials={}, normalize={}, brr={} B (sha={}); -> {}",
        atom.id,
        atom.name,
        atom.cycle_len_samples,
        partial_count,
        normalize_label,
        render.brr_bytes.len(),
        &render.brr_sha256,
        out_brr_owned.display(),
    );
    Ok(())
}

fn atom_render_report(atom: &AtomSlot, render: &AtomBrrOutput) -> AtomRenderReport {
    let (kind_str, partial_count) = match &atom.kind {
        AtomKind::AdditiveSingleCycleV0 { partials } => (
            "additive_single_cycle_v0".to_string(),
            partials.len() as u32,
        ),
    };
    AtomRenderReport {
        schema_version: SCHEMA_VERSION,
        report_type: AtomRenderReport::REPORT_TYPE.to_string(),
        atom_id: atom.id.clone(),
        atom_name: atom.name.clone(),
        atom_kind: kind_str,
        cycle_len_samples: atom.cycle_len_samples as u32,
        partial_count,
        normalize: atom.render.normalize,
        atom_amplitude: atom.amplitude,
        root_midi_note: atom.root_midi_note,
        pcm_sha256: render.pcm_sha256.clone(),
        brr_sha256: render.brr_sha256.clone(),
        brr_bytes: render.brr_bytes.len() as u32,
        encode_summary: render.encode_summary,
    }
}

/// `sfcwc preview-atom --project <v2> --atom <id>` — render the atom,
/// decode BRR, repeat to fill `--duration-seconds` at 32 kHz, write a
/// 32 kHz mono PCM16 WAV. Engineer-side audition; not committed.
fn cmd_preview_atom(
    project_path: &Path,
    atom_id: &str,
    duration_seconds: f64,
    out_wav: Option<&Path>,
) -> Result<(), CliError> {
    let v2 = match load_project_versioned(project_path) {
        Ok(LoadedProject::V1(_)) => {
            eprintln!(
                "preview-atom: {} is a v1 project; preview-atom requires v2. Run `sfcwc migrate-project` first.",
                project_path.display()
            );
            std::process::exit(2);
        }
        Ok(LoadedProject::V2(p)) => p,
        Err(e) => {
            eprintln!("preview-atom: {e}");
            std::process::exit(match e {
                ProjectIoError::Validation(_) => 2,
                _ => 1,
            });
        }
    };
    if let Err(verrors) = v2.validate() {
        eprintln!(
            "preview-atom: project invalid — {} ({} error{})",
            project_path.display(),
            verrors.len(),
            if verrors.len() == 1 { "" } else { "s" }
        );
        for e in &verrors {
            eprintln!("  {} : {}", e.path, e.kind);
        }
        std::process::exit(2);
    }
    let atom = match v2.atom_pool.iter().find(|a| a.id == atom_id) {
        Some(a) => a.clone(),
        None => {
            let available: Vec<&str> = v2.atom_pool.iter().map(|a| a.id.as_str()).collect();
            eprintln!(
                "preview-atom: atom id {atom_id:?} not found in atom_pool. available: [{}]",
                available.join(", ")
            );
            std::process::exit(2);
        }
    };
    let render = render_to_brr(&atom)
        .expect("AtomRenderError is uninhabited at M2.2 — render is infallible");

    // Decode the BRR back to PCM (one cycle), then repeat to fill
    // the requested duration at 32 kHz.
    let blocks: Vec<[u8; 9]> = render
        .brr_bytes
        .chunks_exact(9)
        .map(|c| {
            let mut b = [0u8; 9];
            b.copy_from_slice(c);
            b
        })
        .collect();
    let mut state = sfc_atomizer_core::brr::BrrDecoderState::default();
    let cycle = sfc_atomizer_core::brr::decode_blocks(&blocks, &mut state);
    let target_samples = (duration_seconds * 32000.0).round().max(0.0) as usize;
    let mut out = Vec::with_capacity(target_samples);
    while out.len() < target_samples {
        let remaining = target_samples - out.len();
        if remaining >= cycle.len() {
            out.extend_from_slice(&cycle);
        } else {
            out.extend_from_slice(&cycle[..remaining]);
        }
    }

    let project_dir = project_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let wav_path = out_wav.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        project_dir
            .join(".sfcwc-preview")
            .join("atoms")
            .join(format!("{atom_id}.audition.wav"))
    });
    if let Some(parent) = wav_path.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir(parent)?;
        }
    }
    sfc_atomizer_core::audition::write_pcm16_mono_wav_pub(&wav_path, &out, 32000).map_err(|e| {
        CliError::Io {
            path: wav_path.clone(),
            source: std::io::Error::other(format!("{e}")),
        }
    })?;

    eprintln!(
        "preview-atom: {} ({}) -> {} ({} s, {} samples, cycle={})",
        atom.id,
        atom.name,
        wav_path.display(),
        duration_seconds,
        out.len(),
        cycle.len(),
    );
    Ok(())
}

/// `sfcwc compile-sequence --project <v2> [--sequence-id <id>]`
///
/// Standalone driver of the M2.4 sequence compiler. Output paths
/// default to `build/m2/sequences/<id>.seq.bin` and
/// `<id>.sequence-compile-report.json` next to it. The report's
/// `voice_setup_addr` and `sequence_addr` are 0 here (not yet
/// packed); the same compiler runs inside `sfcwc pack` for v2
/// multi_voice_atom and fills those addresses from the AramMapReport.
fn cmd_compile_sequence(
    project_path: &Path,
    sequence_id: Option<&str>,
    out_bin: Option<&Path>,
    out_report: Option<&Path>,
) -> Result<(), CliError> {
    let v2 = match load_project_versioned(project_path) {
        Ok(LoadedProject::V1(_)) => {
            eprintln!(
                "compile-sequence: {} is a v1 project; compile-sequence requires v2.",
                project_path.display()
            );
            std::process::exit(2);
        }
        Ok(LoadedProject::V2(p)) => p,
        Err(e) => {
            eprintln!("compile-sequence: {e}");
            std::process::exit(match e {
                ProjectIoError::Validation(_) => 2,
                _ => 1,
            });
        }
    };
    if let Err(verrors) = v2.validate() {
        eprintln!(
            "compile-sequence: project invalid — {} ({} error{})",
            project_path.display(),
            verrors.len(),
            if verrors.len() == 1 { "" } else { "s" }
        );
        for e in &verrors {
            eprintln!("  {} : {}", e.path, e.kind);
        }
        std::process::exit(2);
    }

    let resolved_id: String = match sequence_id
        .map(|s| s.to_string())
        .or_else(|| v2.m2.active_sequence_id.clone())
    {
        Some(id) => id,
        None => {
            eprintln!(
                "compile-sequence: no active sequence; specify --sequence-id (m2.active_sequence_id is null in {})",
                project_path.display()
            );
            std::process::exit(2);
        }
    };
    let sequence = match v2.atom_sequences.iter().find(|s| s.id == resolved_id) {
        Some(s) => s.clone(),
        None => {
            let available: Vec<&str> = v2.atom_sequences.iter().map(|s| s.id.as_str()).collect();
            eprintln!(
                "compile-sequence: sequence id {resolved_id:?} not found in atom_sequences. available: [{}]",
                available.join(", ")
            );
            std::process::exit(2);
        }
    };

    let manifest = sfc_atomizer_core::capability_manifest::CapabilityManifest::multi_voice_atom();
    let source_directory = sfc_atomizer_core::sequence_compiler::SourceDirectory::from_project(&v2);
    let output = match sfc_atomizer_core::sequence_compiler::compile_sequence(
        sfc_atomizer_core::sequence_compiler::SequenceCompileInput {
            project: &v2,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        },
    ) {
        Ok(o) => o,
        Err(e) => {
            use sfc_atomizer_core::sequence_compiler::SequenceCompileError as SE;
            let exit = match &e {
                SE::CapabilityMissing { .. }
                | SE::AtomIdNotInPool { .. }
                | SE::FirstStepNotInitialKon { .. }
                | SE::NonFirstStepWrongTransition { .. } => 2,
                SE::WriteBudgetExceeded { .. }
                | SE::OverlappingSlides { .. }
                | SE::BytecodeTooLarge { .. }
                | SE::StepTooShortForTransition { .. } => 3,
            };
            eprintln!("compile-sequence: {e}");
            std::process::exit(exit);
        }
    };

    let out_bin_owned = out_bin.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        PathBuf::from("build/m2/sequences").join(format!("{resolved_id}.seq.bin"))
    });
    let out_report_owned = out_report.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        PathBuf::from("build/m2/sequences")
            .join(format!("{resolved_id}.sequence-compile-report.json"))
    });
    if let Some(parent) = out_bin_owned.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir(parent)?;
        }
    }
    std::fs::write(&out_bin_owned, &output.bytecode).map_err(|source| CliError::Io {
        path: out_bin_owned.clone(),
        source,
    })?;
    let report = build_sequence_compile_report(
        &v2.project.name,
        Some(&resolved_id),
        &output,
        0, // voice_setup_addr — pack fills
        0, // sequence_addr — pack fills
        0, // region bytes — pack fills
        0, // padding — pack fills
        &manifest,
    );
    write_json(&out_report_owned, &report)?;

    eprintln!(
        "compile-sequence: {} ({}) — {} step(s), {} bytes total ({} header + {} payload), {} ticks, max writes/tick estimate={} (budget {}); -> {}",
        resolved_id,
        sequence.name,
        sequence.steps.len(),
        output.bytecode.len(),
        sfc_atomizer_core::bytecode::SEQUENCE_HEADER_LEN,
        output.bytecode_payload_len,
        output.total_ticks,
        output.max_writes_per_tick_estimate,
        manifest.limits.max_dsp_writes_per_tick,
        out_bin_owned.display(),
    );
    Ok(())
}

fn default_migration_report_path(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    let parent = out.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}.migration-report.json"))
}

fn derive_project_name_from_path(out: &Path) -> String {
    let stem = out.file_stem().map(|s| s.to_string_lossy().into_owned());
    let raw = stem.unwrap_or_else(|| "untitled".to_string());
    // Drop a trailing ".sfcproj" if the user picked the recommended
    // double extension.
    raw.strip_suffix(".sfcproj")
        .map(str::to_string)
        .unwrap_or(raw)
}

fn validation_error_to_json(e: ValidationError) -> ValidationErrorJson {
    ValidationErrorJson {
        path: e.path.clone(),
        message: format!("{}", e.kind),
    }
}

fn project_io_to_cli(e: ProjectIoError, path: &Path) -> CliError {
    CliError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(format!("{e}")),
    }
}

fn cmd_import(project: &Path, audio: &Path, options: ImportOptions) -> Result<(), CliError> {
    match import_audio(project, audio, options) {
        Ok(r) => {
            let format = match r.metadata.format {
                AudioFormat::Wav => "wav",
                AudioFormat::Aiff => "aiff",
                AudioFormat::Brr => "brr",
            };
            let channels = match r.metadata.channels {
                1 => "mono",
                2 => "stereo",
                n => {
                    return Err(CliError::Io {
                        path: audio.to_path_buf(),
                        source: std::io::Error::other(format!("unexpected channel count {n}")),
                    })
                }
            };
            let sha_short = r.sha256.get(..8).unwrap_or(&r.sha256);
            eprintln!(
                "import: added {} ({:?}) — {} {} Hz {} {} frames; sha={}...",
                r.sample_id,
                r.stored_path,
                format,
                r.metadata.sample_rate_hz,
                channels,
                r.metadata.frames,
                sha_short,
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("import: {e}");
            let exit_code = match &e {
                ImportError::AudioNotFound(_) => 1,
                ImportError::Project(_) | ImportError::Io(_) => 1,
                ImportError::Audio(_) | ImportError::PathTraversal(_) => 2,
                ImportError::ResultingProjectInvalid(_) => 3,
            };
            std::process::exit(exit_code);
        }
    }
}

// =============================================================================
// pack (M1.4)
// =============================================================================

/// Outcome of [`compile_aram_image`] — a fully packed 64 KB image
/// plus the matching map report plus the driver-build attribution
/// (size + SHA) so callers can put both in their reports.
struct CompileAramOutcome {
    project: ProjectV1,
    image: Box<[u8; 0x10000]>,
    map_report: AramMapReport,
    driver_code_bytes: u32,
    driver_code_sha256: String,
}

/// Produce a 64 KB ARAM image from a project file: load → validate
/// → decode each sample → encode BRR → run M1.5 driver_build →
/// run the M1.4 packer with the real driver. `driver_override`
/// (used for `sfcwc pack --driver <path>`) bypasses driver_build
/// and feeds the raw bytes straight to the packer.
fn compile_aram_image(
    label: &str,
    project_path: &Path,
    driver_override: Option<Vec<u8>>,
    refresh_source_hash: bool,
) -> Result<CompileAramOutcome, ()> {
    // Load + validate.
    let mut project = match ProjectV1::load_from_path(project_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{label}: load failed for {}: {e}", project_path.display());
            std::process::exit(1);
        }
    };
    if let Err(verrors) = project.validate() {
        eprintln!(
            "{label}: project invalid — {} ({} error{})",
            project_path.display(),
            verrors.len(),
            if verrors.len() == 1 { "" } else { "s" },
        );
        for e in &verrors {
            eprintln!("  {} : {}", e.path, e.kind);
        }
        std::process::exit(2);
    }

    // Encode each sample's BRR.
    let project_dir = project_path.parent().unwrap_or_else(|| Path::new("."));
    let mut encoded: Vec<EncodedSample> = Vec::with_capacity(project.sample_pool.len());
    let mut hash_refreshes: Vec<(usize, String, String)> = Vec::new();
    for (idx, slot) in project.sample_pool.iter().enumerate() {
        let raw = Path::new(&slot.source.path);
        let audio_path: PathBuf = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            project_dir.join(raw)
        };

        // M2.0: enforce declared SHA against the live file. With
        // --refresh-source-hash, mismatches are converted into
        // pending project-update entries to apply after the
        // compile succeeds.
        match sfc_atomizer_core::audio::check_or_refresh_source_hash(
            &audio_path,
            &slot.id,
            &slot.source.sha256,
            refresh_source_hash,
        ) {
            Ok(sfc_atomizer_core::audio::SourceHashCheck::Match) => {}
            Ok(sfc_atomizer_core::audio::SourceHashCheck::Refreshed { previous, actual }) => {
                eprintln!(
                    "refresh-source-hash: {}: {} -> {}",
                    slot.id, previous, actual
                );
                hash_refreshes.push((idx, previous, actual));
            }
            Err(e) => {
                eprintln!("{label}: {e}");
                std::process::exit(2);
            }
        }

        let pcm = match decode_to_mono_pcm(&audio_path) {
            Ok(p) => p,
            Err(e) => {
                let exit = match &e {
                    AudioDecodeError::Probe(_) | AudioDecodeError::Io(_) => 1,
                    AudioDecodeError::Symphonia(_)
                    | AudioDecodeError::FrameCountMismatch { .. }
                    | AudioDecodeError::SourceHashMismatch { .. } => 2,
                };
                eprintln!("{label}: decode failed for sample {}: {e}", slot.id);
                std::process::exit(exit);
            }
        };
        let opts = EncodeOptions::default();
        let (bytes, loop_entry_block) = if slot.looped.enabled {
            match slot.looped.start_sample {
                Some(start) => {
                    let r = match encode_looped(&pcm, start, &opts) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("{label}: encode failed for sample {}: {e}", slot.id);
                            std::process::exit(3);
                        }
                    };
                    (r.bytes, Some(start / 16))
                }
                None => (brr_encode(&pcm, &opts).bytes, None),
            }
        } else {
            (brr_encode(&pcm, &opts).bytes, None)
        };
        encoded.push(EncodedSample {
            sample_id: slot.id.clone(),
            bytes,
            loop_entry_block,
        });
    }

    // Build the driver. With an override path we skip driver_build
    // entirely; otherwise we shadow-pack with an empty driver to
    // get the layout (src_dir_page, echo_esa) and feed that to
    // driver_build.
    let (driver_code, driver_code_sha256) = match driver_override {
        Some(bytes) => {
            let sha = sfc_atomizer_core::asm::sha256_hex(&bytes);
            (bytes, sha)
        }
        None => {
            let shadow = match packer_pack(PackInput {
                project: project.clone(),
                encoded_samples: encoded.clone(),
                driver_code: Vec::new(),
            }) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{label}: shadow pack failed: {e}");
                    std::process::exit(3);
                }
            };
            let work = match tempfile::tempdir() {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("{label}: tempdir: {e}");
                    std::process::exit(1);
                }
            };
            match driver_build(DriverBuildInput {
                project: &project,
                map_report: &shadow.map_report,
                source_override: None,
                working_dir: work.path().to_path_buf(),
            }) {
                Ok(out) => (out.driver_code, out.driver_code_sha256),
                Err(e) => {
                    eprintln!("{label}: driver_build failed: {e}");
                    std::process::exit(4);
                }
            }
        }
    };
    let driver_code_bytes = driver_code.len() as u32;

    // Real pack.
    let result = match packer_pack(PackInput {
        project: project.clone(),
        encoded_samples: encoded,
        driver_code,
    }) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{label}: {e}");
            std::process::exit(3);
        }
    };

    // Persist any --refresh-source-hash updates to the project on
    // disk, after we know the compile succeeded.
    if !hash_refreshes.is_empty() {
        for (idx, _prev, new) in &hash_refreshes {
            project.sample_pool[*idx].source.sha256 = new.clone();
        }
        if let Err(e) = project.save_to_path(project_path) {
            eprintln!(
                "{label}: refresh-source-hash: failed to save updated project at {}: {e}",
                project_path.display()
            );
            std::process::exit(1);
        }
    }

    Ok(CompileAramOutcome {
        project,
        image: result.aram_image,
        map_report: result.map_report,
        driver_code_bytes,
        driver_code_sha256,
    })
}

fn cmd_pack(
    project_path: &Path,
    out_image: &Path,
    out_map: &Path,
    driver_path: Option<&Path>,
    refresh_source_hash: bool,
    out_capability_manifest: Option<&Path>,
) -> Result<(), CliError> {
    // Peek the schema to decide between the v1-shim path (M1.4
    // bit-identical output) and the M2.3 multi-source pack_v2 path.
    let multi_voice = match load_project_versioned(project_path) {
        Ok(LoadedProject::V2(p)) if p.driver.profile == "multi_voice_atom" => Some(p),
        _ => None,
    };

    let manifest_path = out_capability_manifest
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_capability_manifest_path(out_map));

    if let Some(v2) = multi_voice {
        return cmd_pack_v2_multi_voice(
            project_path,
            &v2,
            out_image,
            out_map,
            &manifest_path,
            driver_path,
        );
    }

    // sample_basic / v1 / sample-only-equivalent v2 path: existing
    // M1.4 layout via the v1 shim.
    let driver_override = match driver_path {
        Some(p) => match std::fs::read(p) {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!("pack: driver read failed for {}: {e}", p.display());
                std::process::exit(1);
            }
        },
        None => None,
    };
    let v1_input = prepare_v1_input("pack", project_path);
    let outcome = compile_aram_image("pack", &v1_input.path, driver_override, refresh_source_hash)
        .expect("compile_aram_image returns via exit on error");
    let project = outcome.project;
    let result_image = outcome.image;
    let map_report = outcome.map_report;

    // Write image + map.
    if let Some(parent) = out_image.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir(parent)?;
        }
    }
    std::fs::write(out_image, &result_image[..]).map_err(|source| CliError::Io {
        path: out_image.to_path_buf(),
        source,
    })?;
    write_json(out_map, &map_report)?;
    let manifest = sfc_atomizer_core::capability_manifest::CapabilityManifest::sample_basic();
    write_json(&manifest_path, &manifest)?;

    let image_sha = sfc_atomizer_core::asm::sha256_hex(&result_image[..]);
    let summ = map_report.samples.as_ref();
    let total_brr = summ.map(|s| s.total_brr_bytes).unwrap_or(0);
    let echo_summ = map_report.echo.as_ref();
    let echo_label = match echo_summ {
        Some(e) if e.enabled => format!("EDL={} ({} B)", e.edl, e.buffer_bytes),
        _ => "off".to_string(),
    };
    let free = map_report.free_bytes;
    let free_pct = (free as f64) * 100.0 / 65536.0;
    eprintln!(
        "pack: {} sample{}, {} BRR bytes, echo {}, free={} B ({:.1}%); image -> {} (sha256={}); map -> {}; capability manifest (sample_basic) -> {}",
        project.sample_pool.len(),
        if project.sample_pool.len() == 1 { "" } else { "s" },
        total_brr,
        echo_label,
        free,
        free_pct,
        out_image.display(),
        image_sha,
        out_map.display(),
        manifest_path.display(),
    );
    Ok(())
}

fn default_capability_manifest_path(out_map: &Path) -> PathBuf {
    let parent = out_map.parent().unwrap_or_else(|| Path::new("."));
    let stem = out_map
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    parent.join(format!("{stem}.capability-manifest.json"))
}

/// M2.3 multi_voice_atom pack path. Renders every atom in the
/// project's atom_pool through `core::atom::render_to_brr`, builds
/// the voice setup table per SPEC §15.7, calls `pack_v2`, writes the
/// image / map / capability manifest. Sequence data is left empty
/// (M2.4 fills).
#[allow(clippy::too_many_arguments)]
fn build_sequence_compile_report(
    project_name: &str,
    active_sequence_id: Option<&str>,
    seq_out: &sfc_atomizer_core::sequence_compiler::SequenceCompileOutput,
    voice_setup_addr: u16,
    sequence_addr: u16,
    region_bytes: u32,
    padding_bytes: u32,
    manifest: &sfc_atomizer_core::capability_manifest::CapabilityManifest,
) -> SequenceCompileReport {
    use sfc_atomizer_core::report::SequenceStepLowering;
    let per_step = seq_out
        .per_step
        .iter()
        .map(|s| SequenceStepLowering {
            step_index: s.step_index,
            atom_id: s.atom_id.clone(),
            voice: s.voice,
            bytecode_offset_start: s.bytecode_offset_start,
            bytecode_offset_end: s.bytecode_offset_end,
            max_writes_in_step: s.max_writes_in_step,
            tick_offset_start: s.tick_offset_start,
            tick_offset_end: s.tick_offset_end,
        })
        .collect();
    SequenceCompileReport {
        schema_version: SCHEMA_VERSION,
        report_type: SequenceCompileReport::REPORT_TYPE.to_string(),
        project_name: project_name.to_string(),
        active_sequence_id: active_sequence_id.map(|s| s.to_string()),
        bytecode_sha256: seq_out.bytecode_sha256.clone(),
        bytecode_payload_bytes: seq_out.bytecode_payload_len as u32,
        bytecode_total_bytes: seq_out.bytecode.len() as u32,
        bytecode_region_bytes: region_bytes,
        bytecode_padding_bytes: padding_bytes,
        max_writes_per_tick_estimate: seq_out.max_writes_per_tick_estimate,
        max_writes_per_tick_budget: manifest.limits.max_dsp_writes_per_tick,
        max_simultaneous_volume_slides: manifest.limits.max_simultaneous_volume_slides as u32,
        total_ticks: seq_out.total_ticks,
        voice_setup_addr,
        sequence_addr,
        per_step,
    }
}

fn cmd_pack_v2_multi_voice(
    project_path: &Path,
    v2: &sfc_atomizer_core::project_v2::ProjectV2,
    out_image: &Path,
    out_map: &Path,
    manifest_path: &Path,
    driver_path: Option<&Path>,
) -> Result<(), CliError> {
    if let Err(verrors) = v2.validate() {
        eprintln!(
            "pack: v2 project invalid — {} ({} error{})",
            project_path.display(),
            verrors.len(),
            if verrors.len() == 1 { "" } else { "s" }
        );
        for e in &verrors {
            eprintln!("  {} : {}", e.path, e.kind);
        }
        std::process::exit(2);
    }

    // Driver-code blob: M2.5 ships the multi_voice_atom driver; M2.3
    // accepts a caller-provided override or zero-fills the budget.
    let driver_code = match driver_path {
        Some(p) => match std::fs::read(p) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("pack: driver read failed for {}: {e}", p.display());
                std::process::exit(1);
            }
        },
        None => Vec::new(),
    };

    // Encode samples through the same M1.3 path the v1 packer uses.
    let project_dir = project_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut encoded_samples: Vec<sfc_atomizer_core::packer::EncodedSample> =
        Vec::with_capacity(v2.sample_pool.len());
    for slot in &v2.sample_pool {
        let raw = Path::new(&slot.source.path);
        let audio_path = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            project_dir.join(raw)
        };
        let pcm = match decode_to_mono_pcm(&audio_path) {
            Ok(p) => p,
            Err(e) => {
                let exit = match &e {
                    AudioDecodeError::Probe(_) | AudioDecodeError::Io(_) => 1,
                    _ => 2,
                };
                eprintln!("pack: decode failed for sample {}: {e}", slot.id);
                std::process::exit(exit);
            }
        };
        let opts = EncodeOptions::default();
        let (bytes, loop_entry_block) = if slot.looped.enabled {
            match slot.looped.start_sample {
                Some(start) => match encode_looped(&pcm, start, &opts) {
                    Ok(r) => (r.bytes, Some(start / 16)),
                    Err(e) => {
                        eprintln!("pack: encode failed for sample {}: {e}", slot.id);
                        std::process::exit(3);
                    }
                },
                None => (brr_encode(&pcm, &opts).bytes, None),
            }
        } else {
            (brr_encode(&pcm, &opts).bytes, None)
        };
        encoded_samples.push(sfc_atomizer_core::packer::EncodedSample {
            sample_id: slot.id.clone(),
            bytes,
            loop_entry_block,
        });
    }

    // Render atoms via render_to_brr.
    let mut encoded_atoms: Vec<sfc_atomizer_core::packer::EncodedSample> =
        Vec::with_capacity(v2.atom_pool.len());
    for atom in &v2.atom_pool {
        let out =
            render_to_brr(atom).expect("AtomRenderError uninhabited at M2; render is infallible");
        encoded_atoms.push(sfc_atomizer_core::packer::EncodedSample {
            sample_id: atom.id.clone(),
            bytes: out.brr_bytes,
            // Atoms always loop, entry block = 0 (single-cycle).
            loop_entry_block: Some(0),
        });
    }

    let voice_setup_table = match sfc_atomizer_core::voice_setup::build_voice_setup_table(v2) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("pack: build voice setup table: {e}");
            std::process::exit(3);
        }
    };

    // M2.4: lower the active atom_sequence to SEQ2 bytecode if one
    // is selected. No active sequence means voice 1 stays silent
    // (its setup-table entry already has src_index=$FF or zeros);
    // pack_v2 sees `sequence_data: None` and omits the region.
    let manifest = sfc_atomizer_core::capability_manifest::CapabilityManifest::multi_voice_atom();
    let (sequence_data, sequence_compile_output) = match v2
        .m2
        .active_sequence_id
        .as_deref()
        .and_then(|id| v2.atom_sequences.iter().find(|s| s.id == id))
    {
        Some(seq) => {
            let source_directory =
                sfc_atomizer_core::sequence_compiler::SourceDirectory::from_project(v2);
            let out = match sfc_atomizer_core::sequence_compiler::compile_sequence(
                sfc_atomizer_core::sequence_compiler::SequenceCompileInput {
                    project: v2,
                    manifest: &manifest,
                    source_directory: &source_directory,
                    sequence: seq,
                },
            ) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("pack: sequence compile: {e}");
                    std::process::exit(3);
                }
            };
            (Some(out.bytecode.clone()), Some(out))
        }
        None => (None, None),
    };

    let result = sfc_atomizer_core::packer::pack_v2(sfc_atomizer_core::packer::PackInputV2 {
        project: v2.clone(),
        encoded_samples,
        encoded_atoms,
        driver_code,
        sequence_data,
        voice_setup_table,
    });
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("pack: {e}");
            std::process::exit(3);
        }
    };

    // Write outputs.
    if let Some(parent) = out_image.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir(parent)?;
        }
    }
    std::fs::write(out_image, &result.aram_image[..]).map_err(|source| CliError::Io {
        path: out_image.to_path_buf(),
        source,
    })?;
    write_json(out_map, &result.map_report)?;
    if let Err(e) = manifest.validate_dependencies() {
        eprintln!("pack: capability manifest dependency check failed: {e}");
        std::process::exit(3);
    }
    write_json(manifest_path, &manifest)?;

    // M2.4: write the sequence-compile report alongside if a
    // sequence was lowered. Pull the sequence_data region's start
    // address + size from the AramMapReport.
    if let Some(seq_out) = sequence_compile_output.as_ref() {
        let (sequence_addr, region_bytes, padding_bytes) = result
            .map_report
            .regions
            .iter()
            .find(|r| r.name == "sequence_data")
            .map(|r| {
                let start =
                    u32::from_str_radix(r.start.trim_start_matches("0x"), 16).unwrap_or(0) as u16;
                let region = r.bytes;
                let total = (seq_out.bytecode.len()) as u32;
                let padding = region.saturating_sub(total);
                (start, region, padding)
            })
            .unwrap_or((0, 0, 0));
        let voice_setup_addr = result
            .map_report
            .regions
            .iter()
            .find(|r| r.name == "voice_setup_table")
            .map(|r| u32::from_str_radix(r.start.trim_start_matches("0x"), 16).unwrap_or(0) as u16)
            .unwrap_or(0);
        let report = build_sequence_compile_report(
            &v2.project.name,
            v2.m2.active_sequence_id.as_deref(),
            seq_out,
            voice_setup_addr,
            sequence_addr,
            region_bytes,
            padding_bytes,
            &manifest,
        );
        let report_path = manifest_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
            .join(format!(
                "{}.sequence-compile-report.json",
                v2.m2.active_sequence_id.as_deref().unwrap_or("sequence")
            ));
        write_json(&report_path, &report)?;
    }

    let image_sha = sfc_atomizer_core::asm::sha256_hex(&result.aram_image[..]);
    let sample_brr = result
        .map_report
        .samples
        .as_ref()
        .map(|s| s.total_brr_bytes)
        .unwrap_or(0);
    let atom_brr = result
        .map_report
        .atoms
        .as_ref()
        .map(|a| a.total_brr_bytes)
        .unwrap_or(0);
    let echo_summ = result.map_report.echo.as_ref();
    let echo_label = match echo_summ {
        Some(e) if e.enabled => format!("EDL={} ({} B)", e.edl, e.buffer_bytes),
        _ => "off".to_string(),
    };
    let free = result.map_report.free_bytes;
    let free_pct = (free as f64) * 100.0 / 65536.0;
    eprintln!(
        "pack: {} sample{} ({} B), {} atom{} ({} B), echo {}, free={} B ({:.1}%); image -> {} (sha256={}); map -> {}; capability manifest (multi_voice_atom) -> {}",
        v2.sample_pool.len(),
        if v2.sample_pool.len() == 1 { "" } else { "s" },
        sample_brr,
        v2.atom_pool.len(),
        if v2.atom_pool.len() == 1 { "" } else { "s" },
        atom_brr,
        echo_label,
        free,
        free_pct,
        out_image.display(),
        image_sha,
        out_map.display(),
        manifest_path.display(),
    );
    Ok(())
}

// =============================================================================
// m1-acceptance / m1-status (M1.7)
// =============================================================================

fn cmd_m1_acceptance(
    project_a: &Path,
    project_b: Option<&Path>,
    out_dir: &Path,
    frames: u32,
) -> Result<(), CliError> {
    create_dir(out_dir)?;
    let workspace_root = std::env::current_dir().map_err(CliError::Cwd)?;

    let bin = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("sfcwc"));

    let doctor_path = out_dir.join("doctor.json");
    let validate_a_path = out_dir.join("validate-a.json");
    let validate_b_path = out_dir.join("validate-b.json");
    let aram_map_path = out_dir.join("aram-map.json");
    let aram_image_path = out_dir.join("project_a.aram.bin");
    let compile_spc_path = out_dir.join("compile-spc.json");
    let spc_path = out_dir.join("project_a.spc");
    let audible_spc_path = out_dir.join("audible-spc.json");
    let audible_spc_pcm_path = out_dir.join("audible-spc.pcm_s16le");
    let compile_sfc_path = out_dir.join("compile-sfc.json");
    let sfc_path = out_dir.join("project.sfc");
    let structure_sfc_path = out_dir.join("structure-sfc.json");
    let audible_sfc_path = out_dir.join("audible-sfc.json");
    let manifest_path = out_dir.join("manifest.json");

    // 1. Doctor.
    let _ = run_subcommand(&bin, &["doctor"], &[("--out", doctor_path.as_path())]);
    let doctor = read_report::<DoctorReport>(&doctor_path);
    eprintln!("m1-acceptance: doctor -> {}", doctor_path.display());

    // 2. Validate project A.
    let _ = run_subcommand(
        &bin,
        &["validate-project"],
        &[
            ("--project", project_a),
            ("--out", validate_a_path.as_path()),
        ],
    );
    let validate_a = read_report::<ValidationReport>(&validate_a_path);

    // 3. Validate project B (optional).
    let validate_b = match project_b {
        Some(p) => {
            let _ = run_subcommand(
                &bin,
                &["validate-project"],
                &[("--project", p), ("--out", validate_b_path.as_path())],
            );
            read_report::<ValidationReport>(&validate_b_path)
        }
        None => None,
    };

    // 4. compile-spc on project A.
    let _ = run_subcommand(
        &bin,
        &["compile-spc"],
        &[
            ("--project", project_a),
            ("--out-spc", spc_path.as_path()),
            ("--out-image", aram_image_path.as_path()),
            ("--out-map", aram_map_path.as_path()),
            ("--out-report", compile_spc_path.as_path()),
        ],
    );
    let compile_spc = read_report::<CompileSpcReport>(&compile_spc_path);
    let aram_map = read_report::<AramMapReport>(&aram_map_path);

    // 5. verify-spc-audible on the produced .spc.
    let _ = run_subcommand_with_kv(
        &bin,
        &["verify-spc-audible"],
        &[
            ("--spc", spc_path.as_path()),
            ("--out-report", audible_spc_path.as_path()),
            ("--out-pcm", audible_spc_pcm_path.as_path()),
        ],
        &[("--frames", frames.to_string())],
    );
    let audible_spc = read_report::<AudibleVerificationReport>(&audible_spc_path);

    // 6. compile-sfc on project A (and B if provided).
    let mut sfc_args: Vec<(&str, &Path)> = vec![
        ("--project-a", project_a),
        ("--out-sfc", sfc_path.as_path()),
        ("--out-report", compile_sfc_path.as_path()),
    ];
    if let Some(b) = project_b {
        sfc_args.push(("--project-b", b));
    }
    let _ = run_subcommand(&bin, &["compile-sfc"], &sfc_args);
    let compile_sfc = read_report::<CompileSfcReport>(&compile_sfc_path);

    // 7. verify-sfc-structure on the produced .sfc.
    let _ = run_subcommand(
        &bin,
        &["verify-sfc-structure"],
        &[
            ("--sfc", sfc_path.as_path()),
            ("--out-report", structure_sfc_path.as_path()),
        ],
    );
    let structure_sfc = read_report::<SfcStructureReport>(&structure_sfc_path);

    // 8. verify-sfc-modules-audible on the produced .sfc.
    let _ = run_subcommand_with_kv(
        &bin,
        &["verify-sfc-modules-audible"],
        &[
            ("--sfc", sfc_path.as_path()),
            ("--out-report", audible_sfc_path.as_path()),
        ],
        &[("--frames", frames.to_string())],
    );
    let audible_sfc = read_report::<SfcModulesAudibleReport>(&audible_sfc_path);

    // 9. Map step statuses.
    let asar_resolved = doctor
        .as_ref()
        .map(|d| d.tools.asar.resolved)
        .unwrap_or(false);
    let oracle_resolved = doctor
        .as_ref()
        .map(|d| d.tools.snes_spc_oracle.resolved)
        .unwrap_or(false);

    let steps = M1BundleSteps {
        doctor: doctor_step_status_m1(doctor.as_ref()),
        validate_a: validation_step_status(validate_a.as_ref()),
        validate_b: match (project_b, validate_b.as_ref()) {
            (None, _) => StepStatus::Skipped,
            (Some(_), Some(v)) => validation_step_status(Some(v)),
            (Some(_), None) => StepStatus::Error,
        },
        compile_spc: compile_spc_step_status(compile_spc.as_ref(), asar_resolved),
        audible_spc: audible_step_status(audible_spc.as_ref(), oracle_resolved),
        compile_sfc: compile_sfc_step_status(compile_sfc.as_ref(), asar_resolved),
        structure_sfc: structure_sfc_step_status(structure_sfc.as_ref()),
        audible_sfc: sfc_modules_audible_step_status(audible_sfc.as_ref(), oracle_resolved),
    };

    let bundle_status = aggregate_m1_bundle_status(&steps, project_b.is_some());

    // 10. Diagnostics rollup.
    let mut diagnostics: Vec<String> = Vec::new();
    if let Some(d) = doctor.as_ref() {
        for s in &d.diagnostics {
            diagnostics.push(format!("doctor: {s}"));
        }
    }
    if let Some(v) = validate_a.as_ref() {
        for e in &v.errors {
            diagnostics.push(format!("validate_a: {} {}", e.path, e.message));
        }
    }
    if let Some(v) = validate_b.as_ref() {
        for e in &v.errors {
            diagnostics.push(format!("validate_b: {} {}", e.path, e.message));
        }
    }
    if let Some(s) = structure_sfc.as_ref() {
        for f in &s.findings {
            diagnostics.push(format!("structure_sfc: {} {}", f.kind, f.message));
        }
    }
    if let Some(a) = audible_spc.as_ref() {
        if !matches!(a.status, AudibleStatus::Ok) {
            diagnostics.push(format!(
                "audible_spc: status={:?} max_abs={}",
                a.status, a.observed.max_abs
            ));
        }
    }
    if let Some(a) = audible_sfc.as_ref() {
        if !matches!(a.status, AudibleStatus::Ok) {
            diagnostics.push(format!(
                "audible_sfc: status={:?} A.max_abs={} B.max_abs={}",
                a.status,
                a.module_a_audible.observed.max_abs,
                a.module_b_audible
                    .as_ref()
                    .map(|m| m.observed.max_abs)
                    .unwrap_or(0)
            ));
        }
    }

    // 11. Cross-reference SHA fields for the bundle.
    let bundle = M1BundleSummary {
        steps: steps.clone(),
        status: bundle_status,
        aram_image_sha256: compile_spc.as_ref().map(|c| c.aram_image_sha256.clone()),
        spc_file_sha256: compile_spc.as_ref().map(|c| c.spc_file_sha256.clone()),
        sfc_file_sha256: compile_sfc.as_ref().map(|c| c.sfc_sha256.clone()),
        module_a_sha256: compile_sfc.as_ref().map(|c| c.module_a_sha256.clone()),
        module_b_sha256: compile_sfc.as_ref().and_then(|c| c.module_b_sha256.clone()),
        driver_code_sha256: compile_spc.as_ref().map(|c| c.driver_code_sha256.clone()),
        spc_audible_max_abs: audible_spc.as_ref().map(|a| a.observed.max_abs),
        sfc_audible_module_a_max_abs: audible_sfc
            .as_ref()
            .map(|a| a.module_a_audible.observed.max_abs),
        sfc_audible_module_b_max_abs: audible_sfc
            .as_ref()
            .and_then(|a| a.module_b_audible.as_ref().map(|m| m.observed.max_abs)),
        modules_audio_identical: audible_sfc.as_ref().map(|a| a.modules_audio_identical),
        diagnostics: diagnostics.clone(),
    };

    let _ = aram_map;
    let _ = workspace_root;

    let manifest_pre = M1Manifest {
        schema_version: SCHEMA_VERSION,
        report_type: M1Manifest::REPORT_TYPE.to_string(),
        generated_at: rfc3339_now(),
        project_a: project_a.display().to_string(),
        project_b: project_b.map(|p| p.display().to_string()),
        doctor_report: doctor_path.display().to_string(),
        validate_a_report: validate_a_path.display().to_string(),
        validate_b_report: project_b.map(|_| validate_b_path.display().to_string()),
        aram_map_report: aram_map_path.display().to_string(),
        compile_spc_report: compile_spc_path.display().to_string(),
        audible_spc_report: audible_spc_path.display().to_string(),
        compile_sfc_report: compile_sfc_path.display().to_string(),
        structure_sfc_report: structure_sfc_path.display().to_string(),
        audible_sfc_report: audible_sfc_path.display().to_string(),
        bundle: bundle.clone(),
    };
    write_json(&manifest_path, &manifest_pre)?;

    // 12. Run integrity check on the just-written bundle and fold
    //     findings into bundle diagnostics.
    let integrity = verify_m1_bundle(out_dir);
    let mut final_diagnostics = diagnostics.clone();
    for f in &integrity.findings {
        final_diagnostics.push(format!("integrity: {f}"));
    }
    truncate_diagnostics(&mut final_diagnostics);

    let final_bundle = M1BundleSummary {
        diagnostics: final_diagnostics,
        ..bundle
    };
    let manifest = M1Manifest {
        bundle: final_bundle,
        ..manifest_pre
    };
    write_json(&manifest_path, &manifest)?;

    eprintln!(
        "m1-acceptance: bundle.status={}; wrote 9 reports + manifest -> {}",
        bundle_status_label(bundle_status),
        manifest_path.display()
    );
    Ok(())
}

fn cmd_m1_status(bundle_dir: &Path, json: bool) -> Result<(), CliError> {
    let manifest_path = bundle_dir.join("manifest.json");
    let manifest_bytes = match std::fs::read(&manifest_path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!(
                "m1-status: no bundle at {} (run `sfcwc m1-acceptance` first)",
                bundle_dir.display()
            );
            std::process::exit(1);
        }
    };
    let manifest: M1Manifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("m1-status: cannot parse {}: {e}", manifest_path.display());
            std::process::exit(1);
        }
    };

    let integrity = verify_m1_bundle(bundle_dir);

    if json {
        let s = serde_json::to_string_pretty(&manifest)?;
        println!("{s}");
    } else {
        print_m1_status_human(&manifest, &integrity);
    }

    let bundle_ok = matches!(
        manifest.bundle.status,
        BundleStatus::Ok | BundleStatus::Degraded
    );
    let integrity_ok = integrity.all_reports_present
        && integrity.reports_parse
        && integrity.schema_versions_consistent
        && integrity.aram_sha_matches_across_reports
        && integrity.spc_sha_matches_across_reports
        && integrity.sfc_sha_matches_across_reports
        && integrity.module_a_sha_matches_across_reports;

    if bundle_ok && integrity_ok {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn print_m1_status_human(m: &M1Manifest, integrity: &M1BundleIntegrity) {
    println!("m1-status:");
    println!(
        "  bundle.status   = {}",
        bundle_status_label(m.bundle.status)
    );
    println!("  generated_at    = {}", m.generated_at);
    println!("  project_a       = {}", m.project_a);
    if let Some(b) = &m.project_b {
        println!("  project_b       = {b}");
    }
    println!("  steps:");
    let s = &m.bundle.steps;
    println!("    doctor        = {}", step_status_label(s.doctor));
    println!("    validate_a    = {}", step_status_label(s.validate_a));
    println!("    validate_b    = {}", step_status_label(s.validate_b));
    println!("    compile_spc   = {}", step_status_label(s.compile_spc));
    println!("    audible_spc   = {}", step_status_label(s.audible_spc));
    println!("    compile_sfc   = {}", step_status_label(s.compile_sfc));
    println!("    structure_sfc = {}", step_status_label(s.structure_sfc));
    println!("    audible_sfc   = {}", step_status_label(s.audible_sfc));
    println!("  cross-references:");
    println!(
        "    aram_image_sha256        = {}",
        m.bundle.aram_image_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    spc_file_sha256          = {}",
        m.bundle.spc_file_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    sfc_file_sha256          = {}",
        m.bundle.sfc_file_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    module_a_sha256          = {}",
        m.bundle.module_a_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    module_b_sha256          = {}",
        m.bundle.module_b_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    driver_code_sha256       = {}",
        m.bundle.driver_code_sha256.as_deref().unwrap_or("<absent>")
    );
    println!(
        "    spc_audible_max_abs      = {}",
        m.bundle
            .spc_audible_max_abs
            .map(|x| x.to_string())
            .unwrap_or_else(|| "<absent>".to_string())
    );
    println!(
        "    sfc_audible_a_max_abs    = {}",
        m.bundle
            .sfc_audible_module_a_max_abs
            .map(|x| x.to_string())
            .unwrap_or_else(|| "<absent>".to_string())
    );
    println!(
        "    sfc_audible_b_max_abs    = {}",
        m.bundle
            .sfc_audible_module_b_max_abs
            .map(|x| x.to_string())
            .unwrap_or_else(|| "<absent>".to_string())
    );
    println!(
        "    modules_audio_identical  = {}",
        m.bundle
            .modules_audio_identical
            .map(|b| b.to_string())
            .unwrap_or_else(|| "<absent>".to_string())
    );
    println!("  integrity:");
    println!(
        "    all_reports_present              = {}",
        integrity.all_reports_present
    );
    println!(
        "    reports_parse                    = {}",
        integrity.reports_parse
    );
    println!(
        "    schema_versions_consistent       = {}",
        integrity.schema_versions_consistent
    );
    println!(
        "    aram_sha_matches_across_reports  = {}",
        integrity.aram_sha_matches_across_reports
    );
    println!(
        "    spc_sha_matches_across_reports   = {}",
        integrity.spc_sha_matches_across_reports
    );
    println!(
        "    sfc_sha_matches_across_reports   = {}",
        integrity.sfc_sha_matches_across_reports
    );
    println!(
        "    module_a_sha_matches             = {}",
        integrity.module_a_sha_matches_across_reports
    );
    if !integrity.findings.is_empty() {
        println!("  integrity findings:");
        for f in integrity.findings.iter().take(10) {
            println!("    - {f}");
        }
        if integrity.findings.len() > 10 {
            println!("    ... ({} more truncated)", integrity.findings.len() - 10);
        }
    }
    if !m.bundle.diagnostics.is_empty() {
        println!("  diagnostics (top 5):");
        for d in m.bundle.diagnostics.iter().take(5) {
            println!("    - {d}");
        }
    }
}

fn run_subcommand(
    bin: &Path,
    args: &[&str],
    flag_paths: &[(&str, &Path)],
) -> std::process::ExitStatus {
    let mut cmd = std::process::Command::new(bin);
    for a in args {
        cmd.arg(a);
    }
    for (k, v) in flag_paths {
        cmd.arg(k).arg(v);
    }
    cmd.status()
        .unwrap_or_else(|_| std::process::ExitStatus::from_raw(127))
}

fn run_subcommand_with_kv(
    bin: &Path,
    args: &[&str],
    flag_paths: &[(&str, &Path)],
    flag_strs: &[(&str, String)],
) -> std::process::ExitStatus {
    let mut cmd = std::process::Command::new(bin);
    for a in args {
        cmd.arg(a);
    }
    for (k, v) in flag_paths {
        cmd.arg(k).arg(v);
    }
    for (k, v) in flag_strs {
        cmd.arg(k).arg(v);
    }
    cmd.status()
        .unwrap_or_else(|_| std::process::ExitStatus::from_raw(127))
}

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
trait ExitStatusFromRaw {
    fn from_raw(code: u32) -> std::process::ExitStatus;
}
#[cfg(windows)]
impl ExitStatusFromRaw for std::process::ExitStatus {
    fn from_raw(code: u32) -> std::process::ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(code)
    }
}

fn doctor_step_status_m1(d: Option<&DoctorReport>) -> StepStatus {
    match d {
        None => StepStatus::Error,
        Some(d) => {
            // M1.7 doctor mapping. Per the bundle aggregation rule
            // documented in STATUS, Mesen2 absence is informational
            // and must NOT downgrade the bundle. So we look at the
            // individual tool flags rather than the doctor's
            // overall enum (which counts Mesen2 missing as Warnings).
            //
            //   asar missing       → Error (M1 cannot ship)
            //   oracle missing     → Warnings on doctor; the audible
            //                        steps will Skip and the bundle
            //                        downgrades there
            //   mesen2 missing     → Ok (manual audition only)
            if !d.tools.asar.resolved {
                StepStatus::Error
            } else if !d.tools.snes_spc_oracle.resolved {
                StepStatus::Warnings
            } else {
                StepStatus::Ok
            }
        }
    }
}

fn validation_step_status(v: Option<&ValidationReport>) -> StepStatus {
    match v {
        Some(v) if matches!(v.status, ValidationStatus::Ok) => StepStatus::Ok,
        Some(_) => StepStatus::Error,
        None => StepStatus::Error,
    }
}

fn compile_spc_step_status(c: Option<&CompileSpcReport>, asar_resolved: bool) -> StepStatus {
    if !asar_resolved {
        return StepStatus::Skipped;
    }
    match c {
        Some(_) => StepStatus::Ok,
        None => StepStatus::Error,
    }
}

fn compile_sfc_step_status(c: Option<&CompileSfcReport>, asar_resolved: bool) -> StepStatus {
    if !asar_resolved {
        return StepStatus::Skipped;
    }
    match c {
        Some(_) => StepStatus::Ok,
        None => StepStatus::Error,
    }
}

fn structure_sfc_step_status(s: Option<&SfcStructureReport>) -> StepStatus {
    match s {
        Some(s) if matches!(s.status, SfcStructureStatus::Ok) => StepStatus::Ok,
        Some(_) => StepStatus::Error,
        None => StepStatus::Error,
    }
}

fn audible_step_status(a: Option<&AudibleVerificationReport>, oracle_resolved: bool) -> StepStatus {
    if !oracle_resolved {
        return StepStatus::Skipped;
    }
    match a {
        Some(a) => match a.status {
            AudibleStatus::Ok => StepStatus::Ok,
            AudibleStatus::SilentFail => StepStatus::Error,
            AudibleStatus::OracleError => StepStatus::Error,
        },
        None => StepStatus::Error,
    }
}

fn sfc_modules_audible_step_status(
    a: Option<&SfcModulesAudibleReport>,
    oracle_resolved: bool,
) -> StepStatus {
    if !oracle_resolved {
        return StepStatus::Skipped;
    }
    match a {
        Some(a) => match a.status {
            AudibleStatus::Ok => StepStatus::Ok,
            AudibleStatus::SilentFail => StepStatus::Error,
            AudibleStatus::OracleError => StepStatus::Error,
        },
        None => StepStatus::Error,
    }
}

/// Required steps: doctor, validate_a, compile_spc, audible_spc,
/// compile_sfc, structure_sfc, audible_sfc. Optional: validate_b
/// (Skipped is fine when no project_b given).
fn aggregate_m1_bundle_status(steps: &M1BundleSteps, has_project_b: bool) -> BundleStatus {
    let required = [
        steps.doctor,
        steps.validate_a,
        steps.compile_spc,
        steps.audible_spc,
        steps.compile_sfc,
        steps.structure_sfc,
        steps.audible_sfc,
    ];
    if required
        .iter()
        .any(|s| matches!(s, StepStatus::Error | StepStatus::Skipped))
    {
        return BundleStatus::Error;
    }
    if has_project_b && matches!(steps.validate_b, StepStatus::Error | StepStatus::Skipped) {
        return BundleStatus::Error;
    }
    if required.iter().any(|s| matches!(s, StepStatus::Warnings)) {
        return BundleStatus::Degraded;
    }
    BundleStatus::Ok
}

// =============================================================================
// compile-sfc / verify-sfc-structure / verify-sfc-modules-audible (M1.6)
// =============================================================================

fn cmd_compile_sfc(
    project_a_path: &Path,
    project_b_path: Option<&Path>,
    out_sfc: Option<&Path>,
    out_report: Option<&Path>,
    refresh_source_hash: bool,
) -> Result<(), CliError> {
    let stem_a = project_path_stem(project_a_path);
    let out_sfc_owned = out_sfc
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem_a}.sfc")));
    let out_report_owned = out_report.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        PathBuf::from("build/m1").join(format!("{stem_a}.compile-sfc-report.json"))
    });

    let work = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("compile-sfc: tempdir: {e}");
            std::process::exit(1);
        }
    };

    let v1_a = prepare_v1_input("compile-sfc", project_a_path);
    let v1_b = project_b_path.map(|p| prepare_v1_input("compile-sfc", p));
    let result = match export_sfc(SfcExportInput {
        project_a_path: v1_a.path.clone(),
        project_b_path: v1_b.as_ref().map(|i| i.path.clone()),
        loader_source_override: None,
        working_dir: work.path().to_path_buf(),
        out_sfc_path: out_sfc_owned.clone(),
        refresh_source_hash,
    }) {
        Ok(r) => r,
        Err(e) => {
            use sfc_atomizer_core::sfc_export::SfcExportError;
            let exit = match &e {
                SfcExportError::Load { .. } | SfcExportError::Io { .. } => 1,
                SfcExportError::Validation { .. } => 2,
                SfcExportError::Decode { .. } => 1,
                SfcExportError::Encode { .. } | SfcExportError::Pack { .. } => 3,
                SfcExportError::Module { .. } => 4,
                SfcExportError::Driver { .. } | SfcExportError::Assemble(_) => 5,
                SfcExportError::ModuleTooLarge(..) => 5,
            };
            eprintln!("compile-sfc: {e}");
            std::process::exit(exit);
        }
    };

    let report = CompileSfcReport {
        schema_version: SCHEMA_VERSION,
        report_type: CompileSfcReport::REPORT_TYPE.to_string(),
        project_a_name: result.module_a.project_name.clone(),
        project_b_name: if result.module_b_is_clone_of_a {
            None
        } else {
            Some(result.module_b.project_name.clone())
        },
        sfc_path: result.sfc_path.display().to_string(),
        sfc_size_bytes: result.sfc_size_bytes,
        sfc_sha256: result.sfc_sha256.clone(),
        module_b_is_clone_of_a: result.module_b_is_clone_of_a,
        module_a_sha256: result.module_a.module_file_sha256.clone(),
        module_a_in_file_sha256: result.module_a.module_in_file_sha256.clone(),
        module_a_bytes: result.module_a.module_bytes.len() as u32,
        module_b_sha256: if result.module_b_is_clone_of_a {
            None
        } else {
            Some(result.module_b.module_file_sha256.clone())
        },
        module_b_in_file_sha256: if result.module_b_is_clone_of_a {
            None
        } else {
            Some(result.module_b.module_in_file_sha256.clone())
        },
        module_b_bytes: if result.module_b_is_clone_of_a {
            None
        } else {
            Some(result.module_b.module_bytes.len() as u32)
        },
        loader_size_bytes: result.loader_size_bytes,
    };
    write_json(&out_report_owned, &report)?;

    let clone_label = if result.module_b_is_clone_of_a {
        " (clone)"
    } else {
        ""
    };
    eprintln!(
        "compile-sfc: A={} ({} B), B={}{} ({} B); .sfc={} B (sha={}); -> {}",
        result.module_a.project_name,
        result.module_a.module_bytes.len(),
        result.module_b.project_name,
        clone_label,
        result.module_b.module_bytes.len(),
        result.sfc_size_bytes,
        result.sfc_sha256,
        result.sfc_path.display(),
    );
    Ok(())
}

fn project_path_stem(p: &Path) -> String {
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    stem.strip_suffix(".sfcproj")
        .map(str::to_string)
        .unwrap_or(stem)
}

fn cmd_verify_sfc_structure(sfc_path: &Path, out_report: Option<&Path>) -> Result<(), CliError> {
    let stem = sfc_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sfc".to_string());
    let out_report_owned = out_report
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem}.structure-report.json")));

    let bytes = match std::fs::read(sfc_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("verify-sfc-structure: read {}: {e}", sfc_path.display());
            std::process::exit(1);
        }
    };

    let mut findings: Vec<SfcFinding> = Vec::new();

    // Power-of-two LoROM size.
    let valid_sizes = [
        256 * 1024,
        512 * 1024,
        1024 * 1024,
        2048 * 1024,
        4096 * 1024,
    ];
    if !valid_sizes.contains(&bytes.len()) {
        findings.push(SfcFinding {
            kind: "size".to_string(),
            message: format!(
                "file size {} not a LoROM power-of-two (256K..4M)",
                bytes.len()
            ),
        });
    }

    // Header.
    let title = if bytes.len() >= LOROM_HEADER_BASE + LOROM_HEADER_TITLE_LEN {
        String::from_utf8_lossy(
            &bytes[LOROM_HEADER_BASE..LOROM_HEADER_BASE + LOROM_HEADER_TITLE_LEN],
        )
        .into_owned()
    } else {
        String::new()
    };
    let mode_byte = bytes.get(LOROM_HEADER_MODE_OFFSET).copied().unwrap_or(0);
    let rom_size_byte = bytes.get(LOROM_HEADER_BASE + 0x17).copied().unwrap_or(0);
    let country_byte = bytes.get(LOROM_HEADER_BASE + 0x19).copied().unwrap_or(0);
    let checksum_complement = u16::from_le_bytes(
        bytes[LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET..LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET + 2]
            .try_into()
            .unwrap_or([0, 0]),
    );
    let checksum = u16::from_le_bytes(
        bytes[LOROM_HEADER_CHECKSUM_OFFSET..LOROM_HEADER_CHECKSUM_OFFSET + 2]
            .try_into()
            .unwrap_or([0, 0]),
    );
    let reset_vector = u16::from_le_bytes(
        bytes[LOROM_HEADER_RESET_VECTOR_OFFSET..LOROM_HEADER_RESET_VECTOR_OFFSET + 2]
            .try_into()
            .unwrap_or([0, 0]),
    );

    if mode_byte != 0x20 {
        findings.push(SfcFinding {
            kind: "header_mode".to_string(),
            message: format!("mode byte ${mode_byte:02X} != $20 (LoROM SlowROM)"),
        });
    }
    if country_byte != 0x01 {
        findings.push(SfcFinding {
            kind: "header_country".to_string(),
            message: format!("country byte ${country_byte:02X} != $01 (US)"),
        });
    }
    if !title.is_ascii() {
        findings.push(SfcFinding {
            kind: "header_title".to_string(),
            message: "title contains non-ASCII bytes".to_string(),
        });
    }
    if checksum_complement ^ checksum != 0xFFFF {
        findings.push(SfcFinding {
            kind: "checksum".to_string(),
            message: format!(
                "complement ${checksum_complement:04X} ^ checksum ${checksum:04X} = ${:04X}, want $FFFF",
                checksum_complement ^ checksum,
            ),
        });
    }
    if reset_vector < 0x8000 {
        findings.push(SfcFinding {
            kind: "reset_vector".to_string(),
            message: format!("reset vector ${reset_vector:04X} < $8000"),
        });
    }

    // Module summaries.
    let module_a_summary = parse_embedded_module(&bytes, MODULE_A_FILE_OFFSET, "A", &mut findings);
    let module_b_summary = parse_embedded_module(&bytes, MODULE_B_FILE_OFFSET, "B", &mut findings);

    let header_summary = SfcHeaderSummary {
        title: title.trim_end().to_string(),
        mode_byte,
        rom_size_byte,
        country_byte,
        checksum,
        checksum_complement,
        reset_vector,
        file_size_bytes: bytes.len() as u32,
    };

    let status = if findings.is_empty() {
        SfcStructureStatus::Ok
    } else {
        SfcStructureStatus::Fail
    };

    let report = SfcStructureReport {
        schema_version: SCHEMA_VERSION,
        report_type: SfcStructureReport::REPORT_TYPE.to_string(),
        sfc_path: sfc_path.display().to_string(),
        status,
        findings,
        header_summary,
        module_a_summary,
        module_b_summary: Some(module_b_summary),
    };
    write_json(&out_report_owned, &report)?;

    let nfind = report.findings.len();
    eprintln!(
        "verify-sfc-structure: status={} ({} finding{}); report -> {}",
        match status {
            SfcStructureStatus::Ok => "ok",
            SfcStructureStatus::Fail => "fail",
        },
        nfind,
        if nfind == 1 { "" } else { "s" },
        out_report_owned.display()
    );
    if status == SfcStructureStatus::Fail {
        std::process::exit(2);
    }
    Ok(())
}

fn parse_embedded_module(
    bytes: &[u8],
    embed_offset: usize,
    label: &str,
    findings: &mut Vec<SfcFinding>,
) -> SfcModuleSummary {
    if embed_offset + 64 > bytes.len() {
        findings.push(SfcFinding {
            kind: format!("module_{label}_offset"),
            message: format!("embed offset ${embed_offset:X} past file end"),
        });
        return zero_module_summary(embed_offset);
    }
    // Parse just the first 64 bytes for header. We need block table
    // + data so feed enough slice. Module length is at $18..$1C.
    let mod_total_len = u32::from_le_bytes(
        bytes[embed_offset + 0x18..embed_offset + 0x1C]
            .try_into()
            .unwrap_or([0; 4]),
    );
    let mod_end = embed_offset + mod_total_len as usize;
    if mod_end > bytes.len() {
        findings.push(SfcFinding {
            kind: format!("module_{label}_size"),
            message: format!("module total_file_len {mod_total_len} runs past file end"),
        });
        return zero_module_summary(embed_offset);
    }
    let module_slice = &bytes[embed_offset..mod_end];
    let header = match parse_module_header(module_slice) {
        Ok(h) => h,
        Err(e) => {
            findings.push(SfcFinding {
                kind: format!("module_{label}_parse"),
                message: format!("{e}"),
            });
            return zero_module_summary(embed_offset);
        }
    };
    if !header.magic_ok {
        findings.push(SfcFinding {
            kind: format!("module_{label}_magic"),
            message: format!("magic != {:?}", MODULE_MAGIC),
        });
    }
    if header.schema_version != 1 {
        findings.push(SfcFinding {
            kind: format!("module_{label}_schema"),
            message: format!("schema {} != 1", header.schema_version),
        });
    }
    if header.header_len != 64 {
        findings.push(SfcFinding {
            kind: format!("module_{label}_header_len"),
            message: format!("header_len {} != 64", header.header_len),
        });
    }
    if header.entrypoint != 0x0200 {
        findings.push(SfcFinding {
            kind: format!("module_{label}_entry"),
            message: format!("entrypoint ${:04X} != $0200", header.entrypoint),
        });
    }
    let blocks = match parse_module_blocks(module_slice, &header) {
        Ok(b) => b,
        Err(e) => {
            findings.push(SfcFinding {
                kind: format!("module_{label}_blocks"),
                message: format!("{e}"),
            });
            Vec::new()
        }
    };
    if blocks.is_empty() && header.block_count > 0 {
        findings.push(SfcFinding {
            kind: format!("module_{label}_blocks"),
            message: "block table parse failed".to_string(),
        });
    }
    let mut prev_addr: Option<u16> = None;
    for b in &blocks {
        if b.dest_addr < 0x0200 {
            findings.push(SfcFinding {
                kind: format!("module_{label}_block_below_driver"),
                message: format!("block @${:04X} below $0200", b.dest_addr),
            });
        }
        let end = b.dest_addr as u32 + b.length as u32;
        if (b.dest_addr as u32) < 0x0100 && end > 0x00F0 {
            findings.push(SfcFinding {
                kind: format!("module_{label}_block_io"),
                message: format!("block @${:04X} intersects $00F0..$00FF", b.dest_addr),
            });
        }
        if let Some(prev) = prev_addr {
            if b.dest_addr <= prev {
                findings.push(SfcFinding {
                    kind: format!("module_{label}_block_unsorted"),
                    message: format!(
                        "block @${:04X} not strictly above prev ${prev:04X}",
                        b.dest_addr
                    ),
                });
            }
        }
        prev_addr = Some(b.dest_addr);
    }

    // SHA: in-file value vs recomputed.
    let mut in_file_sha_hex = String::with_capacity(64);
    for b in &header.content_sha256_in_file {
        use std::fmt::Write as _;
        let _ = write!(in_file_sha_hex, "{b:02x}");
    }
    let recomputed = recompute_in_file_sha(module_slice);
    let matches = in_file_sha_hex == recomputed;
    if !matches {
        findings.push(SfcFinding {
            kind: format!("module_{label}_sha"),
            message: format!("in-file SHA {in_file_sha_hex} != recomputed {recomputed}"),
        });
    }

    SfcModuleSummary {
        embed_offset: embed_offset as u32,
        magic_ok: header.magic_ok,
        schema_version: header.schema_version,
        block_count: header.block_count,
        entrypoint: header.entrypoint,
        total_file_len: header.total_file_len,
        flags: header.flags,
        in_file_sha256: in_file_sha_hex,
        recomputed_in_file_sha256: recomputed,
        in_file_sha_matches: matches,
    }
}

fn zero_module_summary(embed_offset: usize) -> SfcModuleSummary {
    SfcModuleSummary {
        embed_offset: embed_offset as u32,
        magic_ok: false,
        schema_version: 0,
        block_count: 0,
        entrypoint: 0,
        total_file_len: 0,
        flags: 0,
        in_file_sha256: String::new(),
        recomputed_in_file_sha256: String::new(),
        in_file_sha_matches: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_verify_sfc_modules_audible(
    sfc_path: &Path,
    frames: u32,
    out_report: Option<&Path>,
    min_max_abs: u32,
    min_rms: f64,
    oracle: Option<&Path>,
    out_wav_a: Option<&Path>,
    out_wav_b: Option<&Path>,
) -> Result<(), CliError> {
    let stem = sfc_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sfc".to_string());
    let out_report_owned = out_report.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        PathBuf::from("build/m1").join(format!("{stem}.modules-audible-report.json"))
    });

    let workspace_root = std::env::current_dir().map_err(CliError::Cwd)?;
    let oracle_path = match resolve_oracle(oracle, &workspace_root) {
        Some(p) => p,
        None => {
            eprintln!(
                "verify-sfc-modules-audible: oracle wrapper not resolved (set SFCWC_SNES_SPC_ORACLE)"
            );
            std::process::exit(1);
        }
    };

    let sfc_bytes = match std::fs::read(sfc_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(
                "verify-sfc-modules-audible: read {}: {e}",
                sfc_path.display()
            );
            std::process::exit(1);
        }
    };

    let work = match tempfile::tempdir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("verify-sfc-modules-audible: tempdir: {e}");
            std::process::exit(1);
        }
    };

    let module_a_audible = render_module_audible(
        &sfc_bytes,
        MODULE_A_FILE_OFFSET,
        sfc_path,
        "A",
        frames,
        min_max_abs,
        min_rms,
        &oracle_path,
        work.path(),
        out_wav_a,
    );
    let module_b_audible = render_module_audible(
        &sfc_bytes,
        MODULE_B_FILE_OFFSET,
        sfc_path,
        "B",
        frames,
        min_max_abs,
        min_rms,
        &oracle_path,
        work.path(),
        out_wav_b,
    );

    let modules_audio_identical = module_a_audible.spc_sha256 == module_b_audible.spc_sha256;
    let any_silent = matches!(
        module_a_audible.status,
        AudibleStatus::SilentFail | AudibleStatus::OracleError
    ) || matches!(
        module_b_audible.status,
        AudibleStatus::SilentFail | AudibleStatus::OracleError
    );

    let status = if matches!(module_a_audible.status, AudibleStatus::OracleError)
        || matches!(module_b_audible.status, AudibleStatus::OracleError)
    {
        AudibleStatus::OracleError
    } else if any_silent {
        AudibleStatus::SilentFail
    } else {
        AudibleStatus::Ok
    };

    let report = SfcModulesAudibleReport {
        schema_version: SCHEMA_VERSION,
        report_type: SfcModulesAudibleReport::REPORT_TYPE.to_string(),
        sfc_path: sfc_path.display().to_string(),
        status,
        module_a_audible: module_a_audible.clone(),
        module_b_audible: Some(module_b_audible.clone()),
        modules_audio_identical,
        error: None,
    };
    write_json(&out_report_owned, &report)?;

    eprintln!(
        "verify-sfc-modules-audible: A={{max_abs={}, rms={:.1}}}, B={{max_abs={}, rms={:.1}}}, identical={}, status={}; report -> {}",
        module_a_audible.observed.max_abs,
        module_a_audible.observed.rms,
        module_b_audible.observed.max_abs,
        module_b_audible.observed.rms,
        modules_audio_identical,
        match status {
            AudibleStatus::Ok => "ok",
            AudibleStatus::SilentFail => "silent_fail",
            AudibleStatus::OracleError => "oracle_error",
        },
        out_report_owned.display(),
    );

    match status {
        AudibleStatus::Ok => Ok(()),
        AudibleStatus::SilentFail => std::process::exit(2),
        AudibleStatus::OracleError => std::process::exit(1),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_module_audible(
    sfc_bytes: &[u8],
    embed_offset: usize,
    sfc_path: &Path,
    label: &str,
    frames: u32,
    min_max_abs: u32,
    min_rms: f64,
    oracle_path: &Path,
    work_dir: &Path,
    out_wav: Option<&Path>,
) -> AudibleVerificationReport {
    let mut report = AudibleVerificationReport {
        schema_version: SCHEMA_VERSION,
        report_type: AudibleVerificationReport::REPORT_TYPE.to_string(),
        spc_path: format!("{}#module_{label}", sfc_path.display()),
        spc_sha256: String::new(),
        frames_rendered: 0,
        sample_rate_hz: 32_000,
        observed: ObservedAudio {
            max_abs: 0,
            rms: 0.0,
            bytes_zero: 0,
            bytes_total: 0,
            fraction_zero: 0.0,
        },
        thresholds: AudibleThresholds {
            min_max_abs,
            min_rms,
        },
        status: AudibleStatus::OracleError,
        error: None,
    };

    if embed_offset + 64 > sfc_bytes.len() {
        report.error = Some(format!("module {label} embed past file end"));
        return report;
    }
    let mod_total_len = u32::from_le_bytes(
        sfc_bytes[embed_offset + 0x18..embed_offset + 0x1C]
            .try_into()
            .unwrap_or([0; 4]),
    );
    let mod_end = embed_offset + mod_total_len as usize;
    if mod_end > sfc_bytes.len() {
        report.error = Some(format!("module {label} runs past file"));
        return report;
    }
    let module_slice = &sfc_bytes[embed_offset..mod_end];
    let header = match parse_module_header(module_slice) {
        Ok(h) => h,
        Err(e) => {
            report.error = Some(format!("module {label} parse: {e}"));
            return report;
        }
    };
    let blocks = match parse_module_blocks(module_slice, &header) {
        Ok(b) => b,
        Err(e) => {
            report.error = Some(format!("module {label} blocks: {e}"));
            return report;
        }
    };
    let aram = project_blocks_to_aram(module_slice, &header, &blocks);

    // Wrap as M1 SPC and write to scratch.
    let spc = match build_m1_image(aram.to_vec()) {
        Ok(s) => s,
        Err(e) => {
            report.error = Some(format!("build_m1_image: {e:?}"));
            return report;
        }
    };
    let spc_bytes = match spc.to_bytes() {
        Ok(b) => b,
        Err(e) => {
            report.error = Some(format!("spc.to_bytes: {e:?}"));
            return report;
        }
    };
    let spc_path = work_dir.join(format!("module_{label}.spc"));
    if let Err(e) = std::fs::write(&spc_path, &spc_bytes) {
        report.error = Some(format!("write spc: {e}"));
        return report;
    }
    let pcm_path = work_dir.join(format!("module_{label}.pcm"));
    let oracle_report = work_dir.join(format!("module_{label}.oracle.json"));

    let output = std::process::Command::new(oracle_path)
        .arg("render")
        .arg("--input-spc")
        .arg(&spc_path)
        .arg("--frames")
        .arg(frames.to_string())
        .arg("--output-pcm")
        .arg(&pcm_path)
        .arg("--report")
        .arg(&oracle_report)
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            report.error = Some(format!("spawn oracle: {e}"));
            return report;
        }
    };
    if !output.status.success() {
        report.error = Some(format!(
            "oracle exited {}",
            output.status.code().unwrap_or(-1)
        ));
        return report;
    }

    let pcm = match std::fs::read(&pcm_path) {
        Ok(b) => b,
        Err(e) => {
            report.error = Some(format!("read pcm: {e}"));
            return report;
        }
    };
    if let Some(wav_path) = out_wav {
        if let Some(parent) = wav_path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    report.error = Some(format!("write WAV (mkdir): {e}"));
                    return report;
                }
            }
        }
        if let Err(e) =
            sfc_atomizer_core::audition::write_oracle_pcm_to_mono_wav(wav_path, &pcm, 32_000)
        {
            report.error = Some(format!("write WAV: {e}"));
            return report;
        }
    }
    let (max_abs, rms) = pcm_stats_from_bytes(&pcm);
    let bytes_zero = pcm.iter().filter(|&&b| b == 0).count() as u32;
    let bytes_total = pcm.len() as u32;
    let fraction_zero = bytes_zero as f64 / bytes_total as f64;

    report.frames_rendered = frames;
    report.observed = ObservedAudio {
        max_abs: max_abs as u32,
        rms,
        bytes_zero,
        bytes_total,
        fraction_zero,
    };
    report.spc_sha256 = sfc_atomizer_core::asm::sha256_hex(&pcm);
    report.status = if (max_abs as u32) < min_max_abs || rms < min_rms {
        AudibleStatus::SilentFail
    } else {
        AudibleStatus::Ok
    };
    report
}

// =============================================================================
// compile-spc / verify-spc-audible (M1.5)
// =============================================================================

fn cmd_compile_spc(
    project_path: &Path,
    out_spc: Option<&Path>,
    out_image: Option<&Path>,
    out_map: Option<&Path>,
    out_report: Option<&Path>,
    refresh_source_hash: bool,
) -> Result<(), CliError> {
    let v1_input = prepare_v1_input("compile-spc", project_path);
    let outcome = compile_aram_image("compile-spc", &v1_input.path, None, refresh_source_hash)
        .expect("compile_aram_image returns via exit on error");
    let project = &outcome.project;

    let stem = project_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.project.name.clone());
    // Strip .sfcproj suffix if present (matches CLI new-project filename).
    let stem = stem
        .strip_suffix(".sfcproj")
        .map(str::to_string)
        .unwrap_or(stem);

    let out_spc_owned = out_spc
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem}.spc")));
    let out_image_owned = out_image
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem}.aram.bin")));
    let out_map_owned = out_map
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem}.aram-map.json")));
    let out_report_owned = out_report
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem}.compile-report.json")));

    // Write ARAM image + map.
    if let Some(p) = out_image_owned.parent() {
        if !p.as_os_str().is_empty() {
            create_dir(p)?;
        }
    }
    std::fs::write(&out_image_owned, &outcome.image[..]).map_err(|source| CliError::Io {
        path: out_image_owned.clone(),
        source,
    })?;
    write_json(&out_map_owned, &outcome.map_report)?;

    // Build the SPC image (M1 contract: zero DSP regs; driver
    // writes FLG=$60 on its first instruction).
    let aram_vec: Vec<u8> = outcome.image[..].to_vec();
    let spc_image: SpcImage =
        build_m1_image(aram_vec).expect("build_m1_image with valid 64 KB input");
    let spc_bytes = spc_image
        .to_bytes()
        .expect("to_bytes on validated M1 image");
    if let Some(p) = out_spc_owned.parent() {
        if !p.as_os_str().is_empty() {
            create_dir(p)?;
        }
    }
    std::fs::write(&out_spc_owned, &spc_bytes).map_err(|source| CliError::Io {
        path: out_spc_owned.clone(),
        source,
    })?;

    let aram_image_sha256 = sfc_atomizer_core::asm::sha256_hex(&outcome.image[..]);
    let spc_file_sha256 = sfc_atomizer_core::asm::sha256_hex(&spc_bytes);

    let report = CompileSpcReport {
        schema_version: SCHEMA_VERSION,
        report_type: CompileSpcReport::REPORT_TYPE.to_string(),
        project_name: project.project.name.clone(),
        active_sample_id: project.m1.active_sample_id.clone(),
        aram_image_sha256: aram_image_sha256.clone(),
        spc_file_sha256: spc_file_sha256.clone(),
        driver_code_sha256: outcome.driver_code_sha256.clone(),
        driver_code_bytes: outcome.driver_code_bytes,
        map_report_path: out_map_owned.display().to_string(),
        spc_path: out_spc_owned.display().to_string(),
        aram_image_path: out_image_owned.display().to_string(),
    };
    write_json(&out_report_owned, &report)?;

    eprintln!(
        "compile-spc: project={:?} sample={}, driver={} B (sha={}), image={} B (sha={}), spc={} B (sha={}); -> {}",
        project.project.name,
        project.m1.active_sample_id,
        outcome.driver_code_bytes,
        outcome.driver_code_sha256,
        outcome.image.len(),
        aram_image_sha256,
        spc_bytes.len(),
        spc_file_sha256,
        out_spc_owned.display(),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_verify_spc_audible(
    spc_path: &Path,
    frames: u32,
    out_report: Option<&Path>,
    out_pcm: Option<&Path>,
    min_max_abs: u32,
    min_rms: f64,
    oracle: Option<&Path>,
    out_wav: Option<&Path>,
) -> Result<(), CliError> {
    let stem = spc_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "spc".to_string());
    let out_report_owned = out_report
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem}.audible-report.json")));
    let out_pcm_owned = out_pcm
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("build/m1").join(format!("{stem}.audible.pcm_s16le")));

    let mut report = AudibleVerificationReport {
        schema_version: SCHEMA_VERSION,
        report_type: AudibleVerificationReport::REPORT_TYPE.to_string(),
        spc_path: spc_path.display().to_string(),
        spc_sha256: sfc_atomizer_core::asm::sha256_hex_file(spc_path).unwrap_or_default(),
        frames_rendered: 0,
        sample_rate_hz: 32_000,
        observed: ObservedAudio {
            max_abs: 0,
            rms: 0.0,
            bytes_zero: 0,
            bytes_total: 0,
            fraction_zero: 0.0,
        },
        thresholds: AudibleThresholds {
            min_max_abs,
            min_rms,
        },
        status: AudibleStatus::OracleError,
        error: None,
    };

    let workspace_root = std::env::current_dir().map_err(CliError::Cwd)?;
    let oracle_path = match resolve_oracle(oracle, &workspace_root) {
        Some(p) => p,
        None => {
            report.error = Some(
                "oracle wrapper not resolved (set SFCWC_SNES_SPC_ORACLE or build it under tools/snes_spc_oracle/build/Release)".to_string(),
            );
            write_json(&out_report_owned, &report)?;
            eprintln!(
                "verify-spc-audible: oracle wrapper not resolved; report -> {}",
                out_report_owned.display()
            );
            std::process::exit(1);
        }
    };

    if !spc_path.is_file() {
        report.error = Some(format!("input SPC missing: {}", spc_path.display()));
        write_json(&out_report_owned, &report)?;
        eprintln!(
            "verify-spc-audible: input SPC missing at {}",
            spc_path.display()
        );
        std::process::exit(1);
    }

    if let Some(p) = out_pcm_owned.parent() {
        if !p.as_os_str().is_empty() {
            create_dir(p)?;
        }
    }

    // Wrapper's own report sidecar (M0.5 pattern).
    let mut wrapper_report = out_report_owned.clone();
    let wrapper_name = match wrapper_report.file_name() {
        Some(n) => format!("{}.oracle-side.json", n.to_string_lossy()),
        None => "oracle-side.json".to_string(),
    };
    wrapper_report.set_file_name(wrapper_name);

    let output = std::process::Command::new(&oracle_path)
        .arg("render")
        .arg("--input-spc")
        .arg(spc_path)
        .arg("--frames")
        .arg(frames.to_string())
        .arg("--output-pcm")
        .arg(&out_pcm_owned)
        .arg("--report")
        .arg(&wrapper_report)
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => {
            report.error = Some(format!("spawn oracle: {e}"));
            write_json(&out_report_owned, &report)?;
            eprintln!(
                "verify-spc-audible: cannot spawn oracle ({e}); report -> {}",
                out_report_owned.display()
            );
            std::process::exit(1);
        }
    };
    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        report.error = Some(format!("oracle exited {code}: {}", first_line(&stderr)));
        write_json(&out_report_owned, &report)?;
        eprintln!(
            "verify-spc-audible: oracle exited {code}; report -> {}",
            out_report_owned.display()
        );
        std::process::exit(1);
    }

    let pcm_bytes = match std::fs::read(&out_pcm_owned) {
        Ok(b) => b,
        Err(e) => {
            report.error = Some(format!("read oracle PCM: {e}"));
            write_json(&out_report_owned, &report)?;
            eprintln!(
                "verify-spc-audible: cannot read oracle PCM: {e}; report -> {}",
                out_report_owned.display()
            );
            std::process::exit(1);
        }
    };
    let expected_pcm_bytes = (frames as usize) * 4;
    if pcm_bytes.len() != expected_pcm_bytes {
        report.error = Some(format!(
            "oracle PCM wrong size: expected {}, got {}",
            expected_pcm_bytes,
            pcm_bytes.len()
        ));
        write_json(&out_report_owned, &report)?;
        eprintln!(
            "verify-spc-audible: PCM wrong size; report -> {}",
            out_report_owned.display()
        );
        std::process::exit(1);
    }

    if let Some(wav_path) = out_wav {
        if let Some(parent) = wav_path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir(parent)?;
            }
        }
        if let Err(e) =
            sfc_atomizer_core::audition::write_oracle_pcm_to_mono_wav(wav_path, &pcm_bytes, 32_000)
        {
            report.error = Some(format!("write WAV: {e}"));
            write_json(&out_report_owned, &report)?;
            eprintln!(
                "verify-spc-audible: write WAV failed: {e}; report -> {}",
                out_report_owned.display()
            );
            std::process::exit(1);
        }
    }

    let (max_abs, rms) = pcm_stats_from_bytes(&pcm_bytes);
    let bytes_zero = pcm_bytes.iter().filter(|&&b| b == 0).count() as u32;
    let bytes_total = pcm_bytes.len() as u32;
    let fraction_zero = (bytes_zero as f64) / (bytes_total as f64);
    report.frames_rendered = frames;
    report.observed = ObservedAudio {
        max_abs: max_abs as u32,
        rms,
        bytes_zero,
        bytes_total,
        fraction_zero,
    };

    let status = if (max_abs as u32) < min_max_abs || rms < min_rms {
        AudibleStatus::SilentFail
    } else {
        AudibleStatus::Ok
    };
    report.status = status;

    write_json(&out_report_owned, &report)?;

    let status_label = match status {
        AudibleStatus::Ok => "ok",
        AudibleStatus::SilentFail => "silent_fail",
        AudibleStatus::OracleError => "oracle_error",
    };
    eprintln!(
        "verify-spc-audible: {} frames, max_abs={}, rms={:.1}, status={}; report -> {}",
        frames,
        max_abs,
        rms,
        status_label,
        out_report_owned.display()
    );

    if status == AudibleStatus::SilentFail {
        std::process::exit(2);
    }
    Ok(())
}

// =============================================================================
// encode-brr / preview-brr / find-loop-candidates (M1.3)
// =============================================================================

fn cmd_encode_brr(
    audio: &Path,
    out_brr: &Path,
    out_report: Option<&Path>,
    loop_start_sample: Option<u32>,
    force_filter_0_first_block: bool,
) -> Result<(), CliError> {
    let metadata = match probe(audio) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("encode-brr: probe failed for {}: {e}", audio.display());
            std::process::exit(1);
        }
    };
    let pcm = match decode_to_mono_pcm(audio) {
        Ok(p) => p,
        Err(e) => {
            let exit = match &e {
                AudioDecodeError::Probe(_) | AudioDecodeError::Io(_) => 1,
                AudioDecodeError::Symphonia(_)
                | AudioDecodeError::FrameCountMismatch { .. }
                | AudioDecodeError::SourceHashMismatch { .. } => 2,
            };
            eprintln!("encode-brr: decode failed for {}: {e}", audio.display());
            std::process::exit(exit);
        }
    };

    let opts = EncodeOptions {
        force_filter_0_first_block,
        loop_entry_block_index: None,
    };
    let encode_result = match loop_start_sample {
        Some(start) => match encode_looped(&pcm, start, &opts) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("encode-brr: {e}");
                std::process::exit(2);
            }
        },
        None => brr_encode(&pcm, &opts),
    };

    if let Some(parent) = out_brr.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir(parent)?;
        }
    }
    std::fs::write(out_brr, &encode_result.bytes).map_err(|source| CliError::Io {
        path: out_brr.to_path_buf(),
        source,
    })?;

    let source_sha = sfc_atomizer_core::asm::sha256_hex_file(audio).unwrap_or_default();
    let output_sha = sfc_atomizer_core::asm::sha256_hex(&encode_result.bytes);

    let summary = encode_result.summary;
    let report = BrrEncodeReport {
        schema_version: SCHEMA_VERSION,
        report_type: BrrEncodeReport::REPORT_TYPE.to_string(),
        source_path: audio.display().to_string(),
        source_sha256: source_sha,
        source_frames: metadata.frames,
        source_sample_rate_hz: metadata.sample_rate_hz,
        output_path: out_brr.display().to_string(),
        output_sha256: output_sha,
        output_bytes: encode_result.bytes.len() as u64,
        total_blocks: summary.total_blocks,
        overall_rms_error: summary.overall_rms_error,
        overall_peak_error: summary.overall_peak_error,
        total_clamp_count: summary.total_clamp_count,
        filter_distribution: summary.filter_distribution,
        force_filter_0_first_block,
        loop_start_sample,
        loop_entry_block_index: loop_start_sample.map(|s| s / 16),
        loop_click_score: summary.loop_click_score,
        blocks: encode_result
            .blocks
            .iter()
            .map(|b| BrrEncodeBlock {
                index: b.index,
                filter: b.filter,
                shift: b.shift,
                end_flag: b.end_flag,
                loop_flag: b.loop_flag,
                block_rms_error: b.block_rms_error,
                block_peak_error: b.block_peak_error,
                block_clamp_count: b.block_clamp_count,
            })
            .collect(),
    };

    if let Some(p) = out_report {
        write_json(p, &report)?;
    }

    eprintln!(
        "encode-brr: {} -> {} ({} blocks, {} bytes; rms={:.2}, peak={}, clamps={})",
        audio.display(),
        out_brr.display(),
        summary.total_blocks,
        encode_result.bytes.len(),
        summary.overall_rms_error,
        summary.overall_peak_error,
        summary.total_clamp_count,
    );

    Ok(())
}

fn cmd_preview_brr(
    brr: &Path,
    out_wav: &Path,
    out_report: Option<&Path>,
    sample_rate_hz: u32,
) -> Result<(), CliError> {
    let brr_bytes = std::fs::read(brr).map_err(|source| CliError::Io {
        path: brr.to_path_buf(),
        source,
    })?;

    let report_inner = match export_decoded_brr_wav(&brr_bytes, sample_rate_hz, out_wav) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("preview-brr: {e}");
            std::process::exit(2);
        }
    };

    let input_sha = sfc_atomizer_core::asm::sha256_hex(&brr_bytes);
    let wav_bytes = std::fs::read(out_wav).map_err(|source| CliError::Io {
        path: out_wav.to_path_buf(),
        source,
    })?;
    let output_sha = sfc_atomizer_core::asm::sha256_hex(&wav_bytes);

    let report = AuditionReport {
        schema_version: SCHEMA_VERSION,
        report_type: AuditionReport::REPORT_TYPE.to_string(),
        input_path: brr.display().to_string(),
        input_sha256: input_sha,
        output_path: out_wav.display().to_string(),
        output_sha256: output_sha,
        blocks_decoded: report_inner.blocks_decoded,
        samples_written: report_inner.samples_written,
        bytes_written: report_inner.bytes_written,
        sample_rate_hz,
    };
    if let Some(p) = out_report {
        write_json(p, &report)?;
    }

    eprintln!(
        "preview-brr: {} -> {} ({} blocks, {} samples, {} Hz)",
        brr.display(),
        out_wav.display(),
        report_inner.blocks_decoded,
        report_inner.samples_written,
        sample_rate_hz,
    );
    Ok(())
}

fn cmd_find_loop_candidates(
    audio: &Path,
    out_report: &Path,
    window_samples: usize,
    max_candidates: usize,
    snap_to_brr_block: bool,
) -> Result<(), CliError> {
    let metadata = match probe(audio) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "find-loop-candidates: probe failed for {}: {e}",
                audio.display()
            );
            std::process::exit(1);
        }
    };
    let pcm = match decode_to_mono_pcm(audio) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "find-loop-candidates: decode failed for {}: {e}",
                audio.display()
            );
            std::process::exit(2);
        }
    };

    let opts = LoopFinderOptions {
        window_samples,
        max_candidates,
        snap_to_brr_block,
    };
    let candidates = find_loop_candidates(&pcm, &opts);
    let source_sha = sfc_atomizer_core::asm::sha256_hex_file(audio).unwrap_or_default();

    let report = LoopFinderReport {
        schema_version: SCHEMA_VERSION,
        report_type: LoopFinderReport::REPORT_TYPE.to_string(),
        source_path: audio.display().to_string(),
        source_sha256: source_sha,
        source_frames: metadata.frames,
        window_samples: window_samples as u32,
        snap_to_brr_block,
        candidates: candidates
            .iter()
            .map(|c| LoopCandidateJson {
                start_sample: c.start_sample,
                end_sample: c.end_sample,
                rms_window_difference: c.rms_window_difference,
                seam_click: c.seam_click,
                score: c.score,
            })
            .collect(),
    };
    write_json(out_report, &report)?;

    eprintln!(
        "find-loop-candidates: {} -> {} ({} candidates)",
        audio.display(),
        out_report.display(),
        report.candidates.len(),
    );
    Ok(())
}

fn print_validate_summary(report: &ValidationReport) {
    match report.status {
        ValidationStatus::Ok => {
            eprintln!("validate-project: ok — {}", report.project_path);
        }
        ValidationStatus::Invalid => {
            eprintln!(
                "validate-project: invalid — {} ({} error{})",
                report.project_path,
                report.errors.len(),
                if report.errors.len() == 1 { "" } else { "s" }
            );
            for err in &report.errors {
                eprintln!("  {} : {}", err.path, err.message);
            }
        }
        ValidationStatus::IoError => {
            eprintln!("validate-project: io_error — {}", report.project_path);
            for err in &report.errors {
                eprintln!("  {}", err.message);
            }
        }
    }
}
