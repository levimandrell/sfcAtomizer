//! M4.4 — Encoder improvement spike measurement.
//!
//! Runs the `encode_looped_m4_4_spike` (beam search, width = 4)
//! against the 11 atom fixtures + 9 `m3_5_canonical`
//! characterization signals, captures the four §10.10 noise-floor
//! metrics + `loop_click_abs` per fixture, and prints them so the
//! Phase C exit-criterion application can compare against the
//! M4.3 documentary baselines from `baselines/m4.json`.
//!
//! The non-print tests assert the spike's BRR is well-formed
//! (decodes cleanly) and that the encoder is deterministic
//! across two runs. The actual numeric thresholds are evaluated
//! externally (in STATUS / Python comparison script) — keeps
//! the test layer's job at "did the spike produce valid output"
//! while the ship/skip decision lives in human-reviewable STATUS.

use sfc_atomizer_core::atom::{
    render_to_brr, render_to_pcm, rotate_pcm, rotation_candidate_offsets, AtomBrrOutput, AtomKind,
    AtomPartial, AtomRenderOptions, AtomSlot,
};
use sfc_atomizer_core::audition::{
    clipping_count_raw, loop_click_abs, peak_abs_raw_vs_source, rms_raw_vs_source, snr_db,
};
use sfc_atomizer_core::brr::{decode_blocks, BrrDecoderState};
use sfc_atomizer_core::brr_encoder::{
    encode_looped_m4_4_spike, EncodeOptions, M44SpikeConfig, M44Strategy,
};
use sfc_atomizer_core::characterize_gaussian::m3_5_canonical_signals;
use sfc_atomizer_core::project::{Envelope, SamplePlayback};

const SPIKE: M44SpikeConfig = M44SpikeConfig {
    strategy: M44Strategy::BeamSearch { beam_width: 4 },
};

// ---- Fixture builders (mirror the M3.2 + atom_render canonical set) ----

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

fn fixtures_11_atoms() -> Vec<(&'static str, AtomSlot)> {
    let mk_zero_amp = || {
        let mut a = base(128);
        a.amplitude = 0.0;
        a
    };
    let mk_all_partials_zero = || {
        let mut a = base(128);
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
        *partials = (1..=8u8)
            .map(|h| AtomPartial {
                harmonic: h,
                amplitude: 0.0,
                phase_cycles: 0.0,
            })
            .collect();
        a
    };
    let mk_two_partials_cancel = || {
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
        a
    };
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
    let mk_phase_cycles_0_9999 = || {
        let mut a = base(128);
        let AtomKind::AdditiveSingleCycleV0 { partials } = &mut a.kind;
        partials[0].phase_cycles = 0.9999;
        a
    };

    vec![
        ("128_SINE", base(128)),
        ("64_SINE", base(64)),
        ("AMPLITUDE_ZERO", mk_zero_amp()),
        ("ALL_PARTIALS_ZERO_NORMALIZE_TRUE", mk_all_partials_zero()),
        ("TWO_PARTIALS_CANCEL_PARTIALLY", mk_two_partials_cancel()),
        ("MAX_AMPLITUDE_NO_NORMALIZE", mk_max_amp_no_norm()),
        (
            "NORMALIZE_FALSE_MULTI_PARTIAL_CLAMP_SAFETY",
            mk_normalize_false_clamp(),
        ),
        ("HARMONIC_16_CYCLE_64", mk_harmonic_16_cycle_64()),
        (
            "ALL_8_PARTIALS_MAX_AMP_HARMONICS_1_TO_8",
            mk_all_8_partials(),
        ),
        ("PHASE_CYCLES_0_9999", mk_phase_cycles_0_9999()),
        ("CYCLE_256_CANONICAL_SINE", base(256)),
    ]
}

// ---- Spike runner: applies the same SPEC §10.7 rotation contract as
// render_to_brr, but uses the beam-search encoder instead of greedy.
// Returns (peak, rms, snr, clip, loop_click_abs, rotation_offset).

struct SpikeResult {
    peak_abs_raw_vs_source: i32,
    rms_raw_vs_source: f64,
    snr_db: f64,
    clipping_count_raw: u32,
    loop_click_abs: i32,
    rotation_offset: u32,
}

struct RotationCandidate {
    offset: u32,
    rotated: Vec<i16>,
    decoded: Vec<i16>,
    loop_click_abs: i32,
    peak: i32,
    rms: f64,
}

