//! `module.bin` binary layout (SPEC §19.4).
//!
//! M1.0 ships layout-aware structs only: no encoder, no decoder, no
//! file I/O. The structs exist so M1.4 (`.sfc` builder) and the
//! 65816 loader stub (M1.5) can refer to a single source of truth
//! for offsets and sizes.
//!
//! Layout, all little-endian, schema version 1:
//!
//! ```text
//! Header — 64 bytes
//!   0x00  u8[8]   magic = "SFCWCM1\0"
//!   0x08  u16     schema_version = 1
//!   0x0A  u16     header_len = 64
//!   0x0C  u16     block_count
//!   0x0E  u16     entrypoint = $0200 for M1
//!   0x10  u32     block_table_offset = 64
//!   0x14  u32     data_offset = 64 + block_count * 8
//!   0x18  u32     total_file_len
//!   0x1C  u16     flags  (bit 0 = echo_enabled_for_module; rest = 0)
//!   0x1E  u16     reserved = 0
//!   0x20  u8[32]  content_sha256_zeroed
//!
//! Block table entry — 8 bytes
//!   0x00  u16  dest_addr
//!   0x02  u16  length
//!   0x04  u32  data_offset
//! ```
//!
//! Fields are naturally aligned, so plain `#[repr(C)]` reproduces
//! the documented offsets without `packed` games. Layout invariants
//! are pinned by tests.

/// 8-byte magic at file offset 0. ASCII; trailing NUL is part of
/// the magic and is significant.
pub const MODULE_MAGIC: [u8; 8] = *b"SFCWCM1\0";

/// M1 schema version.
pub const MODULE_SCHEMA_VERSION: u16 = 1;

/// Header length in bytes.
pub const MODULE_HEADER_LEN: u16 = 64;

/// SPEC §19.4 entry-point address for M1 (start of driver code).
pub const MODULE_ENTRYPOINT_M1: u16 = 0x0200;

/// `flags` bit 0: module includes a zero-filled echo buffer block.
pub const MODULE_FLAG_ECHO_ENABLED: u16 = 0x0001;

/// On-disk module header. 64 bytes, little-endian. See module docs
/// for the layout.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleHeader {
    /// 8-byte magic; matches [`MODULE_MAGIC`] for valid files.
    pub magic: [u8; 8],
    /// Schema version; M1 = [`MODULE_SCHEMA_VERSION`].
    pub schema_version: u16,
    /// Always [`MODULE_HEADER_LEN`].
    pub header_len: u16,
    /// Number of block-table entries.
    pub block_count: u16,
    /// SPC700 entry point address; [`MODULE_ENTRYPOINT_M1`] for M1.
    pub entrypoint: u16,
    /// Always 64 (= [`MODULE_HEADER_LEN`]).
    pub block_table_offset: u32,
    /// `64 + block_count * 8`.
    pub data_offset: u32,
    /// Total `module.bin` length in bytes.
    pub total_file_len: u32,
    /// Bit 0 = `echo_enabled_for_module`; bits 1..15 reserved, must
    /// be zero.
    pub flags: u16,
    /// Reserved; must be zero.
    pub reserved: u16,
    /// SHA-256 of the entire `module.bin` with bytes `0x20..0x40` set
    /// to zero (self-reference workaround, SPEC §19.4). The literal
    /// full-file SHA-256 lives in the M1 manifest as
    /// `module_file_sha256`.
    pub content_sha256_zeroed: [u8; 32],
}

impl ModuleHeader {
    /// Constant-zero header used as a "shape only" sentinel for
    /// tests and stubs. Not a valid module — all length fields zero,
    /// magic empty.
    pub const ZERO: ModuleHeader = ModuleHeader {
        magic: [0u8; 8],
        schema_version: 0,
        header_len: 0,
        block_count: 0,
        entrypoint: 0,
        block_table_offset: 0,
        data_offset: 0,
        total_file_len: 0,
        flags: 0,
        reserved: 0,
        content_sha256_zeroed: [0u8; 32],
    };
}

/// One entry in the block table. 8 bytes, little-endian.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleBlockEntry {
    /// ARAM destination address.
    pub dest_addr: u16,
    /// Block length in bytes; must be `> 0`.
    pub length: u16,
    /// File offset of this block's data.
    pub data_offset: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn module_header_size_is_64() {
        assert_eq!(std::mem::size_of::<ModuleHeader>(), 64);
    }

    #[test]
    fn module_header_align_is_4() {
        // u32 fields drive alignment.
        assert_eq!(std::mem::align_of::<ModuleHeader>(), 4);
    }

    #[test]
    fn module_header_field_offsets_match_spec_19_4() {
        assert_eq!(offset_of!(ModuleHeader, magic), 0x00);
        assert_eq!(offset_of!(ModuleHeader, schema_version), 0x08);
        assert_eq!(offset_of!(ModuleHeader, header_len), 0x0A);
        assert_eq!(offset_of!(ModuleHeader, block_count), 0x0C);
        assert_eq!(offset_of!(ModuleHeader, entrypoint), 0x0E);
        assert_eq!(offset_of!(ModuleHeader, block_table_offset), 0x10);
        assert_eq!(offset_of!(ModuleHeader, data_offset), 0x14);
        assert_eq!(offset_of!(ModuleHeader, total_file_len), 0x18);
        assert_eq!(offset_of!(ModuleHeader, flags), 0x1C);
        assert_eq!(offset_of!(ModuleHeader, reserved), 0x1E);
        assert_eq!(offset_of!(ModuleHeader, content_sha256_zeroed), 0x20);
    }

    #[test]
    fn module_block_entry_size_is_8() {
        assert_eq!(std::mem::size_of::<ModuleBlockEntry>(), 8);
    }

    #[test]
    fn module_block_entry_align_is_4() {
        assert_eq!(std::mem::align_of::<ModuleBlockEntry>(), 4);
    }

    #[test]
    fn module_block_entry_field_offsets_match_spec_19_4() {
        assert_eq!(offset_of!(ModuleBlockEntry, dest_addr), 0x00);
        assert_eq!(offset_of!(ModuleBlockEntry, length), 0x02);
        assert_eq!(offset_of!(ModuleBlockEntry, data_offset), 0x04);
    }

    #[test]
    fn module_constants_match_spec_19_4() {
        assert_eq!(&MODULE_MAGIC, b"SFCWCM1\0");
        assert_eq!(MODULE_SCHEMA_VERSION, 1);
        assert_eq!(MODULE_HEADER_LEN, 64);
        assert_eq!(MODULE_ENTRYPOINT_M1, 0x0200);
        assert_eq!(MODULE_FLAG_ECHO_ENABLED, 0x0001);
    }

    #[test]
    fn module_zero_sentinel_is_well_defined() {
        let z = ModuleHeader::ZERO;
        assert_eq!(z.magic, [0u8; 8]);
        assert_eq!(z.schema_version, 0);
        assert_eq!(z.total_file_len, 0);
        assert_eq!(z.content_sha256_zeroed, [0u8; 32]);
    }
}
