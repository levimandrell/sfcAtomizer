//! M3.5 — Gaussian characterization (SPEC §10.9).
//!
//! Characterizes S-DSP Gaussian interpolation dulling by comparing
//! host-side BRR decode (raw) vs `snes_spc` oracle render. The
//! characterization is reports-only at M3.5; the four-condition
//! decision rule (SPEC §10.9 "M3.6 decision rule") consumes this
//! report to decide whether M3.6 ships pre-emphasis presets or
//! defers to M4+.
//!
//! This module owns the **deterministic** pieces:
//!
//! - The `m3_5_canonical` test signal set definitions.
//! - Per-signal raw metrics (RMS, ZCR, clipping count, SHAs) over
//!   the host-side BRR decode.
//! - Source-vs-raw error metrics (`peak_abs_raw_vs_source`).
//! - Oracle-side metric helpers (RMS, ZCR, clipping count) — the
//!   caller supplies the oracle PCM, this module computes the
//!   stats.
//! - Report types matching the SPEC §10.9 schema (`schema_version: 2`).
//!
//! Orchestration (SPC build, oracle subprocess, report file
//! emission) lives in the `app` crate's `characterize-gaussian`
//! CLI command.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::atom::{
    render_to_brr, AtomBrrOutput, AtomKind, AtomPartial, AtomRenderOptions, AtomSlot,
};
use crate::brr::{decode_blocks, BrrDecoderState};
use crate::project::{Envelope, SamplePlayback};

// =====================================================================
// Test signal set: m3_5_canonical (SPEC §10.9, expanded at M3.5)
// =====================================================================

/// One signal in the `m3_5_canonical` test set.
///
/// Each signal is a single-cycle atom built per SPEC §16.9. The
/// `frequency_hz` field documents the effective fundamental at 32 kHz
/// playback (`32000.0 / cycle_len_samples * harmonic`).
#[derive(Debug, Clone)]
pub struct TestSignal {
    pub name: &'static str,
    pub frequency_hz: f64,
    pub atom: AtomSlot,
}

