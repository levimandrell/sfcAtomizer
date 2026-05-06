"""Independent reference BRR decoder for fixture ground truth.

This is NOT the production decoder. It is a deliberate second
implementation written directly from the canonical formulas to serve
as ground truth for every expected_pcm array under
core/fixtures/brr/. If this script and core::brr::decode_block ever
disagree, the resolution path is:

1. Hand-walk the disagreement against fullsnes (nocash).
2. Whichever side mis-implements the spec is wrong.
3. Update the wrong side. expected_pcm is only updated when this
   script's output agrees with hand-walked spec math.

Reference: nocash fullsnes BRR section + boldowa/snesbrr decoder.
Header layout: SSSS FFLE (loop bit 1, end bit 0).

Run:
    python3 core/fixtures/brr/_reference.py

Prints expected_pcm for each fixture so they can be eyeballed against
the JSON files.
"""

from __future__ import annotations


def parse_header(b: int) -> tuple[int, int, bool, bool]:
    shift = (b >> 4) & 0x0F
    flt = (b >> 2) & 0x03
    loop_flag = bool(b & 0x02)
    end = bool(b & 0x01)
    return shift, flt, end, loop_flag


def sign_ext_4(n: int) -> int:
    n &= 0x0F
    return (n ^ 8) - 8


def arith_rshift(value: int, n: int) -> int:
    """Floor-divide-by-2^n, matching i32 arithmetic right shift."""
    if value >= 0:
        return value >> n
    return -((-value + (1 << n) - 1) >> n)


def clamp_i16(s: int) -> int:
    if s > 32767:
        return 32767
    if s < -32768:
        return -32768
    return s


def wrap_15bit(s: int) -> int:
    """Reproduces `int16(s << 1) >> 1` from boldowa/snesbrr."""
    s2 = (s << 1) & 0xFFFFFFFF
    s2 &= 0xFFFF
    if s2 & 0x8000:
        s2 -= 0x10000
    return arith_rshift(s2, 1)


