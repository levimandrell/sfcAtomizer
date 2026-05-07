//! End-to-end tests for the `sfcwc` CLI binary.
//!
//! These tests invoke the compiled `sfcwc` executable via
//! `CARGO_BIN_EXE_sfcwc`, write outputs into per-test [`TempDir`]s,
//! and validate the resulting JSON against the M0.1 report schemas.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfcwc"))
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(bin()).args(args).output().expect("run sfcwc")
}

fn run_with_arg_path(args: &[&str], path_arg: &str, path: &Path) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .arg(path_arg)
        .arg(path)
        .output()
        .expect("run sfcwc")
}

fn read_json(path: &Path) -> Value {
    let s = std::fs::read_to_string(path).expect("read report");
    serde_json::from_str(&s).expect("valid json")
}

fn assert_envelope(v: &Value, expected_type: &str) {
    assert_eq!(v["schema_version"], 1, "wrong schema_version: {v}");
    assert_eq!(v["report_type"], expected_type, "wrong report_type: {v}");
}

#[test]
fn doctor_json_to_stdout() {
    let out = run(&["doctor", "--json"]);
    assert!(
        out.status.success(),
        "doctor --json failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let v: Value = serde_json::from_str(&stdout).expect("stdout is valid json");
    assert_envelope(&v, "doctor");
    assert!(v["tools"]["asar"].is_object());
    assert!(v["tools"]["snes_spc_oracle"].is_object());
    assert!(v["tools"]["mesen2"].is_object());
    assert!(v["rust"]["channel"].is_string());
}

#[test]
fn doctor_writes_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("doctor.json");
    let out = run_with_arg_path(&["doctor"], "--out", &path);
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&path);
    assert_envelope(&v, "doctor");
}

#[test]
fn doctor_sfcwc_asar_env_resolves() {
    // Using the test binary itself as a sentinel: it exists and is
    // executable, so resolve_asar() must accept it via the env path.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("doctor.json");
    let sentinel = bin();
    let out = Command::new(bin())
        .env("SFCWC_ASAR", &sentinel)
        .args(["doctor", "--out"])
        .arg(&path)
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "doctor failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = read_json(&path);
    assert_eq!(v["tools"]["asar"]["resolved"], true);
    assert_eq!(v["tools"]["asar"]["source"], "env");
    let reported_path = v["tools"]["asar"]["path"]
        .as_str()
        .expect("asar.path is string");
    assert_eq!(PathBuf::from(reported_path), sentinel);
}

#[test]
fn decode_fixtures_runs_corpus() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("brr.json");
    let out = run_with_arg_path(&["decode-fixtures"], "--out", &path);
    assert!(
        out.status.success(),
        "decode-fixtures failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = read_json(&path);
    assert_envelope(&v, "brr_fixture");
    assert_eq!(v["fixture_set"], "m0_raw_decode");
    assert_eq!(v["total"], 9);
    assert_eq!(v["passed"], 9);
    assert_eq!(v["failed"], 0);
    assert_eq!(v["skipped"], 0);

    let results = v["results"].as_array().expect("results array");
    assert_eq!(results.len(), 9);
    let names: Vec<&str> = results
        .iter()
        .map(|r| r["name"].as_str().expect("name string"))
        .collect();
    let expected_names = [
        "filter0_basic",
        "filter0_shift_clamp",
        "filter1_zero_history",
        "filter1_nonzero_history",
        "filter2_nonzero_history",
        "filter3_nonzero_history",
        "multi_block_predictor_history",
        "loop_boundary_history",
        "flags_end_loop_ignored_by_raw_decode",
    ];
    assert_eq!(names, expected_names);
    for r in results {
        assert_eq!(r["passed"], true, "fixture failed: {r}");
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("9/9 passed"),
        "expected '9/9 passed' in stderr, got: {stderr}"
    );
}

/// Workspace root resolved from the app crate's `CARGO_MANIFEST_DIR`.
/// `m0-acceptance` resolves `core/fixtures/asm/m0_smoke.asm` relative
/// to the process cwd, so tests that exercise it set
/// `Command::current_dir(workspace_root())`.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Path to `core/fixtures/asm/m0_smoke.asm` resolved relative to the
/// app crate's manifest dir at compile time so the test works
/// regardless of cwd.
fn smoke_asm_path() -> PathBuf {
    workspace_root()
        .join("core")
        .join("fixtures")
        .join("asm")
        .join("m0_smoke.asm")
}

#[test]
fn assemble_smoke_when_asar_resolved() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip assemble_smoke_when_asar_resolved: asar not resolved on this host");
        return;
    }

    let dir = TempDir::new().unwrap();
    let report_path = dir.path().join("r.json");
    let image_path = dir.path().join("d.bin");
    let src = smoke_asm_path();
    assert!(src.is_file(), "smoke .asm fixture missing at {src:?}");

    let out = Command::new(bin())
        .args(["assemble-smoke", "--source"])
        .arg(&src)
        .args(["--out"])
        .arg(&report_path)
        .args(["--out-image"])
        .arg(&image_path)
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "exit failure: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v = read_json(&report_path);
    assert_envelope(&v, "assemble");
    assert_eq!(v["status"], "ok", "report status: {v}");
    assert_eq!(v["backend"], "asar");
    assert_eq!(v["output_bytes"], 65536);
    assert_eq!(v["exit_code"], 0);
    let sha = v["output_image_sha256"]
        .as_str()
        .expect("sha string present on success");
    assert_eq!(sha.len(), 64, "sha must be 64 hex chars: {sha}");
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "sha must be hex: {sha}"
    );
    assert!(v["error"].is_null(), "error should be null on success: {v}");

    // Image: exactly 64 KB, sentinel bytes at offset 0x0200..0x0202.
    let image = std::fs::read(&image_path).expect("read image");
    assert_eq!(image.len(), 65536, "image size");
    assert_eq!(
        &image[0x0200..0x0203],
        &[0x00, 0x2F, 0xFD],
        "sentinel bytes mismatch — locked to NOP + BRA -3 from m0_smoke.asm"
    );
    // Spot-check: every other byte is zero.
    let nonzero: Vec<usize> = image
        .iter()
        .enumerate()
        .filter(|(_, b)| **b != 0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        nonzero,
        vec![0x0201, 0x0202],
        "expected only the BRA opcode + disp to be nonzero"
    );

    // Stderr summary line should announce success and the sha.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("asar OK"),
        "expected 'asar OK' in stderr: {stderr}"
    );
    assert!(stderr.contains(sha), "stderr should echo sha: {stderr}");
}

#[test]
fn assemble_smoke_when_asar_missing() {
    let dir = TempDir::new().unwrap();
    let bogus_asar = dir.path().join("not-asar-does-not-exist");
    let isolated_path = dir.path().to_path_buf();
    let report_path = dir.path().join("r.json");
    let image_path = dir.path().join("d.bin");
    let src = smoke_asm_path();

    let out = Command::new(bin())
        .env("SFCWC_ASAR", &bogus_asar)
        .env("PATH", &isolated_path) // contains no asar.exe
        .args(["assemble-smoke", "--source"])
        .arg(&src)
        .args(["--out"])
        .arg(&report_path)
        .args(["--out-image"])
        .arg(&image_path)
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "expected failure-as-data (exit 0) even when asar is missing: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v = read_json(&report_path);
    assert_envelope(&v, "assemble");
    assert_eq!(v["status"], "error", "report status: {v}");
    let err_msg = v["error"]
        .as_str()
        .expect("error string present on failure");
    assert!(
        err_msg.contains("SFCWC_ASAR"),
        "error should mention SFCWC_ASAR: {err_msg}"
    );

    // Image file should NOT exist (we never reached the assemble step).
    assert!(
        !image_path.exists(),
        "image should not be created when asar is missing"
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("asar not resolved"),
        "expected 'asar not resolved' in stderr: {stderr}"
    );
}

#[test]
fn export_spc_smoke_with_assembled_aram() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip export_spc_smoke_with_assembled_aram: asar not resolved");
        return;
    }

    let dir = TempDir::new().unwrap();
    let assemble_report = dir.path().join("a.json");
    let driver_bin = dir.path().join("driver.bin");
    let spc_report = dir.path().join("spc.json");
    let smoke_spc = dir.path().join("smoke.spc");

    // Step 1: assemble the smoke .asm into driver.bin.
    let out = Command::new(bin())
        .args(["assemble-smoke", "--source"])
        .arg(smoke_asm_path())
        .args(["--out"])
        .arg(&assemble_report)
        .args(["--out-image"])
        .arg(&driver_bin)
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "assemble failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Step 2: export-spc-smoke against driver.bin.
    let out = Command::new(bin())
        .args(["export-spc-smoke", "--aram"])
        .arg(&driver_bin)
        .args(["--out"])
        .arg(&spc_report)
        .args(["--out-spc"])
        .arg(&smoke_spc)
        .arg("--verify-structure")
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "export-spc-smoke failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Report invariants.
    let v = read_json(&spc_report);
    assert_envelope(&v, "spc_export");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["verified_structure"], true);
    assert_eq!(v["file_size_bytes"], 66048);
    assert_eq!(v["initial_state"]["pc"], 0x0200);
    assert_eq!(v["initial_state"]["sp"], 0xEF);
    assert_eq!(v["initial_state"]["a"], 0);
    assert_eq!(v["initial_state"]["psw"], 0);
    let spc_sha = v["spc_file_sha256"].as_str().expect("spc_file_sha256 set");
    assert_eq!(spc_sha.len(), 64);

    // SPC byte invariants.
    let spc_bytes = std::fs::read(&smoke_spc).expect("read smoke.spc");
    assert_eq!(spc_bytes.len(), 66048);
    assert_eq!(
        &spc_bytes[0..0x21],
        b"SNES-SPC700 Sound File Data v0.30",
        "SPC magic must match"
    );
    assert_eq!(spc_bytes[0x23], 0x1B, "ID666 indicator: absent");
    assert_eq!(spc_bytes[0x24], 0x1E, "minor version 30");
    assert_eq!(&spc_bytes[0x25..0x27], &[0x00, 0x02], "PC = $0200 LE");
    assert_eq!(spc_bytes[0x2B], 0xEF, "SP");
    assert_eq!(spc_bytes[0x1016C], 0x60, "DSP $6C (FLG)");
    // Sentinel bytes from the assembled ARAM at file offset 0x100 + 0x200.
    assert_eq!(
        &spc_bytes[0x300..0x303],
        &[0x00, 0x2F, 0xFD],
        "smoke ARAM sentinel"
    );
}

#[test]
fn export_spc_smoke_when_aram_missing() {
    let dir = TempDir::new().unwrap();
    let report_path = dir.path().join("spc.json");
    let spc_path = dir.path().join("smoke.spc");
    let bogus_aram = dir.path().join("does-not-exist.bin");

    let out = Command::new(bin())
        .args(["export-spc-smoke", "--aram"])
        .arg(&bogus_aram)
        .args(["--out"])
        .arg(&report_path)
        .args(["--out-spc"])
        .arg(&spc_path)
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "expected failure-as-data exit 0: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v = read_json(&report_path);
    assert_envelope(&v, "spc_export");
    assert_eq!(v["status"], "error");
    let err = v["error"].as_str().expect("error string set");
    assert!(
        err.contains("aram input missing"),
        "error should mention missing aram: {err}"
    );
    assert!(
        !spc_path.exists(),
        "smoke.spc must not be created when aram is missing"
    );
}

