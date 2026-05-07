//! Capability manifest (SPEC §5.4).
//!
//! Structured metadata describing what the compiled module supports.
//! Emitted as a sidecar JSON next to the ARAM image / module.bin so
//! downstream consumers (instrument editor, sequencer, runtime
//! audition, M2 acceptance) can dispatch on actual capabilities
//! rather than just `driver.profile`.
//!
//! Two profiles ship at M2.3:
//!
//! - `sample_basic`: M1 single-voice sample playback. No atoms, no
//!   sequence, no multi-voice. Used by v1 projects and v2 sample-only-
//!   equivalent projects.
//! - `multi_voice_atom`: M2 two-voice path with atom support, atom
//!   sequences, source-step (per Appendix A.6 in SPEC).
//!
//! Per SPEC §5.4 enforcement clause: every compiler entry point that
//! emits atom data, sequence bytecode, or voice-1 data MUST consult
//! the manifest. UI gating is best-effort cosmetic; the compile-time
//! check is the source of truth.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityManifest {
    pub schema_version: u32,
    pub report_type: String,
    /// `"sample_basic"` or `"multi_voice_atom"` at M2.3.
    pub driver_profile: String,
    /// `1` for `sample_basic`, `2` for `multi_voice_atom`.
    pub driver_version: u8,
    pub bytecode_version: u8,
    /// Feature flags. Keys per SPEC §5.4 / Appendix A.6 (see
    /// [`SAMPLE_BASIC_FEATURES`] / [`MULTI_VOICE_ATOM_FEATURES`]).
    pub features: BTreeMap<String, bool>,
    pub limits: CapabilityLimits,
}

impl CapabilityManifest {
    pub const REPORT_TYPE: &'static str = "capability_manifest";

    /// `sample_basic` profile manifest — M1 single-voice sample
    /// playback, no atoms / sequences / multi-voice.
    pub fn sample_basic() -> Self {
        Self {
            schema_version: crate::report::SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            driver_profile: "sample_basic".to_string(),
            driver_version: 1,
            bytecode_version: 1,
            features: features_map(SAMPLE_BASIC_FEATURES),
            limits: CapabilityLimits::sample_basic(),
        }
    }

    /// `multi_voice_atom` profile manifest — M2 two-voice path with
    /// atom + atom-sequence + source-step support.
    pub fn multi_voice_atom() -> Self {
        Self {
            schema_version: crate::report::SCHEMA_VERSION,
            report_type: Self::REPORT_TYPE.to_string(),
            driver_profile: "multi_voice_atom".to_string(),
            driver_version: 2,
            bytecode_version: 2,
            features: features_map(MULTI_VOICE_ATOM_FEATURES),
            limits: CapabilityLimits::multi_voice_atom(),
        }
    }

