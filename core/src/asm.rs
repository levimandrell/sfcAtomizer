//! Assembler abstraction for the SFC Wave Compiler.
//!
//! SPEC §4 calls for an `AssemblerBackend` trait so the compiler can
//! swap asar for WLA-DX (or another backend) without touching the
//! build pipeline. M0.3 ships only [`AsarBackend`]; WLA-DX is
//! forward-looking and not implemented in this pass.
//!
//! ARAM layout (which addresses get which bytes) is the compiler's
//! concern, not the assembler's, per SPEC §15.1. The backend's job
//! is to take a source file and emit the corresponding bytes at the
//! right file offsets in a 64 KB ARAM image.
//!
//! ## asar specifics — discovered in M0.3 probing
//!
//! Asar is fundamentally a SNES ROM patcher; out of the box it
//! expects a SNES bus address space and rejects raw 64 KB inputs.
//! Two quirks must be coaxed:
//!
//! 1. **Address mapping.** With `org $0200`, asar reports
//!    `Esnes_address_doesnt_map_to_rom`. The .asm must declare a
//!    SNES mapper (`lorom`) and use `org $008200` so the SNES bus
//!    address and the file offset coincide, plus `base $0200` so
//!    SPC700 labels resolve to ARAM-relative addresses.
//! 2. **Checksum injection.** Asar's default behavior writes 4
//!    bytes of LoROM checksum into the output at `$7FDC..$7FDF`.
//!    `--fix-checksum=off` disables that. Without it our ARAM image
//!    is corrupted in the middle of what the compiler intends to be
//!    sequence-bytecode or sample-pool space.
//!
//! Locked invocation: `asar --no-title-check --fix-checksum=off
//! <source.asm> <output.bin>`. On clean success asar emits nothing
//! on either stream and exits 0.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// 64 KB. Echoes [`crate::report::AramMapReport::TOTAL_ARAM`] but
/// typed as `u64` for filesystem-size comparisons.
pub const ARAM_SIZE: u64 = 65536;

/// Inputs to one assemble call.
#[derive(Debug, Clone)]
pub struct AssembleInput {
    /// Source `.asm` file the assembler reads.
    pub source_path: PathBuf,
    /// Path where the assembled 64 KB ARAM image should land.
    pub output_image_path: PathBuf,
    /// Working directory for the assembler invocation.
    pub working_dir: PathBuf,
}

/// Successful assemble result. All fields populated on success.
#[derive(Debug, Clone)]
pub struct AssembleOutput {
    pub backend: String,
    pub version: String,
    pub output_image_path: PathBuf,
    pub output_bytes: u64,
    pub output_image_sha256: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Failure modes.
#[derive(Debug, Error)]
pub enum AssembleError {
    #[error("assembler not resolved: {hint}")]
    NotResolved { hint: String },
    #[error("assembler exited {code}: {stderr}")]
    NonZeroExit { code: i32, stderr: String },
    #[error("output image missing or unreadable at {path}: {source}")]
    OutputMissing {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("output image wrong size: expected {expected}, got {actual}")]
    WrongOutputSize { expected: u64, actual: u64 },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Pluggable assembler interface. M0.3 ships [`AsarBackend`]; WLA-DX
/// is forward-looking.
pub trait AssemblerBackend {
    fn name(&self) -> &'static str;
    /// Best-effort version string. Probe failures yield `Ok("unknown")`
    /// rather than an error — version is informational, not gating.
    fn version(&self) -> Result<String, AssembleError>;
    fn assemble(&self, input: &AssembleInput) -> Result<AssembleOutput, AssembleError>;
}

/// Asar-backed assembler. Resolves the asar executable per SPEC §17.1.
pub struct AsarBackend {
    pub asar_path: PathBuf,
}

impl AsarBackend {
    /// Resolve asar via [`crate::tools::resolve_asar`] and return a
    /// constructed backend, or `NotResolved` if asar is missing.
    pub fn from_resolution() -> Result<Self, AssembleError> {
        let r = crate::tools::resolve_asar();
        if !r.resolved {
            return Err(AssembleError::NotResolved {
                hint: "set SFCWC_ASAR or put asar on PATH (see SPEC §17.1)".to_string(),
            });
        }
        Ok(Self {
            asar_path: r.path.expect("resolved => path"),
        })
    }
}

impl AssemblerBackend for AsarBackend {
    fn name(&self) -> &'static str {
        "asar"
    }