/// Path to the workspace's build directory of the oracle wrapper, if
/// the C++ build has been run.
fn oracle_wrapper_path() -> PathBuf {
    let exe = if cfg!(windows) {
        "snes_spc_oracle.exe"
    } else {
        "snes_spc_oracle"
    };
    workspace_root()
        .join("tools")
        .join("snes_spc_oracle")
        .join("build")
        .join("Release")
        .join(exe)
}

fn oracle_resolved_for_test() -> bool {
    if std::env::var_os("SFCWC_SNES_SPC_ORACLE").is_some() {
        return true;
    }
    oracle_wrapper_path().is_file()
        || workspace_root()
            .join("tools")
            .join("snes_spc_oracle")
            .join("build")
            .join(if cfg!(windows) {
                "snes_spc_oracle.exe"
            } else {
                "snes_spc_oracle"
            })
            .is_file()
}

#[test]
fn calibrate_oracle_when_oracle_resolved() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip calibrate_oracle_when_oracle_resolved: asar not resolved");
        return;
    }
    if !oracle_resolved_for_test() {
        eprintln!("skip calibrate_oracle_when_oracle_resolved: oracle wrapper not built");
        return;
    }

    let dir = TempDir::new().unwrap();
    // Step 1: produce a smoke.spc via assemble + export-spc-smoke.
    let assemble_report = dir.path().join("a.json");
    let driver_bin = dir.path().join("driver.bin");
    let spc_report = dir.path().join("spc.json");
    let smoke_spc = dir.path().join("smoke.spc");

    let out = Command::new(bin())
        .args(["assemble-smoke", "--source"])
        .arg(smoke_asm_path())
        .args(["--out"])
        .arg(&assemble_report)
        .args(["--out-image"])
        .arg(&driver_bin)
        .output()
        .expect("run sfcwc");
    assert!(out.status.success());

    let out = Command::new(bin())
        .args(["export-spc-smoke", "--aram"])
        .arg(&driver_bin)
        .args(["--out"])
        .arg(&spc_report)
        .args(["--out-spc"])
        .arg(&smoke_spc)
        .arg("--verify-structure")
        .output()
        .expect("run sfcwc");
    assert!(out.status.success());

    // Step 2: calibrate-oracle against the smoke .spc.
    let cal_report = dir.path().join("cal.json");
    let pcm_path = dir.path().join("oracle.pcm");
    let out = Command::new(bin())
        .args(["calibrate-oracle", "--input-spc"])
        .arg(&smoke_spc)
        .args(["--frames", "2048"])
        .args(["--out"])
        .arg(&cal_report)
        .args(["--out-pcm"])
        .arg(&pcm_path)
        .current_dir(workspace_root()) // for default-oracle resolution
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "calibrate-oracle failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v = read_json(&cal_report);
    assert_envelope(&v, "calibration");
    assert_eq!(v["status"], "provisional_not_ci_gate");
    assert_eq!(v["oracle"]["backend"], "snes_spc_wrapper");
    assert_eq!(v["render"]["frames"], 2048);
    assert_eq!(v["render"]["sample_rate_hz"], 32000);
    assert_eq!(v["render"]["channels"], 2);
    assert_eq!(v["observed"]["voice_render_max_abs_lsb"], 0);
    assert!(
        v["observed"]["voice_render_rms_lsb"]
            .as_f64()
            .map(|x| x == 0.0)
            .unwrap_or(false),
        "rms must be 0.0 for muted smoke: {v}"
    );
    assert_eq!(v["ci_gate"], false);
    assert_eq!(v["freeze_target"], "M1");
    assert!(
        v["error"].is_null(),
        "no error expected on resolved oracle: {v}"
    );

    // PCM file: exactly 2048 * 4 = 8192 bytes, all zero.
    let pcm = std::fs::read(&pcm_path).expect("read pcm");
    assert_eq!(pcm.len(), 8192);
    assert!(pcm.iter().all(|&b| b == 0), "muted smoke must be all zeros");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("max_abs=0"),
        "stderr should announce max_abs=0: {stderr}"
    );
}

#[test]
fn calibrate_oracle_when_oracle_missing() {
    let dir = TempDir::new().unwrap();
    let bogus_oracle = dir.path().join("not-an-oracle.exe");
    let cal_report = dir.path().join("cal.json");
    let pcm_path = dir.path().join("oracle.pcm");
    let smoke_spc = dir.path().join("placeholder.spc");
    // The oracle-missing branch fires before we open the SPC, so the
    // file doesn't have to exist.

    let out = Command::new(bin())
        .env("SFCWC_SNES_SPC_ORACLE", &bogus_oracle)
        .env("PATH", dir.path()) // isolated PATH with no asar/oracle
        .args(["calibrate-oracle", "--input-spc"])
        .arg(&smoke_spc)
        .args(["--out"])
        .arg(&cal_report)
        .args(["--out-pcm"])
        .arg(&pcm_path)
        // Force the workspace-default branch to also miss by pointing
        // cwd at an empty tempdir (no tools/snes_spc_oracle/build).
        .current_dir(dir.path())
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "expected failure-as-data exit 0: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let v = read_json(&cal_report);
    assert_envelope(&v, "calibration");
    assert_eq!(v["status"], "error");
    let err = v["error"].as_str().expect("error string");
    assert!(
        err.contains("oracle wrapper not resolved"),
        "error should mention oracle: {err}"
    );
    assert!(
        !pcm_path.exists(),
        "oracle PCM must not be created when oracle is missing"
    );
}

#[test]
fn calibrate_oracle_when_input_spc_missing() {
    if !oracle_resolved_for_test() {
        eprintln!("skip calibrate_oracle_when_input_spc_missing: oracle wrapper not built");
        return;
    }

    let dir = TempDir::new().unwrap();
    let cal_report = dir.path().join("cal.json");
    let pcm_path = dir.path().join("oracle.pcm");
    let bogus_spc = dir.path().join("does-not-exist.spc");

    let out = Command::new(bin())
        .args(["calibrate-oracle", "--input-spc"])
        .arg(&bogus_spc)
        .args(["--out"])
        .arg(&cal_report)
        .args(["--out-pcm"])
        .arg(&pcm_path)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(out.status.success(), "{:?}", out);

    let v = read_json(&cal_report);
    assert_envelope(&v, "calibration");
    assert_eq!(v["status"], "error");
    let err = v["error"].as_str().expect("error string");
    assert!(
        err.to_lowercase().contains("input spc"),
        "error should mention input SPC: {err}"
    );
}

/// Copy `core/fixtures/asm/m0_smoke.asm` into `<tempdir>/core/fixtures/asm/`
/// so a test can use the tempdir as an isolated workspace_root for
/// resolution checks (asar/oracle missing scenarios).
fn copy_smoke_asm_into(workspace: &Path) {
    let dst_dir = workspace.join("core").join("fixtures").join("asm");
    std::fs::create_dir_all(&dst_dir).unwrap();
    std::fs::copy(smoke_asm_path(), dst_dir.join("m0_smoke.asm")).unwrap();
}

#[test]
fn m0_acceptance_full_chain_when_all_tools_resolved() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip m0_acceptance_full_chain_when_all_tools_resolved: asar not resolved");
        return;
    }
    if !oracle_resolved_for_test() {
        eprintln!("skip m0_acceptance_full_chain_when_all_tools_resolved: oracle not built");
        return;
    }

    let dir = TempDir::new().unwrap();
    let out_dir = dir.path().join("m0");
    // Make Mesen2 "resolved" by pointing SFCWC_MESEN2 at any existing
    // file (the test binary itself); doctor never executes mesen2,
    // it only reports presence.
    let out = Command::new(bin())
        .env("SFCWC_MESEN2", bin())
        .args(["m0-acceptance", "--out"])
        .arg(&out_dir)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "m0-acceptance failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Files present.
    let names = [
        ("doctor.json", "doctor"),
        ("brr-fixture-report.json", "brr_fixture"),
        ("aram-map.json", "aram_map"),
        ("assemble-report.json", "assemble"),
        ("spc-export-report.json", "spc_export"),
        ("calibration-report.json", "calibration"),
    ];
    for (name, ty) in names {
        let p = out_dir.join(name);
        assert!(p.is_file(), "missing {name}");
        assert_envelope(&read_json(&p), ty);
    }
    assert!(out_dir.join("driver.bin").is_file());
    assert!(out_dir.join("smoke.spc").is_file());
    assert!(out_dir.join("oracle.pcm_s16le").is_file());

    // Manifest bundle: status ok, every step ok, all three SHAs.
    let manifest = read_json(&out_dir.join("manifest.json"));
    assert_envelope(&manifest, "m0_manifest");
    let bundle = &manifest["bundle"];
    assert_eq!(bundle["status"], "ok", "bundle: {bundle}");
    let steps = &bundle["steps"];
    for step in [
        "doctor",
        "decode_fixtures",
        "assemble",
        "spc_export",
        "aram_map",
        "calibration",
    ] {
        assert_eq!(steps[step], "ok", "step {step}: {bundle}");
    }
    assert!(bundle["aram_image_sha256"].is_string());
    assert!(bundle["spc_file_sha256"].is_string());
    assert!(bundle["oracle_pcm_sha256"].is_string());

    // Stderr summary mentions bundle.status.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bundle.status=ok"),
        "expected 'bundle.status=ok' in stderr: {stderr}"
    );
}

#[test]
fn m0_acceptance_when_oracle_missing() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip m0_acceptance_when_oracle_missing: asar not resolved");
        return;
    }

    // Set up an isolated workspace with smoke.asm but no
    // tools/snes_spc_oracle/build/ — so the oracle resolution chain
    // finds nothing in env (bogus) and nothing at workspace defaults.
    let dir = TempDir::new().unwrap();
    copy_smoke_asm_into(dir.path());
    let out_dir = dir.path().join("build").join("m0");
    let bogus_oracle = dir.path().join("not-an-oracle");

    let out = Command::new(bin())
        .env("SFCWC_SNES_SPC_ORACLE", &bogus_oracle)
        .args(["m0-acceptance", "--out"])
        .arg(&out_dir)
        .current_dir(dir.path())
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "m0-acceptance failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest = read_json(&out_dir.join("manifest.json"));
    let bundle = &manifest["bundle"];
    assert_eq!(bundle["status"], "degraded", "bundle: {bundle}");
    assert_eq!(bundle["steps"]["assemble"], "ok");
    assert_eq!(bundle["steps"]["spc_export"], "ok");
    assert_eq!(bundle["steps"]["aram_map"], "ok");
    assert_eq!(bundle["steps"]["calibration"], "skipped");
}

