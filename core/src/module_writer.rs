//! M1 `module.bin` writer (SPEC §19.4).
//!
//! Converts a packed 64 KB ARAM image + its [`AramMapReport`] into
//! a sparse-block module file the 65816 loader will upload to the
//! S-DSP. Skips runtime regions (`$0000..$01FF`), the IPL ROM
//! pad/shadow at the top of ARAM, and any `Free` region; emits one
//! block per relevant payload region (driver / source dir / BRR
//! pool / optional zero-filled echo buffer).
//!
//! ### File layout
//!
//! ```text
//! 0x00..0x08  magic = "SFCWCM1\0"
//! 0x08..0x0A  schema_version (u16 LE) = 1
//! 0x0A..0x0C  header_len (u16 LE) = 64
//! 0x0C..0x0E  block_count (u16 LE)
//! 0x0E..0x10  entrypoint (u16 LE) = $0200
//! 0x10..0x14  block_table_offset (u32 LE) = 64
//! 0x14..0x18  data_offset (u32 LE) = 64 + block_count * 8
//! 0x18..0x1C  total_file_len (u32 LE)
//! 0x1C..0x1E  flags (u16 LE; bit 0 = echo_enabled_for_module)
//! 0x1E..0x20  reserved (u16 LE) = 0
//! 0x20..0x40  content_sha256_zeroed (SHA-256 of file with these
//!             32 bytes set to 0 — the §19.4 self-reference workaround)
//!
//! 0x40..0x40+8N  block table (8 bytes per block: dest_addr u16,
//!                length u16, data_offset u32, all LE)
//! 0x40+8N..end  block data
//! ```

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::report::{AramKind, AramMapReport};

pub const MODULE_MAGIC: &[u8; 8] = b"SFCWCM1\0";
pub const MODULE_SCHEMA_VERSION: u16 = 1;
pub const MODULE_HEADER_LEN: u16 = 64;
pub const MODULE_ENTRYPOINT_M1: u16 = 0x0200;
pub const MODULE_HEADER_SHA_OFFSET: usize = 0x20;
pub const MODULE_HEADER_SHA_LEN: usize = 32;
pub const BLOCK_ENTRY_LEN: usize = 8;

#[derive(Debug, Clone)]
pub struct ModuleWriteInput<'a> {
    pub aram_image: &'a [u8; 0x10000],
    pub map_report: &'a AramMapReport,
    pub echo_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct ModuleWriteOutput {
    /// Full `module.bin` contents.
    pub bytes: Vec<u8>,
    pub block_count: u32,
    pub total_bytes: u32,
    /// SHA-256 of the literal full file. Lives in the M1 manifest,
    /// not inside the file. Computed AFTER patching the in-file SHA
    /// — once patched, the bytes at `$20..$40` are no longer zero.
    pub module_file_sha256: String,
    /// SHA-256 of the file with bytes `$20..$40` set to 0 — the
    /// value stored at `$20..$40` inside the file itself (§19.4
    /// self-reference workaround).
    pub content_sha256_zeroed: String,
}

#[derive(Debug, Error)]
pub enum ModuleWriteError {
    #[error("block at ${addr:04X} has zero length")]
    EmptyBlock { addr: u16 },
    #[error("block at ${addr:04X} below driver area ($0200)")]
    BlockBelowDriverArea { addr: u16 },
    #[error("block at ${addr:04X} (length {len}) targets hardware I/O range $00F0..$00FF")]
    BlockTargetsHardwareIo { addr: u16, len: u32 },
    #[error("blocks overlap: ${a_start:04X}..${a_end:04X} and ${b_start:04X}..${b_end:04X}")]
    BlockOverlap {
        a_start: u16,
        a_end: u16,
        b_start: u16,
        b_end: u16,
    },
    #[error("region ${start:04X}..${end:04X} extends past 64 KB")]
    RegionPastEnd { start: u32, end: u32 },
    #[error("invalid region address {addr:?}: {reason}")]
    InvalidRegionAddr { addr: String, reason: String },
}

#[derive(Debug, Clone, Copy)]
struct PendingBlock {
    dest_addr: u16,
    length: u32,
}

