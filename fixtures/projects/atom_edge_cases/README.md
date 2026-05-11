# Atom edge-case fixtures

Reproducible project files for the M3.2 atom edge-case fixture
set (see `core/tests/atom_edge_cases.rs`). M3.3 commits one of
the nine fixtures as an on-disk `.sfcproj.json` per consultant M3
plan #12; the remaining eight stay synthesized in tests only.

## Why `harmonic_16_cycle_64`

This near-Nyquist atom (harmonic 16 over a 64-sample cycle = a
quarter of the sample rate) is the most interesting fixture for
the remaining M3 sub-passes:

- **M3.3 phase rotation (this pass).** Rotation correctly
  defaults to `offset=0` here — every block-aligned candidate
  produces the same large `loop_click_abs` (16384) because the
  signal's period-4 structure makes the seam discontinuity
  insensitive to block-aligned rotation. The lex tie-breaker
  picks the smallest offset. Documents the "rotation finds no
  improvement" boundary case.
- **M3.4 predictor optimization (conditional).** High-frequency
  content is where the M2.2 greedy per-block predictor
  degenerates most aggressively. If M3.4 ships, this fixture is
  one of the most likely to show measurable cross-block beam
  search gains.
- **M3.6 pre-emphasis (conditional).** S-DSP Gaussian
  interpolation dulls quarter-rate content the hardest; if
  pre-emphasis presets ship, this fixture is where they will
  diverge most.

## Reproduction

From the repository root:

```bash
cargo run --release --bin sfcwc -- render-atom \
    --project fixtures/projects/atom_edge_cases/harmonic_16_cycle_64.sfcproj.json \
    --atom harmonic_16_cycle_64 \
    --out-report build/m3/atom_edge_cases/harmonic_16_cycle_64.atom-render-report.json
```

The `AtomRenderReport` JSON output is byte-deterministic and its
fields should match the M3.3-locked baseline values:

| Field | Value | Source |
|---|---|---|
| `pcm_sha256` | `071ae38dc919a75db8be191c0a11970934e525481df81e1515b14fe00cb46de1` | `baselines/m3.json::identity_gated::M3_ATOM_HARMONIC_16_CYCLE_64_PCM_SHA256` |
| `brr_sha256` | `779e9be8b35891ba18acbe74d3ac3643a5c06109a1a270844b3c002f6e7b3c06` | `…documentary_snapshot::M3_ATOM_HARMONIC_16_CYCLE_64_BRR_SHA256_PHASE_ROTATION` |
| `decoded_brr_pcm_sha256` | `87ffee5e6cd3fedb6474f7d582542341e62a2cdae1aca8c7b7cf46cab95acbf8` | `…_DECODED_BRR_PCM_SHA256_PHASE_ROTATION` |
| `rotation_offset` | `0` | `…_ROTATION_OFFSET_PHASE_ROTATION` |
| `loop_click_abs` | `16384` | `…_LOOP_CLICK_ABS_PHASE_ROTATION` (equal to `…_PRE_M3` — rotation found no improvement) |
| `peak_abs_error_post_rotation` | `18431` | `…_PEAK_ABS_ERROR_PHASE_ROTATION` |
| `rms_error_post_rotation` | `12329.88696217447` | `…_RMS_ERROR_PHASE_ROTATION` |
| `loop_window_rms_delta` | `0.0` | `…_LOOP_WINDOW_RMS_DELTA_PHASE_ROTATION` |

The PCM SHA is identity-gated (SPEC §16.9 amendment); BRR /
decoded-BRR / metric values are documentary and expected to shift
at M3.4+ if those sub-passes ship encoder changes.

## Schema

The fixture relies on M2.5 SPEC §16.6 empty-`sample_pool`
relaxation: an atoms-only project with no `sample_sustain` track,
one `atom_sequence` track on voice 0 (validation rule 50 / 54
permit voice 0 for atom_sequence — voice 0 is only conventionally
sample). The atom uses `partial.amplitude = 1.0`,
`atom.amplitude = 1.0` (full-scale), and `normalize = true`.
