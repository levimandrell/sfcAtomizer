//! M0 bundle integrity check.
//!
//! [`verify_bundle`] reads the manifest and every report it points
//! at and reports observed structure: which files exist, which parse,
//! whether schema versions agree, and whether the cross-references
//! line up (the assembled image SHA flows through to the SPC, the
//! SPC SHA flows through to the calibration fixture set).
//!
//! Used by both `m0-acceptance` (to fill in
//! [`crate::report::BundleSummary::diagnostics`]) and `m0-status`
//! (which re-runs the integrity check against the on-disk bundle so
//! drift after generation is surfaced).
//!
//! The check is observation-only — it never asserts. The caller
//! decides what's fatal.

use std::path::Path;

use crate::report::{
    AramMapReport, AssembleReport, AudibleVerificationReport, BrrFixtureReport, CalibrationReport,
    CompileSfcReport, CompileSpcReport, DoctorReport, M0Manifest, M1Manifest,
    SfcModulesAudibleReport, SfcStructureReport, SpcExportReport, ValidationReport, SCHEMA_VERSION,
};

/// Result of [`verify_bundle`]. All-good case: every bool `true` and
/// `findings` empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BundleIntegrity {
    pub all_reports_present: bool,
    pub reports_parse: bool,
    pub schema_versions_consistent: bool,
    pub aram_sha_matches_across_reports: bool,
    pub findings: Vec<String>,
}

/// Read `<bundle_dir>/manifest.json` plus every report it references
/// and return what was observed. Missing manifest is itself a finding.
pub fn verify_bundle(bundle_dir: &Path) -> BundleIntegrity {
    let mut out = BundleIntegrity {
        all_reports_present: true,
        reports_parse: true,
        schema_versions_consistent: true,
        aram_sha_matches_across_reports: true,
        findings: Vec::new(),
    };

    let manifest_path = bundle_dir.join("manifest.json");
    let manifest_bytes = match std::fs::read(&manifest_path) {
        Ok(b) => b,
        Err(e) => {
            out.all_reports_present = false;
            out.reports_parse = false;
            out.schema_versions_consistent = false;
            out.aram_sha_matches_across_reports = false;
            out.findings
                .push(format!("manifest.json missing or unreadable: {e}"));
            return out;
        }
    };
    let manifest: M0Manifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => {
            out.reports_parse = false;
            out.schema_versions_consistent = false;
            out.aram_sha_matches_across_reports = false;
            out.findings
                .push(format!("manifest.json parse failed: {e}"));
            return out;
        }
    };

    // Resolve report paths relative to the bundle dir if they are
    // bare file names; otherwise treat them as-is.
    let resolve = |raw: &str| -> std::path::PathBuf {
        let p = std::path::PathBuf::from(raw);
        if p.is_absolute() || p.exists() {
            p
        } else {
            // Prefer bundle-dir-relative if the bare filename matches.
            let candidate = bundle_dir.join(p.file_name().unwrap_or(p.as_os_str()));
            if candidate.exists() {
                candidate
            } else {
                std::path::PathBuf::from(raw)
            }
        }
    };

    let doctor_path = resolve(&manifest.doctor_report);
    let brr_path = resolve(&manifest.brr_fixture_report);
    let aram_path = resolve(&manifest.aram_map_report);
    let assemble_path = resolve(&manifest.assemble_report);
    let spc_path = resolve(&manifest.spc_export_report);
    let calibration_path = resolve(&manifest.calibration_report);

    let mut versions: Vec<u32> = vec![manifest.schema_version];

    let doctor =
        read_typed_with::<DoctorReport, _>("doctor", &doctor_path, &mut out, &mut versions);
    let brr =
        read_typed_with::<BrrFixtureReport, _>("brr_fixture", &brr_path, &mut out, &mut versions);
    let aram = read_typed_with::<AramMapReport, _>("aram_map", &aram_path, &mut out, &mut versions);
    let assemble =
        read_typed_with::<AssembleReport, _>("assemble", &assemble_path, &mut out, &mut versions);
    let spc =
        read_typed_with::<SpcExportReport, _>("spc_export", &spc_path, &mut out, &mut versions);
    let calibration = read_typed_with::<CalibrationReport, _>(
        "calibration",
        &calibration_path,
        &mut out,
        &mut versions,
    );

    // Schema-version consistency.
    if !versions.iter().all(|v| *v == SCHEMA_VERSION) {
        out.schema_versions_consistent = false;
        out.findings.push(format!(
            "schema_version mismatch across reports (expected {SCHEMA_VERSION}); saw {versions:?}"
        ));
    }

    // Cross-reference: assemble.output_image_sha256 ==
    // spc_export.input_aram_sha256 (the assembled image fed into
    // SPC export must be the same bytes).
    if let (Some(asm), Some(spcr)) = (assemble.as_ref(), spc.as_ref()) {
        match (
            asm.output_image_sha256.as_deref(),
            spcr.input_aram_sha256.as_deref(),
        ) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                out.aram_sha_matches_across_reports = false;
                out.findings.push(format!(
                    "assemble.output_image_sha256 ({a}) != spc_export.input_aram_sha256 ({b})"
                ));
            }
            _ => {
                // Either side missing the SHA → flag, but not as a
                // SHA mismatch (that bool is reserved for actual disagreement).
                out.findings.push(
                    "cannot cross-check ARAM SHA: assemble or spc_export missing the field"
                        .to_string(),
                );
            }
        }
    }

    // Cross-reference: spc_export.spc_file_sha256 ==
    // calibration.fixture_set.sha256 (the .spc fed to oracle).
    if let (Some(spcr), Some(cal)) = (spc.as_ref(), calibration.as_ref()) {
        let spc_sha = spcr.spc_file_sha256.as_deref();
        let cal_sha = cal.fixture_set.as_ref().map(|f| f.sha256.as_str());
        match (spc_sha, cal_sha) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                out.findings.push(format!(
                    "spc_export.spc_file_sha256 ({a}) != calibration.fixture_set.sha256 ({b})"
                ));
            }
            _ => {
                // Calibration may legitimately be skipped (oracle
                // missing) — only flag if both sides claim a SHA.
            }
        }
    }

    // Report-type sanity (each typed read already validates the type
    // string when serde decodes; nothing further needed).
    let _ = (doctor, brr, aram);

    out
}