fn run_spike_on_atom(atom: &AtomSlot) -> SpikeResult {
    let source = render_to_pcm(atom);
    let opts = EncodeOptions {
        force_filter_0_first_block: atom.render.force_filter_0_first_block,
        loop_entry_block_index: Some(0),
    };
    let offsets = rotation_candidate_offsets(source.len());
    let mut candidates: Vec<RotationCandidate> = Vec::with_capacity(offsets.len());
    for offset in offsets {
        let rotated = rotate_pcm(&source, offset);
        let result = encode_looped_m4_4_spike(&rotated, 0, &opts, &SPIKE)
            .expect("spike encode infallible at loop_start=0 for valid atoms");
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
        let lc = loop_click_abs(&decoded, 0, decoded.len());
        let pae = peak_abs_raw_vs_source(&rotated, &decoded);
        let rmse = rms_raw_vs_source(&rotated, &decoded);
        candidates.push(RotationCandidate {
            offset: offset as u32,
            rotated,
            decoded,
            loop_click_abs: lc,
            peak: pae,
            rms: rmse,
        });
    }
    // SPEC §10.7 lex objective: loop_click primary, peak secondary,
    // rms tertiary, offset tie-break.
    let best = candidates
        .into_iter()
        .min_by(|a, b| {
            a.loop_click_abs
                .cmp(&b.loop_click_abs)
                .then(a.peak.cmp(&b.peak))
                .then(a.rms.total_cmp(&b.rms))
                .then(a.offset.cmp(&b.offset))
        })
        .expect("at least one rotation candidate");
    SpikeResult {
        peak_abs_raw_vs_source: best.peak,
        rms_raw_vs_source: best.rms,
        snr_db: snr_db(&best.rotated, &best.decoded),
        clipping_count_raw: clipping_count_raw(&best.decoded),
        loop_click_abs: best.loop_click_abs,
        rotation_offset: best.offset,
    }
}

#[test]
fn m4_4_spike_decode_roundtrip_clean_on_all_atom_fixtures() {
    for (name, atom) in fixtures_11_atoms() {
        let _ = run_spike_on_atom(&atom);
        // run_spike_on_atom already drives decode_blocks; reaching
        // here means decoding succeeded for every rotation candidate.
        // Sanity log:
        eprintln!("M4_4_SPIKE\t{name}\tdecode_roundtrip_ok");
    }
}

#[test]
fn m4_4_spike_deterministic_two_run_byte_identity() {
    for (name, atom) in fixtures_11_atoms() {
        let a = run_spike_on_atom(&atom);
        let b = run_spike_on_atom(&atom);
        assert_eq!(
            a.peak_abs_raw_vs_source, b.peak_abs_raw_vs_source,
            "peak non-deterministic for {name}"
        );
        assert_eq!(
            a.rms_raw_vs_source.to_bits(),
            b.rms_raw_vs_source.to_bits(),
            "rms non-deterministic for {name}"
        );
        assert_eq!(
            a.snr_db.to_bits(),
            b.snr_db.to_bits(),
            "snr non-deterministic for {name}"
        );
        assert_eq!(
            a.clipping_count_raw, b.clipping_count_raw,
            "clip non-deterministic for {name}"
        );
        assert_eq!(
            a.loop_click_abs, b.loop_click_abs,
            "loop_click non-deterministic for {name}"
        );
        assert_eq!(
            a.rotation_offset, b.rotation_offset,
            "rotation_offset non-deterministic for {name}"
        );
    }
}

/// Print spike metrics for every atom fixture so Phase C can
/// compare against M4.3 baselines. Ignored (one-off capture).
#[test]
#[ignore]
fn m4_4_print_spike_atom_fixture_metrics() {
    for (name, atom) in fixtures_11_atoms() {
        let r = run_spike_on_atom(&atom);
        eprintln!(
            "M4_4_SPIKE_ATOM\t{name}\tpeak_abs={}\trms={}\tsnr_db={}\tclip={}\tloop_click={}\trot_off={}",
            r.peak_abs_raw_vs_source,
            r.rms_raw_vs_source,
            r.snr_db,
            r.clipping_count_raw,
            r.loop_click_abs,
            r.rotation_offset,
        );
    }
}

