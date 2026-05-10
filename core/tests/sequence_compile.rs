//! Integration tests for the M2.4 sequence compiler.

use sfc_atomizer_core::atom::{render_to_brr, AtomKind, AtomPartial, AtomRenderOptions, AtomSlot};
use sfc_atomizer_core::bytecode::{SEQUENCE_HEADER_LEN, SEQUENCE_REGION_MAGIC};
use sfc_atomizer_core::capability_manifest::CapabilityManifest;
use sfc_atomizer_core::packer::{pack_v2, EncodedSample, PackInputV2, VOICE_SETUP_TABLE_M2_BYTES};
use sfc_atomizer_core::project::{
    Driver, Envelope, MasterEcho, Project, SampleFormat, SampleLoop, SamplePlayback, SampleSlot,
    SampleSource,
};
use sfc_atomizer_core::project_v2::{
    AtomSequence, AtomSequenceStep, AtomTransition, M2Block, ProjectV2, Track, TrackKind,
};
use sfc_atomizer_core::sequence_compiler::{
    compile_sequence, SequenceCompileInput, SourceDirectory,
};
use sfc_atomizer_core::voice_setup::build_voice_setup_table;

fn sample(id: &str) -> SampleSlot {
    SampleSlot {
        id: id.to_string(),
        name: id.to_string(),
        source: SampleSource {
            path: format!("audio/{id}.wav"),
            sha256: "0".repeat(64),
            format: SampleFormat::Wav,
            sample_rate_hz: 32000,
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
            // Sample voice panned full LEFT for the M2 acceptance fixture.
            pan: -1.0,
            echo: false,
            envelope: Envelope::GainRaw { gain_byte: 127 },
        },
    }
}

fn atom_pan_right(id: &str, cycle: u16) -> AtomSlot {
    AtomSlot {
        id: id.to_string(),
        name: id.to_string(),
        kind: AtomKind::AdditiveSingleCycleV0 {
            partials: vec![AtomPartial {
                harmonic: 1,
                amplitude: 1.0,
                phase_cycles: 0.0,
            }],
        },
        root_midi_note: 60,
        cycle_len_samples: cycle,
        amplitude: 0.75,
        render: AtomRenderOptions {
            normalize: true,
            force_filter_0_first_block: true,
            force_filter_0_loop_entry: true,
        },
        playback: SamplePlayback {
            volume: 1.0,
            // Atom voice panned full RIGHT for the M2 acceptance fixture.
            pan: 1.0,
            echo: false,
            envelope: Envelope::GainRaw { gain_byte: 127 },
        },
    }
}

