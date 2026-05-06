//! `sfcwc` — host CLI for the SFC Wave Compiler M0 harness.
//!
//! M0.1 ships shape only: real tool resolution in `doctor`; stub
//! reports for the other subcommands. Substance lands in M0.2–M0.6.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sfc_atomizer_core::asm::{
    sha256_hex_file, AsarBackend, AssembleError, AssembleInput, AssemblerBackend,
};
use sfc_atomizer_core::brr_fixtures::{run_fixture, M0_RAW_DECODE_FIXTURES};
use sfc_atomizer_core::report::{
    AramMapReport, AssembleReport, AssembleStatus, BrrFixtureReport, CalibrationReport,
    DoctorReport, DoctorStatus, DoctorTools, M0Manifest, RustInfo, SpcExportReport, ToolStatus,
    SCHEMA_VERSION,
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
    /// Smoke-test SPC export (M0.1: stub report).
    ExportSpcSmoke {
        #[arg(long, default_value = "build/m0/spc-export-report.json")]
        out: PathBuf,
    },
    /// Run the oracle calibration harness (M0.1: stub report).
    CalibrateOracle {
        #[arg(long)]
        oracle: Option<PathBuf>,
        #[arg(long, default_value = "build/m0/calibration-report.json")]
        out: PathBuf,
    },
    /// Run all M0 acceptance steps and write a manifest pointing at the reports.
    M0Acceptance {
        #[arg(long, default_value = "build/m0")]
        out: PathBuf,
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
        Command::ExportSpcSmoke { out } => cmd_export_spc_smoke(&out),
        Command::CalibrateOracle { oracle, out } => cmd_calibrate_oracle(oracle.as_deref(), &out),
        Command::M0Acceptance { out } => cmd_m0_acceptance(&out),
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

    let input = AssembleInput {
        source_path: source.to_path_buf(),
        output_image_path: image_out.to_path_buf(),
        working_dir: working_dir.to_path_buf(),
    };

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

fn cmd_export_spc_smoke(out: &Path) -> Result<(), CliError> {
    let report = SpcExportReport::stub();
    write_json(out, &report)?;
    eprintln!("export-spc-smoke: wrote {}", out.display());
    Ok(())
}

fn cmd_calibrate_oracle(_oracle: Option<&Path>, out: &Path) -> Result<(), CliError> {
    let report = CalibrationReport::stub();
    write_json(out, &report)?;
    eprintln!("calibrate-oracle: wrote {}", out.display());
    Ok(())
}

// =============================================================================
// m0-acceptance
// =============================================================================

fn cmd_m0_acceptance(out_dir: &Path) -> Result<(), CliError> {
    create_dir(out_dir)?;
    let workspace_root = std::env::current_dir().map_err(CliError::Cwd)?;

    let doctor_path = out_dir.join("doctor.json");
    let brr_path = out_dir.join("brr-fixture-report.json");
    let aram_path = out_dir.join("aram-map.json");
    let assemble_path = out_dir.join("assemble-report.json");
    let spc_path = out_dir.join("spc-export-report.json");
    let calibration_path = out_dir.join("calibration-report.json");

    write_json(&doctor_path, &build_doctor_report(&workspace_root))?;
    eprintln!("m0-acceptance: wrote {}", doctor_path.display());

    write_json(&brr_path, &BrrFixtureReport::stub())?;
    eprintln!("m0-acceptance: wrote {}", brr_path.display());

    write_json(&aram_path, &AramMapReport::stub())?;
    eprintln!("m0-acceptance: wrote {}", aram_path.display());

    write_json(&assemble_path, &AssembleReport::stub())?;
    eprintln!("m0-acceptance: wrote {}", assemble_path.display());

    write_json(&spc_path, &SpcExportReport::stub())?;
    eprintln!("m0-acceptance: wrote {}", spc_path.display());

    write_json(&calibration_path, &CalibrationReport::stub())?;
    eprintln!("m0-acceptance: wrote {}", calibration_path.display());

    let manifest = M0Manifest {
        schema_version: SCHEMA_VERSION,
        report_type: M0Manifest::REPORT_TYPE.to_string(),
        generated_at: None,
        doctor_report: doctor_path.display().to_string(),
        brr_fixture_report: brr_path.display().to_string(),
        aram_map_report: aram_path.display().to_string(),
        assemble_report: assemble_path.display().to_string(),
        spc_export_report: spc_path.display().to_string(),
        calibration_report: calibration_path.display().to_string(),
    };
    let manifest_path = out_dir.join("manifest.json");
    write_json(&manifest_path, &manifest)?;
    eprintln!("m0-acceptance: wrote {}", manifest_path.display());

    Ok(())
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