/// Spike runtime measurement — render all 11 atom fixtures and
/// report wall-clock totals for the greedy M3.3 path vs. the
/// beam-search M4.4 path. Ignored (informational only; runtime
/// exit criterion is < 2x baseline, evaluated externally).
#[test]
#[ignore]
fn m4_4_runtime_spike_vs_m3_3_production() {
    use std::time::Instant;
    let fixtures = fixtures_11_atoms();

    let t0 = Instant::now();
    for (_, atom) in &fixtures {
        let _ = render_to_brr(atom).expect("M3.3");
    }
    let prod_elapsed = t0.elapsed();

    let t1 = Instant::now();
    for (_, atom) in &fixtures {
        let _ = run_spike_on_atom(atom);
    }
    let spike_elapsed = t1.elapsed();

    eprintln!(
        "M4_4_RUNTIME\tm3_3_production={:.3}ms\tm4_4_spike_beam_4={:.3}ms\tratio={:.2}",
        prod_elapsed.as_secs_f64() * 1000.0,
        spike_elapsed.as_secs_f64() * 1000.0,
        spike_elapsed.as_secs_f64() / prod_elapsed.as_secs_f64().max(1e-9),
    );
}

/// Same shape as `m4_4_print_spike_atom_fixture_metrics` but with
/// beam_width=16 to confirm wider beam doesn't unlock additional
/// improvement beyond what width=4 found. Ignored.
#[test]
#[ignore]
fn m4_4_print_spike_atom_fixture_metrics_beam_16() {
    let cfg_16 = M44SpikeConfig {
        strategy: M44Strategy::BeamSearch { beam_width: 16 },
    };
    for (name, atom) in fixtures_11_atoms() {
        let source = render_to_pcm(&atom);
        let opts = EncodeOptions {
            force_filter_0_first_block: atom.render.force_filter_0_first_block,
            loop_entry_block_index: Some(0),
        };
        let offsets = rotation_candidate_offsets(source.len());
        let mut candidates: Vec<RotationCandidate> = Vec::with_capacity(offsets.len());
        for offset in offsets {
            let rotated = rotate_pcm(&source, offset);
            let result = encode_looped_m4_4_spike(&rotated, 0, &opts, &cfg_16).expect("spike");
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
            let lc = loop_click_abs(&decoded, 0, decoded.len());
            let pae = peak_abs_raw_vs_source(&rotated, &decoded);
            let rmse = rms_raw_vs_source(&rotated, &decoded);
            candidates.push(RotationCandidate {
                offset: offset as u32,
                rotated,
                decoded,
                loop_click_abs: lc,
                peak: pae,
                rms: rmse,
            });
        }
        let best = candidates
            .into_iter()
            .min_by(|a, b| {
                a.loop_click_abs
                    .cmp(&b.loop_click_abs)
                    .then(a.peak.cmp(&b.peak))
                    .then(a.rms.total_cmp(&b.rms))
                    .then(a.offset.cmp(&b.offset))
            })
            .expect("at least one candidate");
        eprintln!(
            "M4_4_SPIKE_ATOM_BEAM16\t{name}\tpeak_abs={}\trms={}\tloop_click={}\trot_off={}",
            best.peak, best.rms, best.loop_click_abs, best.offset,
        );
    }
}

/// Print spike metrics for the 9 characterization signals.
/// Ignored (one-off capture).
#[test]
#[ignore]
fn m4_4_print_spike_characterization_signal_metrics() {
    for signal in m3_5_canonical_signals() {
        let r = run_spike_on_atom(&signal.atom);
        eprintln!(
            "M4_4_SPIKE_CHARSIG\t{}\tpeak_abs={}\trms={}\tsnr_db={}\tclip={}\tloop_click={}\trot_off={}",
            signal.name,
            r.peak_abs_raw_vs_source,
            r.rms_raw_vs_source,
            r.snr_db,
            r.clipping_count_raw,
            r.loop_click_abs,
            r.rotation_offset,
        );
    }
}

/// Compare M4.4 spike against M3.3 production loop_click_abs on
/// every atom fixture. Per SPEC §24.1 exit criterion #2, the spike
/// MUST NOT worsen loop_click_abs anywhere. This test asserts the
/// invariant in-process so a regression surfaces at test time.
#[test]
fn m4_4_spike_does_not_worsen_loop_click_vs_m3_3_production() {
    for (name, atom) in fixtures_11_atoms() {
        let prod: AtomBrrOutput = render_to_brr(&atom).expect("M3.3 production render");
        let spike = run_spike_on_atom(&atom);
        assert!(
            spike.loop_click_abs <= prod.loop_click_abs,
            "M4.4 spike worsens loop_click_abs for {name}: prod={}, spike={}",
            prod.loop_click_abs,
            spike.loop_click_abs
        );
    }
}
