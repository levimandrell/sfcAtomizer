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
use sfc_atomizer_core::packer::DRIVER_CODE_BUDGET_4KIB;
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
        (out.driver_code.len() as u32) <= DRIVER_CODE_BUDGET_4KIB,
        "driver {} bytes > budget {}",
        out.driver_code.len(),
        DRIVER_CODE_BUDGET_4KIB
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

/// M2.8.1 (consultant M2 close-out #16): the M2.4 release-baseline
/// table classified `M1_DRIVER_CODE_SHA256` as identity-gated, but
/// the only test pinning it (`build_driver_for_minimal_project`)
/// asserted byte-equality across two consecutive builds — not
/// against the locked literal. Drift in the loader / asar / driver
/// asm would slip through. This test pulls the locked value from
/// `baselines/m2.json` (the single source of truth) and asserts
/// the build produces the same SHA.
///
/// `baselines/m2.json` is included via `include_str!` so the test
/// fails to compile when the file is missing, rather than skipping
/// silently. asar resolution gates the runtime build (skips with
/// stderr note when asar isn't present, same as
/// `build_driver_for_minimal_project`).
#[test]
fn m1_driver_code_sha_matches_locked_baseline() {
    if skip_if_no_asar() {
        return;
    }
    const BASELINES_JSON: &str =
        include_str!("../../baselines/m2.json");
    let baselines: serde_json::Value =
        serde_json::from_str(BASELINES_JSON).expect("baselines/m2.json must parse");
    let identity_gated = baselines["identity_gated"]
        .as_array()
        .expect("baselines.identity_gated must be an array");
    let entry = identity_gated
        .iter()
        .find(|e| e["name"].as_str() == Some("M1_DRIVER_CODE_SHA256"))
        .expect("baselines/m2.json must have M1_DRIVER_CODE_SHA256");
    let locked_sha = entry["value"]
        .as_str()
        .expect("M1_DRIVER_CODE_SHA256 value must be a string");

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

    assert_eq!(
        out.driver_code_sha256, locked_sha,
        "M1 driver SHA drift vs baselines/m2.json — investigate before \
         updating the baseline (locked at M2.0 rebase)."
    );
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

/// M2.0 (consultant #1) regression: the M1 driver must seed
/// dp_last_token from `$F4` BEFORE writing the ready signature, so
/// the IPL exec-residual byte isn't mistaken for a fresh command.
/// Asar emits the dp form `mov a, $f4` (E4 F4) followed by
/// `mov $00, a` (C4 00); we look for that 4-byte sequence in the
/// driver bytes and assert it lands before the ready-signature
/// writes.
#[test]
fn driver_seeds_dp_last_token_from_f4_before_ready_signature() {
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

    // Expected encoded sequence: E4 F4 C4 00 (mov a,$f4 ; mov $00, a).
    let bootstrap_seq: &[u8] = &[0xE4, 0xF4, 0xC4, 0x00];
    let bootstrap_pos = out
        .driver_code
        .windows(bootstrap_seq.len())
        .position(|w| w == bootstrap_seq)
        .expect("bootstrap sequence E4 F4 C4 00 missing from driver");

    // Ready signature begins with `mov $f4, #$a5` (8F A5 F4) — the
    // first DP-store of the literal $A5 to driver_out_0. Confirm
    // the bootstrap occurs before the ready signature.
    let ready_seq: &[u8] = &[0x8F, 0xA5, 0xF4];
    let ready_pos = out
        .driver_code
        .windows(ready_seq.len())
        .position(|w| w == ready_seq)
        .expect("ready signature 8F A5 F4 missing from driver");

    assert!(
        bootstrap_pos < ready_pos,
        "bootstrap at {bootstrap_pos} must precede ready signature at {ready_pos}"
    );
}

/// M2.0 (consultant #7) regression: an oversized injected sentinel
/// inside the .asm body should trip `SentinelCollision`, not silent
/// truncation. Constructs a synthetic .asm with the canonical
/// sentinel pattern emitted before driver_end.
#[test]
fn driver_build_flags_sentinel_collision() {
    if skip_if_no_asar() {
        return;
    }
    // Inject the sentinel pattern into a near-empty driver source
    // BEFORE the canonical driver_end marker. The build path should
    // catch the collision rather than truncate at the inner
    // sentinel and leave a tiny "driver".
    let bad_src = "\
incsrc \"m1_constants.inc\"
lorom
arch spc700
org $008200
base $0200
driver_entry:
    db $de, $ad, $be, $ef        ; intentional collision
    nop
driver_end:
    db $de, $ad, $be, $ef        ; canonical sentinel
";
    let project = minimal_project();
    let map = map_for(0x1200);
    let dir = TempDir::new().unwrap();
    let err = build(DriverBuildInput {
        project: &project,
        map_report: &map,
        source_override: Some(bad_src),
        working_dir: dir.path().to_path_buf(),
    })
    .unwrap_err();
    assert!(
        matches!(err, DriverBuildError::SentinelCollision(..)),
        "expected SentinelCollision, got {err:?}"
    );
}

// =============================================================================
// M2.5 — build_m2 (multi_voice_atom driver)
// =============================================================================

use sfc_atomizer_core::driver_build::{build_m2, DriverBuildInputM2};
use sfc_atomizer_core::project_v2::{
    AtomSequence, AtomSequenceStep, AtomTransition, M2Block, ProjectV2, Track, TrackKind,
};

fn minimal_v2_multi_voice() -> ProjectV2 {
    use sfc_atomizer_core::atom::{AtomKind, AtomPartial, AtomRenderOptions, AtomSlot};
    let v1 = minimal_project();
    ProjectV2 {
        schema_version: 2,
        project: v1.project.clone(),
        driver: Driver {
            profile: "multi_voice_atom".to_string(),
            bytecode_version: 2,
        },
        master_echo: v1.master_echo.clone(),
        sample_pool: v1.sample_pool.clone(),
        atom_pool: vec![AtomSlot {
            id: "atom_a".to_string(),
            name: "atom_a".to_string(),
            kind: AtomKind::AdditiveSingleCycleV0 {
                partials: vec![AtomPartial {
                    harmonic: 1,
                    amplitude: 1.0,
                    phase_cycles: 0.0,
                }],
            },
            root_midi_note: 60,
            cycle_len_samples: 128,
            amplitude: 0.75,
            render: AtomRenderOptions {
                normalize: true,
                force_filter_0_first_block: true,
                force_filter_0_loop_entry: true,
            },
            playback: SamplePlayback {
                volume: 1.0,
                pan: 1.0,
                echo: false,
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        }],
        atom_sequences: vec![AtomSequence {
            id: "atomseq_0001".to_string(),
            name: "single".to_string(),
            voice: 1,
            steps: vec![AtomSequenceStep {
                atom_id: "atom_a".to_string(),
                duration_ticks: 60,
                target_volume: 0.8,
                transition: AtomTransition::InitialKon,
            }],
            looped: false,
        }],
        tracks: vec![
            Track {
                id: "track_sample_0".to_string(),
                name: String::new(),
                voice: 0,
                kind: TrackKind::SampleSustain {
                    sample_id: "lead".to_string(),
                },
            },
            Track {
                id: "track_atom_1".to_string(),
                name: String::new(),
                voice: 1,
                kind: TrackKind::AtomSequence {
                    atom_sequence_id: "atomseq_0001".to_string(),
                },
            },
        ],
        m2: M2Block {
            active_sequence_id: Some("atomseq_0001".to_string()),
        },
    }
}

#[test]
fn build_driver_m2_assembles_within_budget() {
    if skip_if_no_asar() {
        return;
    }
    let project = minimal_v2_multi_voice();
    let map = map_for(0x1200);
    let dir = TempDir::new().unwrap();
    let out = build_m2(DriverBuildInputM2 {
        project: &project,
        map_report: &map,
        voice_setup_addr: 0x14FC,
        sequence_addr: 0x1300,
        source_override: None,
        working_dir: dir.path().to_path_buf(),
    })
    .expect("build_m2 must succeed");
    assert!(
        out.driver_code.len() as u32 <= DRIVER_CODE_BUDGET_4KIB,
        "M2 driver {} bytes exceeds {} budget",
        out.driver_code.len(),
        DRIVER_CODE_BUDGET_4KIB
    );
    // Driver code SHA must be deterministic.
    let dir2 = TempDir::new().unwrap();
    let out2 = build_m2(DriverBuildInputM2 {
        project: &project,
        map_report: &map,
        voice_setup_addr: 0x14FC,
        sequence_addr: 0x1300,
        source_override: None,
        working_dir: dir2.path().to_path_buf(),
    })
    .unwrap();
    assert_eq!(out.driver_code_sha256, out2.driver_code_sha256);
    eprintln!(
        "M2 driver: {} bytes, sha256={}",
        out.driver_code.len(),
        out.driver_code_sha256
    );
}

/// M2.5: `init_kon_mask` is derived from project tracks. A project
/// with one `sample_sustain` track on voice 0 + one `atom_sequence`
/// track on voice 1 yields mask=$01 (only voice 0 KON'd at init);
/// the atom voice is left for the bytecode's first KON opcode.
#[test]
fn compute_constants_m2_init_kon_mask_only_sample_voices() {
    use sfc_atomizer_core::driver_build::compute_constants_m2;
    let project = minimal_v2_multi_voice();
    let map = map_for(0x1200);
    let c = compute_constants_m2(&project, &map, 0x14FC, 0x1300).unwrap();
    assert_eq!(
        c.init_kon_mask, 0b01,
        "expected init_kon_mask=0b01 (sample on v0, atom on v1)"
    );
}

/// `sequence_addr` constant in the .inc must point at the
/// bytecode payload, i.e. region_start + 8 bytes (skipping the
/// SEQ2 header). Driver init reads from this address directly.
#[test]
fn render_constants_inc_m2_sequence_addr_skips_seq2_header() {
    use sfc_atomizer_core::driver_build::{compute_constants_m2, render_constants_inc_m2};
    let project = minimal_v2_multi_voice();
    let map = map_for(0x1200);
    let c = compute_constants_m2(&project, &map, 0x14FC, 0x1300).unwrap();
    let inc = render_constants_inc_m2(&c, &project);
    // 0x1300 region start + 8-byte header = 0x1308. Lo=$08, hi=$13.
    let line_lo = inc
        .lines()
        .find(|l| l.starts_with("sequence_addr_lo"))
        .expect("sequence_addr_lo line missing");
    let line_hi = inc
        .lines()
        .find(|l| l.starts_with("sequence_addr_hi"))
        .expect("sequence_addr_hi line missing");
    assert!(
        line_lo.ends_with("$08"),
        "expected payload-aligned lo byte $08; line='{line_lo}'"
    );
    assert!(
        line_hi.ends_with("$13"),
        "expected payload-aligned hi byte $13; line='{line_hi}'"
    );
}

/// M2.5 driver implements the KOFF-clear-before-KON pattern in
/// `op_kon` (SPEC §14.3 "KON / KOFF latching"). The assembled body
/// must therefore contain both a `mov $f2, #$5c ; mov $f3, #$00`
/// (KOFF clear) sequence AND a subsequent `mov $f2, #$4c` (KON write)
/// inside the bytecode handler block — distinct from the init-time
/// KOFF clear at the top.
#[test]
fn build_driver_m2_clears_koff_before_kon_in_op_kon() {
    if skip_if_no_asar() {
        return;
    }
    let project = minimal_v2_multi_voice();
    let map = map_for(0x1200);
    let dir = TempDir::new().unwrap();
    let out = build_m2(DriverBuildInputM2 {
        project: &project,
        map_report: &map,
        voice_setup_addr: 0x14FC,
        sequence_addr: 0x1300,
        source_override: None,
        working_dir: dir.path().to_path_buf(),
    })
    .unwrap();

    // KOFF = $5c, immediate $00. Encoded: 8F 5C F2 8F 00 F3.
    let koff_clear: &[u8] = &[0x8F, 0x5C, 0xF2, 0x8F, 0x00, 0xF3];
    let koff_positions: Vec<usize> = out
        .driver_code
        .windows(koff_clear.len())
        .enumerate()
        .filter_map(|(i, w)| if w == koff_clear { Some(i) } else { None })
        .collect();
    assert!(
        koff_positions.len() >= 2,
        "expected at least two KOFF=00 writes (init + op_kon); found {}: {:?}",
        koff_positions.len(),
        koff_positions
    );
    // KON = $4c. Encoded: 8F 4C F2 (immediate write follows).
    let kon_write: &[u8] = &[0x8F, 0x4C, 0xF2];
    let kon_positions: Vec<usize> = out
        .driver_code
        .windows(kon_write.len())
        .enumerate()
        .filter_map(|(i, w)| if w == kon_write { Some(i) } else { None })
        .collect();
    // Three expected KON writes: init clear (mask=00) + init_kon_mask + op_kon.
    assert!(
        kon_positions.len() >= 3,
        "expected at least three KON writes; found {}: {:?}",
        kon_positions.len(),
        kon_positions
    );
    // The last KOFF=00 write must precede the last KON write — that's
    // the op_kon body's "clear KOFF, then KON" pair.
    let last_koff = *koff_positions.last().unwrap();
    let last_kon = *kon_positions.last().unwrap();
    assert!(
        last_koff < last_kon,
        "op_kon's KOFF clear must precede its KON write: koff@{last_koff}, kon@{last_kon}"
    );
}