#[test]
fn m0_acceptance_when_asar_missing() {
    let dir = TempDir::new().unwrap();
    copy_smoke_asm_into(dir.path());
    let out_dir = dir.path().join("build").join("m0");
    let bogus = dir.path().join("not-asar");

    let out = Command::new(bin())
        .env("SFCWC_ASAR", &bogus)
        .env("PATH", dir.path()) // empty PATH — no asar
        .args(["m0-acceptance", "--out"])
        .arg(&out_dir)
        .current_dir(dir.path())
        .output()
        .expect("run sfcwc");
    // Failure-as-data: process exits 0 even though the bundle has errored.
    assert!(out.status.success(), "{:?}", out);

    let manifest = read_json(&out_dir.join("manifest.json"));
    let bundle = &manifest["bundle"];
    assert_eq!(bundle["status"], "error", "bundle: {bundle}");
    assert_eq!(bundle["steps"]["doctor"], "error");
    assert_eq!(bundle["steps"]["assemble"], "skipped");
}

#[test]
fn m0_status_on_valid_bundle() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip m0_status_on_valid_bundle: asar not resolved");
        return;
    }

    let dir = TempDir::new().unwrap();
    let out_dir = dir.path().join("m0");
    let acc = Command::new(bin())
        .args(["m0-acceptance", "--out"])
        .arg(&out_dir)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(acc.status.success());

    // Human-readable output.
    let st = Command::new(bin())
        .args(["m0-status", "--bundle"])
        .arg(&out_dir)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(
        st.status.success(),
        "m0-status failed: stderr={}",
        String::from_utf8_lossy(&st.stderr)
    );
    let stdout = String::from_utf8_lossy(&st.stdout);
    assert!(
        stdout.contains("bundle.status"),
        "expected 'bundle.status' in stdout: {stdout}"
    );

    // JSON output parses as a manifest.
    let st_json = Command::new(bin())
        .args(["m0-status", "--json", "--bundle"])
        .arg(&out_dir)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(st_json.status.success());
    let v: Value = serde_json::from_slice(&st_json.stdout).expect("valid manifest json");
    assert_envelope(&v, "m0_manifest");
    assert!(v["bundle"]["steps"].is_object());
}

#[test]
fn m0_status_on_missing_bundle() {
    let dir = TempDir::new().unwrap();
    let st = Command::new(bin())
        .args(["m0-status", "--bundle"])
        .arg(dir.path())
        .output()
        .expect("run sfcwc");
    assert!(
        !st.status.success(),
        "m0-status should exit non-zero on missing bundle"
    );
    let stderr = String::from_utf8_lossy(&st.stderr);
    assert!(
        stderr.contains("no bundle") || stderr.contains("missing"),
        "expected diagnostic about missing bundle: {stderr}"
    );
}

#[test]
fn m0_status_on_corrupted_bundle() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip m0_status_on_corrupted_bundle: asar not resolved");
        return;
    }

    let dir = TempDir::new().unwrap();
    let out_dir = dir.path().join("m0");
    let acc = Command::new(bin())
        .args(["m0-acceptance", "--out"])
        .arg(&out_dir)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(acc.status.success());

    // Corrupt one of the reports.
    std::fs::write(out_dir.join("assemble-report.json"), "{}\n").unwrap();

    let st = Command::new(bin())
        .args(["m0-status", "--bundle"])
        .arg(&out_dir)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(
        !st.status.success(),
        "m0-status should exit non-zero on corrupted bundle"
    );
    let stdout = String::from_utf8_lossy(&st.stdout);
    assert!(
        stdout.contains("findings") || stdout.contains("parse"),
        "expected integrity findings in stdout: {stdout}"
    );
}

#[test]
fn aram_map_in_acceptance_partitions_total() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip aram_map_in_acceptance_partitions_total: asar not resolved");
        return;
    }

    let dir = TempDir::new().unwrap();
    let out_dir = dir.path().join("m0");
    let out = Command::new(bin())
        .env("SFCWC_MESEN2", bin())
        .args(["m0-acceptance", "--out"])
        .arg(&out_dir)
        .current_dir(workspace_root())
        .output()
        .expect("run sfcwc");
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&out_dir.join("aram-map.json"));
    let total = v["total_aram"].as_u64().unwrap();
    let free = v["free_bytes"].as_u64().unwrap();
    let used: u64 = v["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["bytes"].as_u64().unwrap())
        .sum();
    // M0.4 semantics: regions partition ARAM (sum to total).
    assert_eq!(used, total, "regions must partition total ARAM");
    assert_eq!(total, 65536);

    // Smoke driver_code region: $0201..$0202 (NOP byte coincides with
    // pre-fill so the first nonzero byte is the BRA opcode at $0201).
    let driver = v["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "driver_code")
        .expect("driver_code region present");
    assert_eq!(driver["start"], "0x0201");
    assert_eq!(driver["end"], "0x0202");
    assert_eq!(driver["bytes"], 2);

    // free_bytes equals sum of free regions.
    let claimed_free: u64 = v["regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "free")
        .map(|r| r["bytes"].as_u64().unwrap())
        .sum();
    assert_eq!(free, claimed_free);
}

#[test]
fn missing_source_arg_fails_with_clap_usage() {
    let out = run(&["assemble-smoke"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--source"),
        "expected --source in stderr: {stderr}"
    );
}

// =============================================================================
// M1.1 — new-project / validate-project
// =============================================================================

#[test]
fn new_project_writes_minimal_valid_template() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("demo.sfcproj.json");
    let out = Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&path)
        .args(["--name", "demo"])
        .output()
        .expect("run sfcwc");
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&path);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["project"]["name"], "demo");
    assert_eq!(v["project"]["tick_rate_hz"], 60);
    assert_eq!(v["driver"]["profile"], "sample_basic");
    assert_eq!(v["driver"]["bytecode_version"], 1);
    assert_eq!(v["master_echo"]["enabled"], false);
    assert!(v["sample_pool"].as_array().unwrap().is_empty());
    assert_eq!(v["m1"]["active_sample_id"], "");
}

#[test]
fn validate_project_with_template_reports_pre_import_errors() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("p.json");
    let report = dir.path().join("v.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", "demo"])
        .output()
        .unwrap();

    let out = Command::new(bin())
        .args(["validate-project", "--project"])
        .arg(&proj)
        .arg("--json")
        .args(["--out"])
        .arg(&report)
        .output()
        .expect("run sfcwc");
    assert_eq!(out.status.code(), Some(2), "expected exit 2: {:?}", out);

    let v = read_json(&report);
    assert_eq!(v["report_type"], "validation");
    assert_eq!(v["status"], "invalid");
    let errors = v["errors"].as_array().unwrap();
    assert!(errors.iter().any(|e| e["path"] == "/sample_pool"));
    assert!(errors.iter().any(|e| e["path"] == "/m1/active_sample_id"));
}

#[test]
fn validate_project_with_valid_input() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("p.json");
    let json = r#"{
        "schema_version": 1,
        "project": { "name": "ok", "tick_rate_hz": 60 },
        "driver": { "profile": "sample_basic", "bytecode_version": 1 },
        "master_echo": { "enabled": false, "edl": 0, "efb": 0,
                          "evol_l": 0, "evol_r": 0,
                          "fir": [127, 0, 0, 0, 0, 0, 0, 0] },
        "sample_pool": [
            {
                "id": "s1", "name": "lead",
                "source": {
                    "path": "audio/lead.wav",
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "format": "wav", "sample_rate_hz": 32000,
                    "channels": 1, "frames": 65536
                },
                "root_midi_note": 60,
                "loop": { "enabled": true, "start_sample": 1024,
                           "end_sample": 32768, "snap": "brr_block_16" },
                "playback": { "volume": 1.0, "pan": 0.0, "echo": false,
                               "envelope": { "type": "adsr", "attack": 9,
                                              "decay": 4, "sustain_level": 5,
                                              "sustain_rate": 12 } }
            }
        ],
        "m1": { "active_sample_id": "s1" }
    }"#;
    std::fs::write(&proj, json).unwrap();
    let out = Command::new(bin())
        .args(["validate-project", "--project"])
        .arg(&proj)
        .arg("--json")
        .output()
        .expect("run sfcwc");
    assert!(out.status.success(), "{:?}", out);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "ok");
    assert!(v["errors"].as_array().unwrap().is_empty());
}

#[test]
fn validate_project_with_invalid_edl() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("p.json");
    // master enabled with edl=0 — rule #8.
    let json = r#"{
        "schema_version": 1,
        "project": { "name": "bad", "tick_rate_hz": 60 },
        "driver": { "profile": "sample_basic", "bytecode_version": 1 },
        "master_echo": { "enabled": true, "edl": 0, "efb": 0,
                          "evol_l": 0, "evol_r": 0,
                          "fir": [127, 0, 0, 0, 0, 0, 0, 0] },
        "sample_pool": [
            {
                "id": "s1", "name": "lead",
                "source": {
                    "path": "x.wav",
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "format": "wav", "sample_rate_hz": 32000,
                    "channels": 1, "frames": 1024
                },
                "root_midi_note": 60,
                "loop": { "enabled": false },
                "playback": { "volume": 1.0, "pan": 0.0, "echo": false,
                               "envelope": { "type": "gain_raw", "gain_byte": 127 } }
            }
        ],
        "m1": { "active_sample_id": "s1" }
    }"#;
    std::fs::write(&proj, json).unwrap();
    let out = Command::new(bin())
        .args(["validate-project", "--project"])
        .arg(&proj)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let errors = v["errors"].as_array().unwrap();
    assert!(errors.iter().any(|e| e["path"] == "/master_echo/edl"));
}

#[test]
fn validate_project_io_error_on_missing_file() {
    let dir = TempDir::new().unwrap();
    let bogus = dir.path().join("does-not-exist.json");
    let out = Command::new(bin())
        .args(["validate-project", "--project"])
        .arg(&bogus)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["status"], "io_error");
}

// =============================================================================
// M1.2 — sfcwc import
// =============================================================================

fn write_minimal_wav(path: &Path, sample_rate: u32, channels: u8, bits: u8, frames: u32) {
    let bytes_per_sample = u32::from(bits / 8);
    let block_align = u32::from(channels) * bytes_per_sample;
    let byte_rate = sample_rate * block_align;
    let data_size = frames * block_align;
    let fmt_size = 16u32;
    let chunk_size = 4 + (8 + fmt_size) + (8 + data_size);
    let mut buf = Vec::new();
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&fmt_size.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&u16::from(channels).to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(block_align as u16).to_le_bytes());
    buf.extend_from_slice(&u16::from(bits).to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.resize(buf.len() + data_size as usize, 0);
    std::fs::write(path, buf).unwrap();
}

#[test]
fn import_happy_path_exit_0() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("p.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", "demo"])
        .output()
        .unwrap();
    let audio = dir.path().join("lead.wav");
    write_minimal_wav(&audio, 32_000, 1, 16, 4096);
    let out = Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj)
        .args(["--audio"])
        .arg(&audio)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("import: added"), "stderr: {stderr}");
    // Project now validates.
    let v = Command::new(bin())
        .args(["validate-project", "--project"])
        .arg(&proj)
        .output()
        .unwrap();
    assert_eq!(v.status.code(), Some(0));
}

