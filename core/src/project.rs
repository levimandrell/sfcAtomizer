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

use std::path::{Path, PathBuf};

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

    /// Validate the project against the SPEC §16 v1 contract. Collects
    /// every failed rule (does not bail on first) so the UI can show
    /// all problems at once.
    ///
    /// Errors carry JSON-pointer-style paths (e.g.
    /// `/sample_pool/0/loop/end_sample`) so a viewer can locate the
    /// offending field.
    ///
    /// **Convention.** `name` fields allow spaces and printable
    /// non-ASCII (UTF-8) but reject any `is_control()` codepoint and
    /// any of `/`, `\`, `:` (path separators). `id` fields are
    /// stricter: ASCII lowercase, digits, underscore only.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors: Vec<ValidationError> = Vec::new();

        // Rule 1: schema_version.
        if self.schema_version != Self::SCHEMA_VERSION_M1 {
            errors.push(ValidationError {
                path: "/schema_version".to_string(),
                kind: ValidationErrorKind::SchemaVersionUnsupported {
                    expected: Self::SCHEMA_VERSION_M1,
                    actual: self.schema_version,
                },
            });
        }

        // Rule 2: project.tick_rate_hz == 60.
        if self.project.tick_rate_hz != 60 {
            errors.push(ValidationError {
                path: "/project/tick_rate_hz".to_string(),
                kind: ValidationErrorKind::TickRateUnsupported(self.project.tick_rate_hz),
            });
        }

        // Rule 3: project.name.
        validate_string_field(
            &mut errors,
            "/project/name",
            &self.project.name,
            true, // disallow path separators
        );

        // Rules 4 & 5: driver.
        if self.driver.profile != "sample_basic" {
            errors.push(ValidationError {
                path: "/driver/profile".to_string(),
                kind: ValidationErrorKind::DriverProfileUnsupported(self.driver.profile.clone()),
            });
        }
        if self.driver.bytecode_version != 1 {
            errors.push(ValidationError {
                path: "/driver/bytecode_version".to_string(),
                kind: ValidationErrorKind::BytecodeVersionUnsupported(self.driver.bytecode_version),
            });
        }

        // Rule 6: master_echo field ranges. i8/u8 already constrain at
        // type level for efb/evol/fir/length; only edl needs runtime
        // checks (u8 allows 0..=255 but rule requires 0..=15).
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

        // Rules 7, 8, 22: master_echo enabled/edl coupling + per-sample
        // echo gating. Delegate to core::echo_validation.
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

        // Rule 9 (relaxed at M2.5, SPEC §16.6): sample_pool length 0..=128.
        // Empty pool is schema-valid; downstream pack/compile still
        // requires at least one source for sample_basic projects.
        let pool_len = self.sample_pool.len();
        if pool_len > 128 {
            errors.push(ValidationError {
                path: "/sample_pool".to_string(),
                kind: ValidationErrorKind::SamplePoolLength(pool_len),
            });
        }

        // Rules 10–24: per-sample.
        let mut seen_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (i, s) in self.sample_pool.iter().enumerate() {
            let prefix = format!("/sample_pool/{i}");
            self.validate_sample(&prefix, s, &mut errors);
            if !seen_ids.insert(&s.id) {
                errors.push(ValidationError {
                    path: format!("{prefix}/id"),
                    kind: ValidationErrorKind::DuplicateSampleId(s.id.clone()),
                });
            }
        }

        // Rule 25: m1.active_sample_id matches a sample in the pool.
        let active = &self.m1.active_sample_id;
        if !self.sample_pool.iter().any(|s| &s.id == active) {
            errors.push(ValidationError {
                path: "/m1/active_sample_id".to_string(),
                kind: ValidationErrorKind::ActiveSampleNotFound(active.clone()),
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_sample(&self, prefix: &str, s: &SampleSlot, errors: &mut Vec<ValidationError>) {
        validate_sample_slot(prefix, s, errors);
    }
}

/// Free-function entry point for sample-slot validation, shared between v1
/// (rules 10–24) and v2 (carry-forward of the same rules under §16.9). The
/// `prefix` is the JSON-pointer path to the slot itself
/// (`/sample_pool/<i>` for v1, same shape for v2).
pub(crate) fn validate_sample_slot(
    prefix: &str,
    s: &SampleSlot,
    errors: &mut Vec<ValidationError>,
) {
    // Rule 10: id pattern + length.
    if !is_valid_id(&s.id) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::SampleIdPatternMismatch(s.id.clone()),
        });
    }
    if !(1..=64).contains(&s.id.chars().count()) {
        errors.push(ValidationError {
            path: format!("{prefix}/id"),
            kind: ValidationErrorKind::StringLength {
                len: s.id.chars().count(),
                min: 1,
                max: 64,
            },
        });
    }

    // Rule 11: name.
    validate_string_field(errors, &format!("{prefix}/name"), &s.name, false);

    // Rules 12–16: source.
    let src_prefix = format!("{prefix}/source");
    // 12: format is enum; serde already enforces.
    // 13: sample_rate_hz 8000..=96000.
    if !(8000..=96000).contains(&s.source.sample_rate_hz) {
        errors.push(ValidationError {
            path: format!("{src_prefix}/sample_rate_hz"),
            kind: ValidationErrorKind::IntegerOutOfRange {
                value: s.source.sample_rate_hz as i64,
                min: 8000,
                max: 96000,
            },
        });
    }
    // 14: channels 1..=2.
    if !(1..=2).contains(&s.source.channels) {
        errors.push(ValidationError {
            path: format!("{src_prefix}/channels"),
            kind: ValidationErrorKind::IntegerOutOfRange {
                value: s.source.channels as i64,
                min: 1,
                max: 2,
            },
        });
    }
    // 15: frames >= 1.
    if s.source.frames < 1 {
        errors.push(ValidationError {
            path: format!("{src_prefix}/frames"),
            kind: ValidationErrorKind::IntegerOutOfRange {
                value: s.source.frames as i64,
                min: 1,
                max: i64::MAX,
            },
        });
    }
    // 16: sha256 lowercase hex 64 chars.
    if !is_sha256_hex(&s.source.sha256) {
        errors.push(ValidationError {
            path: format!("{src_prefix}/sha256"),
            kind: ValidationErrorKind::Sha256Invalid(s.source.sha256.clone()),
        });
    }

    // Rule 17: root_midi_note 0..=127. u8 allows 0..=255.
    if s.root_midi_note > 127 {
        errors.push(ValidationError {
            path: format!("{prefix}/root_midi_note"),
            kind: ValidationErrorKind::IntegerOutOfRange {
                value: s.root_midi_note as i64,
                min: 0,
                max: 127,
            },
        });
    }

    // Rule 18 + 19: loop bounds + snap.
    let lp_prefix = format!("{prefix}/loop");
    if s.looped.enabled {
        match (s.looped.start_sample, s.looped.end_sample) {
            (None, _) => errors.push(ValidationError {
                path: format!("{lp_prefix}/start_sample"),
                kind: ValidationErrorKind::LoopMissing {
                    field: "start_sample",
                },
            }),
            (Some(_), None) => errors.push(ValidationError {
                path: format!("{lp_prefix}/end_sample"),
                kind: ValidationErrorKind::LoopMissing {
                    field: "end_sample",
                },
            }),
            (Some(start), Some(end)) => {
                if start % 16 != 0 {
                    errors.push(ValidationError {
                        path: format!("{lp_prefix}/start_sample"),
                        kind: ValidationErrorKind::LoopBoundNotMultipleOf16(start),
                    });
                }
                if end % 16 != 0 {
                    errors.push(ValidationError {
                        path: format!("{lp_prefix}/end_sample"),
                        kind: ValidationErrorKind::LoopBoundNotMultipleOf16(end),
                    });
                }
                if end <= start {
                    errors.push(ValidationError {
                        path: format!("{lp_prefix}/end_sample"),
                        kind: ValidationErrorKind::LoopEndNotGreaterThanStart { start, end },
                    });
                } else if end - start < 16 {
                    errors.push(ValidationError {
                        path: format!("{lp_prefix}/end_sample"),
                        kind: ValidationErrorKind::LoopRangeTooShort { start, end },
                    });
                }
                if (end as u64) > s.source.frames {
                    errors.push(ValidationError {
                        path: format!("{lp_prefix}/end_sample"),
                        kind: ValidationErrorKind::LoopEndExceedsFrames {
                            end,
                            frames: s.source.frames,
                        },
                    });
                }
            }
        }
        match s.looped.snap.as_deref() {
            Some("brr_block_16") => {}
            Some(other) => errors.push(ValidationError {
                path: format!("{lp_prefix}/snap"),
                kind: ValidationErrorKind::LoopSnapUnsupported(other.to_string()),
            }),
            None => errors.push(ValidationError {
                path: format!("{lp_prefix}/snap"),
                kind: ValidationErrorKind::LoopMissing { field: "snap" },
            }),
        }
    }

    // Rules 20 & 21: playback.volume + pan.
    let pb_prefix = format!("{prefix}/playback");
    if s.playback.volume.is_nan() {
        errors.push(ValidationError {
            path: format!("{pb_prefix}/volume"),
            kind: ValidationErrorKind::NaN,
        });
    } else if !(0.0..=1.0).contains(&s.playback.volume) {
        errors.push(ValidationError {
            path: format!("{pb_prefix}/volume"),
            kind: ValidationErrorKind::FloatOutOfRange {
                value: s.playback.volume,
                min: 0.0,
                max: 1.0,
            },
        });
    }
    if s.playback.pan.is_nan() {
        errors.push(ValidationError {
            path: format!("{pb_prefix}/pan"),
            kind: ValidationErrorKind::NaN,
        });
    } else if !(-1.0..=1.0).contains(&s.playback.pan) {
        errors.push(ValidationError {
            path: format!("{pb_prefix}/pan"),
            kind: ValidationErrorKind::FloatOutOfRange {
                value: s.playback.pan,
                min: -1.0,
                max: 1.0,
            },
        });
    }

    // Rules 23 & 24: envelope ranges.
    match &s.playback.envelope {
        Envelope::Adsr {
            attack,
            decay,
            sustain_level,
            sustain_rate,
        } => {
            if *attack > 15 {
                errors.push(ValidationError {
                    path: format!("{pb_prefix}/envelope/attack"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *attack as i64,
                        min: 0,
                        max: 15,
                    },
                });
            }
            if *decay > 7 {
                errors.push(ValidationError {
                    path: format!("{pb_prefix}/envelope/decay"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *decay as i64,
                        min: 0,
                        max: 7,
                    },
                });
            }
            if *sustain_level > 7 {
                errors.push(ValidationError {
                    path: format!("{pb_prefix}/envelope/sustain_level"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *sustain_level as i64,
                        min: 0,
                        max: 7,
                    },
                });
            }
            if *sustain_rate > 31 {
                errors.push(ValidationError {
                    path: format!("{pb_prefix}/envelope/sustain_rate"),
                    kind: ValidationErrorKind::IntegerOutOfRange {
                        value: *sustain_rate as i64,
                        min: 0,
                        max: 31,
                    },
                });
            }
        }
        Envelope::GainRaw { .. } => {
            // Rule 24: gain_byte 0..=255 — already enforced by u8.
        }
    }
}

