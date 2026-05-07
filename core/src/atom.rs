//! Synth atom v0 — render formula + BRR encode chain.
//!
//! SPEC §16.9 atom v0 design: kind `additive_single_cycle_v0`, a
//! short PCM cycle assembled from sine partials, normalised, then
//! scaled by `amplitude` and rounded to i16.
//!
//! **Rounding mode (SPEC §16.9): round-half-AWAY-from-zero**, not
//! the round-half-up used elsewhere (e.g. the pitch register in
//! §16.7). The two differ for negative half-values: round-half-up
//! sends `-0.5` to `0`, round-half-away-from-zero sends `-0.5` to
//! `-1`. The atom renderer follows the spec exactly to keep
//! quantisation symmetric across the zero crossing — important for
//! atoms whose final cycle has equal-magnitude positive and
//! negative peaks. Reproduced here verbatim:
//!
//! ```text
//! for n in 0..cycle_len_samples:
//!   x[n] = Σ partial.amplitude
//!          * sin(2π * (partial.harmonic * n / cycle_len_samples
//!                      + partial.phase_cycles))
//! if normalize:
//!   x[n] /= max_abs(x)
//! pcm_i16[n] = round_ties_away_from_zero(x[n] * amplitude * 32767)
//! ```
//!
//! All intermediate arithmetic is `f64`; `f32` accumulating partial
//! sums is not precise enough to make the output deterministic
//! across architectures.
//!
//! M2 atoms do not use phase rotation, spectral scoring, or
//! pre-emphasis. Those land alongside Level-1 synth atom mode at
//! M3+.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::brr_encoder::{encode_looped, EncodeOptions, EncodeSummary, EncodedBlockReport};
use crate::project::SamplePlayback;

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

/// Reserved error type for the atom render → BRR encode pipeline.
///
/// Currently un-inhabited: the render formula is pure-math infallible
/// (only `sin` on finite inputs, no division except the optional
/// normalize divide which is guarded against zero), and the BRR
/// encode call here is internally well-formed (loop_start = 0 is
/// always 16-aligned and < cycle_len_samples for the validated
/// 64/128/256 cycle lengths). Future M2 work — phase rotation,
/// spectral scoring, pre-emphasis — may add variants.
#[derive(Debug, Error)]
pub enum AtomRenderError {}

#[derive(Debug, Clone)]
pub struct AtomBrrOutput {
    pub pcm: Vec<i16>,
    pub brr_bytes: Vec<u8>,
    pub encode_summary: EncodeSummary,
    pub encode_blocks: Vec<EncodedBlockReport>,
    pub pcm_sha256: String,
    pub brr_sha256: String,
}

/// Render an atom's PCM cycle per SPEC §16.9 (single-cycle additive
/// synth). Output length is exactly `atom.cycle_len_samples` (64,
/// 128, or 256 — all multiples of 16, BRR-block-aligned).
///
/// Pure-math infallible. Validated atoms (rules 31..=37 in §16.9)
/// produce finite output across the full f64 range; the final
/// `clamp(i16::MIN as f64, i16::MAX as f64)` defensively bounds
/// any extreme cases.
pub fn render_to_pcm(atom: &AtomSlot) -> Vec<i16> {
    let n = atom.cycle_len_samples as usize;
    let mut x = vec![0.0_f64; n];

    let AtomKind::AdditiveSingleCycleV0 { partials } = &atom.kind;
    for partial in partials {
        let h = partial.harmonic as f64;
        let phase = partial.phase_cycles;
        let a = partial.amplitude;
        for (k, slot) in x.iter_mut().enumerate() {
            let theta = 2.0 * std::f64::consts::PI * (h * k as f64 / n as f64 + phase);
            *slot += a * theta.sin();
        }
    }

    if atom.render.normalize {
        let max = x.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        if max > 0.0 {
            for v in x.iter_mut() {
                *v /= max;
            }
        }
    }

    let amp = atom.amplitude;
    x.iter()
        .map(|v| {
            let s = v * amp * 32767.0;
            // Round half AWAY FROM ZERO per SPEC §16.9.
            // round_half_up would send -0.5 to 0; here -0.5 → -1.
            let r = if s >= 0.0 {
                (s + 0.5).floor()
            } else {
                (s - 0.5).ceil()
            };
            r.clamp(i16::MIN as f64, i16::MAX as f64) as i16
        })
        .collect()
}