fn canonical_project() -> ProjectV2 {
    ProjectV2 {
        schema_version: 2,
        project: Project {
            name: "atomic_seq_e2e".to_string(),
            tick_rate_hz: 60,
        },
        driver: Driver {
            profile: "multi_voice_atom".to_string(),
            bytecode_version: 2,
        },
        master_echo: MasterEcho {
            enabled: false,
            edl: 0,
            efb: 0,
            evol_l: 0,
            evol_r: 0,
            fir: [0; 8],
        },
        sample_pool: vec![sample("lead")],
        atom_pool: vec![atom_pan_right("atom_a", 128), atom_pan_right("atom_b", 64)],
        atom_sequences: vec![AtomSequence {
            id: "atomseq_0001".to_string(),
            name: "two_step_demo".to_string(),
            voice: 1,
            steps: vec![
                AtomSequenceStep {
                    atom_id: "atom_a".to_string(),
                    duration_ticks: 120,
                    target_volume: 0.8,
                    transition: AtomTransition::InitialKon,
                },
                AtomSequenceStep {
                    atom_id: "atom_b".to_string(),
                    duration_ticks: 120,
                    target_volume: 0.8,
                    transition: AtomTransition::FadeToZeroRetrigger {
                        fade_out_ticks: 4,
                        fade_in_ticks: 4,
                    },
                },
            ],
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
fn end_to_end_compile_sequence_canonical_byte_pinned() {
    let project = canonical_project();
    let manifest = CapabilityManifest::multi_voice_atom();
    let source_directory = SourceDirectory::from_project(&project);
    let sequence = project.atom_sequences[0].clone();
    let out = compile_sequence(SequenceCompileInput {
        project: &project,
        manifest: &manifest,
        source_directory: &source_directory,
        sequence: &sequence,
    })
    .expect("compile");
    // 49 bytes total = 8 SEQ2 header + 41 payload. Locked.
    assert_eq!(out.bytecode.len(), 49);
    assert_eq!(out.bytecode_payload_len, 41);
    assert_eq!(&out.bytecode[0..4], &SEQUENCE_REGION_MAGIC);
    assert_eq!(
        out.bytecode[SEQUENCE_HEADER_LEN + out.bytecode_payload_len as usize - 1],
        0x00
    );

    // M2.8.2 (consultant M2.8.1 follow-up audit): pin against
    // baselines/m2.json so drift catches at the SHA layer alongside
    // the byte-shape assertions. Mirrors the M2.8.1
    // `m1_driver_code_sha_matches_locked_baseline` pattern.
    const BASELINES_JSON: &str = include_str!("../../baselines/m2.json");
    let baselines: serde_json::Value =
        serde_json::from_str(BASELINES_JSON).expect("baselines/m2.json must parse");
    let identity_gated = baselines["identity_gated"]
        .as_array()
        .expect("baselines.identity_gated must be an array");
    let entry = identity_gated
        .iter()
        .find(|e| e["name"].as_str() == Some("M2_CANONICAL_SEQUENCE_BYTECODE_SHA256"))
        .expect("baselines/m2.json must have M2_CANONICAL_SEQUENCE_BYTECODE_SHA256");
    let locked_sha = entry["value"]
        .as_str()
        .expect("M2_CANONICAL_SEQUENCE_BYTECODE_SHA256 value must be a string");

    assert_eq!(
        out.bytecode_sha256, locked_sha,
        "SEQ2 bytecode SHA drift vs baselines/m2.json — investigate before \
         updating the baseline (locked at M2.4)."
    );
}

#[test]
fn end_to_end_pack_with_sequence_emits_sequence_region() {
    let project = canonical_project();
    let manifest = CapabilityManifest::multi_voice_atom();
    let source_directory = SourceDirectory::from_project(&project);
    let sequence = project.atom_sequences[0].clone();

    let mut encoded_atoms: Vec<EncodedSample> = Vec::new();
    for atom in &project.atom_pool {
        let r = render_to_brr(atom).expect("render");
        encoded_atoms.push(EncodedSample {
            sample_id: atom.id.clone(),
            bytes: r.brr_bytes,
            loop_entry_block: Some(0),
        });
    }

    let voice_table = build_voice_setup_table(&project).expect("voice table");
    let seq = compile_sequence(SequenceCompileInput {
        project: &project,
        manifest: &manifest,
        source_directory: &source_directory,
        sequence: &sequence,
    })
    .expect("compile");

    let r = pack_v2(PackInputV2 {
        project: project.clone(),
        encoded_samples: vec![EncodedSample {
            sample_id: "lead".to_string(),
            bytes: vec![0u8; 18],
            loop_entry_block: None,
        }],
        encoded_atoms,
        driver_code: Vec::new(),
        sequence_data: Some(seq.bytecode.clone()),
        voice_setup_table: Some(voice_table),
    })
    .expect("pack");

    // sequence_data region present in map.
    let names: Vec<&str> = r
        .map_report
        .regions
        .iter()
        .map(|x| x.name.as_str())
        .collect();
    assert!(
        names.contains(&"sequence_data"),
        "expected sequence_data region"
    );

    // SEQ2 magic at the sequence_data start address.
    let seq_region = r
        .map_report
        .regions
        .iter()
        .find(|x| x.name == "sequence_data")
        .unwrap();
    let start =
        u32::from_str_radix(seq_region.start.trim_start_matches("0x"), 16).unwrap() as usize;
    assert_eq!(&r.aram_image[start..start + 4], b"SEQ2");

    // Region order: source_directory < sequence_data < sample_brr_pool < synth_atom_pool < voice_setup_table.
    let positions = |n: &str| names.iter().position(|x| *x == n);
    assert!(positions("source_directory").unwrap() < positions("sequence_data").unwrap());
    assert!(positions("sequence_data").unwrap() < positions("sample_brr_pool").unwrap());
    assert!(positions("sample_brr_pool").unwrap() < positions("synth_atom_pool").unwrap());
    assert!(positions("synth_atom_pool").unwrap() < positions("voice_setup_table").unwrap());

    // Voice setup table is exactly 22 bytes.
    let v_region = r
        .map_report
        .regions
        .iter()
        .find(|x| x.name == "voice_setup_table")
        .unwrap();
    assert_eq!(v_region.bytes, VOICE_SETUP_TABLE_M2_BYTES);
}

#[test]
fn pack_with_no_active_sequence_omits_sequence_region() {
    // Edge case from the brief Phase I test R: multi_voice_atom
    // project with active_sequence_id = null. Per validation rule
    // 57 the project still needs at least one atom_sequence track,
    // and rule 55 allows active_sequence_id = null. The pack path
    // should proceed without a sequence_data region; the compiler's
    // input is "no active sequence" so it isn't called.
    let mut project = canonical_project();
    project.m2.active_sequence_id = None;
    project
        .validate()
        .expect("active=null is allowed when atom_sequence track exists");

    let mut encoded_atoms: Vec<EncodedSample> = Vec::new();
    for atom in &project.atom_pool {
        let r = render_to_brr(atom).expect("render");
        encoded_atoms.push(EncodedSample {
            sample_id: atom.id.clone(),
            bytes: r.brr_bytes,
            loop_entry_block: Some(0),
        });
    }
    let voice_table = build_voice_setup_table(&project).expect("voice table");
    let r = pack_v2(PackInputV2 {
        project: project.clone(),
        encoded_samples: vec![EncodedSample {
            sample_id: "lead".to_string(),
            bytes: vec![0u8; 18],
            loop_entry_block: None,
        }],
        encoded_atoms,
        driver_code: Vec::new(),
        sequence_data: None, // active sequence is null
        voice_setup_table: Some(voice_table),
    })
    .expect("pack");

    let has_seq = r
        .map_report
        .regions
        .iter()
        .any(|x| x.name == "sequence_data");
    assert!(
        !has_seq,
        "no sequence region when active_sequence_id is null"
    );
}

/// Voice-setup-table byte-pinned ABI test (consultant #28).
///
/// Pack the canonical M2 multi-voice fixture; extract the 22-byte
/// table from the ARAM image at the address reported by the
/// AramMapReport; assert a byte-exact match against the expected
/// vector computed from the fixture's pan/volume/envelope/pitch.
#[test]
fn voice_setup_table_byte_pinned_abi() {
    let project = canonical_project();
    let mut encoded_atoms: Vec<EncodedSample> = Vec::new();
    for atom in &project.atom_pool {
        let r = render_to_brr(atom).expect("render");
        encoded_atoms.push(EncodedSample {
            sample_id: atom.id.clone(),
            bytes: r.brr_bytes,
            loop_entry_block: Some(0),
        });
    }
    let voice_table = build_voice_setup_table(&project).expect("voice table");
    let manifest = CapabilityManifest::multi_voice_atom();
    let source_directory = SourceDirectory::from_project(&project);
    let sequence = project.atom_sequences[0].clone();
    let seq = compile_sequence(SequenceCompileInput {
        project: &project,
        manifest: &manifest,
        source_directory: &source_directory,
        sequence: &sequence,
    })
    .expect("compile");

    let r = pack_v2(PackInputV2 {
        project: project.clone(),
        encoded_samples: vec![EncodedSample {
            sample_id: "lead".to_string(),
            bytes: vec![0u8; 18],
            loop_entry_block: None,
        }],
        encoded_atoms,
        driver_code: Vec::new(),
        sequence_data: Some(seq.bytecode),
        voice_setup_table: Some(voice_table.clone()),
    })
    .expect("pack");

    // Find voice_setup_table region.
    let v_region = r
        .map_report
        .regions
        .iter()
        .find(|x| x.name == "voice_setup_table")
        .unwrap();
    let addr = u32::from_str_radix(v_region.start.trim_start_matches("0x"), 16).unwrap() as usize;
    let actual = &r.aram_image[addr..addr + VOICE_SETUP_TABLE_M2_BYTES as usize];

    // Expected — fixture-derived:
    //
    // Voice 0: sample track on SRCN 0, sample at 32 kHz / root MIDI 60.
    //   pitch = $1000; vol_l/vol_r from constant-power pan = -1.0
    //   (full LEFT) at volume 1.0: theta = 0, cos=1, sin=0; vol_l=127,
    //   vol_r=0. ADSR1/ADSR2 = $00/$00 (gain_raw envelope), GAIN = 127.
    //
    // Voice 1: atom track on SRCN 1, atom at 32 kHz / root MIDI 60.
    //   pitch = $1000; vol_l/vol_r from atom playback (used by
    //   build_voice_setup_table — atom.playback.volume * pan):
    //   atom volume=1.0 pan=+1.0 → theta = π/2, cos=0, sin=1;
    //   vol_l=0, vol_r=127. ADSR/GAIN same.
    let expected: [u8; 22] = [
        // voice 0
        0x00, // voice
        0x00, // src_index
        0x00, 0x10, // pitch_le ($1000)
        0x7F, // vol_l (127)
        0x00, // vol_r (0)
        0x00, // adsr1
        0x00, // adsr2
        0x7F, // gain (127)
        0x00, // flags_reserved
        0x00, // pad_reserved
        // voice 1
        0x01, // voice
        0x01, // src_index = sample_count + atom-pool index = 1 + 0 = 1
        0x00, 0x10, // pitch_le
        0x00, // vol_l (0)
        0x7F, // vol_r (127)
        0x00, // adsr1
        0x00, // adsr2
        0x7F, // gain
        0x00, // flags_reserved
        0x00, // pad_reserved
    ];
    assert_eq!(
        actual,
        &expected[..],
        "voice setup table ABI drift: expected {expected:?}, got {actual:?}"
    );

    // M2.8.2 (consultant M2.8.1 follow-up audit): standardize the
    // identity-pin pattern on baselines/m2.json. The byte-vector
    // assertion above documents the ABI directly; the SHA assertion
    // catches future drift via the baseline file (single source of
    // truth alongside the M1 driver / SEQ2 bytecode pins).
    let actual_sha = sfc_atomizer_core::asm::sha256_hex(actual);
    const BASELINES_JSON: &str = include_str!("../../baselines/m2.json");
    let baselines: serde_json::Value =
        serde_json::from_str(BASELINES_JSON).expect("baselines/m2.json must parse");
    let entry = baselines["identity_gated"]
        .as_array()
        .expect("baselines.identity_gated must be an array")
        .iter()
        .find(|e| e["name"].as_str() == Some("M2_CANONICAL_VOICE_SETUP_TABLE_SHA256"))
        .expect("M2_CANONICAL_VOICE_SETUP_TABLE_SHA256 missing from baselines");
    let locked_sha = entry["value"]
        .as_str()
        .expect("M2_CANONICAL_VOICE_SETUP_TABLE_SHA256 value must be a string");
    assert_eq!(
        actual_sha, locked_sha,
        "voice setup table SHA drift vs baselines/m2.json — investigate before \
         updating the baseline (locked at M2.4)."
    );
}
