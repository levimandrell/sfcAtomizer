//! Audio file probing for the M1.2 import surface.
//!
//! Hand-rolled WAV + AIFF/AIFC + BRR header parsers, sized to what
//! `sfcwc import` and the GUI's File → Import Audio need: format,
//! sample rate, channel count, bit depth, and total frame count.
//! No PCM decoding — `symphonia` handles that at M1.3 when the BRR
//! encoder needs the actual sample bytes.
//!
//! Supported inputs (SPEC §16.4):
//!
//! - **WAV**: RIFF/WAVE container, integer PCM only (8/16/24-bit),
//!   1–2 channels, 8000..=96000 Hz. WAVE_FORMAT_PCM = `0x0001`. Float
//!   PCM (`0x0003`) and 32-bit int are rejected.
//! - **AIFF / AIFC**: IFF FORM container. Plain AIFF accepted as
//!   uncompressed big-endian PCM. AIFC accepted only when the
//!   compression code is `NONE` or `sowt`.
//! - **BRR**: file size must be a positive multiple of 9 bytes;
//!   `frames = (size / 9) * 16`, `channels = 1`, `bits = 4`,
//!   `sample_rate_hz = 32000` by default (override at import time).

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Wav,
    Aiff,
    Brr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioMetadata {
    pub format: AudioFormat,
    pub sample_rate_hz: u32,
    pub channels: u8,
    pub frames: u64,
    pub bits_per_sample: u8,
}

#[derive(Debug, Error)]
pub enum AudioProbeError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("unsupported audio format extension: {0:?}")]
    UnsupportedExtension(String),
    #[error("file truncated or malformed: {0}")]
    Malformed(String),
    #[error("AIFF-C compression {0:?} not supported (M1: PCM only — NONE or sowt)")]
    AiffCompressionUnsupported(String),
    #[error("unsupported bits-per-sample: {0} (M1: 8/16/24)")]
    UnsupportedBitDepth(u8),
    #[error("float WAV not supported (M1: integer PCM only)")]
    FloatPcmUnsupported,
    #[error("unsupported channel count: {0} (M1: 1 or 2)")]
    UnsupportedChannelCount(u8),
    #[error("sample rate {0} Hz outside 8000..=96000")]
    UnsupportedSampleRate(u32),
    #[error("BRR file size {size} not a positive multiple of 9 bytes")]
    BrrInvalidSize { size: u64 },
}

const WAV_FORMAT_PCM_INT: u16 = 0x0001;
const WAV_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAV_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// Probe a single audio file. Dispatches on extension, then delegates
/// to the per-format parser. Extension is lowercased before matching.
pub fn probe(audio_path: &Path) -> Result<AudioMetadata, AudioProbeError> {
    let ext = audio_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = std::fs::read(audio_path)?;
    match ext.as_str() {
        "wav" => probe_wav(&bytes),
        "aif" | "aiff" | "aifc" => probe_aiff(&bytes),
        "brr" => probe_brr(bytes.len() as u64),
        other => Err(AudioProbeError::UnsupportedExtension(other.to_string())),
    }
}

/// Streaming SHA-256 of `path`. 64 KiB chunks; lowercase hex output.
pub fn sha256_of_file(path: &Path) -> io::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    Ok(s)
}

// =============================================================================
// WAV (RIFF/WAVE)
// =============================================================================