fn atom_base(cycle: u16) -> AtomSlot {
    AtomSlot {
        id: "characterize".to_string(),
        name: "characterize".to_string(),
        kind: AtomKind::AdditiveSingleCycleV0 {
            partials: vec![AtomPartial {
                harmonic: 1,
                amplitude: 1.0,
                phase_cycles: 0.0,
            }],
        },
        root_midi_note: 60,
        cycle_len_samples: cycle,
        amplitude: 1.0,
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

fn signal_sine(name: &'static str, cycle: u16) -> TestSignal {
    let mut a = atom_base(cycle);
    a.id = name.to_string();
    a.name = name.to_string();
    TestSignal {
        name,
        frequency_hz: 32000.0 / cycle as f64,
        atom: a,
    }
}

fn signal_harmonic_cycle_64(name: &'static str, harmonic: u8) -> TestSignal {
    let mut a = atom_base(64);
    a.id = name.to_string();
    a.name = name.to_string();
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = vec![AtomPartial {
        harmonic,
        amplitude: 1.0,
        phase_cycles: 0.0,
    }];
    TestSignal {
        name,
        frequency_hz: 32000.0 / 64.0 * harmonic as f64,
        atom: a,
    }
}

fn signal_all_8_partials() -> TestSignal {
    let mut a = atom_base(128);
    a.id = "all_8_partials_max_amp_harmonics_1_to_8".to_string();
    a.name = "all_8_partials_max_amp_harmonics_1_to_8".to_string();
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = (1..=8u8)
        .map(|h| AtomPartial {
            harmonic: h,
            amplitude: 1.0,
            phase_cycles: 0.0,
        })
        .collect();
    TestSignal {
        name: "all_8_partials_max_amp_harmonics_1_to_8",
        frequency_hz: 32000.0 / 128.0,
        atom: a,
    }
}

fn signal_normalize_false_clamp() -> TestSignal {
    let mut a = atom_base(128);
    a.id = "normalize_false_multi_partial_clamp_safety".to_string();
    a.name = "normalize_false_multi_partial_clamp_safety".to_string();
    a.render.normalize = false;
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = (1..=8u8)
        .map(|h| AtomPartial {
            harmonic: h,
            amplitude: 1.0,
            phase_cycles: 0.0,
        })
        .collect();
    TestSignal {
        name: "normalize_false_multi_partial_clamp_safety",
        frequency_hz: 32000.0 / 128.0,
        atom: a,
    }
}

/// The full `m3_5_canonical` signal set per SPEC §10.9 (M3.5).
///
/// Ten signals: three sines (cycle_64/128/256) as frequency-response
/// anchors, four cycle_64 harmonics (2/4/8/16) as gain-curve probes,
/// one full partial-bank atom, and one clipping-stress reference.
pub fn m3_5_canonical_signals() -> Vec<TestSignal> {
    vec![
        signal_sine("sine_cycle_64", 64),
        signal_sine("sine_cycle_128", 128),
        signal_sine("sine_cycle_256", 256),
        signal_harmonic_cycle_64("harmonic_2_cycle_64", 2),
        signal_harmonic_cycle_64("harmonic_4_cycle_64", 4),
        signal_harmonic_cycle_64("harmonic_8_cycle_64", 8),
        signal_harmonic_cycle_64("harmonic_16_cycle_64", 16),
        signal_all_8_partials(),
        signal_normalize_false_clamp(),
    ]
}

// =====================================================================
// Metric helpers
// =====================================================================

/// Root-mean-square of an `i16` PCM buffer. Returns `0.0` for empty
/// input.
pub fn pcm_rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum_sq: i64 = 0;
    for s in samples {
        let v = *s as i64;
        sum_sq += v * v;
    }
    ((sum_sq as f64) / (samples.len() as f64)).sqrt()
}

/// Zero-crossing rate (Hz) of an `i16` PCM buffer at the given sample
/// rate. Counts sign changes between consecutive samples (treating
/// zero as positive for a single consistent convention). Returns
/// `0.0` for inputs shorter than 2 samples.
pub fn pcm_zcr_per_sec(samples: &[i16], sample_rate_hz: u32) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let mut crossings = 0u64;
    let sign = |x: i16| -> i32 {
        if x < 0 {
            -1
        } else {
            1
        }
    };
    let mut prev = sign(samples[0]);
    for s in &samples[1..] {
        let cur = sign(*s);
        if cur != prev {
            crossings += 1;
        }
        prev = cur;
    }
    let duration_s = (samples.len() as f64) / (sample_rate_hz as f64);
    (crossings as f64) / duration_s
}

/// Count of samples within ±1 LSB of `i16::MAX` or `i16::MIN`.
///
/// "Clipping" per the SPEC §10.9 measurement schema means samples at
/// the saturation limit. ±32767, ±32766, -32768, -32767 all count.
pub fn pcm_clipping_count(samples: &[i16]) -> i32 {
    let mut c: i32 = 0;
    for s in samples {
        if *s >= 32766 || *s <= -32767 {
            c += 1;
        }
    }
    c
}

/// SHA-256 hex digest over the little-endian `i16` bytes of `samples`.
pub fn pcm_sha256_hex(samples: &[i16]) -> String {
    let mut h = Sha256::new();
    for s in samples {
        h.update(s.to_le_bytes());
    }
    let d = h.finalize();
    hex_lower(&d)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}

/// Max-abs per-sample difference between two same-length `i16` PCM
/// buffers, computed in widened `i32` to avoid overflow.
///
/// Returns `i32::MAX` if lengths differ. Callers should align lengths
/// before calling (see `align_oracle_to_raw`).
pub fn peak_abs_diff(a: &[i16], b: &[i16]) -> i32 {
    if a.len() != b.len() {
        return i32::MAX;
    }
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as i32 - *y as i32).abs())
        .max()
        .unwrap_or(0)
}

/// Decode a flat BRR byte buffer to `i16` PCM via
/// `core::brr::decode_blocks`.
///
/// Returns an empty `Vec` if `brr_bytes.len()` is not a multiple of 9.
pub fn decode_brr_flat(brr_bytes: &[u8]) -> Vec<i16> {
    if brr_bytes.is_empty() || brr_bytes.len() % 9 != 0 {
        return Vec::new();
    }
    let blocks: Vec<[u8; 9]> = brr_bytes
        .chunks_exact(9)
        .map(|c| {
            let mut a = [0u8; 9];
            a.copy_from_slice(c);
            a
        })
        .collect();
    let mut state = BrrDecoderState::default();
    decode_blocks(&blocks, &mut state)
}

