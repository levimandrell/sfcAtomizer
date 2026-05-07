//! Stable report types emitted by the SFC Wave Compiler tooling.
//!
//! Every report shares an envelope:
//!
//! ```json
//! { "schema_version": 1, "report_type": "<name>", ...specific fields }
//! ```
//!
//! The `schema_version` constant is bumped whenever a breaking field
//! change ships; the `report_type` tag lets a generic JSON consumer
//! dispatch on report kind without a wrapping discriminator.
//!
//! Address fields in [`AramRegion`] and [`AramCollision`] are
//! serialized as hex strings (e.g. `"0x0200"`) for human readability
//! when the JSON is read directly. Numeric byte counts use plain
//! integers.

use serde::{Deserialize, Serialize};

use crate::tools::ToolSource;

/// Current schema version. Bump on any breaking field change.
pub const SCHEMA_VERSION: u32 = 1;

// =============================================================================
// Doctor report — `sfcwc doctor`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub report_type: String,
    pub tools: DoctorTools,
    pub rust: RustInfo,
    pub status: DoctorStatus,
    pub diagnostics: Vec<String>,
}

impl DoctorReport {
    pub const REPORT_TYPE: &'static str = "doctor";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DoctorTools {
    pub asar: ToolStatus,
    pub snes_spc_oracle: ToolStatus,
    pub mesen2: ToolStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolStatus {
    pub resolved: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub source: ToolSource,
    /// Resolution attempts. Omitted from JSON when empty (resolved tools).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub searched: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Ok,
    Warnings,
    Errors,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RustInfo {
    pub channel: String,
    pub version: String,
}

// =============================================================================
// BRR fixture report — `sfcwc decode-fixtures`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrrFixtureReport {
    pub schema_version: u32,
    pub report_type: String,
    pub fixture_set: String,
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub results: Vec<BrrFixtureResult>,
}

impl BrrFixtureReport {
    pub const REPORT_TYPE: &'static str = "brr_fixture";

    /// Empty stub used by M0.1 before the actual fixture corpus exists.
    pub fn stub() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            fixture_set: "m0_raw_decode".to_string(),
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            results: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrrFixtureResult {
    pub name: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}

// =============================================================================
// ARAM map report — emitted alongside `m0-acceptance`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AramMapReport {
    pub schema_version: u32,
    pub report_type: String,
    pub total_aram: u32,
    pub regions: Vec<AramRegion>,
    pub free_bytes: u32,
    pub collisions: Vec<AramCollision>,
    /// Master-echo summary. `None` when the producer is the M0
    /// byte-scanning [`crate::aram::map_from_image`] (no project
    /// context to derive echo state from). Populated by the M1+
    /// packer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echo: Option<AramEchoSummary>,
    /// Source-directory summary; populated by the M1+ packer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_directory: Option<AramSourceDirSummary>,
    /// Per-sample BRR-pool layout; populated by the M1+ packer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<AramSamplesSummary>,
    /// Soft warnings — informational, never block the pack. Examples:
    /// "FREE_LESS_THAN_256_BYTES", "ECHO_NEAR_TOP_OF_ARAM_REVIEW_IPL_BIT".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl AramMapReport {
    pub const REPORT_TYPE: &'static str = "aram_map";
    pub const TOTAL_ARAM: u32 = 65536;