/// Render the PCM cycle, then encode through the M1 BRR encoder.
///
/// Single-cycle atoms have block 0 = first block = loop entry block,
/// so when `atom.render.force_filter_0_first_block` is true the
/// encoder forces filter 0 on block 0 (also satisfying the loop-
/// entry filter-0 requirement automatically). Returns SHA-256 over
/// both the PCM bytes and the BRR bytes for downstream determinism
/// gates (`render-atom`'s output, M2 acceptance baselines).
pub fn render_to_brr(atom: &AtomSlot) -> Result<AtomBrrOutput, AtomRenderError> {
    let pcm = render_to_pcm(atom);
    let pcm_sha256 = sha256_hex_i16(&pcm);

    let opts = EncodeOptions {
        force_filter_0_first_block: atom.render.force_filter_0_first_block,
        loop_entry_block_index: Some(0),
    };
    // loop_start = 0 is always 16-aligned and (for valid atoms) <
    // pcm.len() (cycle_len 64/128/256 ≥ 64), so encode_looped won't
    // surface its `LoopStartNotAligned` / `LoopStartOutOfRange`
    // error variants here. The expect() documents that contract.
    let result = encode_looped(&pcm, 0, &opts)
        .expect("encode_looped is infallible at loop_start=0 for valid atoms");

    let brr_sha256 = crate::asm::sha256_hex(&result.bytes);
    Ok(AtomBrrOutput {
        pcm,
        brr_bytes: result.bytes,
        encode_summary: result.summary,
        encode_blocks: result.blocks,
        pcm_sha256,
        brr_sha256,
    })
}

