//! M5.4 Phase B — Alternative shift-selection objective spike.
//!
//! Drives `encode_looped_m5_4_alt_shift_spike` with
//! `ShiftObjective::RmsThenPeak` against `HARMONIC_16_CYCLE_64` (the
//! worst high-noise cluster fixture by per-block error rms) and
//! reports peak/rms/snr delta vs the M4.3 documentary baseline.
//!
//! Marked `#[ignore]` per the M5.4 brief: documentary single-fixture
//! spike. Run explicitly:
//!
//! ```text
//! cargo test --release --test m5_4_alt_shift_objective_spike \
//!     -- --ignored --nocapture
//! ```
//!
//! Consultant M4.4 audit #7 prediction: alt-shift-objective does NOT
//! clear the 10% exit criterion. The hypothesis tested here is the
//! flipped lex ordering — `sum_sq` first, `peak` as tiebreak —
//! which trades peak-clipping avoidance for inner-sample RMS
//! reduction. If results exceed `>5%` rms or peak improvement on
//! HARMONIC_16_CYCLE_64, STOP per M5.4 brief.

use sfc_atomizer_core::atom::{
    render_to_pcm, rotate_pcm, rotation_candidate_offsets, AtomKind, AtomPartial,
    AtomRenderOptions, AtomSlot,
};
use sfc_atomizer_core::audition::{peak_abs_raw_vs_source, rms_raw_vs_source, snr_db};
use sfc_atomizer_core::brr::{decode_blocks, BrrDecoderState};
use sfc_atomizer_core::brr_encoder::{
    encode_looped_m5_4_alt_shift_spike, EncodeOptions, ShiftObjective,
};
use sfc_atomizer_core::project::{Envelope, SamplePlayback};

// M4.3 baseline for HARMONIC_16_CYCLE_64 (peak, rms, snr) sourced
// from baselines/m4.json::M4_3_ATOM_HARMONIC_16_CYCLE_64_*.
const M43_HARMONIC_16_PEAK: i32 = 18431;
const M43_HARMONIC_16_RMS: f64 = 12329.88696217447;
const M43_HARMONIC_16_SNR_DB: f64 = 5.479251764334529;

fn harmonic_16_cycle_64() -> AtomSlot {
    let mut a = AtomSlot {
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
        cycle_len_samples: 64,
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
    };
    let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
    *partials = vec![AtomPartial {
        harmonic: 16,
        amplitude: 1.0,
        phase_cycles: 0.0,
    }];
    a
}

struct AltShiftResult {
    peak: i32,
    rms: f64,
    snr_db: f64,
    clipping: u32,
}

fn run_alt_shift(atom: &AtomSlot, objective: ShiftObjective) -> AltShiftResult {
    let source = render_to_pcm(atom);
    let opts = EncodeOptions {
        force_filter_0_first_block: atom.render.force_filter_0_first_block,
        loop_entry_block_index: Some(0),
    };
    let offsets = rotation_candidate_offsets(source.len());
    let mut best: Option<(i32, f64, Vec<i16>, Vec<i16>)> = None;
    for offset in offsets {
        let rotated = rotate_pcm(&source, offset);
        let result = encode_looped_m5_4_alt_shift_spike(&rotated, 0, &opts, objective)
            .expect("alt-shift spike infallible at loop_start=0");
        let blocks: Vec<[u8; 9]> = result
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
        let p = peak_abs_raw_vs_source(&rotated, &decoded);
        let r = rms_raw_vs_source(&rotated, &decoded);
        // Production-equivalent rotation lex objective (peak then
        // rms) so the comparison vs M4.3 baseline is apples-to-apples
        // at the rotation-selection layer; only the per-block
        // shift-selection objective differs.
        let key = (p, r);
        if best.as_ref().is_none_or(|b| key < (b.0, b.1)) {
            best = Some((p, r, rotated, decoded));
        }
    }
    let (peak, rms, rotated, decoded) = best.expect("at least one rotation candidate");
    let mut clip = 0u32;
    for s in &decoded {
        if (*s as i32).abs() >= 32767 {
            clip += 1;
        }
    }
    AltShiftResult {
        peak,
        rms,
        snr_db: snr_db(&rotated, &decoded),
        clipping: clip,
    }
}

fn pct_delta(spike: f64, baseline: f64) -> f64 {
    if baseline.abs() < 1e-9 {
        0.0
    } else {
        (spike - baseline) / baseline * 100.0
    }
}

