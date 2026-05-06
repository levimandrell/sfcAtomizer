//! Raw S-DSP BRR block decoder.
//!
//! BRR (Bit Rate Reduction) is the SNES sample format: 16 decoded samples
//! per 9-byte block. Layout:
//!
//! ```text
//! offset 0: header   SSSS FFLE
//!                    │    │ ││
//!                    │    │ │└─ end flag  (bit 0)
//!                    │    │ └── loop flag (bit 1)
//!                    │    └──── filter    (bits 3-2, values 0..=3)
//!                    └───────── shift     (bits 7-4, values 0..=15)
//!
//! offsets 1..=8: 8 data bytes, each holding two 4-bit signed nibbles.
//!                High nibble of byte N is the earlier sample.
//! ```
//!
//! Per-sample pipeline:
//!
//! 1. Sign-extend the 4-bit nibble to `-8..=+7` in `i32` via the
//!    `((n & 0x0F) ^ 8) - 8` trick.
//! 2. Apply shift. Normal range (`shift <= 12`):
//!    `(nibble << shift) >> 1` in `i32`. Out-of-range (`shift >= 13`):
//!    `nibble & !0x07FF`, which yields `-2048` for negative nibbles
//!    and `0` for non-negative — the documented hardware behavior.
//! 3. Add filter prediction terms (filter 0 = none; 1 = `prev1*15/16`;
//!    2 = `prev1*61/32 - prev2*15/16`; 3 = `prev1*115/64 - prev2*13/16`).
//!    The "fraction" forms are computed via integer add + arithmetic
//!    right-shift so rounding matches hardware.
//! 4. (Filters 2, 3 only) Clamp the prediction sum to signed 16-bit.
//! 5. Wrap to 15-bit: `(int16(s << 1)) >> 1`. Output range is
//!    `-16384..=16383`.
//! 6. Update predictor: `prev2 ← prev1; prev1 ← output`.
//!
//! Header `END` and `LOOP` flags are reported via [`BrrHeader`] but
//! intentionally do **not** affect raw decode — they are S-DSP
//! voice-loop control flow concerns, not raw-decoder concerns.
//!
//! Reference: nocash fullsnes BRR section, cross-checked against
//! the boldowa/snesbrr reference C++ decoder and the SNESdev /
//! sneslab community wikis. Where sources disagreed, fullsnes /
//! boldowa-snesbrr won.

/// Parsed view of the 1-byte BRR block header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrrHeader {
    /// Shift / range, 0..=15. Values 13..=15 trigger the special
    /// "negative nibbles → -2048, non-negative → 0" path.
    pub shift: u8,
    /// Filter selector, 0..=3.
    pub filter: u8,
    /// END flag (header bit 0).
    pub end: bool,
    /// LOOP flag (header bit 1).
    pub loop_flag: bool,
}

impl BrrHeader {
    /// Parse a header byte. Always succeeds (every 256-bit pattern
    /// decodes to a valid `BrrHeader`).
    pub fn parse(byte: u8) -> Self {
        Self {
            shift: byte >> 4,
            filter: (byte >> 2) & 0b11,
            end: (byte & 0b01) != 0,
            loop_flag: (byte & 0b10) != 0,
        }
    }
}

/// Predictor history carried between BRR blocks. `prev1` is the most
/// recent decoded output sample; `prev2` is the one before it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BrrDecoderState {
    pub prev1: i16,
    pub prev2: i16,
}

/// Decode one 9-byte BRR block. Updates `state` in place so the next
/// `decode_block` call continues with the correct predictor history.
///
/// Output samples are 15-bit signed (`-16384..=16383`) stored in `i16`.
pub fn decode_block(block: &[u8; 9], state: &mut BrrDecoderState) -> [i16; 16] {
    let header = BrrHeader::parse(block[0]);
    let mut p1 = state.prev1 as i32;
    let mut p2 = state.prev2 as i32;
    let mut out = [0i16; 16];

    for i in 0..16usize {
        let byte = block[1 + i / 2];
        let raw_nibble = if i & 1 == 0 { byte >> 4 } else { byte & 0x0F };

        // Sign-extend 4-bit nibble to i32 in the range -8..=+7.
        let n = (((raw_nibble as i32) & 0x0F) ^ 8) - 8;

        // Shift / range stage.
        let shifted = if header.shift > 12 {
            n & !0x07FFi32
        } else {
            (n << header.shift) >> 1
        };

        // Filter stage.
        let mut s = shifted;
        match header.filter {
            0 => {}
            1 => {
                s += p1;
                s += -p1 >> 4;
            }
            2 => {
                s += p1 << 1;
                s += -(p1 + (p1 << 1)) >> 5;
                s += -p2;
                s += p2 >> 4;
                s = s.clamp(i16::MIN as i32, i16::MAX as i32);
            }
            3 => {
                s += p1 << 1;
                s += -(p1 + (p1 << 2) + (p1 << 3)) >> 6;
                s += -p2;
                s += (p2 + (p2 << 1)) >> 4;
                s = s.clamp(i16::MIN as i32, i16::MAX as i32);
            }
            _ => unreachable!("filter masked to 2 bits"),
        }

        // Wrap to 15-bit signed: int16(s << 1) >> 1.
        let wrapped = (s.wrapping_shl(1) as i16) >> 1;
        out[i] = wrapped;

        // Predictor history update: prev2 ← prev1, prev1 ← output.
        p2 = p1;
        p1 = wrapped as i32;
    }

    state.prev1 = p1 as i16;
    state.prev2 = p2 as i16;
    out
}

