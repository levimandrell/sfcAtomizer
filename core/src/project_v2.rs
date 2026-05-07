//! Project schema v2 — full type tree + validation + migration.
//!
//! SPEC §16.9 (schema v2) and §16.10 (v1 → v2 migration table).
//! Validation rules 1–25 are inherited from v1 (sample_pool /
//! master_echo / project / driver.bytecode_version / source SHA hex /
//! root_midi_note / loop / playback / envelope). Rules 26–57 are v2
//! additions covering `schema_version`, the `multi_voice_atom`
//! profile coupling, atom pool, atom sequences, tracks, the m2 block,
//! and the cross-cutting profile/feature rules.
//!
//! Migration is one-way (v1 → v2). Load-time silent upgrades are
//! forbidden per §16.10; the host pipeline routes versioned loads
//! through `core::project_v2::load_project_versioned` (M2.1).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atom::{AtomKind, AtomSlot, AtomPartial};
use crate::project::{
    is_valid_id, validate_sample_slot, validate_string_field, Driver, Envelope, MasterEcho,
    Project, ProjectIoError, ProjectV1, SampleSlot, ValidationError, ValidationErrorKind,
};

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
    /// Apply SPEC §16.9 validation rules. Multi-error collection: every
    /// failed rule produces a `ValidationError { path, kind }` and we
    /// continue, so the UI can show every problem at once.
    ///
    /// Rules 1–25 carry forward from v1 (per-sample, master_echo,
    /// project block, source SHA, root_midi_note, loop, playback,
    /// envelope). Rules 26–57 are v2 additions: schema_version,
    /// `multi_voice_atom` profile coupling, atom pool, atom sequences,
    /// tracks, the m2 block, and the cross-cutting profile/feature
    /// rules.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors: Vec<ValidationError> = Vec::new();

        // Rule 26: schema_version == 2.
        if self.schema_version != Self::SCHEMA_VERSION_M2 {
            errors.push(ValidationError {
                path: "/schema_version".to_string(),
                kind: ValidationErrorKind::SchemaVersionUnsupported {
                    expected: Self::SCHEMA_VERSION_M2,
                    actual: self.schema_version,
                },
            });
        }

        // Rule 2 (carry-forward): project.tick_rate_hz == 60.
        if self.project.tick_rate_hz != 60 {
            errors.push(ValidationError {
                path: "/project/tick_rate_hz".to_string(),
                kind: ValidationErrorKind::TickRateUnsupported(self.project.tick_rate_hz),
            });
        }
        // Rule 3 (carry-forward): project.name.
        validate_string_field(&mut errors, "/project/name", &self.project.name, true);

        // Rule 27: driver.profile in {sample_basic, multi_voice_atom}.
        let profile_ok = matches!(
            self.driver.profile.as_str(),
            "sample_basic" | "multi_voice_atom"
        );
        if !profile_ok {
            errors.push(ValidationError {
                path: "/driver/profile".to_string(),
                kind: ValidationErrorKind::DriverProfileUnsupportedV2(self.driver.profile.clone()),
            });
        }
        // Rule 28: bytecode_version coupling. Skip when profile is
        // unrecognised (rule 27 already flagged it).
        if profile_ok {
            let expected_bc = match self.driver.profile.as_str() {
                "sample_basic" => 1,
                "multi_voice_atom" => 2,
                _ => unreachable!(),
            };
            if self.driver.bytecode_version != expected_bc {
                errors.push(ValidationError {
                    path: "/driver/bytecode_version".to_string(),
                    kind: ValidationErrorKind::DriverBytecodeProfileMismatch {
                        profile: self.driver.profile.clone(),
                        bytecode: self.driver.bytecode_version,
                        expected: expected_bc,
                    },
                });
            }
        }

        // Rule 6 / 7 / 8 / 22 (carry-forward): master_echo + per-sample
        // echo coupling. Same path/kind shape as v1.
        if self.master_echo.edl > 15 {
            errors.push(ValidationError {
                path: "/master_echo/edl".to_string(),
                kind: ValidationErrorKind::IntegerOutOfRange {
                    value: self.master_echo.edl as i64,
                    min: 0,
                    max: 15,
                },
            });
        }
        if let Err(echo_errors) =
            crate::echo_validation::validate_echo(&self.master_echo, &self.sample_pool)
        {
            for e in echo_errors {
                use crate::echo_validation::EchoConfigError as Eee;
                let (path, kind) = match e {
                    Eee::EnabledRequiresEdlInRange(edl) => (
                        "/master_echo/edl".to_string(),
                        ValidationErrorKind::MasterEchoEnabledRequiresEdl(edl),
                    ),
                    Eee::DisabledRequiresZeroEdl(edl) => (
                        "/master_echo/edl".to_string(),
                        ValidationErrorKind::MasterEchoDisabledRequiresZeroEdl(edl),
                    ),
                    Eee::SampleEchoWithoutMaster(id) => {
                        let idx = self
                            .sample_pool
                            .iter()
                            .position(|s| s.id == id)
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "?".to_string());
                        (
                            format!("/sample_pool/{idx}/playback/echo"),
                            ValidationErrorKind::SampleEchoWithoutMaster,
                        )
                    }
                };
                errors.push(ValidationError { path, kind });
            }
        }

        // Rule 9 (carry-forward): sample_pool length 1..=128.
        let pool_len = self.sample_pool.len();
        if !(1..=128).contains(&pool_len) {
            errors.push(ValidationError {
                path: "/sample_pool".to_string(),
                kind: ValidationErrorKind::SamplePoolLength(pool_len),
            });
        }

        // Rules 10–24 (carry-forward): per-sample.
        let mut seen_sample_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (i, s) in self.sample_pool.iter().enumerate() {
            let prefix = format!("/sample_pool/{i}");
            validate_sample_slot(&prefix, s, &mut errors);
            if !seen_sample_ids.insert(&s.id) {
                errors.push(ValidationError {
                    path: format!("{prefix}/id"),
                    kind: ValidationErrorKind::DuplicateSampleId(s.id.clone()),
                });
            }
        }

        // Rules 29–39: atom_pool.
        let mut seen_atom_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (i, a) in self.atom_pool.iter().enumerate() {
            let prefix = format!("/atom_pool/{i}");
            validate_atom_slot(&prefix, a, &mut errors);
            // Rule 29: id pattern + uniqueness within atom_pool.
            if !seen_atom_ids.insert(&a.id) {
                errors.push(ValidationError {
                    path: format!("{prefix}/id"),
                    kind: ValidationErrorKind::DuplicateAtomId(a.id.clone()),
                });
            }
            // Rule 30: cross-pool id collision.
            if seen_sample_ids.contains(a.id.as_str()) {
                errors.push(ValidationError {
                    path: format!("{prefix}/id"),
                    kind: ValidationErrorKind::SampleAtomIdCollision(a.id.clone()),
                });
            }
        }

        // Rules 40–48: atom_sequences.
        let mut seen_seq_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (i, sq) in self.atom_sequences.iter().enumerate() {
            let prefix = format!("/atom_sequences/{i}");
            validate_atom_sequence(&prefix, sq, &seen_atom_ids, &mut errors);
            if !seen_seq_ids.insert(&sq.id) {
                errors.push(ValidationError {
                    path: format!("{prefix}/id"),
                    kind: ValidationErrorKind::DuplicateAtomSequenceId(sq.id.clone()),
                });
            }
        }

        // Rules 49–54: tracks.
        let mut seen_track_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let mut seen_track_voices: std::collections::HashSet<u8> = std::collections::HashSet::new();
        for (i, t) in self.tracks.iter().enumerate() {
            let prefix = format!("/tracks/{i}");
            validate_track(
                &prefix,
                t,
                &seen_sample_ids,
                &seen_seq_ids,
                &self.atom_sequences,
                &mut errors,
            );
            if !seen_track_ids.insert(&t.id) {
                errors.push(ValidationError {
                    path: format!("{prefix}/id"),
                    kind: ValidationErrorKind::DuplicateTrackId(t.id.clone()),
                });
            }
            if !seen_track_voices.insert(t.voice) {
                errors.push(ValidationError {
                    path: format!("{prefix}/voice"),
                    kind: ValidationErrorKind::DuplicateTrackVoice(t.voice),
                });
            }
        }

        // Rule 55: m2.active_sequence_id null OR matches a sequence.
        if let Some(active) = &self.m2.active_sequence_id {
            if !self.atom_sequences.iter().any(|s| &s.id == active) {
                errors.push(ValidationError {
                    path: "/m2/active_sequence_id".to_string(),
                    kind: ValidationErrorKind::ActiveAtomSequenceNotFound(active.clone()),
                });
            }
        }

        // Rule 56: sample_basic forbids atom data.
        if self.driver.profile == "sample_basic" {
            let any_atom_data = !self.atom_pool.is_empty()
                || !self.atom_sequences.is_empty()
                || self
                    .tracks
                    .iter()
                    .any(|t| matches!(t.kind, TrackKind::AtomSequence { .. }))
                || self.tracks.iter().any(|t| t.voice >= 1);
            if any_atom_data {
                errors.push(ValidationError {
                    path: "/driver/profile".to_string(),
                    kind: ValidationErrorKind::SampleBasicForbidsAtomData,
                });
            }
        }

        // Rule 57: multi_voice_atom requires at least one
        // atom_sequence track.
        if self.driver.profile == "multi_voice_atom" {
            let has_atom_track = self
                .tracks
                .iter()
                .any(|t| matches!(t.kind, TrackKind::AtomSequence { .. }));
            if !has_atom_track {
                errors.push(ValidationError {
                    path: "/tracks".to_string(),
                    kind: ValidationErrorKind::MultiVoiceAtomRequiresAtomSequenceTrack,
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn validate_atom_slot(prefix: &str, a: &AtomSlot, errors: &mut Vec<ValidationError>) {
    // Rule 29: id pattern + length.
    if !is_valid_id(&a.id) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::AtomIdPatternMismatch(a.id.clone()),
        });
    }
    if !(1..=64).contains(&a.id.chars().count()) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::StringLength {
                len: a.id.chars().count(),
                min: 1,
                max: 64,
            },
        });
    }

    // Atom name uses the same rules as sample name (no control chars,
    // path separators OK — atom names are display-only).
    validate_string_field(errors, &format!("{prefix}/name"), &a.name, false);

    // Rule 31: kind == additive_single_cycle_v0 (M2 only).
    let AtomKind::AdditiveSingleCycleV0 { partials } = &a.kind;

    // Rule 32: cycle_len_samples in {64, 128, 256}.
    if !matches!(a.cycle_len_samples, 64 | 128 | 256) {
        errors.push(ValidationError {
            path: format!("{prefix}/cycle_len_samples"),
            kind: ValidationErrorKind::AtomCycleLenUnsupported(a.cycle_len_samples),
        });
    }

    // Rule 33: partials length 1..=8.
    if !(1..=8).contains(&partials.len()) {
        errors.push(ValidationError {
            path: format!("{prefix}/partials"),
            kind: ValidationErrorKind::AtomPartialsLength(partials.len()),
        });
    }

    // Rules 34–36: per-partial.
    for (j, p) in partials.iter().enumerate() {
        let pp = format!("{prefix}/partials/{j}");
        validate_atom_partial(&pp, p, errors);
    }

    // Rule 37: top-level amplitude in 0.0..=1.0.
    validate_unit_float(&format!("{prefix}/amplitude"), a.amplitude, errors);

    // Rule 38: root_midi_note 0..=127.
    if a.root_midi_note > 127 {
        errors.push(ValidationError {
            path: format!("{prefix}/root_midi_note"),
            kind: ValidationErrorKind::IntegerOutOfRange {
                value: a.root_midi_note as i64,
                min: 0,
                max: 127,
            },
        });
    }

    // Rule 39: playback (same as sample playback).
    validate_playback(&format!("{prefix}/playback"), &a.playback, errors);
}