#[test]
fn import_missing_audio_exit_1() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("p.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", "demo"])
        .output()
        .unwrap();
    let bogus = dir.path().join("does-not-exist.wav");
    let out = Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj)
        .args(["--audio"])
        .arg(&bogus)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn import_unsupported_extension_exit_2() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("p.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", "demo"])
        .output()
        .unwrap();
    let audio = dir.path().join("a.flac");
    std::fs::write(&audio, b"fake").unwrap();
    let out = Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj)
        .args(["--audio"])
        .arg(&audio)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

// =============================================================================
// M1.3 — encode-brr / preview-brr / find-loop-candidates
// =============================================================================

fn write_pcm16_wav_with_samples(path: &Path, sample_rate: u32, channels: u16, samples: &[i16]) {
    let bytes_per_sample = 2u32;
    let block_align = u32::from(channels) * bytes_per_sample;
    let byte_rate = sample_rate * block_align;
    let data_size = (samples.len() as u32) * 2;
    let fmt_size = 16u32;
    let chunk_size = 4 + (8 + fmt_size) + (8 + data_size);
    let mut buf = Vec::new();
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&fmt_size.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(block_align as u16).to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, buf).unwrap();
}

fn synth_sine_pcm(len: usize, period: f64, amp: f64) -> Vec<i16> {
    (0..len)
        .map(|i| {
            let phase = (i as f64) * std::f64::consts::TAU / period;
            (phase.sin() * amp).round() as i16
        })
        .collect()
}

#[test]
fn encode_brr_writes_brr_and_report() {
    let dir = TempDir::new().unwrap();
    let wav = dir.path().join("sine.wav");
    let brr = dir.path().join("sine.brr");
    let report = dir.path().join("encode-report.json");
    let pcm = synth_sine_pcm(256, 64.0, 8000.0);
    write_pcm16_wav_with_samples(&wav, 32000, 1, &pcm);

    let out = Command::new(bin())
        .args(["encode-brr", "--audio"])
        .arg(&wav)
        .args(["--out-brr"])
        .arg(&brr)
        .args(["--out-report"])
        .arg(&report)
        .args(["--no-force-filter-0-first-block"])
        .output()
        .expect("run sfcwc");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let brr_bytes = std::fs::read(&brr).unwrap();
    assert_eq!(brr_bytes.len() % 9, 0);
    assert_eq!(brr_bytes.len(), 256 / 16 * 9);
    let v = read_json(&report);
    assert_envelope(&v, "brr_encode");
    assert_eq!(v["total_blocks"], 16);
    assert_eq!(v["output_bytes"], 144);
    let peak = v["overall_peak_error"].as_u64().unwrap();
    assert!(peak < 256, "peak error {peak} >= 256");
}

#[test]
fn preview_brr_writes_wav_and_report() {
    let dir = TempDir::new().unwrap();
    // Hand-roll a single all-zero BRR block — decodes to 16 zero
    // samples, so the audition WAV PCM body is 32 zero bytes.
    let brr = dir.path().join("z.brr");
    std::fs::write(&brr, [0u8; 9]).unwrap();
    let wav = dir.path().join("z.wav");
    let report = dir.path().join("audition.json");

    let out = Command::new(bin())
        .args(["preview-brr", "--brr"])
        .arg(&brr)
        .args(["--out-wav"])
        .arg(&wav)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .expect("run sfcwc");
    assert!(out.status.success(), "{:?}", out);
    let wav_bytes = std::fs::read(&wav).unwrap();
    assert_eq!(&wav_bytes[0..4], b"RIFF");
    assert_eq!(wav_bytes.len(), 44 + 32);
    let v = read_json(&report);
    assert_envelope(&v, "audition");
    assert_eq!(v["blocks_decoded"], 1);
    assert_eq!(v["samples_written"], 16);
    assert_eq!(v["sample_rate_hz"], 32000);
}

// =============================================================================
// M2.0 — source SHA enforcement (--refresh-source-hash)
// =============================================================================

#[test]
fn cli_compile_spc_with_intact_source_succeeds() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "intact");
    let out = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&proj)
        .args(["--out-spc"])
        .arg(dir.path().join("a.spc"))
        .args(["--out-image"])
        .arg(dir.path().join("a.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("a.map.json"))
        .args(["--out-report"])
        .arg(dir.path().join("a.compile.json"))
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_compile_spc_with_drifted_source_errors() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "drift");
    // Mutate the imported audio so its SHA differs from the
    // recorded one. Append a single zero byte (stays a valid WAV
    // at the file system level).
    let audio = dir.path().join("audio").join("drift.wav");
    let mut bytes = std::fs::read(&audio).expect("read audio");
    // Modify a sample byte well into the data section (offset 60 is
    // safely past the WAV header).
    if bytes.len() > 60 {
        bytes[60] ^= 0xFF;
    }
    std::fs::write(&audio, &bytes).unwrap();

    let out = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&proj)
        .args(["--out-spc"])
        .arg(dir.path().join("a.spc"))
        .args(["--out-image"])
        .arg(dir.path().join("a.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("a.map.json"))
        .args(["--out-report"])
        .arg(dir.path().join("a.compile.json"))
        .output()
        .expect("run sfcwc");
    assert_eq!(out.status.code(), Some(2), "expected exit 2 on SHA drift");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("source SHA-256 mismatch"),
        "expected explicit SHA mismatch message; got: {stderr}"
    );
}

#[test]
fn cli_compile_spc_with_refresh_source_hash_succeeds() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "refresh");
    // Mutate the audio (same as the drift test).
    let audio = dir.path().join("audio").join("refresh.wav");
    let mut bytes = std::fs::read(&audio).expect("read audio");
    bytes[60] ^= 0xFF;
    std::fs::write(&audio, &bytes).unwrap();

    let out = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&proj)
        .args(["--out-spc"])
        .arg(dir.path().join("a.spc"))
        .args(["--out-image"])
        .arg(dir.path().join("a.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("a.map.json"))
        .args(["--out-report"])
        .arg(dir.path().join("a.compile.json"))
        .args(["--refresh-source-hash"])
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "expected exit 0 with --refresh-source-hash; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("refresh-source-hash:"),
        "expected refresh-source-hash log line; got: {stderr}"
    );

    // Project should now have the new SHA persisted; running again
    // without the flag should succeed.
    let out2 = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&proj)
        .args(["--out-spc"])
        .arg(dir.path().join("a2.spc"))
        .args(["--out-image"])
        .arg(dir.path().join("a2.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("a2.map.json"))
        .args(["--out-report"])
        .arg(dir.path().join("a2.compile.json"))
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out2.status.code(),
        Some(0),
        "second run without --refresh should succeed once project SHA is updated"
    );
}

// =============================================================================
// M1.7 — m1-acceptance / m1-status
// =============================================================================

