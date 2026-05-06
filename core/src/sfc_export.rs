//! M1.6 `.sfc` builder.
//!
//! Orchestrates the full project → `.sfc` pipeline:
//!
//! 1. For each project (A and optional B): encode samples → run
//!    [`crate::driver_build`] → run [`crate::packer`] → run
//!    [`crate::module_writer`] to produce a `module.bin`.
//! 2. Embed both modules at fixed LoROM bank offsets ($01:8000 and
//!    $02:8000 / file offsets $8000 and $10000).
//! 3. Invoke [`crate::asm::AsarBackend::assemble`] on the bundled
//!    65816 loader source. Asar fixes the LoROM header
//!    checksum/complement bytes (`--fix-checksum=on`, opposite of
//!    the SPC700 path's `--fix-checksum=off`).
//! 4. Pad to the next power-of-two LoROM size if needed (the
//!    embedded loader already pads to 256 KB; this is the
//!    safety net).
//! 5. Re-fix the checksum after embedding the modules — asar's
//!    initial fix runs against the empty module banks; once the
//!    real module bytes are written into the file the checksum
//!    needs to be recomputed.
//!
//! When only project A is provided, the output `.sfc` carries
//! module B as a duplicate of module A: the loader still exercises
//! the swap mechanism, and the user hears the same audio twice
//! with a brief gap, which is itself a positive signal that
//! RESET_TO_IPL + re-upload worked.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::asm::{AsarBackend, AssembleError, AssembleInput, AssemblerBackend};
use crate::audio::{decode_to_mono_pcm, AudioDecodeError};
use crate::brr_encoder::{encode as brr_encode, encode_looped, EncodeOptions};
use crate::driver_build::{build as driver_build, DriverBuildError, DriverBuildInput};
use crate::module_writer::{write_module, ModuleWriteError, ModuleWriteInput};
use crate::packer::{pack as packer_pack, EncodedSample, PackError, PackInput};
use crate::project::{ProjectIoError, ProjectV1, ValidationError};

/// LoROM file offset for module A embedding (bank $01).
pub const MODULE_A_FILE_OFFSET: usize = 0x8000;
/// LoROM file offset for module B embedding (bank $02).
pub const MODULE_B_FILE_OFFSET: usize = 0x10000;
/// Smallest LoROM size we ship — matches the loader's pad target.
pub const LOROM_MIN_SIZE: usize = 256 * 1024;
/// Maximum LoROM size we accept (8 Mbit = 1 MB) for M1.
pub const LOROM_MAX_SIZE: usize = 4 * 1024 * 1024;

/// Bytes-per-bank in LoROM (bank N owns SNES $N:8000-$N:FFFF).
pub const LOROM_BANK_SIZE: usize = 0x8000;

/// LoROM header offsets (within file, computed from $00:FFC0 ↔ $7FC0).
pub const LOROM_HEADER_BASE: usize = 0x7FC0;
pub const LOROM_HEADER_TITLE_LEN: usize = 21;
pub const LOROM_HEADER_MODE_OFFSET: usize = LOROM_HEADER_BASE + 0x15;
pub const LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET: usize = LOROM_HEADER_BASE + 0x1C;
pub const LOROM_HEADER_CHECKSUM_OFFSET: usize = LOROM_HEADER_BASE + 0x1E;
pub const LOROM_HEADER_RESET_VECTOR_OFFSET: usize = 0x7FFC;

#[derive(Debug, Clone)]
pub struct SfcExportInput<'a> {
    pub project_a_path: PathBuf,
    pub project_b_path: Option<PathBuf>,
    /// Optional override for the loader source. `None` uses the
    /// embedded canonical [`crate::driver_build`]-style include.
    pub loader_source_override: Option<&'a str>,
    /// Working directory for asar scratch and intermediate files.
    pub working_dir: PathBuf,
    /// Output `.sfc` path.
    pub out_sfc_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SfcModuleArtifact {
    pub project_name: String,
    pub module_bytes: Vec<u8>,
    pub module_file_sha256: String,
    pub module_in_file_sha256: String,
}

