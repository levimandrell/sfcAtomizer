# BRR fixture corpus

Frozen-literal expected_pcm for the M0.2 raw-decode test suite. Each
JSON file in this directory is one fixture: an `initial_history` seed,
one or more 9-byte BRR blocks, and the exact 16-bit PCM the raw
decoder must produce.

## Statement of intent

> `expected_pcm` arrays in this directory are frozen literals derived
> from independent ground truth — hand-computation against the
> fullsnes BRR section, cross-checked with the boldowa/snesbrr
> reference C++ decoder, and reproduced by the deliberately separate
> Python implementation in `_reference.py`.
>
> They are **not** output of `core::brr::decode_block`.
>
> If a fixture's `expected_pcm` changes, the change must include a
> justification in this README and a corresponding update to that
> fixture's `provenance_notes`.

`_reference.py` is a second-source check, not an authority. It is
small enough to read in five minutes and was written from the same
spec as the production decoder — but in a different language with
different idioms, so a copy-paste bug is unlikely to land in both.
When `_reference.py` and `core::brr` disagree, the resolution path
is to hand-walk the disagreement against fullsnes; whichever side
mis-implements the spec is wrong.

## Format

```jsonc
{
  "name": "fixture_name",
  "description": "what regression class this catches",
  "initial_history": { "prev1": 0, "prev2": 0 },
  "blocks_hex": ["HH HH HH HH HH HH HH HH HH", ...],
  "expected_pcm": [s0, s1, ..., sN],
  "provenance": "hand-walked" | "spec-derived",
  "provenance_notes": "math walked in enough detail to audit"
}
```

`blocks_hex` is one string per BRR block, exactly 9 hex byte tokens
each, separated by single spaces (parser tolerates other whitespace
runs). `expected_pcm` length is `blocks.len() * 16` for non-loop
fixtures, and 16 for the loop-entry fixture (which decodes only the
loop-entry block).

## Catalogue

| Name                                            | Provenance    | Regression class                                                                                  |
|-------------------------------------------------|---------------|---------------------------------------------------------------------------------------------------|
| `filter0_basic`                                 | hand-walked   | filter 0, sign extension, `>>1` post-shift step                                                   |
| `filter0_shift_clamp`                           | hand-walked   | shift=12 (max normal) and shift=13 (special: neg→-2048, non-neg→0)                                |
| `filter1_zero_history`                          | hand-walked   | filter 1 from {0, 0} — pure decay shape                                                           |
| `filter1_nonzero_history`                       | hand-walked   | filter 1 carrying a seeded prev1/prev2                                                            |
| `filter2_nonzero_history`                       | hand-walked   | filter 2 = `prev1*61/32 - prev2*15/16` (most arithmetically delicate filter)                      |
| `filter3_nonzero_history`                       | hand-walked   | filter 3 = `prev1*115/64 - prev2*13/16`                                                           |
| `multi_block_predictor_history`                 | spec-derived  | cross-block predictor continuity — block N's last 2 outputs become block N+1's seeds              |
| `loop_boundary_history`                         | spec-derived  | SPEC §10.2 loop-state continuity at the loop entry block                                          |
| `flags_end_loop_ignored_by_raw_decode`          | hand-walked   | END/LOOP header bits do **not** affect raw decode (S-DSP voice-loop concern only)                 |

Hand-walked fixtures show the math for the first 2–3 samples in their
`provenance_notes` so a reviewer can audit without re-deriving the
formulas. Spec-derived fixtures cite `_reference.py` for the full
array but include sample-0 and at least one transition (cross-block
or loop-entry) hand-checked.

## Reference implementation

`_reference.py` implements the same filter formulas as the production
Rust decoder, written from the canonical sources:

- nocash fullsnes (BRR Samples / BRR Pitch sections)
- boldowa/snesbrr reference C++ decoder
- SNESdev wiki and SnesLab cross-references

Run it any time to regenerate the expected_pcm arrays for inspection:

```sh
py core/fixtures/brr/_reference.py
```

The script is read-only: it prints to stdout, never mutates the JSON
files. To intentionally update a fixture, edit its JSON file by
hand, update its `provenance_notes` to describe why the value
changed, and re-run `cargo test -p sfc-atomizer-core` to confirm the
production decoder still agrees.

## Caveats and resolved ambiguities

- **Header bit 0 vs bit 1 for END/LOOP.** SnesLab's wiki page documents
  `end = bit 1, loop = bit 0`; the SNESdev wiki and most modern
  emulator code use `end = bit 0, loop = bit 1`. We implement the
  SNESdev wiki convention (also matches boldowa/snesbrr). Locked by
  `flags_end_loop_ignored_by_raw_decode` and the `header_parse_layout`
  unit test in `core::brr`.

- **Shift > 12 path.** Documented identically by both fullsnes
  (paraphrased: clear bottom 11 bits of the sign-extended nibble) and
  SnesLab (paraphrased: `(nibble >> 3) << 11`). Both produce the same
  values: 0 for non-negative nibbles, −2048 for negative. Locked by
  `filter0_shift_clamp`.

- **15-bit wrap vs clamp.** Filters 0 and 1 do not clamp to 16-bit
  before the final 15-bit wrap; filters 2 and 3 do. This matches the
  boldowa/snesbrr code (`clamp<16>(s)` is called only in filter 2/3).
  We have not yet built a fixture whose intermediate would actually
  exceed 16-bit on filters 0 or 1 — possible future addition if a
  pathological encoder choice surfaces in M3.

- **No regression-baseline fixtures.** Per the M0.2 brief, every
  fixture in this corpus has independent ground truth. If we ever
  add an "audit-only, decoder-output-frozen" fixture for performance
  regression testing, it must live in a separate directory and be
  excluded from the M0 acceptance gate.