#[test]
fn cli_m1_acceptance_full_chain_one_project() {
    use sfc_atomizer_core::tools::{resolve_asar, resolve_snes_spc_oracle};
    if !resolve_asar().resolved
        || !resolve_snes_spc_oracle(&workspace_root()).resolved && !oracle_resolved_for_test()
    {
        eprintln!("skip cli_m1_acceptance_full_chain_one_project: asar / oracle missing");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "m17_demo");
    let bundle = dir.path().join("bundle");
    let oracle = oracle_wrapper_path();

    let out = Command::new(bin())
        .env("SFCWC_SNES_SPC_ORACLE", &oracle)
        .args(["m1-acceptance", "--project-a"])
        .arg(&proj)
        .args(["--out"])
        .arg(&bundle)
        .args(["--frames", "8192"])
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // All step reports present.
    for name in [
        "doctor.json",
        "validate-a.json",
        "aram-map.json",
        "compile-spc.json",
        "audible-spc.json",
        "compile-sfc.json",
        "structure-sfc.json",
        "audible-sfc.json",
        "manifest.json",
    ] {
        assert!(
            bundle.join(name).is_file(),
            "missing report {name}: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let manifest = read_json(&bundle.join("manifest.json"));
    assert_envelope(&manifest, "m1_manifest");
    assert_eq!(manifest["bundle"]["status"], "ok");
    assert!(manifest["bundle"]["aram_image_sha256"].is_string());
    assert!(manifest["bundle"]["spc_file_sha256"].is_string());
    assert!(manifest["bundle"]["sfc_file_sha256"].is_string());
    assert!(manifest["bundle"]["module_a_sha256"].is_string());
    assert!(manifest["bundle"]["driver_code_sha256"].is_string());
    assert_eq!(manifest["bundle"]["modules_audio_identical"], true);
}

#[test]
fn cli_m1_acceptance_two_projects() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved || !oracle_resolved_for_test() {
        eprintln!("skip: asar / oracle missing");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj_a = make_project_with_one_imported_sample(dir.path(), "m17_a");

    let proj_b = dir.path().join("m17_b.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj_b)
        .args(["--name", "m17_b"])
        .output()
        .unwrap();
    let audio_b = dir.path().join("low.wav");
    let pcm_b = synth_sine_pcm(4096, 128.0, 6000.0);
    write_pcm16_wav_with_samples(&audio_b, 22050, 1, &pcm_b);
    Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj_b)
        .args(["--audio"])
        .arg(&audio_b)
        .output()
        .unwrap();

    let bundle = dir.path().join("bundle");
    let oracle = oracle_wrapper_path();
    let out = Command::new(bin())
        .env("SFCWC_SNES_SPC_ORACLE", &oracle)
        .args(["m1-acceptance", "--project-a"])
        .arg(&proj_a)
        .args(["--project-b"])
        .arg(&proj_b)
        .args(["--out"])
        .arg(&bundle)
        .args(["--frames", "4096"])
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest = read_json(&bundle.join("manifest.json"));
    assert_eq!(manifest["bundle"]["status"], "ok");
    assert_eq!(manifest["bundle"]["steps"]["validate_b"], "ok");
    assert_eq!(manifest["bundle"]["modules_audio_identical"], false);
    // Distinct module SHAs.
    let a = manifest["bundle"]["module_a_sha256"].as_str().unwrap();
    let b = manifest["bundle"]["module_b_sha256"].as_str().unwrap();
    assert_ne!(a, b);
}

#[test]
fn cli_m1_status_on_valid_bundle() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved || !oracle_resolved_for_test() {
        eprintln!("skip: asar / oracle missing");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "m17_status");
    let bundle = dir.path().join("bundle");
    let oracle = oracle_wrapper_path();
    Command::new(bin())
        .env("SFCWC_SNES_SPC_ORACLE", &oracle)
        .args(["m1-acceptance", "--project-a"])
        .arg(&proj)
        .args(["--out"])
        .arg(&bundle)
        .args(["--frames", "4096"])
        .output()
        .unwrap();

    let out = Command::new(bin())
        .args(["m1-status", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_m1_status_on_missing_bundle() {
    let dir = TempDir::new().unwrap();
    let out = Command::new(bin())
        .args(["m1-status", "--bundle"])
        .arg(dir.path().join("nope"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn cli_m1_status_on_corrupted_bundle() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved || !oracle_resolved_for_test() {
        eprintln!("skip: asar / oracle missing");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "m17_corrupt");
    let bundle = dir.path().join("bundle");
    let oracle = oracle_wrapper_path();
    Command::new(bin())
        .env("SFCWC_SNES_SPC_ORACLE", &oracle)
        .args(["m1-acceptance", "--project-a"])
        .arg(&proj)
        .args(["--out"])
        .arg(&bundle)
        .args(["--frames", "4096"])
        .output()
        .unwrap();

    // Mangle compile-spc.json — change spc_file_sha256.
    let path = bundle.join("compile-spc.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    v["spc_file_sha256"] = serde_json::json!("z".repeat(64));
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    let out = Command::new(bin())
        .args(["m1-status", "--bundle"])
        .arg(&bundle)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("spc_sha_matches_across_reports   = false")
            || stdout.contains("integrity findings"),
        "expected integrity drift in stdout: {stdout}"
    );
}

// =============================================================================
// M1.6 — compile-sfc / verify-sfc-structure / verify-sfc-modules-audible
// =============================================================================

#[test]
fn cli_compile_sfc_one_project_happy_path() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip cli_compile_sfc_one_project_happy_path: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "sfc_demo");
    let sfc = dir.path().join("demo.sfc");
    let report = dir.path().join("compile.json");

    let out = Command::new(bin())
        .args(["compile-sfc", "--project-a"])
        .arg(&proj)
        .args(["--out-sfc"])
        .arg(&sfc)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&sfc).unwrap();
    assert_eq!(bytes.len(), 262_144, "256 KB minimum");
    let v = read_json(&report);
    assert_envelope(&v, "compile_sfc");
    assert_eq!(v["module_b_is_clone_of_a"], true);
    assert!(v["sfc_sha256"].as_str().unwrap().len() == 64);
}

#[test]
fn cli_compile_sfc_two_projects_happy_path() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj_a = make_project_with_one_imported_sample(dir.path(), "demo_a");
    let proj_b = dir.path().join("demo_b.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj_b)
        .args(["--name", "demo_b"])
        .output()
        .unwrap();
    // Synthesise a different sample (lower-pitched sine) for B.
    let audio_b = dir.path().join("lead_b.wav");
    let pcm_b = synth_sine_pcm(2048, 32.0, 6000.0);
    write_pcm16_wav_with_samples(&audio_b, 22050, 1, &pcm_b);
    Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj_b)
        .args(["--audio"])
        .arg(&audio_b)
        .output()
        .unwrap();

    let sfc = dir.path().join("two.sfc");
    let report = dir.path().join("two.compile.json");
    let out = Command::new(bin())
        .args(["compile-sfc", "--project-a"])
        .arg(&proj_a)
        .args(["--project-b"])
        .arg(&proj_b)
        .args(["--out-sfc"])
        .arg(&sfc)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = read_json(&report);
    assert_envelope(&v, "compile_sfc");
    assert_eq!(v["module_b_is_clone_of_a"], false);
    assert_ne!(v["module_a_sha256"], v["module_b_sha256"]);
}

#[test]
fn cli_compile_sfc_invalid_project_a() {
    if !sfc_atomizer_core::tools::resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("bad.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", "bad"])
        .output()
        .unwrap();
    let out = Command::new(bin())
        .args(["compile-sfc", "--project-a"])
        .arg(&proj)
        .args(["--out-sfc"])
        .arg(dir.path().join("x.sfc"))
        .args(["--out-report"])
        .arg(dir.path().join("x.json"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn cli_verify_sfc_structure_on_compiled_sfc_passes() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "structure_demo");
    let sfc = dir.path().join("s.sfc");
    Command::new(bin())
        .args(["compile-sfc", "--project-a"])
        .arg(&proj)
        .args(["--out-sfc"])
        .arg(&sfc)
        .args(["--out-report"])
        .arg(dir.path().join("c.json"))
        .output()
        .unwrap();

    let report = dir.path().join("struct.json");
    let out = Command::new(bin())
        .args(["verify-sfc-structure", "--sfc"])
        .arg(&sfc)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v = read_json(&report);
    assert_envelope(&v, "sfc_structure");
    assert_eq!(v["status"], "ok");
}

#[test]
fn cli_verify_sfc_structure_on_corrupted_sfc_fails() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "corrupt_demo");
    let sfc = dir.path().join("c.sfc");
    Command::new(bin())
        .args(["compile-sfc", "--project-a"])
        .arg(&proj)
        .args(["--out-sfc"])
        .arg(&sfc)
        .args(["--out-report"])
        .arg(dir.path().join("c.json"))
        .output()
        .unwrap();

    // Corrupt the LoROM mode byte at $7FD5.
    let mut bytes = std::fs::read(&sfc).unwrap();
    bytes[0x7FD5] = 0x00;
    std::fs::write(&sfc, &bytes).unwrap();

    let report = dir.path().join("struct.json");
    let out = Command::new(bin())
        .args(["verify-sfc-structure", "--sfc"])
        .arg(&sfc)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v = read_json(&report);
    assert_eq!(v["status"], "fail");
}

#[test]
fn cli_verify_sfc_modules_audible_two_distinct_modules() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved || !oracle_resolved_for_test() {
        eprintln!("skip: asar / oracle not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj_a = make_project_with_one_imported_sample(dir.path(), "audible_a");

    // Project B with a lower-frequency sine for distinct rendering.
    let proj_b = dir.path().join("audible_b.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj_b)
        .args(["--name", "audible_b"])
        .output()
        .unwrap();
    let audio_b = dir.path().join("low.wav");
    let pcm_b = synth_sine_pcm(4096, 128.0, 6000.0);
    write_pcm16_wav_with_samples(&audio_b, 22050, 1, &pcm_b);
    Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj_b)
        .args(["--audio"])
        .arg(&audio_b)
        .output()
        .unwrap();

    let sfc = dir.path().join("two.sfc");
    Command::new(bin())
        .args(["compile-sfc", "--project-a"])
        .arg(&proj_a)
        .args(["--project-b"])
        .arg(&proj_b)
        .args(["--out-sfc"])
        .arg(&sfc)
        .args(["--out-report"])
        .arg(dir.path().join("c.json"))
        .output()
        .unwrap();

    let oracle = oracle_wrapper_path();
    let report = dir.path().join("audible.json");
    let out = Command::new(bin())
        .args(["verify-sfc-modules-audible", "--sfc"])
        .arg(&sfc)
        .args(["--frames", "8192"])
        .args(["--out-report"])
        .arg(&report)
        .args(["--oracle"])
        .arg(&oracle)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = read_json(&report);
    assert_envelope(&v, "sfc_modules_audible");
    assert_eq!(v["status"], "ok");
    assert_eq!(v["modules_audio_identical"], false);
    assert_ne!(
        v["module_a_audible"]["spc_sha256"],
        v["module_b_audible"]["spc_sha256"]
    );
}

#[test]
fn cli_verify_sfc_modules_audible_silent_module_fails() {
    if !oracle_resolved_for_test() {
        eprintln!("skip: oracle not resolved");
        return;
    }
    // Hand-build a minimal SFC: 256 KB of zeros with a fake module
    // header at MODULE_A_FILE_OFFSET that has no blocks (so the
    // projected ARAM is empty, the SPC is muted by default, and
    // the oracle renders silence).
    let dir = TempDir::new().unwrap();
    let mut bytes = vec![0u8; 256 * 1024];
    // Title + mode + checksum so verify-sfc-structure would mostly
    // pass — but we're testing modules_audible not structure.
    let title = b"MUTED FIXTURE        ";
    bytes[0x7FC0..0x7FC0 + 21].copy_from_slice(title);
    bytes[0x7FD5] = 0x20;
    bytes[0x7FD9] = 0x01;
    // Reset vector pointing somewhere reasonable.
    bytes[0x7FFC..0x7FFE].copy_from_slice(&0x8000u16.to_le_bytes());

    // Embed a zero-block module at $8000.
    let module = sfc_atomizer_core::module_writer::write_module(
        sfc_atomizer_core::module_writer::ModuleWriteInput {
            aram_image: &[0u8; 0x10000],
            map_report: &{
                use sfc_atomizer_core::report::*;
                AramMapReport {
                    schema_version: SCHEMA_VERSION,
                    report_type: AramMapReport::REPORT_TYPE.to_string(),
                    total_aram: 0x10000,
                    regions: vec![AramRegion {
                        name: "driver_code".to_string(),
                        start: "0x0200".to_string(),
                        end: "0x020F".to_string(),
                        bytes: 16,
                        kind: AramKind::DriverCode,
                    }],
                    free_bytes: 0,
                    collisions: Vec::new(),
                    echo: None,
                    source_directory: None,
                    samples: None,
                    atoms: None,
                    warnings: Vec::new(),
                }
            },
            echo_enabled: false,
        },
    )
    .unwrap();
    bytes[0x8000..0x8000 + module.bytes.len()].copy_from_slice(&module.bytes);
    // Embed the same muted module at $10000 so module B parses
    // correctly; otherwise the verify path returns oracle_error.
    bytes[0x10000..0x10000 + module.bytes.len()].copy_from_slice(&module.bytes);

    let sfc = dir.path().join("muted.sfc");
    std::fs::write(&sfc, &bytes).unwrap();

    let oracle = oracle_wrapper_path();
    let report = dir.path().join("audible.json");
    let out = Command::new(bin())
        .args(["verify-sfc-modules-audible", "--sfc"])
        .arg(&sfc)
        .args(["--frames", "4096"])
        .args(["--out-report"])
        .arg(&report)
        .args(["--oracle"])
        .arg(&oracle)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "expected silent_fail exit 2");
    let v = read_json(&report);
    assert_eq!(v["status"], "silent_fail");
}

// =============================================================================
// M1.5 — compile-spc / verify-spc-audible
// =============================================================================

#[test]
fn cli_compile_spc_happy_path() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip cli_compile_spc_happy_path: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "compile_demo");

    let spc = dir.path().join("demo.spc");
    let image = dir.path().join("demo.aram.bin");
    let map = dir.path().join("demo.aram-map.json");
    let report = dir.path().join("demo.compile-report.json");

    let out = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&proj)
        .args(["--out-spc"])
        .arg(&spc)
        .args(["--out-image"])
        .arg(&image)
        .args(["--out-map"])
        .arg(&map)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let spc_bytes = std::fs::read(&spc).unwrap();
    assert_eq!(spc_bytes.len(), 0x10200);
    let aram_bytes = std::fs::read(&image).unwrap();
    assert_eq!(aram_bytes.len(), 0x10000);
    // Driver bytes start at offset $0200 — first instruction is
    // `mov $f2, #$6c` ⇒ $8F $6C $F2.
    assert_eq!(&aram_bytes[0x200..0x203], &[0x8F, 0x6C, 0xF2]);
    let v = read_json(&report);
    assert_envelope(&v, "compile_spc");
    let driver_bytes = v["driver_code_bytes"].as_u64().unwrap();
    assert!((100..=4096).contains(&driver_bytes));
}

