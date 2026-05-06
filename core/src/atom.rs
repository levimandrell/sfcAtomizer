//! Synth atom v0 — type skeleton.
//!
//! SPEC §16.9 atom v0 design: kind `additive_single_cycle_v0`, a
//! short PCM cycle assembled from sine partials, normalised, then
//! scaled by `amplitude` and rounded to i16. The implementation
//! lands at M2.2.

use serde::{Deserialize, Serialize};

use crate::project::{Envelope, SamplePlayback};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtomSlot {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: AtomKind,
    pub root_midi_note: u8,
    pub cycle_len_samples: u16,
    /// Top-level amplitude scaler, 0.0..=1.0. Multiplied by the
    /// summed-and-normalised partial waveform to produce the
    /// pre-quantisation float.
    pub amplitude: f64,
    pub render: AtomRenderOptions,
    pub playback: SamplePlayback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AtomKind {
    /// Additive synthesis from one or more sine partials over a
    /// single cycle (SPEC §16.9). The most basic atom kind shipped
    /// with M2.0 contracts; M3+ adds two-oscillator atoms,
    /// wavetable atoms, and morph atoms.
    AdditiveSingleCycleV0 {
        /// 1..=8 partials. `harmonic` 1..=16, `amplitude` 0.0..=1.0,
        /// `phase_cycles` 0.0..1.0 (mod 1).
        partials: Vec<AtomPartial>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AtomPartial {
    pub harmonic: u8,
    pub amplitude: f64,
    pub phase_cycles: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AtomRenderOptions {
    pub normalize: bool,
    pub force_filter_0_first_block: bool,
    pub force_filter_0_loop_entry: bool,
}

/// Inputs to the atom renderer. Implementation lands at M2.2.
#[derive(Debug, Clone)]
pub struct AtomRenderInput<'a> {
    pub atom: &'a AtomSlot,
}

/// Outputs of the atom renderer: PCM cycle (length =
/// `atom.cycle_len_samples`) plus encode metadata for the BRR
/// encoder downstream.
#[derive(Debug, Clone)]
pub struct AtomRenderOutput {
    pub pcm: Vec<i16>,
    /// `loop_start_sample = 0` so the BRR encoder treats the entire
    /// rendered cycle as the loop region.
    pub loop_start_sample: u32,
}

/// Render an atom's PCM cycle. **M2.0 stub.** Implementation lands
/// at M2.2 once the atom encoder pipeline is wired.
pub fn render(input: AtomRenderInput<'_>) -> AtomRenderOutput {
    let _ = input;
    todo!("atom rendering lands at M2.2 — see SPEC §16.9 render formula");
}

/// Discard the unused-import warning on `Envelope` while the body
/// is `todo!()`. Kept exported for the M2.2 implementation.
#[doc(hidden)]
pub fn _envelope_marker() -> Option<Envelope> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(v: &T)
    where
        T: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(v).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, &back);
    }

    fn sample_atom() -> AtomSlot {
        AtomSlot {
            id: "atom_0001".to_string(),
            name: "sine_128".to_string(),
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
                pan: 1.0,
                echo: false,
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        }
    }

    #[test]
    fn atom_slot_round_trip() {
        round_trip(&sample_atom());
    }

    #[test]
    fn atom_kind_serializes_as_tagged_kind_field() {
        // Spec wording: top-level `kind: "additive_single_cycle_v0"`
        // colocated with the kind-specific fields. The
        // #[serde(flatten)] on AtomSlot.kind plus the #[serde(tag)]
        // on AtomKind achieves that.
        let json = serde_json::to_string(&sample_atom()).unwrap();
        assert!(
            json.contains("\"kind\":\"additive_single_cycle_v0\""),
            "{json}"
        );
        assert!(json.contains("\"partials\""), "{json}");
    }

    #[test]
    fn partial_round_trip() {
        round_trip(&AtomPartial {
            harmonic: 3,
            amplitude: 0.5,
            phase_cycles: 0.25,
        });
    }
}
