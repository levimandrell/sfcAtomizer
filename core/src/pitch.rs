//! S-DSP voice pitch register (SPEC §16.7).
//!
//! Pure math, no I/O, no state. The driver and the M1 compiler both
//! call [`pitch_register`] to encode a pitch as the 14-bit
//! `VxPITCHL`/`VxPITCHH` value the S-DSP wants.
//!
//! Formula (from SPEC §16.7):
//!
//! ```text
//! pitch_float =
//!   4096.0
//!   * (source_sample_rate_hz / 32000.0)
//!   * 2^((desired_midi_note - root_midi_note + cents_offset / 100.0) / 12.0)
//!
//! pitch_u16 = clamp(round_half_up(pitch_float), 0x0000, 0x3FFF)
//! ```
//!
//! Rounding: `round_half_up(x) = floor(x + 0.5)`.
//!
//! Reference values from SPEC §16.7:
//! - 32 kHz at root → `0x1000`.
//! - 22050 Hz at root → `0x0B06` (= `round_half_up(4096 * 22050 / 32000) = 2822`).

/// Maximum value of the 14-bit pitch register.
pub const PITCH_MAX: u16 = 0x3FFF;

/// Encode a desired playback pitch as the 14-bit `VxPITCHL`+`VxPITCHH`
/// value used by the S-DSP.
///
/// `cents_offset` is signed 100ths of a semitone (positive sharper,
/// negative flatter). Out-of-range results clamp to the documented
/// 14-bit range; the caller is responsible for upstream warnings if
/// clamping happens during compile.
pub fn pitch_register(
    source_sample_rate_hz: u32,
    root_midi_note: u8,
    desired_midi_note: u8,
    cents_offset: i32,
) -> u16 {
    let semitones =
        (desired_midi_note as f64) - (root_midi_note as f64) + (cents_offset as f64) / 100.0;
    let rate_ratio = (source_sample_rate_hz as f64) / 32000.0;
    let pitch_float = 4096.0 * rate_ratio * (semitones / 12.0).exp2();
    let rounded = (pitch_float + 0.5).floor();
    if rounded.is_nan() || rounded < 0.0 {
        return 0;
    }
    if rounded > PITCH_MAX as f64 {
        return PITCH_MAX;
    }
    rounded as u16
}

/// Split the 14-bit pitch register into its two on-bus bytes:
/// `(VxPITCHL, VxPITCHH)`. `VxPITCHH`'s top 2 bits are masked off
/// (the S-DSP reads only the low 6 bits there).
pub const fn split_pitch(pitch_u16: u16) -> (u8, u8) {
    let lo = (pitch_u16 & 0xFF) as u8;
    let hi = ((pitch_u16 >> 8) & 0x3F) as u8;
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_at_32khz_is_0x1000() {
        // SPEC §16.7 reference: 32 kHz at root → $1000.
        assert_eq!(pitch_register(32_000, 60, 60, 0), 0x1000);
    }

    #[test]
    fn at_root_22050_is_0x0b06() {
        // SPEC §16.7 reference: 22050 Hz at root → $0B06 (= 2822).
        assert_eq!(pitch_register(22_050, 60, 60, 0), 0x0B06);
    }

    #[test]
    fn one_octave_up_is_0x2000() {
        assert_eq!(pitch_register(32_000, 60, 72, 0), 0x2000);
    }

    #[test]
    fn one_octave_down_is_0x0800() {
        assert_eq!(pitch_register(32_000, 60, 48, 0), 0x0800);
    }

    #[test]
    fn extreme_transposition_clamps_to_max() {
        // Root = 0, desired = 127 → ~10.5 octaves up. Way above 14-bit.
        assert_eq!(pitch_register(32_000, 0, 127, 0), PITCH_MAX);
        // Plus a positive cents offset for good measure.
        assert_eq!(pitch_register(32_000, 0, 127, 50), PITCH_MAX);
    }

    #[test]
    fn one_hundred_cents_equals_one_semitone() {
        // 100-cent shift on root key must equal a 1-semitone shift.
        let with_cents = pitch_register(32_000, 60, 60, 100);
        let with_semitone = pitch_register(32_000, 60, 61, 0);
        assert_eq!(with_cents, with_semitone);
    }

    #[test]
    fn negative_cents_equals_semitone_down() {
        // -100 cents on root key must equal one semitone down.
        let with_cents = pitch_register(32_000, 60, 60, -100);
        let with_semitone = pitch_register(32_000, 60, 59, 0);
        assert_eq!(with_cents, with_semitone);
    }

    #[test]
    fn split_pitch_at_0x1000() {
        assert_eq!(split_pitch(0x1000), (0x00, 0x10));
    }

    #[test]
    fn split_pitch_at_0x3fff() {
        assert_eq!(split_pitch(0x3FFF), (0xFF, 0x3F));
    }

    #[test]
    fn split_pitch_masks_top_two_bits_of_high_byte() {
        // Inputs above $3FFF aren't expected from pitch_register, but
        // split_pitch must still mask defensively.
        let (lo, hi) = split_pitch(0xFFFF);
        assert_eq!(lo, 0xFF);
        assert_eq!(hi, 0x3F);
    }
}
