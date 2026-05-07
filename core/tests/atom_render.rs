//! Integration tests for the M2.2 atom render → BRR encode pipeline.

use sfc_atomizer_core::atom::{render_to_brr, AtomKind, AtomPartial, AtomRenderOptions, AtomSlot};
use sfc_atomizer_core::project::{Envelope, SamplePlayback};

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
fn render_canonical_128_sine_atom_deterministic() {
    let atom = canonical_sine_atom(128);
    let a = render_to_brr(&atom).expect("render");
    let b = render_to_brr(&atom).expect("render");
    assert_eq!(a.brr_bytes, b.brr_bytes);
    assert_eq!(a.pcm, b.pcm);
    assert_eq!(a.brr_sha256, b.brr_sha256);
    assert_eq!(a.pcm_sha256, b.pcm_sha256);
}

#[test]
fn render_canonical_64_sine_atom_deterministic() {
    let atom = canonical_sine_atom(64);
    let a = render_to_brr(&atom).expect("render");
    let b = render_to_brr(&atom).expect("render");
    assert_eq!(a.brr_bytes, b.brr_bytes);
    assert_eq!(a.brr_sha256, b.brr_sha256);
}

#[test]
fn render_64_vs_128_atom_distinct() {
    let a = render_to_brr(&canonical_sine_atom(64)).expect("render");
    let b = render_to_brr(&canonical_sine_atom(128)).expect("render");
    assert_ne!(
        a.brr_sha256, b.brr_sha256,
        "64- and 128-sample atoms must produce distinct BRR SHAs"
    );
    assert_ne!(a.brr_bytes, b.brr_bytes);
    assert_ne!(a.pcm.len(), b.pcm.len());
    assert_eq!(a.brr_bytes.len(), 36); // 64 / 16 * 9
    assert_eq!(b.brr_bytes.len(), 72); // 128 / 16 * 9
}

#[test]
fn render_canonical_atoms_match_locked_sha_baselines() {
    // Mirrors the in-module `m2_atom_render_baselines_locked` test
    // but at integration-test scope so a `cargo test` against a
    // clone of the repo (without recompiling internal-tests-only
    // targets) still pins the baselines.
    let out_128 = render_to_brr(&canonical_sine_atom(128)).expect("render");
    assert_eq!(
        out_128.brr_sha256, "348c791449916e1f9169d0e229cd79bf97967b19e22db3c4a5be7dc9c69ac876",
        "M2_ATOM_128_SINE_BRR_SHA256 drift"
    );

    let out_64 = render_to_brr(&canonical_sine_atom(64)).expect("render");
    assert_eq!(
        out_64.brr_sha256, "78da253b65a6a8d067102fe30ed90353c25b6981a71e3cafc6dd4f3041822e96",
        "M2_ATOM_64_SINE_BRR_SHA256 drift"
    );
}
