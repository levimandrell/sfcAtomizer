//! M3.0 — loop-click metric formula tests independent of encoder.
//!
//! These tests pin the metric formula on hand-constructed PCM
//! vectors so the metric implementation cannot be retroactively
//! tweaked to make encoder changes look better. The metric is
//! SPEC §10.6; M3 sub-passes (M3.1+) gate on `loop_click_abs`
//! against atom BRR encoder output, but the formula itself must
//! be testable without rendering atoms or encoding BRR — this
//! prevents circular validation per the M3 PM brief.

use sfc_atomizer_core::audition::{loop_click_abs, loop_window_rms_delta};

#[test]
fn loop_click_abs_simple_seam() {
    let decoded: Vec<i16> = vec![0, 100, 0, -100];
    // loop_start=0, loop_end=4 → seam: decoded[0] - decoded[3] = 0 - (-100) = 100.
    assert_eq!(loop_click_abs(&decoded, 0, decoded.len()), 100);
}

#[test]
fn loop_click_abs_perfect_seam() {
    let decoded: Vec<i16> = vec![0, 100, 0, 0];
    // seam: decoded[0] - decoded[3] = 0 - 0 = 0.
    assert_eq!(loop_click_abs(&decoded, 0, decoded.len()), 0);
}

#[test]
fn loop_click_abs_negative_to_positive_seam() {
    let decoded: Vec<i16> = vec![1000, 500, 0, -500, -1000];
    // seam: 1000 - (-1000) = 2000. Exercises the i32-widening
    // path: a raw i16 subtraction would still fit here, but the
    // formula is locked at i32 to support full i16-range inputs.
    assert_eq!(loop_click_abs(&decoded, 0, decoded.len()), 2000);
}

#[test]
fn loop_window_rms_delta_perfect_seam() {
    let decoded: Vec<i16> = vec![0; 16];
    let rms = loop_window_rms_delta(&decoded, 0, decoded.len(), 8);
    assert!(
        rms < 1.0,
        "all-zero buffer is a perfect seam; expected ~0 windowed delta, got {rms}"
    );
}

#[test]
fn loop_window_rms_delta_sample_count_matches_window() {
    // Exercises the windowing math: 16-sample buffer, window=8.
    // Pre window  = decoded[8..16] = [0, 100, 200, 300, 400, 500, 600, 700].
    // Post window = decoded[0..8]  = [-800, -700, -600, -500, -400, -300, -200, -100].
    // Per-sample deltas = pre - post = [800, 800, 800, 800, 800, 800, 800, 800].
    // sum_sq = 8 * 800^2 = 5_120_000; sqrt ≈ 2262.74.
    let decoded: Vec<i16> = (0..16).map(|i| (i * 100 - 800) as i16).collect();
    let rms = loop_window_rms_delta(&decoded, 0, decoded.len(), 8);
    assert!(rms.is_finite());
    assert!(
        (rms - (5_120_000f64).sqrt()).abs() < 1e-6,
        "windowed RMS expected sqrt(5_120_000) ≈ 2262.74, got {rms}"
    );
}
