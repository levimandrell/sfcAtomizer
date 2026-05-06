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
use sfc_atomizer_core::report::{
    AramMapReport, AssembleReport, AssembleStatus, BrrFixtureReport, CalibrationReport,
    CalibrationStatus, DoctorReport, DoctorStatus, DoctorTools, FixtureSetInfo, M0Manifest,
    ObservedInfo, OracleInfo, ProvisionalTolerances, RenderInfo, RustInfo, SpcExportReport,
    SpcInitialState, SpcStatus, ToolStatus, SCHEMA_VERSION,
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
    let manifest_path = out_dir.join("manifest.json");

    // 1. Doctor.
    write_json(&doctor_path, &build_doctor_report(&workspace_root))?;
    eprintln!("m0-acceptance: doctor -> {}", doctor_path.display());

    // 2. Decode BRR fixtures.
    cmd_decode_fixtures(&brr_path)?;

    // 3. Assemble. Source is the canonical M0 smoke .asm relative to
    // the workspace root (assumed to be cwd).
    let smoke_asm = workspace_root
        .join("core")
        .join("fixtures")
        .join("asm")
        .join("m0_smoke.asm");
    cmd_assemble_smoke(&smoke_asm, &assemble_path, &driver_bin)?;

    // 4. Export SPC. Reads driver.bin from step 3; failure-as-data if
    // assemble didn't produce one.
    cmd_export_spc_smoke(&driver_bin, &spc_path, &smoke_spc, true)?;

    // 5. ARAM map. Real if assemble succeeded, stub otherwise.
    let aram_report = match read_aram_image(&driver_bin) {
        Some(img) => map_from_image(&img),
        None => AramMapReport::stub(),
    };
    write_json(&aram_map_path, &aram_report)?;
    eprintln!("m0-acceptance: aram-map -> {}", aram_map_path.display());

    // 6. Calibrate-oracle. Real if oracle wrapper resolves; failure-
    // as-data otherwise. The chain continues regardless.
    let oracle_pcm = out_dir.join("oracle.pcm_s16le");
    cmd_calibrate_oracle(None, &smoke_spc, 2048, &calibration_path, &oracle_pcm)?;

    // 7. Manifest.
    let manifest = M0Manifest {
        schema_version: SCHEMA_VERSION,
        report_type: M0Manifest::REPORT_TYPE.to_string(),
        generated_at: None,
        doctor_report: doctor_path.display().to_string(),
        brr_fixture_report: brr_path.display().to_string(),
        aram_map_report: aram_map_path.display().to_string(),
        assemble_report: assemble_path.display().to_string(),
        spc_export_report: spc_path.display().to_string(),
        calibration_report: calibration_path.display().to_string(),
    };
    write_json(&manifest_path, &manifest)?;
    eprintln!(
        "m0-acceptance: wrote 7 reports + manifest -> {}",
        manifest_path.display()
    );

    Ok(())
}

/// Read a 64 KB ARAM image into a fixed array. Returns `None` if the
/// file is missing or not exactly the right size — m0-acceptance
/// uses that to decide between a real ARAM map and the stub.
fn read_aram_image(path: &Path) -> Option<[u8; ARAM_LEN]> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() != ARAM_LEN {
        return None;
    }
    let mut img = [0u8; ARAM_LEN];
    img.copy_from_slice(&bytes);
    Some(img)
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