fn probe_wav(bytes: &[u8]) -> Result<AudioMetadata, AudioProbeError> {
    if bytes.len() < 12 {
        return Err(AudioProbeError::Malformed("WAV header < 12 bytes".into()));
    }
    if &bytes[0..4] != b"RIFF" {
        return Err(AudioProbeError::Malformed("missing 'RIFF' magic".into()));
    }
    if &bytes[8..12] != b"WAVE" {
        return Err(AudioProbeError::Malformed(
            "missing 'WAVE' form type".into(),
        ));
    }

    let mut fmt: Option<WavFmt> = None;
    let mut data_bytes: Option<u32> = None;

    for ch in iter_riff_chunks(&bytes[12..])? {
        match &ch.id {
            b"fmt " => fmt = Some(parse_wav_fmt(ch.body)?),
            b"data" => data_bytes = Some(ch.body.len() as u32),
            _ => {}
        }
    }

    let fmt = fmt.ok_or_else(|| AudioProbeError::Malformed("missing 'fmt ' chunk".into()))?;
    let data =
        data_bytes.ok_or_else(|| AudioProbeError::Malformed("missing 'data' chunk".into()))?;

    validate_channels(fmt.channels)?;
    validate_sample_rate(fmt.sample_rate)?;
    let bits = match fmt.bits_per_sample {
        8 | 16 | 24 => fmt.bits_per_sample as u8,
        other => return Err(AudioProbeError::UnsupportedBitDepth(other as u8)),
    };
    let bytes_per_frame = (fmt.channels as u32) * (fmt.bits_per_sample as u32 / 8);
    let frames = if bytes_per_frame > 0 {
        u64::from(data) / u64::from(bytes_per_frame)
    } else {
        0
    };
    Ok(AudioMetadata {
        format: AudioFormat::Wav,
        sample_rate_hz: fmt.sample_rate,
        channels: fmt.channels as u8,
        frames,
        bits_per_sample: bits,
    })
}

struct WavFmt {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
}

fn parse_wav_fmt(body: &[u8]) -> Result<WavFmt, AudioProbeError> {
    if body.len() < 16 {
        return Err(AudioProbeError::Malformed("'fmt ' chunk < 16 bytes".into()));
    }
    let format_tag = u16::from_le_bytes([body[0], body[1]]);
    let channels = u16::from_le_bytes([body[2], body[3]]);
    let sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
    let bits_per_sample = u16::from_le_bytes([body[14], body[15]]);

    let effective_format = if format_tag == WAV_FORMAT_EXTENSIBLE {
        // WAVE_FORMAT_EXTENSIBLE: a 16-byte SubFormat GUID at offset 24
        // names the actual codec. The first two bytes of the GUID match
        // the legacy format tags.
        if body.len() < 40 {
            return Err(AudioProbeError::Malformed(
                "WAVE_FORMAT_EXTENSIBLE chunk too short".into(),
            ));
        }
        u16::from_le_bytes([body[24], body[25]])
    } else {
        format_tag
    };

    match effective_format {
        WAV_FORMAT_PCM_INT => Ok(WavFmt {
            sample_rate,
            channels,
            bits_per_sample,
        }),
        WAV_FORMAT_IEEE_FLOAT => Err(AudioProbeError::FloatPcmUnsupported),
        other => Err(AudioProbeError::Malformed(format!(
            "unsupported WAV format tag {other:#06x}"
        ))),
    }
}

struct RiffChunk<'a> {
    id: [u8; 4],
    body: &'a [u8],
}

fn iter_riff_chunks(buf: &[u8]) -> Result<Vec<RiffChunk<'_>>, AudioProbeError> {
    let mut out = Vec::new();
    let mut off = 0;
    while off + 8 <= buf.len() {
        let id = [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]];
        let size =
            u32::from_le_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]) as usize;
        let body_start = off + 8;
        let body_end = body_start
            .checked_add(size)
            .ok_or_else(|| AudioProbeError::Malformed("RIFF chunk size overflow".into()))?;
        if body_end > buf.len() {
            return Err(AudioProbeError::Malformed(format!(
                "RIFF chunk {:?} runs past EOF",
                std::str::from_utf8(&id).unwrap_or("??")
            )));
        }
        out.push(RiffChunk {
            id,
            body: &buf[body_start..body_end],
        });
        // Chunks are padded to even length.
        off = body_end + (size & 1);
    }
    Ok(out)
}

// =============================================================================
// AIFF / AIFC (IFF FORM)
// =============================================================================