pub fn write_module(input: ModuleWriteInput<'_>) -> Result<ModuleWriteOutput, ModuleWriteError> {
    let mut blocks: Vec<PendingBlock> = Vec::new();
    let mut echo_block: Option<PendingBlock> = None;

    for region in &input.map_report.regions {
        match region.kind {
            // Skip runtime / hardware / pad / free regions entirely.
            AramKind::FixedRuntime | AramKind::FixedHardware | AramKind::Free => continue,
            // Echo buffer is zero-filled in the file; only emit when
            // echo is enabled for this module.
            AramKind::EchoBuffer => {
                if !input.echo_enabled {
                    continue;
                }
                let (start, end_inclusive) = parse_addr_range(&region.start, &region.end)?;
                let length = end_inclusive as u32 - start as u32 + 1;
                if length == 0 {
                    return Err(ModuleWriteError::EmptyBlock { addr: start });
                }
                echo_block = Some(PendingBlock {
                    dest_addr: start,
                    length,
                });
            }
            // Real payload regions — driver, source dir, BRR pool,
            // and the various M2+ region kinds we don't generate
            // yet but route the same way.
            AramKind::DriverCode
            | AramKind::SourceDirectory
            | AramKind::PitchTables
            | AramKind::SequenceData
            | AramKind::InstrumentMetadata
            | AramKind::SampleBrrPool
            | AramKind::SynthAtomPool => {
                let (start, end_inclusive) = parse_addr_range(&region.start, &region.end)?;
                let length = end_inclusive as u32 - start as u32 + 1;
                if length == 0 {
                    return Err(ModuleWriteError::EmptyBlock { addr: start });
                }
                if start < MODULE_ENTRYPOINT_M1 {
                    return Err(ModuleWriteError::BlockBelowDriverArea { addr: start });
                }
                if intersects_hardware_io(start, length) {
                    return Err(ModuleWriteError::BlockTargetsHardwareIo {
                        addr: start,
                        len: length,
                    });
                }
                blocks.push(PendingBlock {
                    dest_addr: start,
                    length,
                });
            }
        }
    }

    if let Some(b) = echo_block {
        blocks.push(b);
    }
    blocks.sort_by_key(|b| b.dest_addr);

    // Validate non-overlap.
    for w in blocks.windows(2) {
        let a = w[0];
        let b = w[1];
        let a_end = a.dest_addr as u32 + a.length;
        if a_end > b.dest_addr as u32 {
            return Err(ModuleWriteError::BlockOverlap {
                a_start: a.dest_addr,
                a_end: (a_end - 1) as u16,
                b_start: b.dest_addr,
                b_end: (b.dest_addr as u32 + b.length - 1) as u16,
            });
        }
    }

    let block_count = blocks.len() as u32;
    let block_table_offset: u32 = MODULE_HEADER_LEN as u32;
    let data_offset: u32 = block_table_offset + block_count * BLOCK_ENTRY_LEN as u32;
    let mut total_bytes: u32 = data_offset;
    let mut block_data_offsets: Vec<u32> = Vec::with_capacity(blocks.len());
    for b in &blocks {
        block_data_offsets.push(total_bytes);
        total_bytes += b.length;
    }

    let mut bytes = vec![0u8; total_bytes as usize];

    // Header.
    bytes[0..8].copy_from_slice(MODULE_MAGIC);
    bytes[8..10].copy_from_slice(&MODULE_SCHEMA_VERSION.to_le_bytes());
    bytes[10..12].copy_from_slice(&MODULE_HEADER_LEN.to_le_bytes());
    bytes[12..14].copy_from_slice(&(block_count as u16).to_le_bytes());
    bytes[14..16].copy_from_slice(&MODULE_ENTRYPOINT_M1.to_le_bytes());
    bytes[16..20].copy_from_slice(&block_table_offset.to_le_bytes());
    bytes[20..24].copy_from_slice(&data_offset.to_le_bytes());
    bytes[24..28].copy_from_slice(&total_bytes.to_le_bytes());
    let flags: u16 = if input.echo_enabled { 0x0001 } else { 0x0000 };
    bytes[28..30].copy_from_slice(&flags.to_le_bytes());
    // bytes[30..32] reserved = 0
    // bytes[32..64] = content_sha256_zeroed (filled below)

    // Block table + data.
    for (i, b) in blocks.iter().enumerate() {
        let entry_off = block_table_offset as usize + i * BLOCK_ENTRY_LEN;
        bytes[entry_off..entry_off + 2].copy_from_slice(&b.dest_addr.to_le_bytes());
        bytes[entry_off + 2..entry_off + 4].copy_from_slice(&(b.length as u16).to_le_bytes());
        bytes[entry_off + 4..entry_off + 8].copy_from_slice(&block_data_offsets[i].to_le_bytes());

        let data_start = block_data_offsets[i] as usize;
        let data_end = data_start + b.length as usize;
        let aram_start = b.dest_addr as usize;
        let aram_end = aram_start + b.length as usize;
        if aram_end > 0x10000 {
            return Err(ModuleWriteError::RegionPastEnd {
                start: aram_start as u32,
                end: aram_end as u32,
            });
        }
        // Echo block stays zero-filled in the file.
        if !is_echo_dest(input.map_report, b.dest_addr) {
            bytes[data_start..data_end].copy_from_slice(&input.aram_image[aram_start..aram_end]);
        }
    }

    // In-file SHA: hash with bytes $20..$40 zeroed (already zero from
    // initial fill); patch the digest in.
    let in_file_sha = sha256_hex(&bytes);
    bytes[MODULE_HEADER_SHA_OFFSET..MODULE_HEADER_SHA_OFFSET + MODULE_HEADER_SHA_LEN]
        .copy_from_slice(&hex_to_bytes32(&in_file_sha));

    // Full-file SHA: hash AFTER the SHA bytes are patched in.
    let full_sha = sha256_hex(&bytes);

    Ok(ModuleWriteOutput {
        bytes,
        block_count,
        total_bytes,
        module_file_sha256: full_sha,
        content_sha256_zeroed: in_file_sha,
    })
}