    /// Stub map containing only the SPEC §15.1 fixed regions and a free
    /// remainder. M0.4+ replaces this with a real packer trace.
    pub fn stub() -> Self {
        let regions = vec![
            AramRegion {
                name: "direct_page".to_string(),
                start: "0x0000".to_string(),
                end: "0x00EF".to_string(),
                bytes: 240,
                kind: AramKind::FixedRuntime,
            },
            AramRegion {
                name: "hardware_io".to_string(),
                start: "0x00F0".to_string(),
                end: "0x00FF".to_string(),
                bytes: 16,
                kind: AramKind::FixedHardware,
            },
            AramRegion {
                name: "stack".to_string(),
                start: "0x0100".to_string(),
                end: "0x01FF".to_string(),
                bytes: 256,
                kind: AramKind::FixedRuntime,
            },
        ];
        let used: u32 = regions.iter().map(|r| r.bytes).sum();
        Self {
            schema_version: SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            total_aram: Self::TOTAL_ARAM,
            regions,
            free_bytes: Self::TOTAL_ARAM - used,
            collisions: Vec::new(),
            echo: None,
            source_directory: None,
            samples: None,
            warnings: Vec::new(),
        }
    }
}

/// Echo-buffer summary for the ARAM meter (M1+).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AramEchoSummary {
    pub enabled: bool,
    pub edl: u8,
    /// `EDL * 2048` bytes when enabled; 0 otherwise.
    pub buffer_bytes: u32,
    /// SPEC §15.3 caveat — even with `enabled=false`, the S-DSP still
    /// performs a 4-byte echo write at `ESA*0x100` unless FLG's
    /// echo-write-disable bit is set. The driver handles the FLG bit;
    /// this field is the reminder.
    pub hardware_tail_bytes: u32,
    /// `ESA = echo_start >> 8`. Zero when echo is disabled.
    pub esa: u8,
    pub percent_of_aram: f64,
    /// `false` when an `enabled=true / edl=0` configuration would
    /// corrupt `[ESA*0x100, ESA*0x100 + 4)` (SPEC §15.3 trap). Pack
    /// validation rejects this so a successful pack never sees
    /// `writeback_safe=false`, but the field is here for the meter
    /// to surface the hazard explicitly.
    pub writeback_safe: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AramSourceDirSummary {
    pub source_count: u32,
    /// `source_count * 4` bytes of actual entries.
    pub bytes: u32,
    /// Padding from the end of the directory entries up to the next
    /// page boundary. The BRR pool starts at the first byte after this
    /// padding.
    pub padding_bytes: u32,
    /// Page-aligned start address of the directory (= S-DSP `DIR`
    /// register, scaled by 0x100).
    pub start_addr: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AramSamplesSummary {
    pub total_samples: u32,
    pub total_brr_bytes: u32,
    pub per_sample: Vec<PerSampleAramEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerSampleAramEntry {
    pub sample_id: String,
    /// ARAM start address of this sample's BRR data (matches the
    /// `start_addr` field in the source-directory entry).
    pub start_addr: u16,
    /// ARAM loop-entry address. `Some(start_addr + loop_block * 9)`
    /// when looped; `Some(start_addr)` for non-looped (S-DSP convention
    /// — the END flag handles termination).
    pub loop_addr: Option<u16>,
    pub bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AramRegion {
    pub name: String,
    pub start: String,
    pub end: String,
    pub bytes: u32,
    pub kind: AramKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AramKind {
    FixedRuntime,
    FixedHardware,
    DriverCode,
    SourceDirectory,
    PitchTables,
    SequenceData,
    InstrumentMetadata,
    SampleBrrPool,
    SynthAtomPool,
    EchoBuffer,
    Free,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AramCollision {
    pub a: String,
    pub b: String,
    pub overlap_start: String,
    pub overlap_end: String,
}

// =============================================================================
// Assemble report — `sfcwc assemble-smoke`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssembleReport {
    pub schema_version: u32,
    pub report_type: String,
    pub backend: String,
    pub backend_version: String,
    pub input_path: Option<String>,
    pub input_sha256: Option<String>,
    pub output_path: Option<String>,
    pub output_bytes: u64,
    pub exit_code: Option<i32>,
    pub stdout_lines: u32,
    pub stderr_lines: u32,
    pub status: AssembleStatus,
    /// Hex-encoded SHA-256 of the assembled 64 KB ARAM image. Added
    /// in M0.3 alongside the asar wiring; older consumers that
    /// don't know the field still parse the report (omitted from
    /// JSON when `None`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_image_sha256: Option<String>,
    /// Human-readable error message when `status == Error`. Added
    /// in M0.3 so failure-as-data carries a diagnostic without
    /// requiring a non-zero process exit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AssembleReport {
    pub const REPORT_TYPE: &'static str = "assemble";

    pub fn stub() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            backend: "asar".to_string(),
            backend_version: "unknown".to_string(),
            input_path: None,
            input_sha256: None,
            output_path: None,
            output_bytes: 0,
            exit_code: None,
            stdout_lines: 0,
            stderr_lines: 0,
            status: AssembleStatus::NotRun,
            output_image_sha256: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssembleStatus {
    NotRun,
    Ok,
    Error,
}

// =============================================================================
// SPC export report — `sfcwc export-spc-smoke`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpcExportReport {
    pub schema_version: u32,
    pub report_type: String,
    pub output_path: Option<String>,
    pub file_size_bytes: u64,
    pub aram_image_sha256: Option<String>,
    pub initial_state: SpcInitialState,
    pub verified_structure: bool,
    pub status: SpcStatus,
    /// Hex SHA-256 of the input ARAM image (64 KB, what
    /// `assemble-smoke` produced). Added in M0.4; non-breaking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_aram_sha256: Option<String>,
    /// Hex SHA-256 of the produced SPC's DSP register block (128 B
    /// at file offset 0x10100). Added in M0.4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dsp_state_sha256: Option<String>,
    /// Hex SHA-256 of the full SPC file produced. Added in M0.4 so
    /// downstream consumers can detect drift across runs without
    /// diffing 66 KB of bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spc_file_sha256: Option<String>,
    /// Human-readable error string when `status == Error`. Added
    /// in M0.4, same shape as `AssembleReport.error` from M0.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SpcExportReport {
    pub const REPORT_TYPE: &'static str = "spc_export";

    pub fn stub() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            output_path: None,
            file_size_bytes: 0,
            aram_image_sha256: None,
            initial_state: SpcInitialState::default(),
            verified_structure: false,
            status: SpcStatus::NotRun,
            input_aram_sha256: None,
            dsp_state_sha256: None,
            spc_file_sha256: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpcInitialState {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub psw: u8,
    pub sp: u8,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpcStatus {
    NotRun,
    Ok,
    Error,
}

// =============================================================================
// Calibration report — `sfcwc calibrate-oracle`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationReport {
    pub schema_version: u32,
    pub report_type: String,
    pub status: CalibrationStatus,
    pub oracle: Option<OracleInfo>,
    pub fixture_set: Option<FixtureSetInfo>,
    pub render: Option<RenderInfo>,
    pub observed: Option<ObservedInfo>,
    pub provisional_tolerances: Option<ProvisionalTolerances>,
    pub ci_gate: bool,
    pub freeze_target: String,
    /// Soft warnings that don't change `status` but flag unexpected
    /// observations (e.g. non-zero PCM from a muted M0 smoke).
    /// Added in M0.5.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    /// Human-readable error string when `status == Error`. Added in
    /// M0.5; same shape as `AssembleReport.error` and
    /// `SpcExportReport.error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Hex SHA-256 of the oracle-rendered PCM. Added in M0.6 so the
    /// `M0Manifest.bundle` can pick it up from one place rather than
    /// reading the wrapper's sidecar JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_pcm_sha256: Option<String>,
}

impl CalibrationReport {
    pub const REPORT_TYPE: &'static str = "calibration";

    pub fn stub() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            status: CalibrationStatus::NotRun,
            oracle: None,
            fixture_set: None,
            render: None,
            observed: None,
            provisional_tolerances: None,
            ci_gate: false,
            freeze_target: "M1".to_string(),
            diagnostics: Vec::new(),
            error: None,
            oracle_pcm_sha256: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationStatus {
    NotRun,
    ProvisionalNotCiGate,
    Frozen,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OracleInfo {
    pub backend: String,
    pub version: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderInfo {
    pub sample_rate_hz: u32,
    pub frames: u32,
    pub channels: u8,
}

/// Identifies which fixture corpus drove this calibration run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FixtureSetInfo {
    pub name: String,
    /// Hex SHA-256 of the input fixture (e.g. the `.spc` file).
    pub sha256: String,
}

/// Numeric observations from one calibration render. Voice render is
/// the per-voice S-DSP path; full-module render comes later. Values
/// are computed from the oracle output PCM by the host (verified in
/// Rust, not trusted from the wrapper).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservedInfo {
    pub voice_render_max_abs_lsb: i32,
    pub voice_render_rms_lsb: f64,
}

/// Hardcoded provisional tolerances for M0.5. These are not yet CI
/// gates (`ci_gate: false`); M1 freezes them per SPEC §10.1, §21.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvisionalTolerances {
    pub voice_render_max_abs_lsb: i32,
    pub voice_render_rms_lsb: f64,
}

// =============================================================================
// Validation report — `sfcwc validate-project`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub report_type: String,
    pub project_path: String,
    pub status: ValidationStatus,
    pub errors: Vec<ValidationErrorJson>,
}

impl ValidationReport {
    pub const REPORT_TYPE: &'static str = "validation";

    pub fn stub() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            project_path: String::new(),
            status: ValidationStatus::Ok,
            errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Ok,
    Invalid,
    IoError,
}

/// One validation error in serializable form. Mirror of
/// `core::project::ValidationError` reduced to a flat `{path, message}`
/// shape so JSON consumers don't have to know the typed error enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationErrorJson {
    pub path: String,
    pub message: String,
}

// =============================================================================
// M0 manifest — `sfcwc m0-acceptance`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct M0Manifest {
    pub schema_version: u32,
    pub report_type: String,
    /// RFC3339 timestamp; populated by `m0-acceptance` from M0.6 onward.
    /// Pre-M0.6 manifests carry `null` here.
    pub generated_at: Option<String>,
    pub doctor_report: String,
    pub brr_fixture_report: String,
    pub aram_map_report: String,
    pub assemble_report: String,
    pub spc_export_report: String,
    pub calibration_report: String,
    /// Bundle-level summary added in M0.6. `#[serde(default)]` so
    /// pre-M0.6 manifests still parse, deserializing to a sentinel
    /// `BundleSummary` whose status is `Error` (forces re-run).
    #[serde(default)]
    pub bundle: BundleSummary,
}

impl M0Manifest {
    pub const REPORT_TYPE: &'static str = "m0_manifest";
}

// =============================================================================
// Bundle summary (M0.6) — aggregate per-step status + cross-references.
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BundleSummary {
    pub steps: BundleSteps,
    pub status: BundleStatus,
    /// SHA-256 of the assembled 64 KB ARAM image (from
    /// `assemble.output_image_sha256`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aram_image_sha256: Option<String>,
    /// SHA-256 of the produced `.spc` file (from
    /// `spc_export.spc_file_sha256`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spc_file_sha256: Option<String>,
    /// SHA-256 of the oracle-rendered PCM (from
    /// `calibration.oracle_pcm_sha256`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_pcm_sha256: Option<String>,
    /// Bundle-level diagnostics — each step's diagnostics flattened
    /// and prefixed with the step name. Capped at 50 entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BundleSteps {
    pub doctor: StepStatus,
    pub decode_fixtures: StepStatus,
    pub assemble: StepStatus,
    pub spc_export: StepStatus,
    pub aram_map: StepStatus,
    pub calibration: StepStatus,
}

/// Per-step rollup. The mapping rules are documented in
/// `core::manifest` and exercised end-to-end by `m0-acceptance`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Ok,
    Warnings,
    Error,
    /// Step was not run because a prerequisite was missing
    /// (e.g. asar not resolved → assemble skipped).
    #[default]
    Skipped,
}

/// Aggregate bundle status. Default is `Error` so a freshly-deserialized
/// bundle-less manifest forces re-acceptance rather than silently
/// passing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BundleStatus {
    Ok,
    Degraded,
    #[default]
    Error,
}

// =============================================================================
// BRR encode report — `sfcwc encode-brr`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrrEncodeReport {
    pub schema_version: u32,
    pub report_type: String,
    pub source_path: String,
    pub source_sha256: String,
    pub source_frames: u64,
    pub source_sample_rate_hz: u32,
    pub output_path: String,
    pub output_sha256: String,
    pub output_bytes: u64,
    pub total_blocks: u32,
    pub overall_rms_error: f64,
    pub overall_peak_error: u32,
    pub total_clamp_count: u32,
    pub filter_distribution: [u32; 4],
    pub force_filter_0_first_block: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_start_sample: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_entry_block_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_click_score: Option<f64>,
    pub blocks: Vec<BrrEncodeBlock>,
}