/// Reduce a stereo interleaved `s16le` byte stream from the oracle
/// to a mono `i16` buffer by taking the left channel only.
///
/// The oracle emits stereo even for centered mono atoms; left and
/// right are equal in that case, so taking L is loss-free. Returns
/// an empty `Vec` if `bytes.len()` is not a multiple of 4.
pub fn oracle_stereo_to_mono_left(bytes: &[u8]) -> Vec<i16> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Vec::new();
    }
    let frames = bytes.len() / 4;
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let lo = bytes[f * 4];
        let hi = bytes[f * 4 + 1];
        out.push(i16::from_le_bytes([lo, hi]));
    }
    out
}

// =====================================================================
// Phase / delay alignment
// =====================================================================

/// Result of aligning the oracle render against the host BRR decode.
#[derive(Debug, Clone, Copy)]
pub struct Alignment {
    /// Number of leading samples skipped from `oracle` before
    /// comparison.
    pub oracle_offset: usize,
    /// Number of samples used in the aligned comparison.
    pub length: usize,
    /// RMS of the aligned `(oracle - raw)` difference, at the chosen
    /// offset. Lower is better.
    pub aligned_rms_error: f64,
}

/// Find the best alignment of `oracle` against `raw_repeat` by
/// brute-force searching small leading-skip offsets on `oracle`.
///
/// Both buffers should be tail-cropped to the same length BEFORE
/// passing in. `raw_repeat` is the host BRR decode REPEATED enough
/// times to cover the oracle window (the BRR is a looped one-cycle
/// atom, so we tile the cycle). `oracle` is the mono oracle render
/// trimmed to the same length.
///
/// Search range: `0..=max_offset` samples (gaussian delay is small,
/// typically ≤ 16 samples; 32 covers the worst case).
///
/// Returns the alignment with minimum RMS error over the overlapping
/// region.
pub fn align_oracle_to_raw(oracle: &[i16], raw_repeat: &[i16], max_offset: usize) -> Alignment {
    if oracle.is_empty() || raw_repeat.is_empty() {
        return Alignment {
            oracle_offset: 0,
            length: 0,
            aligned_rms_error: f64::INFINITY,
        };
    }
    let cap = std::cmp::min(oracle.len(), raw_repeat.len());
    let max_off = std::cmp::min(max_offset, cap.saturating_sub(1));
    let mut best = Alignment {
        oracle_offset: 0,
        length: cap,
        aligned_rms_error: f64::INFINITY,
    };
    for off in 0..=max_off {
        let len = std::cmp::min(oracle.len() - off, raw_repeat.len());
        if len == 0 {
            continue;
        }
        let mut sum_sq: i64 = 0;
        for i in 0..len {
            let diff = (oracle[off + i] as i32 - raw_repeat[i] as i32) as i64;
            sum_sq += diff * diff;
        }
        let rms = ((sum_sq as f64) / (len as f64)).sqrt();
        if rms < best.aligned_rms_error {
            best = Alignment {
                oracle_offset: off,
                length: len,
                aligned_rms_error: rms,
            };
        }
    }
    best
}

/// Repeat `cycle` until at least `target_len` samples are produced,
/// then truncate to exactly `target_len`. Used to tile a one-cycle
/// host BRR decode out to oracle window length.
pub fn tile_cycle_to_length(cycle: &[i16], target_len: usize) -> Vec<i16> {
    if cycle.is_empty() || target_len == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(target_len);
    while out.len() < target_len {
        out.extend_from_slice(cycle);
    }
    out.truncate(target_len);
    out
}