    /// Validate the manifest against the SPEC §5.4 dependency graph:
    /// every feature whose flag is `true` must have all its declared
    /// dependencies also `true`. Returns the first failing dependency
    /// (feature name + missing dep) so the caller can surface a
    /// precise error.
    pub fn validate_dependencies(&self) -> Result<(), CapabilityDepError> {
        for (feat, &enabled) in &self.features {
            if !enabled {
                continue;
            }
            for dep in dependencies_of(feat) {
                if self.features.get(*dep).copied().unwrap_or(false) {
                    continue;
                }
                return Err(CapabilityDepError::MissingDep {
                    feature: feat.clone(),
                    missing: (*dep).to_string(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityLimits {
    pub max_music_voices: u8,
    pub reserved_sfx_voices: u8,
    pub max_sources: u32,
    pub max_dsp_writes_per_tick: u32,
    pub min_keyoff_to_keyon_ticks: u32,
    pub max_sequence_bytes: u32,
    pub max_atom_sources: u32,
    pub max_simultaneous_volume_slides: u8,
}

impl CapabilityLimits {
    /// `sample_basic` limits — M1 numbers (no atoms, no sequence
    /// bytecode beyond M1.0).
    pub fn sample_basic() -> Self {
        Self {
            max_music_voices: 1,
            reserved_sfx_voices: 0,
            max_sources: 128,
            max_dsp_writes_per_tick: 24,
            min_keyoff_to_keyon_ticks: 1,
            max_sequence_bytes: 0,
            max_atom_sources: 0,
            max_simultaneous_volume_slides: 0,
        }
    }

    /// `multi_voice_atom` limits — SPEC §5.4 Appendix A.6.
    pub fn multi_voice_atom() -> Self {
        Self {
            max_music_voices: 2,
            reserved_sfx_voices: 0,
            max_sources: 128,
            max_dsp_writes_per_tick: 24,
            min_keyoff_to_keyon_ticks: 1,
            max_sequence_bytes: 1024,
            max_atom_sources: 32,
            max_simultaneous_volume_slides: 1,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum CapabilityDepError {
    #[error("feature {feature:?} requires dependency {missing:?} but it isn't enabled")]
    MissingDep { feature: String, missing: String },
}

/// Feature names enabled under `sample_basic`. Subset of the
/// multi_voice_atom set: only the M1 sample-playback core.
pub const SAMPLE_BASIC_FEATURES: &[&str] = &[
    "core_tick_loop",
    "core_dsp_write",
    "core_note_on_off",
    "core_source_directory",
    "core_key_on_delay_safety",
    "sample_playback",
    "volume_set",
    "pitch_set",
    "adsr",
    "gain",
    "pan_set",
    "echo_enable",
    "echo_static_params",
];

/// Feature names enabled under `multi_voice_atom` (SPEC §5.4
/// Appendix A.6 / §5.4 #M2 profile).
pub const MULTI_VOICE_ATOM_FEATURES: &[&str] = &[
    "core_tick_loop",
    "core_dsp_write",
    "core_sequence_wait",
    "core_note_on_off",
    "core_source_directory",
    "core_key_on_delay_safety",
    "sample_playback",
    "sample_runtime_src_change",
    "volume_set",
    "volume_slide",
    "pitch_set",
    "adsr",
    "gain",
    "pan_set",
    "echo_enable",
    "echo_static_params",
    "echo_per_voice_mask",
    "synth_static_atom",
    "synth_atom_sequence",
    "synth_source_step",
    "multi_voice_playback",
];

fn features_map(names: &[&str]) -> BTreeMap<String, bool> {
    names.iter().map(|n| ((*n).to_string(), true)).collect()
}

/// Per-feature dependency list (SPEC §5.4). Edges follow the
/// "feature → required prerequisites" arrows.
fn dependencies_of(feature: &str) -> &'static [&'static str] {
    match feature {
        "multi_voice_playback" => &["core_note_on_off", "core_dsp_write"],
        "synth_static_atom" => &["sample_playback", "core_source_directory"],
        "synth_atom_sequence" => &["synth_static_atom", "core_sequence_wait"],
        "synth_source_step" => &[
            "synth_atom_sequence",
            "sample_runtime_src_change",
            "core_key_on_delay_safety",
        ],
        "volume_slide" => &["volume_set", "core_tick_loop"],
        "sample_runtime_src_change" => &["core_source_directory", "core_note_on_off"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_basic_round_trip() {
        let m = CapabilityManifest::sample_basic();
        let json = serde_json::to_string_pretty(&m).unwrap();
        let back: CapabilityManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn multi_voice_atom_round_trip() {
        let m = CapabilityManifest::multi_voice_atom();
        let json = serde_json::to_string_pretty(&m).unwrap();
        let back: CapabilityManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn sample_basic_features_match_documented_set() {
        let m = CapabilityManifest::sample_basic();
        let on: std::collections::HashSet<&str> = m
            .features
            .iter()
            .filter(|(_, &v)| v)
            .map(|(k, _)| k.as_str())
            .collect();
        let expected: std::collections::HashSet<&str> =
            SAMPLE_BASIC_FEATURES.iter().copied().collect();
        assert_eq!(on, expected);
        // No atom-related features should be present.
        for forbidden in [
            "synth_static_atom",
            "synth_atom_sequence",
            "synth_source_step",
            "multi_voice_playback",
            "volume_slide",
        ] {
            assert!(
                !on.contains(forbidden),
                "sample_basic must not enable {forbidden}"
            );
        }
    }

    #[test]
    fn multi_voice_atom_features_match_documented_set() {
        let m = CapabilityManifest::multi_voice_atom();
        let on: std::collections::HashSet<&str> = m
            .features
            .iter()
            .filter(|(_, &v)| v)
            .map(|(k, _)| k.as_str())
            .collect();
        let expected: std::collections::HashSet<&str> =
            MULTI_VOICE_ATOM_FEATURES.iter().copied().collect();
        assert_eq!(on, expected);
    }

    #[test]
    fn dependency_consistency_passes_for_both_profiles() {
        CapabilityManifest::sample_basic()
            .validate_dependencies()
            .expect("sample_basic deps must be self-consistent");
        CapabilityManifest::multi_voice_atom()
            .validate_dependencies()
            .expect("multi_voice_atom deps must be self-consistent");
    }

    #[test]
    fn dependency_error_when_dep_missing() {
        let mut m = CapabilityManifest::multi_voice_atom();
        // Disable a dep of synth_atom_sequence and verify validation fires.
        m.features.insert("synth_static_atom".to_string(), false);
        let err = m.validate_dependencies().unwrap_err();
        let CapabilityDepError::MissingDep { feature, missing } = err;
        assert_eq!(feature, "synth_atom_sequence");
        assert_eq!(missing, "synth_static_atom");
    }

    #[test]
    fn limits_match_spec_for_each_profile() {
        let s = CapabilityManifest::sample_basic().limits;
        assert_eq!(s.max_music_voices, 1);
        assert_eq!(s.max_sequence_bytes, 0);
        assert_eq!(s.max_atom_sources, 0);

        let m = CapabilityManifest::multi_voice_atom().limits;
        assert_eq!(m.max_music_voices, 2);
        assert_eq!(m.max_dsp_writes_per_tick, 24);
        assert_eq!(m.max_sequence_bytes, 1024);
        assert_eq!(m.max_atom_sources, 32);
        assert_eq!(m.max_simultaneous_volume_slides, 1);
    }
}
