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
        }
    }
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
    fn aram_map_stub_accounts_for_full_aram() {
        let r = AramMapReport::stub();
        let used: u32 = r.regions.iter().map(|x| x.bytes).sum();
        assert_eq!(used + r.free_bytes, r.total_aram);
        assert_eq!(r.total_aram, 65536);
    }
}
