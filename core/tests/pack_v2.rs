//! Integration tests for the M2.3 multi-source packer.

use sfc_atomizer_core::atom::{render_to_brr, AtomKind, AtomPartial, AtomRenderOptions, AtomSlot};
use sfc_atomizer_core::packer::{
    pack_v2, EncodedSample, PackError, PackInputV2, VOICE_SETUP_TABLE_M2_BYTES,
};
use sfc_atomizer_core::project::{
    Driver, Envelope, MasterEcho, Project, SampleFormat, SampleLoop, SamplePlayback, SampleSlot,
    SampleSource,
};
use sfc_atomizer_core::project_v2::{
    AtomSequence, AtomSequenceStep, AtomTransition, M2Block, ProjectV2, Track, TrackKind,
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
            pan: 0.0,
            echo: false,
            envelope: Envelope::GainRaw { gain_byte: 127 },
        },
    }
}

fn atom(id: &str, cycle: u16) -> AtomSlot {
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
            volume: 0.8,
            pan: 0.0,
            echo: false,
            envelope: Envelope::GainRaw { gain_byte: 127 },
        },
    }
}

fn project_v2_multi_voice() -> ProjectV2 {
    ProjectV2 {
        schema_version: 2,
        project: Project {
            name: "atomic_e2e".to_string(),
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
        atom_pool: vec![atom("atom_a", 128), atom("atom_b", 64)],
        atom_sequences: vec![AtomSequence {
            id: "atomseq_0001".to_string(),
            name: "single".to_string(),
            voice: 1,
            steps: vec![AtomSequenceStep {
                atom_id: "atom_a".to_string(),
                duration_ticks: 120,
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
fn end_to_end_pack_v2_multi_voice_one_sample_two_atoms() {
    let project = project_v2_multi_voice();
    project.validate().expect("project must validate");

    // Render atoms via the real M2.2 path (not synthetic zeros).
    let mut encoded_atoms: Vec<EncodedSample> = Vec::with_capacity(project.atom_pool.len());
    for a in &project.atom_pool {
        let out = render_to_brr(a).expect("atom render");
        encoded_atoms.push(EncodedSample {
            sample_id: a.id.clone(),
            bytes: out.brr_bytes,
            loop_entry_block: Some(0),
        });
    }

    let voice_table = build_voice_setup_table(&project).expect("voice table");
    assert_eq!(voice_table.len(), VOICE_SETUP_TABLE_M2_BYTES as usize);

    let r = pack_v2(PackInputV2 {
        project: project.clone(),
        encoded_samples: vec![EncodedSample {
            sample_id: "lead".to_string(),
            bytes: vec![0u8; 18], // 2 zero blocks
            loop_entry_block: None,
        }],
        encoded_atoms: encoded_atoms.clone(),
        driver_code: Vec::new(),
        sequence_data: None,
        voice_setup_table: Some(voice_table.clone()),
    })
    .expect("pack_v2 must succeed for valid multi-voice project");

    // Region structure: driver, srcdir, sample_brr_pool, synth_atom_pool, voice_setup_table, free, echo (off so omitted), ipl pad, ipl shadow.
    let names: Vec<&str> = r
        .map_report
        .regions
        .iter()
        .map(|x| x.name.as_str())
        .collect();
    for required in [
        "direct_page",
        "hardware_io",
        "stack",
        "driver_code",
        "source_directory",
        "sample_brr_pool",
        "synth_atom_pool",
        "voice_setup_table",
        "free",
        "ipl_rom_safe_pad",
        "ipl_rom_shadow",
    ] {
        assert!(names.contains(&required), "missing region {required}");
    }

    // Atoms summary populated.
    let atoms = r.map_report.atoms.as_ref().expect("atoms summary");
    assert_eq!(atoms.total_atoms, 2);
    assert_eq!(
        atoms.total_brr_bytes,
        encoded_atoms
            .iter()
            .map(|a| a.bytes.len() as u32)
            .sum::<u32>()
    );
    assert_eq!(atoms.per_atom[0].atom_id, "atom_a");
    assert_eq!(atoms.per_atom[0].source_index, 1);
    assert_eq!(atoms.per_atom[0].cycle_len_samples, 128);
    assert_eq!(atoms.per_atom[1].atom_id, "atom_b");
    assert_eq!(atoms.per_atom[1].source_index, 2);
    assert_eq!(atoms.per_atom[1].cycle_len_samples, 64);

    // Source directory shape: 3 entries × 4 bytes, page-padded.
    let srcdir = r.map_report.source_directory.as_ref().unwrap();
    assert_eq!(srcdir.source_count, 3);
    assert_eq!(srcdir.bytes, 12);
    assert_eq!(srcdir.start_addr, 0x1200);

    // Voice setup table region appears with 22 bytes.
    let voice_region = r
        .map_report
        .regions
        .iter()
        .find(|x| x.name == "voice_setup_table")
        .expect("voice_setup_table region");
    assert_eq!(voice_region.bytes, VOICE_SETUP_TABLE_M2_BYTES);
}

#[test]
fn pack_v2_atom_count_mismatch_errors() {
    let project = project_v2_multi_voice();
    let err = pack_v2(PackInputV2 {
        project,
        encoded_samples: vec![EncodedSample {
            sample_id: "lead".to_string(),
            bytes: vec![0u8; 18],
            loop_entry_block: None,
        }],
        encoded_atoms: Vec::new(), // project has 2 atoms; we pass 0
        driver_code: Vec::new(),
        sequence_data: None,
        voice_setup_table: None,
    })
    .unwrap_err();
    assert!(
        matches!(
            err,
            PackError::AtomCountMismatch {
                got: 0,
                expected: 2
            }
        ),
        "got: {err:?}"
    );
}

#[test]
fn pack_v2_voice_setup_table_wrong_size_errors() {
    let project = project_v2_multi_voice();
    let err = pack_v2(PackInputV2 {
        project,
        encoded_samples: vec![EncodedSample {
            sample_id: "lead".to_string(),
            bytes: vec![0u8; 18],
            loop_entry_block: None,
        }],
        encoded_atoms: vec![
            EncodedSample {
                sample_id: "atom_a".to_string(),
                bytes: vec![0u8; 72],
                loop_entry_block: Some(0),
            },
            EncodedSample {
                sample_id: "atom_b".to_string(),
                bytes: vec![0u8; 36],
                loop_entry_block: Some(0),
            },
        ],
        driver_code: Vec::new(),
        sequence_data: None,
        voice_setup_table: Some(vec![0u8; 11]), // wrong size — should be 22
    })
    .unwrap_err();
    assert!(
        matches!(
            err,
            PackError::VoiceSetupTableSize {
                actual: 11,
                expected: 22
            }
        ),
        "got: {err:?}"
    );
}

#[test]
fn pack_module_size_within_32_kib_cap() {
    use sfc_atomizer_core::module_writer::{write_module, ModuleWriteInput, MODULE_MAX_BYTES};

    // Pack a typical multi-voice project (1 sample + 2 atoms) and
    // confirm the resulting module.bin fits the §15.6 cap.
    let project = project_v2_multi_voice();
    let mut encoded_atoms: Vec<EncodedSample> = Vec::with_capacity(project.atom_pool.len());
    for a in &project.atom_pool {
        let out = render_to_brr(a).expect("atom render");
        encoded_atoms.push(EncodedSample {
            sample_id: a.id.clone(),
            bytes: out.brr_bytes,
            loop_entry_block: Some(0),
        });
    }
    let voice_table = build_voice_setup_table(&project).expect("voice table");
    let r = pack_v2(PackInputV2 {
        project,
        encoded_samples: vec![EncodedSample {
            sample_id: "lead".to_string(),
            bytes: vec![0u8; 18],
            loop_entry_block: None,
        }],
        encoded_atoms,
        driver_code: Vec::new(),
        sequence_data: None,
        voice_setup_table: Some(voice_table),
    })
    .expect("pack_v2");

    let module = write_module(ModuleWriteInput {
        aram_image: &r.aram_image,
        map_report: &r.map_report,
        echo_enabled: false,
    })
    .expect("module write");
    assert!(
        module.bytes.len() < MODULE_MAX_BYTES as usize,
        "module {} bytes >= {}",
        module.bytes.len(),
        MODULE_MAX_BYTES
    );
}
