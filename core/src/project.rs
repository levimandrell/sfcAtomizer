//! Project file v1 — the M1 source-of-truth schema.
//!
//! Mirrors SPEC §16 byte-for-byte: every field name, range, and
//! invariant in the spec corresponds 1:1 to a type or a documented
//! constraint here. M1.0 ships shape only — `validate` is `todo!()`
//! and lands at M1.1. Numeric ranges are documented but not enforced
//! at deserialize time; they are enforced by [`ProjectV1::validate`]
//! once that body is implemented.
//!
//! Round-trip tests in `report.rs`-style cover three schema-faithful
//! example projects.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Top-level project document. `schema_version = 1` for M1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectV1 {
    /// Schema version. M1 allowed value: `1`.
    pub schema_version: u32,
    pub project: Project,
    pub driver: Driver,
    pub master_echo: MasterEcho,
    /// Length `1..=128` for M1 (validated at compile, not by serde).
    pub sample_pool: Vec<SampleSlot>,
    pub m1: M1Block,
}

impl ProjectV1 {
    pub const SCHEMA_VERSION_M1: u32 = 1;

    /// Validate the project against §16 cross-field rules.
    ///
    /// **Body lands at M1.1.** Currently a `todo!()` so a caller that
    /// invokes it during M1.0 surfaces the gap rather than silently
    /// passing.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        todo!("ProjectV1::validate body lands at M1.1")
    }
}

/// `project` block (SPEC §16.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    /// Project display name. `1..=64` chars, no path separators,
    /// printable UTF-8.
    pub name: String,
    /// Tick rate. M1 allowed value: `60`.
    pub tick_rate_hz: u32,
}

/// `driver` block (SPEC §16.2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Driver {
    /// Driver profile. M1 allowed value: `"sample_basic"`.
    pub profile: String,
    /// Bytecode version. M1 allowed value: `1`.
    pub bytecode_version: u32,
}

/// `master_echo` block (SPEC §16.3).
///
/// **Coupling:** `enabled = false ⇒ edl = 0`, `enabled = true ⇒
/// edl ∈ 1..=15`. Validated by [`crate::echo_validation`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MasterEcho {
    pub enabled: bool,
    /// `0..=15`. Constraint above.
    pub edl: u8,
    /// Raw signed byte → DSP `EFB`.
    pub efb: i8,
    /// Raw signed byte → DSP `EVOLL`.
    pub evol_l: i8,
    /// Raw signed byte → DSP `EVOLR`.
    pub evol_r: i8,
    /// 8 raw signed bytes → DSP FIR coefficients.
    pub fir: [i8; 8],
}

/// One sample-pool entry (SPEC §16.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SampleSlot {
    /// Globally unique within the project.
    pub id: String,
    pub name: String,
    pub source: SampleSource,
    /// MIDI note number, `0..=127`. C4 = 60 (SPEC §16.7).
    pub root_midi_note: u8,
    #[serde(rename = "loop")]
    pub looped: SampleLoop,
    pub playback: SamplePlayback,
}

/// `source` sub-object (SPEC §16.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SampleSource {
    /// Relative path preferred; absolute allowed only with warning.
    pub path: String,
    /// Lowercase hex SHA-256, length 64.
    pub sha256: String,
    pub format: SampleFormat,
    /// `8000..=96000` for WAV/AIFF; for BRR import: `32000` default
    /// or explicit user-provided.
    pub sample_rate_hz: u32,
    /// `1..=2` for M1.
    pub channels: u8,
    pub frames: u64,
}

/// Sample source format (SPEC §16.4). M1 AIFF: PCM only — AIFF-C is
/// rejected at import.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SampleFormat {
    Wav,
    Aiff,
    Brr,
}

/// `loop` sub-object (SPEC §16.4).
///
/// End-exclusive: looped sample = `[start_sample, end_sample)`.
/// Both endpoints must be multiples of 16 when `enabled = true`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SampleLoop {
    pub enabled: bool,
    /// Required if `enabled = true`. Multiple of 16.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_sample: Option<u32>,
    /// Required if `enabled = true`. Multiple of 16, > start_sample,
    /// `end_sample - start_sample >= 16`, `end_sample <= source.frames`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_sample: Option<u32>,
    /// M1 allowed value: `"brr_block_16"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap: Option<String>,
}

/// `playback` sub-object (SPEC §16.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SamplePlayback {
    /// `0.0..=1.0`.
    pub volume: f64,
    /// `-1.0..=1.0` (-1.0 = hard left, 0.0 = center, +1.0 = hard
    /// right). Constant-power pan mapping in §16.4.
    pub pan: f64,
    /// Maps to DSP `EON` bit. If `true`, `master_echo.enabled` must
    /// also be `true` (cross-checked by [`crate::echo_validation`]).
    pub echo: bool,
    pub envelope: Envelope,
}