    fn version(&self) -> Result<String, AssembleError> {
        let out = Command::new(&self.asar_path)
            .arg("--version")
            .output()
            .map_err(AssembleError::Io)?;
        Ok(parse_version_line(&String::from_utf8_lossy(&out.stdout))
            .unwrap_or_else(|| "unknown".to_string()))
    }

    fn assemble(&self, input: &AssembleInput) -> Result<AssembleOutput, AssembleError> {
        // 1. Pre-create a 64 KB zero-filled scratch file. asar will
        //    patch this in place; the LoROM-mapped writes from the
        //    .asm land at file-offset == ARAM-address.
        if let Some(parent) = input.output_image_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(AssembleError::Io)?;
            }
        }
        let zeros = vec![0u8; ARAM_SIZE as usize];
        std::fs::write(&input.output_image_path, &zeros).map_err(AssembleError::Io)?;

        // 2. Invoke asar. Both flags are necessary; see module docs.
        let output = Command::new(&self.asar_path)
            .arg("--no-title-check")
            .arg("--fix-checksum=off")
            .arg(&input.source_path)
            .arg(&input.output_image_path)
            .current_dir(&input.working_dir)
            .output()
            .map_err(AssembleError::Io)?;
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(AssembleError::NonZeroExit {
                code: exit_code,
                stderr,
            });
        }

        // 3. Read assembled image and verify size. asar can silently
        //    expand the file if `org` lands outside bank 0; that is
        //    a hard error here.
        let bytes =
            std::fs::read(&input.output_image_path).map_err(|e| AssembleError::OutputMissing {
                path: input.output_image_path.clone(),
                source: e,
            })?;
        let actual_size = bytes.len() as u64;
        if actual_size != ARAM_SIZE {
            return Err(AssembleError::WrongOutputSize {
                expected: ARAM_SIZE,
                actual: actual_size,
            });
        }

        // 4. SHA-256 of the full 64 KB image.
        let sha256 = sha256_hex(&bytes);

        // 5. Backend version is informational; failure → "unknown".
        let version = self.version().unwrap_or_else(|_| "unknown".to_string());

        Ok(AssembleOutput {
            backend: self.name().to_string(),
            version,
            output_image_path: input.output_image_path.clone(),
            output_bytes: actual_size,
            output_image_sha256: sha256,
            stdout,
            stderr,
            exit_code,
        })
    }
}

/// Parse asar's `--version` first non-empty line. Asar's actual
/// output is `"Asar 1.91, originally developed by Alcaro, ..."` —
/// we keep the whole line; downstream report fields don't try to
/// extract the bare semver.
pub fn parse_version_line(stdout: &str) -> Option<String> {
    stdout.lines().find_map(|l| {
        let t = l.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

/// Hex-encoded SHA-256 of `bytes`. Lowercase, 64 chars.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Convenience: SHA-256 of a file's bytes.
pub fn sha256_hex_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version_first_line() {
        let s = "Asar 1.91, originally developed by Alcaro, ...\nSource code: https://...\n";
        assert_eq!(
            parse_version_line(s).as_deref(),
            Some("Asar 1.91, originally developed by Alcaro, ...")
        );
    }

    #[test]
    fn parse_version_skips_blank_lines() {
        assert_eq!(
            parse_version_line("\n\n   \nasar 1.0\n").as_deref(),
            Some("asar 1.0")
        );
    }

    #[test]
    fn parse_version_empty_is_none() {
        assert!(parse_version_line("").is_none());
        assert!(parse_version_line("\n   \n").is_none());
    }

    #[test]
    fn sha256_hex_known_vector() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // SHA-256("abc")
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn aram_size_is_64_kib() {
        assert_eq!(ARAM_SIZE, 65536);
        assert_eq!(
            ARAM_SIZE,
            u64::from(crate::report::AramMapReport::TOTAL_ARAM)
        );
    }

    #[test]
    fn from_resolution_returns_not_resolved_hint_on_missing_asar() {
        // We can't deterministically scrub SFCWC_ASAR + PATH inside a
        // unit test without env mutation; if asar is currently
        // resolvable on this host, the call succeeds and we exit
        // early. The integration test exercises the missing-asar
        // branch in a process-isolated subprocess.
        match AsarBackend::from_resolution() {
            Ok(b) => {
                assert_eq!(b.name(), "asar");
                assert!(b.asar_path.is_file());
            }
            Err(AssembleError::NotResolved { hint }) => {
                assert!(hint.contains("SFCWC_ASAR"));
            }
            Err(other) => panic!("unexpected error from from_resolution: {other}"),
        }
    }
}