impl BrrEncodeReport {
    pub const REPORT_TYPE: &'static str = "brr_encode";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BrrEncodeBlock {
    pub index: u32,
    pub filter: u8,
    pub shift: u8,
    pub end_flag: bool,
    pub loop_flag: bool,
    pub block_rms_error: f64,
    pub block_peak_error: u32,
    pub block_clamp_count: u32,
}

// =============================================================================
// Atom render report — `sfcwc render-atom` (M2.2)
// =============================================================================

/// Output of `sfcwc render-atom <atom_id>` — describes an atom's
/// rendered single-cycle PCM and its M1.3-encoded BRR payload. The
/// PCM and BRR SHAs serve as M2 producer-side baselines analogous
/// to M1's `M1_DRIVER_CODE_SHA256` / `M1_ARAM_IMAGE_SHA256` /
/// `M1_SPC_FILE_SHA256` block.
///
/// The embedded `EncodeSummary` reuses the M1.3 BRR encoder type
/// (no parallel mirror), so any future encoder-side change
/// surfaces directly through the report's serialised shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtomRenderReport {
    pub schema_version: u32,
    pub report_type: String,
    pub atom_id: String,
    pub atom_name: String,
    /// e.g. `"additive_single_cycle_v0"`.
    pub atom_kind: String,
    pub cycle_len_samples: u32,
    pub partial_count: u32,
    pub normalize: bool,
    pub atom_amplitude: f64,
    pub root_midi_note: u8,
    pub pcm_sha256: String,
    pub brr_sha256: String,
    pub brr_bytes: u32,
    pub encode_summary: crate::brr_encoder::EncodeSummary,
}

impl AtomRenderReport {
    pub const REPORT_TYPE: &'static str = "atom_render";
}

// =============================================================================
// Loop finder report — `sfcwc find-loop-candidates`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopFinderReport {
    pub schema_version: u32,
    pub report_type: String,
    pub source_path: String,
    pub source_sha256: String,
    pub source_frames: u64,
    pub window_samples: u32,
    pub snap_to_brr_block: bool,
    pub candidates: Vec<LoopCandidateJson>,
}

impl LoopFinderReport {
    pub const REPORT_TYPE: &'static str = "loop_finder";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LoopCandidateJson {
    pub start_sample: u32,
    pub end_sample: u32,
    pub rms_window_difference: f64,
    pub seam_click: u32,
    pub score: f64,
}

// =============================================================================
// Audition report — `sfcwc preview-brr`
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditionReport {
    pub schema_version: u32,
    pub report_type: String,
    pub input_path: String,
    pub input_sha256: String,
    pub output_path: String,
    pub output_sha256: String,
    pub blocks_decoded: u32,
    pub samples_written: u32,
    pub bytes_written: u64,
    pub sample_rate_hz: u32,
}

impl AuditionReport {
    pub const REPORT_TYPE: &'static str = "audition";
}

// =============================================================================
// Compile-SPC report — `sfcwc compile-spc` (M1.5)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompileSpcReport {
    pub schema_version: u32,
    pub report_type: String,
    pub project_name: String,
    pub active_sample_id: String,
    pub aram_image_sha256: String,
    pub spc_file_sha256: String,
    pub driver_code_sha256: String,
    pub driver_code_bytes: u32,
    pub map_report_path: String,
    pub spc_path: String,
    pub aram_image_path: String,
}

impl CompileSpcReport {
    pub const REPORT_TYPE: &'static str = "compile_spc";
}

