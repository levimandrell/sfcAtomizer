//! Probe tests against synthesized WAV / AIFF / AIFC / BRR fixtures.

mod common;

use sfc_atomizer_core::audio::{self, AudioFormat, AudioProbeError};

#[test]
fn wav_16bit_mono_8khz_1024_frames() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.wav");
    common::write_test_wav(&p, 8_000, 1, 16, 1024).unwrap();
    let m = audio::probe(&p).unwrap();
    assert_eq!(m.format, AudioFormat::Wav);
    assert_eq!(m.sample_rate_hz, 8_000);
    assert_eq!(m.channels, 1);
    assert_eq!(m.bits_per_sample, 16);
    assert_eq!(m.frames, 1024);
}

#[test]
fn wav_24bit_stereo_44100_2048_frames() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.wav");
    common::write_test_wav(&p, 44_100, 2, 24, 2048).unwrap();
    let m = audio::probe(&p).unwrap();
    assert_eq!(m.format, AudioFormat::Wav);
    assert_eq!(m.sample_rate_hz, 44_100);
    assert_eq!(m.channels, 2);
    assert_eq!(m.bits_per_sample, 24);
    assert_eq!(m.frames, 2048);
}

#[test]
fn aiff_16bit_mono_22050_4096_frames() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.aiff");
    common::write_test_aiff(&p, 22_050, 1, 16, 4096).unwrap();
    let m = audio::probe(&p).unwrap();
    assert_eq!(m.format, AudioFormat::Aiff);
    assert_eq!(m.sample_rate_hz, 22_050);
    assert_eq!(m.channels, 1);
    assert_eq!(m.bits_per_sample, 16);
    assert_eq!(m.frames, 4096);
}

#[test]
fn aifc_with_none_compression_passes() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.aifc");
    common::write_test_aifc(&p, 32_000, 1, 16, 256, b"NONE").unwrap();
    let m = audio::probe(&p).unwrap();
    assert_eq!(m.format, AudioFormat::Aiff);
    assert_eq!(m.sample_rate_hz, 32_000);
}

#[test]
fn aifc_with_sowt_compression_passes() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.aifc");
    common::write_test_aifc(&p, 44_100, 2, 16, 256, b"sowt").unwrap();
    let m = audio::probe(&p).unwrap();
    assert_eq!(m.format, AudioFormat::Aiff);
    assert_eq!(m.channels, 2);
}

#[test]
fn aifc_with_ima4_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.aifc");
    common::write_test_aifc(&p, 32_000, 1, 16, 256, b"ima4").unwrap();
    let err = audio::probe(&p).unwrap_err();
    assert!(
        matches!(err, AudioProbeError::AiffCompressionUnsupported(ref s) if s == "ima4"),
        "{err}"
    );
}

#[test]
fn brr_8_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.brr");
    common::write_test_brr(&p, 8).unwrap();
    let m = audio::probe(&p).unwrap();
    assert_eq!(m.format, AudioFormat::Brr);
    assert_eq!(m.frames, 128);
    assert_eq!(m.sample_rate_hz, 32_000);
    assert_eq!(m.channels, 1);
    assert_eq!(m.bits_per_sample, 4);
}

#[test]
fn unsupported_extension() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.flac");
    std::fs::write(&p, b"fake").unwrap();
    let err = audio::probe(&p).unwrap_err();
    assert!(matches!(err, AudioProbeError::UnsupportedExtension(ref s) if s == "flac"));
}

#[test]
fn truncated_wav_header() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.wav");
    std::fs::write(&p, b"RIFF").unwrap();
    let err = audio::probe(&p).unwrap_err();
    assert!(matches!(err, AudioProbeError::Malformed(_)), "{err}");
}

#[test]
fn brr_size_not_multiple_of_9() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.brr");
    std::fs::write(&p, vec![0u8; 70]).unwrap();
    let err = audio::probe(&p).unwrap_err();
    assert!(
        matches!(err, AudioProbeError::BrrInvalidSize { size: 70 }),
        "{err}"
    );
}

#[test]
fn float_wav_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.wav");
    common::write_test_wav_float(&p, 44_100, 1, 256).unwrap();
    let err = audio::probe(&p).unwrap_err();
    assert!(matches!(err, AudioProbeError::FloatPcmUnsupported), "{err}");
}

#[test]
fn sha256_streaming_matches_known_vector() {
    // SHA-256 of "abc".
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.bin");
    std::fs::write(&p, b"abc").unwrap();
    let h = audio::sha256_of_file(&p).unwrap();
    assert_eq!(
        h,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