/// `envelope` tagged union (SPEC §16.4).
///
/// JSON shape: `{"type": "adsr", "attack": ..., ...}` or
/// `{"type": "gain_raw", "gain_byte": ...}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Envelope {
    /// Standard ADSR. Register mapping per SPEC §16.4:
    /// `ADSR1 = $80 | (decay << 4) | attack`,
    /// `ADSR2 = (sustain_level << 5) | sustain_rate`, `GAIN = $00`.
    ///
    /// **Trap.** The four fields are `attack` / `decay` /
    /// `sustain_level` / `sustain_rate`. Key-off release is **not**
    /// a programmable ADSR field.
    Adsr {
        /// `0..=15`.
        attack: u8,
        /// `0..=7`.
        decay: u8,
        /// `0..=7`.
        sustain_level: u8,
        /// `0..=31`.
        sustain_rate: u8,
    },
    /// Raw DSP `GAIN` byte (SPEC §16.4). Register mapping:
    /// `ADSR1 = $00`, `ADSR2 = $00`, `GAIN = gain_byte`.
    ///
    /// M1 deliberately doesn't expose a high-level GAIN envelope
    /// model; that lands in a later milestone after listening tests.
    GainRaw {
        /// `0..=255`.
        gain_byte: u8,
    },
}

/// `m1` block (SPEC §16.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct M1Block {
    /// Must match exactly one `sample_pool[].id`.
    pub active_sample_id: String,
}