// =============================================================================
// Audible-verification report — `sfcwc verify-spc-audible` (M1.5)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudibleVerificationReport {
    pub schema_version: u32,
    pub report_type: String,
    pub spc_path: String,
    pub spc_sha256: String,
    pub frames_rendered: u32,
    pub sample_rate_hz: u32,
    pub observed: ObservedAudio,
    pub thresholds: AudibleThresholds,
    pub status: AudibleStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl AudibleVerificationReport {
    pub const REPORT_TYPE: &'static str = "audible_verification";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ObservedAudio {
    pub max_abs: u32,
    pub rms: f64,
    pub bytes_zero: u32,
    pub bytes_total: u32,
    pub fraction_zero: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AudibleThresholds {
    pub min_max_abs: u32,
    pub min_rms: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudibleStatus {
    Ok,
    SilentFail,
    OracleError,
}

// =============================================================================
// M1 manifest — `sfcwc m1-acceptance` (M1.7)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct M1Manifest {
    pub schema_version: u32,
    pub report_type: String,
    pub generated_at: String,
    pub project_a: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_b: Option<String>,

    pub doctor_report: String,
    pub validate_a_report: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate_b_report: Option<String>,
    pub aram_map_report: String,
    pub compile_spc_report: String,
    pub audible_spc_report: String,
    pub compile_sfc_report: String,
    pub structure_sfc_report: String,
    pub audible_sfc_report: String,

    pub bundle: M1BundleSummary,
}

impl M1Manifest {
    pub const REPORT_TYPE: &'static str = "m1_manifest";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct M1BundleSummary {
    pub steps: M1BundleSteps,
    pub status: BundleStatus,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aram_image_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spc_file_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sfc_file_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_a_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_b_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_code_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spc_audible_max_abs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sfc_audible_module_a_max_abs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sfc_audible_module_b_max_abs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modules_audio_identical: Option<bool>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct M1BundleSteps {
    pub doctor: StepStatus,
    pub validate_a: StepStatus,
    /// `Skipped` when no project B was provided. Optional step;
    /// does not downgrade the bundle status when skipped this way.
    pub validate_b: StepStatus,
    pub compile_spc: StepStatus,
    pub audible_spc: StepStatus,
    pub compile_sfc: StepStatus,
    pub structure_sfc: StepStatus,
    pub audible_sfc: StepStatus,
}

// =============================================================================
// Compile-SFC report — `sfcwc compile-sfc` (M1.6)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompileSfcReport {
    pub schema_version: u32,
    pub report_type: String,
    pub project_a_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_b_name: Option<String>,
    pub sfc_path: String,
    pub sfc_size_bytes: u32,
    pub sfc_sha256: String,
    /// Module B is a clone of module A when only project A was given.
    pub module_b_is_clone_of_a: bool,
    pub module_a_sha256: String,
    pub module_a_in_file_sha256: String,
    pub module_a_bytes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_b_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_b_in_file_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_b_bytes: Option<u32>,
    pub loader_size_bytes: u32,
}

impl CompileSfcReport {
    pub const REPORT_TYPE: &'static str = "compile_sfc";
}

// =============================================================================
// SFC structure report — `sfcwc verify-sfc-structure` (M1.6)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SfcStructureReport {
    pub schema_version: u32,
    pub report_type: String,
    pub sfc_path: String,
    pub status: SfcStructureStatus,
    pub findings: Vec<SfcFinding>,
    pub header_summary: SfcHeaderSummary,
    pub module_a_summary: SfcModuleSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_b_summary: Option<SfcModuleSummary>,
}

impl SfcStructureReport {
    pub const REPORT_TYPE: &'static str = "sfc_structure";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SfcStructureStatus {
    Ok,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SfcFinding {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SfcHeaderSummary {
    pub title: String,
    pub mode_byte: u8,
    pub rom_size_byte: u8,
    pub country_byte: u8,
    pub checksum: u16,
    pub checksum_complement: u16,
    pub reset_vector: u16,
    pub file_size_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SfcModuleSummary {
    pub embed_offset: u32,
    pub magic_ok: bool,
    pub schema_version: u16,
    pub block_count: u16,
    pub entrypoint: u16,
    pub total_file_len: u32,
    pub flags: u16,
    pub in_file_sha256: String,
    pub recomputed_in_file_sha256: String,
    pub in_file_sha_matches: bool,
}

// =============================================================================
// SFC modules audible report — `sfcwc verify-sfc-modules-audible` (M1.6)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SfcModulesAudibleReport {
    pub schema_version: u32,
    pub report_type: String,
    pub sfc_path: String,
    pub status: AudibleStatus,
    pub module_a_audible: AudibleVerificationReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_b_audible: Option<AudibleVerificationReport>,
    /// `true` when both rendered PCMs hash to the same SHA-256
    /// (M1.6 single-project clone case). Useful for the user to
    /// confirm "swap to identical module produced identical
    /// audio".
    pub modules_audio_identical: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SfcModulesAudibleReport {
    pub const REPORT_TYPE: &'static str = "sfc_modules_audible";
}

// =============================================================================
// Tests — round-trip every report through serde to catch field renames.
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(v: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(v).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, &back, "round-trip mismatch");
    }

    #[test]
    fn doctor_round_trip() {
        let r = DoctorReport {
            schema_version: SCHEMA_VERSION,
            report_type: DoctorReport::REPORT_TYPE.to_string(),
            tools: DoctorTools {
                asar: ToolStatus {
                    resolved: true,
                    path: Some("C:\\tools\\asar.exe".to_string()),
                    version: Some("1.91".to_string()),
                    source: ToolSource::Env,
                    searched: Vec::new(),
                },
                snes_spc_oracle: ToolStatus {
                    resolved: false,
                    path: None,
                    version: None,
                    source: ToolSource::Missing,
                    searched: vec![
                        "env:SFCWC_SNES_SPC_ORACLE".to_string(),
                        "default:tools/snes_spc_oracle".to_string(),
                    ],
                },
                mesen2: ToolStatus {
                    resolved: false,
                    path: None,
                    version: None,
                    source: ToolSource::Missing,
                    searched: vec!["env:SFCWC_MESEN2".to_string()],
                },
            },
            rust: RustInfo {
                channel: "stable".to_string(),
                version: "1.78.0".to_string(),
            },
            status: DoctorStatus::Warnings,
            diagnostics: vec!["snes_spc oracle wrapper not found".to_string()],
        };
        round_trip(&r);
    }

    #[test]
    fn brr_fixture_round_trip() {
        round_trip(&BrrFixtureReport::stub());
        let mut r = BrrFixtureReport::stub();
        r.total = 2;
        r.passed = 1;
        r.failed = 1;
        r.results = vec![
            BrrFixtureResult {
                name: "block_filter0".to_string(),
                passed: true,
                failure: None,
            },
            BrrFixtureResult {
                name: "block_filter3_clamp".to_string(),
                passed: false,
                failure: Some("sample 7: expected -32768 got -32767".to_string()),
            },
        ];
        round_trip(&r);
    }

    #[test]
    fn aram_map_round_trip() {
        round_trip(&AramMapReport::stub());
    }

    #[test]
    fn assemble_round_trip() {
        // Stub: both new optional fields absent.
        round_trip(&AssembleReport::stub());

        // Both new optional fields populated (success path).
        let mut r = AssembleReport::stub();
        r.status = AssembleStatus::Ok;
        r.backend_version = "Asar 1.91, ...".to_string();
        r.input_path = Some("core/fixtures/asm/m0_smoke.asm".to_string());
        r.output_path = Some("build/m0/driver.bin".to_string());
        r.output_bytes = 65536;
        r.exit_code = Some(0);
        r.output_image_sha256 = Some("abc123".repeat(10) + "abcd"); // 64 hex chars
        round_trip(&r);

        // error field populated (failure path).
        let mut r = AssembleReport::stub();
        r.status = AssembleStatus::Error;
        r.error = Some("assembler not resolved: set SFCWC_ASAR".to_string());
        round_trip(&r);
    }

    #[test]
    fn assemble_stub_omits_new_optional_fields_in_json() {
        let json = serde_json::to_string(&AssembleReport::stub()).unwrap();
        assert!(
            !json.contains("output_image_sha256"),
            "stub should omit unset sha: {json}"
        );
        assert!(
            !json.contains("\"error\""),
            "stub should omit unset error: {json}"
        );
    }

    #[test]
    fn assemble_report_v1_without_new_fields_still_parses() {
        // Older consumer wrote a report without the M0.3 fields.
        let pre_m03 = r#"{
            "schema_version": 1,
            "report_type": "assemble",
            "backend": "asar",
            "backend_version": "unknown",
            "input_path": null,
            "input_sha256": null,
            "output_path": null,
            "output_bytes": 0,
            "exit_code": null,
            "stdout_lines": 0,
            "stderr_lines": 0,
            "status": "not_run"
        }"#;
        let r: AssembleReport = serde_json::from_str(pre_m03).unwrap();
        assert_eq!(r.output_image_sha256, None);
        assert_eq!(r.error, None);
        assert_eq!(r.status, AssembleStatus::NotRun);
    }

