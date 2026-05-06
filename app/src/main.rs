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
use sfc_atomizer_core::brr_fixtures::{run_fixture, M0_RAW_DECODE_FIXTURES};
use sfc_atomizer_core::manifest::verify_bundle;
use sfc_atomizer_core::report::{
    AramKind, AramMapReport, AssembleReport, AssembleStatus, BrrFixtureReport, BundleStatus,
    BundleSteps, BundleSummary, CalibrationReport, CalibrationStatus, DoctorReport, DoctorStatus,
    DoctorTools, FixtureSetInfo, M0Manifest, ObservedInfo, OracleInfo, ProvisionalTolerances,
    RenderInfo, RustInfo, SpcExportReport, SpcInitialState, SpcStatus, StepStatus, ToolStatus,
    SCHEMA_VERSION,
};
use sfc_atomizer_core::spc::{
    build_smoke_image, verify_structure, SpcCpuState, SpcImage, SMOKE_CPU_STATE, SPC_ARAM_SIZE,
    SPC_FILE_SIZE,
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
