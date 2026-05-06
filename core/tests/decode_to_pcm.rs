//! Integration tests for `core::audio::decode_to_mono_pcm`.

use std::path::Path;

use sfc_atomizer_core::audio::{decode_to_mono_pcm, probe};
use tempfile::tempdir;

mod common;

/// Write a minimal 16-bit PCM WAV with caller-supplied interleaved
/// sample data. `samples_per_channel` is what AIFF/SPEC call "frames".
fn write_pcm16_wav(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples_interleaved: &[i16],
) -> std::io::Result<()> {
    let bytes_per_sample = 2u32;
    let block_align = u32::from(channels) * bytes_per_sample;
    let byte_rate = sample_rate * block_align;
    let data_size = (samples_interleaved.len() as u32) * 2;
    let fmt_size = 16u32;
    let chunk_size = 4 + (8 + fmt_size) + (8 + data_size);

    let mut buf = Vec::with_capacity((chunk_size + 8) as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&fmt_size.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&(block_align as u16).to_le_bytes());
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    for s in samples_interleaved {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, buf)
}

#[test]
fn decode_mono_wav_returns_exact_samples() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("mono.wav");
    let samples: Vec<i16> = (0..32).map(|i| (i as i16) * 100).collect();
    write_pcm16_wav(&path, 32000, 1, &samples).unwrap();

    let out = decode_to_mono_pcm(&path).unwrap();
    assert_eq!(out, samples);
}

#[test]
fn decode_stereo_wav_averages_channels() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("stereo.wav");
    // Frame i: L=200, R=400 → mono = 300 each frame.
    let mut interleaved = Vec::new();
    for _ in 0..16 {
        interleaved.push(200i16);
        interleaved.push(400i16);
    }
    write_pcm16_wav(&path, 32000, 2, &interleaved).unwrap();

    let out = decode_to_mono_pcm(&path).unwrap();
    assert_eq!(out.len(), 16);
    for v in &out {
        assert_eq!(*v, 300);
    }
}

#[test]
fn decode_zero_filled_wav_matches_probed_frame_count() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("zero.wav");
    common::write_test_wav(&path, 32000, 1, 16, 256).unwrap();
    let metadata = probe(&path).unwrap();
    let out = decode_to_mono_pcm(&path).unwrap();
    assert_eq!(out.len() as u64, metadata.frames);
    assert!(out.iter().all(|s| *s == 0));
}

#[test]
fn decode_brr_returns_block_count_times_16() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("zero.brr");
    common::write_test_brr(&path, 4).unwrap();
    let out = decode_to_mono_pcm(&path).unwrap();
    assert_eq!(out.len(), 64);
    assert!(out.iter().all(|s| *s == 0));
}