/// Apply rules 3 / 11: length 1..=64, no control chars; optionally
/// reject `/`, `\`, `:` (path separators).
pub(crate) fn validate_string_field(
    errors: &mut Vec<ValidationError>,
    path: &str,
    s: &str,
    disallow_path_separators: bool,
) {
    let len = s.chars().count();
    if !(1..=64).contains(&len) {
        errors.push(ValidationError {
            path: path.to_string(),
            kind: ValidationErrorKind::StringLength {
                len,
                min: 1,
                max: 64,
            },
        });
    }
    if s.chars().any(|c| c.is_control()) {
        errors.push(ValidationError {
            path: path.to_string(),
            kind: ValidationErrorKind::StringContainsControlChars(s.to_string()),
        });
    }
    if disallow_path_separators && s.chars().any(|c| matches!(c, '/' | '\\' | ':')) {
        errors.push(ValidationError {
            path: path.to_string(),
            kind: ValidationErrorKind::StringContainsPathSeparator(s.to_string()),
        });
    }
}

/// Sample id pattern: `^[a-z0-9_]+$`. Empty is invalid (covered by
/// length check too, but doubled here defensively).
pub(crate) fn is_valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// 64 lowercase hex chars.
pub(crate) fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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

// =============================================================================
// I/O — load / save / migrate
// =============================================================================

