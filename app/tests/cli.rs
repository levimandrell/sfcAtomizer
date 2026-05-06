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