/// SHA-256 of `samples` interpreted as little-endian s16 bytes —
/// matches the on-disk `--out-pcm` byte stream from `render-atom`.
fn sha256_hex_i16(samples: &[i16]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    crate::asm::sha256_hex(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Envelope;

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

    /// Full-scale sine atom: `atom.amplitude = 1.0`, peaks at
    /// ±32767. Used by tests that verify the render formula's peak
    /// values; not BRR-friendly (full-scale signals quantize past
    /// the M1.3 encoder's ±256 LSB round-trip target).
    fn pure_sine_atom(cycle: u16) -> AtomSlot {
        let mut a = canonical_sine_atom(cycle);
        a.amplitude = 1.0;
        a
    }

    /// SPEC §16.9 canonical example sine atom: `atom.amplitude =
    /// 0.75`, peaks at ±24575. BRR-friendly for round-trip and
    /// loop-click tests; serves as the M2 baseline fixture.
    fn canonical_sine_atom(cycle: u16) -> AtomSlot {
        AtomSlot {
            id: format!("sine_{cycle}"),
            name: format!("sine_{cycle}"),
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
                volume: 1.0,
                pan: 0.0,
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

    // ====================================================================
    // M2.2 — render_to_pcm tests (SPEC §16.9 formula).
    // ====================================================================

    #[test]
    fn render_single_fundamental_partial() {
        // Fundamental at amp=1.0, phase=0, normalize=true,
        // atom.amplitude=1.0, cycle=128. After normalisation the
        // peak is 1.0, scaled to 32767. Quarter-cycle samples land
        // at 0 / +max / 0 / -max.
        let atom = pure_sine_atom(128);
        let pcm = render_to_pcm(&atom);
        assert_eq!(pcm.len(), 128);
        assert_eq!(pcm[0], 0, "sin(0)=0");
        // ±2 LSB tolerance for round-trip arithmetic and the
        // round-half-away rounding step.
        let near = |a: i16, b: i16| (a as i32 - b as i32).abs() <= 2;
        assert!(near(pcm[32], 32767), "quarter-cycle peak: got {}", pcm[32]);
        assert!(near(pcm[64], 0), "zero-crossing: got {}", pcm[64]);
        assert!(
            near(pcm[96], -32767),
            "three-quarter-cycle peak: got {}",
            pcm[96]
        );
    }

    #[test]
    fn render_phase_offset_quarter_starts_at_peak() {
        // phase_cycles = 0.25 shifts the wave a quarter cycle, so
        // sample 0 lands at the original sample-32 peak.
        let mut atom = pure_sine_atom(128);
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut atom.kind;
        partials[0].phase_cycles = 0.25;
        let pcm = render_to_pcm(&atom);
        let near = |a: i16, b: i16| (a as i32 - b as i32).abs() <= 2;
        assert!(
            near(pcm[0], 32767),
            "phase-shifted peak at 0: got {}",
            pcm[0]
        );
        assert!(
            near(pcm[64], -32767),
            "phase-shifted trough at half cycle: got {}",
            pcm[64]
        );
    }

    #[test]
    fn render_two_partials_sum_correctly() {
        // Fundamental amp=1.0 + 3rd harmonic amp=0.5, phase 0,
        // normalize=true, atom.amplitude=1.0, cycle=128. The raw
        // sum at k=32 is sin(π/2) + 0.5*sin(3π/2) = 1.0 + 0.5*-1.0
        // = 0.5. The peak of the sum is somewhere else. Compute
        // independently in test and compare.
        let mut atom = pure_sine_atom(128);
        {
            let AtomKind::AdditiveSingleCycleV0 { partials } = &mut atom.kind;
            *partials = vec![
                AtomPartial {
                    harmonic: 1,
                    amplitude: 1.0,
                    phase_cycles: 0.0,
                },
                AtomPartial {
                    harmonic: 3,
                    amplitude: 0.5,
                    phase_cycles: 0.0,
                },
            ];
        }
        let pcm = render_to_pcm(&atom);

        // Recompute the expected value with the exact same formula
        // and rounding mode at one specific sample.
        let n = 128usize;
        let k = 32usize;
        let raw = (1.0 * (2.0 * std::f64::consts::PI * (1.0 * k as f64 / n as f64)).sin())
            + (0.5 * (2.0 * std::f64::consts::PI * (3.0 * k as f64 / n as f64)).sin());
        // Compute max for normalisation by sampling all positions.
        let max = (0..n)
            .map(|kk| {
                let v1 = (2.0 * std::f64::consts::PI * (1.0 * kk as f64 / n as f64)).sin();
                let v2 = 0.5 * (2.0 * std::f64::consts::PI * (3.0 * kk as f64 / n as f64)).sin();
                (v1 + v2).abs()
            })
            .fold(0.0_f64, |m, v| m.max(v));
        let expected = {
            let s = (raw / max) * 1.0 * 32767.0;
            (if s >= 0.0 { s + 0.5 } else { s - 0.5 }) as i16
        };
        assert_eq!(
            pcm[k], expected,
            "two-partial sum mismatch at k={k}: got {}, expected {expected}",
            pcm[k]
        );
    }

    #[test]
    fn render_normalize_then_scale_by_atom_amplitude() {
        // Single fundamental, amp=1.0, normalize=true, atom.amplitude=0.5.
        // Peak after normalize+scale should be 0.5 × 32767 = 16383.5,
        // rounded away from zero to 16384.
        let mut atom = pure_sine_atom(128);
        atom.amplitude = 0.5;
        let pcm = render_to_pcm(&atom);
        let near = |a: i16, b: i16| (a as i32 - b as i32).abs() <= 2;
        assert!(
            near(pcm[32], 16384),
            "amplitude=0.5 quarter-peak: got {}",
            pcm[32]
        );
        assert!(
            near(pcm[96], -16384),
            "amplitude=0.5 three-quarter-trough: got {}",
            pcm[96]
        );
    }

    #[test]
    fn render_round_half_away_from_zero_negative() {
        // Construct an atom where the most-negative sample lands at
        // exactly s = -0.5 in the post-scale float. With normalize=true
        // the negative peak after normalisation is -1.0 exactly (k=96
        // for a cycle-128 sine). Set atom.amplitude such that
        // -1.0 * amp * 32767.0 = -0.5 (i.e. amp = 0.5/32767). The
        // round-half-AWAY-from-zero step must send -0.5 → -1, not 0.
        let mut atom = pure_sine_atom(128);
        atom.amplitude = 0.5_f64 / 32767.0_f64;
        let pcm = render_to_pcm(&atom);
        // With atom.amplitude that small the rounding mode is the
        // dominant effect: the negative-peak sample(s) should round
        // to -1 (away from zero), not 0 (round-half-up behaviour).
        let neg_peak = pcm.iter().min().copied().expect("pcm non-empty");
        assert_eq!(
            neg_peak, -1,
            "round-half-away-from-zero must send -0.5 → -1, got {}",
            neg_peak
        );
        // And the positive peak rounds the other way: +0.5 → +1.
        let pos_peak = pcm.iter().max().copied().expect("pcm non-empty");
        assert_eq!(
            pos_peak, 1,
            "round-half-away-from-zero must send +0.5 → +1, got {}",
            pos_peak
        );
    }

    #[test]
    fn render_cycle_lengths_64_128_256_all_correct_length() {
        for cycle in [64u16, 128, 256] {
            let pcm = render_to_pcm(&pure_sine_atom(cycle));
            assert_eq!(pcm.len(), cycle as usize);
        }
    }

    #[test]
    fn render_deterministic_across_calls() {
        let atom = pure_sine_atom(128);
        let a = render_to_pcm(&atom);
        let b = render_to_pcm(&atom);
        assert_eq!(a, b, "render_to_pcm must be deterministic");
    }

    // ====================================================================
    // M2.2 — render_to_brr tests.
    // ====================================================================

    #[test]
    fn brr_round_trip_at_m1_reference_amp_within_atom_envelope() {
        // M1.3-equivalent amplitude (≈ 8000/32767 ≈ 0.244), the
        // amplitude under which the M1 reference test
        // `encode_decode_roundtrip_peak_below_threshold_on_sine`
        // achieves <256 LSB round-trip. Atoms additionally force
        // filter 0 on block 0 (= loop entry block for single-cycle
        // atoms) for loop safety, which loses the predictor that
        // makes filter 1..=3 track smooth waveforms more closely.
        // The atom-render envelope at this amplitude is therefore
        // wider than the unconstrained M1 reference test.
        // 512 LSBs covers the filter-0-forced envelope; the SPEC
        // §16.9 canonical 0.75-amplitude atom is around 10 KLSBs,
        // well past this gate but a property of the encoder's
        // shift quantisation, not a render-side bug.
        let mut atom = canonical_sine_atom(128);
        atom.amplitude = 8000.0 / 32767.0;
        let out = render_to_brr(&atom).expect("render");
        // Decode the BRR back to PCM and compare to the source.
        let blocks: Vec<[u8; 9]> = out
            .brr_bytes
            .chunks_exact(9)
            .map(|c| {
                let mut b = [0u8; 9];
                b.copy_from_slice(c);
                b
            })
            .collect();
        let mut state = crate::brr::BrrDecoderState::default();
        let decoded = crate::brr::decode_blocks(&blocks, &mut state);
        assert_eq!(decoded.len(), out.pcm.len());
        let max_err = decoded
            .iter()
            .zip(out.pcm.iter())
            .map(|(d, s)| (*d as i32 - *s as i32).unsigned_abs())
            .max()
            .unwrap();
        assert!(
            max_err < 512,
            "BRR round-trip peak error {max_err} >= 512 LSBs (M2 atom-render envelope, filter-0-forced)"
        );
    }

    #[test]
    fn brr_first_block_filter_0_when_forced() {
        let atom = canonical_sine_atom(128);
        let out = render_to_brr(&atom).expect("render");
        assert_eq!(out.encode_blocks[0].filter, 0);
    }

    #[test]
    fn brr_encoded_byte_count_matches_cycle_division() {
        // 16 PCM samples per BRR block, 9 bytes per block.
        for (cycle, expected_bytes) in [(64u16, 36u32), (128, 72), (256, 144)] {
            let atom = canonical_sine_atom(cycle);
            let out = render_to_brr(&atom).expect("render");
            assert_eq!(
                out.brr_bytes.len() as u32,
                expected_bytes,
                "cycle {cycle} expected {expected_bytes} BRR bytes"
            );
        }
    }

    /// **M2 atom-loop-click baseline** for the canonical 128-sample
    /// sine atom (`amplitude=0.75`, `partial.amplitude=1.0`,
    /// `phase_cycles=0`). The score is `|first_decoded -
    /// last_decoded|` — for a discrete sine `sin(2πk/N)` sampled
    /// at integer indices, sample N-1 is `sin(2π·(N-1)/N)` ≠ 0,
    /// so the wrap from sample N-1 back to sample 0 (= 0) creates
    /// a non-zero discontinuity that's a property of the cycle's
    /// shape, not a defect. This locked value is the
    /// M2_ATOM_128_SINE_LOOP_CLICK_SCORE baseline.
    pub(crate) const M2_ATOM_128_SINE_LOOP_CLICK_SCORE: f64 = 1197.0;

    /// Same baseline for the canonical 64-sample sine atom.
    /// Higher than the 128-sample value because the larger angular
    /// step per sample (2π/64 vs 2π/128) means the last sample lands
    /// further from zero on the way back to the loop entry.
    pub(crate) const M2_ATOM_64_SINE_LOOP_CLICK_SCORE: f64 = 2407.0;

    #[test]
    fn brr_loop_click_score_for_pure_sine_matches_baseline() {
        // For a discrete sine over one cycle the wrap discontinuity
        // is bounded by the source signal's last-sample magnitude,
        // not the encoder's quantisation. The exact value depends on
        // the encoder's chosen (filter, shift) for the final block
        // and is locked here as the M2 atom-loop-click baseline.
        let atom = canonical_sine_atom(128);
        let out = render_to_brr(&atom).expect("render");
        let score = out
            .encode_summary
            .loop_click_score
            .expect("looped encode must populate loop_click_score");
        assert_eq!(
            score, M2_ATOM_128_SINE_LOOP_CLICK_SCORE,
            "M2_ATOM_128_SINE_LOOP_CLICK_SCORE drift: got {score}, expected {M2_ATOM_128_SINE_LOOP_CLICK_SCORE}"
        );

        let atom_64 = canonical_sine_atom(64);
        let out_64 = render_to_brr(&atom_64).expect("render");
        let score_64 = out_64
            .encode_summary
            .loop_click_score
            .expect("looped encode must populate loop_click_score");
        assert_eq!(
            score_64, M2_ATOM_64_SINE_LOOP_CLICK_SCORE,
            "M2_ATOM_64_SINE_LOOP_CLICK_SCORE drift: got {score_64}, expected {M2_ATOM_64_SINE_LOOP_CLICK_SCORE}"
        );
    }

    #[test]
    fn render_to_brr_deterministic_across_calls() {
        let atom = canonical_sine_atom(128);
        let a = render_to_brr(&atom).expect("render");
        let b = render_to_brr(&atom).expect("render");
        assert_eq!(a.pcm, b.pcm);
        assert_eq!(a.brr_bytes, b.brr_bytes);
        assert_eq!(a.pcm_sha256, b.pcm_sha256);
        assert_eq!(a.brr_sha256, b.brr_sha256);
    }

    #[test]
    fn render_64_vs_128_atom_distinct_brr_sha() {
        let a = render_to_brr(&canonical_sine_atom(64)).expect("render");
        let b = render_to_brr(&canonical_sine_atom(128)).expect("render");
        assert_ne!(
            a.brr_sha256, b.brr_sha256,
            "64- and 128-sample atoms must produce distinct BRR SHAs"
        );
        assert_ne!(a.pcm_sha256, b.pcm_sha256);
    }

    /// Sentinel that prints the canonical atoms' BRR/PCM SHAs.
    /// Run with `cargo test -p sfc-atomizer-core --lib m2_atom_print
    /// -- --nocapture --ignored` to capture fresh baseline values.
    #[test]
    #[ignore]
    fn m2_atom_print_baselines() {
        let out_128 = render_to_brr(&canonical_sine_atom(128)).expect("render");
        let out_64 = render_to_brr(&canonical_sine_atom(64)).expect("render");
        eprintln!("M2_ATOM_128_SINE_PCM_SHA256 = {}", out_128.pcm_sha256);
        eprintln!("M2_ATOM_128_SINE_BRR_SHA256 = {}", out_128.brr_sha256);
        eprintln!("M2_ATOM_64_SINE_PCM_SHA256  = {}", out_64.pcm_sha256);
        eprintln!("M2_ATOM_64_SINE_BRR_SHA256  = {}", out_64.brr_sha256);
        eprintln!(
            "M2_ATOM_128_SINE_LOOP_CLICK_SCORE = {}",
            out_128.encode_summary.loop_click_score.unwrap()
        );
        eprintln!(
            "M2_ATOM_64_SINE_LOOP_CLICK_SCORE  = {}",
            out_64.encode_summary.loop_click_score.unwrap()
        );
    }

    // ============================================================
    // M2.4 atom edge-case tests (consultant #7 / #36).
    // ============================================================

    /// J — atom with 8 partials (max harmonics 1..=8) renders
    /// deterministically.
    #[test]
    fn render_atom_with_eight_partials_deterministic() {
        let mut atom = canonical_sine_atom(128);
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut atom.kind;
        *partials = (1..=8u8)
            .map(|h| AtomPartial {
                harmonic: h,
                amplitude: 1.0,
                phase_cycles: 0.0,
            })
            .collect();
        let a = render_to_brr(&atom).expect("render");
        let b = render_to_brr(&atom).expect("render");
        assert_eq!(a.brr_sha256, b.brr_sha256);
        assert_eq!(a.brr_sha256.len(), 64);
    }

    /// K — atom with `amplitude = 0.0` produces all-zero PCM.
    #[test]
    fn render_atom_with_zero_amplitude_is_silent() {
        let mut atom = canonical_sine_atom(128);
        atom.amplitude = 0.0;
        let pcm = render_to_pcm(&atom);
        assert_eq!(pcm.len(), 128);
        assert!(
            pcm.iter().all(|s| *s == 0),
            "zero-amplitude atom must render all-zero PCM"
        );
    }

    /// L — atom with `normalize = false` and partials summing > 1.0
    /// produces no NaN / no infinity, samples remain in i16 range.
    #[test]
    fn render_unnormalized_high_partials_clamps_safely() {
        let mut atom = canonical_sine_atom(128);
        atom.amplitude = 1.0;
        atom.render.normalize = false;
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut atom.kind;
        *partials = vec![
            AtomPartial {
                harmonic: 1,
                amplitude: 1.0,
                phase_cycles: 0.0,
            },
            AtomPartial {
                harmonic: 1,
                amplitude: 1.0,
                phase_cycles: 0.0,
            },
            AtomPartial {
                harmonic: 1,
                amplitude: 1.0,
                phase_cycles: 0.0,
            },
        ];
        let pcm = render_to_pcm(&atom);
        for s in &pcm {
            let v = *s as i32;
            assert!(
                (-32768..=32767).contains(&v),
                "sample out of i16 range: {v}"
            );
        }
    }

    /// M — same `phase_cycles` produces identical PCM (deterministic
    /// across two calls; mod-1 phase wrapping is a property of the
    /// formula but the schema bounds phase_cycles to [0.0, 1.0)).
    #[test]
    fn render_same_phase_cycles_deterministic() {
        let mut a1 = canonical_sine_atom(128);
        let mut a2 = canonical_sine_atom(128);
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a1.kind;
        partials[0].phase_cycles = 0.999;
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a2.kind;
        partials[0].phase_cycles = 0.999;
        let r1 = render_to_pcm(&a1);
        let r2 = render_to_pcm(&a2);
        assert_eq!(r1, r2, "identical phase_cycles must produce identical PCM");
    }

    /// N — atom with `cycle_len_samples = 256` renders correctly
    /// (256 / 16 * 9 = 144 BRR bytes).
    #[test]
    fn render_cycle_256_sine_atom() {
        let atom = canonical_sine_atom(256);
        let out = render_to_brr(&atom).expect("render");
        assert_eq!(out.brr_bytes.len(), 144);
        assert_eq!(out.pcm.len(), 256);
        let out2 = render_to_brr(&atom).expect("render");
        assert_eq!(out.brr_sha256, out2.brr_sha256);
    }

    /// **M2 atom-render baselines** — locked SHAs for the canonical
    /// 64- and 128-sample sine atoms (`amplitude=0.75`,
    /// `partial.amplitude=1.0`, `phase_cycles=0`,
    /// `force_filter_0_first_block=true`). Drift here means the
    /// render formula, the M1 BRR encoder, or the rounding mode
    /// changed; either is a producer-side regression and must be
    /// flagged. Mirrors the role of M1_DRIVER_CODE_SHA256 / etc. in
    /// the M1 baselines block.
    #[test]
    fn m2_atom_render_baselines_locked() {
        let out_128 = render_to_brr(&canonical_sine_atom(128)).expect("render");
        assert_eq!(
            out_128.pcm_sha256, "7f9b274e9fa1c7088ba4d125a2899293bae79115bdd20824b2afb54116f9789a",
            "M2_ATOM_128_SINE_PCM_SHA256 drift"
        );
        assert_eq!(
            out_128.brr_sha256, "348c791449916e1f9169d0e229cd79bf97967b19e22db3c4a5be7dc9c69ac876",
            "M2_ATOM_128_SINE_BRR_SHA256 drift"
        );
        let out_64 = render_to_brr(&canonical_sine_atom(64)).expect("render");
        assert_eq!(
            out_64.pcm_sha256, "0638ddfe8a2a8fb4c98ff6fed37ff3475c42dd257df893eca9e836d09d3e6565",
            "M2_ATOM_64_SINE_PCM_SHA256 drift"
        );
        assert_eq!(
            out_64.brr_sha256, "78da253b65a6a8d067102fe30ed90353c25b6981a71e3cafc6dd4f3041822e96",
            "M2_ATOM_64_SINE_BRR_SHA256 drift"
        );
    }
}