fn probe_aiff(bytes: &[u8]) -> Result<AudioMetadata, AudioProbeError> {
    if bytes.len() < 12 {
        return Err(AudioProbeError::Malformed("AIFF header < 12 bytes".into()));
    }
    if &bytes[0..4] != b"FORM" {
        return Err(AudioProbeError::Malformed("missing 'FORM' magic".into()));
    }
    let form_type = &bytes[8..12];
    let aifc = match form_type {
        b"AIFF" => false,
        b"AIFC" => true,
        other => {
            return Err(AudioProbeError::Malformed(format!(
                "unsupported FORM type {:?}",
                std::str::from_utf8(other).unwrap_or("??")
            )));
        }
    };

    let mut comm: Option<&[u8]> = None;
    for ch in iter_iff_chunks(&bytes[12..])? {
        if ch.id == *b"COMM" {
            comm = Some(ch.body);
            break;
        }
    }
    let comm = comm.ok_or_else(|| AudioProbeError::Malformed("missing 'COMM' chunk".into()))?;
    if comm.len() < 18 {
        return Err(AudioProbeError::Malformed("COMM chunk < 18 bytes".into()));
    }
    let channels = i16::from_be_bytes([comm[0], comm[1]]);
    let frames = u32::from_be_bytes([comm[2], comm[3], comm[4], comm[5]]);
    let bits = i16::from_be_bytes([comm[6], comm[7]]);
    let sample_rate = decode_extended_80(&comm[8..18])?;

    if channels < 1 {
        return Err(AudioProbeError::UnsupportedChannelCount(0));
    }
    validate_channels(channels as u16)?;
    validate_sample_rate(sample_rate)?;
    let bits_u8 = match bits {
        8 | 16 | 24 => bits as u8,
        other => return Err(AudioProbeError::UnsupportedBitDepth(other.max(0) as u8)),
    };

    if aifc {
        if comm.len() < 22 {
            return Err(AudioProbeError::Malformed(
                "AIFC COMM missing compression code".into(),
            ));
        }
        let comp = &comm[18..22];
        match comp {
            b"NONE" | b"sowt" => {}
            other => {
                let s = std::str::from_utf8(other).unwrap_or("??").to_string();
                return Err(AudioProbeError::AiffCompressionUnsupported(s));
            }
        }
    }

    Ok(AudioMetadata {
        format: AudioFormat::Aiff,
        sample_rate_hz: sample_rate,
        channels: channels as u8,
        frames: frames as u64,
        bits_per_sample: bits_u8,
    })
}

struct IffChunk<'a> {
    id: [u8; 4],
    body: &'a [u8],
}

fn iter_iff_chunks(buf: &[u8]) -> Result<Vec<IffChunk<'_>>, AudioProbeError> {
    let mut out = Vec::new();
    let mut off = 0;
    while off + 8 <= buf.len() {
        let id = [buf[off], buf[off + 1], buf[off + 2], buf[off + 3]];
        let size =
            u32::from_be_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]) as usize;
        let body_start = off + 8;
        let body_end = body_start
            .checked_add(size)
            .ok_or_else(|| AudioProbeError::Malformed("IFF chunk size overflow".into()))?;
        if body_end > buf.len() {
            return Err(AudioProbeError::Malformed(format!(
                "IFF chunk {:?} runs past EOF",
                std::str::from_utf8(&id).unwrap_or("??")
            )));
        }
        out.push(IffChunk {
            id,
            body: &buf[body_start..body_end],
        });
        off = body_end + (size & 1);
    }
    Ok(out)
}

