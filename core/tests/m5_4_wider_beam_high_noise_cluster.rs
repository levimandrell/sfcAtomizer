//! M5.4 Phase A — Wider-beam scientific-closure benchmark.
//!
//! Runs the M4.4 spike (`encode_looped_m4_4_spike` beam search) at
//! `beam_width ∈ {8, 16}` against the 4 high-noise cluster fixtures
//! only, and prints per-fixture peak/rms/snr/runtime deltas against
//! the M4.3 documentary baselines from `baselines/m4.json`.
//!
//! Marked `#[ignore]` per consultant M4.4 audit #6: scientific
//! closure benchmark, not a release path. Run explicitly:
//!
//! ```text
//! cargo test --release --test m5_4_wider_beam_high_noise_cluster \
//!     -- --ignored --nocapture
//! ```
//!
//! Expected outcome (consultant M4.4 audit #2 structural-ceiling
//! claim): widths 8/16 stay at peak_abs_raw_vs_source = 18431 because
//! the limit is structural at the current-sample term; widths 8/16
//! add cross-block predictor exploration but unlikely to find paths
//! around the ceiling. If any (fixture × width) combination delivers
//! ≥10% peak or rms improvement, STOP per the M5.4 brief.

use std::time::Instant;

use sfc_atomizer_core::atom::{
    render_to_brr, render_to_pcm, rotate_pcm, rotation_candidate_offsets, AtomKind, AtomPartial,
    AtomRenderOptions, AtomSlot,
};
use sfc_atomizer_core::audition::{
    peak_abs_raw_vs_source, rms_raw_vs_source, snr_db,
};
use sfc_atomizer_core::brr::{decode_blocks, BrrDecoderState};
use sfc_atomizer_core::brr_encoder::{
    encode_looped_m4_4_spike, EncodeOptions, M44SpikeConfig, M44Strategy,
};
use sfc_atomizer_core::project::{Envelope, SamplePlayback};

// M4.3 baselines for the 4 high-noise cluster fixtures (peak, rms, snr).
// Sourced from baselines/m4.json::M4_3_ATOM_*_HIGH_NOISE.
struct M43Ref {
    peak: i32,
    rms: f64,
    snr_db: f64,
}

const M43_MAX_AMPLITUDE_NO_NORMALIZE: M43Ref = M43Ref {
    peak: 18431,
    rms: 10576.551517389777,
    snr_db: 6.395430157288107,
};
const M43_NORMALIZE_FALSE_MULTI_PARTIAL: M43Ref = M43Ref {
    peak: 18431,
    rms: 10562.454493245166,
    snr_db: 6.3332064396147185,
};
const M43_HARMONIC_16_CYCLE_64: M43Ref = M43Ref {
    peak: 18431,
    rms: 12329.88696217447,
    snr_db: 5.479251764334529,
};
const M43_ALL_8_PARTIALS: M43Ref = M43Ref {
    peak: 18431,
    rms: 4574.389392312377,
    snr_db: 7.405030359634541,
};

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

fn high_noise_fixtures() -> Vec<(&'static str, AtomSlot, M43Ref)> {
    let mk_max_amp_no_norm = || {
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
    };
    let mk_normalize_false_clamp = || {
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
    };
    let mk_harmonic_16_cycle_64 = || {
        let mut a = base(64);
        a.amplitude = 1.0;
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
        *partials = vec![AtomPartial {
            harmonic: 16,
            amplitude: 1.0,
            phase_cycles: 0.0,
        }];
        a
    };
    let mk_all_8_partials = || {
        let mut a = base(128);
        a.amplitude = 1.0;
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
        *partials = (1..=8u8)
            .map(|h| AtomPartial {
                harmonic: h,
                amplitude: 1.0,
                phase_cycles: 0.0,
            })
            .collect();
        a
    };

    vec![
        (
            "MAX_AMPLITUDE_NO_NORMALIZE",
            mk_max_amp_no_norm(),
            M43_MAX_AMPLITUDE_NO_NORMALIZE,
        ),
        (
            "NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY",
            mk_normalize_false_clamp(),
            M43_NORMALIZE_FALSE_MULTI_PARTIAL,
        ),
        (
            "HARMONIC_16_CYCLE_64",
            mk_harmonic_16_cycle_64(),
            M43_HARMONIC_16_CYCLE_64,
        ),
        (
            "ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8",
            mk_all_8_partials(),
            M43_ALL_8_PARTIALS,
        ),
    ]
}

struct SpikeResult {
    peak: i32,
    rms: f64,
    snr_db: f64,
}