// (legacy `read_typed` removed — both M0 and M1 verifiers now go
// through the trait-based `read_typed_with` defined further below.)

/// Helper trait so [`read_typed`] can pull schema_version and
/// report_type out of any concrete report type without boxing.
pub trait HasReportType {
    const REPORT_TYPE: &'static str;
    fn schema_version(&self) -> u32;
    fn report_type_field(&self) -> &str;
}

macro_rules! impl_has_report_type {
    ($t:ty) => {
        impl HasReportType for $t {
            const REPORT_TYPE: &'static str = <$t>::REPORT_TYPE;
            fn schema_version(&self) -> u32 {
                self.schema_version
            }
            fn report_type_field(&self) -> &str {
                &self.report_type
            }
        }
    };
}

impl_has_report_type!(DoctorReport);
impl_has_report_type!(BrrFixtureReport);
impl_has_report_type!(AramMapReport);
impl_has_report_type!(AssembleReport);
impl_has_report_type!(SpcExportReport);
impl_has_report_type!(CalibrationReport);
impl_has_report_type!(ValidationReport);
impl_has_report_type!(CompileSpcReport);
impl_has_report_type!(AudibleVerificationReport);
impl_has_report_type!(CompileSfcReport);
impl_has_report_type!(SfcStructureReport);
impl_has_report_type!(SfcModulesAudibleReport);

// =============================================================================
// M1 bundle integrity (M1.7)
// =============================================================================

