//! M2 voice setup table builder (SPEC §15.7).
//!
//! Per the spec the table has 11 bytes per voice, 2 voices for M2,
//! 22 bytes total. The driver init sequence consumes one entry per
//! voice and programs each voice's S-DSP registers from the
//! contained values.
//!
//! ```text
//! u8   voice              ; 0..=1 for M2 (sample voice / atom voice)
//! u8   src_index          ; index into source directory (0xFF = unused)
//! u16  pitch_le           ; SNES pitch register, little-endian
//! u8   vol_l              ; 0..=127
//! u8   vol_r              ; 0..=127
//! u8   adsr1
//! u8   adsr2
//! u8   gain
//! u8   flags              ; reserved = 0 in M2
//! ```
//!
//! Track → entry mapping:
//! - `kind: sample_sustain`: pull params from `sample_pool[sample_id]`.
//! - `kind: atom_sequence`: pull params from `atom_pool[steps[0].atom_id]`.
//!
//! Unused voices (a project with fewer than 2 active tracks) emit
//! `src_index=0xFF`, `flags=0`, all other fields 0; the driver skips
//! them based on the `0xFF` sentinel.
//!
//! `build_voice_setup_table` returns exactly
//! `VOICE_SETUP_TABLE_M2_BYTES` (= 22) bytes.

use thiserror::Error;

use crate::driver_build::playback_to_voll_volr;
use crate::packer::VOICE_SETUP_TABLE_M2_BYTES;
use crate::pitch::{pitch_register, split_pitch};
use crate::project::Envelope;
use crate::project_v2::{ProjectV2, TrackKind};

/// SPEC §15.7 unused-voice sentinel: driver leaves the voice silent.
pub const VOICE_UNUSED_SRC_INDEX: u8 = 0xFF;

#[derive(Debug, Error)]
pub enum VoiceSetupError {
    #[error("track {track_id:?}: sample_id {sample_id:?} not found in sample_pool")]
    SampleIdNotFound { track_id: String, sample_id: String },
    #[error("track {track_id:?}: atom_sequence_id {sequence_id:?} not found in atom_sequences")]
    AtomSequenceIdNotFound {
        track_id: String,
        sequence_id: String,
    },
    #[error("atom_sequence {sequence_id:?} has no steps; cannot derive voice playback")]
    AtomSequenceEmpty { sequence_id: String },
    #[error("track {track_id:?}: atom_id {atom_id:?} not found in atom_pool")]
    AtomIdNotFound { track_id: String, atom_id: String },
    #[error("track {track_id:?}: voice {voice} out of range 0..=1")]
    VoiceOutOfRange { track_id: String, voice: u8 },
    #[error("duplicate track on voice {voice} (validation should have caught this)")]
    DuplicateVoice { voice: u8 },
}