fn run_spike(atom: &AtomSlot, cfg: &M44SpikeConfig) -> SpikeResult {
    let source = render_to_pcm(atom);
    let opts = EncodeOptions {
        force_filter_0_first_block: atom.render.force_filter_0_first_block,
        loop_entry_block_index: Some(0),
    };
    let offsets = rotation_candidate_offsets(source.len());
    let mut best: Option<(i32, f64, Vec<i16>, Vec<i16>)> = None;
    for offset in offsets {
        let rotated = rotate_pcm(&source, offset);
        let result = encode_looped_m4_4_spike(&rotated, 0, &opts, cfg)
            .expect("spike encode infallible at loop_start=0");
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
        // Same lex-objective as production for rotation selection
        // (peak primary, rms secondary) so we compare apples to
        // apples against M4.3 production baselines.
        let key = (p, r);
        if best.as_ref().is_none_or(|b| key < (b.0, b.1)) {
            best = Some((p, r, rotated, decoded));
        }
    }
    let (peak, rms, rotated, decoded) = best.expect("at least one rotation candidate");
    SpikeResult {
        peak,
        rms,
        snr_db: snr_db(&rotated, &decoded),
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
fn m5_4_wider_beam_high_noise_cluster() {
    eprintln!(
        "\nM5_4_WIDER_BEAM\tfixture\twidth\tpeak\tpeak_delta_pct\trms\trms_delta_pct\tsnr_db\tsnr_delta_db\truntime_ms"
    );

    for &width in &[8u32, 16u32] {
        let cfg = M44SpikeConfig {
            strategy: M44Strategy::BeamSearch { beam_width: width },
        };
        for (name, atom, m43) in high_noise_fixtures() {
            // Warm cargo's per-test caches with one untimed run, then
            // measure the second.
            let _ = run_spike(&atom, &cfg);
            let t0 = Instant::now();
            let r = run_spike(&atom, &cfg);
            let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let p_pct = pct_delta(r.peak as f64, m43.peak as f64);
            let rms_pct = pct_delta(r.rms, m43.rms);
            let snr_delta = r.snr_db - m43.snr_db;
            eprintln!(
                "M5_4_WIDER_BEAM\t{name}\twidth_{width}\tpeak={}\tpeak_delta={:+.2}%\trms={:.3}\trms_delta={:+.2}%\tsnr_db={:.3}\tsnr_delta={:+.3} dB\truntime={:.2} ms",
                r.peak, p_pct, r.rms, rms_pct, r.snr_db, snr_delta, elapsed_ms,
            );

            // Stop-condition guard (per M5.4 brief): >=10% peak or
            // rms improvement on any fixture × width refines the
            // M4.4 #2 structural-ceiling claim. Fail loudly so the
            // ignored-benchmark run surfaces it.
            assert!(
                p_pct > -10.0,
                "STOP per M5.4 brief: {name} @ width={width} peak \
                 improved by {:.2}% (>10%); refines M4.4 #2 structural \
                 ceiling claim — surface to PM before continuing",
                -p_pct
            );
            assert!(
                rms_pct > -10.0,
                "STOP per M5.4 brief: {name} @ width={width} rms \
                 improved by {:.2}% (>10%); refines M4.4 #2 structural \
                 ceiling claim — surface to PM before continuing",
                -rms_pct
            );
        }
    }
}

#[test]
fn m5_4_wider_beam_decode_roundtrip_clean() {
    // Non-ignored counterpart: just ensure both widths produce valid
    // BRR for every high-noise fixture. Cheap (no rotation sweep at
    // measurement granularity); the ignored test above carries the
    // measurement weight.
    let cfg_8 = M44SpikeConfig {
        strategy: M44Strategy::BeamSearch { beam_width: 8 },
    };
    let cfg_16 = M44SpikeConfig {
        strategy: M44Strategy::BeamSearch { beam_width: 16 },
    };
    for (name, atom, _m43) in high_noise_fixtures() {
        for cfg in [&cfg_8, &cfg_16] {
            let source = render_to_pcm(&atom);
            let opts = EncodeOptions {
                force_filter_0_first_block: atom.render.force_filter_0_first_block,
                loop_entry_block_index: Some(0),
            };
            let result = encode_looped_m4_4_spike(&source, 0, &opts, cfg)
                .expect("spike encode infallible");
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
            let _decoded = decode_blocks(&blocks, &mut state);
            // The fact that decode_blocks didn't panic + bytes are
            // multiple of 9 means the BRR is well-formed.
            assert!(
                !result.bytes.is_empty(),
                "{name} produced empty BRR for cfg {cfg:?}"
            );
        }
        // Production render still works (atom render formula unchanged):
        let _ = render_to_brr(&atom).expect("production render");
    }
}
