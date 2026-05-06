//! SPC v0.30 file format builder, structural verifier, and the
//! M0 smoke initial-state contract.
//!
//! ## Layout (per fullsnes / vspcplay reference)
//!
//! ```text
//!   0x00000..0x00100   header  (256 B)
//!     0x00..0x21         33-byte ASCII magic "SNES-SPC700 Sound File Data v0.30"
//!     0x21..0x23         fixed bytes 0x1A 0x1A
//!     0x23               ID666 indicator: 0x1A = present, 0x1B = absent
//!     0x24               minor version (= 30, byte value 0x1E)
//!     0x25..0x27         PC, little-endian
//!     0x27               A
//!     0x28               X
//!     0x29               Y
//!     0x2A               PSW
//!     0x2B               SP (low byte)
//!     0x2C..0x2E         reserved (0x00 0x00)
//!     0x2E..0x100        ID666 tag region (210 B, zero-filled when absent)
//!   0x00100..0x10100   ARAM (64 KB)
//!   0x10100..0x10180   DSP registers (128 B)
//!   0x10180..0x101C0   unused (64 B, zero-filled)
//!   0x101C0..0x10200   extra RAM ($FFC0..$FFFF shadow when IPL ROM is mapped)
//! ```
//!
//! Total size: 0x10200 = 66,048 bytes.
//!
//! ## M0 smoke state contract
//!
//! See SPEC §19.3. The M0 smoke .spc boots into a state that is
//! silent, deterministic, and obviously running: PC = $0200, all
//! GPRs zero, SP = $EF, FLG = $60 (Mute amp + Echo write disable),
//! ID666 absent. Captured here as constants so tests can pin every
//! byte that ships in our M0 .spc.

use std::path::Path;

use thiserror::Error;

use crate::asm::sha256_hex;

// =============================================================================
// Layout constants
// =============================================================================

pub const SPC_FILE_SIZE: usize = 0x10200;
pub const SPC_HEADER_SIZE: usize = 0x100;
pub const SPC_ARAM_OFFSET: usize = 0x100;
pub const SPC_ARAM_SIZE: usize = 0x10000;
pub const SPC_DSP_OFFSET: usize = 0x10100;
pub const SPC_DSP_SIZE: usize = 0x80;
/// 64-byte gap between DSP regs and Extra RAM; zero-filled.
pub const SPC_UNUSED_OFFSET: usize = 0x10180;
pub const SPC_UNUSED_SIZE: usize = 0x40;
pub const SPC_EXTRA_RAM_OFFSET: usize = 0x101C0;
pub const SPC_EXTRA_RAM_SIZE: usize = 0x40;

/// 33-byte ASCII magic at file offset 0.
pub const SPC_MAGIC: &[u8; 33] = b"SNES-SPC700 Sound File Data v0.30";

/// Header byte 0x23. Per spec, 0x1A = ID666 tag present, 0x1B = absent.
pub const SPC_ID666_PRESENT: u8 = 0x1A;
pub const SPC_ID666_ABSENT: u8 = 0x1B;

/// Header byte 0x24. Minor version 30 (= 0x1E).
pub const SPC_MINOR_VERSION: u8 = 0x1E;

// =============================================================================
// Types
// =============================================================================

/// SPC700 CPU state captured at file generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpcCpuState {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub psw: u8,
    pub sp: u8,
}

/// Inputs needed to assemble a complete SPC v0.30 file.
#[derive(Debug, Clone)]
pub struct SpcImage {
    pub cpu: SpcCpuState,
    /// Must be exactly [`SPC_ARAM_SIZE`] bytes.
    pub aram: Vec<u8>,
    pub dsp_regs: [u8; SPC_DSP_SIZE],
    pub extra_ram: [u8; SPC_EXTRA_RAM_SIZE],
}

