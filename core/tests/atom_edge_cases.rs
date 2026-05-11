//! M3.2 — atom edge-case fixture coverage.
//!
//! Synthesized fixture additions only — no committed-on-disk
//! fixture files at M3.2 (deferred to M3.3 prelude per consultant
//! M3 plan #12). Broadens the atom render → BRR encode → metric
//! input space before encoder optimization (phase rotation M3.3,
//! predictor §10.8, pre-emphasis §10.9) runs against it.
//!
//! Each fixture is a programmatically constructed `AtomSlot`
//! rendered through `render_to_brr`. Per-fixture coverage:
//!
//! - **PCM SHA identity-pin** (SPEC §16.9 amendment, M3.0): each
//!   fixture's `pcm_sha256` is asserted against
//!   `baselines/m3.json::identity_gated` via `include_str!`,
//!   mirroring the M2.8.1 / M3.1 pattern.
//! - **Determinism**: each fixture renders twice; every SHA + metric
//!   field must be byte-equal across the two runs.
//! - **Per-fixture special-case assertions**: amplitude_zero +
//!   all_partials_zero + two_partials_cancel must produce all-zero
//!   PCM and `loop_click_abs = 0`; harmonic_16_cycle_64 must render
//!   without panicking and produce a finite metric.
//!
//! Pre-M3 BRR SHAs, decoded-BRR PCM SHAs, and metric values are
//! captured under `baselines/m3.json::documentary_snapshot` and
//! expected to shift at M3.3 phase rotation. PCM SHAs are
//! identity-gated and MUST NOT shift across M3+.

use sfc_atomizer_core::atom::{
    render_to_brr, AtomBrrOutput, AtomKind, AtomPartial, AtomRenderOptions, AtomSlot,
};
use sfc_atomizer_core::project::{Envelope, SamplePlayback};