fn validate_atom_partial(prefix: &str, p: &AtomPartial, errors: &mut Vec<ValidationError>) {
    // Rule 34: harmonic 1..=16.
    if !(1..=16).contains(&p.harmonic) {
        errors.push(ValidationError {
            path: format!("{prefix}/harmonic"),
            kind: ValidationErrorKind::AtomPartialHarmonicOutOfRange(p.harmonic),
        });
    }
    // Rule 35: amplitude 0.0..=1.0; reject NaN.
    validate_unit_float(&format!("{prefix}/amplitude"), p.amplitude, errors);
    // Rule 36: phase_cycles in [0.0, 1.0).
    if p.phase_cycles.is_nan() {
        errors.push(ValidationError {
            path: format!("{prefix}/phase_cycles"),
            kind: ValidationErrorKind::NaN,
        });
    } else if !(0.0..1.0).contains(&p.phase_cycles) {
        errors.push(ValidationError {
            path: format!("{prefix}/phase_cycles"),
            kind: ValidationErrorKind::AtomPartialPhaseOutOfRange(p.phase_cycles),
        });
    }
}

fn validate_unit_float(path: &str, v: f64, errors: &mut Vec<ValidationError>) {
    if v.is_nan() {
        errors.push(ValidationError {
            path: path.to_string(),
            kind: ValidationErrorKind::NaN,
        });
    } else if !(0.0..=1.0).contains(&v) {
        errors.push(ValidationError {
            path: path.to_string(),
            kind: ValidationErrorKind::FloatOutOfRange {
                value: v,
                min: 0.0,
                max: 1.0,
            },
        });
    }
}

