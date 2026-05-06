//! Audition WAV exporter — decodes BRR bytes and writes a 16-bit
//! mono PCM RIFF/WAVE for offline preview.
//!
//! The header is hand-rolled (no `hound` dependency) so output is
//! byte-stable: identical inputs always produce identical files.
//! Sample data is the raw 15-bit decoder output stored in `i16`
//! (range −16384..=16383); no gain compensation is applied so the
//! audition reflects what the S-DSP would emit at unity voice gain.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use thiserror::Error;

use crate::brr::{decode_block, BrrDecoderState};

#[derive(Debug, Error)]
pub enum AuditionError {
    #[error("BRR input length {0} is not a multiple of 9 bytes")]
    UnalignedBrrLength(usize),
    #[error("sample_rate_hz {0} is out of range (1..=192000)")]
    SampleRateOutOfRange(u32),
    #[error("io error writing audition WAV: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct AuditionReport {
    pub blocks_decoded: u32,
    pub samples_written: u32,
    pub bytes_written: u64,
}

/// Decode `brr_bytes` and emit a 16-bit mono PCM WAV at `out_path`.
/// Returns counts so callers (CLI / GUI) can report what was written.
pub fn export_decoded_brr_wav(
    brr_bytes: &[u8],
    sample_rate_hz: u32,
    out_path: &Path,
) -> Result<AuditionReport, AuditionError> {
    if !brr_bytes.len().is_multiple_of(9) {
        return Err(AuditionError::UnalignedBrrLength(brr_bytes.len()));
    }
    if sample_rate_hz == 0 || sample_rate_hz > 192_000 {
        return Err(AuditionError::SampleRateOutOfRange(sample_rate_hz));
    }

    let pcm = decode_brr_pcm(brr_bytes);
    write_pcm16_mono_wav(out_path, &pcm, sample_rate_hz)?;

    let bytes_written = 44 + pcm.len() as u64 * 2;
    Ok(AuditionReport {
        blocks_decoded: (brr_bytes.len() / 9) as u32,
        samples_written: pcm.len() as u32,
        bytes_written,
    })
}

fn decode_brr_pcm(brr_bytes: &[u8]) -> Vec<i16> {
    let mut state = BrrDecoderState::default();
    let mut pcm = Vec::with_capacity((brr_bytes.len() / 9) * 16);
    for chunk in brr_bytes.chunks_exact(9) {
        let mut block = [0u8; 9];
        block.copy_from_slice(chunk);
        let decoded = decode_block(&block, &mut state);
        pcm.extend_from_slice(&decoded);
    }
    pcm
}

/// Convert interleaved-stereo s16le bytes (the snes_spc oracle's
/// PCM output) to a mono 32 kHz PCM16 WAV. Each frame's L and R
/// samples are averaged with truncation toward zero — equal-pan
/// signals come out identical to either channel; panned signals
/// land at the obvious midpoint.
pub fn write_oracle_pcm_to_mono_wav(
    out_path: &Path,
    pcm_stereo_le: &[u8],
    sample_rate_hz: u32,
) -> Result<(), AuditionError> {
    if !pcm_stereo_le.len().is_multiple_of(4) {
        return Err(AuditionError::UnalignedBrrLength(pcm_stereo_le.len()));
    }
    let frames = pcm_stereo_le.len() / 4;
    let mut mono: Vec<i16> = Vec::with_capacity(frames);
    for f in 0..frames {
        let off = f * 4;
        let l = i16::from_le_bytes([pcm_stereo_le[off], pcm_stereo_le[off + 1]]) as i32;
        let r = i16::from_le_bytes([pcm_stereo_le[off + 2], pcm_stereo_le[off + 3]]) as i32;
        mono.push(((l + r) / 2) as i16);
    }
    write_pcm16_mono_wav(out_path, &mono, sample_rate_hz)
}

fn write_pcm16_mono_wav(
    out_path: &Path,
    samples: &[i16],
    sample_rate_hz: u32,
) -> Result<(), AuditionError> {
    let data_size = (samples.len() * 2) as u32;
    let riff_size = 36 + data_size;
    let bytes_per_sec = sample_rate_hz * 2; // mono * 16-bit
    let block_align: u16 = 2;
    let bits_per_sample: u16 = 16;

    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&riff_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // num channels
    buf.extend_from_slice(&sample_rate_hz.to_le_bytes());
    buf.extend_from_slice(&bytes_per_sec.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }

    let mut f = File::create(out_path)?;
    f.write_all(&buf)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn rejects_unaligned_brr_length() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("a.wav");
        let err = export_decoded_brr_wav(&[0u8; 10], 32000, &out).unwrap_err();
        assert!(matches!(err, AuditionError::UnalignedBrrLength(10)));
    }

    #[test]
    fn rejects_zero_sample_rate() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("a.wav");
        let err = export_decoded_brr_wav(&[0u8; 9], 0, &out).unwrap_err();
        assert!(matches!(err, AuditionError::SampleRateOutOfRange(0)));
    }

    #[test]
    fn header_layout_is_riff_wave_pcm_mono() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("a.wav");
        let brr = [0u8; 9]; // single all-zero block → 16 zero samples
        let r = export_decoded_brr_wav(&brr, 32000, &out).unwrap();
        assert_eq!(r.blocks_decoded, 1);
        assert_eq!(r.samples_written, 16);
        assert_eq!(r.bytes_written, 44 + 32);

        let bytes = fs::read(&out).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 16);
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1); // PCM
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1); // mono
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 32000);
        assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 64000);
        assert_eq!(u16::from_le_bytes(bytes[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 16);
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 32);
        assert_eq!(bytes.len(), 44 + 32);
    }

    #[test]
    fn output_is_byte_stable() {
        let dir = tempdir().unwrap();
        let out_a = dir.path().join("a.wav");
        let out_b = dir.path().join("b.wav");
        // Some non-trivial BRR: filter 0 / shift 4 / data 0x77 ⇒ 56s.
        let mut brr = vec![0u8; 9 * 4];
        for i in 0..4 {
            brr[i * 9] = 0x40;
            for j in 1..9 {
                brr[i * 9 + j] = 0x77;
            }
        }
        export_decoded_brr_wav(&brr, 32000, &out_a).unwrap();
        export_decoded_brr_wav(&brr, 32000, &out_b).unwrap();
        let a = fs::read(&out_a).unwrap();
        let b = fs::read(&out_b).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn pcm_payload_matches_decoder_output() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("a.wav");
        // Filter 0 / shift 4 / data 0x77 ⇒ all decoded samples 56.
        let block = [0x40u8, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77];
        export_decoded_brr_wav(&block, 32000, &out).unwrap();
        let bytes = fs::read(&out).unwrap();
        for i in 0..16 {
            let s = i16::from_le_bytes(bytes[44 + i * 2..44 + i * 2 + 2].try_into().unwrap());
            assert_eq!(s, 56);
        }
    }
}
