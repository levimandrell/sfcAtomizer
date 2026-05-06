//! Loop-point candidate search for sustain-loop samples.
//!
//! Heuristic: BRR-block-aligned `(start, end)` pairs are scored by
//! windowed RMS difference of the W samples preceding each point —
//! similar local waveform shape implies the loop will wrap with low
//! discontinuity — plus a single-sample click magnitude at the seam.
//!
//! Search ranges are restricted to the first 25% (start) and last
//! 25% (end) of the sample, matching typical sustained-instrument
//! loop shapes (attack → loop region → release implicit). Iteration
//! is on multiples of 16 when `snap_to_brr_block`, since SPEC
//! `SAMPLE_LOOP` requires `start_sample % 16 == 0` for BRR alignment.

#[derive(Debug, Clone, Copy)]
pub struct LoopFinderOptions {
    /// Comparison window length, in samples. Larger windows reward
    /// global waveform similarity at the cost of false positives
    /// from noise. 32 is a reasonable default for ~32 kHz material.
    pub window_samples: usize,
    /// Number of best candidates to return.
    pub max_candidates: usize,
    /// If `true`, restrict `(start, end)` to multiples of 16 — required
    /// for BRR loop alignment.
    pub snap_to_brr_block: bool,
}

impl Default for LoopFinderOptions {
    fn default() -> Self {
        Self {
            window_samples: 32,
            max_candidates: 8,
            snap_to_brr_block: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LoopCandidate {
    pub start_sample: u32,
    pub end_sample: u32,
    /// RMS difference of the W samples preceding `start_sample` vs.
    /// the W samples preceding `end_sample`. Lower is better.
    pub rms_window_difference: f64,
    /// `|samples[start_sample] - samples[end_sample - 1]|` — the
    /// single-sample step the S-DSP takes when looping.
    pub seam_click: u32,
    /// Combined ranking score; lower is better. Sums RMS difference
    /// with a quarter-weight seam click so a high-RMS / zero-click
    /// pair never beats a low-RMS / small-click pair.
    pub score: f64,
}

pub fn find_loop_candidates(samples: &[i16], options: &LoopFinderOptions) -> Vec<LoopCandidate> {
    let len = samples.len();
    let w = options.window_samples;
    if w == 0 || len < w * 4 {
        return Vec::new();
    }

    let block = if options.snap_to_brr_block {
        16usize
    } else {
        1
    };
    let s_min = w.div_ceil(block) * block;
    let s_max = (len / 4 / block) * block;
    let e_min = ((3 * len / 4).div_ceil(block)) * block;
    let e_max = (len / block) * block;

    if s_min > s_max || e_min > e_max || e_min <= s_max {
        return Vec::new();
    }

    let mut candidates: Vec<LoopCandidate> = Vec::new();
    let mut s = s_min;
    while s <= s_max {
        let mut e = e_min;
        while e <= e_max {
            if e >= s + w && e <= len {
                let rms = window_rms_difference(samples, s, e, w);
                let seam_click = (samples[s] as i32 - samples[e - 1] as i32).unsigned_abs();
                let score = rms + 0.25 * (seam_click as f64);
                candidates.push(LoopCandidate {
                    start_sample: s as u32,
                    end_sample: e as u32,
                    rms_window_difference: rms,
                    seam_click,
                    score,
                });
            }
            e += block;
        }
        s += block;
    }

    candidates.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(options.max_candidates);
    candidates
}

fn window_rms_difference(samples: &[i16], s: usize, e: usize, w: usize) -> f64 {
    let mut sum_sq: f64 = 0.0;
    for i in 0..w {
        let a = samples[s - w + i] as f64;
        let b = samples[e - w + i] as f64;
        let d = a - b;
        sum_sq += d * d;
    }
    (sum_sq / w as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_sine(len: usize, period: f64, amp: f64) -> Vec<i16> {
        (0..len)
            .map(|i| {
                let phase = (i as f64) * std::f64::consts::TAU / period;
                (phase.sin() * amp).round() as i16
            })
            .collect()
    }

    #[test]
    fn returns_empty_when_too_short() {
        let s = synth_sine(50, 16.0, 1000.0);
        let r = find_loop_candidates(&s, &LoopFinderOptions::default());
        assert!(r.is_empty());
    }

    #[test]
    fn finds_low_score_loop_for_periodic_sine() {
        // 4-second equivalent at 64 samples/period → highly periodic.
        let s = synth_sine(2048, 64.0, 8000.0);
        let r = find_loop_candidates(&s, &LoopFinderOptions::default());
        assert!(!r.is_empty(), "should find at least one candidate");
        // Best candidate's RMS should be near zero — the sine
        // matches itself a period later within rounding noise.
        let best = r[0];
        assert!(
            best.rms_window_difference < 50.0,
            "expected near-zero RMS for periodic sine, got {}",
            best.rms_window_difference
        );
    }

    #[test]
    fn snaps_candidates_to_block_boundaries() {
        let s = synth_sine(4096, 64.0, 8000.0);
        let r = find_loop_candidates(&s, &LoopFinderOptions::default());
        for c in &r {
            assert_eq!(c.start_sample % 16, 0, "start must be 16-aligned");
            assert_eq!(c.end_sample % 16, 0, "end must be 16-aligned");
        }
    }

    #[test]
    fn start_in_first_quarter_end_in_last_quarter() {
        let len = 4096;
        let s = synth_sine(len, 64.0, 8000.0);
        let r = find_loop_candidates(&s, &LoopFinderOptions::default());
        for c in &r {
            assert!(
                (c.start_sample as usize) <= len / 4,
                "start {} not in first quarter",
                c.start_sample
            );
            assert!(
                (c.end_sample as usize) >= 3 * len / 4,
                "end {} not in last quarter",
                c.end_sample
            );
        }
    }

    #[test]
    fn returns_no_more_than_max_candidates() {
        let s = synth_sine(4096, 64.0, 8000.0);
        let opts = LoopFinderOptions {
            max_candidates: 3,
            ..LoopFinderOptions::default()
        };
        let r = find_loop_candidates(&s, &opts);
        assert!(r.len() <= 3);
    }

    #[test]
    fn candidates_are_sorted_ascending_by_score() {
        let s = synth_sine(4096, 64.0, 8000.0);
        let r = find_loop_candidates(&s, &LoopFinderOptions::default());
        for w in r.windows(2) {
            assert!(
                w[0].score <= w[1].score,
                "candidates not sorted: {} > {}",
                w[0].score,
                w[1].score
            );
        }
    }
}
