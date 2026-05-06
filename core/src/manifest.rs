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
    AramMapReport, AssembleReport, BrrFixtureReport, CalibrationReport, DoctorReport, M0Manifest,
    SpcExportReport, SCHEMA_VERSION,
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

    let doctor = read_typed::<DoctorReport>("doctor", &doctor_path, &mut out, &mut versions);
    let brr = read_typed::<BrrFixtureReport>("brr_fixture", &brr_path, &mut out, &mut versions);
    let aram = read_typed::<AramMapReport>("aram_map", &aram_path, &mut out, &mut versions);
    let assemble =
        read_typed::<AssembleReport>("assemble", &assemble_path, &mut out, &mut versions);
    let spc = read_typed::<SpcExportReport>("spc_export", &spc_path, &mut out, &mut versions);
    let calibration =
        read_typed::<CalibrationReport>("calibration", &calibration_path, &mut out, &mut versions);

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

fn read_typed<T: serde::de::DeserializeOwned + HasReportType>(
    label: &str,
    path: &Path,
    out: &mut BundleIntegrity,
    versions: &mut Vec<u32>,
) -> Option<T> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            out.all_reports_present = false;
            out.findings.push(format!(
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
                out.findings.push(format!(
                    "{label}: report_type mismatch (got {:?}, expected {:?})",
                    r.report_type_field(),
                    T::REPORT_TYPE
                ));
            }
            Some(r)
        }
        Err(e) => {
            out.reports_parse = false;
            out.findings.push(format!(
                "{label}: report parse failed at {}: {e}",
                path.display()
            ));
            None
        }
    }
}

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
}