#[test]
fn cli_compile_spc_invalid_project_returns_2() {
    if !sfc_atomizer_core::tools::resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("bad.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", "bad"])
        .output()
        .unwrap();
    let out = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&proj)
        .args(["--out-spc"])
        .arg(dir.path().join("x.spc"))
        .args(["--out-image"])
        .arg(dir.path().join("x.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("x.json"))
        .args(["--out-report"])
        .arg(dir.path().join("x.report.json"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn cli_verify_spc_audible_oracle_resolved() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved || !oracle_resolved_for_test() {
        eprintln!(
            "skip cli_verify_spc_audible_oracle_resolved: asar={} oracle={}",
            resolve_asar().resolved,
            oracle_resolved_for_test()
        );
        return;
    }
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "audible_demo");
    let spc = dir.path().join("audible.spc");

    // Compile.
    let compile = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&proj)
        .args(["--out-spc"])
        .arg(&spc)
        .args(["--out-image"])
        .arg(dir.path().join("a.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("a.map.json"))
        .args(["--out-report"])
        .arg(dir.path().join("a.compile.json"))
        .output()
        .unwrap();
    assert_eq!(compile.status.code(), Some(0), "{:?}", compile);

    // Verify.
    let report = dir.path().join("audible-report.json");
    let pcm = dir.path().join("audible.pcm");
    let oracle = oracle_wrapper_path();
    let out = Command::new(bin())
        .args(["verify-spc-audible", "--spc"])
        .arg(&spc)
        .args(["--frames", "8192"])
        .args(["--out-report"])
        .arg(&report)
        .args(["--out-pcm"])
        .arg(&pcm)
        .args(["--oracle"])
        .arg(&oracle)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = read_json(&report);
    assert_envelope(&v, "audible_verification");
    assert_eq!(v["status"], "ok");
    let max_abs = v["observed"]["max_abs"].as_u64().unwrap();
    assert!(max_abs >= 1000, "max_abs {max_abs} below threshold");
}

#[test]
fn cli_verify_spc_audible_silent_fixture_fails() {
    if !oracle_resolved_for_test() {
        eprintln!("skip: oracle not resolved");
        return;
    }
    // Build the M0 muted smoke .spc by writing a 66048-byte buffer
    // with only the magic + the M0 contract bytes set. Easier: we
    // know the m0-acceptance test produces it, so reuse that path.
    let dir = TempDir::new().unwrap();
    // Instead of running m0-acceptance (heavy), construct a muted
    // SPC inline: build_smoke_image needs a 64KB ARAM blob; use
    // all zeros (driver doesn't run audibly with FLG=$60 mute).
    use sfc_atomizer_core::spc::{build_smoke_image, SPC_FILE_SIZE};
    let aram = vec![0u8; 0x10000];
    let img = build_smoke_image(aram).unwrap();
    let spc_bytes = img.to_bytes().unwrap();
    assert_eq!(spc_bytes.len(), SPC_FILE_SIZE);
    let spc = dir.path().join("muted.spc");
    std::fs::write(&spc, &spc_bytes).unwrap();

    let report = dir.path().join("muted-report.json");
    let pcm = dir.path().join("muted.pcm");
    let oracle = oracle_wrapper_path();
    let out = Command::new(bin())
        .args(["verify-spc-audible", "--spc"])
        .arg(&spc)
        .args(["--frames", "4096"])
        .args(["--out-report"])
        .arg(&report)
        .args(["--out-pcm"])
        .arg(&pcm)
        .args(["--oracle"])
        .arg(&oracle)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "expected silent_fail exit 2");
    let v = read_json(&report);
    assert_eq!(v["status"], "silent_fail");
}

#[test]
fn cli_verify_spc_audible_oracle_missing_returns_1() {
    let dir = TempDir::new().unwrap();
    let bogus_spc = dir.path().join("nope.spc");
    // Force oracle missing via a definitely-not-an-executable env
    // pointing at a non-existent file.
    let report = dir.path().join("r.json");
    let pcm = dir.path().join("p.pcm");
    let bogus_oracle = dir.path().join("nonexistent_oracle.exe");
    let out = Command::new(bin())
        .env("SFCWC_SNES_SPC_ORACLE", &bogus_oracle)
        .args(["verify-spc-audible", "--spc"])
        .arg(&bogus_spc)
        .args(["--out-report"])
        .arg(&report)
        .args(["--out-pcm"])
        .arg(&pcm)
        // Explicitly DON'T pass --oracle so the resolver path runs.
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

// =============================================================================
// M1.4 — pack
// =============================================================================

fn make_project_with_one_imported_sample(dir: &Path, name: &str) -> PathBuf {
    let proj = dir.join(format!("{name}.sfcproj.json"));
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", name])
        .output()
        .unwrap();
    let audio = dir.join(format!("{name}.wav"));
    let pcm = synth_sine_pcm(256, 64.0, 8000.0);
    write_pcm16_wav_with_samples(&audio, 32_000, 1, &pcm);
    let imp = Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj)
        .args(["--audio"])
        .arg(&audio)
        .output()
        .unwrap();
    assert_eq!(imp.status.code(), Some(0), "{:?}", imp);
    proj
}

#[test]
fn cli_pack_happy_path_single_sample() {
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "single");
    let image = dir.path().join("aram.bin");
    let map = dir.path().join("aram-map.json");

    let out = Command::new(bin())
        .args(["pack", "--project"])
        .arg(&proj)
        .args(["--out-image"])
        .arg(&image)
        .args(["--out-map"])
        .arg(&map)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&image).unwrap();
    assert_eq!(bytes.len(), 65536);

    let v = read_json(&map);
    assert_envelope(&v, "aram_map");
    let regions = v["regions"].as_array().unwrap();
    let region_names: Vec<&str> = regions.iter().filter_map(|r| r["name"].as_str()).collect();
    assert!(region_names.contains(&"driver_code"));
    assert!(region_names.contains(&"source_directory"));
    assert!(region_names.contains(&"sample_brr_pool"));
    assert!(region_names.contains(&"ipl_rom_shadow"));
    assert!(v["echo"]["enabled"] == false);
    assert_eq!(v["samples"]["total_samples"], 1);
    assert!(v["source_directory"]["start_addr"].as_u64().unwrap() == 0x1200);
}

#[test]
fn cli_pack_with_echo_overflow_returns_3() {
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "overflow");
    // Mutate the master_echo block to enabled=true / edl=15 via
    // serde_json so we don't accidentally flip the sample's
    // looped.enabled with a string replace.
    let text = std::fs::read_to_string(&proj).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["master_echo"]["enabled"] = serde_json::json!(true);
    v["master_echo"]["edl"] = serde_json::json!(15);
    std::fs::write(&proj, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    // EDL=15 echo eats 30 KB. Driver 4 KB + srcdir page = ~4.25 KB.
    // Available pool ≈ 29.7 KB. Add a sample big enough to push past
    // that on its own (65K frames → 4063 BRR blocks → 36567 B).
    let big = dir.path().join("huge.wav");
    let huge_pcm = synth_sine_pcm(65_000, 64.0, 8000.0);
    write_pcm16_wav_with_samples(&big, 32_000, 1, &huge_pcm);
    let imp = Command::new(bin())
        .args(["import", "--project"])
        .arg(&proj)
        .args(["--audio"])
        .arg(&big)
        .output()
        .unwrap();
    assert_eq!(imp.status.code(), Some(0));

    let out = Command::new(bin())
        .args(["pack", "--project"])
        .arg(&proj)
        .args(["--out-image"])
        .arg(dir.path().join("aram.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("aram-map.json"))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_pack_invalid_project_returns_2() {
    let dir = TempDir::new().unwrap();
    let proj = dir.path().join("bad.sfcproj.json");
    Command::new(bin())
        .args(["new-project", "--out"])
        .arg(&proj)
        .args(["--name", "bad"])
        .output()
        .unwrap();
    // Template has empty sample_pool → fails validation rule #9.
    let out = Command::new(bin())
        .args(["pack", "--project"])
        .arg(&proj)
        .args(["--out-image"])
        .arg(dir.path().join("aram.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("aram-map.json"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn cli_pack_writes_image_and_map_at_default_paths() {
    let dir = TempDir::new().unwrap();
    let proj = make_project_with_one_imported_sample(dir.path(), "defaults");

    // Run pack with no --out-image / --out-map; defaults land under
    // build/m1/. Override CWD so we don't pollute the workspace.
    let out = Command::new(bin())
        .args(["pack", "--project"])
        .arg(&proj)
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{:?}", out);
    assert!(dir.path().join("build/m1/aram-image.bin").exists());
    assert!(dir.path().join("build/m1/aram-map.json").exists());
}

#[test]
fn find_loop_candidates_writes_report() {
    let dir = TempDir::new().unwrap();
    let wav = dir.path().join("sine.wav");
    let report = dir.path().join("loops.json");
    let pcm = synth_sine_pcm(2048, 64.0, 8000.0);
    write_pcm16_wav_with_samples(&wav, 32000, 1, &pcm);

    let out = Command::new(bin())
        .args(["find-loop-candidates", "--audio"])
        .arg(&wav)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .expect("run sfcwc");
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&report);
    assert_envelope(&v, "loop_finder");
    let cands = v["candidates"].as_array().expect("candidates array");
    assert!(!cands.is_empty(), "expected ≥1 candidate for periodic sine");
    for c in cands {
        let start = c["start_sample"].as_u64().unwrap();
        let end = c["end_sample"].as_u64().unwrap();
        assert_eq!(start % 16, 0);
        assert_eq!(end % 16, 0);
    }
}

// =============================================================================
// M2.1 — migrate-project (SPEC §16.10)
// =============================================================================

#[test]
fn cli_migrate_project_happy_path() {
    let dir = TempDir::new().unwrap();
    let v1 = make_project_with_one_imported_sample(dir.path(), "v1mig");
    let v2 = dir.path().join("v1mig.v2.json");

    let out = Command::new(bin())
        .args(["migrate-project", "--in"])
        .arg(&v1)
        .args(["--out"])
        .arg(&v2)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let migrated = read_json(&v2);
    assert_eq!(migrated["schema_version"], 2);
    assert!(migrated["m1"].is_null());
    assert_eq!(migrated["m2"]["active_sequence_id"], Value::Null);
    assert!(migrated["atom_pool"].as_array().unwrap().is_empty());
    assert!(migrated["atom_sequences"].as_array().unwrap().is_empty());
    let tracks = migrated["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0]["id"], "track_sample_0");
    assert_eq!(tracks[0]["voice"], 0);
    assert_eq!(tracks[0]["kind"], "sample_sustain");

    // Default migration report path lives next to --out.
    let report = dir.path().join("v1mig.v2.migration-report.json");
    let r = read_json(&report);
    assert_eq!(r["schema_version"], 1);
    assert_eq!(r["report_type"], "migration_v1_to_v2");
    assert_eq!(r["source_schema_version"], 1);
    assert_eq!(r["target_schema_version"], 2);
    let xs = r["transformations"].as_array().unwrap();
    let paths: Vec<&str> = xs.iter().map(|t| t["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"/m1"));
    assert!(paths.contains(&"/atom_pool"));
    assert!(paths.contains(&"/m2"));
    assert!(paths.contains(&"/tracks"));
}

#[test]
fn cli_migrate_project_already_v2_exits_2() {
    let dir = TempDir::new().unwrap();
    let v2_in = dir.path().join("already.v2.json");
    let v2 = serde_json::json!({
        "schema_version": 2,
        "project": { "name": "already", "tick_rate_hz": 60 },
        "driver": { "profile": "sample_basic", "bytecode_version": 1 },
        "master_echo": {
            "enabled": false, "edl": 0, "efb": 0,
            "evol_l": 0, "evol_r": 0, "fir": [127, 0, 0, 0, 0, 0, 0, 0]
        },
        "sample_pool": [],
        "atom_pool": [],
        "atom_sequences": [],
        "tracks": [],
        "m2": { "active_sequence_id": null }
    });
    std::fs::write(&v2_in, serde_json::to_string(&v2).unwrap()).unwrap();
    let v2_out = dir.path().join("ignored.v2.json");

    let out = Command::new(bin())
        .args(["migrate-project", "--in"])
        .arg(&v2_in)
        .args(["--out"])
        .arg(&v2_out)
        .output()
        .expect("run sfcwc");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("already at schema_version 2"),
        "expected explicit already-v2 error; got: {stderr}"
    );
    assert!(
        !v2_out.exists(),
        "must not write output for already-v2 input"
    );
}

#[test]
fn cli_migrate_project_corrupted_v1_exits_2() {
    let dir = TempDir::new().unwrap();
    let v1 = dir.path().join("corrupt.json");
    // Valid JSON, valid v1 envelope, but fails v1 validation
    // (invalid driver profile, empty sample_pool, missing
    // active_sample_id).
    let bad = serde_json::json!({
        "schema_version": 1,
        "project": { "name": "bad", "tick_rate_hz": 60 },
        "driver": { "profile": "synth_static", "bytecode_version": 1 },
        "master_echo": {
            "enabled": false, "edl": 0, "efb": 0,
            "evol_l": 0, "evol_r": 0, "fir": [127, 0, 0, 0, 0, 0, 0, 0]
        },
        "sample_pool": [],
        "m1": { "active_sample_id": "" }
    });
    std::fs::write(&v1, serde_json::to_string(&bad).unwrap()).unwrap();
    let v2 = dir.path().join("corrupt.v2.json");

    let out = Command::new(bin())
        .args(["migrate-project", "--in"])
        .arg(&v1)
        .args(["--out"])
        .arg(&v2)
        .output()
        .expect("run sfcwc");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !v2.exists(),
        "must not write output when v1 fails validation"
    );
}

#[test]
fn cli_migrate_project_then_compile_spc_matches_v1_baseline() {
    use sfc_atomizer_core::tools::resolve_asar;
    if !resolve_asar().resolved {
        eprintln!("skip: asar not resolved");
        return;
    }
    let dir = TempDir::new().unwrap();
    let v1 = make_project_with_one_imported_sample(dir.path(), "baseline");

    // Compile from v1 directly.
    let v1_image = dir.path().join("v1.aram.bin");
    let v1_spc = dir.path().join("v1.spc");
    let out_v1 = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&v1)
        .args(["--out-spc"])
        .arg(&v1_spc)
        .args(["--out-image"])
        .arg(&v1_image)
        .args(["--out-map"])
        .arg(dir.path().join("v1.map.json"))
        .args(["--out-report"])
        .arg(dir.path().join("v1.compile.json"))
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out_v1.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out_v1.stderr)
    );

    // Migrate v1 -> v2.
    let v2 = dir.path().join("baseline.v2.json");
    let mig = Command::new(bin())
        .args(["migrate-project", "--in"])
        .arg(&v1)
        .args(["--out"])
        .arg(&v2)
        .output()
        .expect("run sfcwc");
    assert_eq!(mig.status.code(), Some(0), "{:?}", mig);

    // Compile from v2.
    let v2_image = dir.path().join("v2.aram.bin");
    let v2_spc = dir.path().join("v2.spc");
    let out_v2 = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&v2)
        .args(["--out-spc"])
        .arg(&v2_spc)
        .args(["--out-image"])
        .arg(&v2_image)
        .args(["--out-map"])
        .arg(dir.path().join("v2.map.json"))
        .args(["--out-report"])
        .arg(dir.path().join("v2.compile.json"))
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out_v2.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out_v2.stderr)
    );

    // Migration must preserve audio behaviour bit-identically.
    let v1_image_bytes = std::fs::read(&v1_image).unwrap();
    let v2_image_bytes = std::fs::read(&v2_image).unwrap();
    assert_eq!(
        v1_image_bytes, v2_image_bytes,
        "ARAM image must be bit-identical across migration"
    );
    let v1_spc_bytes = std::fs::read(&v1_spc).unwrap();
    let v2_spc_bytes = std::fs::read(&v2_spc).unwrap();
    assert_eq!(
        v1_spc_bytes, v2_spc_bytes,
        "SPC file must be bit-identical across migration"
    );
}