fn parse_addr_range(start: &str, end: &str) -> Result<(u16, u16), ModuleWriteError> {
    let parse = |s: &str| -> Result<u16, ModuleWriteError> {
        let stripped = s.trim_start_matches("0x").trim_start_matches("0X");
        u16::from_str_radix(stripped, 16).map_err(|e| ModuleWriteError::InvalidRegionAddr {
            addr: s.to_string(),
            reason: format!("{e}"),
        })
    };
    Ok((parse(start)?, parse(end)?))
}

fn intersects_hardware_io(start: u16, length: u32) -> bool {
    let end_exclusive = start as u32 + length;
    let io_start: u32 = 0x00F0;
    let io_end_exclusive: u32 = 0x0100;
    (start as u32) < io_end_exclusive && end_exclusive > io_start
}

fn is_echo_dest(map: &AramMapReport, addr: u16) -> bool {
    map.echo
        .as_ref()
        .map(|e| e.enabled && (e.esa as u16) << 8 == addr)
        .unwrap_or(false)
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

fn hex_to_bytes32(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        let h = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0);
        *b = h;
    }
    out
}

/// Re-parse a [`ModuleWriteOutput::bytes`] header for verifiers.
/// Observation-only — never asserts; the caller decides what to do
/// with the values.
#[derive(Debug, Clone)]
pub struct ParsedModuleHeader {
    pub magic_ok: bool,
    pub schema_version: u16,
    pub header_len: u16,
    pub block_count: u16,
    pub entrypoint: u16,
    pub block_table_offset: u32,
    pub data_offset: u32,
    pub total_file_len: u32,
    pub flags: u16,
    pub content_sha256_in_file: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct ParsedBlock {
    pub dest_addr: u16,
    pub length: u16,
    pub data_offset: u32,
}

#[derive(Debug, Error)]
pub enum ModuleParseError {
    #[error("module bytes too short: {actual} < 64")]
    TooShort { actual: usize },
    #[error("malformed block table: entry {index} at offset {offset} runs past file")]
    BlockTableOverrun { index: usize, offset: usize },
    #[error("block {index} data range {start}..{end} runs past file ({file_len})")]
    BlockDataOverrun {
        index: usize,
        start: u32,
        end: u32,
        file_len: u32,
    },
}

pub fn parse_module_header(bytes: &[u8]) -> Result<ParsedModuleHeader, ModuleParseError> {
    if bytes.len() < 64 {
        return Err(ModuleParseError::TooShort {
            actual: bytes.len(),
        });
    }
    let magic_ok = &bytes[0..8] == MODULE_MAGIC.as_slice();
    let schema_version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    let header_len = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
    let block_count = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
    let entrypoint = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
    let block_table_offset = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let data_offset = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let total_file_len = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let flags = u16::from_le_bytes(bytes[28..30].try_into().unwrap());
    let mut content_sha256_in_file = [0u8; 32];
    content_sha256_in_file.copy_from_slice(&bytes[32..64]);
    Ok(ParsedModuleHeader {
        magic_ok,
        schema_version,
        header_len,
        block_count,
        entrypoint,
        block_table_offset,
        data_offset,
        total_file_len,
        flags,
        content_sha256_in_file,
    })
}

pub fn parse_module_blocks(
    bytes: &[u8],
    header: &ParsedModuleHeader,
) -> Result<Vec<ParsedBlock>, ModuleParseError> {
    let mut out = Vec::with_capacity(header.block_count as usize);
    for i in 0..header.block_count as usize {
        let off = header.block_table_offset as usize + i * BLOCK_ENTRY_LEN;
        if off + BLOCK_ENTRY_LEN > bytes.len() {
            return Err(ModuleParseError::BlockTableOverrun {
                index: i,
                offset: off,
            });
        }
        let dest_addr = u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap());
        let length = u16::from_le_bytes(bytes[off + 2..off + 4].try_into().unwrap());
        let data_offset = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
        let end = data_offset + length as u32;
        if end as usize > bytes.len() {
            return Err(ModuleParseError::BlockDataOverrun {
                index: i,
                start: data_offset,
                end,
                file_len: bytes.len() as u32,
            });
        }
        out.push(ParsedBlock {
            dest_addr,
            length,
            data_offset,
        });
    }
    Ok(out)
}