/// Failure modes for [`SpcImage::to_bytes`] and [`SpcImage::write_to_path`].
#[derive(Debug, Error)]
pub enum SpcBuildError {
    #[error("aram length {actual} != expected {expected}")]
    AramSize { expected: usize, actual: usize },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl SpcImage {
    /// Encode this image as a 66,048-byte SPC v0.30 buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SpcBuildError> {
        if self.aram.len() != SPC_ARAM_SIZE {
            return Err(SpcBuildError::AramSize {
                expected: SPC_ARAM_SIZE,
                actual: self.aram.len(),
            });
        }
        let mut buf = vec![0u8; SPC_FILE_SIZE];

        // Header.
        buf[0..0x21].copy_from_slice(SPC_MAGIC);
        buf[0x21] = 0x1A;
        buf[0x22] = 0x1A;
        buf[0x23] = SPC_ID666_ABSENT;
        buf[0x24] = SPC_MINOR_VERSION;
        let pc = self.cpu.pc.to_le_bytes();
        buf[0x25] = pc[0];
        buf[0x26] = pc[1];
        buf[0x27] = self.cpu.a;
        buf[0x28] = self.cpu.x;
        buf[0x29] = self.cpu.y;
        buf[0x2A] = self.cpu.psw;
        buf[0x2B] = self.cpu.sp;
        // 0x2C..0x2E reserved, stay zero.
        // 0x2E..0x100 ID666 tag region, zero-filled (indicator=absent).

        // ARAM.
        buf[SPC_ARAM_OFFSET..SPC_ARAM_OFFSET + SPC_ARAM_SIZE].copy_from_slice(&self.aram);

        // DSP regs.
        buf[SPC_DSP_OFFSET..SPC_DSP_OFFSET + SPC_DSP_SIZE].copy_from_slice(&self.dsp_regs);

        // 0x10180..0x101C0 stays zero.

        // Extra RAM.
        buf[SPC_EXTRA_RAM_OFFSET..SPC_EXTRA_RAM_OFFSET + SPC_EXTRA_RAM_SIZE]
            .copy_from_slice(&self.extra_ram);

        Ok(buf)
    }

    /// Write the encoded SPC to `path`, creating parent directories
    /// as needed.
    pub fn write_to_path(&self, path: &Path) -> Result<(), SpcBuildError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

// =============================================================================
// M0 smoke contract — SPEC §19.3
// =============================================================================

/// Initial CPU state for the M0 smoke SPC. Matches SPEC §19.3.
pub const SMOKE_CPU_STATE: SpcCpuState = SpcCpuState {
    pc: 0x0200,
    a: 0,
    x: 0,
    y: 0,
    psw: 0,
    sp: 0xEF,
};

/// FLG register value in the M0 smoke DSP state. Matches SPEC §19.3:
/// bit 6 (Mute amp) | bit 5 (Echo write disable).
pub const SMOKE_FLG: u8 = 0x60;

/// DSP register `$6C` (FLG).
pub const DSP_FLG_REG: usize = 0x6C;

/// Build the M0 smoke DSP register block: `FLG=$60`, all else zero.
pub fn smoke_dsp_regs() -> [u8; SPC_DSP_SIZE] {
    let mut dsp = [0u8; SPC_DSP_SIZE];
    dsp[DSP_FLG_REG] = SMOKE_FLG;
    dsp
}

/// Build the M0 smoke `SpcImage` from an ARAM image.
pub fn build_smoke_image(aram: Vec<u8>) -> Result<SpcImage, SpcBuildError> {
    if aram.len() != SPC_ARAM_SIZE {
        return Err(SpcBuildError::AramSize {
            expected: SPC_ARAM_SIZE,
            actual: aram.len(),
        });
    }
    Ok(SpcImage {
        cpu: SMOKE_CPU_STATE,
        aram,
        dsp_regs: smoke_dsp_regs(),
        extra_ram: [0u8; SPC_EXTRA_RAM_SIZE],
    })
}

// =============================================================================
// M1 contract — sample_basic driver expects to write the DSP itself
// =============================================================================

/// Initial CPU state for an M1 audible SPC. Same shape as
/// [`SMOKE_CPU_STATE`] (PC=$0200, GPRs=0, SP=$EF, PSW=0) — only
/// the DSP block differs. Locked at M1.5.
pub const M1_CPU_STATE: SpcCpuState = SMOKE_CPU_STATE;

/// Build an M1 audible `SpcImage` from a packed ARAM image. DSP
/// registers are all zero; the driver's first instruction at
/// `$0200` writes `FLG=$60` (mute amp + echo write disable)
/// before unmuting later in init, so there's no audible glitch
/// from the zero-filled boot state.
pub fn build_m1_image(aram: Vec<u8>) -> Result<SpcImage, SpcBuildError> {
    if aram.len() != SPC_ARAM_SIZE {
        return Err(SpcBuildError::AramSize {
            expected: SPC_ARAM_SIZE,
            actual: aram.len(),
        });
    }
    Ok(SpcImage {
        cpu: M1_CPU_STATE,
        aram,
        dsp_regs: [0u8; SPC_DSP_SIZE],
        extra_ram: [0u8; SPC_EXTRA_RAM_SIZE],
    })
}

// =============================================================================
// Structural verifier — observation only, never assertion
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpcStructure {
    pub file_size: usize,
    pub magic_ok: bool,
    pub minor_version: u8,
    pub id666_present: bool,
    pub cpu: SpcCpuState,
    pub aram_sha256: String,
    pub dsp_sha256: String,
    pub extra_ram_sha256: String,
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("file size {actual} != expected {expected}")]
    FileSize { expected: usize, actual: usize },
}