/// Build the 22-byte M2 voice setup table from a validated v2 project.
///
/// Pre-condition: `project.validate()` returned Ok — in particular,
/// rules 50/52/53/54 mean every track has a unique voice in 0..=1
/// and every referenced sample_id / atom_sequence_id resolves.
pub fn build_voice_setup_table(project: &ProjectV2) -> Result<Vec<u8>, VoiceSetupError> {
    let mut table = vec![0u8; VOICE_SETUP_TABLE_M2_BYTES as usize];
    let mut written: [bool; 2] = [false; 2];

    for track in &project.tracks {
        if track.voice > 1 {
            return Err(VoiceSetupError::VoiceOutOfRange {
                track_id: track.id.clone(),
                voice: track.voice,
            });
        }
        let voice = track.voice as usize;
        if written[voice] {
            return Err(VoiceSetupError::DuplicateVoice { voice: track.voice });
        }

        let entry = match &track.kind {
            TrackKind::SampleSustain { sample_id } => {
                let (idx, slot) = project
                    .sample_pool
                    .iter()
                    .enumerate()
                    .find(|(_, s)| &s.id == sample_id)
                    .ok_or_else(|| VoiceSetupError::SampleIdNotFound {
                        track_id: track.id.clone(),
                        sample_id: sample_id.clone(),
                    })?;
                let src_index = idx as u8;
                build_voice_entry(
                    track.voice,
                    src_index,
                    slot.source.sample_rate_hz,
                    slot.root_midi_note,
                    slot.playback.volume,
                    slot.playback.pan,
                    &slot.playback.envelope,
                )
            }
            TrackKind::AtomSequence { atom_sequence_id } => {
                let seq = project
                    .atom_sequences
                    .iter()
                    .find(|s| &s.id == atom_sequence_id)
                    .ok_or_else(|| VoiceSetupError::AtomSequenceIdNotFound {
                        track_id: track.id.clone(),
                        sequence_id: atom_sequence_id.clone(),
                    })?;
                let first_step =
                    seq.steps
                        .first()
                        .ok_or_else(|| VoiceSetupError::AtomSequenceEmpty {
                            sequence_id: atom_sequence_id.clone(),
                        })?;
                let (idx_in_pool, atom) = project
                    .atom_pool
                    .iter()
                    .enumerate()
                    .find(|(_, a)| a.id == first_step.atom_id)
                    .ok_or_else(|| VoiceSetupError::AtomIdNotFound {
                        track_id: track.id.clone(),
                        atom_id: first_step.atom_id.clone(),
                    })?;
                // SRCN for atoms = samples.len() + atom-pool index.
                let src_index = (project.sample_pool.len() + idx_in_pool) as u8;
                // Atom rendered PCM is at the source rate fixed at
                // 32 kHz (§16.7 atom source-rate convention; the
                // §16.9 render formula produces the cycle directly
                // at the project's audio rate, which is 32 kHz).
                build_voice_entry(
                    track.voice,
                    src_index,
                    32000,
                    atom.root_midi_note,
                    atom.playback.volume,
                    atom.playback.pan,
                    &atom.playback.envelope,
                )
            }
        };
        let off = voice * 11;
        table[off..off + 11].copy_from_slice(&entry);
        written[voice] = true;
    }

    // Unused voices: src_index = 0xFF sentinel, all other bytes 0.
    for (v, w) in written.iter().enumerate() {
        if !*w {
            let off = v * 11;
            table[off] = v as u8;
            table[off + 1] = VOICE_UNUSED_SRC_INDEX;
            // Bytes 2..=10 stay 0.
        }
    }

    Ok(table)
}