#[test]
#[ignore]
fn m5_4_alt_shift_objective_harmonic_16() {
    let atom = harmonic_16_cycle_64();

    // Control trial: PeakThenSumSq via the spike entry. Should match
    // M3.3 production at the rotation lex objective level (= M4.3
    // documentary baselines on this fixture exactly).
    let control = run_alt_shift(&atom, ShiftObjective::PeakThenSumSq);
    // Treatment trial: M5.4 alt objective.
    let treatment = run_alt_shift(&atom, ShiftObjective::RmsThenPeak);

    eprintln!(
        "\nM5_4_ALT_SHIFT\tfixture\tobjective\tpeak\tpeak_delta_pct_vs_m4_3\trms\trms_delta_pct_vs_m4_3\tsnr_db\tsnr_delta_db_vs_m4_3\tclipping_count_raw"
    );
    for (label, r) in [
        ("PeakThenSumSq_control", &control),
        ("RmsThenPeak_treatment", &treatment),
    ] {
        let p_pct = pct_delta(r.peak as f64, M43_HARMONIC_16_PEAK as f64);
        let rms_pct = pct_delta(r.rms, M43_HARMONIC_16_RMS);
        let snr_delta = r.snr_db - M43_HARMONIC_16_SNR_DB;
        eprintln!(
            "M5_4_ALT_SHIFT\tHARMONIC_16_CYCLE_64\t{label}\tpeak={}\tpeak_delta={:+.2}%\trms={:.3}\trms_delta={:+.2}%\tsnr_db={:.3}\tsnr_delta={:+.3} dB\tclip={}",
            r.peak, p_pct, r.rms, rms_pct, r.snr_db, snr_delta, r.clipping,
        );
    }

    // Stop-condition guard per M5.4 brief: >5% rms or peak
    // improvement on HARMONIC_16_CYCLE_64 falsifies consultant M4.4
    // audit #7's prediction.
    let treatment_peak_pct = pct_delta(treatment.peak as f64, M43_HARMONIC_16_PEAK as f64);
    let treatment_rms_pct = pct_delta(treatment.rms, M43_HARMONIC_16_RMS);
    assert!(
        treatment_peak_pct > -5.0,
        "STOP per M5.4 brief: alt-shift-objective peak improved by \
         {:.2}% (>5%); falsifies consultant M4.4 audit #7 prediction \
         on HARMONIC_16_CYCLE_64 — surface to PM before continuing",
        -treatment_peak_pct
    );
    assert!(
        treatment_rms_pct > -5.0,
        "STOP per M5.4 brief: alt-shift-objective rms improved by \
         {:.2}% (>5%); falsifies consultant M4.4 audit #7 prediction \
         on HARMONIC_16_CYCLE_64 — surface to PM before continuing",
        -treatment_rms_pct
    );
}

#[test]
fn m5_4_alt_shift_decode_roundtrip_clean() {
    // Non-ignored sanity: alt-shift spike still produces decode-clean
    // BRR. Same as the M4.4 spike's roundtrip test pattern.
    let atom = harmonic_16_cycle_64();
    for obj in [ShiftObjective::PeakThenSumSq, ShiftObjective::RmsThenPeak] {
        let _ = run_alt_shift(&atom, obj);
    }
}

#[test]
fn m5_4_alt_shift_peak_then_sum_sq_matches_production_path() {
    // The spike's PeakThenSumSq objective is the same lex ordering
    // as M3.3 production. This sanity asserts that the M5.4 spike
    // entry, when invoked with PeakThenSumSq, produces byte-identical
    // BRR to production encode_looped on a simple test signal —
    // catches accidental drift in the refactor.
    use sfc_atomizer_core::brr_encoder::{encode_looped, EncodeOptions};

    let atom = harmonic_16_cycle_64();
    let source = render_to_pcm(&atom);
    let opts = EncodeOptions {
        force_filter_0_first_block: atom.render.force_filter_0_first_block,
        loop_entry_block_index: Some(0),
    };
    let prod = encode_looped(&source, 0, &opts).expect("prod encode");
    let spike =
        encode_looped_m5_4_alt_shift_spike(&source, 0, &opts, ShiftObjective::PeakThenSumSq)
            .expect("spike encode");
    assert_eq!(
        prod.bytes, spike.bytes,
        "M5.4 spike with PeakThenSumSq must match production encode_looped byte-for-byte"
    );
}