/// Parse an SPC v0.30 buffer and report observed structure. Reports
/// `magic_ok: false` (instead of erroring) when the magic disagrees;
/// the caller decides what's fatal.
pub fn verify_structure(spc_bytes: &[u8]) -> Result<SpcStructure, VerifyError> {
    if spc_bytes.len() != SPC_FILE_SIZE {
        return Err(VerifyError::FileSize {
            expected: SPC_FILE_SIZE,
            actual: spc_bytes.len(),
        });
    }

    let magic_ok = &spc_bytes[0..0x21] == SPC_MAGIC.as_slice();
    let minor_version = spc_bytes[0x24];
    let id666_present = match spc_bytes[0x23] {
        SPC_ID666_PRESENT => true,
        SPC_ID666_ABSENT => false,
        _ => false, // unknown indicator → treat as absent for observation
    };
    let cpu = SpcCpuState {
        pc: u16::from_le_bytes([spc_bytes[0x25], spc_bytes[0x26]]),
        a: spc_bytes[0x27],
        x: spc_bytes[0x28],
        y: spc_bytes[0x29],
        psw: spc_bytes[0x2A],
        sp: spc_bytes[0x2B],
    };

    let aram_sha256 = sha256_hex(&spc_bytes[SPC_ARAM_OFFSET..SPC_ARAM_OFFSET + SPC_ARAM_SIZE]);
    let dsp_sha256 = sha256_hex(&spc_bytes[SPC_DSP_OFFSET..SPC_DSP_OFFSET + SPC_DSP_SIZE]);
    let extra_ram_sha256 =
        sha256_hex(&spc_bytes[SPC_EXTRA_RAM_OFFSET..SPC_EXTRA_RAM_OFFSET + SPC_EXTRA_RAM_SIZE]);

    Ok(SpcStructure {
        file_size: spc_bytes.len(),
        magic_ok,
        minor_version,
        id666_present,
        cpu,
        aram_sha256,
        dsp_sha256,
        extra_ram_sha256,
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn smoke_with_zero_aram() -> SpcImage {
        build_smoke_image(vec![0u8; SPC_ARAM_SIZE]).unwrap()
    }

    #[test]
    fn build_smoke_rejects_wrong_aram_size() {
        let too_small = vec![0u8; SPC_ARAM_SIZE - 1];
        match build_smoke_image(too_small) {
            Err(SpcBuildError::AramSize { expected, actual }) => {
                assert_eq!(expected, SPC_ARAM_SIZE);
                assert_eq!(actual, SPC_ARAM_SIZE - 1);
            }
            other => panic!("unexpected: {other:?}"),
        }

        let too_big = vec![0u8; SPC_ARAM_SIZE + 1];
        assert!(matches!(
            build_smoke_image(too_big),
            Err(SpcBuildError::AramSize { .. })
        ));
    }

    #[test]
    fn to_bytes_yields_exact_file_size() {
        let img = smoke_with_zero_aram();
        let bytes = img.to_bytes().unwrap();
        assert_eq!(bytes.len(), SPC_FILE_SIZE);
        assert_eq!(SPC_FILE_SIZE, 66048);
    }

    #[test]
    fn magic_string_is_at_offset_zero() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_eq!(&bytes[0..0x21], SPC_MAGIC.as_slice());
    }

    #[test]
    fn fixed_bytes_at_0x21_0x22_are_0x1a() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_eq!(bytes[0x21], 0x1A);
        assert_eq!(bytes[0x22], 0x1A);
    }

    #[test]
    fn id666_indicator_is_absent_for_smoke() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_eq!(bytes[0x23], SPC_ID666_ABSENT);
    }

    #[test]
    fn minor_version_byte_is_30() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_eq!(bytes[0x24], 0x1E);
        assert_eq!(SPC_MINOR_VERSION, 30);
    }

    #[test]
    fn pc_is_le_encoded_at_0x25_0x26() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_eq!(bytes[0x25], 0x00, "PC low byte");
        assert_eq!(bytes[0x26], 0x02, "PC high byte");
    }

    #[test]
    fn cpu_register_bytes_at_0x27_to_0x2b() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_eq!(bytes[0x27], 0x00, "A");
        assert_eq!(bytes[0x28], 0x00, "X");
        assert_eq!(bytes[0x29], 0x00, "Y");
        assert_eq!(bytes[0x2A], 0x00, "PSW");
        assert_eq!(bytes[0x2B], 0xEF, "SP");
    }

    #[test]
    fn dsp_flg_is_at_offset_0x1016c() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_eq!(bytes[SPC_DSP_OFFSET + DSP_FLG_REG], SMOKE_FLG);
        assert_eq!(SPC_DSP_OFFSET + DSP_FLG_REG, 0x1016C);
        assert_eq!(SMOKE_FLG, 0x60);
    }

    fn assert_all_zero(slice: &[u8], label: &str) {
        if let Some((i, b)) = slice.iter().enumerate().find(|(_, b)| **b != 0) {
            panic!("{label}: nonzero byte at offset {i:#X} = {b:#04X}");
        }
    }

    #[test]
    fn id666_region_zero_filled_when_absent() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_all_zero(&bytes[0x2E..0x100], "id666 region");
    }

    #[test]
    fn extra_ram_zero_filled_for_smoke() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_all_zero(
            &bytes[SPC_EXTRA_RAM_OFFSET..SPC_EXTRA_RAM_OFFSET + SPC_EXTRA_RAM_SIZE],
            "extra ram",
        );
    }

    #[test]
    fn unused_gap_zero_filled() {
        let bytes = smoke_with_zero_aram().to_bytes().unwrap();
        assert_all_zero(
            &bytes[SPC_UNUSED_OFFSET..SPC_UNUSED_OFFSET + SPC_UNUSED_SIZE],
            "unused gap",
        );
    }

    #[test]
    fn aram_section_carries_input_bytes() {
        let mut aram = vec![0u8; SPC_ARAM_SIZE];
        aram[0x0200] = 0x00;
        aram[0x0201] = 0x2F;
        aram[0x0202] = 0xFD;
        let img = build_smoke_image(aram).unwrap();
        let bytes = img.to_bytes().unwrap();
        assert_eq!(bytes[SPC_ARAM_OFFSET + 0x0200], 0x00);
        assert_eq!(bytes[SPC_ARAM_OFFSET + 0x0201], 0x2F);
        assert_eq!(bytes[SPC_ARAM_OFFSET + 0x0202], 0xFD);
    }

    #[test]
    fn round_trip_through_verify_structure() {
        let mut aram = vec![0u8; SPC_ARAM_SIZE];
        aram[0x0200] = 0x00;
        aram[0x0201] = 0x2F;
        aram[0x0202] = 0xFD;
        let img = build_smoke_image(aram.clone()).unwrap();
        let bytes = img.to_bytes().unwrap();
        let s = verify_structure(&bytes).unwrap();

        assert_eq!(s.file_size, SPC_FILE_SIZE);
        assert!(s.magic_ok);
        assert_eq!(s.minor_version, SPC_MINOR_VERSION);
        assert!(!s.id666_present);
        assert_eq!(s.cpu, SMOKE_CPU_STATE);
        assert_eq!(s.aram_sha256, sha256_hex(&aram));
        assert_eq!(s.dsp_sha256, sha256_hex(&smoke_dsp_regs()));
        assert_eq!(s.extra_ram_sha256, sha256_hex(&[0u8; SPC_EXTRA_RAM_SIZE]));
    }

    #[test]
    fn verify_structure_rejects_wrong_size() {
        match verify_structure(&[0u8; 10]) {
            Err(VerifyError::FileSize { expected, actual }) => {
                assert_eq!(expected, SPC_FILE_SIZE);
                assert_eq!(actual, 10);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn verify_structure_reports_magic_ok_false_on_garbage() {
        let mut bytes = vec![0u8; SPC_FILE_SIZE];
        bytes[0..0x21].fill(b'X');
        let s = verify_structure(&bytes).unwrap();
        assert!(!s.magic_ok);
    }
}
