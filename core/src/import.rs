//! M1.2 import orchestration.
//!
//! The end-to-end flow `import_audio` runs:
//!
//! 1. Load the project via [`crate::project::ProjectV1::load_from_path`].
//! 2. Probe the audio file via [`crate::audio::probe`].
//! 3. SHA-256 the file (streaming).
//! 4. Optionally copy the audio into `<project_dir>/audio/`. Same
//!    name + same SHA → reuse the existing copy. Same name + different
//!    SHA → suffix `_2`, `_3`, … until unique. Path-traversal guard:
//!    the resolved target must live inside the project's audio
//!    subtree.
//! 5. Derive a default `id` from the filename if the caller didn't
//!    pass one (`my_lead_sample.wav` → `my_lead_sample`; collisions
//!    suffix; empty derivation → `sample_<N>`).
//! 6. Build a default-fields `SampleSlot` (root MIDI 60, loop
//!    disabled, `GainRaw { gain_byte: 127 }`, vol 1.0, pan 0.0,
//!    echo off).
//! 7. Append to `sample_pool`. First sample also seeds
//!    `m1.active_sample_id` so the project validates immediately.
//! 8. Run the full SPEC §16.6 validation. Failure here means a bug
//!    in the import logic — return `ResultingProjectInvalid` rather
//!    than save a broken project.
//! 9. Save the project back via
//!    [`crate::project::ProjectV1::save_to_path`].

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::audio::{self, AudioFormat, AudioMetadata, AudioProbeError};
use crate::project::{
    Envelope, ProjectIoError, ProjectV1, SampleFormat, SampleLoop, SamplePlayback, SampleSlot,
    SampleSource, ValidationError,
};

/// Caller-supplied options for [`import_audio`]. Defaults: copy into
/// project, no overrides.
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    pub id: Option<String>,
    pub name: Option<String>,
    pub copy_into_project: bool,
    pub brr_sample_rate_hz: Option<u32>,
}

