# Canonical M2 fixture

Committed at M2.8.1 (consultant M2 close-out #3) so the
[`docs/reproduce-m2.md`](../../../docs/reproduce-m2.md) walkthrough
has a concrete file to point at without "synthesize this in your
test harness" hand-waving.

## Contents

- `canonical_m2.sfcproj.json` — v2 multi_voice_atom project,
  identical in shape to the `canonical_project()` helper in
  `core/tests/sequence_compile.rs`. Sample (`lead`) panned LEFT;
  two atoms (`atom_a` = sine_128, `atom_b` = sine_64) panned
  RIGHT; 2-step atom_sequence (`initial_kon` + `fade_to_zero_retrigger`).
- `audio/lead.wav` — deterministic 32 kHz mono PCM16 WAV, 8192
  frames, `8000 * sin(2π n / 64)`. SHA-256
  `b42397b85a8788c2563fd0f2c35dc345b9957130d543fc7e82aa846387ce5b61`.
  Generated reproducibly by the synthesis recipe in this README.

## Locked SHAs (canonical bytecode + voice setup table)

```
M2_CANONICAL_SEQUENCE_BYTECODE_SHA256 = f9fa6ea85a7197b662a3b386c9606bb25954708b9714629de7069c90f0fd24f0
M2_CANONICAL_VOICE_SETUP_TABLE_SHA256 = f2faaed8530fb82933e3c30b7537190ba150c38e00b82eee83517ac818089ad5
```

These are the identity-gated values from `baselines/m2.json`.
Any drift on a re-compile of this fixture is a regression.
Hex dumps + per-byte breakdowns live in
[`baselines/m2_canonical_fixtures.md`](../../../baselines/m2_canonical_fixtures.md).

## Reproducing `audio/lead.wav`

```python
import struct, math
pcm = bytearray()
for n in range(8192):
    v = int(8000 * math.sin(2*math.pi*n/64))
    pcm += struct.pack('<h', v)
data = bytearray()
data += b'RIFF' + struct.pack('<I', 36 + len(pcm))
data += b'WAVE' + b'fmt '
data += struct.pack('<IHHIIHH', 16, 1, 1, 32000, 32000*2, 2, 16)
data += b'data' + struct.pack('<I', len(pcm)) + pcm
open('audio/lead.wav', 'wb').write(data)
```

The same recipe lives inside the `app/tests/cli.rs::synth_sine_pcm`
+ `write_pcm16_wav_with_samples` helpers used by every M2 oracle
test.

## Use

From the repository root:

```bash
cargo run --release --bin sfcwc -- m2-acceptance \
    --project-a fixtures/projects/canonical_m2/canonical_m2.sfcproj.json \
    --out build/m2/acceptance/canonical \
    --frames 160000
```

Expected: `bundle.status: ok`, all four stages green.