/// Failure modes for project file I/O.
#[derive(Debug, Error)]
pub enum ProjectIoError {
    #[error("project file not found at {path}")]
    NotFound { path: PathBuf },
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json parse error at {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("malformed value (no usable schema_version)")]
    MalformedValue,
    #[error("schema_version {actual} unsupported (expected {expected}); no migration available")]
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    #[error("validation failed with {} error(s)", .0.len())]
    Validation(Vec<ValidationError>),
}

impl ProjectV1 {
    /// Read + parse + migrate a project file. **Does not validate** —
    /// callers that need a guaranteed-valid project should use
    /// [`Self::load_and_validate`]. The two-step API exists so the
    /// GUI viewer can render an invalid project alongside its
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
        Self::migrate_from_value(v).map_err(|e| match e {
            ProjectIoError::Parse { source, .. } => ProjectIoError::Parse {
                path: path.to_path_buf(),
                source,
            },
            other => other,
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
    /// Creates parent directories as needed.
    pub fn save_to_path(&self, path: &Path) -> Result<(), ProjectIoError> {
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

    /// Migrate a parsed `serde_json::Value` into a typed `ProjectV1`.
    ///
    /// M1.1 supports only `schema_version: 1`. Older schemas (v0 if
    /// any leaks out of M0.x) and future schemas (v2+) return
    /// [`ProjectIoError::UnsupportedSchemaVersion`].
    pub fn migrate_from_value(v: serde_json::Value) -> Result<Self, ProjectIoError> {
        let version = v
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or(ProjectIoError::MalformedValue)?;
        if version != Self::SCHEMA_VERSION_M1 as u64 {
            return Err(ProjectIoError::UnsupportedSchemaVersion {
                expected: Self::SCHEMA_VERSION_M1,
                actual: version as u32,
            });
        }
        // No-op migration for v1 → v1; deserialize directly.
        serde_json::from_value(v).map_err(|source| ProjectIoError::Parse {
            path: PathBuf::new(),
            source,
        })
    }

    /// Build the M1.1 minimal pre-import template the `new-project`
    /// CLI command writes. Note that this template **fails
    /// validation** by design — `sample_pool` is empty (rule #9
    /// requires 1..=128) and `m1.active_sample_id` is empty (rule
    /// #25). The user is expected to run `import` before the project
    /// validates.
    pub fn new_template(name: &str) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION_M1,
            project: Project {
                name: name.to_string(),
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
            sample_pool: Vec::new(),
            m1: M1Block {
                active_sample_id: String::new(),
            },
        }
    }
}

/// One validation failure, with a JSON-pointer-style `path` to the
/// offending field and a typed `kind`. M1.1 ships the full set of
/// rules per SPEC §16.6.
#[derive(Debug, Clone, Error, PartialEq)]
#[error("{path}: {kind}")]
pub struct ValidationError {
    pub path: String,
    pub kind: ValidationErrorKind,
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum ValidationErrorKind {
    #[error("schema_version {actual} unsupported (expected {expected}; consider running migrate_from_value)")]
    SchemaVersionUnsupported { expected: u32, actual: u32 },
    #[error("tick_rate_hz {0} not allowed for M1 (expected 60)")]
    TickRateUnsupported(u32),
    #[error("driver.profile {0:?} not allowed for M1 (expected \"sample_basic\")")]
    DriverProfileUnsupported(String),
    #[error("driver.bytecode_version {0} not allowed for M1 (expected 1)")]
    BytecodeVersionUnsupported(u32),
    #[error("string length {len} out of range {min}..={max}")]
    StringLength { len: usize, min: usize, max: usize },
    #[error("string {0:?} contains a path separator (/, \\, or :)")]
    StringContainsPathSeparator(String),
    #[error("string {0:?} contains control characters")]
    StringContainsControlChars(String),
    #[error("integer {value} out of range {min}..={max}")]
    IntegerOutOfRange { value: i64, min: i64, max: i64 },
    #[error("float {value} out of range {min}..={max}")]
    FloatOutOfRange { value: f64, min: f64, max: f64 },
    #[error("NaN not allowed")]
    NaN,
    #[error("master_echo.enabled=true requires edl in 1..=15, got {0}")]
    MasterEchoEnabledRequiresEdl(u8),
    #[error("master_echo.enabled=false requires edl=0, got {0}")]
    MasterEchoDisabledRequiresZeroEdl(u8),
    #[error("sample_pool length {0} out of range 0..=128")]
    SamplePoolLength(usize),
    #[error("duplicate sample id {0:?}")]
    DuplicateSampleId(String),
    #[error("sample id {0:?} doesn't match required pattern ^[a-z0-9_]+$")]
    SampleIdPatternMismatch(String),
    #[error("sha256 {0:?} is not 64 lowercase hex chars")]
    Sha256Invalid(String),
    #[error("loop.enabled=true requires {field}")]
    LoopMissing { field: &'static str },
    #[error("loop bound {0} is not a multiple of 16")]
    LoopBoundNotMultipleOf16(u32),
    #[error("loop end_sample {end} must be > start_sample {start}")]
    LoopEndNotGreaterThanStart { start: u32, end: u32 },
    #[error("loop range too short: {end} - {start} < 16")]
    LoopRangeTooShort { start: u32, end: u32 },
    #[error("loop end_sample {end} exceeds source.frames {frames}")]
    LoopEndExceedsFrames { end: u32, frames: u64 },
    #[error("loop.snap {0:?} not allowed for M1 (expected \"brr_block_16\")")]
    LoopSnapUnsupported(String),
    #[error("playback.echo=true but master_echo.enabled=false")]
    SampleEchoWithoutMaster,
    #[error("m1.active_sample_id {0:?} not found in sample_pool")]
    ActiveSampleNotFound(String),

    // -------- v2-only kinds (SPEC §16.9 rules 26..=57) --------
    #[error(
        "driver.profile {0:?} not allowed (expected \"sample_basic\" or \"multi_voice_atom\")"
    )]
    DriverProfileUnsupportedV2(String),
    #[error("driver.bytecode_version {bytecode} not allowed for profile {profile:?} (expected {expected})")]
    DriverBytecodeProfileMismatch {
        profile: String,
        bytecode: u32,
        expected: u32,
    },
    #[error("atom id {0:?} doesn't match required pattern ^[a-z0-9_]+$")]
    AtomIdPatternMismatch(String),
    #[error("duplicate atom id {0:?}")]
    DuplicateAtomId(String),
    #[error("id {0:?} collides between sample_pool and atom_pool (cross-pool uniqueness)")]
    SampleAtomIdCollision(String),
    #[error("atom kind {0:?} not allowed (M2 expects \"additive_single_cycle_v0\")")]
    AtomKindUnsupported(String),
    #[error("atom cycle_len_samples {0} not allowed (expected 64, 128, or 256)")]
    AtomCycleLenUnsupported(u16),
    #[error("partials length {0} out of range 1..=8")]
    AtomPartialsLength(usize),
    #[error("partial.harmonic {0} out of range 1..=16")]
    AtomPartialHarmonicOutOfRange(u8),
    #[error("partial.phase_cycles {0} not in 0.0..1.0 (mod 1)")]
    AtomPartialPhaseOutOfRange(f64),
    #[error("atom_sequence id {0:?} doesn't match required pattern ^[a-z0-9_]+$")]
    AtomSequenceIdPatternMismatch(String),
    #[error("duplicate atom_sequence id {0:?}")]
    DuplicateAtomSequenceId(String),
    #[error("atom_sequence steps length {0} out of range 1..=32")]
    AtomSequenceStepsLength(usize),
    #[error("atom_sequence step {idx}: atom_id {id:?} not found in atom_pool")]
    AtomSequenceStepAtomIdNotFound { idx: usize, id: String },
    #[error("atom_sequence step 0: transition must be \"initial_kon\", got {0:?}")]
    AtomSequenceFirstStepMustBeInitialKon(String),
    #[error(
        "atom_sequence step {idx}: subsequent steps must be \"fade_to_zero_retrigger\" in M2, got {got:?}"
    )]
    AtomSequenceLaterStepWrongTransition { idx: usize, got: String },
    #[error("track id {0:?} doesn't match required pattern ^[a-z0-9_]+$")]
    TrackIdPatternMismatch(String),
    #[error("duplicate track id {0:?}")]
    DuplicateTrackId(String),
    #[error("duplicate track voice {0} (must be unique across tracks[])")]
    DuplicateTrackVoice(u8),
    #[error("track sample_id {0:?} not found in sample_pool")]
    TrackSampleIdNotFound(String),
    #[error("track atom_sequence_id {0:?} not found in atom_sequences")]
    TrackAtomSequenceIdNotFound(String),
    #[error("track voice {track_voice} doesn't match its atom_sequence voice {sequence_voice}")]
    TrackAtomSequenceVoiceMismatch { track_voice: u8, sequence_voice: u8 },
    #[error("m2.active_sequence_id {0:?} not found in atom_sequences")]
    ActiveAtomSequenceNotFound(String),
    #[error(
        "driver.profile=\"sample_basic\" forbids non-empty atom_pool / atom_sequences / atom_sequence tracks / voice-1 tracks"
    )]
    SampleBasicForbidsAtomData,
    #[error("driver.profile=\"multi_voice_atom\" requires at least one atom_sequence track")]
    MultiVoiceAtomRequiresAtomSequenceTrack,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a project that validates cleanly. Each per-rule test
    /// starts from this and mutates one field to force a failure.
    fn valid_project() -> ProjectV1 {
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
            sample_pool: vec![SampleSlot {
                id: "sample_0001".to_string(),
                name: "lead".to_string(),
                source: SampleSource {
                    path: "audio/lead.wav".to_string(),
                    sha256: "0".repeat(64),
                    format: SampleFormat::Wav,
                    sample_rate_hz: 32000,
                    channels: 1,
                    frames: 65536,
                },
                root_midi_note: 60,
                looped: SampleLoop {
                    enabled: true,
                    start_sample: Some(1024),
                    end_sample: Some(32768),
                    snap: Some("brr_block_16".to_string()),
                },
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
        }
    }

    fn assert_has_path(errors: &[ValidationError], path: &str) {
        assert!(
            errors.iter().any(|e| e.path == path),
            "expected error at {path}, got: {errors:?}"
        );
    }

    #[test]
    fn baseline_valid_project_passes() {
        assert!(valid_project().validate().is_ok());
    }

    // Rule 1
    #[test]
    fn rule_01_schema_version_must_be_1() {
        let mut p = valid_project();
        p.schema_version = 2;
        let e = p.validate().unwrap_err();
        assert_has_path(&e, "/schema_version");
    }

    // Rule 2
    #[test]
    fn rule_02_tick_rate_must_be_60() {
        let mut p = valid_project();
        p.project.tick_rate_hz = 50;
        assert_has_path(&p.validate().unwrap_err(), "/project/tick_rate_hz");
    }

    // Rule 3
    #[test]
    fn rule_03_project_name_length() {
        let mut p = valid_project();
        p.project.name = String::new();
        assert_has_path(&p.validate().unwrap_err(), "/project/name");
        let mut p = valid_project();
        p.project.name = "x".repeat(65);
        assert_has_path(&p.validate().unwrap_err(), "/project/name");
    }

    #[test]
    fn rule_03_project_name_no_path_separators() {
        for sep in ["a/b", "a\\b", "a:b"] {
            let mut p = valid_project();
            p.project.name = sep.to_string();
            assert_has_path(&p.validate().unwrap_err(), "/project/name");
        }
    }

    #[test]
    fn rule_03_project_name_no_control_chars() {
        let mut p = valid_project();
        p.project.name = "a\nb".to_string();
        assert_has_path(&p.validate().unwrap_err(), "/project/name");
    }

    #[test]
    fn rule_03_project_name_allows_spaces_and_unicode() {
        let mut p = valid_project();
        p.project.name = "Hello 世界".to_string();
        assert!(p.validate().is_ok());
    }

    // Rule 4
    #[test]
    fn rule_04_driver_profile_sample_basic_only() {
        let mut p = valid_project();
        p.driver.profile = "synth_static".to_string();
        assert_has_path(&p.validate().unwrap_err(), "/driver/profile");
    }

    // Rule 5
    #[test]
    fn rule_05_bytecode_version_1_only() {
        let mut p = valid_project();
        p.driver.bytecode_version = 2;
        assert_has_path(&p.validate().unwrap_err(), "/driver/bytecode_version");
    }

    // Rule 6
    #[test]
    fn rule_06_master_edl_range() {
        let mut p = valid_project();
        p.master_echo.edl = 16;
        assert_has_path(&p.validate().unwrap_err(), "/master_echo/edl");
    }

    // Rule 7
    #[test]
    fn rule_07_disabled_master_requires_zero_edl() {
        let mut p = valid_project();
        p.master_echo.enabled = false;
        p.master_echo.edl = 4;
        assert_has_path(&p.validate().unwrap_err(), "/master_echo/edl");
    }

    // Rule 8
    #[test]
    fn rule_08_enabled_master_requires_edl_in_range() {
        let mut p = valid_project();
        p.master_echo.enabled = true;
        p.master_echo.edl = 0;
        assert_has_path(&p.validate().unwrap_err(), "/master_echo/edl");
    }

    // Rule 9
    #[test]
    fn rule_09_sample_pool_length_range() {
        // M2.5 (SPEC §16.6): empty pool is now valid; only > 128 errors.
        let mut p = valid_project();
        p.sample_pool.clear();
        let errs = p.validate().unwrap_err();
        assert!(
            !errs.iter().any(|e| e.path == "/sample_pool"),
            "empty pool no longer errors at /sample_pool: {errs:?}"
        );

        // Length 129 still errors.
        let template = p.sample_pool.first().cloned().unwrap_or_else(|| {
            // Build a minimal template if pool was empty.
            valid_project().sample_pool[0].clone()
        });
        let mut over = valid_project();
        over.sample_pool.clear();
        for i in 0..129 {
            let mut s = template.clone();
            s.id = format!("s{i:03}");
            over.sample_pool.push(s);
        }
        over.m1.active_sample_id = "s000".to_string();
        assert_has_path(&over.validate().unwrap_err(), "/sample_pool");
    }

    // Rule 10 — id pattern
    #[test]
    fn rule_10_sample_id_pattern() {
        for bad in ["BadCase", "with-dash", "with space", "Übung"] {
            let mut p = valid_project();
            p.sample_pool[0].id = bad.to_string();
            p.m1.active_sample_id = bad.to_string();
            let e = p.validate().unwrap_err();
            assert!(
                e.iter().any(|x| x.path == "/sample_pool/0/id"),
                "expected pattern error for {bad:?}: {e:?}"
            );
        }
    }

    #[test]
    fn rule_10_sample_id_must_be_unique() {
        let mut p = valid_project();
        let extra = SampleSlot {
            id: "sample_0001".to_string(), // duplicate of [0]
            ..p.sample_pool[0].clone()
        };
        p.sample_pool.push(extra);
        let e = p.validate().unwrap_err();
        assert!(e.iter().any(|x| x.path == "/sample_pool/1/id"));
    }

    // Rule 11
    #[test]
    fn rule_11_sample_name_no_control_chars() {
        let mut p = valid_project();
        p.sample_pool[0].name = "with\rcontrol".to_string();
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/name");
    }

    #[test]
    fn rule_11_sample_name_allows_spaces_and_path_chars() {
        // sample names are not paths — the rule allows '/' here.
        let mut p = valid_project();
        p.sample_pool[0].name = "lead/main".to_string();
        assert!(p.validate().is_ok());
    }

    // Rule 13 — sample_rate_hz range
    #[test]
    fn rule_13_sample_rate_range() {
        let mut p = valid_project();
        p.sample_pool[0].source.sample_rate_hz = 7999;
        assert_has_path(
            &p.validate().unwrap_err(),
            "/sample_pool/0/source/sample_rate_hz",
        );
        let mut p = valid_project();
        p.sample_pool[0].source.sample_rate_hz = 96001;
        assert_has_path(
            &p.validate().unwrap_err(),
            "/sample_pool/0/source/sample_rate_hz",
        );
    }

    // Rule 14 — channels range
    #[test]
    fn rule_14_channels_range() {
        let mut p = valid_project();
        p.sample_pool[0].source.channels = 0;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/source/channels");
        let mut p = valid_project();
        p.sample_pool[0].source.channels = 3;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/source/channels");
    }

    // Rule 15 — frames >= 1
    #[test]
    fn rule_15_frames_at_least_1() {
        let mut p = valid_project();
        p.sample_pool[0].source.frames = 0;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/source/frames");
    }

    // Rule 16 — sha256 lowercase hex 64
    #[test]
    fn rule_16_sha256_format() {
        for bad in [
            "0".repeat(63),                         // too short
            "0".repeat(65),                         // too long
            "ABCDEF".to_string() + &"0".repeat(58), // uppercase
            "g".repeat(64),                         // non-hex
        ] {
            let mut p = valid_project();
            p.sample_pool[0].source.sha256 = bad;
            assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/source/sha256");
        }
    }

    // Rule 17 — root_midi_note range
    #[test]
    fn rule_17_root_midi_note_range() {
        let mut p = valid_project();
        p.sample_pool[0].root_midi_note = 128;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/root_midi_note");
    }

    // Rule 18 — loop bounds
    #[test]
    fn rule_18_loop_start_must_be_multiple_of_16() {
        let mut p = valid_project();
        p.sample_pool[0].looped.start_sample = Some(1023);
        assert_has_path(
            &p.validate().unwrap_err(),
            "/sample_pool/0/loop/start_sample",
        );
    }

    #[test]
    fn rule_18_loop_end_must_be_multiple_of_16() {
        let mut p = valid_project();
        p.sample_pool[0].looped.end_sample = Some(32769);
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/loop/end_sample");
    }

    #[test]
    fn rule_18_loop_end_must_exceed_start() {
        let mut p = valid_project();
        p.sample_pool[0].looped.start_sample = Some(1024);
        p.sample_pool[0].looped.end_sample = Some(1024);
        let e = p.validate().unwrap_err();
        assert!(e.iter().any(|x| x.path == "/sample_pool/0/loop/end_sample"));
    }

    #[test]
    fn rule_18_loop_range_at_least_16_samples() {
        let mut p = valid_project();
        p.sample_pool[0].looped.start_sample = Some(1024);
        p.sample_pool[0].looped.end_sample = Some(1024 + 8); // not multiple of 16 either
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/loop/end_sample");
    }

    #[test]
    fn rule_18_loop_end_within_frames() {
        let mut p = valid_project();
        p.sample_pool[0].source.frames = 1024;
        p.sample_pool[0].looped.start_sample = Some(0);
        p.sample_pool[0].looped.end_sample = Some(2048);
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/loop/end_sample");
    }

    // Rule 19 — loop.snap
    #[test]
    fn rule_19_loop_snap_brr_block_16_only() {
        let mut p = valid_project();
        p.sample_pool[0].looped.snap = Some("other".to_string());
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/loop/snap");
    }

    // Rule 20 — volume
    #[test]
    fn rule_20_volume_range_and_nan() {
        let mut p = valid_project();
        p.sample_pool[0].playback.volume = 1.5;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/playback/volume");
        let mut p = valid_project();
        p.sample_pool[0].playback.volume = -0.1;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/playback/volume");
        let mut p = valid_project();
        p.sample_pool[0].playback.volume = f64::NAN;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/playback/volume");
    }

    // Rule 21 — pan
    #[test]
    fn rule_21_pan_range_and_nan() {
        let mut p = valid_project();
        p.sample_pool[0].playback.pan = -1.5;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/playback/pan");
        let mut p = valid_project();
        p.sample_pool[0].playback.pan = 1.5;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/playback/pan");
        let mut p = valid_project();
        p.sample_pool[0].playback.pan = f64::NAN;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/playback/pan");
    }

    // Rule 22 — per-sample echo requires master enabled
    #[test]
    fn rule_22_per_sample_echo_requires_master() {
        let mut p = valid_project();
        p.master_echo.enabled = false;
        p.sample_pool[0].playback.echo = true;
        assert_has_path(&p.validate().unwrap_err(), "/sample_pool/0/playback/echo");
    }

    // Rule 23 — ADSR ranges
    #[test]
    fn rule_23_adsr_attack_range() {
        let mut p = valid_project();
        p.sample_pool[0].playback.envelope = Envelope::Adsr {
            attack: 16,
            decay: 0,
            sustain_level: 0,
            sustain_rate: 0,
        };
        assert_has_path(
            &p.validate().unwrap_err(),
            "/sample_pool/0/playback/envelope/attack",
        );
    }

    #[test]
    fn rule_23_adsr_decay_range() {
        let mut p = valid_project();
        p.sample_pool[0].playback.envelope = Envelope::Adsr {
            attack: 0,
            decay: 8,
            sustain_level: 0,
            sustain_rate: 0,
        };
        assert_has_path(
            &p.validate().unwrap_err(),
            "/sample_pool/0/playback/envelope/decay",
        );
    }

    #[test]
    fn rule_23_adsr_sustain_level_range() {
        let mut p = valid_project();
        p.sample_pool[0].playback.envelope = Envelope::Adsr {
            attack: 0,
            decay: 0,
            sustain_level: 8,
            sustain_rate: 0,
        };
        assert_has_path(
            &p.validate().unwrap_err(),
            "/sample_pool/0/playback/envelope/sustain_level",
        );
    }

    #[test]
    fn rule_23_adsr_sustain_rate_range() {
        let mut p = valid_project();
        p.sample_pool[0].playback.envelope = Envelope::Adsr {
            attack: 0,
            decay: 0,
            sustain_level: 0,
            sustain_rate: 32,
        };
        assert_has_path(
            &p.validate().unwrap_err(),
            "/sample_pool/0/playback/envelope/sustain_rate",
        );
    }

    // Rule 24 — gain_byte (always satisfied by u8)
    #[test]
    fn rule_24_gain_raw_passes() {
        let mut p = valid_project();
        p.sample_pool[0].playback.envelope = Envelope::GainRaw { gain_byte: 200 };
        assert!(p.validate().is_ok());
    }

    // Rule 25 — m1.active_sample_id matches a sample
    #[test]
    fn rule_25_active_sample_id_must_match() {
        let mut p = valid_project();
        p.m1.active_sample_id = "doesnt_exist".to_string();
        assert_has_path(&p.validate().unwrap_err(), "/m1/active_sample_id");
    }

    // ========================================================================
    // I/O round-trip tests
    // ========================================================================

    #[test]
    fn save_and_load_round_trip_byte_stable() {
        let dir = tempfile::tempdir().unwrap();
        let p = valid_project();
        let path1 = dir.path().join("a.json");
        let path2 = dir.path().join("b.json");
        p.save_to_path(&path1).unwrap();
        let loaded = ProjectV1::load_from_path(&path1).unwrap();
        assert_eq!(p, loaded);
        loaded.save_to_path(&path2).unwrap();
        let bytes1 = std::fs::read(&path1).unwrap();
        let bytes2 = std::fs::read(&path2).unwrap();
        assert_eq!(bytes1, bytes2, "round-trip must be byte-stable");
    }

    #[test]
    fn migrate_from_value_v1_succeeds() {
        let p = valid_project();
        let v = serde_json::to_value(&p).unwrap();
        let p2 = ProjectV1::migrate_from_value(v).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn migrate_from_value_unknown_version_fails() {
        let p = valid_project();
        let mut v = serde_json::to_value(&p).unwrap();
        v["schema_version"] = serde_json::json!(99);
        let err = ProjectV1::migrate_from_value(v).unwrap_err();
        assert!(matches!(
            err,
            ProjectIoError::UnsupportedSchemaVersion { actual: 99, .. }
        ));
    }

    #[test]
    fn load_from_path_not_found() {
        let err =
            ProjectV1::load_from_path(std::path::Path::new("/__nonexistent__/x.json")).unwrap_err();
        assert!(matches!(err, ProjectIoError::NotFound { .. }));
    }

    #[test]
    fn load_and_validate_chains_validation_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("template.json");
        ProjectV1::new_template("demo").save_to_path(&path).unwrap();
        let err = ProjectV1::load_and_validate(&path).unwrap_err();
        assert!(matches!(err, ProjectIoError::Validation(_)));
    }

    #[test]
    fn new_template_fields_match_brief() {
        let p = ProjectV1::new_template("demo");
        assert_eq!(p.schema_version, 1);
        assert_eq!(p.project.name, "demo");
        assert_eq!(p.project.tick_rate_hz, 60);
        assert_eq!(p.driver.profile, "sample_basic");
        assert_eq!(p.driver.bytecode_version, 1);
        assert!(!p.master_echo.enabled);
        assert_eq!(p.master_echo.edl, 0);
        assert!(p.sample_pool.is_empty());
        assert_eq!(p.m1.active_sample_id, "");
    }

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
    fn validate_minimal_valid_project() {
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
        assert!(
            p.validate().is_ok(),
            "valid project must pass: {:?}",
            p.validate().err()
        );
    }
}