/// Decode a 10-byte big-endian IEEE 754 80-bit extended-precision
/// float into a `u32` Hz sample rate. Rejects negative, zero, and
/// non-finite values; rejects any decoded value that can't be
/// represented as a positive integer in `8000..=96000` (the caller's
/// validate_sample_rate gate handles that).
fn decode_extended_80(bytes: &[u8]) -> Result<u32, AudioProbeError> {
    if bytes.len() < 10 {
        return Err(AudioProbeError::Malformed(
            "80-bit extended float < 10 bytes".into(),
        ));
    }
    let sign = bytes[0] >> 7;
    let exp = ((u16::from(bytes[0] & 0x7F)) << 8) | u16::from(bytes[1]);
    let mantissa = u64::from_be_bytes([
        bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9],
    ]);
    if sign != 0 {
        return Err(AudioProbeError::Malformed("negative sample rate".into()));
    }
    if exp == 0 && mantissa == 0 {
        return Err(AudioProbeError::Malformed("zero sample rate".into()));
    }
    if exp == 0x7FFF {
        return Err(AudioProbeError::Malformed("non-finite sample rate".into()));
    }
    if mantissa & (1 << 63) == 0 {
        return Err(AudioProbeError::Malformed(
            "80-bit float without explicit leading bit".into(),
        ));
    }
    let unbiased = i32::from(exp) - 16383;
    // value = mantissa * 2^(unbiased - 63)
    let shift = unbiased - 63;
    let value: u64 = if shift >= 0 {
        if shift >= 64 {
            return Err(AudioProbeError::Malformed("sample rate overflow".into()));
        }
        mantissa
            .checked_shl(shift as u32)
            .ok_or_else(|| AudioProbeError::Malformed("sample rate overflow".into()))?
    } else {
        let s = (-shift) as u32;
        if s >= 64 {
            0
        } else {
            // round-to-nearest by adding half-LSB before shifting down
            let half = 1u64 << (s - 1);
            mantissa.saturating_add(half) >> s
        }
    };
    if value > u64::from(u32::MAX) {
        return Err(AudioProbeError::Malformed("sample rate exceeds u32".into()));
    }
    Ok(value as u32)
}

// =============================================================================
// BRR (raw 9-byte blocks)
// =============================================================================

fn probe_brr(size: u64) -> Result<AudioMetadata, AudioProbeError> {
    if size == 0 || !size.is_multiple_of(9) {
        return Err(AudioProbeError::BrrInvalidSize { size });
    }
    let blocks = size / 9;
    Ok(AudioMetadata {
        format: AudioFormat::Brr,
        sample_rate_hz: 32_000,
        channels: 1,
        frames: blocks * 16,
        bits_per_sample: 4,
    })
}

// =============================================================================
// Shared validation
// =============================================================================

fn validate_channels(c: u16) -> Result<(), AudioProbeError> {
    if !(1..=2).contains(&c) {
        return Err(AudioProbeError::UnsupportedChannelCount(c.min(255) as u8));
    }
    Ok(())
}

fn validate_sample_rate(rate: u32) -> Result<(), AudioProbeError> {
    if !(8000..=96000).contains(&rate) {
        return Err(AudioProbeError::UnsupportedSampleRate(rate));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brr_size_must_be_multiple_of_9() {
        assert!(probe_brr(0).is_err());
        assert!(probe_brr(7).is_err());
        assert!(probe_brr(70).is_err());
        let m = probe_brr(72).unwrap();
        assert_eq!(m.format, AudioFormat::Brr);
        assert_eq!(m.frames, 128);
        assert_eq!(m.sample_rate_hz, 32_000);
        assert_eq!(m.channels, 1);
        assert_eq!(m.bits_per_sample, 4);
    }

    #[test]
    fn extended_80_round_trip_against_brief_test_rates() {
        // The synthesized test fixtures use the four pre-computed byte
        // tables for 32000 / 44100 / 22050 / 8000 Hz; this confirms the
        // decoder agrees with each.
        for (rate, bytes) in [
            (
                32_000u32,
                &[0x40, 0x0D, 0xFA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            ),
            (
                44_100u32,
                &[0x40, 0x0E, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            ),
            (
                22_050u32,
                &[0x40, 0x0D, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            ),
            (
                8_000u32,
                &[0x40, 0x0B, 0xFA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            ),
        ] {
            assert_eq!(decode_extended_80(bytes).unwrap(), rate);
        }
    }
}