    #[test]
    fn spc_export_round_trip() {
        // Stub: every M0.4 optional field absent.
        round_trip(&SpcExportReport::stub());

        // Success path with all M0.4 fields populated.
        let mut r = SpcExportReport::stub();
        r.status = SpcStatus::Ok;
        r.output_path = Some("build/m0/smoke.spc".to_string());
        r.file_size_bytes = 66048;
        r.aram_image_sha256 = Some("a".repeat(64));
        r.initial_state = SpcInitialState {
            pc: 0x0200,
            a: 0,
            x: 0,
            y: 0,
            psw: 0,
            sp: 0xEF,
        };
        r.verified_structure = true;
        r.input_aram_sha256 = Some("b".repeat(64));
        r.dsp_state_sha256 = Some("c".repeat(64));
        r.spc_file_sha256 = Some("d".repeat(64));
        round_trip(&r);

        // Error path: only error populated.
        let mut r = SpcExportReport::stub();
        r.status = SpcStatus::Error;
        r.error = Some("aram input missing at build/m0/driver.bin".to_string());
        round_trip(&r);
    }

    #[test]
    fn spc_export_v1_without_new_fields_still_parses() {
        let pre_m04 = r#"{
            "schema_version": 1,
            "report_type": "spc_export",
            "output_path": null,
            "file_size_bytes": 0,
            "aram_image_sha256": null,
            "initial_state": { "pc": 0, "a": 0, "x": 0, "y": 0, "psw": 0, "sp": 0 },
            "verified_structure": false,
            "status": "not_run"
        }"#;
        let r: SpcExportReport = serde_json::from_str(pre_m04).unwrap();
        assert_eq!(r.input_aram_sha256, None);
        assert_eq!(r.dsp_state_sha256, None);
        assert_eq!(r.spc_file_sha256, None);
        assert_eq!(r.error, None);
        assert_eq!(r.status, SpcStatus::NotRun);
    }

    #[test]
    fn calibration_round_trip() {
        // Stub: every M0.5 optional field absent.
        round_trip(&CalibrationReport::stub());

        // Provisional path with all M0.5 fields populated.
        let mut r = CalibrationReport::stub();
        r.status = CalibrationStatus::ProvisionalNotCiGate;
        r.oracle = Some(OracleInfo {
            backend: "snes_spc_wrapper".to_string(),
            version: "snes_spc_oracle 0.1.0 (snes_spc abc...)".to_string(),
            path: "/abs/path/to/snes_spc_oracle".to_string(),
        });
        r.fixture_set = Some(FixtureSetInfo {
            name: "m0_smoke".to_string(),
            sha256: "0".repeat(64),
        });
        r.render = Some(RenderInfo {
            sample_rate_hz: 32000,
            frames: 2048,
            channels: 2,
        });
        r.observed = Some(ObservedInfo {
            voice_render_max_abs_lsb: 0,
            voice_render_rms_lsb: 0.0,
        });
        r.provisional_tolerances = Some(ProvisionalTolerances {
            voice_render_max_abs_lsb: 1,
            voice_render_rms_lsb: 0.25,
        });
        round_trip(&r);

        // Diagnostics + error populated.
        let mut r = CalibrationReport::stub();
        r.status = CalibrationStatus::Error;
        r.error = Some("oracle wrapper not resolved".to_string());
        r.diagnostics = vec!["non-zero PCM from muted smoke".to_string()];
        round_trip(&r);
    }

    #[test]
    fn calibration_v1_without_new_fields_still_parses() {
        // Pre-M0.5 stub JSON. Inner structs were placeholders; the
        // outer envelope is what matters for non-breaking parsing.
        let pre_m05 = r#"{
            "schema_version": 1,
            "report_type": "calibration",
            "status": "not_run",
            "oracle": null,
            "fixture_set": null,
            "render": null,
            "observed": null,
            "provisional_tolerances": null,
            "ci_gate": false,
            "freeze_target": "M1"
        }"#;
        let r: CalibrationReport = serde_json::from_str(pre_m05).unwrap();
        assert!(r.diagnostics.is_empty());
        assert_eq!(r.error, None);
        assert_eq!(r.status, CalibrationStatus::NotRun);
    }

    #[test]
    fn validation_report_round_trip() {
        round_trip(&ValidationReport::stub());

        let r = ValidationReport {
            schema_version: SCHEMA_VERSION,
            report_type: ValidationReport::REPORT_TYPE.to_string(),
            project_path: "build/m1/project.sfcproj.json".to_string(),
            status: ValidationStatus::Invalid,
            errors: vec![
                ValidationErrorJson {
                    path: "/master_echo/edl".to_string(),
                    message: "master_echo.enabled=true requires edl in 1..=15, got 0".to_string(),
                },
                ValidationErrorJson {
                    path: "/m1/active_sample_id".to_string(),
                    message: "m1.active_sample_id \"\" not found in sample_pool".to_string(),
                },
            ],
        };
        round_trip(&r);
    }

    #[test]
    fn validation_status_round_trip() {
        for s in [
            ValidationStatus::Ok,
            ValidationStatus::Invalid,
            ValidationStatus::IoError,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: ValidationStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn m0_manifest_round_trip() {
        // Bundle defaults (BundleStatus::Error, all StepStatus::Skipped).
        let m = M0Manifest {
            schema_version: SCHEMA_VERSION,
            report_type: M0Manifest::REPORT_TYPE.to_string(),
            generated_at: Some("2026-05-05T20:00:00Z".to_string()),
            doctor_report: "build/m0/doctor.json".to_string(),
            brr_fixture_report: "build/m0/brr-fixture-report.json".to_string(),
            aram_map_report: "build/m0/aram-map.json".to_string(),
            assemble_report: "build/m0/assemble-report.json".to_string(),
            spc_export_report: "build/m0/spc-export-report.json".to_string(),
            calibration_report: "build/m0/calibration-report.json".to_string(),
            bundle: BundleSummary::default(),
        };
        round_trip(&m);

        // Fully populated bundle (M0 acceptance happy path).
        let mut m = m.clone();
        m.bundle = BundleSummary {
            steps: BundleSteps {
                doctor: StepStatus::Ok,
                decode_fixtures: StepStatus::Ok,
                assemble: StepStatus::Ok,
                spc_export: StepStatus::Ok,
                aram_map: StepStatus::Ok,
                calibration: StepStatus::Ok,
            },
            status: BundleStatus::Ok,
            aram_image_sha256: Some("a".repeat(64)),
            spc_file_sha256: Some("b".repeat(64)),
            oracle_pcm_sha256: Some("c".repeat(64)),
            diagnostics: vec!["doctor: example diagnostic".to_string()],
        };
        round_trip(&m);
    }

    #[test]
    fn m0_manifest_pre_m06_without_bundle_still_parses() {
        // M0.4/M0.5 manifest shape — no `bundle` field.
        let pre_m06 = r#"{
            "schema_version": 1,
            "report_type": "m0_manifest",
            "generated_at": null,
            "doctor_report": "build/m0/doctor.json",
            "brr_fixture_report": "build/m0/brr-fixture-report.json",
            "aram_map_report": "build/m0/aram-map.json",
            "assemble_report": "build/m0/assemble-report.json",
            "spc_export_report": "build/m0/spc-export-report.json",
            "calibration_report": "build/m0/calibration-report.json"
        }"#;
        let m: M0Manifest = serde_json::from_str(pre_m06).unwrap();
        assert_eq!(m.bundle.status, BundleStatus::Error);
        assert_eq!(m.bundle.steps.doctor, StepStatus::Skipped);
        assert!(m.bundle.diagnostics.is_empty());
        assert!(m.bundle.aram_image_sha256.is_none());
    }

    #[test]
    fn step_status_round_trip() {
        for s in [
            StepStatus::Ok,
            StepStatus::Warnings,
            StepStatus::Error,
            StepStatus::Skipped,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: StepStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn bundle_status_round_trip() {
        for s in [
            BundleStatus::Ok,
            BundleStatus::Degraded,
            BundleStatus::Error,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let back: BundleStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn tool_status_omits_searched_when_empty() {
        let t = ToolStatus {
            resolved: true,
            path: Some("/tmp/asar".to_string()),
            version: Some("1.91".to_string()),
            source: ToolSource::Path,
            searched: Vec::new(),
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(
            !json.contains("searched"),
            "empty searched should be omitted: {json}"
        );
    }

    #[test]
    fn m1_manifest_round_trip_minimal() {
        let m = M1Manifest {
            schema_version: SCHEMA_VERSION,
            report_type: M1Manifest::REPORT_TYPE.to_string(),
            generated_at: "2026-05-06T00:00:00Z".to_string(),
            project_a: "demo.sfcproj.json".to_string(),
            project_b: None,
            doctor_report: "build/m1/doctor.json".to_string(),
            validate_a_report: "build/m1/validate-a.json".to_string(),
            validate_b_report: None,
            aram_map_report: "build/m1/aram-map.json".to_string(),
            compile_spc_report: "build/m1/compile-spc.json".to_string(),
            audible_spc_report: "build/m1/audible-spc.json".to_string(),
            compile_sfc_report: "build/m1/compile-sfc.json".to_string(),
            structure_sfc_report: "build/m1/structure-sfc.json".to_string(),
            audible_sfc_report: "build/m1/audible-sfc.json".to_string(),
            bundle: M1BundleSummary::default(),
        };
        round_trip(&m);
    }

    #[test]
    fn m1_manifest_round_trip_populated() {
        let bundle = M1BundleSummary {
            steps: M1BundleSteps {
                doctor: StepStatus::Ok,
                validate_a: StepStatus::Ok,
                validate_b: StepStatus::Ok,
                compile_spc: StepStatus::Ok,
                audible_spc: StepStatus::Ok,
                compile_sfc: StepStatus::Ok,
                structure_sfc: StepStatus::Ok,
                audible_sfc: StepStatus::Ok,
            },
            status: BundleStatus::Ok,
            aram_image_sha256: Some("a".repeat(64)),
            spc_file_sha256: Some("b".repeat(64)),
            sfc_file_sha256: Some("c".repeat(64)),
            module_a_sha256: Some("d".repeat(64)),
            module_b_sha256: Some("e".repeat(64)),
            driver_code_sha256: Some("f".repeat(64)),
            spc_audible_max_abs: Some(11072),
            sfc_audible_module_a_max_abs: Some(11072),
            sfc_audible_module_b_max_abs: Some(11072),
            modules_audio_identical: Some(false),
            diagnostics: vec!["doctor: mesen2 missing (informational)".to_string()],
        };
        let m = M1Manifest {
            schema_version: SCHEMA_VERSION,
            report_type: M1Manifest::REPORT_TYPE.to_string(),
            generated_at: "2026-05-06T00:00:00Z".to_string(),
            project_a: "a.sfcproj.json".to_string(),
            project_b: Some("b.sfcproj.json".to_string()),
            doctor_report: "build/m1/doctor.json".to_string(),
            validate_a_report: "build/m1/validate-a.json".to_string(),
            validate_b_report: Some("build/m1/validate-b.json".to_string()),
            aram_map_report: "build/m1/aram-map.json".to_string(),
            compile_spc_report: "build/m1/compile-spc.json".to_string(),
            audible_spc_report: "build/m1/audible-spc.json".to_string(),
            compile_sfc_report: "build/m1/compile-sfc.json".to_string(),
            structure_sfc_report: "build/m1/structure-sfc.json".to_string(),
            audible_sfc_report: "build/m1/audible-sfc.json".to_string(),
            bundle,
        };
        round_trip(&m);
    }

    #[test]
    fn compile_sfc_report_round_trip() {
        let r = CompileSfcReport {
            schema_version: SCHEMA_VERSION,
            report_type: CompileSfcReport::REPORT_TYPE.to_string(),
            project_a_name: "demo".to_string(),
            project_b_name: None,
            sfc_path: "build/m1/demo.sfc".to_string(),
            sfc_size_bytes: 262144,
            sfc_sha256: "0".repeat(64),
            module_b_is_clone_of_a: true,
            module_a_sha256: "1".repeat(64),
            module_a_in_file_sha256: "2".repeat(64),
            module_a_bytes: 9048,
            module_b_sha256: None,
            module_b_in_file_sha256: None,
            module_b_bytes: None,
            loader_size_bytes: 581,
        };
        round_trip(&r);
        let mut r2 = r.clone();
        r2.project_b_name = Some("swap".to_string());
        r2.module_b_is_clone_of_a = false;
        r2.module_b_sha256 = Some("3".repeat(64));
        r2.module_b_in_file_sha256 = Some("4".repeat(64));
        r2.module_b_bytes = Some(9048);
        round_trip(&r2);
    }

    #[test]
    fn sfc_structure_report_round_trip() {
        let r = SfcStructureReport {
            schema_version: SCHEMA_VERSION,
            report_type: SfcStructureReport::REPORT_TYPE.to_string(),
            sfc_path: "build/m1/demo.sfc".to_string(),
            status: SfcStructureStatus::Ok,
            findings: Vec::new(),
            header_summary: SfcHeaderSummary {
                title: "DEMO".to_string(),
                mode_byte: 0x20,
                rom_size_byte: 0x08,
                country_byte: 0x01,
                checksum: 0x1234,
                checksum_complement: 0xEDCB,
                reset_vector: 0x8000,
                file_size_bytes: 262144,
            },
            module_a_summary: SfcModuleSummary {
                embed_offset: 0x8000,
                magic_ok: true,
                schema_version: 1,
                block_count: 3,
                entrypoint: 0x0200,
                total_file_len: 9048,
                flags: 0,
                in_file_sha256: "0".repeat(64),
                recomputed_in_file_sha256: "0".repeat(64),
                in_file_sha_matches: true,
            },
            module_b_summary: None,
        };
        round_trip(&r);
    }

    #[test]
    fn sfc_modules_audible_report_round_trip() {
        let mod_a = AudibleVerificationReport {
            schema_version: SCHEMA_VERSION,
            report_type: AudibleVerificationReport::REPORT_TYPE.to_string(),
            spc_path: "demo.sfc#module_A".to_string(),
            spc_sha256: "0".repeat(64),
            frames_rendered: 16384,
            sample_rate_hz: 32000,
            observed: ObservedAudio {
                max_abs: 11072,
                rms: 5520.0,
                bytes_zero: 0,
                bytes_total: 65536,
                fraction_zero: 0.0,
            },
            thresholds: AudibleThresholds {
                min_max_abs: 1000,
                min_rms: 200.0,
            },
            status: AudibleStatus::Ok,
            error: None,
        };
        let r = SfcModulesAudibleReport {
            schema_version: SCHEMA_VERSION,
            report_type: SfcModulesAudibleReport::REPORT_TYPE.to_string(),
            sfc_path: "build/m1/demo.sfc".to_string(),
            status: AudibleStatus::Ok,
            module_a_audible: mod_a.clone(),
            module_b_audible: Some(mod_a),
            modules_audio_identical: true,
            error: None,
        };
        round_trip(&r);
    }

    #[test]
    fn compile_spc_report_round_trip() {
        let r = CompileSpcReport {
            schema_version: SCHEMA_VERSION,
            report_type: CompileSpcReport::REPORT_TYPE.to_string(),
            project_name: "demo".to_string(),
            active_sample_id: "lead".to_string(),
            aram_image_sha256: "0".repeat(64),
            spc_file_sha256: "1".repeat(64),
            driver_code_sha256: "2".repeat(64),
            driver_code_bytes: 324,
            map_report_path: "build/m1/demo.aram-map.json".to_string(),
            spc_path: "build/m1/demo.spc".to_string(),
            aram_image_path: "build/m1/demo.aram.bin".to_string(),
        };
        round_trip(&r);
    }

    #[test]
    fn audible_verification_report_round_trip() {
        let r = AudibleVerificationReport {
            schema_version: SCHEMA_VERSION,
            report_type: AudibleVerificationReport::REPORT_TYPE.to_string(),
            spc_path: "build/m1/demo.spc".to_string(),
            spc_sha256: "0".repeat(64),
            frames_rendered: 16384,
            sample_rate_hz: 32000,
            observed: ObservedAudio {
                max_abs: 8420,
                rms: 2150.0,
                bytes_zero: 0,
                bytes_total: 65536,
                fraction_zero: 0.0,
            },
            thresholds: AudibleThresholds {
                min_max_abs: 1000,
                min_rms: 200.0,
            },
            status: AudibleStatus::Ok,
            error: None,
        };
        round_trip(&r);
        let mut r2 = r.clone();
        r2.status = AudibleStatus::SilentFail;
        r2.error = Some("max_abs=0".to_string());
        round_trip(&r2);
    }

    #[test]
    fn aram_echo_summary_round_trip() {
        let r = AramEchoSummary {
            enabled: true,
            edl: 4,
            buffer_bytes: 8192,
            hardware_tail_bytes: 4,
            esa: 0xDF,
            percent_of_aram: 12.5,
            writeback_safe: true,
        };
        round_trip(&r);
    }

    #[test]
    fn aram_source_dir_summary_round_trip() {
        let r = AramSourceDirSummary {
            source_count: 5,
            bytes: 20,
            padding_bytes: 236,
            start_addr: 0x1200,
        };
        round_trip(&r);
    }

    #[test]
    fn aram_samples_summary_round_trip() {
        let r = AramSamplesSummary {
            total_samples: 2,
            total_brr_bytes: 27,
            per_sample: vec![
                PerSampleAramEntry {
                    sample_id: "a".to_string(),
                    start_addr: 0x1300,
                    loop_addr: Some(0x1300),
                    bytes: 9,
                },
                PerSampleAramEntry {
                    sample_id: "b".to_string(),
                    start_addr: 0x1309,
                    loop_addr: Some(0x1309),
                    bytes: 18,
                },
            ],
        };
        round_trip(&r);
    }

    #[test]
    fn aram_map_report_round_trip_with_m1_fields() {
        let mut r = AramMapReport::stub();
        r.echo = Some(AramEchoSummary {
            enabled: true,
            edl: 4,
            buffer_bytes: 8192,
            hardware_tail_bytes: 4,
            esa: 0xDF,
            percent_of_aram: 12.5,
            writeback_safe: true,
        });
        r.source_directory = Some(AramSourceDirSummary {
            source_count: 1,
            bytes: 4,
            padding_bytes: 252,
            start_addr: 0x1200,
        });
        r.samples = Some(AramSamplesSummary {
            total_samples: 1,
            total_brr_bytes: 9,
            per_sample: vec![PerSampleAramEntry {
                sample_id: "a".to_string(),
                start_addr: 0x1300,
                loop_addr: Some(0x1300),
                bytes: 9,
            }],
        });
        r.warnings = vec!["FREE_LESS_THAN_256_BYTES".to_string()];
        round_trip(&r);
    }

    #[test]
    fn aram_map_report_pre_m14_without_meter_fields_still_parses() {
        // M0.6 manifest shape — none of the M1.4 meter summaries.
        let pre_m14 = r#"{
            "schema_version": 1,
            "report_type": "aram_map",
            "total_aram": 65536,
            "regions": [],
            "free_bytes": 65536,
            "collisions": []
        }"#;
        let r: AramMapReport = serde_json::from_str(pre_m14).unwrap();
        assert!(r.echo.is_none());
        assert!(r.source_directory.is_none());
        assert!(r.samples.is_none());
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn brr_encode_round_trip() {
        let r = BrrEncodeReport {
            schema_version: SCHEMA_VERSION,
            report_type: BrrEncodeReport::REPORT_TYPE.to_string(),
            source_path: "build/m1/sample.wav".to_string(),
            source_sha256: "0".repeat(64),
            source_frames: 4096,
            source_sample_rate_hz: 32000,
            output_path: "build/m1/sample.brr".to_string(),
            output_sha256: "1".repeat(64),
            output_bytes: 2304,
            total_blocks: 256,
            overall_rms_error: 12.34,
            overall_peak_error: 55,
            total_clamp_count: 0,
            filter_distribution: [10, 50, 100, 96],
            force_filter_0_first_block: true,
            loop_start_sample: Some(1024),
            loop_entry_block_index: Some(64),
            loop_click_score: Some(7.0),
            blocks: vec![BrrEncodeBlock {
                index: 0,
                filter: 0,
                shift: 11,
                end_flag: false,
                loop_flag: false,
                block_rms_error: 100.0,
                block_peak_error: 255,
                block_clamp_count: 0,
            }],
        };
        round_trip(&r);
    }

    #[test]
    fn atom_render_round_trip_populated() {
        let r = AtomRenderReport {
            schema_version: SCHEMA_VERSION,
            report_type: AtomRenderReport::REPORT_TYPE.to_string(),
            atom_id: "atom_0001".to_string(),
            atom_name: "sine_128".to_string(),
            atom_kind: "additive_single_cycle_v0".to_string(),
            cycle_len_samples: 128,
            partial_count: 1,
            normalize: true,
            atom_amplitude: 0.75,
            root_midi_note: 60,
            pcm_sha256: "a".repeat(64),
            brr_sha256: "b".repeat(64),
            brr_bytes: 72,
            encode_summary: crate::brr_encoder::EncodeSummary {
                total_blocks: 8,
                encoded_bytes: 72,
                overall_rms_error: 12.5,
                overall_peak_error: 256,
                total_clamp_count: 0,
                filter_distribution: [3, 2, 2, 1],
                loop_click_score: Some(1197.0),
            },
        };
        round_trip(&r);
    }

    #[test]
    fn atom_render_round_trip_minimal() {
        let r = AtomRenderReport {
            schema_version: SCHEMA_VERSION,
            report_type: AtomRenderReport::REPORT_TYPE.to_string(),
            atom_id: "x".to_string(),
            atom_name: "x".to_string(),
            atom_kind: "additive_single_cycle_v0".to_string(),
            cycle_len_samples: 64,
            partial_count: 1,
            normalize: false,
            atom_amplitude: 0.0,
            root_midi_note: 0,
            pcm_sha256: "0".repeat(64),
            brr_sha256: "0".repeat(64),
            brr_bytes: 36,
            encode_summary: crate::brr_encoder::EncodeSummary {
                total_blocks: 4,
                encoded_bytes: 36,
                overall_rms_error: 0.0,
                overall_peak_error: 0,
                total_clamp_count: 0,
                filter_distribution: [4, 0, 0, 0],
                loop_click_score: None,
            },
        };
        round_trip(&r);
    }

    #[test]
    fn loop_finder_round_trip() {
        let r = LoopFinderReport {
            schema_version: SCHEMA_VERSION,
            report_type: LoopFinderReport::REPORT_TYPE.to_string(),
            source_path: "build/m1/sample.wav".to_string(),
            source_sha256: "0".repeat(64),
            source_frames: 4096,
            window_samples: 32,
            snap_to_brr_block: true,
            candidates: vec![LoopCandidateJson {
                start_sample: 64,
                end_sample: 3072,
                rms_window_difference: 12.5,
                seam_click: 42,
                score: 23.0,
            }],
        };
        round_trip(&r);
    }

    #[test]
    fn audition_round_trip() {
        let r = AuditionReport {
            schema_version: SCHEMA_VERSION,
            report_type: AuditionReport::REPORT_TYPE.to_string(),
            input_path: "build/m1/sample.brr".to_string(),
            input_sha256: "0".repeat(64),
            output_path: "build/m1/sample.audition.wav".to_string(),
            output_sha256: "1".repeat(64),
            blocks_decoded: 256,
            samples_written: 4096,
            bytes_written: 8236,
            sample_rate_hz: 32000,
        };
        round_trip(&r);
    }

    #[test]
    fn aram_map_stub_accounts_for_full_aram() {
        let r = AramMapReport::stub();
        let used: u32 = r.regions.iter().map(|x| x.bytes).sum();
        assert_eq!(used + r.free_bytes, r.total_aram);
        assert_eq!(r.total_aram, 65536);
    }
}