fn validate_playback(
    prefix: &str,
    pb: &crate::project::SamplePlayback,
    errors: &mut Vec<ValidationError>,
) {
    // Mirror of sample-playback rules 20 / 21 / 23 / 24. Rule 22 is
    // applied at project level via echo_validation, not here.
    if pb.volume.is_nan() {
        errors.push(ValidationError {
            path: format!("{prefix}/volume"),
            kind: ValidationErrorKind::NaN,
        });
    } else if !(0.0..=1.0).contains(&pb.volume) {
        errors.push(ValidationError {
            path: format!("{prefix}/volume"),
            kind: ValidationErrorKind::FloatOutOfRange {
                value: pb.volume,
                min: 0.0,
                max: 1.0,
            },
        });
    }
    if pb.pan.is_nan() {
        errors.push(ValidationError {
            path: format!("{prefix}/pan"),
            kind: ValidationErrorKind::NaN,
        });
    } else if !(-1.0..=1.0).contains(&pb.pan) {
        errors.push(ValidationError {
            path: format!("{prefix}/pan"),
            kind: ValidationErrorKind::FloatOutOfRange {
                value: pb.pan,
                min: -1.0,
                max: 1.0,
            },
        });
    }
    match &pb.envelope {
        Envelope::Adsr {
            attack,
            decay,
            sustain_level,
            sustain_rate,
        } => {
            if *attack > 15 {
                errors.push(ValidationError {
                    path: format!("{prefix}/envelope/attack"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *attack as i64,
                        min: 0,
                        max: 15,
                    },
                });
            }
            if *decay > 7 {
                errors.push(ValidationError {
                    path: format!("{prefix}/envelope/decay"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *decay as i64,
                        min: 0,
                        max: 7,
                    },
                });
            }
            if *sustain_level > 7 {
                errors.push(ValidationError {
                    path: format!("{prefix}/envelope/sustain_level"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *sustain_level as i64,
                        min: 0,
                        max: 7,
                    },
                });
            }
            if *sustain_rate > 31 {
                errors.push(ValidationError {
                    path: format!("{prefix}/envelope/sustain_rate"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *sustain_rate as i64,
                        min: 0,
                        max: 31,
                    },
                });
            }
        }
        Envelope::GainRaw { .. } => {}
    }
}

fn validate_atom_sequence(
    prefix: &str,
    sq: &AtomSequence,
    seen_atom_ids: &std::collections::HashSet<&str>,
    errors: &mut Vec<ValidationError>,
) {
    // Rule 40: id pattern + length.
    if !is_valid_id(&sq.id) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::AtomSequenceIdPatternMismatch(sq.id.clone()),
        });
    }
    if !(1..=64).contains(&sq.id.chars().count()) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::StringLength {
                len: sq.id.chars().count(),
                min: 1,
                max: 64,
            },
        });
    }

    // Sequence name: same rule as sample/atom name.
    validate_string_field(errors, &format!("{prefix}/name"), &sq.name, false);

    // Rule 41: voice 0..=1 (M2). u8 already constrains 0..=255.
    if sq.voice > 1 {
        errors.push(ValidationError {
            path: format!("{prefix}/voice"),
            kind: ValidationErrorKind::IntegerOutOfRange {
                value: sq.voice as i64,
                min: 0,
                max: 1,
            },
        });
    }

    // Rule 42: steps length 1..=32.
    if !(1..=32).contains(&sq.steps.len()) {
        errors.push(ValidationError {
            path: format!("{prefix}/steps"),
            kind: ValidationErrorKind::AtomSequenceStepsLength(sq.steps.len()),
        });
    }

    for (j, step) in sq.steps.iter().enumerate() {
        let sp = format!("{prefix}/steps/{j}");

        // Rule 43: duration_ticks 1..=255 (u8 already constrains the
        // upper bound; we just guard against zero).
        if step.duration_ticks == 0 {
            errors.push(ValidationError {
                path: format!("{sp}/duration_ticks"),
                kind: ValidationErrorKind::IntegerOutOfRange {
                    value: 0,
                    min: 1,
                    max: 255,
                },
            });
        }

        // Rule 44: target_volume 0.0..=1.0.
        validate_unit_float(&format!("{sp}/target_volume"), step.target_volume, errors);

        // Rule 45: atom_id present in atom_pool.
        if !seen_atom_ids.contains(step.atom_id.as_str()) {
            errors.push(ValidationError {
                path: format!("{sp}/atom_id"),
                kind: ValidationErrorKind::AtomSequenceStepAtomIdNotFound {
                    idx: j,
                    id: step.atom_id.clone(),
                },
            });
        }

        // Rules 46 / 47: transition kind by step index.
        match (j, &step.transition) {
            (0, AtomTransition::InitialKon) => {}
            (0, AtomTransition::FadeToZeroRetrigger { .. }) => {
                errors.push(ValidationError {
                    path: format!("{sp}/transition/type"),
                    kind: ValidationErrorKind::AtomSequenceFirstStepMustBeInitialKon(
                        "fade_to_zero_retrigger".to_string(),
                    ),
                });
            }
            (
                _,
                AtomTransition::FadeToZeroRetrigger {
                    fade_out_ticks,
                    fade_in_ticks,
                },
            ) => {
                // Rule 48: fade_out_ticks / fade_in_ticks 1..=255.
                if *fade_out_ticks == 0 {
                    errors.push(ValidationError {
                        path: format!("{sp}/transition/fade_out_ticks"),
                        kind: ValidationErrorKind::IntegerOutOfRange {
                            value: 0,
                            min: 1,
                            max: 255,
                        },
                    });
                }
                if *fade_in_ticks == 0 {
                    errors.push(ValidationError {
                        path: format!("{sp}/transition/fade_in_ticks"),
                        kind: ValidationErrorKind::IntegerOutOfRange {
                            value: 0,
                            min: 1,
                            max: 255,
                        },
                    });
                }
            }
            (_, AtomTransition::InitialKon) => {
                errors.push(ValidationError {
                    path: format!("{sp}/transition/type"),
                    kind: ValidationErrorKind::AtomSequenceLaterStepWrongTransition {
                        idx: j,
                        got: "initial_kon".to_string(),
                    },
                });
            }
        }
    }
}