def decode_block(block: bytes, prev1: int, prev2: int) -> tuple[list[int], int, int]:
    assert len(block) == 9
    shift, flt, _, _ = parse_header(block[0])
    out = []
    for i in range(16):
        byte = block[1 + i // 2]
        nib = (byte >> 4) if (i & 1) == 0 else (byte & 0x0F)
        n = sign_ext_4(nib)

        if shift > 12:
            shifted = n & ~0x07FF
        else:
            shifted = arith_rshift(n << shift, 1)

        s = shifted
        if flt == 0:
            pass
        elif flt == 1:
            s += prev1
            s += arith_rshift(-prev1, 4)
        elif flt == 2:
            s += prev1 << 1
            s += arith_rshift(-(prev1 + (prev1 << 1)), 5)
            s += -prev2
            s += arith_rshift(prev2, 4)
            s = clamp_i16(s)
        elif flt == 3:
            s += prev1 << 1
            s += arith_rshift(-(prev1 + (prev1 << 2) + (prev1 << 3)), 6)
            s += -prev2
            s += arith_rshift(prev2 + (prev2 << 1), 4)
            s = clamp_i16(s)
        else:
            raise ValueError(flt)

        wrapped = wrap_15bit(s)
        out.append(wrapped)
        prev2 = prev1
        prev1 = wrapped
    return out, prev1, prev2


def decode_blocks(blocks: list[bytes], prev1: int, prev2: int) -> list[int]:
    samples = []
    for blk in blocks:
        block_samples, prev1, prev2 = decode_block(blk, prev1, prev2)
        samples.extend(block_samples)
    return samples


def parse_hex_block(s: str) -> bytes:
    tokens = s.split()
    assert len(tokens) == 9, f"expected 9 hex bytes, got {len(tokens)}: {s}"
    return bytes(int(t, 16) for t in tokens)


# =============================================================================
# Fixtures — each entry yields one JSON file's expected_pcm.
# =============================================================================

FIXTURES = [
    {
        "name": "filter0_basic",
        "blocks_hex": ["00 12 34 56 78 9A BC DE F0"],
        "history": (0, 0),
    },
    {
        "name": "filter0_shift_clamp",
        # Two blocks: shift=12 normal, shift=13 special.
        "blocks_hex": [
            "C0 12 34 56 78 9A BC DE F0",
            "D0 12 34 56 78 9A BC DE F0",
        ],
        "history": (0, 0),
    },
    {
        "name": "filter1_zero_history",
        # filter=1, shift=4. Header: 0x40 | 0x04 = 0x44.
        "blocks_hex": ["44 11 11 11 11 11 11 11 11"],
        "history": (0, 0),
    },
    {
        "name": "filter1_nonzero_history",
        # Header 0x44: shift=4, filter=1. Same shape as filter1_zero_history
        # but with nonzero predictor seed.
        "blocks_hex": ["44 11 11 11 11 11 11 11 11"],
        "history": (1024, -512),  # prev1=1024, prev2=-512
    },
    {
        "name": "filter2_nonzero_history",
        # filter=2 → header bits 3-2 = 0b10 = 0x08. shift=2 → 0x20. Total 0x28.
        "blocks_hex": ["28 12 34 56 78 9A BC DE F0"],
        "history": (512, -1024),
    },
    {
        "name": "filter3_nonzero_history",
        # filter=3 → 0x0C. shift=2 → 0x20. Total 0x2C.
        "blocks_hex": ["2C 12 34 56 78 9A BC DE F0"],
        "history": (512, -1024),
    },
    {
        "name": "multi_block_predictor_history",
        # Three filter-1 blocks, shift=4 each. Predictor must carry.
        "blocks_hex": [
            "44 12 34 56 78 9A BC DE F0",
            "44 11 22 33 44 55 66 77 88",
            "44 FF EE DD CC BB AA 99 88",
        ],
        "history": (0, 0),
    },
    {
        "name": "loop_boundary_history",
        # Four blocks, filter=2, shift=2 throughout. Then re-decode with
        # the post-tail predictor state seeded as the loop entry.
        "blocks_hex": [
            "28 12 34 56 78 9A BC DE F0",
            "28 11 22 33 44 55 66 77 88",
            "28 7F 6E 5D 4C 3B 2A 19 08",
            "28 88 77 66 55 44 33 22 11",
        ],
        "history": (0, 0),
        "loop_iteration_2": True,
    },
    {
        "name": "flags_end_loop_ignored_by_raw_decode",
        # Same data as filter0_basic, but END | LOOP set in header.
        "blocks_hex": ["03 12 34 56 78 9A BC DE F0"],
        "history": (0, 0),
    },
]


def main() -> None:
    for fx in FIXTURES:
        blocks = [parse_hex_block(s) for s in fx["blocks_hex"]]
        p1, p2 = fx["history"]
        if fx.get("loop_iteration_2"):
            # First iteration: walk all blocks, capture final predictor state.
            cur1, cur2 = p1, p2
            for blk in blocks:
                _, cur1, cur2 = decode_block(blk, cur1, cur2)
            # The fixture file commits to a self-contained shape:
            #   initial_history = post-iteration-1 state
            #   blocks_hex = [block 0]  (just the loop entry block)
            #   expected_pcm = iteration-2's first 16 samples.
            samples, _, _ = decode_block(blocks[0], cur1, cur2)
            print(f"{fx['name']}: (LOOP ENTRY)")
            print(f"  loop entry history = (prev1={cur1}, prev2={cur2})")
            print(f"  block = {fx['blocks_hex'][0]}")
            print(f"  expected_pcm ({len(samples)} samples):")
            for i in range(0, len(samples), 16):
                chunk = ", ".join(str(s) for s in samples[i : i + 16])
                print(f"    {chunk}")
            print()
            continue
        samples = decode_blocks(blocks, p1, p2)
        print(f"{fx['name']}:")
        print(f"  history = {fx['history']}")
        print(f"  blocks = {fx['blocks_hex']}")
        print(f"  expected_pcm ({len(samples)} samples):")
        # Pretty-print 16 per line.
        for i in range(0, len(samples), 16):
            chunk = ", ".join(str(s) for s in samples[i : i + 16])
            print(f"    {chunk}")
        print()


if __name__ == "__main__":
    main()
