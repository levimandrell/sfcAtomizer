# M2.0-postlude WAV renders (temporary)

Three 5-second mono PCM16 WAVs at 32 kHz, rendered through the
snes_spc oracle from the audition `.spc` / `.sfc` artefacts in
`build/audition/`. The WAVs let PM verify M2.0 audibly via
GitHub Raw URLs without needing local Mesen2 access.

**This directory is temporary.** It is removed in the next pass
once PM has confirmed each render audibly. Source-controlled
audition stays in SPEC / STATUS / `m1-acceptance` reports; raw
audio dumps are not part of the long-term repo.

## Files

| File | Path | Contents |
|---|---|---|
| 1 | `audition/m2.0-postlude/canonical_sine.wav` | rendered from `build/audition/canonical_sine.spc` |
| 2 | `audition/m2.0-postlude/swap_module_a.wav` | module A from `build/audition/swap_220_to_440.sfc` |
| 3 | `audition/m2.0-postlude/swap_module_b.wav` | module B from `build/audition/swap_220_to_440.sfc` |

All three: 320,044 bytes, 5.000 s, mono, 32 kHz, signed 16-bit
LE PCM, 44-byte standard RIFF/WAVE header.

## What you should hear

### `canonical_sine.wav` — 500 Hz continuous sine

A single sustained 500.00 Hz tone for the entire 5 seconds.
The fixture project has loop enabled covering the full
8192-sample sine cycle (period 64 samples), so the SNES voice
loops indefinitely. No clicks, no envelope decay, no silence
gaps.

This is the audible-baseline render: confirms the M1 driver +
oracle path produces clean sustained playback.

### `swap_module_a.wav` — 219 Hz continuous sine

Continuous tone at **218.99 Hz** (period 146 samples at 32 kHz
synth source rate; near A3 minus ~9 cents because integer
period is required for bit-exact cycle).

This is what voice 0 will play during the first half of the
`.sfc` audition — before the loader's `RESET_TO_IPL` and the
upload of module B.

### `swap_module_b.wav` — 438 Hz continuous sine

Continuous tone at **438.36 Hz** (period 73 samples; near A4
minus ~9 cents). The 2:1 period ratio with module A produces
an exact one-octave jump.

This is what voice 0 plays after the `RESET_TO_IPL` swap.
Listening A → B should sound like an octave step up; listening
B → A like an octave step down.

## Engineer-side spot check (already verified before commit)

```
canonical_sine.wav  peak=11072  rms=7819.8  freq=500.00 Hz   silent_windows_1s=0
swap_module_a.wav   peak=11086  rms=7824.6  freq=218.99 Hz   silent_windows_1s=0
swap_module_b.wav   peak=11082  rms=7821.4  freq=438.36 Hz   silent_windows_1s=0
```

RMS ≈ 0.707 × peak as expected for a pure sine; the tiny
deviation from a perfect 500/220/440 Hz target reflects BRR
quantization (the synthesized cycle is bit-exact at integer
period only).

## Pitch derivation (SPEC §16.7)

Source samples: 32 kHz, root MIDI 60. Played at root key with
no transposition, the SNES pitch register is `$1000` (the
`pitch_float = 4096 × (source_sr / 32000)` formula collapses
to 4096), so the S-DSP plays each sample at its native rate.
The audible fundamental therefore equals `source_sample_rate
/ cycle_len_in_samples`:

- Canonical: 32000 / 64 = 500.00 Hz
- Swap A:    32000 / 146 = 219.18 Hz
- Swap B:    32000 / 73 = 438.36 Hz

The swap A : swap B period ratio is exactly 2:1 (146 = 73 × 2),
so the octave relationship is bit-exact.
