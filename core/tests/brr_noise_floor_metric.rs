//! M4.0 — BRR noise floor metric formula tests independent
//! of encoder. Per consultant M4 plan #4 (research-spike vs
//! contracted-implementation discipline), these tests pin the
//! metric formulas on hand-constructed PCM vectors so the
//! metric implementation cannot be retroactively tweaked to
//! make M4.4 encoder changes look better. Mirror of the M3.0
//! Phase H pattern for loop-click metrics (see
//! `core/tests/loop_click_metric.rs`).
//!
//! Source of truth: SPEC §10.10 BRR encoder noise floor
//! metrics.

use sfc_atomizer_core::audition::{
    clipping_count_raw, peak_abs_raw_vs_source, rms_raw_vs_source, snr_db,
};

// ---- peak_abs_raw_vs_source --------------------------------

#[test]
fn peak_abs_raw_vs_source_simple() {
    let source: Vec<i16> = vec![1000, 2000, 3000, 4000];
    let decoded: Vec<i16> = vec![900, 2100, 3050, 4500];
    // Per-sample |delta|: 100, 100, 50, 500 — max = 500.
    assert_eq!(peak_abs_raw_vs_source(&source, &decoded), 500);
}

#[test]
fn peak_abs_raw_vs_source_negative_deltas() {
    let source: Vec<i16> = vec![1000, 2000];
    let decoded: Vec<i16> = vec![-500, 3500];
    // Per-sample |delta|: 1500, 1500 — max = 1500. Verifies
    // widened i32 arithmetic handles sign-crossing deltas
    // without overflow.
    assert_eq!(peak_abs_raw_vs_source(&source, &decoded), 1500);
}

#[test]
fn peak_abs_raw_vs_source_zero_for_exact_match() {
    let source: Vec<i16> = vec![100, -100, 200, -200, 0];
    let decoded = source.clone();
    assert_eq!(peak_abs_raw_vs_source(&source, &decoded), 0);
}

#[test]
fn peak_abs_raw_vs_source_handles_i16_extremes() {
    // i16::MIN.abs() overflows in narrow i16 arithmetic; widened
    // i32 must yield 32768 cleanly. Source +32767 vs decoded
    // -32768 gives |32767 - (-32768)| = 65535.
    let source: Vec<i16> = vec![32767];
    let decoded: Vec<i16> = vec![i16::MIN];
    assert_eq!(peak_abs_raw_vs_source(&source, &decoded), 65535);
}

// ---- rms_raw_vs_source -------------------------------------

#[test]
fn rms_raw_vs_source_zero_error() {
    let source: Vec<i16> = vec![100, 200, 300, 400];
    let decoded = source.clone();
    let rms = rms_raw_vs_source(&source, &decoded);
    assert!(rms < 1e-9, "exact match should give ≈ 0 RMS, got {rms}");
}

#[test]
fn rms_raw_vs_source_constant_offset() {
    let source: Vec<i16> = vec![0, 0, 0, 0];
    let decoded: Vec<i16> = vec![10, 10, 10, 10];
    // sum_sq = 4 × 100 = 400; mean = 100; sqrt = 10.
    let rms = rms_raw_vs_source(&source, &decoded);
    assert!((rms - 10.0).abs() < 1e-9, "expected 10.0, got {rms}");
}

#[test]
fn rms_raw_vs_source_handles_empty_input() {
    assert_eq!(rms_raw_vs_source(&[], &[]), 0.0);
}

// ---- snr_db ------------------------------------------------

#[test]
fn snr_db_perfect_encode_returns_inf() {
    let source: Vec<i16> = vec![1000, 2000, -1000, -2000];
    let decoded = source.clone();
    let snr = snr_db(&source, &decoded);
    assert!(
        snr.is_infinite() && snr.is_sign_positive(),
        "expected +infinity for exact match, got {snr}"
    );
}

#[test]
fn snr_db_finite_for_typical_error() {
    // source = ±1000 square wave; source_rms = 1000.
    // decoded = ±900; err = ±100, err_rms = 100.
    // snr_db = 20 × log10(1000 / 100) = 20.0 dB.
    let source: Vec<i16> = vec![1000, -1000, 1000, -1000];
    let decoded: Vec<i16> = vec![900, -900, 900, -900];
    let snr = snr_db(&source, &decoded);
    assert!((snr - 20.0).abs() < 0.01, "expected ≈ 20 dB, got {snr}");
}

#[test]
fn snr_db_silent_source_returns_zero() {
    // source_rms = 0 → ratio undefined. Implementation returns
    // 0.0 (documented in the §10.10 contract).
    let source: Vec<i16> = vec![0, 0, 0, 0];
    let decoded: Vec<i16> = vec![10, -10, 10, -10];
    assert_eq!(snr_db(&source, &decoded), 0.0);
}

// ---- clipping_count_raw ------------------------------------

#[test]
fn clipping_count_raw_zero_for_unclipped() {
    let decoded: Vec<i16> = vec![1000, -1000, 30000, -30000];
    assert_eq!(clipping_count_raw(&decoded), 0);
}

#[test]
fn clipping_count_raw_counts_saturated_samples() {
    // SPEC §10.10: |x| >= 32767 in widened i32 arithmetic.
    // - 32767 counts (= i16::MAX, |x| = 32767).
    // - -32767 counts (|x| = 32767).
    // - 32766 does NOT count (|x| = 32766).
    // - 0 does NOT count.
    // Expected: 3 (two 32767 plus one -32767).
    let decoded: Vec<i16> = vec![32767, -32767, 32766, 0, 32767];
    assert_eq!(clipping_count_raw(&decoded), 3);
}

#[test]
fn clipping_count_raw_counts_i16_min_as_saturation() {
    // i16::MIN = -32768; widened |x| = 32768 ≥ 32767 → counts.
    // Verifies the widened-i32 abs trick handles the i16::MIN
    // edge that would overflow in narrow i16 arithmetic.
    let decoded: Vec<i16> = vec![i16::MIN, 0, i16::MIN];
    assert_eq!(clipping_count_raw(&decoded), 2);
}

#[test]
fn clipping_count_raw_handles_empty_input() {
    assert_eq!(clipping_count_raw(&[]), 0);
}