/// Validation failure modes (SPEC §16.6). Body lands at M1.1; this
/// enum is shape-only for now so callers can name the variants.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("schema_version {actual} unsupported (expected {expected})")]
    SchemaVersionUnsupported { expected: u32, actual: u32 },
    #[error("project.tick_rate_hz {0} not allowed (expected 60)")]
    TickRateUnsupported(u32),
    #[error("driver.profile {0:?} not allowed for M1 (expected \"sample_basic\")")]
    DriverProfileUnsupported(String),
    #[error("driver.bytecode_version {0} not allowed for M1 (expected 1)")]
    BytecodeVersionUnsupported(u32),
    #[error("sample_pool length {0} out of range 1..=128")]
    SamplePoolLength(usize),
    #[error("sample_pool[].id {0:?} duplicated")]
    DuplicateSampleId(String),
    #[error("master_echo.enabled=true requires edl in 1..=15, got {0}")]
    MasterEchoEnabledRequiresEdl(u8),
    #[error("master_echo.enabled=false requires edl=0, got {0}")]
    MasterEchoDisabledRequiresZeroEdl(u8),
    #[error("sample_pool[].playback.echo=true requires master_echo.enabled=true (sample {0:?})")]
    SampleEchoRequiresMasterEcho(String),
    #[error("loop.enabled=true requires start_sample (sample {0:?})")]
    LoopMissingStart(String),
    #[error("loop.enabled=true requires end_sample (sample {0:?})")]
    LoopMissingEnd(String),
    #[error(
        "loop bounds invalid (sample {sample:?}): start={start} end={end} \
         (need end>start, end-start>=16, both multiples of 16, end<=frames={frames})"
    )]
    LoopBoundsInvalid {
        sample: String,
        start: u32,
        end: u32,
        frames: u64,
    },
    #[error("loop.snap {0:?} not allowed for M1 (expected \"brr_block_16\")")]
    LoopSnapUnsupported(String),
    #[error("m1.active_sample_id {0:?} not found in sample_pool")]
    ActiveSampleNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(v: &ProjectV1) {
        let json = serde_json::to_string_pretty(v).unwrap();
        let back: ProjectV1 = serde_json::from_str(&json).unwrap();
        assert_eq!(v, &back, "round-trip mismatch: {json}");
    }

    fn example_source() -> SampleSource {
        SampleSource {
            path: "audio/lead.wav".to_string(),
            sha256: "0".repeat(64),
            format: SampleFormat::Wav,
            sample_rate_hz: 32000,
            channels: 1,
            frames: 44100,
        }
    }

    fn example_loop_enabled() -> SampleLoop {
        SampleLoop {
            enabled: true,
            start_sample: Some(1024),
            end_sample: Some(32768),
            snap: Some("brr_block_16".to_string()),
        }
    }

    #[test]
    fn round_trip_adsr_no_echo() {
        // SPEC §16.4 main example.
        let p = ProjectV1 {
            schema_version: 1,
            project: Project {
                name: "m1_single_sample".to_string(),
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
            sample_pool: vec![SampleSlot {
                id: "sample_0001".to_string(),
                name: "lead_sample".to_string(),
                source: example_source(),
                root_midi_note: 60,
                looped: example_loop_enabled(),
                playback: SamplePlayback {
                    volume: 1.0,
                    pan: 0.0,
                    echo: false,
                    envelope: Envelope::Adsr {
                        attack: 9,
                        decay: 4,
                        sustain_level: 5,
                        sustain_rate: 12,
                    },
                },
            }],
            m1: M1Block {
                active_sample_id: "sample_0001".to_string(),
            },
        };
        round_trip(&p);
    }

    #[test]
    fn round_trip_gain_raw_envelope() {
        let p = ProjectV1 {
            schema_version: 1,
            project: Project {
                name: "m1_gain_raw".to_string(),
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
            sample_pool: vec![SampleSlot {
                id: "sample_g".to_string(),
                name: "raw_gain".to_string(),
                source: example_source(),
                root_midi_note: 60,
                looped: SampleLoop {
                    enabled: false,
                    start_sample: None,
                    end_sample: None,
                    snap: None,
                },
                playback: SamplePlayback {
                    volume: 0.8,
                    pan: -0.25,
                    echo: false,
                    envelope: Envelope::GainRaw { gain_byte: 127 },
                },
            }],
            m1: M1Block {
                active_sample_id: "sample_g".to_string(),
            },
        };
        round_trip(&p);
    }

    #[test]
    fn round_trip_master_echo_with_per_sample_echo() {
        let p = ProjectV1 {
            schema_version: 1,
            project: Project {
                name: "m1_echo".to_string(),
                tick_rate_hz: 60,
            },
            driver: Driver {
                profile: "sample_basic".to_string(),
                bytecode_version: 1,
            },
            master_echo: MasterEcho {
                enabled: true,
                edl: 4,
                efb: 64,
                evol_l: 64,
                evol_r: 64,
                fir: [127, 0, 0, 0, 0, 0, 0, 0],
            },
            sample_pool: vec![SampleSlot {
                id: "sample_e".to_string(),
                name: "echo_lead".to_string(),
                source: example_source(),
                root_midi_note: 60,
                looped: example_loop_enabled(),
                playback: SamplePlayback {
                    volume: 1.0,
                    pan: 0.0,
                    echo: true,
                    envelope: Envelope::Adsr {
                        attack: 9,
                        decay: 4,
                        sustain_level: 5,
                        sustain_rate: 12,
                    },
                },
            }],
            m1: M1Block {
                active_sample_id: "sample_e".to_string(),
            },
        };
        round_trip(&p);
    }

    #[test]
    fn envelope_tag_is_snake_case() {
        let adsr = Envelope::Adsr {
            attack: 0,
            decay: 0,
            sustain_level: 0,
            sustain_rate: 0,
        };
        let json = serde_json::to_string(&adsr).unwrap();
        assert!(
            json.contains(r#""type":"adsr""#),
            "expected snake_case adsr tag: {json}"
        );

        let gain = Envelope::GainRaw { gain_byte: 127 };
        let json = serde_json::to_string(&gain).unwrap();
        assert!(
            json.contains(r#""type":"gain_raw""#),
            "expected snake_case gain_raw tag: {json}"
        );
    }

    #[test]
    fn loop_field_serializes_as_loop_in_json() {
        let s = SampleSlot {
            id: "x".to_string(),
            name: "x".to_string(),
            source: example_source(),
            root_midi_note: 60,
            looped: example_loop_enabled(),
            playback: SamplePlayback {
                volume: 1.0,
                pan: 0.0,
                echo: false,
                envelope: Envelope::Adsr {
                    attack: 0,
                    decay: 0,
                    sustain_level: 0,
                    sustain_rate: 0,
                },
            },
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains(r#""loop":"#),
            "expected JSON 'loop' key: {json}"
        );
        assert!(
            !json.contains("looped"),
            "Rust field 'looped' must not leak into JSON: {json}"
        );
    }

    #[test]
    #[should_panic]
    fn validate_is_todo_at_m10() {
        let p = ProjectV1 {
            schema_version: 1,
            project: Project {
                name: "x".to_string(),
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
            sample_pool: vec![SampleSlot {
                id: "s".to_string(),
                name: "s".to_string(),
                source: example_source(),
                root_midi_note: 60,
                looped: example_loop_enabled(),
                playback: SamplePlayback {
                    volume: 1.0,
                    pan: 0.0,
                    echo: false,
                    envelope: Envelope::Adsr {
                        attack: 0,
                        decay: 0,
                        sustain_level: 0,
                        sustain_rate: 0,
                    },
                },
            }],
            m1: M1Block {
                active_sample_id: "s".to_string(),
            },
        };
        let _ = p.validate();
    }
}