fn validate_track(
    prefix: &str,
    t: &Track,
    seen_sample_ids: &std::collections::HashSet<&str>,
    seen_seq_ids: &std::collections::HashSet<&str>,
    sequences: &[AtomSequence],
    errors: &mut Vec<ValidationError>,
) {
    // Rule 49: track id pattern + length.
    if !is_valid_id(&t.id) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::TrackIdPatternMismatch(t.id.clone()),
        });
    }
    if !(1..=64).contains(&t.id.chars().count()) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::StringLength {
                len: t.id.chars().count(),
                min: 1,
                max: 64,
            },
        });
    }

    // Track name optional but, if present, follows sample-name rules.
    if !t.name.is_empty() {
        validate_string_field(errors, &format!("{prefix}/name"), &t.name, false);
    }

    // Rule 50: voice 0..=1.
    if t.voice > 1 {
        errors.push(ValidationError {
            path: format!("{prefix}/voice"),
            kind: ValidationErrorKind::IntegerOutOfRange {
                value: t.voice as i64,
                min: 0,
                max: 1,
            },
        });
    }

    // Rules 51–54: kind-specific id + voice consistency.
    match &t.kind {
        TrackKind::SampleSustain { sample_id } => {
            if !seen_sample_ids.contains(sample_id.as_str()) {
                errors.push(ValidationError {
                    path: format!("{prefix}/sample_id"),
                    kind: ValidationErrorKind::TrackSampleIdNotFound(sample_id.clone()),
                });
            }
        }
        TrackKind::AtomSequence { atom_sequence_id } => {
            if !seen_seq_ids.contains(atom_sequence_id.as_str()) {
                errors.push(ValidationError {
                    path: format!("{prefix}/atom_sequence_id"),
                    kind: ValidationErrorKind::TrackAtomSequenceIdNotFound(
                        atom_sequence_id.clone(),
                    ),
                });
            } else if let Some(sq) = sequences.iter().find(|s| s.id == *atom_sequence_id) {
                // Rule 54: track voice == sequence voice.
                if t.voice != sq.voice {
                    errors.push(ValidationError {
                        path: format!("{prefix}/voice"),
                        kind: ValidationErrorKind::TrackAtomSequenceVoiceMismatch {
                            track_voice: t.voice,
                            sequence_voice: sq.voice,
                        },
                    });
                }
            }
        }
    }
}

/// Migrate a v1 [`ProjectV1`] to v2 per SPEC §16.10. Pure
/// transformation; the caller is expected to validate the result.
///
/// - `schema_version`: 1 → 2
/// - `project` / `master_echo` / `sample_pool` / `driver`: carry forward
/// - `atom_pool`, `atom_sequences`: added empty
/// - `tracks`: added with a single voice-0 `sample_sustain` track
///   pointing at `m1.active_sample_id`
/// - `m1`: dropped (the migration log records the
///   `active_sample_id → tracks[0].sample_id` mapping)
/// - `m2`: added with `active_sequence_id: None`
pub fn migrate_from_v1(v1: &ProjectV1) -> ProjectV2 {
    ProjectV2 {
        schema_version: ProjectV2::SCHEMA_VERSION_M2,
        project: v1.project.clone(),
        driver: Driver {
            profile: v1.driver.profile.clone(),
            bytecode_version: v1.driver.bytecode_version,
        },
        master_echo: v1.master_echo.clone(),
        sample_pool: v1.sample_pool.clone(),
        atom_pool: Vec::new(),
        atom_sequences: Vec::new(),
        tracks: vec![Track {
            id: "track_sample_0".to_string(),
            name: "Migrated sample".to_string(),
            voice: 0,
            kind: TrackKind::SampleSustain {
                sample_id: v1.m1.active_sample_id.clone(),
            },
        }],
        m2: M2Block {
            active_sequence_id: None,
        },
    }
}

// =============================================================================
// I/O — load / save / versioned dispatch
// =============================================================================

