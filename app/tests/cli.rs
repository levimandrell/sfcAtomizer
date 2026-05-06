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
fn decode_fixtures_writes_stub() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("brr.json");
    let out = run_with_arg_path(&["decode-fixtures"], "--out", &path);
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&path);
    assert_envelope(&v, "brr_fixture");
    assert_eq!(v["fixture_set"], "m0_raw_decode");
    assert_eq!(v["total"], 0);
    assert_eq!(v["passed"], 0);
    assert_eq!(v["failed"], 0);
    assert!(v["results"].as_array().unwrap().is_empty());
}

#[test]
fn assemble_smoke_writes_stub() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("hello.asm");
    std::fs::write(&src, "; placeholder\n").unwrap();
    let path = dir.path().join("assemble.json");
    let out = Command::new(bin())
        .args(["assemble-smoke", "--source"])
        .arg(&src)
        .args(["--out"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&path);
    assert_envelope(&v, "assemble");
    assert_eq!(v["status"], "not_run");
    assert_eq!(v["backend"], "asar");
    assert_eq!(v["input_path"], Value::Null);
}

#[test]
fn export_spc_smoke_writes_stub() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("spc.json");
    let out = run_with_arg_path(&["export-spc-smoke"], "--out", &path);
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&path);
    assert_envelope(&v, "spc_export");
    assert_eq!(v["status"], "not_run");
    assert_eq!(v["verified_structure"], false);
    assert_eq!(v["initial_state"]["pc"], 0);
}

#[test]
fn calibrate_oracle_writes_stub() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("cal.json");
    let out = run_with_arg_path(&["calibrate-oracle"], "--out", &path);
    assert!(out.status.success(), "{:?}", out);
    let v = read_json(&path);
    assert_envelope(&v, "calibration");
    assert_eq!(v["status"], "not_run");
    assert_eq!(v["ci_gate"], false);
    assert_eq!(v["freeze_target"], "M1");
}

#[test]
fn m0_acceptance_writes_all_reports_and_manifest() {
    let dir = TempDir::new().unwrap();
    let out_dir = dir.path().join("m0");
    let out = run_with_arg_path(&["m0-acceptance"], "--out", &out_dir);
    assert!(
        out.status.success(),
        "m0-acceptance failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

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

    let manifest_path = out_dir.join("manifest.json");
    let manifest = read_json(&manifest_path);
    assert_envelope(&manifest, "m0_manifest");
    for field in [
        "doctor_report",
        "brr_fixture_report",
        "aram_map_report",
        "assemble_report",
        "spc_export_report",
        "calibration_report",
    ] {
        assert!(
            manifest[field].is_string(),
            "manifest.{field} should be a string path"
        );
    }
}

#[test]
fn aram_map_in_acceptance_sums_to_total() {
    let dir = TempDir::new().unwrap();
    let out_dir = dir.path().join("m0");
    let out = run_with_arg_path(&["m0-acceptance"], "--out", &out_dir);
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
    assert_eq!(used + free, total);
    assert_eq!(total, 65536);
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