/// Recompute the in-file SHA-256: copy the file, zero bytes
/// `$20..$40`, hash. Returns hex.
pub fn recompute_in_file_sha(bytes: &[u8]) -> String {
    let mut copy = bytes.to_vec();
    if copy.len() >= 64 {
        for b in
            &mut copy[MODULE_HEADER_SHA_OFFSET..MODULE_HEADER_SHA_OFFSET + MODULE_HEADER_SHA_LEN]
        {
            *b = 0;
        }
    }
    sha256_hex(&copy)
}

/// Project the `module.bin` blocks back into a 64 KB ARAM image —
/// used by `verify-sfc-modules-audible` to reconstruct what the
/// loader will upload, then render through the snes_spc oracle.
pub fn project_blocks_to_aram(
    bytes: &[u8],
    header: &ParsedModuleHeader,
    blocks: &[ParsedBlock],
) -> [u8; 0x10000] {
    let mut aram = [0u8; 0x10000];
    let _ = header;
    for b in blocks {
        let dest = b.dest_addr as usize;
        let end = dest + b.length as usize;
        let src_start = b.data_offset as usize;
        let src_end = src_start + b.length as usize;
        if end <= 0x10000 && src_end <= bytes.len() {
            aram[dest..end].copy_from_slice(&bytes[src_start..src_end]);
        }
    }
    aram
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        AramEchoSummary, AramKind, AramRegion, AramSourceDirSummary, SCHEMA_VERSION,
    };

    fn aram_image_zeroed() -> Box<[u8; 0x10000]> {
        Box::new([0u8; 0x10000])
    }

    fn region(name: &str, start: u32, end_incl: u32, kind: AramKind) -> AramRegion {
        AramRegion {
            name: name.to_string(),
            start: format!("0x{start:04X}"),
            end: format!("0x{end_incl:04X}"),
            bytes: end_incl - start + 1,
            kind,
        }
    }

    fn map_no_echo() -> AramMapReport {
        AramMapReport {
            schema_version: SCHEMA_VERSION,
            report_type: AramMapReport::REPORT_TYPE.to_string(),
            total_aram: 0x10000,
            regions: vec![
                region("direct_page", 0x0000, 0x00EF, AramKind::FixedRuntime),
                region("hardware_io", 0x00F0, 0x00FF, AramKind::FixedHardware),
                region("stack", 0x0100, 0x01FF, AramKind::FixedRuntime),
                region("driver_code", 0x0200, 0x11FF, AramKind::DriverCode),
                region(
                    "source_directory",
                    0x1200,
                    0x12FF,
                    AramKind::SourceDirectory,
                ),
                region("sample_brr_pool", 0x1300, 0x132F, AramKind::SampleBrrPool),
                region("free", 0x1330, 0xFEFF, AramKind::Free),
                region("ipl_rom_safe_pad", 0xFF00, 0xFFBF, AramKind::FixedHardware),
                region("ipl_rom_shadow", 0xFFC0, 0xFFFF, AramKind::FixedHardware),
            ],
            free_bytes: 0xFEFF - 0x1330 + 1,
            collisions: Vec::new(),
            echo: None,
            source_directory: Some(AramSourceDirSummary {
                source_count: 1,
                bytes: 4,
                padding_bytes: 252,
                start_addr: 0x1200,
            }),
            samples: None,
            warnings: Vec::new(),
        }
    }

    fn map_with_echo_edl4() -> AramMapReport {
        let mut m = map_no_echo();
        // Replace free with shorter free + echo region.
        m.regions = vec![
            region("direct_page", 0x0000, 0x00EF, AramKind::FixedRuntime),
            region("hardware_io", 0x00F0, 0x00FF, AramKind::FixedHardware),
            region("stack", 0x0100, 0x01FF, AramKind::FixedRuntime),
            region("driver_code", 0x0200, 0x11FF, AramKind::DriverCode),
            region(
                "source_directory",
                0x1200,
                0x12FF,
                AramKind::SourceDirectory,
            ),
            region("sample_brr_pool", 0x1300, 0x132F, AramKind::SampleBrrPool),
            region("free", 0x1330, 0xDEFF, AramKind::Free),
            region("echo_buffer", 0xDF00, 0xFEFF, AramKind::EchoBuffer),
            region("ipl_rom_safe_pad", 0xFF00, 0xFFBF, AramKind::FixedHardware),
            region("ipl_rom_shadow", 0xFFC0, 0xFFFF, AramKind::FixedHardware),
        ];
        m.echo = Some(AramEchoSummary {
            enabled: true,
            edl: 4,
            buffer_bytes: 8192,
            hardware_tail_bytes: 4,
            esa: 0xDF,
            percent_of_aram: 12.5,
            writeback_safe: true,
        });
        m
    }

    #[test]
    fn module_bin_layout_byte_exact_no_echo() {
        let aram = aram_image_zeroed();
        let map = map_no_echo();
        let r = write_module(ModuleWriteInput {
            aram_image: &aram,
            map_report: &map,
            echo_enabled: false,
        })
        .unwrap();

        // Magic + version.
        assert_eq!(&r.bytes[0..8], MODULE_MAGIC);
        assert_eq!(u16::from_le_bytes(r.bytes[8..10].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(r.bytes[10..12].try_into().unwrap()), 64);
        // Three blocks: driver / srcdir / brr pool.
        assert_eq!(u16::from_le_bytes(r.bytes[12..14].try_into().unwrap()), 3);
        assert_eq!(
            u16::from_le_bytes(r.bytes[14..16].try_into().unwrap()),
            0x0200
        );
        assert_eq!(u32::from_le_bytes(r.bytes[16..20].try_into().unwrap()), 64);
        assert_eq!(
            u32::from_le_bytes(r.bytes[20..24].try_into().unwrap()),
            64 + 3 * 8
        );
        // total_bytes matches r.bytes.len().
        let total = u32::from_le_bytes(r.bytes[24..28].try_into().unwrap());
        assert_eq!(total as usize, r.bytes.len());
        // Flags echo bit clear.
        assert_eq!(u16::from_le_bytes(r.bytes[28..30].try_into().unwrap()), 0);
        // Reserved zero.
        assert_eq!(u16::from_le_bytes(r.bytes[30..32].try_into().unwrap()), 0);
    }

    #[test]
    fn in_file_sha256_correctly_self_zeroed() {
        let aram = aram_image_zeroed();
        let map = map_no_echo();
        let r = write_module(ModuleWriteInput {
            aram_image: &aram,
            map_report: &map,
            echo_enabled: false,
        })
        .unwrap();
        // Recompute by zeroing the SHA region.
        let recomputed = recompute_in_file_sha(&r.bytes);
        assert_eq!(recomputed, r.content_sha256_zeroed);
        // The 32 bytes inside the file at $20..$40 must equal the
        // recomputed digest.
        let mut expected = [0u8; 32];
        for (i, b) in expected.iter_mut().enumerate() {
            *b = u8::from_str_radix(&recomputed[i * 2..i * 2 + 2], 16).unwrap();
        }
        assert_eq!(&r.bytes[32..64], &expected[..]);
        // Full-file SHA differs from in-file SHA.
        assert_ne!(r.module_file_sha256, r.content_sha256_zeroed);
    }

    #[test]
    fn blocks_sorted_ascending_by_dest_addr() {
        let aram = aram_image_zeroed();
        let map = map_no_echo();
        let r = write_module(ModuleWriteInput {
            aram_image: &aram,
            map_report: &map,
            echo_enabled: false,
        })
        .unwrap();
        let header = parse_module_header(&r.bytes).unwrap();
        let blocks = parse_module_blocks(&r.bytes, &header).unwrap();
        for w in blocks.windows(2) {
            assert!(w[0].dest_addr < w[1].dest_addr);
        }
    }

    #[test]
    fn blocks_skip_free_and_fixed_runtime_regions() {
        let aram = aram_image_zeroed();
        let map = map_no_echo();
        let r = write_module(ModuleWriteInput {
            aram_image: &aram,
            map_report: &map,
            echo_enabled: false,
        })
        .unwrap();
        let header = parse_module_header(&r.bytes).unwrap();
        let blocks = parse_module_blocks(&r.bytes, &header).unwrap();
        for b in &blocks {
            assert!(
                b.dest_addr >= 0x0200,
                "block at ${:04X} below driver area",
                b.dest_addr
            );
            assert!(
                !(b.dest_addr < 0x0100 && (b.dest_addr + b.length) > 0x00F0),
                "block intersects $00F0..$00FF"
            );
        }
        // Driver / srcdir / brr only.
        assert_eq!(blocks.len(), 3);
    }

    #[test]
    fn block_overlap_errors() {
        // Overlap: srcdir extends into BRR pool's address.
        let mut map = map_no_echo();
        map.regions = vec![
            region("driver_code", 0x0200, 0x12FF, AramKind::DriverCode),
            region(
                "source_directory",
                0x1200,
                0x12FF,
                AramKind::SourceDirectory,
            ), // overlaps
            region("ipl_rom_shadow", 0xFFC0, 0xFFFF, AramKind::FixedHardware),
        ];
        let aram = aram_image_zeroed();
        let err = write_module(ModuleWriteInput {
            aram_image: &aram,
            map_report: &map,
            echo_enabled: false,
        })
        .unwrap_err();
        assert!(
            matches!(err, ModuleWriteError::BlockOverlap { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn echo_block_zero_filled_when_enabled() {
        let mut aram = aram_image_zeroed();
        // Pre-fill ARAM echo region with non-zero junk to verify
        // the writer doesn't COPY echo data — it should emit a
        // zero-filled block.
        for b in aram[0xDF00..0xFF00].iter_mut() {
            *b = 0xAA;
        }
        let map = map_with_echo_edl4();
        let r = write_module(ModuleWriteInput {
            aram_image: &aram,
            map_report: &map,
            echo_enabled: true,
        })
        .unwrap();
        let header = parse_module_header(&r.bytes).unwrap();
        let blocks = parse_module_blocks(&r.bytes, &header).unwrap();
        let echo = blocks
            .iter()
            .find(|b| b.dest_addr == 0xDF00)
            .expect("echo block");
        assert_eq!(echo.length, 8192);
        let data_start = echo.data_offset as usize;
        let data_end = data_start + echo.length as usize;
        assert!(r.bytes[data_start..data_end].iter().all(|&b| b == 0));
        // Flags echo bit is set.
        assert_eq!(header.flags & 0x0001, 0x0001);
    }

    #[test]
    fn project_blocks_to_aram_round_trip_no_echo() {
        let mut aram = aram_image_zeroed();
        // Stamp a recognizable pattern across each region.
        aram[0x0200] = 0xAB;
        aram[0x0201] = 0xCD;
        aram[0x1200] = 0xDE;
        aram[0x1300] = 0xBE;
        let map = map_no_echo();
        let r = write_module(ModuleWriteInput {
            aram_image: &aram,
            map_report: &map,
            echo_enabled: false,
        })
        .unwrap();
        let header = parse_module_header(&r.bytes).unwrap();
        let blocks = parse_module_blocks(&r.bytes, &header).unwrap();
        let projected = project_blocks_to_aram(&r.bytes, &header, &blocks);
        // The bytes inside region ranges round-trip; outside regions
        // (free / runtime), projected stays zero (matches what the
        // loader would actually upload to a fresh APU).
        assert_eq!(projected[0x0200], 0xAB);
        assert_eq!(projected[0x0201], 0xCD);
        assert_eq!(projected[0x1200], 0xDE);
        assert_eq!(projected[0x1300], 0xBE);
        // Everything before $0200 stays zero in the projected image.
        for b in &projected[..0x0200] {
            assert_eq!(*b, 0);
        }
    }
}
