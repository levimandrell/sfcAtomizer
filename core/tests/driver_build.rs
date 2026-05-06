//! Integration tests for `core::driver_build` — the full
//! constants → asar invocation → driver-bytes path.
//!
//! Gated on asar resolution. When asar isn't on PATH or
//! `SFCWC_ASAR`, tests skip with a stderr note (same pattern as
//! `app/tests/cli.rs::assemble_smoke_when_asar_resolved`).

use std::path::PathBuf;

use sfc_atomizer_core::driver_build::{
    build, compute_constants, workspace_driver_asm_path, DriverBuildError, DriverBuildInput,
    DRIVER_END_SENTINEL,
};
use sfc_atomizer_core::packer::DRIVER_CODE_BUDGET_M1;
use sfc_atomizer_core::project::{
    Driver, Envelope, M1Block, MasterEcho, Project, ProjectV1, SampleFormat, SampleLoop,
    SamplePlayback, SampleSlot, SampleSource,
};
use sfc_atomizer_core::report::{AramMapReport, AramSourceDirSummary};
use sfc_atomizer_core::tools::resolve_asar;
use tempfile::TempDir;

fn skip_if_no_asar() -> bool {
    if !resolve_asar().resolved {
        eprintln!("SKIP: asar not resolved on this host (set SFCWC_ASAR or put asar on PATH)");
        true
    } else {
        false
    }
}

fn minimal_project() -> ProjectV1 {
    ProjectV1 {
        schema_version: 1,
        project: Project {
            name: "demo".to_string(),
            tick_rate_hz: 60,
        },
        driver: Driver {
            profile: "sample_basic".to_string(),
            bytecode_version: 1,
        },
        master_echo: MasterEcho {
            enabled: false,
            edl: 0,
            efb: 0,
            evol_l: 0,
            evol_r: 0,
            fir: [0; 8],
        },
        sample_pool: vec![SampleSlot {
            id: "lead".to_string(),
            name: "lead".to_string(),
            source: SampleSource {
                path: "audio/lead.wav".to_string(),
                sha256: "0".repeat(64),
                format: SampleFormat::Wav,
                sample_rate_hz: 32_000,
                channels: 1,
                frames: 256,
            },
            root_midi_note: 60,
            looped: SampleLoop {
                enabled: false,
                start_sample: None,
                end_sample: None,
                snap: None,
            },
            playback: SamplePlayback {
                volume: 1.0,
                pan: 0.0,
                echo: false,
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        }],
        m1: M1Block {
            active_sample_id: "lead".to_string(),
        },
    }
}

fn map_for(start_addr: u16) -> AramMapReport {
    let mut m = AramMapReport::stub();
    m.source_directory = Some(AramSourceDirSummary {
        source_count: 1,
        bytes: 4,
        padding_bytes: 252,
        start_addr,
    });
    m
}

#[test]
fn build_driver_for_minimal_project() {
    if skip_if_no_asar() {
        return;
    }
    let project = minimal_project();
    let map = map_for(0x1200);
    let dir = TempDir::new().unwrap();

    let out = build(DriverBuildInput {
        project: &project,
        map_report: &map,
        source_override: None,
        working_dir: dir.path().to_path_buf(),
    })
    .expect("build ok");

    assert!(!out.driver_code.is_empty(), "driver bytes nonempty");
    assert!(
        (out.driver_code.len() as u32) <= DRIVER_CODE_BUDGET_M1,
        "driver {} bytes > budget {}",
        out.driver_code.len(),
        DRIVER_CODE_BUDGET_M1
    );
    // Driver bytes must NOT contain the sentinel; we sliced before it.
    assert!(out.driver_code.windows(4).all(|w| w != DRIVER_END_SENTINEL));
    // First instruction is `mov $f2, #$6c` — opcode $8F, imm $6C, dp $F2.
    assert_eq!(&out.driver_code[..3], &[0x8F, 0x6C, 0xF2]);

    // SHA-256 stable across two builds with identical input.
    let dir2 = TempDir::new().unwrap();
    let out2 = build(DriverBuildInput {
        project: &project,
        map_report: &map,
        source_override: None,
        working_dir: dir2.path().to_path_buf(),
    })
    .expect("build ok");
    assert_eq!(
        out.driver_code_sha256, out2.driver_code_sha256,
        "deterministic"
    );
    assert_eq!(out.driver_code, out2.driver_code);
}

#[test]
fn build_driver_active_sample_missing_errors() {
    let mut project = minimal_project();
    project.m1.active_sample_id = "ghost".to_string();
    let map = map_for(0x1200);
    let dir = TempDir::new().unwrap();

    let err = build(DriverBuildInput {
        project: &project,
        map_report: &map,
        source_override: None,
        working_dir: dir.path().to_path_buf(),
    })
    .unwrap_err();
    assert!(matches!(err, DriverBuildError::ActiveSampleMissing(_)));
}

#[test]
fn compute_constants_with_missing_source_directory_errors() {
    // Use stub map report (no source_directory) to verify the
    // error path without going through asar.
    let project = minimal_project();
    let mut map = AramMapReport::stub();
    map.source_directory = None;
    let err = compute_constants(&project, &map).unwrap_err();
    assert!(matches!(err, DriverBuildError::SourceDirectoryMissing));
}

#[test]
fn build_driver_uses_correct_active_sample_index() {
    if skip_if_no_asar() {
        return;
    }
    let mut project = minimal_project();
    project.sample_pool.push(SampleSlot {
        id: "second".to_string(),
        name: "second".to_string(),
        source: SampleSource {
            path: "audio/second.wav".to_string(),
            sha256: "0".repeat(64),
            format: SampleFormat::Wav,
            sample_rate_hz: 32_000,
            channels: 1,
            frames: 256,
        },
        root_midi_note: 60,
        looped: SampleLoop {
            enabled: false,
            start_sample: None,
            end_sample: None,
            snap: None,
        },
        playback: SamplePlayback {
            volume: 1.0,
            pan: 0.0,
            echo: false,
            envelope: Envelope::GainRaw { gain_byte: 127 },
        },
    });
    project.m1.active_sample_id = "second".to_string();

    let map = map_for(0x1200);
    let dir = TempDir::new().unwrap();
    let out = build(DriverBuildInput {
        project: &project,
        map_report: &map,
        source_override: None,
        working_dir: dir.path().to_path_buf(),
    })
    .expect("build ok");

    // Find the V0SRCN write: `mov $f2, #$04` followed by `mov $f3, #imm`
    // where imm should be 1 (index of "second"). Encoded as
    // 8F 04 F2 8F 01 F3.
    let pat = [0x8F, 0x04, 0xF2, 0x8F, 0x01, 0xF3];
    assert!(
        out.driver_code.windows(pat.len()).any(|w| w == pat),
        "expected SRCN write with index 1; driver_code prefix: {:02X?}",
        &out.driver_code[..32]
    );
}

/// Check that the workspace's driver asm path exists. Cheap
/// guard — if this fails everything else does too.
#[test]
fn workspace_asm_path_exists() {
    let p: PathBuf = workspace_driver_asm_path();
    assert!(p.is_file(), "missing driver asm at {p:?}");
}