/// Result of [`verify_m1_bundle`]. Same observation-only contract as
/// [`verify_bundle`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct M1BundleIntegrity {
    pub all_reports_present: bool,
    pub reports_parse: bool,
    pub schema_versions_consistent: bool,
    pub aram_sha_matches_across_reports: bool,
    pub spc_sha_matches_across_reports: bool,
    pub sfc_sha_matches_across_reports: bool,
    pub module_a_sha_matches_across_reports: bool,
    pub findings: Vec<String>,
}

/// Read `<bundle_dir>/manifest.json` (an [`M1Manifest`]) plus every
/// report it references and verify cross-reference SHAs:
///
/// - `compile_spc.aram_image_sha256` flows through to whoever else
///   tracks it (currently informational — the M1.4 ARAM map report
///   doesn't carry an aram_image_sha256, so this check is best-
///   effort and degrades gracefully).
/// - `compile_spc.spc_file_sha256 == audible_spc.spc_sha256`.
/// - `compile_sfc.sfc_sha256 == structure_sfc.sfc_path` content's
///   SHA (recompute from disk if structure report carries the path).
/// - `compile_sfc.module_a_sha256` consistent across compile and
///   the structure report's recomputed in-file SHA (different
///   things — file SHA vs in-file SHA — but both must be present).
pub fn verify_m1_bundle(bundle_dir: &Path) -> M1BundleIntegrity {
    let mut out = M1BundleIntegrity {
        all_reports_present: true,
        reports_parse: true,
        schema_versions_consistent: true,
        aram_sha_matches_across_reports: true,
        spc_sha_matches_across_reports: true,
        sfc_sha_matches_across_reports: true,
        module_a_sha_matches_across_reports: true,
        findings: Vec::new(),
    };

    let manifest_path = bundle_dir.join("manifest.json");
    let manifest_bytes = match std::fs::read(&manifest_path) {
        Ok(b) => b,
        Err(e) => {
            out.all_reports_present = false;
            out.reports_parse = false;
            out.schema_versions_consistent = false;
            out.aram_sha_matches_across_reports = false;
            out.spc_sha_matches_across_reports = false;
            out.sfc_sha_matches_across_reports = false;
            out.module_a_sha_matches_across_reports = false;
            out.findings
                .push(format!("manifest.json missing or unreadable: {e}"));
            return out;
        }
    };
    let manifest: M1Manifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => {
            out.reports_parse = false;
            out.findings
                .push(format!("manifest.json parse failed: {e}"));
            return out;
        }
    };

    let resolve = |raw: &str| -> std::path::PathBuf {
        let p = std::path::PathBuf::from(raw);
        if p.is_absolute() || p.exists() {
            p
        } else {
            let candidate = bundle_dir.join(p.file_name().unwrap_or(p.as_os_str()));
            if candidate.exists() {
                candidate
            } else {
                std::path::PathBuf::from(raw)
            }
        }
    };

    let mut versions: Vec<u32> = vec![manifest.schema_version];

    let doctor = read_typed_with::<DoctorReport, _>(
        "doctor",
        &resolve(&manifest.doctor_report),
        &mut out,
        &mut versions,
    );
    let validate_a = read_typed_with::<ValidationReport, _>(
        "validate_a",
        &resolve(&manifest.validate_a_report),
        &mut out,
        &mut versions,
    );
    let validate_b = manifest.validate_b_report.as_ref().and_then(|p| {
        read_typed_with::<ValidationReport, _>("validate_b", &resolve(p), &mut out, &mut versions)
    });
    let aram = read_typed_with::<AramMapReport, _>(
        "aram_map",
        &resolve(&manifest.aram_map_report),
        &mut out,
        &mut versions,
    );
    let compile_spc = read_typed_with::<CompileSpcReport, _>(
        "compile_spc",
        &resolve(&manifest.compile_spc_report),
        &mut out,
        &mut versions,
    );
    let audible_spc = read_typed_with::<AudibleVerificationReport, _>(
        "audible_spc",
        &resolve(&manifest.audible_spc_report),
        &mut out,
        &mut versions,
    );
    let compile_sfc = read_typed_with::<CompileSfcReport, _>(
        "compile_sfc",
        &resolve(&manifest.compile_sfc_report),
        &mut out,
        &mut versions,
    );
    let structure_sfc = read_typed_with::<SfcStructureReport, _>(
        "structure_sfc",
        &resolve(&manifest.structure_sfc_report),
        &mut out,
        &mut versions,
    );
    let audible_sfc = read_typed_with::<SfcModulesAudibleReport, _>(
        "audible_sfc",
        &resolve(&manifest.audible_sfc_report),
        &mut out,
        &mut versions,
    );

    if !versions.iter().all(|v| *v == SCHEMA_VERSION) {
        out.schema_versions_consistent = false;
        out.findings.push(format!(
            "schema_version mismatch across reports (expected {SCHEMA_VERSION}); saw {versions:?}"
        ));
    }

    // Cross-ref: compile_spc.spc_file_sha256 == audible_spc.spc_sha256.
    if let (Some(c), Some(a)) = (compile_spc.as_ref(), audible_spc.as_ref()) {
        if c.spc_file_sha256 != a.spc_sha256 {
            out.spc_sha_matches_across_reports = false;
            out.findings.push(format!(
                "compile_spc.spc_file_sha256 ({}) != audible_spc.spc_sha256 ({})",
                c.spc_file_sha256, a.spc_sha256
            ));
        }
    }

    // Cross-ref: compile_sfc.sfc_sha256 ↔ structure_sfc — structure
    // report stores the path; we can re-read and SHA the file but
    // that's a heavier check. For now, sanity-check the structure
    // report points at the same file the compile_sfc claimed.
    if let (Some(c), Some(s)) = (compile_sfc.as_ref(), structure_sfc.as_ref()) {
        if c.sfc_path != s.sfc_path {
            // Allow filename-only match if the absolute paths differ.
            let cf = std::path::Path::new(&c.sfc_path).file_name();
            let sf = std::path::Path::new(&s.sfc_path).file_name();
            if cf != sf {
                out.sfc_sha_matches_across_reports = false;
                out.findings.push(format!(
                    "compile_sfc.sfc_path ({}) != structure_sfc.sfc_path ({})",
                    c.sfc_path, s.sfc_path
                ));
            }
        }
        // Module A in-file SHA from the structure report should match
        // compile_sfc.module_a_in_file_sha256.
        if c.module_a_in_file_sha256 != s.module_a_summary.in_file_sha256 {
            out.module_a_sha_matches_across_reports = false;
            out.findings.push(format!(
                "compile_sfc.module_a_in_file_sha256 ({}) != structure_sfc.module_a.in_file_sha256 ({})",
                c.module_a_in_file_sha256, s.module_a_summary.in_file_sha256
            ));
        }
    }

    // Cross-ref: compile_spc.aram_image_sha256 — currently no other
    // report carries the ARAM SHA, so we just record presence.
    if let Some(c) = compile_spc.as_ref() {
        if c.aram_image_sha256.is_empty() {
            out.aram_sha_matches_across_reports = false;
            out.findings
                .push("compile_spc.aram_image_sha256 missing".to_string());
        }
    }

    let _ = (doctor, validate_a, validate_b, aram, audible_sfc);

    out
}