#[test]
fn cli_compile_spc_on_v2_with_atoms_errors_with_m25_pending() {
    let dir = TempDir::new().unwrap();
    let v2_path = dir.path().join("v2_atoms.json");
    let v2 = serde_json::json!({
        "schema_version": 2,
        "project": { "name": "atomic", "tick_rate_hz": 60 },
        "driver": { "profile": "multi_voice_atom", "bytecode_version": 2 },
        "master_echo": {
            "enabled": false, "edl": 0, "efb": 0,
            "evol_l": 0, "evol_r": 0, "fir": [127, 0, 0, 0, 0, 0, 0, 0]
        },
        "sample_pool": [],
        "atom_pool": [{
            "id": "atom_0001", "name": "sine_128",
            "kind": "additive_single_cycle_v0",
            "root_midi_note": 60,
            "cycle_len_samples": 128,
            "amplitude": 0.75,
            "partials": [{ "harmonic": 1, "amplitude": 1.0, "phase_cycles": 0.0 }],
            "render": {
                "normalize": true,
                "force_filter_0_first_block": true,
                "force_filter_0_loop_entry": true
            },
            "playback": {
                "volume": 0.8, "pan": 0.0, "echo": false,
                "envelope": { "type": "gain_raw", "gain_byte": 127 }
            }
        }],
        "atom_sequences": [{
            "id": "atomseq_0001", "name": "single",
            "voice": 1,
            "steps": [{
                "atom_id": "atom_0001",
                "duration_ticks": 120,
                "target_volume": 0.8,
                "transition": { "type": "initial_kon" }
            }],
            "loop": false
        }],
        "tracks": [{
            "id": "track_atom_1",
            "voice": 1,
            "kind": "atom_sequence",
            "atom_sequence_id": "atomseq_0001"
        }],
        "m2": { "active_sequence_id": "atomseq_0001" }
    });
    std::fs::write(&v2_path, serde_json::to_string(&v2).unwrap()).unwrap();

    let out = Command::new(bin())
        .args(["compile-spc", "--project"])
        .arg(&v2_path)
        .args(["--out-spc"])
        .arg(dir.path().join("a.spc"))
        .args(["--out-image"])
        .arg(dir.path().join("a.bin"))
        .args(["--out-map"])
        .arg(dir.path().join("a.map.json"))
        .output()
        .expect("run sfcwc");
    assert_ne!(out.status.code(), Some(0), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("M2.5") || stderr.contains("atom") || stderr.contains("v2 project"),
        "expected M2.5-pending or atom-related error; got: {stderr}"
    );
}

// =============================================================================
// M2.2 — render-atom / preview-atom (SPEC §16.9)
// =============================================================================

/// Build a v2 project with two sine atoms (cycle 64 + cycle 128).
/// Both validate cleanly and exercise the M2.2 render path.
fn write_v2_project_with_two_sine_atoms(dir: &Path) -> PathBuf {
    let path = dir.join("atoms.v2.json");
    let v2 = serde_json::json!({
        "schema_version": 2,
        "project": { "name": "atomic", "tick_rate_hz": 60 },
        "driver": { "profile": "multi_voice_atom", "bytecode_version": 2 },
        "master_echo": {
            "enabled": false, "edl": 0, "efb": 0,
            "evol_l": 0, "evol_r": 0, "fir": [127, 0, 0, 0, 0, 0, 0, 0]
        },
        "sample_pool": [{
            "id": "lead", "name": "lead",
            "source": {
                "path": "audio/lead.wav",
                "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "format": "wav", "sample_rate_hz": 32000,
                "channels": 1, "frames": 4096
            },
            "root_midi_note": 60,
            "loop": { "enabled": false },
            "playback": {
                "volume": 1.0, "pan": 0.0, "echo": false,
                "envelope": { "type": "gain_raw", "gain_byte": 127 }
            }
        }],
        "atom_pool": [
            {
                "id": "sine_64", "name": "sine_64",
                "kind": "additive_single_cycle_v0",
                "root_midi_note": 60, "cycle_len_samples": 64, "amplitude": 0.75,
                "partials": [{ "harmonic": 1, "amplitude": 1.0, "phase_cycles": 0.0 }],
                "render": { "normalize": true, "force_filter_0_first_block": true,
                            "force_filter_0_loop_entry": true },
                "playback": {
                    "volume": 1.0, "pan": 0.0, "echo": false,
                    "envelope": { "type": "gain_raw", "gain_byte": 127 }
                }
            },
            {
                "id": "sine_128", "name": "sine_128",
                "kind": "additive_single_cycle_v0",
                "root_midi_note": 60, "cycle_len_samples": 128, "amplitude": 0.75,
                "partials": [{ "harmonic": 1, "amplitude": 1.0, "phase_cycles": 0.0 }],
                "render": { "normalize": true, "force_filter_0_first_block": true,
                            "force_filter_0_loop_entry": true },
                "playback": {
                    "volume": 1.0, "pan": 0.0, "echo": false,
                    "envelope": { "type": "gain_raw", "gain_byte": 127 }
                }
            }
        ],
        "atom_sequences": [{
            "id": "atomseq_0001", "name": "single", "voice": 1,
            "steps": [{
                "atom_id": "sine_128", "duration_ticks": 120, "target_volume": 0.8,
                "transition": { "type": "initial_kon" }
            }],
            "loop": false
        }],
        "tracks": [
            { "id": "track_sample_0", "voice": 0, "kind": "sample_sustain",
              "sample_id": "lead" },
            { "id": "track_atom_1", "voice": 1, "kind": "atom_sequence",
              "atom_sequence_id": "atomseq_0001" }
        ],
        "m2": { "active_sequence_id": "atomseq_0001" }
    });
    std::fs::write(&path, serde_json::to_string_pretty(&v2).unwrap()).unwrap();
    path
}