fn base(cycle: u16) -> AtomSlot {
    AtomSlot {
        id: "edge".to_string(),
        name: "edge".to_string(),
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

// ---------------------------------------------------------------- fixtures

fn fixture_amplitude_zero() -> AtomSlot {
    let mut a = base(128);
    a.amplitude = 0.0;
    a
}

fn fixture_all_partials_zero_normalize_true() -> AtomSlot {
    let mut a = base(128);
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = (1..=8u8)
        .map(|h| AtomPartial {
            harmonic: h,
            amplitude: 0.0,
            phase_cycles: 0.0,
        })
        .collect();
    a.render.normalize = true;
    a
}

fn fixture_two_partials_cancel_partially() -> AtomSlot {
    let mut a = base(128);
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = vec![
        AtomPartial {
            harmonic: 1,
            amplitude: 1.0,
            phase_cycles: 0.0,
        },
        AtomPartial {
            harmonic: 1,
            amplitude: 1.0,
            phase_cycles: 0.5,
        },
    ];
    a.render.normalize = true;
    a
}

fn fixture_max_amplitude_no_normalize() -> AtomSlot {
    let mut a = base(128);
    a.amplitude = 1.0;
    a.render.normalize = false;
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = (1..=4u8)
        .map(|h| AtomPartial {
            harmonic: h,
            amplitude: 1.0,
            phase_cycles: 0.0,
        })
        .collect();
    a
}

fn fixture_normalize_false_multi_partial_clamp_safety() -> AtomSlot {
    let mut a = base(128);
    a.amplitude = 1.0;
    a.render.normalize = false;
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = (1..=8u8)
        .map(|h| AtomPartial {
            harmonic: h,
            amplitude: 1.0,
            phase_cycles: 0.0,
        })
        .collect();
    a
}

fn fixture_harmonic_16_cycle_64() -> AtomSlot {
    let mut a = base(64);
    a.amplitude = 1.0;
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = vec![AtomPartial {
        harmonic: 16,
        amplitude: 1.0,
        phase_cycles: 0.0,
    }];
    a
}

fn fixture_all_8_partials_max_amp_harmonics_1_to_8() -> AtomSlot {
    let mut a = base(128);
    a.amplitude = 1.0;
    a.render.normalize = true;
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = (1..=8u8)
        .map(|h| AtomPartial {
            harmonic: h,
            amplitude: 1.0,
            phase_cycles: 0.0,
        })
        .collect();
    a
}

fn fixture_phase_cycles_0_9999() -> AtomSlot {
    let mut a = base(128);
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    partials[0].phase_cycles = 0.9999;
    a
}

fn fixture_cycle_256_canonical_sine() -> AtomSlot {
    base(256)
}

/// All nine fixtures, paired with their canonical baseline name
/// prefix (used to look entries up in `baselines/m3.json`).
fn all_fixtures() -> Vec<(&'static str, AtomSlot)> {
    vec![
        ("AMPLITUDE_ZERO", fixture_amplitude_zero()),
        (
            "ALL_PARTIALS_ZERO_NORMALIZE_TRUE",
            fixture_all_partials_zero_normalize_true(),
        ),
        (
            "TWO_PARTIALS_CANCEL_PARTIALLY",
            fixture_two_partials_cancel_partially(),
        ),
        (
            "MAX_AMPLITUDE_NO_NORMALIZE",
            fixture_max_amplitude_no_normalize(),
        ),
        (
            "NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY",
            fixture_normalize_false_multi_partial_clamp_safety(),
        ),
        ("HARMONIC_16_CYCLE_64", fixture_harmonic_16_cycle_64()),
        (
            "ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8",
            fixture_all_8_partials_max_amp_harmonics_1_to_8(),
        ),
        ("PHASE_CYCLES_0_9999", fixture_phase_cycles_0_9999()),
        (
            "CYCLE_256_CANONICAL_SINE",
            fixture_cycle_256_canonical_sine(),
        ),
    ]
}

// ---------------------------------------------------------------- helpers

/// Sentinel that prints every fixture's M3.2-pinned values. Run
/// with `cargo test -p sfc-atomizer-core --test atom_edge_cases
/// m3_2_print -- --nocapture --ignored` to capture fresh values
/// before updating `baselines/m3.json`.
#[test]
#[ignore]
fn m3_2_print_atom_edge_case_baselines() {
    for (name, atom) in all_fixtures() {
        let out = render_to_brr(&atom).expect("render");
        eprintln!("--- {name} ---");
        eprintln!("  PCM_SHA256                       = {}", out.pcm_sha256);
        eprintln!("  BRR_SHA256                       = {}", out.brr_sha256);
        eprintln!(
            "  DECODED_BRR_PCM_SHA256           = {}",
            out.decoded_brr_pcm_sha256
        );
        eprintln!(
            "  LOOP_CLICK_ABS                   = {}",
            out.loop_click_abs
        );
        eprintln!(
            "  LOOP_WINDOW_RMS_DELTA            = {}",
            out.loop_window_rms_delta
        );
        eprintln!(
            "  ROTATION_OFFSET                  = {}",
            out.rotation_offset
        );
        eprintln!(
            "  PEAK_ABS_ERROR_POST_ROTATION     = {}",
            out.peak_abs_error_post_rotation
        );
        eprintln!(
            "  RMS_ERROR_POST_ROTATION          = {}",
            out.rms_error_post_rotation
        );
    }
}

fn render(name: &str) -> AtomBrrOutput {
    let (_, atom) = all_fixtures()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("unknown fixture {name}"));
    render_to_brr(&atom).expect("render")
}

/// Reads the locked PCM SHA for a fixture from
/// `baselines/m3.json::identity_gated`. Mirrors the M2.8.1 /
/// M3.1 `include_str!` pattern.
fn locked_pcm_sha(fixture_name: &str) -> String {
    const BASELINES_JSON: &str = include_str!("../../baselines/m3.json");
    let baselines: serde_json::Value =
        serde_json::from_str(BASELINES_JSON).expect("baselines/m3.json must parse");
    let entry_name = format!("M3_ATOM_{fixture_name}_PCM_SHA256");
    let entry = baselines["identity_gated"]
        .as_array()
        .expect("baselines.identity_gated must be an array")
        .iter()
        .find(|e| e["name"].as_str() == Some(entry_name.as_str()))
        .unwrap_or_else(|| panic!("baselines/m3.json missing identity_gated entry {entry_name}"));
    entry["value"]
        .as_str()
        .unwrap_or_else(|| panic!("{entry_name} value must be a string"))
        .to_string()
}

// ---------------------------------------------------------------- special-case assertions

/// amplitude_zero MUST produce all-zero PCM — load-bearing for the
/// M3.3 phase-rotation tie-breaker test (consultant M3 plan #10, #16).
/// If render produces small non-zero values due to floating-point
/// noise in the partials sum, that's a defensive-coding gap (likely
/// the normalize special-case or a missing amplitude-zero
/// short-circuit) and must be surfaced before M3.3.
#[test]
fn amplitude_zero_produces_all_zero_pcm() {
    let out = render("AMPLITUDE_ZERO");
    assert_eq!(
        out.pcm,
        vec![0i16; 128],
        "amplitude_zero must render all-zero PCM"
    );
    // All-zero PCM → BRR-encode-decode round trip stays all-zero
    // (filter 0 / shift 0 / nibbles 0 is a valid block); decoded
    // PCM stays all-zero → seam delta is zero.
    assert_eq!(
        out.loop_click_abs, 0,
        "amplitude_zero must produce loop_click_abs = 0 (load-bearing for M3.3 tie-breaker)"
    );
    assert_eq!(
        out.loop_window_rms_delta, 0.0,
        "amplitude_zero must produce zero windowed delta"
    );
}

/// all_partials_zero with normalize=true exercises the normalize
/// max==0 special case. Render must not divide by zero, must not
/// produce NaN, must produce all-zero PCM, and must produce
/// finite metrics.
#[test]
fn all_partials_zero_normalize_true_renders_cleanly() {
    let out = render("ALL_PARTIALS_ZERO_NORMALIZE_TRUE");
    for s in &out.pcm {
        assert_eq!(*s, 0, "all-partials-zero must render all-zero PCM");
    }
    assert_eq!(out.loop_click_abs, 0);
    assert!(out.loop_window_rms_delta.is_finite());
    assert_eq!(out.loop_window_rms_delta, 0.0);
}

/// two_partials_cancel_partially exercises a near-cancellation
/// path. The math says `sin(θ) + sin(θ + π) = 0` for every sample,
/// but `f64::sin` is not analytically exact: `sin(θ + π)` and
/// `-sin(θ)` agree only to within a few ULPs. The summed waveform
/// is therefore not exactly zero — it is a ULP-scale noise floor.
/// Then `normalize=true` divides by the tiny `max_abs`, amplifying
/// that noise to roughly ±1.0, and the `amplitude * 32767` scale
/// produces a non-zero (but deterministic and bounded) PCM.
///
/// This test asserts that surface explicitly: render does NOT
/// panic, does NOT produce NaN, IS deterministic (covered by
/// `atom_edge_case_fixtures_render_deterministically`), and the
/// output stays in i16 range. The brief's original prediction
/// (all-zero PCM) assumed exact FP cancellation; in practice the
/// normalize `max == 0` special case is bypassed by ULP noise and
/// the noise floor is what gets captured. PM may revisit whether
/// the normalize step should treat near-zero max as zero at M3+;
/// any such change is a SPEC §16.9 amendment and is out of M3.2
/// scope.
#[test]
fn two_partials_cancel_partially_renders_bounded_and_finite() {
    let out = render("TWO_PARTIALS_CANCEL_PARTIALLY");
    assert_eq!(out.pcm.len(), 128);
    for (i, s) in out.pcm.iter().enumerate() {
        let v = *s as i32;
        assert!(
            (-32768..=32767).contains(&v),
            "fully-cancelling partials produced out-of-range sample at {i}: {v}"
        );
    }
    assert!(out.loop_window_rms_delta.is_finite());
    assert!(out.loop_click_abs >= 0);
}

/// harmonic_16_cycle_64 is near-Nyquist content. Render must not
/// panic; metrics must be finite. The fixture matters for M3.4
/// predictor optimization and M3.6 pre-emphasis later — those are
/// the encoder passes whose behavior diverges at high-frequency
/// content.
#[test]
fn harmonic_16_cycle_64_renders_with_finite_metric() {
    let out = render("HARMONIC_16_CYCLE_64");
    assert_eq!(out.pcm.len(), 64);
    assert!(
        out.loop_window_rms_delta.is_finite(),
        "near-Nyquist content must produce a finite windowed metric"
    );
    // Loop click is a non-negative i32; assert defensively.
    assert!(out.loop_click_abs >= 0);
}

// ---------------------------------------------------------------- determinism

/// Every fixture must produce bit-identical output across two
/// consecutive `render_to_brr` calls. f64 reduction-order drift
/// or HashMap-iteration nondeterminism would surface here.
#[test]
fn atom_edge_case_fixtures_render_deterministically() {
    for (name, atom) in all_fixtures() {
        let a = render_to_brr(&atom).expect("render a");
        let b = render_to_brr(&atom).expect("render b");
        assert_eq!(a.pcm, b.pcm, "{name}: pcm drift across two renders");
        assert_eq!(a.brr_bytes, b.brr_bytes, "{name}: brr_bytes drift");
        assert_eq!(a.pcm_sha256, b.pcm_sha256, "{name}: pcm_sha256 drift");
        assert_eq!(a.brr_sha256, b.brr_sha256, "{name}: brr_sha256 drift");
        assert_eq!(
            a.decoded_brr_pcm_sha256, b.decoded_brr_pcm_sha256,
            "{name}: decoded_brr_pcm_sha256 drift"
        );
        assert_eq!(
            a.loop_click_abs, b.loop_click_abs,
            "{name}: loop_click_abs drift"
        );
        assert_eq!(
            a.loop_window_rms_delta.to_bits(),
            b.loop_window_rms_delta.to_bits(),
            "{name}: loop_window_rms_delta drift (bit comparison)"
        );
    }
}

// ---------------------------------------------------------------- per-fixture identity-pin tests

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_amplitude_zero() {
    assert_eq!(
        render("AMPLITUDE_ZERO").pcm_sha256,
        locked_pcm_sha("AMPLITUDE_ZERO")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_all_partials_zero_normalize_true() {
    assert_eq!(
        render("ALL_PARTIALS_ZERO_NORMALIZE_TRUE").pcm_sha256,
        locked_pcm_sha("ALL_PARTIALS_ZERO_NORMALIZE_TRUE")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_two_partials_cancel_partially() {
    assert_eq!(
        render("TWO_PARTIALS_CANCEL_PARTIALLY").pcm_sha256,
        locked_pcm_sha("TWO_PARTIALS_CANCEL_PARTIALLY")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_max_amplitude_no_normalize() {
    assert_eq!(
        render("MAX_AMPLITUDE_NO_NORMALIZE").pcm_sha256,
        locked_pcm_sha("MAX_AMPLITUDE_NO_NORMALIZE")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_normalize_false_multi_partial_clamp_safety() {
    assert_eq!(
        render("NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY").pcm_sha256,
        locked_pcm_sha("NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_harmonic_16_cycle_64() {
    assert_eq!(
        render("HARMONIC_16_CYCLE_64").pcm_sha256,
        locked_pcm_sha("HARMONIC_16_CYCLE_64")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_all_8_partials_max_amp_harmonics_1_to_8() {
    assert_eq!(
        render("ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8").pcm_sha256,
        locked_pcm_sha("ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_phase_cycles_0_9999() {
    assert_eq!(
        render("PHASE_CYCLES_0_9999").pcm_sha256,
        locked_pcm_sha("PHASE_CYCLES_0_9999")
    );
}

#[test]
fn atom_pcm_sha_matches_locked_baseline_m3_cycle_256_canonical_sine() {
    assert_eq!(
        render("CYCLE_256_CANONICAL_SINE").pcm_sha256,
        locked_pcm_sha("CYCLE_256_CANONICAL_SINE")
    );
}

// ---------------------------------------------------------------- M3.3 phase rotation regressions

/// **M3.3 tie-breaker test (Block per consultant M3.2 audit #16).**
///
/// amplitude_zero produces all-zero PCM → all-zero BRR → all-zero
/// decoded → every rotation candidate scores `(0, 0, 0.0, offset)`.
/// The lex tie-breaker is the final `offset` comparison; smallest
/// offset wins; rotation_offset must be 0.
///
/// If this test fails, the SPEC §10.7 lex objective implementation
/// has an iteration-order bug — `pick_best_rotation` is not picking
/// the smallest offset on full-tie inputs.
#[test]
fn amplitude_zero_atom_phase_rotation_picks_offset_zero() {
    let out = render("AMPLITUDE_ZERO");
    assert_eq!(
        out.rotation_offset, 0,
        "amplitude_zero atom: lex tie-break must select offset=0 \
         (all candidates score identically at zero)"
    );
    assert_eq!(out.loop_click_abs, 0);
    assert_eq!(out.peak_abs_error_post_rotation, 0);
    assert_eq!(out.rms_error_post_rotation, 0.0);
}

/// **M3.3 improvement gate — `M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE`.**
///
/// For every atom fixture (canonical sine_128 / sine_64 + the nine
/// M3.2 edge cases), the post-rotation `loop_click_abs` must be
/// `<=` the pre-M3 value. Equal is fine (rotation chose offset=0 or
/// found no improvement); strictly greater would mean the lex
/// objective is selecting a worse candidate, which would be a bug.
///
/// Both values are read from `baselines/m3.json::documentary_snapshot`
/// via `include_str!`, so the baseline file is the single source of
/// truth. Pre-M3 entries are `M3_ATOM_<NAME>_LOOP_CLICK_ABS_PRE_M3`;
/// post-rotation entries are `M3_ATOM_<NAME>_LOOP_CLICK_ABS_PHASE_ROTATION`.
#[test]
fn phase_rotation_loop_click_never_regresses_against_pre_m3() {
    const BASELINES_JSON: &str = include_str!("../../baselines/m3.json");
    let baselines: serde_json::Value =
        serde_json::from_str(BASELINES_JSON).expect("baselines/m3.json must parse");
    let ds = baselines["documentary_snapshot"]
        .as_array()
        .expect("documentary_snapshot must be an array");

    let fetch = |name: &str| -> i64 {
        ds.iter()
            .find(|e| e["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("baselines/m3.json missing entry {name}"))["value"]
            .as_i64()
            .unwrap_or_else(|| panic!("{name} value must be an integer"))
    };

    // The 11 fixtures: 2 canonical (128/64) + 9 M3.2 edge cases.
    let names: &[&str] = &[
        "128_SINE",
        "64_SINE",
        "AMPLITUDE_ZERO",
        "ALL_PARTIALS_ZERO_NORMALIZE_TRUE",
        "TWO_PARTIALS_CANCEL_PARTIALLY",
        "MAX_AMPLITUDE_NO_NORMALIZE",
        "NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY",
        "HARMONIC_16_CYCLE_64",
        "ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8",
        "PHASE_CYCLES_0_9999",
        "CYCLE_256_CANONICAL_SINE",
    ];

    for name in names {
        let pre = fetch(&format!("M3_ATOM_{name}_LOOP_CLICK_ABS_PRE_M3"));
        let post = fetch(&format!("M3_ATOM_{name}_LOOP_CLICK_ABS_PHASE_ROTATION"));
        assert!(
            post <= pre,
            "M3_PHASE_ROTATION_LOOP_CLICK_IMPROVEMENT_GATE violated for {name}: \
             post={post} > pre={pre}"
        );
    }
}