#[derive(Debug, Clone)]
pub struct SfcExportOutput {
    pub sfc_path: PathBuf,
    pub sfc_size_bytes: u32,
    pub sfc_sha256: String,
    pub loader_size_bytes: u32,
    pub module_a: SfcModuleArtifact,
    pub module_b: SfcModuleArtifact,
    /// `true` when `module_b` was emitted as a duplicate of
    /// `module_a` (the single-project fallback). Lets the CLI
    /// summary mention the clone explicitly.
    pub module_b_is_clone_of_a: bool,
}

#[derive(Debug, Error)]
pub enum SfcExportError {
    #[error("project {label}: load: {source}")]
    Load {
        label: &'static str,
        #[source]
        source: ProjectIoError,
    },
    #[error("project {label} invalid: {0:?}", errors)]
    Validation {
        label: &'static str,
        errors: Vec<ValidationError>,
    },
    #[error("project {label}: decode {sample_id:?}: {source}")]
    Decode {
        label: &'static str,
        sample_id: String,
        #[source]
        source: AudioDecodeError,
    },
    #[error("project {label}: encode {sample_id:?}: {source}")]
    Encode {
        label: &'static str,
        sample_id: String,
        #[source]
        source: crate::brr_encoder::EncodeError,
    },
    #[error("project {label}: pack: {0}", source)]
    Pack {
        label: &'static str,
        #[source]
        source: PackError,
    },
    #[error("project {label}: driver_build: {0}", source)]
    Driver {
        label: &'static str,
        #[source]
        source: DriverBuildError,
    },
    #[error("project {label}: module_write: {0}", source)]
    Module {
        label: &'static str,
        #[source]
        source: ModuleWriteError,
    },
    #[error("loader assemble: {0}")]
    Assemble(#[from] AssembleError),
    #[error("module too large: {0} > {1} bytes (bank {2})")]
    ModuleTooLarge(usize, usize, &'static str),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub const LOADER_ASM_SRC: &str = include_str!("../fixtures/asm/m1_loader_65816.asm");

pub fn export_sfc(input: SfcExportInput<'_>) -> Result<SfcExportOutput, SfcExportError> {
    // 1. Compile project A → module_a.bin.
    let module_a = compile_module("A", &input.project_a_path)?;

    // 2. Optional project B → module_b.bin (or clone of A).
    let (module_b, module_b_is_clone_of_a) = match &input.project_b_path {
        Some(p) => (compile_module("B", p)?, false),
        None => {
            let mut clone = module_a.clone();
            clone.project_name = format!("{}_swap_clone", clone.project_name);
            (clone, true)
        }
    };

    if module_a.module_bytes.len() > LOROM_BANK_SIZE {
        return Err(SfcExportError::ModuleTooLarge(
            module_a.module_bytes.len(),
            LOROM_BANK_SIZE,
            "A",
        ));
    }
    if module_b.module_bytes.len() > LOROM_BANK_SIZE {
        return Err(SfcExportError::ModuleTooLarge(
            module_b.module_bytes.len(),
            LOROM_BANK_SIZE,
            "B",
        ));
    }

    // 3. Assemble the 65816 loader.
    std::fs::create_dir_all(&input.working_dir).map_err(|source| SfcExportError::Io {
        path: input.working_dir.clone(),
        source,
    })?;
    let loader_src = input.loader_source_override.unwrap_or(LOADER_ASM_SRC);
    let loader_asm_path = input.working_dir.join("m1_loader_65816.asm");
    std::fs::write(&loader_asm_path, loader_src).map_err(|source| SfcExportError::Io {
        path: loader_asm_path.clone(),
        source,
    })?;

    let scratch_sfc = input.working_dir.join("scratch.sfc");
    let backend = AsarBackend::from_resolution()?;
    backend.assemble(&AssembleInput {
        source_path: loader_asm_path,
        output_image_path: scratch_sfc.clone(),
        working_dir: input.working_dir.clone(),
        expected_output_size: LOROM_MIN_SIZE as u64,
        extra_args: vec![
            "--no-title-check".to_string(),
            "--fix-checksum=on".to_string(),
        ],
    })?;

    let mut sfc_bytes = std::fs::read(&scratch_sfc).map_err(|source| SfcExportError::Io {
        path: scratch_sfc.clone(),
        source,
    })?;

    // 4. Pad to LOROM_MIN_SIZE if needed.
    if sfc_bytes.len() < LOROM_MIN_SIZE {
        sfc_bytes.resize(LOROM_MIN_SIZE, 0);
    }
    if sfc_bytes.len() > LOROM_MAX_SIZE {
        return Err(SfcExportError::ModuleTooLarge(
            sfc_bytes.len(),
            LOROM_MAX_SIZE,
            "SFC file",
        ));
    }

    // 5. Compute loader size BEFORE we overwrite the embedded module
    //    regions: scan bank 0 ($00..$7FBF) for the highest nonzero byte.
    let loader_size_bytes = bank0_last_nonzero_offset(&sfc_bytes) as u32 + 1;

    // 6. Embed module A and module B at fixed bank offsets.
    write_into(&mut sfc_bytes, MODULE_A_FILE_OFFSET, &module_a.module_bytes)?;
    write_into(&mut sfc_bytes, MODULE_B_FILE_OFFSET, &module_b.module_bytes)?;

    // 7. Re-fix the LoROM checksum / complement now that module
    //    bytes have changed the file content.
    fix_lorom_checksum(&mut sfc_bytes);

    // 8. Write final .sfc + report.
    if let Some(parent) = input.out_sfc_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|source| SfcExportError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    std::fs::write(&input.out_sfc_path, &sfc_bytes).map_err(|source| SfcExportError::Io {
        path: input.out_sfc_path.clone(),
        source,
    })?;

    let sfc_sha256 = sha256_hex(&sfc_bytes);

    Ok(SfcExportOutput {
        sfc_path: input.out_sfc_path,
        sfc_size_bytes: sfc_bytes.len() as u32,
        sfc_sha256,
        loader_size_bytes,
        module_a,
        module_b,
        module_b_is_clone_of_a,
    })
}

fn compile_module(
    label: &'static str,
    project_path: &Path,
) -> Result<SfcModuleArtifact, SfcExportError> {
    let project = ProjectV1::load_from_path(project_path)
        .map_err(|source| SfcExportError::Load { label, source })?;
    if let Err(errors) = project.validate() {
        return Err(SfcExportError::Validation { label, errors });
    }

    let project_dir = project_path.parent().unwrap_or_else(|| Path::new("."));
    let mut encoded: Vec<EncodedSample> = Vec::with_capacity(project.sample_pool.len());
    for slot in &project.sample_pool {
        let raw = Path::new(&slot.source.path);
        let audio_path: PathBuf = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            project_dir.join(raw)
        };
        let pcm = decode_to_mono_pcm(&audio_path).map_err(|source| SfcExportError::Decode {
            label,
            sample_id: slot.id.clone(),
            source,
        })?;
        let opts = EncodeOptions::default();
        let (bytes, loop_entry_block) = if slot.looped.enabled {
            match slot.looped.start_sample {
                Some(start) => {
                    let r = encode_looped(&pcm, start, &opts).map_err(|source| {
                        SfcExportError::Encode {
                            label,
                            sample_id: slot.id.clone(),
                            source,
                        }
                    })?;
                    (r.bytes, Some(start / 16))
                }
                None => (brr_encode(&pcm, &opts).bytes, None),
            }
        } else {
            (brr_encode(&pcm, &opts).bytes, None)
        };
        encoded.push(EncodedSample {
            sample_id: slot.id.clone(),
            bytes,
            loop_entry_block,
        });
    }

    let shadow = packer_pack(PackInput {
        project: project.clone(),
        encoded_samples: encoded.clone(),
        driver_code: Vec::new(),
    })
    .map_err(|source| SfcExportError::Pack { label, source })?;

    let work = tempfile::tempdir().map_err(|source| SfcExportError::Io {
        path: PathBuf::from("<tempdir>"),
        source,
    })?;
    let driver_out = driver_build(DriverBuildInput {
        project: &project,
        map_report: &shadow.map_report,
        source_override: None,
        working_dir: work.path().to_path_buf(),
    })
    .map_err(|source| SfcExportError::Driver { label, source })?;

    let real_pack = packer_pack(PackInput {
        project: project.clone(),
        encoded_samples: encoded,
        driver_code: driver_out.driver_code.clone(),
    })
    .map_err(|source| SfcExportError::Pack { label, source })?;

    let echo_enabled = project.master_echo.enabled;
    let module = write_module(ModuleWriteInput {
        aram_image: &real_pack.aram_image,
        map_report: &real_pack.map_report,
        echo_enabled,
    })
    .map_err(|source| SfcExportError::Module { label, source })?;

    Ok(SfcModuleArtifact {
        project_name: project.project.name.clone(),
        module_bytes: module.bytes,
        module_file_sha256: module.module_file_sha256,
        module_in_file_sha256: module.content_sha256_zeroed,
    })
}

fn write_into(buf: &mut [u8], offset: usize, bytes: &[u8]) -> Result<(), SfcExportError> {
    if offset + bytes.len() > buf.len() {
        return Err(SfcExportError::ModuleTooLarge(
            offset + bytes.len(),
            buf.len(),
            "embed range",
        ));
    }
    buf[offset..offset + bytes.len()].copy_from_slice(bytes);
    Ok(())
}

fn bank0_last_nonzero_offset(buf: &[u8]) -> usize {
    let end = LOROM_HEADER_BASE.min(buf.len());
    for i in (0..end).rev() {
        if buf[i] != 0 {
            return i;
        }
    }
    0
}

/// Recompute the LoROM checksum (sum of all bytes mod $10000) and
/// its complement, write them at $7FDC..$7FDF. Per fullsnes,
/// fixed-checksum tools temporarily zero the four bytes before
/// summing — otherwise the previous checksum bytes inflate the new
/// one. We mirror that.
pub fn fix_lorom_checksum(buf: &mut [u8]) {
    if buf.len() <= LOROM_HEADER_CHECKSUM_OFFSET + 1 {
        return;
    }
    // Zero out the four checksum bytes.
    for off in 0..4 {
        buf[LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET + off] = 0;
    }
    let sum: u32 = buf.iter().map(|&b| b as u32).sum();
    let checksum = (sum & 0xFFFF) as u16;
    let complement = !checksum;
    buf[LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET..LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET + 2]
        .copy_from_slice(&complement.to_le_bytes());
    buf[LOROM_HEADER_CHECKSUM_OFFSET..LOROM_HEADER_CHECKSUM_OFFSET + 2]
        .copy_from_slice(&checksum.to_le_bytes());
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    let mut s = String::with_capacity(64);
    for b in d {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_complement_invariant() {
        let mut buf = vec![0u8; LOROM_MIN_SIZE];
        // Stamp some bytes.
        for (i, b) in buf.iter_mut().enumerate().take(0x1000) {
            *b = (i & 0xFF) as u8;
        }
        fix_lorom_checksum(&mut buf);
        let comp = u16::from_le_bytes(
            buf[LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET
                ..LOROM_HEADER_CHECKSUM_COMPLEMENT_OFFSET + 2]
                .try_into()
                .unwrap(),
        );
        let sum = u16::from_le_bytes(
            buf[LOROM_HEADER_CHECKSUM_OFFSET..LOROM_HEADER_CHECKSUM_OFFSET + 2]
                .try_into()
                .unwrap(),
        );
        assert_eq!(comp ^ sum, 0xFFFF);
    }
}
