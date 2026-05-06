//! Project schema v2 — type skeleton.
//!
//! SPEC §16.9 / §16.10. The full type tree lands at M2.0 contracts
//! freeze; `validate()` and `migrate_from_v1()` bodies are
//! `todo!()` until M2.1 ships the implementation.

use serde::{Deserialize, Serialize};

use crate::atom::AtomSlot;
use crate::project::{Driver, MasterEcho, Project, ProjectV1, SampleSlot};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectV2 {
    /// Locked at `2` for v2 projects.
    pub schema_version: u32,
    pub project: Project,
    pub driver: Driver,
    pub master_echo: MasterEcho,
    pub sample_pool: Vec<SampleSlot>,
    pub atom_pool: Vec<AtomSlot>,
    pub atom_sequences: Vec<AtomSequence>,
    pub tracks: Vec<Track>,
    pub m2: M2Block,
}

impl ProjectV2 {
    pub const SCHEMA_VERSION_M2: u32 = 2;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtomSequence {
    pub id: String,
    pub name: String,
    /// 0..=1 in M2; references `tracks[].voice` for the matching
    /// `kind: atom_sequence` track.
    pub voice: u8,
    pub steps: Vec<AtomSequenceStep>,
    /// Whether to loop back to step 0 after the last step.
    #[serde(rename = "loop")]
    pub looped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtomSequenceStep {
    pub atom_id: String,
    /// 1..=255 in M2 (sequence ticks).
    pub duration_ticks: u8,
    /// 0.0..=1.0; mapped to driver `target_l`/`target_r` via the
    /// pan formula in §16.4.
    pub target_volume: f64,
    pub transition: AtomTransition,
}

/// Tagged-union transition kind (SPEC §16.9). First step of a
/// sequence must be `InitialKon`; subsequent steps must be
/// `FadeToZeroRetrigger` in M2.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AtomTransition {
    InitialKon,
    FadeToZeroRetrigger {
        fade_out_ticks: u8,
        fade_in_ticks: u8,
    },
}

/// Tagged-union track. `sample_sustain` references a
/// `sample_pool[].id`; `atom_sequence` references an
/// `atom_sequences[].id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Track {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// 0..=1 in M2; unique across `tracks[]`.
    pub voice: u8,
    #[serde(flatten)]
    pub kind: TrackKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrackKind {
    SampleSustain { sample_id: String },
    AtomSequence { atom_sequence_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct M2Block {
    /// `None` until the user designates an active sequence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_sequence_id: Option<String>,
}

impl ProjectV2 {
    /// **M2.1 stub.** SPEC §16.9 cross-field validation rules.
    pub fn validate(&self) -> Result<(), Vec<crate::project::ValidationError>> {
        // M2.0 contracts only — types frozen, validation body is M2.1.
        let _ = self;
        todo!("ProjectV2::validate body lands at M2.1 — see SPEC §16.9 validation rules");
    }
}

/// Migrate a v1 [`ProjectV1`] to v2 per SPEC §16.10. Stub for M2.0
/// contracts pass; M2.1 implements.
pub fn migrate_from_v1(_v1: ProjectV1) -> ProjectV2 {
    todo!("migrate_from_v1 lands at M2.1 — see SPEC §16.10 migration table");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Envelope, SampleFormat, SampleLoop, SamplePlayback, SampleSource};

    fn round_trip<T>(v: &T)
    where
        T: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(v).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, &back);
    }

    fn minimal_project_v2() -> ProjectV2 {
        ProjectV2 {
            schema_version: 2,
            project: Project {
                name: "demo".to_string(),
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
            sample_pool: vec![SampleSlot {
                id: "sample_0001".to_string(),
                name: "lead".to_string(),
                source: SampleSource {
                    path: "audio/lead.wav".to_string(),
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
            }],
            atom_pool: Vec::new(),
            atom_sequences: Vec::new(),
            tracks: vec![Track {
                id: "track_sample_0".to_string(),
                name: "".to_string(),
                voice: 0,
                kind: TrackKind::SampleSustain {
                    sample_id: "sample_0001".to_string(),
                },
            }],
            m2: M2Block {
                active_sequence_id: None,
            },
        }
    }

    #[test]
    fn project_v2_round_trip_minimal() {
        round_trip(&minimal_project_v2());
    }

    #[test]
    fn track_kind_serializes_as_tagged_kind_field() {
        let t = Track {
            id: "t1".to_string(),
            name: "atom".to_string(),
            voice: 1,
            kind: TrackKind::AtomSequence {
                atom_sequence_id: "atomseq_0001".to_string(),
            },
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"kind\":\"atom_sequence\""), "{json}");
        assert!(
            json.contains("\"atom_sequence_id\":\"atomseq_0001\""),
            "{json}"
        );
    }

    #[test]
    fn atom_transition_round_trip_initial_kon() {
        round_trip(&AtomTransition::InitialKon);
    }

    #[test]
    fn atom_transition_round_trip_fade_to_zero_retrigger() {
        round_trip(&AtomTransition::FadeToZeroRetrigger {
            fade_out_ticks: 4,
            fade_in_ticks: 4,
        });
    }

    #[test]
    fn atom_sequence_round_trip_two_steps() {
        let s = AtomSequence {
            id: "atomseq_0001".to_string(),
            name: "two_step".to_string(),
            voice: 1,
            steps: vec![
                AtomSequenceStep {
                    atom_id: "atom_0001".to_string(),
                    duration_ticks: 120,
                    target_volume: 0.8,
                    transition: AtomTransition::InitialKon,
                },
                AtomSequenceStep {
                    atom_id: "atom_0002".to_string(),
                    duration_ticks: 120,
                    target_volume: 0.8,
                    transition: AtomTransition::FadeToZeroRetrigger {
                        fade_out_ticks: 4,
                        fade_in_ticks: 4,
                    },
                },
            ],
            looped: false,
        };
        round_trip(&s);
    }
}
