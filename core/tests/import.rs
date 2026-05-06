//! Import-pipeline integration tests.
//!
//! Each test wires a fresh project via `ProjectV1::new_template`,
//! synthesizes a minimal WAV/AIFF/BRR fixture in a tempdir, runs
//! `import_audio`, and inspects the resulting project + filesystem.

mod common;

use std::path::PathBuf;

use sfc_atomizer_core::import::{import_audio, ImportError, ImportOptions};
use sfc_atomizer_core::project::{ProjectV1, SampleFormat};

fn write_template_at(dir: &std::path::Path) -> PathBuf {
    let project_path = dir.join("p.sfcproj.json");
    ProjectV1::new_template("demo")
        .save_to_path(&project_path)
        .unwrap();
    project_path
}

#[test]
fn import_wav_into_empty_project_validates() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let audio = dir.path().join("lead.wav");
    common::write_test_wav(&audio, 32_000, 1, 16, 4096).unwrap();

    let r = import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap();
    assert_eq!(r.sample_id, "lead");
    assert_eq!(r.metadata.frames, 4096);
    assert!(r.stored_path.starts_with("audio/"));

    let p = ProjectV1::load_from_path(&project_path).unwrap();
    p.validate().expect("post-import project must validate");
    assert_eq!(p.sample_pool.len(), 1);
    assert_eq!(p.sample_pool[0].id, "lead");
    assert_eq!(p.m1.active_sample_id, "lead");
    assert_eq!(p.sample_pool[0].source.format, SampleFormat::Wav);
    assert!(dir.path().join("audio/lead.wav").is_file());
}

#[test]
fn re_import_same_file_dedupes_by_sha() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let audio = dir.path().join("lead.wav");
    common::write_test_wav(&audio, 32_000, 1, 16, 4096).unwrap();

    let r1 = import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap();
    let r2 = import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap();
    // Second import gets a unique id (e.g. lead_2) but reuses the
    // existing audio/lead.wav file (no _2 copy).
    assert_eq!(r1.stored_path, "audio/lead.wav");
    assert_eq!(r2.stored_path, "audio/lead.wav");
    assert_ne!(r1.sample_id, r2.sample_id);
    let p = ProjectV1::load_from_path(&project_path).unwrap();
    assert_eq!(p.sample_pool.len(), 2);
    // No second physical file.
    let entries: Vec<_> = std::fs::read_dir(dir.path().join("audio"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["lead.wav"]);
}

#[test]
fn re_import_same_filename_different_content_suffixes_file() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let audio = dir.path().join("lead.wav");
    common::write_test_wav(&audio, 32_000, 1, 16, 4096).unwrap();
    import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap();

    // Replace the source with different bytes (different frame count
    // = different file size = different SHA).
    common::write_test_wav(&audio, 32_000, 1, 16, 8192).unwrap();
    let r2 = import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap();
    assert_eq!(r2.stored_path, "audio/lead_2.wav");
    assert!(dir.path().join("audio/lead_2.wav").is_file());
    assert!(dir.path().join("audio/lead.wav").is_file());
}

#[test]
fn no_copy_stores_relative_path_when_source_is_inside_project() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    // Place the audio file inside the project dir but not in audio/.
    let audio = dir.path().join("lead.wav");
    common::write_test_wav(&audio, 32_000, 1, 16, 4096).unwrap();

    let mut opts = ImportOptions::copy_default();
    opts.copy_into_project = false;
    let r = import_audio(&project_path, &audio, opts).unwrap();
    assert_eq!(r.stored_path, "lead.wav");
    // Audio dir is not created in --no-copy mode.
    assert!(!dir.path().join("audio").exists());
}

#[test]
fn brr_import_uses_override_sample_rate() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let audio = dir.path().join("loop.brr");
    common::write_test_brr(&audio, 4).unwrap();

    let mut opts = ImportOptions::copy_default();
    opts.brr_sample_rate_hz = Some(22_050);
    let r = import_audio(&project_path, &audio, opts).unwrap();
    assert_eq!(r.metadata.sample_rate_hz, 22_050);
    let p = ProjectV1::load_from_path(&project_path).unwrap();
    assert_eq!(p.sample_pool[0].source.sample_rate_hz, 22_050);
    p.validate().expect("brr import must validate");
}

#[test]
fn id_derived_from_messy_filename() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let audio = dir.path().join("My Lead Sample!.wav");
    common::write_test_wav(&audio, 32_000, 1, 16, 4096).unwrap();
    let r = import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap();
    assert_eq!(r.sample_id, "my_lead_sample");
}

#[test]
fn id_falls_back_to_sample_n_when_filename_strips_to_empty() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let audio = dir.path().join("___.wav");
    common::write_test_wav(&audio, 32_000, 1, 16, 4096).unwrap();
    let r = import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap();
    assert_eq!(r.sample_id, "sample_1");
}

#[test]
fn missing_audio_file_errors() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let bogus = dir.path().join("nope.wav");
    let err = import_audio(&project_path, &bogus, ImportOptions::copy_default()).unwrap_err();
    assert!(matches!(err, ImportError::AudioNotFound(_)), "{err}");
}

#[test]
fn unsupported_extension_errors() {
    let dir = tempfile::tempdir().unwrap();
    let project_path = write_template_at(dir.path());
    let audio = dir.path().join("a.flac");
    std::fs::write(&audio, b"fake").unwrap();
    let err = import_audio(&project_path, &audio, ImportOptions::copy_default()).unwrap_err();
    assert!(matches!(err, ImportError::Audio(_)), "{err}");
}
