//! M5.0 — Pitch register fixture pin per SPEC §10.11.
//!
//! The native-rate characterization harness (M5.1
//! implementation) MUST configure each characterization SPC
//! such that the DSP pitch register for the test voice equals
//! exactly `0x1000` (4096). This test asserts that contract via
//! the `baselines/m5.json` literal-pin pattern, anticipating
//! M5.1's implementation.
//!
//! Per consultant M5 plan #6. Mirrors the M4.0 Phase G pattern
//! (fixture-pin metric formulas before encoder implementation);
//! the equivalent for M5.0 is pinning the
//! "pitch register MUST equal 0x1000" baseline value at compile
//! time so an accidental edit to the baselines file is caught
//! before M5.1 lands.

#[test]
fn m5_pitch_register_constant_pinned_at_4096() {
    const BASELINES_JSON: &str = include_str!("../../baselines/m5.json");
    let baselines: serde_json::Value =
        serde_json::from_str(BASELINES_JSON).expect("baselines/m5.json must parse");

    let behavior_gated = baselines["behavior_gated"]
        .as_array()
        .expect("baselines/m5.json::behavior_gated must be an array");

    let pitch_entry = behavior_gated
        .iter()
        .find(|e| e["name"].as_str() == Some("M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER"))
        .expect(
            "baselines/m5.json must contain a behavior_gated entry named \
             M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER",
        );

    let value = pitch_entry["value"]
        .as_i64()
        .expect("M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER value must be an integer");

    assert_eq!(
        value, 4096,
        "M5 native-rate characterization pitch register MUST be 0x1000 (4096) per SPEC §10.11"
    );

    // Cross-checks on the entry's shape so accidental edits to
    // the locked-at / kind / test fields are also caught.
    assert_eq!(
        pitch_entry["locked_at"].as_str(),
        Some("M5.0"),
        "pitch-register baseline must be locked_at M5.0"
    );
    assert_eq!(
        pitch_entry["kind"].as_str(),
        Some("harness_constant"),
        "pitch-register baseline kind must be harness_constant"
    );
    assert!(
        pitch_entry["test"].is_string(),
        "pitch-register baseline must carry a test: field"
    );
}

// M5.1 — runtime regression guard. `m5_pitch_register_constant_pinned_at_4096`
// (above) keeps the `baselines/m5.json` literal value honest;
// `pitch_register_equals_4096_for_native_rate_signals` (below)
// drives every `m3_5_canonical` signal through
// `build_voice_setup_table` and verifies the resulting voice-setup
// bytes encode `pitch_register == 0x1000`. If a future driver /
// voice-setup refactor silently shifts the hardcoded
// `source_sample_rate_hz` away from 32000 (or the `desired == root`
// convention changes for atoms), this test fires before the change
// ships.

use sfc_atomizer_core::atom::AtomSlot;
use sfc_atomizer_core::characterize_gaussian::{m3_5_canonical_signals, TestSignal};
use sfc_atomizer_core::project::{Driver, MasterEcho, Project};
use sfc_atomizer_core::project_v2::{
    AtomSequence, AtomSequenceStep, AtomTransition, M2Block, ProjectV2, Track, TrackKind,
};
use sfc_atomizer_core::voice_setup::build_voice_setup_table;

/// Mirrors `app::build_characterization_project_json` but in the
/// typed `ProjectV2` form so the test can call
/// `build_voice_setup_table` directly. The atom is placed on voice
/// 0 and driven by a single-step `initial_kon` sequence — exactly
/// what `characterize-gaussian` builds at run time.
fn build_characterization_project_v2(signal: &TestSignal) -> ProjectV2 {
    let atom: AtomSlot = signal.atom.clone();
    let atom_id = atom.id.clone();
    ProjectV2 {
        schema_version: 2,
        project: Project {
            name: signal.name.to_string(),
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
            fir: [127, 0, 0, 0, 0, 0, 0, 0],
        },
        sample_pool: Vec::new(),
        atom_pool: vec![atom],
        atom_sequences: vec![AtomSequence {
            id: "atomseq_0001".to_string(),
            name: format!("{}_demo", signal.name),
            voice: 0,
            steps: vec![AtomSequenceStep {
                atom_id,
                duration_ticks: 240,
                target_volume: 1.0,
                transition: AtomTransition::InitialKon,
            }],
            looped: false,
        }],
        tracks: vec![Track {
            id: "track_atom_0".to_string(),
            name: String::new(),
            voice: 0,
            kind: TrackKind::AtomSequence {
                atom_sequence_id: "atomseq_0001".to_string(),
            },
        }],
        m2: M2Block {
            active_sequence_id: Some("atomseq_0001".to_string()),
        },
    }
}

#[test]
fn pitch_register_equals_4096_for_native_rate_signals() {
    // Per SPEC §10.11 (M5.1 update): the M2 atom-sequence voice
    // path is contractually required to program `pitch_register
    // == 0x1000` for every characterization voice. M5.1 preflight
    // verified this holds today via `core::voice_setup`'s
    // hardcoded `source_sample_rate_hz = 32000` + the `desired ==
    // root` convention. This test guards against silent drift.

    let signals = m3_5_canonical_signals();
    assert!(
        !signals.is_empty(),
        "m3_5_canonical_signals() must return at least one signal"
    );

    for signal in &signals {
        let project = build_characterization_project_v2(signal);
        let table = build_voice_setup_table(&project).unwrap_or_else(|e| {
            panic!(
                "build_voice_setup_table failed for signal {}: {e:?}",
                signal.name
            )
        });
        assert_eq!(
            table.len(),
            22,
            "voice setup table for {} must be 22 bytes (M2 layout, 2 voices × 11 bytes)",
            signal.name
        );
        // Voice 0 entry layout per SPEC §15.7: [voice, src_index,
        // pitch_l, pitch_h, vol_l, vol_r, adsr1, adsr2, gain,
        // reserved, reserved].
        assert_eq!(
            table[0], 0,
            "voice 0 byte must equal 0 for signal {}",
            signal.name
        );
        let pitch_l = table[2] as u16;
        let pitch_h = table[3] as u16;
        let pitch_register = (pitch_h << 8) | pitch_l;
        assert_eq!(
            pitch_register, 0x1000,
            "Signal {}: pitch register MUST be 0x1000 per SPEC §10.11 \
             (M5.1 unity-pitch contract); got pitch_l={:#04x} \
             pitch_h={:#04x} (= {:#06x})",
            signal.name, pitch_l, pitch_h, pitch_register
        );
    }
}