fn build_voice_entry(
    voice: u8,
    src_index: u8,
    source_sample_rate_hz: u32,
    root_midi_note: u8,
    volume: f64,
    pan: f64,
    envelope: &Envelope,
) -> [u8; 11] {
    let pitch_u16 = pitch_register(source_sample_rate_hz, root_midi_note, root_midi_note, 0);
    let (pitchl, pitchh) = split_pitch(pitch_u16);
    let (vol_l, vol_r) = playback_to_voll_volr(volume, pan);
    let (adsr1, adsr2, gain) = match envelope {
        Envelope::Adsr {
            attack,
            decay,
            sustain_level,
            sustain_rate,
        } => {
            let a1 = 0x80 | ((decay & 0x07) << 4) | (attack & 0x0F);
            let a2 = ((sustain_level & 0x07) << 5) | (sustain_rate & 0x1F);
            (a1, a2, 0x00)
        }
        Envelope::GainRaw { gain_byte } => (0x00, 0x00, *gain_byte),
    };
    [
        voice, src_index, pitchl, pitchh, vol_l, vol_r, adsr1, adsr2, gain,
        0x00, // flags reserved = 0 in M2
        0x00, // pad to 11 bytes — TODO M3+: split flags into two if needed
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atom::{AtomKind, AtomPartial, AtomRenderOptions, AtomSlot};
    use crate::project::{
        Driver, MasterEcho, Project, SampleFormat, SampleLoop, SamplePlayback, SampleSlot,
        SampleSource,
    };
    use crate::project_v2::{
        AtomSequence, AtomSequenceStep, AtomTransition, M2Block, ProjectV2, Track, TrackKind,
    };

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
                frames: 1024,
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

    fn atom(id: &str) -> AtomSlot {
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
            cycle_len_samples: 128,
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

    fn project_v2_sample_only(sample_id: &str) -> ProjectV2 {
        ProjectV2 {
            schema_version: 2,
            project: Project {
                name: "test".to_string(),
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
            sample_pool: vec![sample(sample_id)],
            atom_pool: Vec::new(),
            atom_sequences: Vec::new(),
            tracks: vec![Track {
                id: "track_sample_0".to_string(),
                name: String::new(),
                voice: 0,
                kind: TrackKind::SampleSustain {
                    sample_id: sample_id.to_string(),
                },
            }],
            m2: M2Block {
                active_sequence_id: None,
            },
        }
    }

    fn project_v2_multi_voice() -> ProjectV2 {
        let mut p = project_v2_sample_only("lead");
        p.driver.profile = "multi_voice_atom".to_string();
        p.driver.bytecode_version = 2;
        p.atom_pool = vec![atom("atom_0001"), atom("atom_0002")];
        p.atom_sequences = vec![AtomSequence {
            id: "atomseq_0001".to_string(),
            name: "single".to_string(),
            voice: 1,
            steps: vec![AtomSequenceStep {
                atom_id: "atom_0001".to_string(),
                duration_ticks: 120,
                target_volume: 0.8,
                transition: AtomTransition::InitialKon,
            }],
            looped: false,
        }];
        p.tracks.push(Track {
            id: "track_atom_1".to_string(),
            name: String::new(),
            voice: 1,
            kind: TrackKind::AtomSequence {
                atom_sequence_id: "atomseq_0001".to_string(),
            },
        });
        p.m2.active_sequence_id = Some("atomseq_0001".to_string());
        p
    }

    #[test]
    fn table_is_22_bytes() {
        let p = project_v2_multi_voice();
        let t = build_voice_setup_table(&p).expect("build");
        assert_eq!(t.len(), 22);
    }

    #[test]
    fn voice_field_offsets_match_spec_15_7() {
        let p = project_v2_multi_voice();
        let t = build_voice_setup_table(&p).expect("build");
        // Voice 0: sample track on voice 0, SRCN 0 (sample_pool[0]).
        assert_eq!(t[0], 0, "voice 0 byte");
        assert_eq!(t[1], 0, "voice 0 src_index = 0 (samples first)");
        // Voice 1: atom track on voice 1, SRCN = samples.len() + atom-pool index = 1 + 0 = 1.
        assert_eq!(t[11], 1, "voice 1 byte");
        assert_eq!(
            t[12], 1,
            "voice 1 src_index = samples.len() + atom-pool index"
        );
    }

    #[test]
    fn unused_voice_uses_src_index_ff_sentinel() {
        let p = project_v2_sample_only("lead");
        let t = build_voice_setup_table(&p).expect("build");
        // Voice 0 written, voice 1 unused.
        assert_eq!(t[0], 0);
        assert_eq!(t[1], 0); // SRCN 0 for the lead sample.
        assert_eq!(t[11], 1, "voice 1 byte still 1 even when unused");
        assert_eq!(t[12], VOICE_UNUSED_SRC_INDEX);
        // Bytes 13..22 are zero.
        for (i, byte) in t.iter().enumerate().skip(13).take(9) {
            assert_eq!(*byte, 0, "unused voice byte {i}");
        }
    }

    #[test]
    fn atom_voice_pulls_atom_playback_params() {
        // Force the atom's volume to a distinctive value so we can
        // see it in the resulting vol_l/vol_r bytes.
        let mut p = project_v2_multi_voice();
        p.atom_pool[0].playback.volume = 0.5;
        let t = build_voice_setup_table(&p).expect("build");
        // Voice 1 vol_l/vol_r at offsets 11+4=15, 11+5=16.
        let (expected_l, expected_r) = playback_to_voll_volr(0.5, 0.0);
        assert_eq!(t[15], expected_l, "voice 1 vol_l from atom playback");
        assert_eq!(t[16], expected_r, "voice 1 vol_r from atom playback");
    }

    #[test]
    fn flags_reserved_zero_in_m2() {
        let p = project_v2_multi_voice();
        let t = build_voice_setup_table(&p).expect("build");
        // flags is byte 9 of each entry (offset 9 for voice 0, 11+9=20 for voice 1).
        assert_eq!(t[9], 0);
        assert_eq!(t[20], 0);
    }
}