impl ImportOptions {
    /// Convenience that mirrors the GUI / CLI default: copy on, no
    /// overrides.
    pub fn copy_default() -> Self {
        Self {
            id: None,
            name: None,
            copy_into_project: true,
            brr_sample_rate_hz: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub sample_id: String,
    /// Path written into `project.json` — relative to the project
    /// directory when copying or the file lives inside it; absolute
    /// otherwise (with a warning-via-stderr from the CLI).
    pub stored_path: String,
    pub absolute_source_path: PathBuf,
    pub metadata: AudioMetadata,
    pub sha256: String,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("project: {0}")]
    Project(#[from] ProjectIoError),
    #[error("audio: {0}")]
    Audio(#[from] AudioProbeError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("path traversal refused: {0}")]
    PathTraversal(PathBuf),
    #[error("audio file not found: {0}")]
    AudioNotFound(PathBuf),
    #[error("resulting project invalid (this is a bug in import): {0:?}")]
    ResultingProjectInvalid(Vec<ValidationError>),
}

pub fn import_audio(
    project_path: &Path,
    audio_path: &Path,
    options: ImportOptions,
) -> Result<ImportResult, ImportError> {
    if !audio_path.exists() {
        return Err(ImportError::AudioNotFound(audio_path.to_path_buf()));
    }

    let mut project = ProjectV1::load_from_path(project_path)?;
    let project_dir = project_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut metadata = audio::probe(audio_path)?;
    if let (AudioFormat::Brr, Some(rate)) = (metadata.format, options.brr_sample_rate_hz) {
        metadata.sample_rate_hz = rate;
    }

    let sha256 = audio::sha256_of_file(audio_path)?;
    let absolute_source_path = audio_path
        .canonicalize()
        .unwrap_or_else(|_| audio_path.to_path_buf());

    let (stored_path, _physical_path) = if options.copy_into_project {
        copy_into_project(&project_dir, audio_path, &sha256)?
    } else {
        (
            stored_path_no_copy(&project_dir, audio_path),
            absolute_source_path.clone(),
        )
    };

    let derived_id = match options.id.as_deref() {
        Some(id) => id.to_string(),
        None => derive_default_id(audio_path, &project),
    };
    let derived_id = uniquify_id(derived_id, &project);

    let derived_name = match options.name.as_deref() {
        Some(n) => n.to_string(),
        None => derive_default_name(audio_path),
    };

    let was_first_sample = project.sample_pool.is_empty();
    let slot = SampleSlot {
        id: derived_id.clone(),
        name: derived_name,
        source: SampleSource {
            path: stored_path.clone(),
            sha256: sha256.clone(),
            format: project_format(metadata.format),
            sample_rate_hz: metadata.sample_rate_hz,
            channels: metadata.channels,
            frames: metadata.frames,
        },
        root_midi_note: 60,
        looped: SampleLoop {
            enabled: false,
            start_sample: None,
            end_sample: None,
            snap: None,
        },
        playback: SamplePlayback {
            volume: 1.0,
            pan: 0.0,
            echo: false,
            envelope: Envelope::GainRaw { gain_byte: 127 },
        },
    };
    project.sample_pool.push(slot);
    if was_first_sample {
        project.m1.active_sample_id = derived_id.clone();
    }

    if let Err(errors) = project.validate() {
        return Err(ImportError::ResultingProjectInvalid(errors));
    }

    project.save_to_path(project_path)?;

    Ok(ImportResult {
        sample_id: derived_id,
        stored_path,
        absolute_source_path,
        metadata,
        sha256,
    })
}

// =============================================================================
// Copy step + dedup
// =============================================================================

/// Copy `audio_path` into `<project_dir>/audio/`, or reuse the
/// existing copy if a same-named same-SHA file already lives there.
/// Returns `(stored_relative_path, absolute_target_path)`.
fn copy_into_project(
    project_dir: &Path,
    audio_path: &Path,
    sha256: &str,
) -> Result<(String, PathBuf), ImportError> {
    let audio_dir = project_dir.join("audio");
    std::fs::create_dir_all(&audio_dir).map_err(ImportError::Io)?;

    // Path-traversal guard: ensure audio_dir resolves inside project_dir.
    let project_canonical = project_dir
        .canonicalize()
        .map_err(|_| ImportError::PathTraversal(project_dir.to_path_buf()))?;
    let audio_dir_canonical = audio_dir
        .canonicalize()
        .map_err(|_| ImportError::PathTraversal(audio_dir.clone()))?;
    if !audio_dir_canonical.starts_with(&project_canonical) {
        return Err(ImportError::PathTraversal(audio_dir));
    }

    let original_filename = audio_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("imported")
        .to_string();
    let (stem, ext) = split_filename(&original_filename);

    // Find a target that's either same-SHA-reused or a unique suffix.
    for n in 0u32.. {
        let candidate = if n == 0 {
            original_filename.clone()
        } else {
            match ext {
                Some(e) => format!("{stem}_{}.{}", n + 1, e),
                None => format!("{stem}_{}", n + 1),
            }
        };
        let target = audio_dir.join(&candidate);
        if target.exists() {
            // Same name; check SHA match for dedup reuse.
            let existing_sha = audio::sha256_of_file(&target).map_err(ImportError::Io)?;
            if existing_sha == sha256 {
                let stored = format!("audio/{candidate}");
                return Ok((stored, target));
            }
            // Different content; try next suffix.
            continue;
        }
        // Free filename. Copy the source bytes here.
        std::fs::copy(audio_path, &target).map_err(ImportError::Io)?;
        let stored = format!("audio/{candidate}");
        return Ok((stored, target));
    }
    unreachable!("u32 collision counter exhausted");
}

fn stored_path_no_copy(project_dir: &Path, audio_path: &Path) -> String {
    // Try to make audio_path relative to project_dir; fall back to
    // the absolute path otherwise.
    let proj_canonical = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let audio_canonical = audio_path
        .canonicalize()
        .unwrap_or_else(|_| audio_path.to_path_buf());
    if let Ok(rel) = audio_canonical.strip_prefix(&proj_canonical) {
        forward_slashes(rel)
    } else {
        forward_slashes(&audio_canonical)
    }
}

fn forward_slashes(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn split_filename(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
        _ => (name, None),
    }
}

// =============================================================================
// id / name derivation
// =============================================================================

fn derive_default_name(audio_path: &Path) -> String {
    audio_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| "sample".to_string())
}

fn derive_default_id(audio_path: &Path, project: &ProjectV1) -> String {
    let stem = audio_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut buf = String::new();
    let mut last_was_sep = true; // suppress leading separator
    for c in stem.chars() {
        let lower = c.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            buf.push(lower);
            last_was_sep = false;
        } else if !last_was_sep {
            // Anything else — including '_' in the source — folds to
            // a single underscore separator and collapses runs.
            buf.push('_');
            last_was_sep = true;
        }
    }
    while buf.ends_with('_') {
        buf.pop();
    }
    while buf.starts_with('_') {
        buf.remove(0);
    }
    if buf.is_empty() {
        return format!("sample_{}", project.sample_pool.len() + 1);
    }
    if buf.chars().count() > 64 {
        buf.truncate(64);
    }
    buf
}

fn uniquify_id(base: String, project: &ProjectV1) -> String {
    if !project.sample_pool.iter().any(|s| s.id == base) {
        return base;
    }
    for n in 2u32.. {
        let candidate = format!("{base}_{n}");
        if !project.sample_pool.iter().any(|s| s.id == candidate) {
            return candidate;
        }
    }
    unreachable!("u32 collision counter exhausted")
}

fn project_format(f: AudioFormat) -> SampleFormat {
    match f {
        AudioFormat::Wav => SampleFormat::Wav,
        AudioFormat::Aiff => SampleFormat::Aiff,
        AudioFormat::Brr => SampleFormat::Brr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_id_basic_lowercase() {
        let project = empty_project();
        assert_eq!(
            derive_default_id(Path::new("My Lead Sample!.wav"), &project),
            "my_lead_sample"
        );
    }

    #[test]
    fn derive_id_collapses_runs_and_trims() {
        let project = empty_project();
        assert_eq!(
            derive_default_id(Path::new("__a___b__.wav"), &project),
            "a_b"
        );
    }

    #[test]
    fn derive_id_falls_back_to_sample_n_on_empty() {
        let project = empty_project();
        assert_eq!(
            derive_default_id(Path::new("___.wav"), &project),
            "sample_1"
        );
    }

    #[test]
    fn derive_id_truncates_to_64_chars() {
        let project = empty_project();
        let long = "a".repeat(120) + ".wav";
        let id = derive_default_id(Path::new(&long), &project);
        assert_eq!(id.chars().count(), 64);
    }

    #[test]
    fn uniquify_id_appends_suffix_on_collision() {
        let mut project = empty_project();
        project.sample_pool.push(slot("x"));
        project.sample_pool.push(slot("x_2"));
        assert_eq!(uniquify_id("x".to_string(), &project), "x_3");
    }

    #[test]
    fn split_filename_handles_no_extension() {
        assert_eq!(split_filename("foo.wav"), ("foo", Some("wav")));
        assert_eq!(split_filename("README"), ("README", None));
        assert_eq!(split_filename(".hidden"), (".hidden", None));
    }

    fn empty_project() -> ProjectV1 {
        ProjectV1::new_template("test")
    }

    fn slot(id: &str) -> SampleSlot {
        SampleSlot {
            id: id.to_string(),
            name: id.to_string(),
            source: SampleSource {
                path: "x.wav".to_string(),
                sha256: "0".repeat(64),
                format: SampleFormat::Wav,
                sample_rate_hz: 32000,
                channels: 1,
                frames: 1024,
            },
            root_midi_note: 60,
            looped: SampleLoop {
                enabled: false,
                start_sample: None,
                end_sample: None,
                snap: None,
            },
            playback: SamplePlayback {
                volume: 1.0,
                pan: 0.0,
                echo: false,
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        }
    }
}