impl ProjectV2 {
    /// Read + parse a v2 project file. **Does not validate** —
    /// callers that need a guaranteed-valid project should chain
    /// [`Self::validate`] explicitly. Mirrors the two-step v1 API so
    /// the GUI viewer can render an invalid v2 project alongside its
    /// validation errors instead of refusing to load it.
    pub fn load_from_path(path: &Path) -> Result<Self, ProjectIoError> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ProjectIoError::NotFound {
                    path: path.to_path_buf(),
                });
            }
            Err(e) => {
                return Err(ProjectIoError::Io {
                    path: path.to_path_buf(),
                    source: e,
                });
            }
        };
        let v: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|source| ProjectIoError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        let version = v
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or(ProjectIoError::MalformedValue)?;
        if version != Self::SCHEMA_VERSION_M2 as u64 {
            return Err(ProjectIoError::UnsupportedSchemaVersion {
                expected: Self::SCHEMA_VERSION_M2,
                actual: version as u32,
            });
        }
        serde_json::from_value(v).map_err(|source| ProjectIoError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Convenience: [`Self::load_from_path`] then [`Self::validate`],
    /// with validation errors lifted into [`ProjectIoError::Validation`].
    pub fn load_and_validate(path: &Path) -> Result<Self, ProjectIoError> {
        let p = Self::load_from_path(path)?;
        p.validate().map_err(ProjectIoError::Validation)?;
        Ok(p)
    }

    /// Serialize `self` to `path` as pretty-printed JSON with a
    /// trailing newline. Stable key ordering (declaration order) is
    /// guaranteed because every nested type is a struct, not a map.
    /// Validates pre-save: refuses to write an invalid v2 project.
    pub fn save_to_path(&self, path: &Path) -> Result<(), ProjectIoError> {
        self.validate().map_err(ProjectIoError::Validation)?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| ProjectIoError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let mut s = serde_json::to_string_pretty(self).map_err(|source| ProjectIoError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        s.push('\n');
        std::fs::write(path, s).map_err(|source| ProjectIoError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }
}

/// Versioned loader result: the host pipeline holds the variant that
/// was actually loaded; callers decide whether to dispatch v1-only
/// behaviour, v2-only behaviour, or treat them uniformly via the
/// v1-equivalent compile path (see SPEC §16.10).
#[derive(Debug, Clone)]
pub enum LoadedProject {
    V1(ProjectV1),
    V2(ProjectV2),
}

impl LoadedProject {
    /// Schema version of the loaded project (1 or 2).
    pub fn schema_version(&self) -> u32 {
        match self {
            LoadedProject::V1(_) => 1,
            LoadedProject::V2(_) => 2,
        }
    }
}

/// Read a project file and dispatch by `schema_version`. Loads v1 as
/// [`ProjectV1`] and v2 as [`ProjectV2`]; any other value rejects via
/// [`ProjectIoError::UnsupportedSchemaVersion`]. Does **not** validate
/// either variant — chain `validate()` on the inner project if you
/// need a guaranteed-valid result.
///
/// Per SPEC §16.10 there are no silent load-time upgrades. v1 → v2
/// migration is explicit (`migrate_from_v1` + the `migrate-project`
/// CLI / GUI menu item).
pub fn load_project_versioned(path: &Path) -> Result<LoadedProject, ProjectIoError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProjectIoError::NotFound {
                path: path.to_path_buf(),
            });
        }
        Err(e) => {
            return Err(ProjectIoError::Io {
                path: path.to_path_buf(),
                source: e,
            });
        }
    };
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|source| ProjectIoError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    let version = v
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(ProjectIoError::MalformedValue)?;
    match version {
        1 => {
            let p: ProjectV1 = serde_json::from_value(v).map_err(|source| ProjectIoError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
            Ok(LoadedProject::V1(p))
        }
        2 => {
            let p: ProjectV2 = serde_json::from_value(v).map_err(|source| ProjectIoError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
            Ok(LoadedProject::V2(p))
        }
        other => Err(ProjectIoError::UnsupportedSchemaVersion {
            expected: ProjectV1::SCHEMA_VERSION_M1,
            actual: other as u32,
        }),
    }
}

// =============================================================================
// Migration report (SPEC §16.10)
// =============================================================================

/// Structured record of every transformation applied during a v1 → v2
/// migration. Written to disk alongside the migrated project so PM
/// can review the change before accepting it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationReport {
    pub schema_version: u32,
    pub report_type: String,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub source_schema_version: u32,
    pub target_schema_version: u32,
    pub transformations: Vec<Transformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transformation {
    /// JSON-pointer-style path of the field that was transformed.
    pub path: String,
    /// `"added"` | `"dropped"` | `"moved"` | `"preserved"`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl MigrationReport {
    /// Build the canonical v1 → v2 migration report for the given
    /// source/target paths. Independent of the migration itself —
    /// `migrate_from_v1` is a pure transformation; this report
    /// describes what `migrate_from_v1` did, without re-doing it.
    pub fn for_v1_to_v2(source_path: PathBuf, target_path: PathBuf, v1: &ProjectV1) -> Self {
        Self {
            schema_version: 1,
            report_type: "migration_v1_to_v2".to_string(),
            source_path,
            target_path,
            source_schema_version: 1,
            target_schema_version: 2,
            transformations: vec![
                Transformation {
                    path: "/schema_version".to_string(),
                    kind: "moved".to_string(),
                    note: Some("1 -> 2".to_string()),
                },
                Transformation {
                    path: "/project".to_string(),
                    kind: "preserved".to_string(),
                    note: None,
                },
                Transformation {
                    path: "/driver".to_string(),
                    kind: "preserved".to_string(),
                    note: None,
                },
                Transformation {
                    path: "/master_echo".to_string(),
                    kind: "preserved".to_string(),
                    note: None,
                },
                Transformation {
                    path: "/sample_pool".to_string(),
                    kind: "preserved".to_string(),
                    note: Some(format!("{} sample(s)", v1.sample_pool.len())),
                },
                Transformation {
                    path: "/atom_pool".to_string(),
                    kind: "added".to_string(),
                    note: Some("empty".to_string()),
                },
                Transformation {
                    path: "/atom_sequences".to_string(),
                    kind: "added".to_string(),
                    note: Some("empty".to_string()),
                },
                Transformation {
                    path: "/tracks".to_string(),
                    kind: "added".to_string(),
                    note: Some(format!(
                        "voice 0 sample_sustain track \"track_sample_0\" -> sample_id={:?}",
                        v1.m1.active_sample_id
                    )),
                },
                Transformation {
                    path: "/m2".to_string(),
                    kind: "added".to_string(),
                    note: Some("active_sequence_id=null".to_string()),
                },
                Transformation {
                    path: "/m1".to_string(),
                    kind: "dropped".to_string(),
                    note: Some(
                        "active_sample_id mapped to /tracks/0/sample_id".to_string(),
                    ),
                },
            ],
        }
    }
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

    // ========================================================================
    // M2.1 — validation, migration, IO round-trip, versioned dispatch.
    // ========================================================================

    use crate::atom::{AtomKind, AtomPartial, AtomRenderOptions};
    use crate::project::{M1Block, ProjectIoError};

    fn valid_sample_slot(id: &str) -> SampleSlot {
        SampleSlot {
            id: id.to_string(),
            name: format!("slot_{id}"),
            source: SampleSource {
                path: format!("audio/{id}.wav"),
                sha256: "0".repeat(64),
                format: SampleFormat::Wav,
                sample_rate_hz: 32000,
                channels: 1,
                frames: 65536,
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

    fn valid_atom_slot(id: &str) -> AtomSlot {
        AtomSlot {
            id: id.to_string(),
            name: format!("atom_{id}"),
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

    /// Sample-only v2 project (driver=sample_basic, bytecode_version=1,
    /// empty atom data, single voice-0 sample_sustain track). Mirrors
    /// what `migrate_from_v1` produces from a typical v1 project.
    fn valid_v2_sample_only() -> ProjectV2 {
        ProjectV2 {
            schema_version: 2,
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
                fir: [127, 0, 0, 0, 0, 0, 0, 0],
            },
            sample_pool: vec![valid_sample_slot("sample_0001")],
            atom_pool: Vec::new(),
            atom_sequences: Vec::new(),
            tracks: vec![Track {
                id: "track_sample_0".to_string(),
                name: String::new(),
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

    /// multi_voice_atom v2 project with both pools populated and one
    /// atom_sequence + one sample track. Hits every cross-coupling
    /// rule.
    fn valid_v2_multi_voice_atom() -> ProjectV2 {
        ProjectV2 {
            schema_version: 2,
            project: Project {
                name: "atomic".to_string(),
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
            sample_pool: vec![valid_sample_slot("sample_0001")],
            atom_pool: vec![valid_atom_slot("atom_0001"), valid_atom_slot("atom_0002")],
            atom_sequences: vec![AtomSequence {
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
            }],
            tracks: vec![
                Track {
                    id: "track_sample_0".to_string(),
                    name: String::new(),
                    voice: 0,
                    kind: TrackKind::SampleSustain {
                        sample_id: "sample_0001".to_string(),
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

    fn assert_has_path(errors: &[ValidationError], path: &str) {
        assert!(
            errors.iter().any(|e| e.path == path),
            "expected error at {path}, got: {errors:?}"
        );
    }

    #[test]
    fn baseline_valid_v2_sample_only_passes() {
        valid_v2_sample_only().validate().expect("must validate");
    }

    #[test]
    fn baseline_valid_v2_multi_voice_atom_passes() {
        valid_v2_multi_voice_atom()
            .validate()
            .expect("must validate");
    }

    // Rule 26: schema_version == 2.
    #[test]
    fn rule_26_schema_version_must_be_2() {
        let mut p = valid_v2_sample_only();
        p.schema_version = 1;
        assert_has_path(&p.validate().unwrap_err(), "/schema_version");
    }

    // Rule 27: driver.profile in allowed set.
    #[test]
    fn rule_27_driver_profile_v2_allowed_set() {
        let mut p = valid_v2_sample_only();
        p.driver.profile = "synth_static".to_string();
        assert_has_path(&p.validate().unwrap_err(), "/driver/profile");
    }

    // Rule 28: bytecode coupling.
    #[test]
    fn rule_28_sample_basic_requires_bytecode_1() {
        let mut p = valid_v2_sample_only();
        p.driver.bytecode_version = 2;
        assert_has_path(&p.validate().unwrap_err(), "/driver/bytecode_version");
    }

    #[test]
    fn rule_28_multi_voice_atom_requires_bytecode_2() {
        let mut p = valid_v2_multi_voice_atom();
        p.driver.bytecode_version = 1;
        assert_has_path(&p.validate().unwrap_err(), "/driver/bytecode_version");
    }

    // Rule 29: atom id pattern + uniqueness.
    #[test]
    fn rule_29_atom_id_pattern() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_pool[0].id = "BadCase".to_string();
        // Step references the old id, so collect both.
        let e = p.validate().unwrap_err();
        assert!(e.iter().any(|x| x.path == "/atom_pool/0/id"));
    }

    #[test]
    fn rule_29_atom_id_uniqueness() {
        let mut p = valid_v2_multi_voice_atom();
        let dup = p.atom_pool[0].clone();
        p.atom_pool.push(AtomSlot {
            id: dup.id,
            ..p.atom_pool[1].clone()
        });
        let e = p.validate().unwrap_err();
        assert!(e.iter().any(|x| x.path == "/atom_pool/2/id"));
    }

    // Rule 30: cross-pool id collision.
    #[test]
    fn rule_30_sample_atom_id_collision() {
        let mut p = valid_v2_multi_voice_atom();
        // Force atom_pool[0].id == sample_pool[0].id.
        let collide_id = p.sample_pool[0].id.clone();
        p.atom_pool[0].id = collide_id.clone();
        // Step needs to reference an existing atom too — keep
        // atom_pool[1] as the step's target so step lookup still
        // resolves cleanly.
        for sq in &mut p.atom_sequences {
            for st in &mut sq.steps {
                st.atom_id = p.atom_pool[1].id.clone();
            }
        }
        let e = p.validate().unwrap_err();
        assert!(
            e.iter().any(|x| matches!(
                &x.kind,
                ValidationErrorKind::SampleAtomIdCollision(id) if id == &collide_id
            )),
            "expected SampleAtomIdCollision, got: {e:?}"
        );
    }

    // Rule 32: cycle_len_samples ∈ {64, 128, 256}.
    #[test]
    fn rule_32_cycle_len_unsupported() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_pool[0].cycle_len_samples = 100;
        assert_has_path(&p.validate().unwrap_err(), "/atom_pool/0/cycle_len_samples");
    }

    // Rule 33: partials length 1..=8.
    #[test]
    fn rule_33_partials_length_too_long() {
        let mut p = valid_v2_multi_voice_atom();
        {
            let AtomKind::AdditiveSingleCycleV0 { partials } = &mut p.atom_pool[0].kind;
            *partials = (0..9)
                .map(|_| AtomPartial {
                    harmonic: 1,
                    amplitude: 0.5,
                    phase_cycles: 0.0,
                })
                .collect();
        }
        assert_has_path(&p.validate().unwrap_err(), "/atom_pool/0/partials");
    }

    // Rule 34: partial.harmonic 1..=16.
    #[test]
    fn rule_34_partial_harmonic_range() {
        let mut p = valid_v2_multi_voice_atom();
        {
            let AtomKind::AdditiveSingleCycleV0 { partials } = &mut p.atom_pool[0].kind;
            partials[0].harmonic = 17;
        }
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_pool/0/partials/0/harmonic",
        );
    }

    // Rule 35: partial.amplitude 0..=1, NaN rejected.
    #[test]
    fn rule_35_partial_amplitude_range_and_nan() {
        let mut p = valid_v2_multi_voice_atom();
        {
            let AtomKind::AdditiveSingleCycleV0 { partials } = &mut p.atom_pool[0].kind;
            partials[0].amplitude = f64::NAN;
        }
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_pool/0/partials/0/amplitude",
        );
        let mut p = valid_v2_multi_voice_atom();
        {
            let AtomKind::AdditiveSingleCycleV0 { partials } = &mut p.atom_pool[0].kind;
            partials[0].amplitude = 1.5;
        }
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_pool/0/partials/0/amplitude",
        );
    }

    // Rule 36: phase_cycles in [0.0, 1.0).
    #[test]
    fn rule_36_partial_phase_cycles_range() {
        let mut p = valid_v2_multi_voice_atom();
        {
            let AtomKind::AdditiveSingleCycleV0 { partials } = &mut p.atom_pool[0].kind;
            partials[0].phase_cycles = 1.0;
        }
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_pool/0/partials/0/phase_cycles",
        );
    }

    // Rule 37: top-level amplitude 0..=1.
    #[test]
    fn rule_37_atom_amplitude_range() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_pool[0].amplitude = 1.5;
        assert_has_path(&p.validate().unwrap_err(), "/atom_pool/0/amplitude");
    }

    // Rule 38: root_midi_note 0..=127.
    #[test]
    fn rule_38_atom_root_midi_note_range() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_pool[0].root_midi_note = 200;
        assert_has_path(&p.validate().unwrap_err(), "/atom_pool/0/root_midi_note");
    }

    // Rule 39: atom playback (volume out of range).
    #[test]
    fn rule_39_atom_playback_volume_range() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_pool[0].playback.volume = 1.5;
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_pool/0/playback/volume",
        );
    }

    // Rule 40: atom_sequence id pattern.
    #[test]
    fn rule_40_atom_sequence_id_pattern() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].id = "BadCase".to_string();
        // Track references it; collect.
        let e = p.validate().unwrap_err();
        assert!(e.iter().any(|x| x.path == "/atom_sequences/0/id"));
    }

    // Rule 41: voice 0..=1.
    #[test]
    fn rule_41_atom_sequence_voice_range() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].voice = 5;
        // Track voice will then differ from sequence voice — we'll get
        // both. We only assert the sequence-voice range error.
        assert_has_path(&p.validate().unwrap_err(), "/atom_sequences/0/voice");
    }

    // Rule 42: steps length 1..=32.
    #[test]
    fn rule_42_atom_sequence_steps_length() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].steps.clear();
        assert_has_path(&p.validate().unwrap_err(), "/atom_sequences/0/steps");
    }

    // Rule 43: duration_ticks > 0.
    #[test]
    fn rule_43_step_duration_zero_rejected() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].steps[0].duration_ticks = 0;
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_sequences/0/steps/0/duration_ticks",
        );
    }

    // Rule 44: target_volume 0..=1.
    #[test]
    fn rule_44_step_target_volume_range() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].steps[0].target_volume = 1.5;
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_sequences/0/steps/0/target_volume",
        );
    }

    // Rule 45: step.atom_id must reference an atom_pool entry.
    #[test]
    fn rule_45_step_atom_id_must_resolve() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].steps[0].atom_id = "ghost_atom".to_string();
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_sequences/0/steps/0/atom_id",
        );
    }

    // Rule 46: first step must be initial_kon.
    #[test]
    fn rule_46_first_step_must_be_initial_kon() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].steps[0].transition = AtomTransition::FadeToZeroRetrigger {
            fade_out_ticks: 4,
            fade_in_ticks: 4,
        };
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_sequences/0/steps/0/transition/type",
        );
    }

    // Rule 47: subsequent steps must be fade_to_zero_retrigger.
    #[test]
    fn rule_47_later_step_must_be_fade_to_zero_retrigger() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].steps[1].transition = AtomTransition::InitialKon;
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_sequences/0/steps/1/transition/type",
        );
    }

    // Rule 48: fade tick counts > 0.
    #[test]
    fn rule_48_fade_tick_counts_nonzero() {
        let mut p = valid_v2_multi_voice_atom();
        p.atom_sequences[0].steps[1].transition = AtomTransition::FadeToZeroRetrigger {
            fade_out_ticks: 0,
            fade_in_ticks: 4,
        };
        assert_has_path(
            &p.validate().unwrap_err(),
            "/atom_sequences/0/steps/1/transition/fade_out_ticks",
        );
    }

    // Rule 49: track id pattern.
    #[test]
    fn rule_49_track_id_pattern() {
        let mut p = valid_v2_multi_voice_atom();
        p.tracks[0].id = "BadCase".to_string();
        assert_has_path(&p.validate().unwrap_err(), "/tracks/0/id");
    }

    // Rule 50: track voice 0..=1, unique.
    #[test]
    fn rule_50_track_voice_uniqueness() {
        let mut p = valid_v2_multi_voice_atom();
        p.tracks[1].voice = 0; // collides with tracks[0].voice
        let e = p.validate().unwrap_err();
        assert!(e.iter().any(|x| x.path == "/tracks/1/voice"));
    }

    #[test]
    fn rule_50_track_voice_range() {
        let mut p = valid_v2_multi_voice_atom();
        p.tracks[1].voice = 5;
        assert_has_path(&p.validate().unwrap_err(), "/tracks/1/voice");
    }

    // Rule 52: sample_sustain track sample_id must resolve.
    #[test]
    fn rule_52_sample_track_sample_id_must_resolve() {
        let mut p = valid_v2_multi_voice_atom();
        p.tracks[0].kind = TrackKind::SampleSustain {
            sample_id: "ghost".to_string(),
        };
        assert_has_path(&p.validate().unwrap_err(), "/tracks/0/sample_id");
    }

    // Rule 53: atom_sequence track atom_sequence_id must resolve.
    #[test]
    fn rule_53_atom_sequence_track_id_must_resolve() {
        let mut p = valid_v2_multi_voice_atom();
        p.tracks[1].kind = TrackKind::AtomSequence {
            atom_sequence_id: "ghost_seq".to_string(),
        };
        assert_has_path(&p.validate().unwrap_err(), "/tracks/1/atom_sequence_id");
    }

    // Rule 54: track voice must equal its atom_sequence's voice.
    #[test]
    fn rule_54_track_atom_sequence_voice_match() {
        let mut p = valid_v2_multi_voice_atom();
        // Track 1 voice = 1, sequence voice = 1. Drop sequence voice
        // to 0; track voice still 1, so they mismatch.
        p.atom_sequences[0].voice = 0;
        let e = p.validate().unwrap_err();
        assert!(
            e.iter().any(|x| matches!(
                &x.kind,
                ValidationErrorKind::TrackAtomSequenceVoiceMismatch { .. }
            )),
            "expected TrackAtomSequenceVoiceMismatch, got: {e:?}"
        );
    }

    // Rule 55: m2.active_sequence_id must resolve when present.
    #[test]
    fn rule_55_active_sequence_id_must_resolve() {
        let mut p = valid_v2_multi_voice_atom();
        p.m2.active_sequence_id = Some("ghost_seq".to_string());
        assert_has_path(&p.validate().unwrap_err(), "/m2/active_sequence_id");
    }

    #[test]
    fn rule_55_active_sequence_id_null_ok() {
        let mut p = valid_v2_multi_voice_atom();
        p.m2.active_sequence_id = None;
        assert!(p.validate().is_ok());
    }

    // Rule 56: sample_basic forbids atom data.
    #[test]
    fn rule_56_sample_basic_forbids_atom_pool() {
        let mut p = valid_v2_sample_only();
        p.atom_pool.push(valid_atom_slot("atom_0001"));
        assert_has_path(&p.validate().unwrap_err(), "/driver/profile");
    }

    #[test]
    fn rule_56_sample_basic_forbids_voice_1_track() {
        let mut p = valid_v2_sample_only();
        p.tracks.push(Track {
            id: "extra".to_string(),
            name: String::new(),
            voice: 1,
            kind: TrackKind::SampleSustain {
                sample_id: "sample_0001".to_string(),
            },
        });
        assert_has_path(&p.validate().unwrap_err(), "/driver/profile");
    }

    // Rule 57: multi_voice_atom requires at least one atom_sequence track.
    #[test]
    fn rule_57_multi_voice_atom_requires_atom_track() {
        let mut p = valid_v2_multi_voice_atom();
        // Drop the atom-track and the now-orphan atom_sequence.
        p.tracks.retain(|t| !matches!(t.kind, TrackKind::AtomSequence { .. }));
        p.atom_sequences.clear();
        p.atom_pool.clear();
        p.m2.active_sequence_id = None;
        assert_has_path(&p.validate().unwrap_err(), "/tracks");
    }

    // ------------------------------------------------------------------
    // Migration v1 → v2 — correctness + round-trip stability.
    // ------------------------------------------------------------------

    fn minimal_v1() -> ProjectV1 {
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
                fir: [127, 0, 0, 0, 0, 0, 0, 0],
            },
            sample_pool: vec![valid_sample_slot("sample_0001")],
            m1: M1Block {
                active_sample_id: "sample_0001".to_string(),
            },
        }
    }

    #[test]
    fn migrate_from_v1_produces_valid_v2() {
        let v1 = minimal_v1();
        let v2 = migrate_from_v1(&v1);
        v2.validate().expect("migrated v2 must validate");
    }

    #[test]
    fn migrate_from_v1_field_mappings_match_spec() {
        let v1 = minimal_v1();
        let v2 = migrate_from_v1(&v1);
        assert_eq!(v2.schema_version, 2);
        assert_eq!(v2.project, v1.project);
        assert_eq!(v2.driver, v1.driver);
        assert_eq!(v2.master_echo, v1.master_echo);
        assert_eq!(v2.sample_pool, v1.sample_pool);
        assert!(v2.atom_pool.is_empty());
        assert!(v2.atom_sequences.is_empty());
        assert_eq!(v2.tracks.len(), 1);
        assert_eq!(v2.tracks[0].id, "track_sample_0");
        assert_eq!(v2.tracks[0].voice, 0);
        match &v2.tracks[0].kind {
            TrackKind::SampleSustain { sample_id } => {
                assert_eq!(sample_id, &v1.m1.active_sample_id);
            }
            _ => panic!("expected SampleSustain track"),
        }
        assert!(v2.m2.active_sequence_id.is_none());
    }

    #[test]
    fn migration_round_trip_byte_stable() {
        let dir = tempfile::tempdir().unwrap();
        let v1 = minimal_v1();

        let v2_a = migrate_from_v1(&v1);
        let path_a = dir.path().join("a.json");
        v2_a.save_to_path(&path_a).unwrap();
        let bytes_a = std::fs::read(&path_a).unwrap();

        // Reload and re-save.
        let v2_b = ProjectV2::load_from_path(&path_a).unwrap();
        let path_b = dir.path().join("b.json");
        v2_b.save_to_path(&path_b).unwrap();
        let bytes_b = std::fs::read(&path_b).unwrap();

        assert_eq!(bytes_a, bytes_b, "v2 round-trip must be byte-stable");

        // Also: migrating again from v1 produces the same bytes.
        let v2_c = migrate_from_v1(&v1);
        let path_c = dir.path().join("c.json");
        v2_c.save_to_path(&path_c).unwrap();
        let bytes_c = std::fs::read(&path_c).unwrap();
        assert_eq!(
            bytes_a, bytes_c,
            "double migration of the same v1 must produce identical v2 bytes"
        );
    }

    #[test]
    fn v2_save_load_round_trip_byte_stable() {
        let dir = tempfile::tempdir().unwrap();
        let p = valid_v2_multi_voice_atom();
        let path1 = dir.path().join("a.json");
        let path2 = dir.path().join("b.json");
        p.save_to_path(&path1).unwrap();
        let loaded = ProjectV2::load_from_path(&path1).unwrap();
        assert_eq!(p, loaded);
        loaded.save_to_path(&path2).unwrap();
        let bytes1 = std::fs::read(&path1).unwrap();
        let bytes2 = std::fs::read(&path2).unwrap();
        assert_eq!(bytes1, bytes2, "v2 round-trip must be byte-stable");
    }

    #[test]
    fn save_to_path_refuses_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = valid_v2_sample_only();
        p.schema_version = 7;
        let err = p.save_to_path(&dir.path().join("bad.json")).unwrap_err();
        assert!(matches!(err, ProjectIoError::Validation(_)));
    }

    // ------------------------------------------------------------------
    // load_project_versioned dispatch.
    // ------------------------------------------------------------------

    #[test]
    fn load_project_versioned_dispatches_v1() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v1.json");
        let v1 = minimal_v1();
        v1.save_to_path(&path).unwrap();
        let loaded = load_project_versioned(&path).unwrap();
        match loaded {
            LoadedProject::V1(p) => assert_eq!(p, v1),
            LoadedProject::V2(_) => panic!("expected V1"),
        }
    }

    #[test]
    fn load_project_versioned_dispatches_v2() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v2.json");
        let v2 = valid_v2_multi_voice_atom();
        v2.save_to_path(&path).unwrap();
        let loaded = load_project_versioned(&path).unwrap();
        match loaded {
            LoadedProject::V2(p) => assert_eq!(p, v2),
            LoadedProject::V1(_) => panic!("expected V2"),
        }
    }

    #[test]
    fn load_project_versioned_rejects_unknown_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v99.json");
        std::fs::write(&path, br#"{"schema_version":99}"#).unwrap();
        let err = load_project_versioned(&path).unwrap_err();
        assert!(
            matches!(err, ProjectIoError::UnsupportedSchemaVersion { actual: 99, .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn migration_report_shape_matches_spec() {
        let v1 = minimal_v1();
        let r = MigrationReport::for_v1_to_v2(
            std::path::PathBuf::from("a.json"),
            std::path::PathBuf::from("b.json"),
            &v1,
        );
        assert_eq!(r.schema_version, 1);
        assert_eq!(r.report_type, "migration_v1_to_v2");
        assert_eq!(r.source_schema_version, 1);
        assert_eq!(r.target_schema_version, 2);
        // Required transformations per SPEC §16.10.
        let paths: std::collections::HashSet<&str> =
            r.transformations.iter().map(|t| t.path.as_str()).collect();
        for required in [
            "/schema_version",
            "/project",
            "/driver",
            "/master_echo",
            "/sample_pool",
            "/atom_pool",
            "/atom_sequences",
            "/tracks",
            "/m2",
            "/m1",
        ] {
            assert!(paths.contains(required), "missing {required}");
        }
        // /m1 must be dropped, /atom_pool / /atom_sequences / /tracks / /m2
        // must be added, /schema_version must be moved.
        let by_path: std::collections::HashMap<&str, &str> = r
            .transformations
            .iter()
            .map(|t| (t.path.as_str(), t.kind.as_str()))
            .collect();
        assert_eq!(by_path["/m1"], "dropped");
        assert_eq!(by_path["/atom_pool"], "added");
        assert_eq!(by_path["/atom_sequences"], "added");
        assert_eq!(by_path["/tracks"], "added");
        assert_eq!(by_path["/m2"], "added");
        assert_eq!(by_path["/schema_version"], "moved");
    }
}