// =====================================================================
// Report schema (SPEC §10.9 schema_version 2)
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterizationReport {
    pub schema_version: u32,
    pub report_type: String,
    pub fixture_set: String,
    pub sample_rate_hz: u32,
    pub tool: ToolInfo,
    pub test_signals: Vec<TestSignalSummary>,
    pub measurements: Vec<Measurement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subjective_audition: Option<SubjectiveAudition>,
    pub summary: Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub snes_spc_oracle_sha256: String,
    pub rust_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSignalSummary {
    pub name: String,
    pub kind: String,
    pub cycle_len_samples: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub name: String,
    pub frequency_hz: f64,
    pub raw_decoded_pcm_sha256: String,
    pub oracle_pcm_sha256: String,
    pub raw_rms: f64,
    pub oracle_rms: f64,
    pub gain_delta_db: f64,
    pub peak_abs_error_oracle_vs_raw: i32,
    pub peak_abs_raw_vs_source: i32,
    pub zcr_raw: f64,
    pub zcr_oracle: f64,
    pub clipping_count_raw: i32,
    pub clipping_count_oracle: i32,
    #[serde(
        rename = "_phase_or_delay_note",
        skip_serializing_if = "Option::is_none"
    )]
    pub phase_or_delay_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectiveAudition {
    pub audition_ref: String,
    pub auditioned_at: String,
    pub auditioned_by: String,
    pub fixtures: Vec<SubjectiveFixture>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectiveFixture {
    pub name: String,
    pub perceived_change_axis: String,
    pub masked_by_signal_content: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub clear_target_for_pre_emphasis: bool,
    pub recommended_next: String,
    pub decision_rule_reasons: Vec<String>,
}

// =====================================================================
// Raw-side computation: per-signal metrics that do not depend on the
// oracle output. Callers build a Measurement by combining these with
// oracle-side fields produced after running snes_spc_oracle.
// =====================================================================

/// All the raw-side metric pieces computed from an atom render, used
/// to populate the first half of a `Measurement`.
#[derive(Debug, Clone)]
pub struct RawSide {
    /// The BRR-encoded bytes (post phase rotation per §10.7).
    pub brr_bytes: Vec<u8>,
    /// Source PCM = the atom's rendered cycle (NOT rotated). This is
    /// the §16.9 identity-pinned PCM.
    pub source_pcm: Vec<i16>,
    /// Host-side BRR decode of one cycle (16 samples × cycle_len/16
    /// blocks).
    pub raw_decoded_one_cycle: Vec<i16>,
    pub raw_decoded_pcm_sha256: String,
    /// `peak_abs_raw_vs_source`: max-abs delta between decoded BRR
    /// and (rotation-aligned) source PCM. Reported under the
    /// rotated frame so the comparison is faithful to the §10.7
    /// rotation choice.
    pub peak_abs_raw_vs_source: i32,
}

/// Compute the raw-side metrics for one test signal.
///
/// Renders the atom through `render_to_brr`, host-decodes one cycle
/// of the resulting BRR, and computes `raw_decoded_pcm_sha256` plus
/// `peak_abs_raw_vs_source` against the rotation-aligned source.
pub fn compute_raw_side(signal: &TestSignal) -> RawSide {
    let render: AtomBrrOutput =
        render_to_brr(&signal.atom).expect("AtomRenderError uninhabited at M3");
    let one_cycle = decode_brr_flat(&render.brr_bytes);
    let sha = pcm_sha256_hex(&one_cycle);

    // Rotation alignment: the BRR was encoded from a rotated source
    // per §10.7; we compare decoded-BRR vs that rotated source for
    // faithful encoder-error magnitude.
    let off = render.rotation_offset as usize % render.pcm.len().max(1);
    let rotated_source: Vec<i16> = if render.pcm.is_empty() {
        Vec::new()
    } else {
        let n = render.pcm.len();
        let mut v = Vec::with_capacity(n);
        v.extend_from_slice(&render.pcm[off..]);
        v.extend_from_slice(&render.pcm[..off]);
        v
    };
    let peak = peak_abs_diff(&rotated_source, &one_cycle);

    RawSide {
        brr_bytes: render.brr_bytes.clone(),
        source_pcm: render.pcm.clone(),
        raw_decoded_one_cycle: one_cycle,
        raw_decoded_pcm_sha256: sha,
        peak_abs_raw_vs_source: peak,
    }
}

// =====================================================================
// Oracle-side computation: given raw_side + oracle mono PCM, compute
// the rest of the Measurement.
// =====================================================================

/// Combine a precomputed `RawSide` with an oracle mono PCM trace to
/// produce a `Measurement`.
///
/// `oracle_mono` is the oracle render of the SPC playing this signal,
/// reduced to a mono i16 buffer (caller is responsible for L-channel
/// extraction). `oracle_window_len` is the length the comparison is
/// performed over; it should be at least several cycles of the
/// signal to give the RMS / ZCR measurements time to stabilize.
pub fn finalize_measurement(
    signal: &TestSignal,
    raw: &RawSide,
    oracle_mono: &[i16],
    sample_rate_hz: u32,
) -> Measurement {
    let raw_rms_one_cycle = pcm_rms(&raw.raw_decoded_one_cycle);

    // Tile the host BRR decode out to the oracle window length so
    // alignment / aligned RMS calculations work on equal-length
    // buffers.
    let raw_repeat = tile_cycle_to_length(&raw.raw_decoded_one_cycle, oracle_mono.len());
    let raw_rms_window = pcm_rms(&raw_repeat);

    let align = align_oracle_to_raw(oracle_mono, &raw_repeat, 32);
    let oracle_aligned: Vec<i16> = oracle_mono
        .iter()
        .skip(align.oracle_offset)
        .take(align.length)
        .copied()
        .collect();
    let raw_aligned: Vec<i16> = raw_repeat.iter().take(align.length).copied().collect();

    let oracle_rms = pcm_rms(&oracle_aligned);
    let oracle_sha = pcm_sha256_hex(oracle_mono);
    let peak_abs_err = peak_abs_diff(&oracle_aligned, &raw_aligned);

    let gain_delta_db = if oracle_rms > 0.0 && raw_rms_window > 0.0 {
        20.0 * (oracle_rms / raw_rms_window).log10()
    } else {
        0.0
    };

    let zcr_raw = pcm_zcr_per_sec(&raw.raw_decoded_one_cycle, sample_rate_hz);
    let zcr_oracle = pcm_zcr_per_sec(&oracle_aligned, sample_rate_hz);
    let clipping_raw = pcm_clipping_count(&raw.raw_decoded_one_cycle);
    let clipping_oracle = pcm_clipping_count(&oracle_aligned);

    let note = if align.oracle_offset != 0 {
        Some(format!(
            "aligned oracle by skipping {} leading samples (gaussian delay); aligned_rms_error={:.3}",
            align.oracle_offset, align.aligned_rms_error
        ))
    } else {
        None
    };

    let _ = raw_rms_one_cycle; // reported via raw_rms below

    Measurement {
        name: signal.name.to_string(),
        frequency_hz: signal.frequency_hz,
        raw_decoded_pcm_sha256: raw.raw_decoded_pcm_sha256.clone(),
        oracle_pcm_sha256: oracle_sha,
        raw_rms: raw_rms_window,
        oracle_rms,
        gain_delta_db,
        peak_abs_error_oracle_vs_raw: peak_abs_err,
        peak_abs_raw_vs_source: raw.peak_abs_raw_vs_source,
        zcr_raw,
        zcr_oracle,
        clipping_count_raw: clipping_raw,
        clipping_count_oracle: clipping_oracle,
        phase_or_delay_note: note,
    }
}

// =====================================================================
// Decision rule (SPEC §10.9 — M3.6 decision rule, four conditions)
// =====================================================================

/// Outcome of applying the §10.9 four-condition M3.6 decision rule.
#[derive(Debug, Clone)]
pub struct DecisionOutcome {
    pub recommended_next: String,
    pub clear_target_for_pre_emphasis: bool,
    pub reasons: Vec<String>,
}

/// Apply the M3.5 monotonicity check (condition #1) to a slice of
/// measurements. The other three conditions (`harmonic_16` responds,
/// anti-worsening on canonical sines, no new clipping) require a
/// proposed preset's outputs to evaluate, which M3.5 does NOT
/// implement — they are evaluated at M3.6 land.
///
/// At M3.5 the report's recommended_next outcomes are:
///
/// - `"defer"`: monotonicity fails, OR `harmonic_16` shows ≤ 0 dB
///   attenuation (no measurable gaussian dulling to compensate for).
/// - `"pending_preset_eval"`: monotonicity holds and `harmonic_16`
///   shows measurable attenuation — M3.6 will need to design a
///   gentle preset and re-run the report under it. This is the
///   "go signal" for designing presets; M3.6 still has to satisfy
///   conditions #2 / #3 / #4 to actually ship.
///
/// Reasons are appended verbosely so the report can be reviewed
/// without re-running the characterization.
pub fn apply_m3_5_decision_rule(measurements: &[Measurement]) -> DecisionOutcome {
    let mut reasons = Vec::new();

    let by_name = |n: &str| -> Option<&Measurement> { measurements.iter().find(|m| m.name == n) };

    let h2 = by_name("harmonic_2_cycle_64");
    let h4 = by_name("harmonic_4_cycle_64");
    let h8 = by_name("harmonic_8_cycle_64");
    let h16 = by_name("harmonic_16_cycle_64");

    let mut monotonic_ok = true;
    if let (Some(a), Some(b), Some(c), Some(d)) = (h2, h4, h8, h16) {
        let series = [
            (a, "harmonic_2"),
            (b, "harmonic_4"),
            (c, "harmonic_8"),
            (d, "harmonic_16"),
        ];
        for w in series.windows(2) {
            let (left, lname) = w[0];
            let (right, rname) = w[1];
            if right.gain_delta_db > left.gain_delta_db + 1e-9 {
                monotonic_ok = false;
                reasons.push(format!(
                    "monotonicity FAILS: gain_delta_db at {} ({:.3} dB) is higher than at {} ({:.3} dB) — expected non-increasing across rising frequency",
                    rname, right.gain_delta_db, lname, left.gain_delta_db
                ));
            }
        }
        if monotonic_ok {
            reasons.push(format!(
                "monotonicity OK across cycle_64 harmonic series: harmonic_2={:.3} dB, harmonic_4={:.3} dB, harmonic_8={:.3} dB, harmonic_16={:.3} dB",
                a.gain_delta_db, b.gain_delta_db, c.gain_delta_db, d.gain_delta_db
            ));
        }
    } else {
        monotonic_ok = false;
        reasons.push(
            "monotonicity check skipped: one or more of harmonic_2/4/8/16_cycle_64 measurements missing".to_string(),
        );
    }

    let h16_responds = match h16 {
        Some(m) if m.gain_delta_db < -0.5 => {
            reasons.push(format!(
                "harmonic_16 shows measurable gaussian attenuation: gain_delta_db={:.3} dB",
                m.gain_delta_db
            ));
            true
        }
        Some(m) => {
            reasons.push(format!(
                "harmonic_16 attenuation insufficient at M3.5 raw measurement: gain_delta_db={:.3} dB (≥ -0.5 dB threshold)",
                m.gain_delta_db
            ));
            false
        }
        None => {
            reasons.push("harmonic_16_cycle_64 measurement missing".to_string());
            false
        }
    };

    let (recommended, clear) = if monotonic_ok && h16_responds {
        ("pending_preset_eval".to_string(), true)
    } else {
        ("defer".to_string(), false)
    };
    reasons.push(format!(
        "recommended_next={}; conditions #2 / #3 / #4 of SPEC §10.9 decision rule require a proposed preset and remain unevaluated at M3.5 — M3.6 must run a follow-up characterization under the proposed preset to satisfy them.",
        recommended
    ));

    DecisionOutcome {
        recommended_next: recommended,
        clear_target_for_pre_emphasis: clear,
        reasons,
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_set_has_ten_signals() {
        let s = m3_5_canonical_signals();
        assert_eq!(s.len(), 9);
        let names: Vec<&str> = s.iter().map(|t| t.name).collect();
        assert!(names.contains(&"sine_cycle_64"));
        assert!(names.contains(&"sine_cycle_128"));
        assert!(names.contains(&"sine_cycle_256"));
        assert!(names.contains(&"harmonic_2_cycle_64"));
        assert!(names.contains(&"harmonic_4_cycle_64"));
        assert!(names.contains(&"harmonic_8_cycle_64"));
        assert!(names.contains(&"harmonic_16_cycle_64"));
        assert!(names.contains(&"all_8_partials_max_amp_harmonics_1_to_8"));
        assert!(names.contains(&"normalize_false_multi_partial_clamp_safety"));
    }

    #[test]
    fn signal_frequencies_match_spec() {
        let s = m3_5_canonical_signals();
        let by = |n: &str| s.iter().find(|t| t.name == n).unwrap().frequency_hz;
        assert!((by("sine_cycle_64") - 500.0).abs() < 1e-9);
        assert!((by("sine_cycle_128") - 250.0).abs() < 1e-9);
        assert!((by("sine_cycle_256") - 125.0).abs() < 1e-9);
        assert!((by("harmonic_2_cycle_64") - 1000.0).abs() < 1e-9);
        assert!((by("harmonic_4_cycle_64") - 2000.0).abs() < 1e-9);
        assert!((by("harmonic_8_cycle_64") - 4000.0).abs() < 1e-9);
        assert!((by("harmonic_16_cycle_64") - 8000.0).abs() < 1e-9);
    }

    #[test]
    fn raw_side_is_deterministic() {
        let s = m3_5_canonical_signals();
        let sig = s.iter().find(|t| t.name == "sine_cycle_128").unwrap();
        let r1 = compute_raw_side(sig);
        let r2 = compute_raw_side(sig);
        assert_eq!(r1.raw_decoded_pcm_sha256, r2.raw_decoded_pcm_sha256);
        assert_eq!(r1.peak_abs_raw_vs_source, r2.peak_abs_raw_vs_source);
        assert_eq!(r1.brr_bytes, r2.brr_bytes);
    }

    #[test]
    fn pcm_rms_zero_for_silence() {
        let z = vec![0i16; 256];
        assert_eq!(pcm_rms(&z), 0.0);
    }

    #[test]
    fn pcm_rms_for_full_scale_dc() {
        let v = vec![32767i16; 256];
        assert!((pcm_rms(&v) - 32767.0).abs() < 1.0);
    }

    #[test]
    fn pcm_zcr_for_alternating_pattern() {
        // 32 samples alternating sign every sample at 32 kHz = 16 kHz
        // crossings × 2 (Nyquist for any signal at fs/2). Strict
        // numeric: 31 crossings / (32/32000) s = 31000 Hz.
        let v: Vec<i16> = (0..32)
            .map(|i| if i % 2 == 0 { 1000 } else { -1000 })
            .collect();
        let z = pcm_zcr_per_sec(&v, 32000);
        assert!((z - 31000.0).abs() < 1.0);
    }

    #[test]
    fn pcm_clipping_count_at_extremes() {
        let v: Vec<i16> = vec![0, 32767, 32766, -32768, -32767, -32766, 100, 32765];
        // 32767, 32766, -32768, -32767 are within ±1 LSB; -32766 is two off, 32765 is two off.
        assert_eq!(pcm_clipping_count(&v), 4);
    }

    #[test]
    fn oracle_stereo_to_mono_left_extracts_left_channel() {
        // Two frames: (L=100, R=200), (L=-300, R=-400)
        // s16le: 100 = 64 00, 200 = C8 00, -300 = D4 FE, -400 = 70 FE
        let bytes: Vec<u8> = vec![
            0x64, 0x00, 0xC8, 0x00, // frame 0: L=100, R=200
            0xD4, 0xFE, 0x70, 0xFE, // frame 1: L=-300, R=-400
        ];
        let m = oracle_stereo_to_mono_left(&bytes);
        assert_eq!(m, vec![100, -300]);
    }

    #[test]
    fn tile_cycle_works_for_partial_tail() {
        let c = vec![1i16, 2, 3];
        let t = tile_cycle_to_length(&c, 7);
        assert_eq!(t, vec![1, 2, 3, 1, 2, 3, 1]);
    }

    #[test]
    fn align_zero_offset_for_identical_buffers() {
        let raw: Vec<i16> = (0..128).map(|i| (i * 100) as i16).collect();
        let a = align_oracle_to_raw(&raw, &raw, 16);
        assert_eq!(a.oracle_offset, 0);
        assert_eq!(a.aligned_rms_error, 0.0);
    }

    #[test]
    fn align_finds_three_sample_delay() {
        let raw: Vec<i16> = (0..128).map(|i| (i * 100) as i16).collect();
        // Shift "oracle" forward by 3 samples (prepend 3 zeros).
        let mut oracle = vec![0i16; 3];
        oracle.extend_from_slice(&raw);
        let a = align_oracle_to_raw(&oracle, &raw, 16);
        assert_eq!(a.oracle_offset, 3);
        assert_eq!(a.aligned_rms_error, 0.0);
    }

    #[test]
    fn decision_rule_defer_when_monotonicity_fails() {
        let mk = |name: &str, gain: f64| Measurement {
            name: name.to_string(),
            frequency_hz: 0.0,
            raw_decoded_pcm_sha256: String::new(),
            oracle_pcm_sha256: String::new(),
            raw_rms: 0.0,
            oracle_rms: 0.0,
            gain_delta_db: gain,
            peak_abs_error_oracle_vs_raw: 0,
            peak_abs_raw_vs_source: 0,
            zcr_raw: 0.0,
            zcr_oracle: 0.0,
            clipping_count_raw: 0,
            clipping_count_oracle: 0,
            phase_or_delay_note: None,
        };
        // Non-monotonic: -1, -3, -2 (bumps up at h8), -4.
        let ms = vec![
            mk("harmonic_2_cycle_64", -1.0),
            mk("harmonic_4_cycle_64", -3.0),
            mk("harmonic_8_cycle_64", -2.0),
            mk("harmonic_16_cycle_64", -4.0),
        ];
        let o = apply_m3_5_decision_rule(&ms);
        assert_eq!(o.recommended_next, "defer");
        assert!(o.reasons.iter().any(|r| r.contains("monotonicity FAILS")));
    }

    #[test]
    fn decision_rule_pending_preset_eval_when_monotonic_and_h16_responds() {
        let mk = |name: &str, gain: f64| Measurement {
            name: name.to_string(),
            frequency_hz: 0.0,
            raw_decoded_pcm_sha256: String::new(),
            oracle_pcm_sha256: String::new(),
            raw_rms: 0.0,
            oracle_rms: 0.0,
            gain_delta_db: gain,
            peak_abs_error_oracle_vs_raw: 0,
            peak_abs_raw_vs_source: 0,
            zcr_raw: 0.0,
            zcr_oracle: 0.0,
            clipping_count_raw: 0,
            clipping_count_oracle: 0,
            phase_or_delay_note: None,
        };
        let ms = vec![
            mk("harmonic_2_cycle_64", -0.5),
            mk("harmonic_4_cycle_64", -2.0),
            mk("harmonic_8_cycle_64", -4.0),
            mk("harmonic_16_cycle_64", -8.0),
        ];
        let o = apply_m3_5_decision_rule(&ms);
        assert_eq!(o.recommended_next, "pending_preset_eval");
        assert!(o.clear_target_for_pre_emphasis);
    }

    #[test]
    fn decision_rule_defer_when_h16_does_not_respond() {
        let mk = |name: &str, gain: f64| Measurement {
            name: name.to_string(),
            frequency_hz: 0.0,
            raw_decoded_pcm_sha256: String::new(),
            oracle_pcm_sha256: String::new(),
            raw_rms: 0.0,
            oracle_rms: 0.0,
            gain_delta_db: gain,
            peak_abs_error_oracle_vs_raw: 0,
            peak_abs_raw_vs_source: 0,
            zcr_raw: 0.0,
            zcr_oracle: 0.0,
            clipping_count_raw: 0,
            clipping_count_oracle: 0,
            phase_or_delay_note: None,
        };
        // Monotonic but h16 barely attenuates (-0.2 dB) — under the
        // -0.5 dB threshold.
        let ms = vec![
            mk("harmonic_2_cycle_64", 0.0),
            mk("harmonic_4_cycle_64", -0.05),
            mk("harmonic_8_cycle_64", -0.10),
            mk("harmonic_16_cycle_64", -0.20),
        ];
        let o = apply_m3_5_decision_rule(&ms);
        assert_eq!(o.recommended_next, "defer");
        assert!(o.reasons.iter().any(|r| r.contains("insufficient")));
    }
}