#[test]
fn cli_render_atom_happy_path() {
    let dir = TempDir::new().unwrap();
    let project = write_v2_project_with_two_sine_atoms(dir.path());
    let brr = dir.path().join("sine_128.brr");
    let report = dir.path().join("sine_128.report.json");

    let out = Command::new(bin())
        .args(["render-atom", "--project"])
        .arg(&project)
        .args(["--atom", "sine_128"])
        .args(["--out-brr"])
        .arg(&brr)
        .args(["--out-report"])
        .arg(&report)
        .output()
        .expect("run sfcwc");
    assert_eq!(out.status.code(), Some(0), "{:?}", out);

    let bytes = std::fs::read(&brr).unwrap();
    assert_eq!(bytes.len(), 72, "cycle=128 expects 72 BRR bytes");

    let r = read_json(&report);
    assert_envelope(&r, "atom_render");
    assert_eq!(r["atom_id"], "sine_128");
    assert_eq!(r["atom_name"], "sine_128");
    assert_eq!(r["atom_kind"], "additive_single_cycle_v0");
    assert_eq!(r["cycle_len_samples"], 128);
    assert_eq!(r["partial_count"], 1);
    assert_eq!(r["normalize"], true);
    assert_eq!(r["brr_bytes"], 72);
    // Locked M2_ATOM_128_SINE_BRR_SHA256 baseline.
    assert_eq!(
        r["brr_sha256"],
        "348c791449916e1f9169d0e229cd79bf97967b19e22db3c4a5be7dc9c69ac876"
    );
}

#[test]
fn cli_render_atom_deterministic_across_runs() {
    let dir = TempDir::new().unwrap();
    let project = write_v2_project_with_two_sine_atoms(dir.path());
    let brr_a = dir.path().join("a.brr");
    let brr_b = dir.path().join("b.brr");

    for out_path in [&brr_a, &brr_b] {
        let out = Command::new(bin())
            .args(["render-atom", "--project"])
            .arg(&project)
            .args(["--atom", "sine_128"])
            .args(["--out-brr"])
            .arg(out_path)
            .args(["--out-report"])
            .arg(out_path.with_extension("report.json"))
            .output()
            .expect("run sfcwc");
        assert_eq!(out.status.code(), Some(0));
    }
    let a = std::fs::read(&brr_a).unwrap();
    let b = std::fs::read(&brr_b).unwrap();
    assert_eq!(a, b, "render-atom must be deterministic across runs");
}

#[test]
fn cli_render_atom_64_and_128_distinct_brr() {
    let dir = TempDir::new().unwrap();
    let project = write_v2_project_with_two_sine_atoms(dir.path());

    let brr_64 = dir.path().join("sine_64.brr");
    let r64 = Command::new(bin())
        .args(["render-atom", "--project"])
        .arg(&project)
        .args(["--atom", "sine_64", "--out-brr"])
        .arg(&brr_64)
        .args(["--out-report"])
        .arg(dir.path().join("sine_64.report.json"))
        .output()
        .expect("run sfcwc");
    assert_eq!(r64.status.code(), Some(0));

    let brr_128 = dir.path().join("sine_128.brr");
    let r128 = Command::new(bin())
        .args(["render-atom", "--project"])
        .arg(&project)
        .args(["--atom", "sine_128", "--out-brr"])
        .arg(&brr_128)
        .args(["--out-report"])
        .arg(dir.path().join("sine_128.report.json"))
        .output()
        .expect("run sfcwc");
    assert_eq!(r128.status.code(), Some(0));

    let bytes_64 = std::fs::read(&brr_64).unwrap();
    let bytes_128 = std::fs::read(&brr_128).unwrap();
    assert_eq!(bytes_64.len(), 36);
    assert_eq!(bytes_128.len(), 72);
    assert_ne!(bytes_64, bytes_128);
}

#[test]
fn cli_render_atom_v1_project_errors_with_migrate_hint() {
    let dir = TempDir::new().unwrap();
    let v1 = make_project_with_one_imported_sample(dir.path(), "v1only");
    let out = Command::new(bin())
        .args(["render-atom", "--project"])
        .arg(&v1)
        .args(["--atom", "anything"])
        .output()
        .expect("run sfcwc");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("v1 project")
            || stderr.contains("migrate-project")
            || stderr.contains("requires v2"),
        "expected migrate hint; got: {stderr}"
    );
}

#[test]
fn cli_render_atom_missing_atom_id_lists_available() {
    let dir = TempDir::new().unwrap();
    let project = write_v2_project_with_two_sine_atoms(dir.path());
    let out = Command::new(bin())
        .args(["render-atom", "--project"])
        .arg(&project)
        .args(["--atom", "ghost_atom"])
        .output()
        .expect("run sfcwc");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ghost_atom"),
        "expected missing-id message; got: {stderr}"
    );
    assert!(
        stderr.contains("sine_64") && stderr.contains("sine_128"),
        "expected available atom ids in stderr; got: {stderr}"
    );
}

// =============================================================================
// M2.3 — pack v2 multi_voice_atom + capability manifest sidecar
// =============================================================================

#[test]
fn cli_pack_v2_multi_voice_emits_capability_manifest() {
    let dir = TempDir::new().unwrap();
    let project = write_v2_project_with_two_sine_atoms(dir.path());
    // Provide a real audio file for the sample (the v2 fixture
    // declares sample-pool[0].path = audio/lead.wav; create it).
    let audio_dir = dir.path().join("audio");
    std::fs::create_dir_all(&audio_dir).unwrap();
    let audio = audio_dir.join("lead.wav");
    let pcm = synth_sine_pcm(256, 64.0, 8000.0);
    write_pcm16_wav_with_samples(&audio, 32_000, 1, &pcm);
    // Update sample SHA in the project to match the just-written WAV.
    let sha = sfc_atomizer_core::asm::sha256_hex_file(&audio).unwrap();
    let text = std::fs::read_to_string(&project).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["sample_pool"][0]["source"]["sha256"] = serde_json::json!(sha);
    // Update frame count too (synth_sine_pcm has 256 frames).
    v["sample_pool"][0]["source"]["frames"] = serde_json::json!(256);
    std::fs::write(&project, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let image = dir.path().join("aram.bin");
    let map = dir.path().join("aram-map.json");
    let manifest = dir.path().join("capability-manifest.json");

    let out = Command::new(bin())
        .args(["pack", "--project"])
        .arg(&project)
        .args(["--out-image"])
        .arg(&image)
        .args(["--out-map"])
        .arg(&map)
        .args(["--out-capability-manifest"])
        .arg(&manifest)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Image is 64 KB.
    let img = std::fs::read(&image).unwrap();
    assert_eq!(img.len(), 65536);

    // Map report has atoms summary.
    let m = read_json(&map);
    assert_envelope(&m, "aram_map");
    assert!(m["atoms"].is_object(), "expected atoms summary in map");
    assert_eq!(m["atoms"]["total_atoms"], 2);
    let region_names: Vec<&str> = m["regions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    assert!(region_names.contains(&"synth_atom_pool"));
    assert!(region_names.contains(&"voice_setup_table"));

    // Capability manifest sidecar.
    let cap = read_json(&manifest);
    assert_envelope(&cap, "capability_manifest");
    assert_eq!(cap["driver_profile"], "multi_voice_atom");
    assert_eq!(cap["driver_version"], 2);
    assert_eq!(cap["bytecode_version"], 2);
    assert_eq!(cap["limits"]["max_music_voices"], 2);
    assert_eq!(cap["limits"]["max_atom_sources"], 32);
    assert_eq!(cap["features"]["synth_static_atom"], true);
    assert_eq!(cap["features"]["synth_atom_sequence"], true);
    assert_eq!(cap["features"]["multi_voice_playback"], true);
}

#[test]
fn cli_pack_v2_sample_only_emits_sample_basic_manifest() {
    let dir = TempDir::new().unwrap();
    // v1 project, then migrate to v2-sample-only-equivalent.
    let v1 = make_project_with_one_imported_sample(dir.path(), "sb_v2");
    let v2 = dir.path().join("sb_v2.v2.json");
    let mig = Command::new(bin())
        .args(["migrate-project", "--in"])
        .arg(&v1)
        .args(["--out"])
        .arg(&v2)
        .output()
        .unwrap();
    assert_eq!(mig.status.code(), Some(0));

    let image = dir.path().join("aram.bin");
    let map = dir.path().join("aram-map.json");
    let manifest = dir.path().join("capability-manifest.json");

    let out = Command::new(bin())
        .args(["pack", "--project"])
        .arg(&v2)
        .args(["--out-image"])
        .arg(&image)
        .args(["--out-map"])
        .arg(&map)
        .args(["--out-capability-manifest"])
        .arg(&manifest)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let cap = read_json(&manifest);
    assert_envelope(&cap, "capability_manifest");
    assert_eq!(cap["driver_profile"], "sample_basic");
    assert_eq!(cap["driver_version"], 1);
    assert_eq!(cap["bytecode_version"], 1);
    assert_eq!(cap["limits"]["max_music_voices"], 1);
    assert_eq!(cap["limits"]["max_atom_sources"], 0);
    // Atom features must NOT be present in sample_basic.
    assert!(cap["features"].get("synth_static_atom").is_none());
    assert!(cap["features"].get("multi_voice_playback").is_none());
}

#[test]
fn cli_pack_v2_sample_only_matches_v1_aram_sha() {
    // M2.1 bit-identity guarantee carried into M2.3: pack on a
    // sample-only-equivalent v2 produces the same ARAM bytes as
    // pack on the original v1.
    let dir = TempDir::new().unwrap();
    let v1 = make_project_with_one_imported_sample(dir.path(), "iso");
    let v2 = dir.path().join("iso.v2.json");
    Command::new(bin())
        .args(["migrate-project", "--in"])
        .arg(&v1)
        .args(["--out"])
        .arg(&v2)
        .output()
        .unwrap();

    let v1_image = dir.path().join("v1.bin");
    let v1_map = dir.path().join("v1.map.json");
    Command::new(bin())
        .args(["pack", "--project"])
        .arg(&v1)
        .args(["--out-image"])
        .arg(&v1_image)
        .args(["--out-map"])
        .arg(&v1_map)
        .output()
        .unwrap();
    let v2_image = dir.path().join("v2.bin");
    let v2_map = dir.path().join("v2.map.json");
    Command::new(bin())
        .args(["pack", "--project"])
        .arg(&v2)
        .args(["--out-image"])
        .arg(&v2_image)
        .args(["--out-map"])
        .arg(&v2_map)
        .output()
        .unwrap();

    let a = std::fs::read(&v1_image).unwrap();
    let b = std::fs::read(&v2_image).unwrap();
    assert_eq!(a, b, "v2-sample-only ARAM must match v1 ARAM byte-for-byte");
}

#[test]
fn cli_preview_atom_writes_audition_wav() {
    let dir = TempDir::new().unwrap();
    let project = write_v2_project_with_two_sine_atoms(dir.path());
    let wav = dir.path().join("preview.wav");
    let out = Command::new(bin())
        .args(["preview-atom", "--project"])
        .arg(&project)
        .args(["--atom", "sine_128"])
        .args(["--duration-seconds", "1.0"])
        .args(["--out-wav"])
        .arg(&wav)
        .output()
        .expect("run sfcwc");
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&wav).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    // 1 second at 32 kHz mono PCM16 = 32000 frames × 2 bytes = 64000
    // payload + 44-byte header = 64044 bytes.
    assert_eq!(bytes.len(), 44 + 32000 * 2);
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    assert_eq!(sample_rate, 32000);
    let channels = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    assert_eq!(channels, 1);
    let bits = u16::from_le_bytes(bytes[34..36].try_into().unwrap());
    assert_eq!(bits, 16);
}
