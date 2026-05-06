// Each integration-test binary that pulls in `mod common;` sees only
// the helpers it actually uses; the unused ones look dead from that
// crate's perspective. Suppressing dead_code at the module level is
// the canonical fix for shared `tests/common/mod.rs` modules.
#![allow(dead_code)]

//! Synthesized audio fixture helpers shared across integration tests.
//!
//! Tests build minimal valid WAV/AIFF/AIFC/BRR files in [`TempDir`]s
//! at runtime; nothing binary is ever committed. The PCM body is
//! always zero-filled — these tests exercise probe + import path
//! logic, not decode quality.
//!
//! Sample-rate values for AIFF use the four pre-computed 80-bit
//! IEEE 754 extended-precision byte tables for 8000 / 22050 / 32000
//! / 44100 Hz. Other rates require the encoder to compute the
//! mantissa/exponent at runtime — out of scope for M1.2.

use std::path::Path;

pub fn write_test_wav(
    path: &Path,
    sample_rate: u32,
    channels: u8,
    bits: u8,
    frames: u32,
) -> std::io::Result<()> {
    let bytes_per_sample = u32::from(bits / 8);
    let block_align = u32::from(channels) * bytes_per_sample;
    let byte_rate = sample_rate * block_align;
    let data_size = frames * block_align;

    // RIFF header + fmt chunk header + 16-byte fmt body + data chunk
    // header + data body.
    let fmt_size = 16u32;
    let chunk_size = 4 + (8 + fmt_size) + (8 + data_size);

    let mut buf = Vec::with_capacity((chunk_size as usize) + 8);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&fmt_size.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // WAVE_FORMAT_PCM
    buf.extend_from_slice(&u16::from(channels).to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(block_align as u16).to_le_bytes());
    buf.extend_from_slice(&u16::from(bits).to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.resize(buf.len() + data_size as usize, 0);

    std::fs::write(path, buf)
}

/// Float WAV with `WAVE_FORMAT_IEEE_FLOAT` (`0x0003`). Used by the
/// reject-float-PCM probe test.
pub fn write_test_wav_float(
    path: &Path,
    sample_rate: u32,
    channels: u8,
    frames: u32,
) -> std::io::Result<()> {
    let bytes_per_sample = 4u32; // f32
    let block_align = u32::from(channels) * bytes_per_sample;
    let byte_rate = sample_rate * block_align;
    let data_size = frames * block_align;
    let fmt_size = 16u32;
    let chunk_size = 4 + (8 + fmt_size) + (8 + data_size);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&fmt_size.to_le_bytes());
    buf.extend_from_slice(&3u16.to_le_bytes()); // WAVE_FORMAT_IEEE_FLOAT
    buf.extend_from_slice(&u16::from(channels).to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(block_align as u16).to_le_bytes());
    buf.extend_from_slice(&32u16.to_le_bytes()); // 32-bit float
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.resize(buf.len() + data_size as usize, 0);
    std::fs::write(path, buf)
}

pub fn write_test_aiff(
    path: &Path,
    sample_rate: u32,
    channels: u8,
    bits: u8,
    frames: u32,
) -> std::io::Result<()> {
    write_aiff_or_aifc(path, sample_rate, channels, bits, frames, None)
}

/// AIFC variant with an explicit 4-byte compression code (e.g.
/// `b"NONE"`, `b"sowt"`, `b"ima4"`).
pub fn write_test_aifc(
    path: &Path,
    sample_rate: u32,
    channels: u8,
    bits: u8,
    frames: u32,
    compression: &[u8; 4],
) -> std::io::Result<()> {
    write_aiff_or_aifc(
        path,
        sample_rate,
        channels,
        bits,
        frames,
        Some(*compression),
    )
}

fn write_aiff_or_aifc(
    path: &Path,
    sample_rate: u32,
    channels: u8,
    bits: u8,
    frames: u32,
    compression: Option<[u8; 4]>,
) -> std::io::Result<()> {
    let bytes_per_sample = u32::from(bits / 8);
    let frame_bytes = u32::from(channels) * bytes_per_sample;
    let pcm_bytes = frames * frame_bytes;
    let ssnd_size = 8 + pcm_bytes; // offset + blockSize + body

    // COMM body: channels(2) + frames(4) + bits(2) + extended(10) +
    // optional AIFC compression (4 + 1 + name=0 = 5 minimum, padded to even).
    let extended = encode_extended_80(sample_rate)
        .expect("AIFF synth requires a pre-computed extended-80 entry for this sample rate");
    let mut comm = Vec::new();
    comm.extend_from_slice(&u16::from(channels).to_be_bytes());
    comm.extend_from_slice(&frames.to_be_bytes());
    comm.extend_from_slice(&u16::from(bits).to_be_bytes());
    comm.extend_from_slice(&extended);
    let is_aifc = compression.is_some();
    if let Some(comp) = compression {
        comm.extend_from_slice(&comp);
        // Pascal-style compression name: 1-byte length, then bytes,
        // padded to even total. We use empty name → length 0, then
        // one zero byte to pad.
        comm.push(0);
        comm.push(0);
    }
    let comm_size = comm.len() as u32;

    let form_type: &[u8; 4] = if is_aifc { b"AIFC" } else { b"AIFF" };
    let mut form_body = Vec::new();
    form_body.extend_from_slice(form_type);
    if is_aifc {
        // FVER chunk per AIFC spec: timestamp 0xA2805140.
        form_body.extend_from_slice(b"FVER");
        form_body.extend_from_slice(&4u32.to_be_bytes());
        form_body.extend_from_slice(&0xA280_5140u32.to_be_bytes());
    }
    form_body.extend_from_slice(b"COMM");
    form_body.extend_from_slice(&comm_size.to_be_bytes());
    form_body.extend_from_slice(&comm);
    if !comm_size.is_multiple_of(2) {
        form_body.push(0);
    }
    form_body.extend_from_slice(b"SSND");
    form_body.extend_from_slice(&ssnd_size.to_be_bytes());
    form_body.extend_from_slice(&0u32.to_be_bytes()); // offset
    form_body.extend_from_slice(&0u32.to_be_bytes()); // blockSize
    form_body.resize(form_body.len() + pcm_bytes as usize, 0);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"FORM");
    buf.extend_from_slice(&(form_body.len() as u32).to_be_bytes());
    buf.extend_from_slice(&form_body);

    std::fs::write(path, buf)
}

pub fn write_test_brr(path: &Path, blocks: usize) -> std::io::Result<()> {
    std::fs::write(path, vec![0u8; blocks * 9])
}

/// Lookup table for the four sample rates the M1.2 tests need. Keeps
/// the AIFF synthesizer trivial and lossless.
fn encode_extended_80(rate: u32) -> Option<[u8; 10]> {
    match rate {
        32_000 => Some([0x40, 0x0D, 0xFA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        44_100 => Some([0x40, 0x0E, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        22_050 => Some([0x40, 0x0D, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        8_000 => Some([0x40, 0x0B, 0xFA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        _ => None,
    }
}