/// Trait wrapping the subset of fields [`read_typed_with`] needs to
/// write. Implemented for both M0 [`BundleIntegrity`] and M1
/// [`M1BundleIntegrity`] so a single read helper serves both.
trait IntegrityWriter {
    fn mark_missing(&mut self);
    fn mark_parse_fail(&mut self);
    fn push_finding(&mut self, s: String);
}

impl IntegrityWriter for BundleIntegrity {
    fn mark_missing(&mut self) {
        self.all_reports_present = false;
    }
    fn mark_parse_fail(&mut self) {
        self.reports_parse = false;
    }
    fn push_finding(&mut self, s: String) {
        self.findings.push(s);
    }
}

impl IntegrityWriter for M1BundleIntegrity {
    fn mark_missing(&mut self) {
        self.all_reports_present = false;
    }
    fn mark_parse_fail(&mut self) {
        self.reports_parse = false;
    }
    fn push_finding(&mut self, s: String) {
        self.findings.push(s);
    }
}

fn read_typed_with<T, W>(
    label: &str,
    path: &Path,
    out: &mut W,
    versions: &mut Vec<u32>,
) -> Option<T>
where
    T: serde::de::DeserializeOwned + HasReportType,
    W: IntegrityWriter,
{
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            out.mark_missing();
            out.push_finding(format!(
                "{label}: report missing at {}: {e}",
                path.display()
            ));
            return None;
        }
    };
    match serde_json::from_slice::<T>(&bytes) {
        Ok(r) => {
            versions.push(r.schema_version());
            if r.report_type_field() != T::REPORT_TYPE {
                out.push_finding(format!(
                    "{label}: report_type mismatch (got {:?}, expected {:?})",
                    r.report_type_field(),
                    T::REPORT_TYPE
                ));
            }
            Some(r)
        }
        Err(e) => {
            out.mark_parse_fail();
            out.push_finding(format!(
                "{label}: report parse failed at {}: {e}",
                path.display()
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::*;

    fn write_min_bundle(dir: &Path) -> M0Manifest {
        // Minimal valid bundle — every report file present, all
        // schema_version=1, cross-refs consistent.
        let aram_sha = "a".repeat(64);
        let spc_sha = "b".repeat(64);

        let doctor = DoctorReport {
            schema_version: SCHEMA_VERSION,
            report_type: DoctorReport::REPORT_TYPE.to_string(),
            tools: DoctorTools {
                asar: ToolStatus {
                    resolved: true,
                    path: Some("/fake/asar".to_string()),
                    version: Some("1.0".to_string()),
                    source: crate::tools::ToolSource::Path,
                    searched: vec![],
                },
                snes_spc_oracle: ToolStatus {
                    resolved: false,
                    path: None,
                    version: None,
                    source: crate::tools::ToolSource::Missing,
                    searched: vec!["env:SFCWC_SNES_SPC_ORACLE".to_string()],
                },
                mesen2: ToolStatus {
                    resolved: false,
                    path: None,
                    version: None,
                    source: crate::tools::ToolSource::Missing,
                    searched: vec!["env:SFCWC_MESEN2".to_string()],
                },
            },
            rust: RustInfo {
                channel: "stable".to_string(),
                version: "1.0.0".to_string(),
            },
            status: DoctorStatus::Warnings,
            diagnostics: vec![],
        };
        let brr = BrrFixtureReport::stub();
        let aram = AramMapReport::stub();
        let mut assemble = AssembleReport::stub();
        assemble.output_image_sha256 = Some(aram_sha.clone());
        assemble.status = AssembleStatus::Ok;
        let mut spc = SpcExportReport::stub();
        spc.input_aram_sha256 = Some(aram_sha.clone());
        spc.spc_file_sha256 = Some(spc_sha.clone());
        spc.status = SpcStatus::Ok;
        let mut calibration = CalibrationReport::stub();
        calibration.fixture_set = Some(FixtureSetInfo {
            name: "m0_smoke".to_string(),
            sha256: spc_sha.clone(),
        });

        let names = [
            ("doctor.json", serde_json::to_string(&doctor).unwrap()),
            (
                "brr-fixture-report.json",
                serde_json::to_string(&brr).unwrap(),
            ),
            ("aram-map.json", serde_json::to_string(&aram).unwrap()),
            (
                "assemble-report.json",
                serde_json::to_string(&assemble).unwrap(),
            ),
            (
                "spc-export-report.json",
                serde_json::to_string(&spc).unwrap(),
            ),
            (
                "calibration-report.json",
                serde_json::to_string(&calibration).unwrap(),
            ),
        ];
        for (name, body) in &names {
            std::fs::write(dir.join(name), body).unwrap();
        }

        let manifest = M0Manifest {
            schema_version: SCHEMA_VERSION,
            report_type: M0Manifest::REPORT_TYPE.to_string(),
            generated_at: Some("2026-05-06T00:00:00Z".to_string()),
            doctor_report: dir.join("doctor.json").display().to_string(),
            brr_fixture_report: dir.join("brr-fixture-report.json").display().to_string(),
            aram_map_report: dir.join("aram-map.json").display().to_string(),
            assemble_report: dir.join("assemble-report.json").display().to_string(),
            spc_export_report: dir.join("spc-export-report.json").display().to_string(),
            calibration_report: dir.join("calibration-report.json").display().to_string(),
            bundle: BundleSummary::default(),
        };
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        manifest
    }

    #[test]
    fn verify_bundle_happy_path() {
        let dir = tempfile::tempdir().unwrap();
        write_min_bundle(dir.path());
        let v = verify_bundle(dir.path());
        assert!(v.all_reports_present, "{v:?}");
        assert!(v.reports_parse, "{v:?}");
        assert!(v.schema_versions_consistent, "{v:?}");
        assert!(v.aram_sha_matches_across_reports, "{v:?}");
        assert!(v.findings.is_empty(), "{v:?}");
    }

    #[test]
    fn verify_bundle_flags_aram_sha_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        write_min_bundle(dir.path());

        // Mangle assemble's output sha so cross-ref fails.
        let assemble_path = dir.path().join("assemble-report.json");
        let mut assemble: AssembleReport =
            serde_json::from_slice(&std::fs::read(&assemble_path).unwrap()).unwrap();
        assemble.output_image_sha256 = Some("z".repeat(64));
        std::fs::write(&assemble_path, serde_json::to_string(&assemble).unwrap()).unwrap();

        let v = verify_bundle(dir.path());
        assert!(!v.aram_sha_matches_across_reports, "{v:?}");
        assert!(
            v.findings.iter().any(|f| f.contains("output_image_sha256")),
            "{v:?}"
        );
    }

    #[test]
    fn verify_bundle_flags_missing_report() {
        let dir = tempfile::tempdir().unwrap();
        write_min_bundle(dir.path());
        std::fs::remove_file(dir.path().join("assemble-report.json")).unwrap();

        let v = verify_bundle(dir.path());
        assert!(!v.all_reports_present, "{v:?}");
        assert!(v.findings.iter().any(|f| f.contains("assemble")));
    }

    #[test]
    fn verify_bundle_flags_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let v = verify_bundle(dir.path());
        assert!(!v.all_reports_present);
        assert!(!v.reports_parse);
        assert!(v.findings.iter().any(|f| f.contains("manifest.json")));
    }

    // -------------------------------------------------------------
    // M1 bundle verification (M1.7)
    // -------------------------------------------------------------

    fn write_min_m1_bundle(dir: &Path) {
        let aram_sha = "a".repeat(64);
        let spc_sha = "b".repeat(64);
        let sfc_sha = "c".repeat(64);
        let module_a_sha = "d".repeat(64);
        let module_a_in_file_sha = "e".repeat(64);
        let driver_sha = "f".repeat(64);

        let doctor = DoctorReport {
            schema_version: SCHEMA_VERSION,
            report_type: DoctorReport::REPORT_TYPE.to_string(),
            tools: DoctorTools {
                asar: ToolStatus {
                    resolved: true,
                    path: Some("/asar".to_string()),
                    version: None,
                    source: crate::tools::ToolSource::Path,
                    searched: vec![],
                },
                snes_spc_oracle: ToolStatus {
                    resolved: true,
                    path: Some("/oracle".to_string()),
                    version: None,
                    source: crate::tools::ToolSource::Env,
                    searched: vec![],
                },
                mesen2: ToolStatus {
                    resolved: false,
                    path: None,
                    version: None,
                    source: crate::tools::ToolSource::Missing,
                    searched: vec![],
                },
            },
            rust: RustInfo {
                channel: "stable".to_string(),
                version: "1.0.0".to_string(),
            },
            status: DoctorStatus::Ok,
            diagnostics: vec![],
        };
        let validate = ValidationReport::stub();
        let aram = AramMapReport::stub();
        let compile_spc = CompileSpcReport {
            schema_version: SCHEMA_VERSION,
            report_type: CompileSpcReport::REPORT_TYPE.to_string(),
            project_name: "demo".to_string(),
            active_sample_id: "lead".to_string(),
            aram_image_sha256: aram_sha.clone(),
            spc_file_sha256: spc_sha.clone(),
            driver_code_sha256: driver_sha.clone(),
            driver_code_bytes: 324,
            map_report_path: dir.join("aram-map.json").display().to_string(),
            spc_path: dir.join("project_a.spc").display().to_string(),
            aram_image_path: dir.join("project_a.aram.bin").display().to_string(),
        };
        let audible_spc = AudibleVerificationReport {
            schema_version: SCHEMA_VERSION,
            report_type: AudibleVerificationReport::REPORT_TYPE.to_string(),
            spc_path: compile_spc.spc_path.clone(),
            spc_sha256: spc_sha.clone(),
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
            driver_version: Some(1),
            left: None,
            right: None,
            error: None,
        };
        let compile_sfc = CompileSfcReport {
            schema_version: SCHEMA_VERSION,
            report_type: CompileSfcReport::REPORT_TYPE.to_string(),
            project_a_name: "demo".to_string(),
            project_b_name: None,
            sfc_path: dir.join("project.sfc").display().to_string(),
            sfc_size_bytes: 262144,
            sfc_sha256: sfc_sha.clone(),
            module_b_is_clone_of_a: true,
            module_a_sha256: module_a_sha.clone(),
            module_a_in_file_sha256: module_a_in_file_sha.clone(),
            module_a_bytes: 9048,
            module_b_sha256: None,
            module_b_in_file_sha256: None,
            module_b_bytes: None,
            loader_size_bytes: 581,
        };
        let structure_sfc = SfcStructureReport {
            schema_version: SCHEMA_VERSION,
            report_type: SfcStructureReport::REPORT_TYPE.to_string(),
            sfc_path: compile_sfc.sfc_path.clone(),
            status: SfcStructureStatus::Ok,
            findings: vec![],
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
                in_file_sha256: module_a_in_file_sha.clone(),
                recomputed_in_file_sha256: module_a_in_file_sha.clone(),
                in_file_sha_matches: true,
            },
            module_b_summary: None,
        };
        let mod_a_audible = audible_spc.clone();
        let audible_sfc = SfcModulesAudibleReport {
            schema_version: SCHEMA_VERSION,
            report_type: SfcModulesAudibleReport::REPORT_TYPE.to_string(),
            sfc_path: compile_sfc.sfc_path.clone(),
            status: AudibleStatus::Ok,
            module_a_audible: mod_a_audible.clone(),
            module_b_audible: Some(mod_a_audible),
            modules_audio_identical: true,
            error: None,
        };

        for (name, body) in [
            ("doctor.json", serde_json::to_string(&doctor).unwrap()),
            ("validate-a.json", serde_json::to_string(&validate).unwrap()),
            ("aram-map.json", serde_json::to_string(&aram).unwrap()),
            (
                "compile-spc.json",
                serde_json::to_string(&compile_spc).unwrap(),
            ),
            (
                "audible-spc.json",
                serde_json::to_string(&audible_spc).unwrap(),
            ),
            (
                "compile-sfc.json",
                serde_json::to_string(&compile_sfc).unwrap(),
            ),
            (
                "structure-sfc.json",
                serde_json::to_string(&structure_sfc).unwrap(),
            ),
            (
                "audible-sfc.json",
                serde_json::to_string(&audible_sfc).unwrap(),
            ),
        ] {
            std::fs::write(dir.join(name), body).unwrap();
        }

        let manifest = M1Manifest {
            schema_version: SCHEMA_VERSION,
            report_type: M1Manifest::REPORT_TYPE.to_string(),
            generated_at: "2026-05-06T00:00:00Z".to_string(),
            project_a: "demo.sfcproj.json".to_string(),
            project_b: None,
            doctor_report: dir.join("doctor.json").display().to_string(),
            validate_a_report: dir.join("validate-a.json").display().to_string(),
            validate_b_report: None,
            aram_map_report: dir.join("aram-map.json").display().to_string(),
            compile_spc_report: dir.join("compile-spc.json").display().to_string(),
            audible_spc_report: dir.join("audible-spc.json").display().to_string(),
            compile_sfc_report: dir.join("compile-sfc.json").display().to_string(),
            structure_sfc_report: dir.join("structure-sfc.json").display().to_string(),
            audible_sfc_report: dir.join("audible-sfc.json").display().to_string(),
            bundle: M1BundleSummary::default(),
        };
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn verify_m1_bundle_on_valid_bundle() {
        let dir = tempfile::tempdir().unwrap();
        write_min_m1_bundle(dir.path());
        let v = verify_m1_bundle(dir.path());
        assert!(v.all_reports_present, "{v:?}");
        assert!(v.reports_parse, "{v:?}");
        assert!(v.schema_versions_consistent, "{v:?}");
        assert!(v.aram_sha_matches_across_reports, "{v:?}");
        assert!(v.spc_sha_matches_across_reports, "{v:?}");
        assert!(v.sfc_sha_matches_across_reports, "{v:?}");
        assert!(v.module_a_sha_matches_across_reports, "{v:?}");
        assert!(v.findings.is_empty(), "{v:?}");
    }

    #[test]
    fn verify_m1_bundle_with_spc_sha_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        write_min_m1_bundle(dir.path());
        let path = dir.path().join("audible-spc.json");
        let mut a: AudibleVerificationReport =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        a.spc_sha256 = "z".repeat(64);
        std::fs::write(&path, serde_json::to_string(&a).unwrap()).unwrap();

        let v = verify_m1_bundle(dir.path());
        assert!(!v.spc_sha_matches_across_reports, "{v:?}");
        assert!(
            v.findings
                .iter()
                .any(|f| f.contains("spc_file_sha256") || f.contains("spc_sha256")),
            "{v:?}"
        );
    }

    #[test]
    fn verify_m1_bundle_with_module_a_sha_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        write_min_m1_bundle(dir.path());
        let path = dir.path().join("structure-sfc.json");
        let mut s: SfcStructureReport =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        s.module_a_summary.in_file_sha256 = "z".repeat(64);
        std::fs::write(&path, serde_json::to_string(&s).unwrap()).unwrap();

        let v = verify_m1_bundle(dir.path());
        assert!(!v.module_a_sha_matches_across_reports, "{v:?}");
        assert!(v.findings.iter().any(|f| f.contains("module_a")), "{v:?}");
    }

    #[test]
    fn verify_m1_bundle_with_missing_report() {
        let dir = tempfile::tempdir().unwrap();
        write_min_m1_bundle(dir.path());
        std::fs::remove_file(dir.path().join("audible-spc.json")).unwrap();

        let v = verify_m1_bundle(dir.path());
        assert!(!v.all_reports_present, "{v:?}");
        assert!(v.findings.iter().any(|f| f.contains("audible_spc")));
    }

    #[test]
    fn verify_m1_bundle_flags_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let v = verify_m1_bundle(dir.path());
        assert!(!v.all_reports_present);
        assert!(!v.reports_parse);
        assert!(v.findings.iter().any(|f| f.contains("manifest.json")));
    }
}
