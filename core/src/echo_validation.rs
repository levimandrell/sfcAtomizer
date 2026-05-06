//! Echo cross-field validation (SPEC §16.6).
//!
//! Small enough to land at M1.0 instead of waiting for M1.1.
//! [`crate::project::ProjectV1::validate`] will call this once that
//! body lands; M1.0 just exposes the rule directly so the M1.1
//! integration is mechanical.
//!
//! Rules:
//!
//! 1. `master.enabled = true` ⇒ `master.edl` ∈ `1..=15`.
//! 2. `master.enabled = false` ⇒ `master.edl == 0`.
//! 3. Any `sample.playback.echo = true` ⇒ `master.enabled = true`.

use thiserror::Error;

use crate::project::{MasterEcho, SampleSlot};

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum EchoConfigError {
    #[error("master_echo.enabled=true requires edl in 1..=15, got {0}")]
    EnabledRequiresEdlInRange(u8),
    #[error("master_echo.enabled=false requires edl=0, got {0}")]
    DisabledRequiresZeroEdl(u8),
    #[error("sample_pool[].id={0:?} has playback.echo=true but master_echo.enabled=false")]
    SampleEchoWithoutMaster(String),
}

/// Apply all echo cross-field rules. Reports every failed rule, not
/// just the first.
pub fn validate_echo(
    master: &MasterEcho,
    samples: &[SampleSlot],
) -> Result<(), Vec<EchoConfigError>> {
    let mut errors = Vec::new();

    // Rule 1 / 2 — master enabled / edl coupling.
    if master.enabled {
        if master.edl == 0 || master.edl > 15 {
            errors.push(EchoConfigError::EnabledRequiresEdlInRange(master.edl));
        }
    } else if master.edl != 0 {
        errors.push(EchoConfigError::DisabledRequiresZeroEdl(master.edl));
    }

    // Rule 3 — per-sample echo requires master.enabled.
    if !master.enabled {
        for s in samples {
            if s.playback.echo {
                errors.push(EchoConfigError::SampleEchoWithoutMaster(s.id.clone()));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::*;

    fn empty_master_disabled() -> MasterEcho {
        MasterEcho {
            enabled: false,
            edl: 0,
            efb: 0,
            evol_l: 0,
            evol_r: 0,
            fir: [127, 0, 0, 0, 0, 0, 0, 0],
        }
    }

    fn master_enabled_edl(edl: u8) -> MasterEcho {
        MasterEcho {
            enabled: true,
            edl,
            efb: 64,
            evol_l: 64,
            evol_r: 64,
            fir: [127, 0, 0, 0, 0, 0, 0, 0],
        }
    }

    fn sample_with_echo(id: &str, echo: bool) -> SampleSlot {
        SampleSlot {
            id: id.to_string(),
            name: id.to_string(),
            source: SampleSource {
                path: "x.wav".to_string(),
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
                echo,
                envelope: Envelope::Adsr {
                    attack: 0,
                    decay: 0,
                    sustain_level: 0,
                    sustain_rate: 0,
                },
            },
        }
    }

    #[test]
    fn disabled_master_zero_edl_passes() {
        let r = validate_echo(&empty_master_disabled(), &[sample_with_echo("a", false)]);
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn enabled_master_edl_1_passes() {
        let r = validate_echo(&master_enabled_edl(1), &[sample_with_echo("a", true)]);
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn enabled_master_edl_15_passes() {
        let r = validate_echo(&master_enabled_edl(15), &[sample_with_echo("a", true)]);
        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn enabled_master_with_zero_edl_fails() {
        let r = validate_echo(&master_enabled_edl(0), &[sample_with_echo("a", true)]);
        let errs = r.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, EchoConfigError::EnabledRequiresEdlInRange(0))),
            "{errs:?}"
        );
    }

    #[test]
    fn disabled_master_with_nonzero_edl_fails() {
        let mut m = empty_master_disabled();
        m.edl = 4;
        let r = validate_echo(&m, &[sample_with_echo("a", false)]);
        let errs = r.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, EchoConfigError::DisabledRequiresZeroEdl(4))),
            "{errs:?}"
        );
    }

    #[test]
    fn sample_echo_with_disabled_master_fails() {
        let r = validate_echo(
            &empty_master_disabled(),
            &[sample_with_echo("a", true), sample_with_echo("b", false)],
        );
        let errs = r.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(
            matches!(&errs[0], EchoConfigError::SampleEchoWithoutMaster(id) if id == "a"),
            "{errs:?}"
        );
    }

    #[test]
    fn multiple_samples_with_echo_all_flagged() {
        let r = validate_echo(
            &empty_master_disabled(),
            &[
                sample_with_echo("a", true),
                sample_with_echo("b", true),
                sample_with_echo("c", false),
            ],
        );
        let errs = r.unwrap_err();
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn master_disabled_with_no_sample_echo_passes_even_if_master_edl_nonzero_is_only_error() {
        // Verify multiple errors collected at once, not short-circuited.
        let mut m = empty_master_disabled();
        m.edl = 7;
        let r = validate_echo(
            &m,
            &[sample_with_echo("a", true), sample_with_echo("b", true)],
        );
        let errs = r.unwrap_err();
        // 1 master-edl error + 2 sample-echo errors = 3 total.
        assert_eq!(errs.len(), 3, "{errs:?}");
    }
}