/// Decode a contiguous run of BRR blocks, preserving predictor state
/// across boundaries. Output length is `blocks.len() * 16`.
pub fn decode_blocks(blocks: &[[u8; 9]], state: &mut BrrDecoderState) -> Vec<i16> {
    let mut out = Vec::with_capacity(blocks.len() * 16);
    for block in blocks {
        out.extend_from_slice(&decode_block(block, state));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parse_layout() {
        // SSSS FFLE
        // shift=0xC, filter=0b10=2, loop=1, end=0  →  0xC0 | 0x08 | 0x02 = 0xCA
        let h = BrrHeader::parse(0xCA);
        assert_eq!(h.shift, 0xC);
        assert_eq!(h.filter, 2);
        assert!(h.loop_flag);
        assert!(!h.end);

        // shift=0, filter=0, loop=0, end=1  →  0x01
        let h = BrrHeader::parse(0x01);
        assert_eq!(h.shift, 0);
        assert_eq!(h.filter, 0);
        assert!(!h.loop_flag);
        assert!(h.end);

        // All zeros.
        let h = BrrHeader::parse(0x00);
        assert_eq!(
            h,
            BrrHeader {
                shift: 0,
                filter: 0,
                end: false,
                loop_flag: false
            }
        );

        // shift=0xF, filter=0b11=3, loop=1, end=1  →  0xFF
        let h = BrrHeader::parse(0xFF);
        assert_eq!(h.shift, 0xF);
        assert_eq!(h.filter, 3);
        assert!(h.loop_flag);
        assert!(h.end);
    }

    #[test]
    fn filter0_zero_data_is_zero() {
        // Filter 0, shift 0, all-zero data bytes ⇒ 16 zero samples.
        let block = [0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut state = BrrDecoderState::default();
        let out = decode_block(&block, &mut state);
        assert_eq!(out, [0i16; 16]);
        assert_eq!(state, BrrDecoderState::default());
    }

    #[test]
    fn negative_nibble_sign_extends() {
        // Filter 0, shift 4, all-0x88 data bytes.
        // nibble 0x8 → sign-extended -8.
        // shifted: (-8 << 4) >> 1 = -128 >> 1 = -64.
        // No prediction; final wrap leaves -64. All 16 samples = -64.
        let block = [0x40, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88];
        let mut state = BrrDecoderState::default();
        let out = decode_block(&block, &mut state);
        assert_eq!(out, [-64i16; 16]);
    }

    #[test]
    fn positive_nibble_no_sign_extension() {
        // Filter 0, shift 4, all-0x77 data bytes.
        // nibble 0x7 → +7. shifted: (7 << 4) >> 1 = 112 >> 1 = 56.
        let block = [0x40, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77];
        let mut state = BrrDecoderState::default();
        let out = decode_block(&block, &mut state);
        assert_eq!(out, [56i16; 16]);
    }

    #[test]
    fn predictor_history_after_decode() {
        // Filter 0, shift 0, alternating nibble pattern 0x12 0x34 ...
        // Last data byte 0x78 → samples 14,15 = 7,8 sign-extended → 7, -8 ?
        // Wait, byte index 8 holds samples 14 (high=7) and 15 (low=8 → -8).
        // Each shifted: (n << 0) >> 1 = n >> 1.
        //   sample 14: 7 >> 1 = 3.
        //   sample 15: -8 >> 1 = -4.
        let block = [0x00, 0x12, 0x34, 0x56, 0x78, 0x12, 0x34, 0x56, 0x78];
        let mut state = BrrDecoderState::default();
        let out = decode_block(&block, &mut state);
        // After decode: prev1 = sample 15, prev2 = sample 14.
        assert_eq!(state.prev1, out[15]);
        assert_eq!(state.prev2, out[14]);
        assert_eq!(state.prev1, -4);
        assert_eq!(state.prev2, 3);
    }

    #[test]
    fn header_flags_do_not_affect_decode() {
        // Identical data bytes; only END/LOOP flag bits differ.
        // Output PCM must be byte-identical.
        let data = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let mut state_a = BrrDecoderState::default();
        let mut block_a = [0x00; 9];
        block_a[1..].copy_from_slice(&data);
        let out_a = decode_block(&block_a, &mut state_a);

        let mut state_b = BrrDecoderState::default();
        let mut block_b = [0x03; 9]; // END | LOOP set
        block_b[1..].copy_from_slice(&data);
        let out_b = decode_block(&block_b, &mut state_b);

        assert_eq!(out_a, out_b);
        assert_eq!(state_a, state_b);
    }

    #[test]
    fn shift_high_range_clamps_nibble_sign_only() {
        // Shift 13 (header 0xD0): negative nibbles → -2048, non-negative → 0.
        // Filter 0, no prediction, no further state.
        // Data byte 0x80 → high nibble 0x8 (= -8 signed) → -2048.
        //                  low  nibble 0x0 (= 0 signed)  → 0.
        let block = [0xD0, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80];
        let mut state = BrrDecoderState::default();
        let out = decode_block(&block, &mut state);
        // After 15-bit wrap: -2048 stays -2048, 0 stays 0.
        let expected: [i16; 16] = [
            -2048, 0, -2048, 0, -2048, 0, -2048, 0, -2048, 0, -2048, 0, -2048, 0, -2048, 0,
        ];
        assert_eq!(out, expected);
    }
}
