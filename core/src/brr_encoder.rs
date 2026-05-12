//! M1 BRR encoder.
//!
//! Per SPEC §10.2 search strategy: exhaustive per-block
//! `(filter, shift)` search over filters `0..=3` × shifts `0..=12`.
//! Greedy across blocks (no Viterbi), no phase rotation, no spectral
//! scoring, no pre-emphasis. Those land in M3+.
//!
//! Shifts `13..=15` aren't in the search range. The decoder's
//! special-case path (`shifted = nibble & !0x07FF`) only encodes
//! `0` or `-2048`, which is degraded enough that an M1 encoder
//! never wants it.
//!
//! Round-trip correctness is the load-bearing gate. The encoder
//! drives the existing M0.2 decoder ([`crate::brr::decode_block`])
//! to produce the canonical decoded output for each candidate so
//! the encoded bytes are guaranteed to decode to the same samples
//! the encoder scored against.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::brr::{decode_block, BrrDecoderState};

/// Caller-supplied options for [`encode`] and [`encode_looped`].
#[derive(Debug, Clone, Copy)]
pub struct EncodeOptions {
    /// If `true`, the first block in the encoded sequence is forced
    /// to filter 0 (no predictor history). The S-DSP starts with
    /// `prev1 = prev2 = 0`, so any other filter on block 0 reads
    /// uninitialised history and produces glitches.
    pub force_filter_0_first_block: bool,
    /// If `Some(i)`, block `i` is forced to filter 0. For looped
    /// samples this is `start_sample / 16` — the block the S-DSP
    /// jumps to on iteration 2+, where the predictor history at
    /// loop entry has no fixed value across iterations.
    pub loop_entry_block_index: Option<u32>,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            force_filter_0_first_block: true,
            loop_entry_block_index: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncodeResult {
    /// Encoded BRR bytes. Length is always `blocks * 9`.
    pub bytes: Vec<u8>,
    /// Per-block scoring data; one entry per encoded block.
    pub blocks: Vec<EncodedBlockReport>,
    /// Aggregate stats across the whole encode.
    pub summary: EncodeSummary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct EncodedBlockReport {
    pub index: u32,
    pub filter: u8,
    pub shift: u8,
    pub end_flag: bool,
    pub loop_flag: bool,
    pub block_rms_error: f64,
    pub block_peak_error: u32,
    pub block_clamp_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct EncodeSummary {
    pub total_blocks: u32,
    pub encoded_bytes: u32,
    pub overall_rms_error: f64,
    pub overall_peak_error: u32,
    pub total_clamp_count: u32,
    pub filter_distribution: [u32; 4],
    /// Per-sample squared discontinuity at the loop seam, RMS-style.
    /// `None` for non-looped encodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_click_score: Option<f64>,
}

#[derive(Debug, Error)]
pub enum EncodeError {
    #[error("loop_start_sample {0} must be a multiple of 16")]
    LoopStartNotAligned(u32),
    #[error("loop_start_sample {start} must be < samples.len() {len}")]
    LoopStartOutOfRange { start: u32, len: u32 },
}

/// Encode `samples` to BRR. Caller doesn't have to align to a
/// multiple of 16 — trailing samples are zero-padded to fill the
/// last block and the per-block stats reflect the padded values.
///
/// Sets `end_flag` on the last block, `loop_flag = false`.
pub fn encode(samples: &[i16], options: &EncodeOptions) -> EncodeResult {
    encode_internal(
        samples,
        /* loop_start = */ None,
        options,
        ShiftObjective::PeakThenSumSq,
    )
    .expect("non-looped encode never fails")
}

/// Encode `samples` to BRR with the loop point at
/// `loop_start_sample`. Sets the last block's `end_flag` and
/// `loop_flag` so the S-DSP jumps to the loop entry block on KOFF.
///
/// `loop_start_sample` must be a multiple of 16 (`< samples.len()`).
pub fn encode_looped(
    samples: &[i16],
    loop_start_sample: u32,
    options: &EncodeOptions,
) -> Result<EncodeResult, EncodeError> {
    if !loop_start_sample.is_multiple_of(16) {
        return Err(EncodeError::LoopStartNotAligned(loop_start_sample));
    }
    if (loop_start_sample as usize) >= samples.len() {
        return Err(EncodeError::LoopStartOutOfRange {
            start: loop_start_sample,
            len: samples.len() as u32,
        });
    }
    let mut opts = *options;
    opts.loop_entry_block_index = Some(loop_start_sample / 16);
    encode_internal(
        samples,
        Some(loop_start_sample),
        &opts,
        ShiftObjective::PeakThenSumSq,
    )
}

fn encode_internal(
    samples: &[i16],
    loop_start: Option<u32>,
    options: &EncodeOptions,
    objective: ShiftObjective,
) -> Result<EncodeResult, EncodeError> {
    if samples.is_empty() {
        return Ok(EncodeResult {
            bytes: Vec::new(),
            blocks: Vec::new(),
            summary: EncodeSummary {
                total_blocks: 0,
                encoded_bytes: 0,
                overall_rms_error: 0.0,
                overall_peak_error: 0,
                total_clamp_count: 0,
                filter_distribution: [0; 4],
                loop_click_score: None,
            },
        });
    }
    let total_blocks = samples.len().div_ceil(16) as u32;
    let mut bytes = Vec::with_capacity(total_blocks as usize * 9);
    let mut block_reports = Vec::with_capacity(total_blocks as usize);
    let mut state = BrrDecoderState::default();

    let mut overall_sum_sq: u128 = 0;
    let mut overall_peak: u32 = 0;
    let mut overall_clamps: u32 = 0;
    let mut filter_dist = [0u32; 4];

    let mut loop_entry_first_decoded: Option<i16> = None;
    let mut last_decoded: Option<i16> = None;

    for block_idx in 0..total_blocks {
        let mut source_block = [0i16; 16];
        let start = block_idx as usize * 16;
        let end = (start + 16).min(samples.len());
        source_block[..end - start].copy_from_slice(&samples[start..end]);

        let is_first_block = block_idx == 0;
        let is_loop_entry = options.loop_entry_block_index == Some(block_idx);
        let force_filter0 = (is_first_block && options.force_filter_0_first_block) || is_loop_entry;

        let is_last_block = block_idx == total_blocks - 1;
        let end_flag = is_last_block;
        let loop_flag = is_last_block && loop_start.is_some();

        let trial = best_filter_shift(
            &source_block,
            state,
            force_filter0,
            end_flag,
            loop_flag,
            objective,
        );

        bytes.extend_from_slice(&trial.block);
        block_reports.push(EncodedBlockReport {
            index: block_idx,
            filter: trial.filter,
            shift: trial.shift,
            end_flag,
            loop_flag,
            block_rms_error: trial.rms,
            block_peak_error: trial.peak,
            block_clamp_count: trial.clamps,
        });
        filter_dist[trial.filter as usize] += 1;
        overall_sum_sq += trial.sum_sq;
        if trial.peak > overall_peak {
            overall_peak = trial.peak;
        }
        overall_clamps += trial.clamps;

        if is_loop_entry {
            loop_entry_first_decoded = Some(trial.first_decoded);
        }
        last_decoded = Some(trial.last_decoded);
        state = trial.new_state;
    }

    let total_samples = (total_blocks as u128) * 16;
    let overall_rms = if total_samples > 0 {
        ((overall_sum_sq as f64) / (total_samples as f64)).sqrt()
    } else {
        0.0
    };

    let loop_click_score = match (loop_start, loop_entry_first_decoded, last_decoded) {
        (Some(_), Some(first_at_entry), Some(last)) => {
            let diff = first_at_entry as i32 - last as i32;
            Some(diff.abs() as f64)
        }
        _ => None,
    };

    let summary = EncodeSummary {
        total_blocks,
        encoded_bytes: total_blocks * 9,
        overall_rms_error: overall_rms,
        overall_peak_error: overall_peak,
        total_clamp_count: overall_clamps,
        filter_distribution: filter_dist,
        loop_click_score,
    };
    Ok(EncodeResult {
        bytes,
        blocks: block_reports,
        summary,
    })
}

struct BlockTrial {
    block: [u8; 9],
    new_state: BrrDecoderState,
    filter: u8,
    shift: u8,
    rms: f64,
    sum_sq: u128,
    peak: u32,
    clamps: u32,
    first_decoded: i16,
    last_decoded: i16,
}

/// Shift-selection objective. Decides how `best_filter_shift`
/// breaks the tie between the 4 filters × 13 shifts grid of
/// per-block trials.
///
/// Production (M3.3) uses [`ShiftObjective::PeakThenSumSq`]. The
/// alternative [`ShiftObjective::RmsThenPeak`] is the M5.4
/// spike's hypothesis per consultant M4.4 audit #7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftObjective {
    /// Score by peak first, sum-of-squares as tiebreak. Pure RMS
    /// scoring lets the encoder pick a smaller shift that cleanly
    /// encodes the bulk of the block but clips at the signal's
    /// peaks; clipping is what makes BRR samples sound distorted,
    /// so avoid it even at the cost of slightly higher quantization
    /// noise on the inner samples. M3.3 production objective.
    PeakThenSumSq,
    /// M5.4 spike — score by sum-of-squares first, peak as
    /// tiebreak. Minimizes block RMS residual at the potential
    /// cost of clipping at signal peaks. Per consultant M4.4 audit
    /// #7: predicted to NOT clear the M4.3 high-noise-cluster
    /// ceiling. Feature-flagged in the spike entry; production code
    /// MUST pass `PeakThenSumSq`.
    RmsThenPeak,
}

fn best_filter_shift(
    source: &[i16; 16],
    state: BrrDecoderState,
    force_filter0: bool,
    end_flag: bool,
    loop_flag: bool,
    objective: ShiftObjective,
) -> BlockTrial {
    let filter_range: &[u8] = if force_filter0 { &[0] } else { &[0, 1, 2, 3] };
    let mut best: Option<BlockTrial> = None;
    for &filter in filter_range {
        for shift in 0u8..=12 {
            let trial = encode_one_filter_shift(source, state, filter, shift, end_flag, loop_flag);
            let is_better = match best {
                None => true,
                Some(ref b) => match objective {
                    ShiftObjective::PeakThenSumSq => {
                        (trial.peak, trial.sum_sq) < (b.peak, b.sum_sq)
                    }
                    ShiftObjective::RmsThenPeak => (trial.sum_sq, trial.peak) < (b.sum_sq, b.peak),
                },
            };
            if is_better {
                best = Some(trial);
            }
        }
    }
    best.expect("filter_range is non-empty")
}

fn encode_one_filter_shift(
    source: &[i16; 16],
    in_state: BrrDecoderState,
    filter: u8,
    shift: u8,
    end_flag: bool,
    loop_flag: bool,
) -> BlockTrial {
    let mut nibbles = [0i8; 16];
    let mut p1 = in_state.prev1 as i32;
    let mut p2 = in_state.prev2 as i32;
    let mut clamps = 0u32;

    for i in 0..16 {
        let pred = predict(filter, p1, p2);
        let target_shifted = source[i] as i32 - pred;
        let n = best_nibble_for(target_shifted, shift);
        nibbles[i] = n;

        let n_i32 = n as i32;
        let shifted = (n_i32 << shift) >> 1;
        let mut s = shifted + pred;
        // Filters 2 and 3 clamp to i16 before the 15-bit wrap.
        if filter >= 2 {
            let clamped = s.clamp(i16::MIN as i32, i16::MAX as i32);
            if clamped != s {
                clamps += 1;
            }
            s = clamped;
        }
        let wrapped = (s.wrapping_shl(1) as i16) >> 1;
        p2 = p1;
        p1 = wrapped as i32;
    }

    let header = ((shift & 0x0F) << 4)
        | ((filter & 0x03) << 2)
        | (if loop_flag { 0b10 } else { 0 })
        | (if end_flag { 0b01 } else { 0 });
    let mut block = [0u8; 9];
    block[0] = header;
    for j in 0..8 {
        let hi = (nibbles[j * 2] as u8) & 0x0F;
        let lo = (nibbles[j * 2 + 1] as u8) & 0x0F;
        block[1 + j] = (hi << 4) | lo;
    }

    // Re-decode via the canonical M0.2 decoder. The encoder's
    // analytic loop above mirrors this exactly, so the outputs
    // must match — round-tripping ensures the BRR file decodes
    // to the same samples we scored against.
    let mut state_for_decode = in_state;
    let decoded = decode_block(&block, &mut state_for_decode);

    let mut sum_sq: u128 = 0;
    let mut peak: u32 = 0;
    for i in 0..16 {
        let diff = (decoded[i] as i32 - source[i] as i32).unsigned_abs();
        sum_sq += (diff as u128) * (diff as u128);
        if diff > peak {
            peak = diff;
        }
    }
    let rms = (sum_sq as f64 / 16.0).sqrt();

    BlockTrial {
        block,
        new_state: state_for_decode,
        filter,
        shift,
        rms,
        sum_sq,
        peak,
        clamps,
        first_decoded: decoded[0],
        last_decoded: decoded[15],
    }
}

/// Filter prediction term, matching M0.2 [`crate::brr::decode_block`]
/// integer math sample-by-sample (without the shifted-nibble term —
/// that's added by the caller).
fn predict(filter: u8, p1: i32, p2: i32) -> i32 {
    match filter {
        0 => 0,
        1 => p1 + ((-p1) >> 4),
        2 => (p1 << 1) + ((-(p1 + (p1 << 1))) >> 5) + (-p2) + (p2 >> 4),
        3 => (p1 << 1) + ((-(p1 + (p1 << 2) + (p1 << 3))) >> 6) + (-p2) + ((p2 + (p2 << 1)) >> 4),
        _ => unreachable!("filter {filter} out of range"),
    }
}

/// Pick the nibble in `-8..=7` that decodes closest to
/// `target_shifted` for the given `shift`. Iterates all 16
/// candidates — fast enough at the encoder's call rate, and avoids
/// rounding subtleties for shift = 0 where `(n << 0) >> 1 = n >> 1`
/// maps two `n` values to the same output.
fn best_nibble_for(target_shifted: i32, shift: u8) -> i8 {
    let mut best_n = 0i8;
    let mut best_err = i32::MAX;
    for n in -8i8..=7 {
        let n_i32 = n as i32;
        let decoded = (n_i32 << shift) >> 1;
        let err = (decoded - target_shifted).abs();
        if err < best_err {
            best_err = err;
            best_n = n;
        }
    }
    best_n
}

// =====================================================================
// M5.4 — Alternative shift-selection objective spike (consultant
// M4.4 audit #7 / M5.4 brief Phase B)
// =====================================================================
//
// Feature-flagged greedy encoder variant. Same per-block algorithm as
// production `encode_looped` but uses `ShiftObjective::RmsThenPeak`
// for the (filter, shift) selection lex-tiebreak instead of
// `PeakThenSumSq`. Tests this hypothesis: would minimizing block RMS
// (at potential cost of peak clipping) improve the M4.3 noise-floor
// metrics on the high-noise cluster?
//
// Consultant M4.4 audit #7 prediction: no. Spike is documentary;
// production code MUST NOT call this entry.

/// M5.4 spike entry. Same signature as `encode_looped` plus an
/// explicit [`ShiftObjective`]. Feature-flagged per the M5.4 brief.
/// Production code path (`encode_looped`) always passes
/// `PeakThenSumSq` and ignores this entry.
pub fn encode_looped_m5_4_alt_shift_spike(
    samples: &[i16],
    loop_start_sample: u32,
    options: &EncodeOptions,
    objective: ShiftObjective,
) -> Result<EncodeResult, EncodeError> {
    if !loop_start_sample.is_multiple_of(16) {
        return Err(EncodeError::LoopStartNotAligned(loop_start_sample));
    }
    if (loop_start_sample as usize) >= samples.len() {
        return Err(EncodeError::LoopStartOutOfRange {
            start: loop_start_sample,
            len: samples.len() as u32,
        });
    }
    let mut opts = *options;
    opts.loop_entry_block_index = Some(loop_start_sample / 16);
    encode_internal(samples, Some(loop_start_sample), &opts, objective)
}

// =====================================================================
// M4.4 — Encoder improvement spike (research-spike per SPEC §24.1)
// =====================================================================
//
// Feature-flagged beam-search encoder. Not wired into production
// `render_to_brr` at the spike's measurement phase. Tests invoke it
// directly to evaluate against the SPEC §24.1 exit criterion.

/// M4.4 spike configuration. Strategy not locked at M4.0 contract;
/// engineer's choice per consultant M4 plan #10.
#[derive(Debug, Clone, Copy)]
pub struct M44SpikeConfig {
    pub strategy: M44Strategy,
}

/// M4.4 spike strategies. M4.4 adds the beam-search variant; future
/// passes may add more.
#[derive(Debug, Clone, Copy)]
pub enum M44Strategy {
    /// Cross-block beam search (the M3.4-deferred predictor
    /// optimization per consultant M3.3 audit #21). `beam_width`
    /// candidates carried block-to-block; pruning by
    /// `(cumulative_sum_sq, cumulative_peak)` lex order. Score
    /// targets RMS primary, peak secondary — matches the SPEC §24.1
    /// exit criterion (`≥10% rms_raw_vs_source` OR
    /// `peak_abs_raw_vs_source`).
    BeamSearch { beam_width: u32 },
}

/// M4.4 spike entry. Mirrors `encode_looped` interface; same
/// `loop_start_sample` validation; emits the same `EncodeResult`
/// shape so downstream code (decoder, noise-floor metric helpers)
/// is unchanged.
pub fn encode_looped_m4_4_spike(
    samples: &[i16],
    loop_start_sample: u32,
    options: &EncodeOptions,
    spike_config: &M44SpikeConfig,
) -> Result<EncodeResult, EncodeError> {
    if !loop_start_sample.is_multiple_of(16) {
        return Err(EncodeError::LoopStartNotAligned(loop_start_sample));
    }
    if (loop_start_sample as usize) >= samples.len() {
        return Err(EncodeError::LoopStartOutOfRange {
            start: loop_start_sample,
            len: samples.len() as u32,
        });
    }
    let mut opts = *options;
    opts.loop_entry_block_index = Some(loop_start_sample / 16);

    match spike_config.strategy {
        M44Strategy::BeamSearch { beam_width } => {
            beam_search_encode(samples, Some(loop_start_sample), &opts, beam_width.max(1))
        }
    }
}

/// One beam-search candidate. Each carries the bytes + per-block
/// reports + decoder state + accumulated scoring info needed to
/// extend it through the next block.
#[derive(Clone)]
struct BeamCandidate {
    bytes: Vec<u8>,
    blocks: Vec<EncodedBlockReport>,
    state: BrrDecoderState,
    sum_sq: u128,
    peak: u32,
    clamps: u32,
    filter_dist: [u32; 4],
    /// Decoded sample at the start of the loop-entry block (the
    /// sample the S-DSP returns to on KOFF). Tracked across the
    /// whole encode; set when the loop-entry block is processed.
    loop_entry_first_decoded: Option<i16>,
    /// Last decoded sample in the most recently encoded block (the
    /// sample whose continuity with `loop_entry_first_decoded`
    /// determines the loop-click).
    last_decoded: Option<i16>,
}

fn beam_search_encode(
    samples: &[i16],
    loop_start: Option<u32>,
    options: &EncodeOptions,
    beam_width: u32,
) -> Result<EncodeResult, EncodeError> {
    if samples.is_empty() {
        return Ok(EncodeResult {
            bytes: Vec::new(),
            blocks: Vec::new(),
            summary: EncodeSummary {
                total_blocks: 0,
                encoded_bytes: 0,
                overall_rms_error: 0.0,
                overall_peak_error: 0,
                total_clamp_count: 0,
                filter_distribution: [0; 4],
                loop_click_score: None,
            },
        });
    }

    let total_blocks = samples.len().div_ceil(16) as u32;
    let beam_width = beam_width as usize;

    // Initial beam: one empty candidate.
    let mut beam: Vec<BeamCandidate> = vec![BeamCandidate {
        bytes: Vec::with_capacity(total_blocks as usize * 9),
        blocks: Vec::with_capacity(total_blocks as usize),
        state: BrrDecoderState::default(),
        sum_sq: 0,
        peak: 0,
        clamps: 0,
        filter_dist: [0; 4],
        loop_entry_first_decoded: None,
        last_decoded: None,
    }];

    for block_idx in 0..total_blocks {
        let mut source_block = [0i16; 16];
        let start = block_idx as usize * 16;
        let end = (start + 16).min(samples.len());
        source_block[..end - start].copy_from_slice(&samples[start..end]);

        let is_first_block = block_idx == 0;
        let is_loop_entry = options.loop_entry_block_index == Some(block_idx);
        let force_filter0 = (is_first_block && options.force_filter_0_first_block) || is_loop_entry;

        let is_last_block = block_idx == total_blocks - 1;
        let end_flag = is_last_block;
        let loop_flag = is_last_block && loop_start.is_some();

        // Expand each beam candidate to every (filter, shift) pair.
        let filter_range: &[u8] = if force_filter0 { &[0] } else { &[0, 1, 2, 3] };
        let mut next_beam: Vec<BeamCandidate> =
            Vec::with_capacity(beam.len() * filter_range.len() * 13);
        for parent in &beam {
            for &filter in filter_range {
                for shift in 0u8..=12 {
                    let trial = encode_one_filter_shift(
                        &source_block,
                        parent.state,
                        filter,
                        shift,
                        end_flag,
                        loop_flag,
                    );
                    let mut child = parent.clone();
                    child.bytes.extend_from_slice(&trial.block);
                    child.blocks.push(EncodedBlockReport {
                        index: block_idx,
                        filter: trial.filter,
                        shift: trial.shift,
                        end_flag,
                        loop_flag,
                        block_rms_error: trial.rms,
                        block_peak_error: trial.peak,
                        block_clamp_count: trial.clamps,
                    });
                    child.state = trial.new_state;
                    child.sum_sq += trial.sum_sq;
                    if trial.peak > child.peak {
                        child.peak = trial.peak;
                    }
                    child.clamps += trial.clamps;
                    child.filter_dist[trial.filter as usize] += 1;
                    if is_loop_entry {
                        child.loop_entry_first_decoded = Some(trial.first_decoded);
                    }
                    child.last_decoded = Some(trial.last_decoded);
                    next_beam.push(child);
                }
            }
        }

        // Prune: keep the `beam_width` lowest-`sum_sq` candidates;
        // tie-break by lowest peak; final tie-break by the
        // accumulated bytes (deterministic across runs).
        next_beam.sort_by(|a, b| {
            a.sum_sq
                .cmp(&b.sum_sq)
                .then(a.peak.cmp(&b.peak))
                .then(a.bytes.cmp(&b.bytes))
        });
        next_beam.truncate(beam_width);
        beam = next_beam;
    }

    // Pick the best survivor.
    let best = beam
        .into_iter()
        .min_by(|a, b| {
            a.sum_sq
                .cmp(&b.sum_sq)
                .then(a.peak.cmp(&b.peak))
                .then(a.bytes.cmp(&b.bytes))
        })
        .expect("beam never empty");

    let total_samples = (total_blocks as u128) * 16;
    let overall_rms = if total_samples > 0 {
        ((best.sum_sq as f64) / (total_samples as f64)).sqrt()
    } else {
        0.0
    };
    let loop_click_score = match (loop_start, best.loop_entry_first_decoded, best.last_decoded) {
        (Some(_), Some(first), Some(last)) => {
            let diff = first as i32 - last as i32;
            Some(diff.abs() as f64)
        }
        _ => None,
    };
    let summary = EncodeSummary {
        total_blocks,
        encoded_bytes: total_blocks * 9,
        overall_rms_error: overall_rms,
        overall_peak_error: best.peak,
        total_clamp_count: best.clamps,
        filter_distribution: best.filter_dist,
        loop_click_score,
    };
    Ok(EncodeResult {
        bytes: best.bytes,
        blocks: best.blocks,
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brr::{decode_blocks, BrrDecoderState};

    #[test]
    fn encode_zero_samples_returns_empty() {
        let r = encode(&[], &EncodeOptions::default());
        assert_eq!(r.bytes.len(), 0);
        assert_eq!(r.summary.total_blocks, 0);
        assert_eq!(r.blocks.len(), 0);
    }

    #[test]
    fn encode_silence_picks_filter_0_shift_0() {
        let samples = vec![0i16; 32];
        let r = encode(&samples, &EncodeOptions::default());
        assert_eq!(r.bytes.len(), 18);
        for blk in &r.blocks {
            assert_eq!(blk.filter, 0, "silence must pick filter 0");
            assert_eq!(blk.shift, 0, "silence must pick shift 0");
        }
    }

    #[test]
    fn encode_decode_roundtrip_peak_below_threshold_on_sine() {
        let mut samples = Vec::with_capacity(256);
        let amp = 8000.0f64;
        for i in 0..256 {
            let phase = (i as f64) * std::f64::consts::TAU / 64.0;
            samples.push((phase.sin() * amp).round() as i16);
        }
        // Disable force_filter_0_first_block: the S-DSP boots with
        // prev1=prev2=0 anyway, so filters 1..=3 are safe on block 0
        // and the encoder needs that freedom to track the sine peak
        // without clipping. The default-on safety guard is still
        // exercised by force_filter_0_first_block_when_set below.
        let opts = EncodeOptions {
            force_filter_0_first_block: false,
            loop_entry_block_index: None,
        };
        let r = encode(&samples, &opts);
        // Decode and compare against source; bound matches the brief.
        let blocks: Vec<[u8; 9]> = r
            .bytes
            .chunks_exact(9)
            .map(|c| {
                let mut b = [0u8; 9];
                b.copy_from_slice(c);
                b
            })
            .collect();
        let mut state = BrrDecoderState::default();
        let decoded = decode_blocks(&blocks, &mut state);
        assert_eq!(decoded.len(), samples.len());
        let peak = decoded
            .iter()
            .zip(samples.iter())
            .map(|(d, s)| (*d as i32 - *s as i32).unsigned_abs())
            .max()
            .unwrap();
        assert!(peak < 256, "round-trip peak error {peak} >= 256 LSBs");
        assert_eq!(r.summary.overall_peak_error, peak);
    }

    #[test]
    fn force_filter_0_first_block_when_set() {
        // Non-trivial samples; without forcing, block 0 might pick
        // a higher filter on real data, but with prev1=prev2=0 those
        // would just zero out the prediction term — still safe to
        // verify the constraint holds.
        let mut samples = vec![0i16; 32];
        for (i, s) in samples.iter_mut().enumerate() {
            *s = ((i as i32) * 100) as i16;
        }
        let r = encode(
            &samples,
            &EncodeOptions {
                force_filter_0_first_block: true,
                loop_entry_block_index: None,
            },
        );
        assert_eq!(r.blocks[0].filter, 0);
    }

    #[test]
    fn loop_entry_block_uses_filter_0_when_provided() {
        let samples = vec![1000i16; 64];
        let opts = EncodeOptions {
            force_filter_0_first_block: true,
            loop_entry_block_index: Some(2),
        };
        let r = encode(&samples, &opts);
        assert_eq!(r.blocks[2].filter, 0, "loop entry block must be filter 0");
    }

    #[test]
    fn encode_looped_sets_end_and_loop_flags_on_last_block() {
        let samples = vec![0i16; 48];
        let r = encode_looped(&samples, 16, &EncodeOptions::default()).unwrap();
        let last = r.blocks.last().unwrap();
        assert!(last.end_flag);
        assert!(last.loop_flag);
        // Earlier blocks have neither flag.
        for blk in &r.blocks[..r.blocks.len() - 1] {
            assert!(!blk.end_flag);
            assert!(!blk.loop_flag);
        }
    }

    #[test]
    fn encode_non_looped_sets_only_end_flag_on_last_block() {
        let samples = vec![0i16; 32];
        let r = encode(&samples, &EncodeOptions::default());
        let last = r.blocks.last().unwrap();
        assert!(last.end_flag);
        assert!(!last.loop_flag);
    }

    #[test]
    fn encode_looped_rejects_unaligned_loop_start() {
        let samples = vec![0i16; 64];
        let err = encode_looped(&samples, 17, &EncodeOptions::default()).unwrap_err();
        assert!(matches!(err, EncodeError::LoopStartNotAligned(17)));
    }

    #[test]
    fn encoded_bytes_length_is_block_count_times_9() {
        for n_blocks in 1..=8 {
            let samples = vec![0i16; n_blocks * 16];
            let r = encode(&samples, &EncodeOptions::default());
            assert_eq!(r.bytes.len(), n_blocks * 9);
            assert_eq!(r.summary.encoded_bytes as usize, n_blocks * 9);
            assert_eq!(r.summary.total_blocks as usize, n_blocks);
        }
    }

    #[test]
    fn pad_to_multiple_of_16_zero_extends_input() {
        let samples = vec![0i16; 7]; // not a multiple of 16
        let r = encode(&samples, &EncodeOptions::default());
        assert_eq!(r.summary.total_blocks, 1);
        assert_eq!(r.bytes.len(), 9);
    }
}
