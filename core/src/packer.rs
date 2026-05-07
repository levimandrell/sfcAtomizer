//! M1 ARAM packer.
//!
//! Takes a validated `ProjectV1` plus pre-encoded BRR samples and a
//! driver-code blob, produces a fully-laid-out 64 KB ARAM image and
//! the matching [`AramMapReport`]. Single source of truth for region
//! layout — the M0.6 byte-scanning path in [`crate::aram`] stays for
//! M0 acceptance, but anything M1+ goes through here.
//!
//! Layout, low to high (SPEC §15.1):
//!
//! ```text
//! 0000–00EF  direct page         (FixedRuntime, 240 B, packer never writes)
//! 00F0–00FF  hardware I/O        (FixedHardware, 16 B, inviolable)
//! 0100–01FF  stack               (FixedRuntime, 256 B, inviolable)
//! 0200–11FF  driver code         (4 KiB budget; M1.4 ships zero-filled placeholder)
//! 1200–      source directory    (page-aligned; 4 B/entry; padded to next page)
//! ....–      BRR sample pool     (single contiguous region, declaration order)
//! ....–      free                (headroom)
//! XXXX–FF00  echo buffer         (when enabled; ESA-aligned; ends below IPL pad)
//! FF00–FFBF  ipl_rom_safe_pad    (192 B reserved — see "Echo placement" below)
//! FFC0–FFFF  ipl_rom_shadow      (FixedHardware; only RAM if IPL ROM unmapped)
//! ```
//!
//! ### Echo placement
//!
//! The S-DSP echo buffer is `EDL * 2048` bytes long, page-aligned at
//! `ESA*0x100`. M1 conservative policy: don't trust the driver to
//! unmap IPL ROM, so the echo buffer must end at or before `0xFFC0`.
//!
//! The largest page boundary at or below `0xFFC0` is `0xFF00`; that's
//! the canonical M1 echo end. Echoes for any EDL fit:
//!
//! - EDL=4  → echo_start = `0xDF00`, ESA = `0xDF`
//! - EDL=15 → echo_start = `0x8700`, ESA = `0x87`
//!
//! 192 bytes between `0xFF00` and `0xFFC0` are reported as
//! `ipl_rom_safe_pad` (FixedHardware). This is the deliberate cost of
//! M1 conservatism — M2+ may reclaim them by gating on driver behaviour.
//!
//! (The brief's "echo_end = 0xFFC0" math gives an ESA-misaligned
//! `echo_start = 0xDFC0`; ESA*0x100 would point at `0xDF00`, not
//! `0xDFC0`, and 0xC0 bytes of the buffer would be silently lost.
//! The page-aligned `0xFF00` end is the correct interpretation.)

use thiserror::Error;

use crate::echo_validation::validate_echo;
use crate::project::ProjectV1;
use crate::report::{
    AramCollision, AramEchoSummary, AramKind, AramMapReport, AramRegion, AramSamplesSummary,
    AramSourceDirSummary, PerSampleAramEntry, SCHEMA_VERSION,
};

pub const ARAM_LEN: usize = 0x10000;
pub const FIXED_REGIONS_END: u16 = 0x0200;
pub const DRIVER_CODE_START: u16 = 0x0200;
/// 4 KiB at $0200..$1200. M1.4 placeholder is zero-filled. M1.5
/// replaces with the assembled `sample_basic` driver.
pub const DRIVER_CODE_BUDGET_M1: u32 = 0x1000;
pub const DRIVER_CODE_END_EXCLUSIVE: u16 = DRIVER_CODE_START + DRIVER_CODE_BUDGET_M1 as u16;
/// Largest page boundary at or below the IPL ROM shadow start
/// (`$FFC0`). All M1 echo buffers end here.
pub const ECHO_END_M1: u16 = 0xFF00;
/// `[$FF00, $FFC0)` — reserved on M1 to keep the echo buffer strictly
/// below the IPL ROM region regardless of CONTROL bit 7 state.
pub const IPL_ROM_SAFE_PAD_START: u16 = 0xFF00;
pub const IPL_ROM_SHADOW_START: u16 = 0xFFC0;

#[derive(Debug, Clone)]
pub struct PackInput {
    /// Project that has already passed [`ProjectV1::validate`].
    pub project: ProjectV1,
    /// Per-sample encoded BRR bytes, in `sample_pool` order.
    pub encoded_samples: Vec<EncodedSample>,
    /// Pre-built driver code blob. M1.4 ships a zero-filled placeholder
    /// of any length `<= DRIVER_CODE_BUDGET_M1`; M1.5 ships the real
    /// assembled driver. Bytes are copied verbatim starting at
    /// `$0200`; the unused tail of the budget is left zero.
    pub driver_code: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct EncodedSample {
    pub sample_id: String,
    /// Encoded BRR bytes — must be a multiple of 9.
    pub bytes: Vec<u8>,
    /// `Some(loop_start_sample / 16)` for looped samples; `None` for
    /// one-shots. Used to fill the source-directory `loop_addr` field.
    pub loop_entry_block: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct PackOutput {
    pub aram_image: Box<[u8; ARAM_LEN]>,
    pub map_report: AramMapReport,
}

#[derive(Debug, Error)]
pub enum PackError {
    #[error("driver code too large: {actual} bytes > {max} budget")]
    DriverTooLarge { actual: u32, max: u32 },
    #[error(
        "source directory overflow: {sources} sources × 4 B (+ pad to page) = {bytes} > {budget}"
    )]
    SourceDirectoryOverflow {
        sources: u32,
        bytes: u32,
        budget: u32,
    },
    #[error("BRR sample pool overflow: needed {needed} bytes, free {free} bytes")]
    SamplePoolOverflow { needed: u32, free: u32 },
    #[error(
        "echo buffer + sample pool overlap: echo at ${echo_start:04X}, pool ends at ${pool_end:04X}"
    )]
    EchoOverlap { echo_start: u16, pool_end: u16 },
    #[error("encoded sample id={sample_id:?} length {actual} not a multiple of 9 bytes")]
    EncodedSampleMisaligned { sample_id: String, actual: u32 },
    #[error("loop_entry_block {block} out of range for sample id={sample_id:?} ({total_blocks} blocks total)")]
    LoopEntryOutOfRange {
        sample_id: String,
        block: u32,
        total_blocks: u32,
    },
    #[error("encoded_samples[{index}].sample_id={got:?} != sample_pool[{index}].id={expected:?}")]
    SampleIdMismatch {
        index: usize,
        got: String,
        expected: String,
    },
    #[error("encoded_samples.len()={got} != sample_pool.len()={expected}")]
    SampleCountMismatch { got: usize, expected: usize },
    #[error("project sample_pool is empty")]
    EmptyPool,
    #[error("invalid echo configuration: {0}")]
    InvalidEcho(String),

    // -------- M2.3 multi-source pack (pack_v2) --------
    #[error("encoded_atoms.len()={got} != atom_pool.len()={expected}")]
    AtomCountMismatch { got: usize, expected: usize },
    #[error("encoded_atoms[{index}].sample_id={got:?} != atom_pool[{index}].id={expected:?}")]
    AtomIdMismatch {
        index: usize,
        got: String,
        expected: String,
    },
    #[error("encoded atom id={atom_id:?} length {actual} not a multiple of 9 bytes")]
    EncodedAtomMisaligned { atom_id: String, actual: u32 },
    #[error("voice setup table size {actual} bytes != expected {expected}")]
    VoiceSetupTableSize { actual: u32, expected: u32 },
}

pub fn pack(input: PackInput) -> Result<PackOutput, PackError> {
    let PackInput {
        project,
        encoded_samples,
        driver_code,
    } = input;

    // --- Sanity / contract checks ---------------------------------------
    if project.sample_pool.is_empty() {
        return Err(PackError::EmptyPool);
    }
    if encoded_samples.len() != project.sample_pool.len() {
        return Err(PackError::SampleCountMismatch {
            got: encoded_samples.len(),
            expected: project.sample_pool.len(),
        });
    }
    for (i, (slot, encoded)) in project
        .sample_pool
        .iter()
        .zip(encoded_samples.iter())
        .enumerate()
    {
        if slot.id != encoded.sample_id {
            return Err(PackError::SampleIdMismatch {
                index: i,
                got: encoded.sample_id.clone(),
                expected: slot.id.clone(),
            });
        }
        if !encoded.bytes.len().is_multiple_of(9) {
            return Err(PackError::EncodedSampleMisaligned {
                sample_id: encoded.sample_id.clone(),
                actual: encoded.bytes.len() as u32,
            });
        }
        if let Some(block) = encoded.loop_entry_block {
            let total_blocks = (encoded.bytes.len() / 9) as u32;
            if block >= total_blocks {
                return Err(PackError::LoopEntryOutOfRange {
                    sample_id: encoded.sample_id.clone(),
                    block,
                    total_blocks,
                });
            }
        }
    }
    validate_echo(&project.master_echo, &project.sample_pool)
        .map_err(|errs| PackError::InvalidEcho(format!("{errs:?}")))?;

    // --- Driver region --------------------------------------------------
    if driver_code.len() as u32 > DRIVER_CODE_BUDGET_M1 {
        return Err(PackError::DriverTooLarge {
            actual: driver_code.len() as u32,
            max: DRIVER_CODE_BUDGET_M1,
        });
    }
    let mut image = Box::new([0u8; ARAM_LEN]);
    image[DRIVER_CODE_START as usize..DRIVER_CODE_START as usize + driver_code.len()]
        .copy_from_slice(&driver_code);

    // --- Source directory placement -------------------------------------
    let sample_count = project.sample_pool.len() as u32;
    let srcdir_bytes = sample_count * 4;
    let srcdir_start: u16 = DRIVER_CODE_END_EXCLUSIVE;
    debug_assert!(
        srcdir_start.is_multiple_of(0x100),
        "driver budget is page-aligned"
    );
    let srcdir_end_unpadded = srcdir_start as u32 + srcdir_bytes;
    let srcdir_end_padded = srcdir_end_unpadded.next_multiple_of(0x100);
    let srcdir_padding = srcdir_end_padded - srcdir_end_unpadded;
    if srcdir_end_padded > 0x10000 {
        return Err(PackError::SourceDirectoryOverflow {
            sources: sample_count,
            bytes: srcdir_end_padded - srcdir_start as u32,
            budget: 0x10000 - srcdir_start as u32,
        });
    }

    // --- Echo placement -------------------------------------------------
    let echo_enabled = project.master_echo.enabled;
    let edl = project.master_echo.edl;
    let echo_size_bytes: u32 = if echo_enabled { edl as u32 * 2048 } else { 0 };
    let echo_start: u16 = if echo_enabled {
        ECHO_END_M1 - echo_size_bytes as u16
    } else {
        // Sentinel; not used when disabled.
        ECHO_END_M1
    };
    let echo_end: u16 = ECHO_END_M1;
    if echo_enabled {
        debug_assert!(echo_start.is_multiple_of(0x100), "ESA must be page-aligned");
    }

    // --- Sample pool placement ------------------------------------------
    let pool_start: u32 = srcdir_end_padded;
    let pool_budget_end: u32 = if echo_enabled {
        echo_start as u32
    } else {
        IPL_ROM_SAFE_PAD_START as u32
    };
    let total_brr: u32 = encoded_samples.iter().map(|s| s.bytes.len() as u32).sum();
    let pool_end = pool_start + total_brr;
    if pool_end > pool_budget_end {
        if echo_enabled {
            return Err(PackError::EchoOverlap {
                echo_start,
                pool_end: pool_end as u16,
            });
        }
        return Err(PackError::SamplePoolOverflow {
            needed: total_brr,
            free: pool_budget_end - pool_start,
        });
    }

    // Write source-directory entries + copy BRR data.
    let mut per_sample: Vec<PerSampleAramEntry> = Vec::with_capacity(encoded_samples.len());
    let mut cursor: u32 = pool_start;
    for (i, encoded) in encoded_samples.iter().enumerate() {
        let start_addr: u16 = cursor as u16;
        let loop_addr: u16 = match encoded.loop_entry_block {
            Some(block) => start_addr + (block * 9) as u16,
            None => start_addr,
        };

        let dir_off = srcdir_start as usize + i * 4;
        image[dir_off] = start_addr as u8;
        image[dir_off + 1] = (start_addr >> 8) as u8;
        image[dir_off + 2] = loop_addr as u8;
        image[dir_off + 3] = (loop_addr >> 8) as u8;

        image[cursor as usize..cursor as usize + encoded.bytes.len()]
            .copy_from_slice(&encoded.bytes);
        cursor += encoded.bytes.len() as u32;

        per_sample.push(PerSampleAramEntry {
            sample_id: encoded.sample_id.clone(),
            start_addr,
            loop_addr: Some(loop_addr),
            bytes: encoded.bytes.len() as u32,
        });
    }
    debug_assert_eq!(cursor, pool_end);

    // --- Build the map report ------------------------------------------
    let map_report = build_map_report(BuildMapInput {
        sample_count,
        driver_budget: DRIVER_CODE_BUDGET_M1,
        srcdir_start,
        srcdir_bytes,
        srcdir_padding,
        pool_start,
        pool_end,
        echo_enabled,
        edl,
        echo_size_bytes,
        echo_start,
        echo_end,
        per_sample,
    });

    Ok(PackOutput {
        aram_image: image,
        map_report,
    })
}

struct BuildMapInput {
    sample_count: u32,
    driver_budget: u32,
    srcdir_start: u16,
    srcdir_bytes: u32,
    srcdir_padding: u32,
    pool_start: u32,
    pool_end: u32,
    echo_enabled: bool,
    edl: u8,
    echo_size_bytes: u32,
    echo_start: u16,
    echo_end: u16,
    per_sample: Vec<PerSampleAramEntry>,
}

fn build_map_report(b: BuildMapInput) -> AramMapReport {
    let mut regions: Vec<AramRegion> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Fixed runtime regions.
    regions.push(region_inclusive(
        "direct_page",
        0x0000,
        0x00EF,
        AramKind::FixedRuntime,
    ));
    regions.push(region_inclusive(
        "hardware_io",
        0x00F0,
        0x00FF,
        AramKind::FixedHardware,
    ));
    regions.push(region_inclusive(
        "stack",
        0x0100,
        0x01FF,
        AramKind::FixedRuntime,
    ));

    // Driver code (full budget — placeholder bytes don't change the
    // region's role; M1.5's real driver fills the same window).
    regions.push(AramRegion {
        name: "driver_code".to_string(),
        start: format_addr(DRIVER_CODE_START as u32),
        end: format_addr(DRIVER_CODE_START as u32 + b.driver_budget - 1),
        bytes: b.driver_budget,
        kind: AramKind::DriverCode,
    });

    // Source directory (entries + page padding rolled into one
    // SourceDirectory region so the meter's region list lines up
    // with what the driver will see at runtime).
    let srcdir_total = b.srcdir_bytes + b.srcdir_padding;
    regions.push(AramRegion {
        name: "source_directory".to_string(),
        start: format_addr(b.srcdir_start as u32),
        end: format_addr(b.srcdir_start as u32 + srcdir_total - 1),
        bytes: srcdir_total,
        kind: AramKind::SourceDirectory,
    });

    // Sample BRR pool.
    if b.pool_end > b.pool_start {
        regions.push(AramRegion {
            name: "sample_brr_pool".to_string(),
            start: format_addr(b.pool_start),
            end: format_addr(b.pool_end - 1),
            bytes: b.pool_end - b.pool_start,
            kind: AramKind::SampleBrrPool,
        });
    }

    // Free region between pool and echo / IPL pad.
    let free_start = b.pool_end;
    let free_end_exclusive: u32 = if b.echo_enabled {
        b.echo_start as u32
    } else {
        IPL_ROM_SAFE_PAD_START as u32
    };
    if free_end_exclusive > free_start {
        regions.push(AramRegion {
            name: "free".to_string(),
            start: format_addr(free_start),
            end: format_addr(free_end_exclusive - 1),
            bytes: free_end_exclusive - free_start,
            kind: AramKind::Free,
        });
    }

    // Echo buffer.
    if b.echo_enabled {
        regions.push(AramRegion {
            name: "echo_buffer".to_string(),
            start: format_addr(b.echo_start as u32),
            end: format_addr(b.echo_end as u32 - 1),
            bytes: b.echo_size_bytes,
            kind: AramKind::EchoBuffer,
        });
    }

    // IPL ROM safe pad ($FF00..$FFC0). Lives on M1 because echo can't
    // straddle the IPL ROM shadow without explicit unmap.
    regions.push(region_inclusive(
        "ipl_rom_safe_pad",
        IPL_ROM_SAFE_PAD_START as u32,
        IPL_ROM_SHADOW_START as u32 - 1,
        AramKind::FixedHardware,
    ));

    // IPL ROM shadow ($FFC0..$FFFF).
    regions.push(region_inclusive(
        "ipl_rom_shadow",
        IPL_ROM_SHADOW_START as u32,
        0xFFFF,
        AramKind::FixedHardware,
    ));

    let free_bytes: u32 = regions
        .iter()
        .filter(|r| r.kind == AramKind::Free)
        .map(|r| r.bytes)
        .sum();

    // M1 meter summaries.
    let echo = AramEchoSummary {
        enabled: b.echo_enabled,
        edl: b.edl,
        buffer_bytes: b.echo_size_bytes,
        // SPEC §15.3 caveat — the 4-byte echo write region is a hardware
        // hazard whenever echo writeback is enabled, regardless of EDL.
        // The driver gates this via FLG; the meter just surfaces the
        // size so the user sees there's nothing magical at EDL=0.
        hardware_tail_bytes: 4,
        esa: if b.echo_enabled {
            (b.echo_start >> 8) as u8
        } else {
            0
        },
        percent_of_aram: (b.echo_size_bytes as f64) * 100.0 / (ARAM_LEN as f64),
        // True except for the EDL=0/enabled trap, which validate_echo
        // rejects upstream — kept here defensively.
        writeback_safe: !(b.echo_enabled && b.edl == 0),
    };
    let source_directory = AramSourceDirSummary {
        source_count: b.sample_count,
        bytes: b.srcdir_bytes,
        padding_bytes: b.srcdir_padding,
        start_addr: b.srcdir_start,
    };
    let total_brr_bytes: u32 = b.per_sample.iter().map(|p| p.bytes).sum();
    let samples = AramSamplesSummary {
        total_samples: b.sample_count,
        total_brr_bytes,
        per_sample: b.per_sample,
    };

    // Warnings.
    if !echo.writeback_safe {
        warnings.push("ECHO_WRITEBACK_HAZARD_AT_EDL_ZERO".to_string());
    }
    if b.echo_enabled && b.echo_start >= 0xFE00 {
        warnings.push("ECHO_NEAR_TOP_OF_ARAM_REVIEW_IPL_BIT".to_string());
    }
    if free_bytes < 256 {
        warnings.push("FREE_LESS_THAN_256_BYTES".to_string());
    }

    AramMapReport {
        schema_version: SCHEMA_VERSION,
        report_type: AramMapReport::REPORT_TYPE.to_string(),
        total_aram: ARAM_LEN as u32,
        regions,
        free_bytes,
        collisions: Vec::<AramCollision>::new(),
        echo: Some(echo),
        source_directory: Some(source_directory),
        samples: Some(samples),
        atoms: None,
        warnings,
    }
}

fn region_inclusive(name: &str, start: u32, end_inclusive: u32, kind: AramKind) -> AramRegion {
    AramRegion {
        name: name.to_string(),
        start: format_addr(start),
        end: format_addr(end_inclusive),
        bytes: end_inclusive - start + 1,
        kind,
    }
}

fn format_addr(addr: u32) -> String {
    format!("0x{addr:04X}")
}

// =============================================================================
// M2.3 — multi-source v2 packer (pack_v2).
//
// Layout per SPEC §15.5 (M2 region order):
//
//   $0000..$01FF   fixed runtime (zero page / I/O / stack)
//   $0200..$11FF   driver code (4 KiB budget)
//   $1200..        source directory (page-aligned; samples first then atoms)
//   ....           sequence data (M2.3: empty / placeholder; M2.4 fills)
//   ....           sample BRR pool (declaration order)
//   ....           synth atom pool (declaration order, after samples)
//   ....           voice setup table (22 bytes for M2; SPEC §15.7)
//   ....           free
//   XXXX..$FF00    echo buffer (when enabled)
//   $FF00..$FFBF   ipl_rom_safe_pad
//   $FFC0..$FFFF   ipl_rom_shadow
//
// For sample-only-equivalent inputs (`encoded_atoms.is_empty() &&
// sequence_data.is_none() && voice_setup_table.is_none()`) the
// emitted ARAM bytes are byte-identical to what `pack` produces for
// the v1-equivalent project, preserving the M2.1 migration→pack
// bit-identity guarantee.
// =============================================================================

/// SPEC §15.7: 11 bytes per voice entry.
pub const VOICE_SETUP_ENTRY_BYTES: u32 = 11;
/// M2 has 2 entries → 22-byte table.
pub const VOICE_SETUP_TABLE_M2_BYTES: u32 = 2 * VOICE_SETUP_ENTRY_BYTES;
/// SPEC §15.7 unused-voice sentinel — `src_index` 0xFF means the
/// driver should leave the voice silent.
pub const VOICE_SETUP_UNUSED_SRC_INDEX: u8 = 0xFF;

#[derive(Debug, Clone)]
pub struct PackInputV2 {
    pub project: crate::project_v2::ProjectV2,
    /// Encoded BRR for `project.sample_pool[]`, in declaration order.
    pub encoded_samples: Vec<EncodedSample>,
    /// Encoded BRR for `project.atom_pool[]`, in declaration order.
    /// Always-looped at block 0 (single-cycle convention).
    pub encoded_atoms: Vec<EncodedSample>,
    pub driver_code: Vec<u8>,
    /// SEQ2 bytecode (M2.4). M2.3 always passes `None`; the packer
    /// reserves zero bytes when absent.
    pub sequence_data: Option<Vec<u8>>,
    /// Voice setup table bytes (SPEC §15.7). For `multi_voice_atom`
    /// projects the host passes a `Some(22)`-byte table; sample-only
    /// passes `None` and the packer skips the region.
    pub voice_setup_table: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct PackOutputV2 {
    pub aram_image: Box<[u8; ARAM_LEN]>,
    pub map_report: AramMapReport,
}

pub fn pack_v2(input: PackInputV2) -> Result<PackOutputV2, PackError> {
    let PackInputV2 {
        project,
        encoded_samples,
        encoded_atoms,
        driver_code,
        sequence_data,
        voice_setup_table,
    } = input;

    // --- Sanity / contract checks ---------------------------------------
    if project.sample_pool.is_empty() && project.atom_pool.is_empty() {
        return Err(PackError::EmptyPool);
    }
    if encoded_samples.len() != project.sample_pool.len() {
        return Err(PackError::SampleCountMismatch {
            got: encoded_samples.len(),
            expected: project.sample_pool.len(),
        });
    }
    for (i, (slot, encoded)) in project
        .sample_pool
        .iter()
        .zip(encoded_samples.iter())
        .enumerate()
    {
        if slot.id != encoded.sample_id {
            return Err(PackError::SampleIdMismatch {
                index: i,
                got: encoded.sample_id.clone(),
                expected: slot.id.clone(),
            });
        }
        if !encoded.bytes.len().is_multiple_of(9) {
            return Err(PackError::EncodedSampleMisaligned {
                sample_id: encoded.sample_id.clone(),
                actual: encoded.bytes.len() as u32,
            });
        }
        if let Some(block) = encoded.loop_entry_block {
            let total_blocks = (encoded.bytes.len() / 9) as u32;
            if block >= total_blocks {
                return Err(PackError::LoopEntryOutOfRange {
                    sample_id: encoded.sample_id.clone(),
                    block,
                    total_blocks,
                });
            }
        }
    }
    if encoded_atoms.len() != project.atom_pool.len() {
        return Err(PackError::AtomCountMismatch {
            got: encoded_atoms.len(),
            expected: project.atom_pool.len(),
        });
    }
    for (i, (slot, encoded)) in project
        .atom_pool
        .iter()
        .zip(encoded_atoms.iter())
        .enumerate()
    {
        if slot.id != encoded.sample_id {
            return Err(PackError::AtomIdMismatch {
                index: i,
                got: encoded.sample_id.clone(),
                expected: slot.id.clone(),
            });
        }
        if !encoded.bytes.len().is_multiple_of(9) {
            return Err(PackError::EncodedAtomMisaligned {
                atom_id: encoded.sample_id.clone(),
                actual: encoded.bytes.len() as u32,
            });
        }
    }
    if let Some(table) = &voice_setup_table {
        if table.len() as u32 != VOICE_SETUP_TABLE_M2_BYTES {
            return Err(PackError::VoiceSetupTableSize {
                actual: table.len() as u32,
                expected: VOICE_SETUP_TABLE_M2_BYTES,
            });
        }
    }
    validate_echo(&project.master_echo, &project.sample_pool)
        .map_err(|errs| PackError::InvalidEcho(format!("{errs:?}")))?;

    // --- Driver region --------------------------------------------------
    if driver_code.len() as u32 > DRIVER_CODE_BUDGET_M1 {
        return Err(PackError::DriverTooLarge {
            actual: driver_code.len() as u32,
            max: DRIVER_CODE_BUDGET_M1,
        });
    }
    let mut image = Box::new([0u8; ARAM_LEN]);
    image[DRIVER_CODE_START as usize..DRIVER_CODE_START as usize + driver_code.len()]
        .copy_from_slice(&driver_code);

    // --- Source directory placement -------------------------------------
    let total_sources = (project.sample_pool.len() + project.atom_pool.len()) as u32;
    let srcdir_bytes = total_sources * 4;
    let srcdir_start: u16 = DRIVER_CODE_END_EXCLUSIVE;
    let srcdir_end_unpadded = srcdir_start as u32 + srcdir_bytes;
    let srcdir_end_padded = srcdir_end_unpadded.next_multiple_of(0x100);
    let srcdir_padding = srcdir_end_padded - srcdir_end_unpadded;
    if srcdir_end_padded > 0x10000 {
        return Err(PackError::SourceDirectoryOverflow {
            sources: total_sources,
            bytes: srcdir_end_padded - srcdir_start as u32,
            budget: 0x10000 - srcdir_start as u32,
        });
    }

    // --- Echo placement (top of usable, ending at $FF00) ----------------
    let echo_enabled = project.master_echo.enabled;
    let edl = project.master_echo.edl;
    let echo_size_bytes: u32 = if echo_enabled { edl as u32 * 2048 } else { 0 };
    let echo_start: u16 = if echo_enabled {
        ECHO_END_M1 - echo_size_bytes as u16
    } else {
        ECHO_END_M1
    };
    let echo_end: u16 = ECHO_END_M1;

    // --- Layout the M2 regions left to right ----------------------------
    let mut cursor: u32 = srcdir_end_padded;

    // Sequence data (M2.4 will populate; M2.3 only reserves when given).
    let seq_bytes = sequence_data.as_ref().map(|v| v.len() as u32).unwrap_or(0);
    let seq_start = cursor;
    if seq_bytes > 0 {
        // Page-align downstream so a future SEQ2 inspector lines up;
        // no spec requirement for this, but it's the recommended
        // convention from the brief. M2.3 doesn't actually populate.
        let seq_end = seq_start + seq_bytes;
        cursor = seq_end.next_multiple_of(0x100);
    }
    let seq_region_end = cursor;

    // Sample BRR pool.
    let total_sample_brr: u32 = encoded_samples.iter().map(|s| s.bytes.len() as u32).sum();
    let sample_pool_start = cursor;
    let sample_pool_end = sample_pool_start + total_sample_brr;
    cursor = sample_pool_end;

    // Atom BRR pool.
    let total_atom_brr: u32 = encoded_atoms.iter().map(|s| s.bytes.len() as u32).sum();
    let atom_pool_start = cursor;
    let atom_pool_end = atom_pool_start + total_atom_brr;
    cursor = atom_pool_end;

    // Voice setup table.
    let voice_table_bytes = voice_setup_table
        .as_ref()
        .map(|v| v.len() as u32)
        .unwrap_or(0);
    let voice_table_start = cursor;
    let voice_table_end = voice_table_start + voice_table_bytes;
    cursor = voice_table_end;

    // The "pool budget" is the byte just before echo or before the
    // IPL safe pad if echo is disabled.
    let budget_end: u32 = if echo_enabled {
        echo_start as u32
    } else {
        IPL_ROM_SAFE_PAD_START as u32
    };
    if cursor > budget_end {
        if echo_enabled {
            return Err(PackError::EchoOverlap {
                echo_start,
                pool_end: cursor as u16,
            });
        }
        let total_payload = total_sample_brr + total_atom_brr + seq_bytes + voice_table_bytes;
        return Err(PackError::SamplePoolOverflow {
            needed: total_payload,
            free: budget_end.saturating_sub(srcdir_end_padded),
        });
    }

    // Write sequence data if present.
    if let Some(seq) = &sequence_data {
        if !seq.is_empty() {
            image[seq_start as usize..seq_start as usize + seq.len()].copy_from_slice(seq);
        }
    }

    // Write sample BRR + source-directory entries (samples first).
    let mut per_sample: Vec<PerSampleAramEntry> = Vec::with_capacity(encoded_samples.len());
    let mut write_cursor = sample_pool_start;
    for (i, encoded) in encoded_samples.iter().enumerate() {
        let start_addr: u16 = write_cursor as u16;
        let loop_addr: u16 = match encoded.loop_entry_block {
            Some(block) => start_addr + (block * 9) as u16,
            None => start_addr,
        };
        let dir_off = srcdir_start as usize + i * 4;
        image[dir_off] = start_addr as u8;
        image[dir_off + 1] = (start_addr >> 8) as u8;
        image[dir_off + 2] = loop_addr as u8;
        image[dir_off + 3] = (loop_addr >> 8) as u8;

        image[write_cursor as usize..write_cursor as usize + encoded.bytes.len()]
            .copy_from_slice(&encoded.bytes);
        write_cursor += encoded.bytes.len() as u32;

        per_sample.push(PerSampleAramEntry {
            sample_id: encoded.sample_id.clone(),
            start_addr,
            loop_addr: Some(loop_addr),
            bytes: encoded.bytes.len() as u32,
        });
    }
    debug_assert_eq!(write_cursor, sample_pool_end);

    // Write atom BRR + source-directory entries (samples first, atoms
    // second per SPEC §15.5; SRCN = samples.len() + i for the i-th atom).
    let mut per_atom: Vec<crate::report::PerAtomAramEntry> =
        Vec::with_capacity(encoded_atoms.len());
    let mut write_cursor = atom_pool_start;
    for (i, encoded) in encoded_atoms.iter().enumerate() {
        let start_addr: u16 = write_cursor as u16;
        // Atoms always loop, entry block = 0 (single-cycle).
        let loop_addr: u16 = start_addr;
        let dir_index = encoded_samples.len() + i;
        let dir_off = srcdir_start as usize + dir_index * 4;
        image[dir_off] = start_addr as u8;
        image[dir_off + 1] = (start_addr >> 8) as u8;
        image[dir_off + 2] = loop_addr as u8;
        image[dir_off + 3] = (loop_addr >> 8) as u8;

        image[write_cursor as usize..write_cursor as usize + encoded.bytes.len()]
            .copy_from_slice(&encoded.bytes);
        write_cursor += encoded.bytes.len() as u32;

        per_atom.push(crate::report::PerAtomAramEntry {
            atom_id: encoded.sample_id.clone(),
            source_index: dir_index as u32,
            start_addr,
            bytes: encoded.bytes.len() as u32,
            cycle_len_samples: project.atom_pool[i].cycle_len_samples as u32,
        });
    }
    debug_assert_eq!(write_cursor, atom_pool_end);

    // Write voice setup table.
    if let Some(table) = &voice_setup_table {
        image[voice_table_start as usize..voice_table_start as usize + table.len()]
            .copy_from_slice(table);
    }

    // --- Build the map report ------------------------------------------
    let map_report = build_map_report_v2(BuildMapInputV2 {
        sample_count: project.sample_pool.len() as u32,
        atom_count: project.atom_pool.len() as u32,
        total_sources,
        driver_budget: DRIVER_CODE_BUDGET_M1,
        srcdir_start,
        srcdir_bytes,
        srcdir_padding,
        seq_present: sequence_data.as_ref().is_some_and(|s| !s.is_empty()),
        seq_start,
        seq_end: seq_region_end,
        sample_pool_start,
        sample_pool_end,
        atom_pool_start,
        atom_pool_end,
        voice_table_present: voice_setup_table.is_some(),
        voice_table_start,
        voice_table_end,
        echo_enabled,
        edl,
        echo_size_bytes,
        echo_start,
        echo_end,
        per_sample,
        per_atom,
    });

    Ok(PackOutputV2 {
        aram_image: image,
        map_report,
    })
}

#[allow(clippy::struct_field_names)]
struct BuildMapInputV2 {
    sample_count: u32,
    atom_count: u32,
    total_sources: u32,
    driver_budget: u32,
    srcdir_start: u16,
    srcdir_bytes: u32,
    srcdir_padding: u32,
    seq_present: bool,
    seq_start: u32,
    seq_end: u32,
    sample_pool_start: u32,
    sample_pool_end: u32,
    atom_pool_start: u32,
    atom_pool_end: u32,
    voice_table_present: bool,
    voice_table_start: u32,
    voice_table_end: u32,
    echo_enabled: bool,
    edl: u8,
    echo_size_bytes: u32,
    echo_start: u16,
    echo_end: u16,
    per_sample: Vec<PerSampleAramEntry>,
    per_atom: Vec<crate::report::PerAtomAramEntry>,
}

fn build_map_report_v2(b: BuildMapInputV2) -> AramMapReport {
    use crate::report::AramAtomsSummary;
    let mut regions: Vec<AramRegion> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    regions.push(region_inclusive(
        "direct_page",
        0x0000,
        0x00EF,
        AramKind::FixedRuntime,
    ));
    regions.push(region_inclusive(
        "hardware_io",
        0x00F0,
        0x00FF,
        AramKind::FixedHardware,
    ));
    regions.push(region_inclusive(
        "stack",
        0x0100,
        0x01FF,
        AramKind::FixedRuntime,
    ));
    regions.push(AramRegion {
        name: "driver_code".to_string(),
        start: format_addr(DRIVER_CODE_START as u32),
        end: format_addr(DRIVER_CODE_START as u32 + b.driver_budget - 1),
        bytes: b.driver_budget,
        kind: AramKind::DriverCode,
    });
    let srcdir_total = b.srcdir_bytes + b.srcdir_padding;
    regions.push(AramRegion {
        name: "source_directory".to_string(),
        start: format_addr(b.srcdir_start as u32),
        end: format_addr(b.srcdir_start as u32 + srcdir_total - 1),
        bytes: srcdir_total,
        kind: AramKind::SourceDirectory,
    });
    if b.seq_present && b.seq_end > b.seq_start {
        regions.push(AramRegion {
            name: "sequence_data".to_string(),
            start: format_addr(b.seq_start),
            end: format_addr(b.seq_end - 1),
            bytes: b.seq_end - b.seq_start,
            kind: AramKind::SequenceData,
        });
    }
    if b.sample_pool_end > b.sample_pool_start {
        regions.push(AramRegion {
            name: "sample_brr_pool".to_string(),
            start: format_addr(b.sample_pool_start),
            end: format_addr(b.sample_pool_end - 1),
            bytes: b.sample_pool_end - b.sample_pool_start,
            kind: AramKind::SampleBrrPool,
        });
    }
    if b.atom_pool_end > b.atom_pool_start {
        regions.push(AramRegion {
            name: "synth_atom_pool".to_string(),
            start: format_addr(b.atom_pool_start),
            end: format_addr(b.atom_pool_end - 1),
            bytes: b.atom_pool_end - b.atom_pool_start,
            kind: AramKind::SynthAtomPool,
        });
    }
    if b.voice_table_present && b.voice_table_end > b.voice_table_start {
        regions.push(AramRegion {
            name: "voice_setup_table".to_string(),
            start: format_addr(b.voice_table_start),
            end: format_addr(b.voice_table_end - 1),
            bytes: b.voice_table_end - b.voice_table_start,
            kind: AramKind::VoiceSetupTable,
        });
    }
    let used_end = b.voice_table_end;
    let free_end_exclusive: u32 = if b.echo_enabled {
        b.echo_start as u32
    } else {
        IPL_ROM_SAFE_PAD_START as u32
    };
    if free_end_exclusive > used_end {
        regions.push(AramRegion {
            name: "free".to_string(),
            start: format_addr(used_end),
            end: format_addr(free_end_exclusive - 1),
            bytes: free_end_exclusive - used_end,
            kind: AramKind::Free,
        });
    }
    if b.echo_enabled {
        regions.push(AramRegion {
            name: "echo_buffer".to_string(),
            start: format_addr(b.echo_start as u32),
            end: format_addr(b.echo_end as u32 - 1),
            bytes: b.echo_size_bytes,
            kind: AramKind::EchoBuffer,
        });
    }
    regions.push(region_inclusive(
        "ipl_rom_safe_pad",
        IPL_ROM_SAFE_PAD_START as u32,
        IPL_ROM_SHADOW_START as u32 - 1,
        AramKind::FixedHardware,
    ));
    regions.push(region_inclusive(
        "ipl_rom_shadow",
        IPL_ROM_SHADOW_START as u32,
        0xFFFF,
        AramKind::FixedHardware,
    ));

    let free_bytes: u32 = regions
        .iter()
        .filter(|r| r.kind == AramKind::Free)
        .map(|r| r.bytes)
        .sum();

    let echo = AramEchoSummary {
        enabled: b.echo_enabled,
        edl: b.edl,
        buffer_bytes: b.echo_size_bytes,
        hardware_tail_bytes: 4,
        esa: if b.echo_enabled {
            (b.echo_start >> 8) as u8
        } else {
            0
        },
        percent_of_aram: (b.echo_size_bytes as f64) * 100.0 / (ARAM_LEN as f64),
        writeback_safe: !(b.echo_enabled && b.edl == 0),
    };
    let source_directory = AramSourceDirSummary {
        source_count: b.total_sources,
        bytes: b.srcdir_bytes,
        padding_bytes: b.srcdir_padding,
        start_addr: b.srcdir_start,
    };
    let total_brr_bytes: u32 = b.per_sample.iter().map(|p| p.bytes).sum();
    let samples = AramSamplesSummary {
        total_samples: b.sample_count,
        total_brr_bytes,
        per_sample: b.per_sample,
    };
    let atoms = if b.atom_count > 0 {
        let total_atom_bytes: u32 = b.per_atom.iter().map(|p| p.bytes).sum();
        Some(AramAtomsSummary {
            total_atoms: b.atom_count,
            total_brr_bytes: total_atom_bytes,
            per_atom: b.per_atom.clone(),
        })
    } else {
        None
    };
    if !echo.writeback_safe {
        warnings.push("ECHO_WRITEBACK_HAZARD_AT_EDL_ZERO".to_string());
    }
    if b.echo_enabled && b.echo_start >= 0xFE00 {
        warnings.push("ECHO_NEAR_TOP_OF_ARAM_REVIEW_IPL_BIT".to_string());
    }
    if free_bytes < 256 {
        warnings.push("FREE_LESS_THAN_256_BYTES".to_string());
    }

    AramMapReport {
        schema_version: SCHEMA_VERSION,
        report_type: AramMapReport::REPORT_TYPE.to_string(),
        total_aram: ARAM_LEN as u32,
        regions,
        free_bytes,
        collisions: Vec::<AramCollision>::new(),
        echo: Some(echo),
        source_directory: Some(source_directory),
        samples: Some(samples),
        atoms,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{
        Driver, Envelope, M1Block, MasterEcho, Project, ProjectV1, SampleFormat, SampleLoop,
        SamplePlayback, SampleSlot, SampleSource,
    };

    fn project_with(samples: Vec<SampleSlot>, master_echo: MasterEcho, active: &str) -> ProjectV1 {
        ProjectV1 {
            schema_version: 1,
            project: Project {
                name: "test".to_string(),
                tick_rate_hz: 60,
            },
            driver: Driver {
                profile: "sample_basic".to_string(),
                bytecode_version: 1,
            },
            master_echo,
            sample_pool: samples,
            m1: M1Block {
                active_sample_id: active.to_string(),
            },
        }
    }

    fn sample(id: &str, frames: u64, looped: Option<(u32, u32)>) -> SampleSlot {
        SampleSlot {
            id: id.to_string(),
            name: id.to_string(),
            source: SampleSource {
                path: format!("audio/{id}.wav"),
                sha256: "0".repeat(64),
                format: SampleFormat::Wav,
                sample_rate_hz: 32000,
                channels: 1,
                frames,
            },
            root_midi_note: 60,
            looped: SampleLoop {
                enabled: looped.is_some(),
                start_sample: looped.map(|(s, _)| s),
                end_sample: looped.map(|(_, e)| e),
                snap: looped.map(|_| "brr_block_16".to_string()),
            },
            playback: SamplePlayback {
                volume: 1.0,
                pan: 0.0,
                echo: false,
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        }
    }

    fn echo_off() -> MasterEcho {
        MasterEcho {
            enabled: false,
            edl: 0,
            efb: 0,
            evol_l: 0,
            evol_r: 0,
            fir: [0; 8],
        }
    }

    fn echo_on(edl: u8) -> MasterEcho {
        MasterEcho {
            enabled: true,
            edl,
            efb: 0,
            evol_l: 0,
            evol_r: 0,
            fir: [0; 8],
        }
    }

    fn driver_zeros(len: usize) -> Vec<u8> {
        vec![0u8; len]
    }

    fn brr_zeros(blocks: usize) -> Vec<u8> {
        vec![0u8; blocks * 9]
    }

    #[test]
    fn pack_empty_pool_errors() {
        // Direct construction: bypass project validation since rule
        // says sample_pool.len() >= 1 (caller guard).
        let project = project_with(Vec::new(), echo_off(), "sample_0001");
        let err = pack(PackInput {
            project,
            encoded_samples: Vec::new(),
            driver_code: driver_zeros(0),
        })
        .unwrap_err();
        assert!(matches!(err, PackError::EmptyPool));
    }

    #[test]
    fn pack_one_sample_no_loop_no_echo() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s.clone()], echo_off(), "a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(2),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();

        // Source dir is one entry at $1200.
        assert_eq!(r.aram_image[0x1200], 0x00); // start_addr lo (= $1300 because srcdir page-pads)
        assert_eq!(r.aram_image[0x1201], 0x13); // start_addr hi
        assert_eq!(r.aram_image[0x1202], 0x00); // loop_addr lo (= start_addr; no loop)
        assert_eq!(r.aram_image[0x1203], 0x13);

        // BRR pool starts at $1300 with the all-zero block (already 0
        // in the zero-filled image, so verify region extents instead).
        let summ = r.map_report.samples.as_ref().unwrap();
        assert_eq!(summ.total_samples, 1);
        assert_eq!(summ.total_brr_bytes, 18);
        assert_eq!(summ.per_sample[0].start_addr, 0x1300);
        assert_eq!(summ.per_sample[0].loop_addr, Some(0x1300));
        let echo = r.map_report.echo.as_ref().unwrap();
        assert!(!echo.enabled);
        assert_eq!(echo.buffer_bytes, 0);
    }

    #[test]
    fn pack_one_sample_with_loop_no_echo() {
        // 4 blocks (64 samples) with loop at sample 16 → block 1.
        let s = sample("loop_a", 64, Some((16, 64)));
        let project = project_with(vec![s], echo_off(), "loop_a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "loop_a".to_string(),
                bytes: brr_zeros(4),
                loop_entry_block: Some(1),
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();

        // start_addr = 0x1300; loop_addr = start_addr + 1*9 = 0x1309.
        let entry = &r.map_report.samples.as_ref().unwrap().per_sample[0];
        assert_eq!(entry.start_addr, 0x1300);
        assert_eq!(entry.loop_addr, Some(0x1309));
        // Source dir loop bytes encode 0x1309.
        assert_eq!(r.aram_image[0x1202], 0x09);
        assert_eq!(r.aram_image[0x1203], 0x13);
    }

    #[test]
    fn pack_with_echo_enabled_edl_4() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s], echo_on(4), "a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(2),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();

        let echo = r.map_report.echo.as_ref().unwrap();
        assert!(echo.enabled);
        assert_eq!(echo.edl, 4);
        assert_eq!(echo.buffer_bytes, 8192);
        // M1 conservative: echo ends at $FF00 (page-aligned, just below
        // the IPL pad). echo_start = $DF00, ESA = $DF.
        assert_eq!(echo.esa, 0xDF);
        assert!(echo.writeback_safe);
        let percent = echo.percent_of_aram;
        assert!(
            (percent - 12.5).abs() < 0.01,
            "expected 12.5%, got {percent}"
        );

        // Region list contains the echo buffer at the right bounds.
        let echo_region = r
            .map_report
            .regions
            .iter()
            .find(|x| x.name == "echo_buffer")
            .unwrap();
        assert_eq!(echo_region.start, "0xDF00");
        assert_eq!(echo_region.end, "0xFEFF");
        assert_eq!(echo_region.bytes, 8192);
    }

    #[test]
    fn pack_with_echo_overlapping_samples_errors() {
        // edl=15 → 30720-byte echo; pool start = $1300, echo_start =
        // $FF00 - $7800 = $8700. Available pool: $8700 - $1300 = $7400
        // = 29696 B. 3300 nine-byte blocks = 29700 B → overflows by 4.
        let s = sample("a", 1, None);
        let project = project_with(vec![s], echo_on(15), "a");
        let err = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(3300),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap_err();
        assert!(matches!(err, PackError::EchoOverlap { .. }), "got {err:?}");
    }

    #[test]
    fn pack_three_samples_layout_invariants() {
        let project = project_with(
            vec![
                sample("a", 32, None),
                sample("b", 32, None),
                sample("c", 32, None),
            ],
            echo_off(),
            "a",
        );
        let r = pack(PackInput {
            project,
            encoded_samples: vec![
                EncodedSample {
                    sample_id: "a".to_string(),
                    bytes: brr_zeros(2),
                    loop_entry_block: None,
                },
                EncodedSample {
                    sample_id: "b".to_string(),
                    bytes: brr_zeros(3),
                    loop_entry_block: None,
                },
                EncodedSample {
                    sample_id: "c".to_string(),
                    bytes: brr_zeros(4),
                    loop_entry_block: None,
                },
            ],
            driver_code: driver_zeros(0),
        })
        .unwrap();
        let summ = r.map_report.samples.as_ref().unwrap();
        assert_eq!(summ.total_samples, 3);
        assert_eq!(summ.total_brr_bytes, 18 + 27 + 36);
        assert_eq!(summ.per_sample[0].start_addr, 0x1300);
        assert_eq!(summ.per_sample[1].start_addr, 0x1300 + 18);
        assert_eq!(summ.per_sample[2].start_addr, 0x1300 + 18 + 27);
    }

    #[test]
    fn pack_source_directory_is_page_aligned() {
        for n in [1usize, 5, 64] {
            let mut samples = Vec::new();
            let mut encoded = Vec::new();
            for i in 0..n {
                let id = format!("s{i}");
                samples.push(sample(&id, 32, None));
                encoded.push(EncodedSample {
                    sample_id: id,
                    bytes: brr_zeros(1),
                    loop_entry_block: None,
                });
            }
            let project = project_with(samples, echo_off(), "s0");
            let r = pack(PackInput {
                project,
                encoded_samples: encoded,
                driver_code: driver_zeros(0),
            })
            .unwrap();
            let dir = r.map_report.source_directory.as_ref().unwrap();
            assert!(
                dir.start_addr.is_multiple_of(0x100),
                "srcdir start ${:04X} not page-aligned for n={n}",
                dir.start_addr
            );
        }
    }

    #[test]
    fn pack_driver_too_large_errors() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s], echo_off(), "a");
        let err = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(1),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(DRIVER_CODE_BUDGET_M1 as usize + 1),
        })
        .unwrap_err();
        assert!(
            matches!(err, PackError::DriverTooLarge { actual, max }
                if actual == DRIVER_CODE_BUDGET_M1 + 1 && max == DRIVER_CODE_BUDGET_M1),
            "got {err:?}"
        );
    }

    #[test]
    fn pack_encoded_sample_misaligned_errors() {
        let s = sample("a", 16, None);
        let project = project_with(vec![s], echo_off(), "a");
        let err = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: vec![0u8; 8], // not a multiple of 9
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap_err();
        assert!(
            matches!(err, PackError::EncodedSampleMisaligned { actual: 8, .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn pack_image_total_bytes_equal_64k() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s], echo_off(), "a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(1),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();
        assert_eq!(r.aram_image.len(), 0x10000);
    }

    #[test]
    fn pack_no_writes_in_fixed_runtime_regions() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s], echo_off(), "a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(1),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();
        for b in r.aram_image[..0x0200].iter() {
            assert_eq!(*b, 0);
        }
    }

    #[test]
    fn map_report_regions_partition_total_aram() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s], echo_on(4), "a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(2),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();
        let total: u32 = r.map_report.regions.iter().map(|x| x.bytes).sum();
        assert_eq!(total, r.map_report.total_aram);
        assert_eq!(r.map_report.total_aram, 65536);
        let claimed_free: u32 = r
            .map_report
            .regions
            .iter()
            .filter(|x| x.kind == AramKind::Free)
            .map(|x| x.bytes)
            .sum();
        assert_eq!(claimed_free, r.map_report.free_bytes);
    }

    #[test]
    fn map_report_no_collisions_on_valid_pack() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s], echo_off(), "a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(1),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();
        assert!(r.map_report.collisions.is_empty());
    }

    #[test]
    fn map_report_echo_summary_consistent_with_image() {
        let s = sample("a", 32, None);
        let project = project_with(vec![s], echo_on(4), "a");
        let r = pack(PackInput {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(1),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();
        let echo = r.map_report.echo.unwrap();
        let echo_region = r
            .map_report
            .regions
            .iter()
            .find(|x| x.name == "echo_buffer")
            .unwrap();
        // bytes match
        assert_eq!(echo.buffer_bytes, echo_region.bytes);
        // start_addr derived from ESA matches the region start
        let region_start =
            u32::from_str_radix(echo_region.start.trim_start_matches("0x"), 16).unwrap();
        assert_eq!(region_start as u8, 0); // page-aligned low byte
        assert_eq!((region_start >> 8) as u8, echo.esa);
    }

    // ===================================================================
    // M2.3 — pack_v2 multi-source layout tests.
    // ===================================================================

    use crate::project_v2::{M2Block, ProjectV2, Track, TrackKind};

    fn project_v2_sample_only(sample_id: &str) -> ProjectV2 {
        ProjectV2 {
            schema_version: 2,
            project: Project {
                name: "test_v2".to_string(),
                tick_rate_hz: 60,
            },
            driver: Driver {
                profile: "sample_basic".to_string(),
                bytecode_version: 1,
            },
            master_echo: echo_off(),
            sample_pool: vec![sample(sample_id, 32, None)],
            atom_pool: Vec::new(),
            atom_sequences: Vec::new(),
            tracks: vec![Track {
                id: "track_sample_0".to_string(),
                name: String::new(),
                voice: 0,
                kind: TrackKind::SampleSustain {
                    sample_id: sample_id.to_string(),
                },
            }],
            m2: M2Block {
                active_sequence_id: None,
            },
        }
    }

    #[test]
    fn pack_v2_sample_only_matches_v1_layout() {
        // The migrated-v1 / v2-sample-only pack path must produce
        // byte-identical ARAM to the v1 packer for the equivalent
        // project — this is the M2.1 bit-identity guarantee carried
        // forward into M2.3.
        let v1_project = project_with(vec![sample("a", 32, None)], echo_off(), "a");
        let v1_out = pack(PackInput {
            project: v1_project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(2),
                loop_entry_block: None,
            }],
            driver_code: driver_zeros(0),
        })
        .unwrap();

        let v2_project = project_v2_sample_only("a");
        let v2_out = pack_v2(PackInputV2 {
            project: v2_project,
            encoded_samples: vec![EncodedSample {
                sample_id: "a".to_string(),
                bytes: brr_zeros(2),
                loop_entry_block: None,
            }],
            encoded_atoms: Vec::new(),
            driver_code: driver_zeros(0),
            sequence_data: None,
            voice_setup_table: None,
        })
        .unwrap();

        assert_eq!(
            v1_out.aram_image[..],
            v2_out.aram_image[..],
            "v2-sample-only ARAM must be bit-identical to v1 ARAM"
        );
        // Free-byte counts must match too (the v2 map report includes
        // the same regions when no M2 extras are present).
        assert_eq!(v1_out.map_report.free_bytes, v2_out.map_report.free_bytes);
    }

    fn project_v2_multi_voice() -> ProjectV2 {
        use crate::atom::{AtomKind, AtomPartial, AtomRenderOptions, AtomSlot};
        use crate::project_v2::{AtomSequence, AtomSequenceStep, AtomTransition};
        let mut p = project_v2_sample_only("lead");
        p.driver.profile = "multi_voice_atom".to_string();
        p.driver.bytecode_version = 2;
        let make_atom = |id: &str, cycle: u16| AtomSlot {
            id: id.to_string(),
            name: id.to_string(),
            kind: AtomKind::AdditiveSingleCycleV0 {
                partials: vec![AtomPartial {
                    harmonic: 1,
                    amplitude: 1.0,
                    phase_cycles: 0.0,
                }],
            },
            root_midi_note: 60,
            cycle_len_samples: cycle,
            amplitude: 0.75,
            render: AtomRenderOptions {
                normalize: true,
                force_filter_0_first_block: true,
                force_filter_0_loop_entry: true,
            },
            playback: SamplePlayback {
                volume: 0.8,
                pan: 0.0,
                echo: false,
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        };
        p.atom_pool = vec![make_atom("atom_a", 128), make_atom("atom_b", 64)];
        p.atom_sequences = vec![AtomSequence {
            id: "atomseq_0001".to_string(),
            name: "single".to_string(),
            voice: 1,
            steps: vec![AtomSequenceStep {
                atom_id: "atom_a".to_string(),
                duration_ticks: 120,
                target_volume: 0.8,
                transition: AtomTransition::InitialKon,
            }],
            looped: false,
        }];
        p.tracks.push(Track {
            id: "track_atom_1".to_string(),
            name: String::new(),
            voice: 1,
            kind: TrackKind::AtomSequence {
                atom_sequence_id: "atomseq_0001".to_string(),
            },
        });
        p.m2.active_sequence_id = Some("atomseq_0001".to_string());
        p
    }

    #[test]
    fn pack_v2_multi_voice_with_one_sample_two_atoms_layout() {
        let project = project_v2_multi_voice();
        let voice_table = vec![0u8; VOICE_SETUP_TABLE_M2_BYTES as usize];
        let r = pack_v2(PackInputV2 {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "lead".to_string(),
                bytes: brr_zeros(2),
                loop_entry_block: None,
            }],
            encoded_atoms: vec![
                EncodedSample {
                    sample_id: "atom_a".to_string(),
                    bytes: brr_zeros(8), // 128 / 16
                    loop_entry_block: Some(0),
                },
                EncodedSample {
                    sample_id: "atom_b".to_string(),
                    bytes: brr_zeros(4), // 64 / 16
                    loop_entry_block: Some(0),
                },
            ],
            driver_code: driver_zeros(0),
            sequence_data: None,
            voice_setup_table: Some(voice_table),
        })
        .unwrap();

        // Region order in the report: fixed -> driver -> source_directory
        // -> sample_brr_pool -> synth_atom_pool -> voice_setup_table
        // -> free -> ipl pad -> ipl shadow.
        let names: Vec<&str> = r
            .map_report
            .regions
            .iter()
            .map(|x| x.name.as_str())
            .collect();
        let positions = |name: &str| names.iter().position(|n| *n == name);
        let driver = positions("driver_code").unwrap();
        let srcdir = positions("source_directory").unwrap();
        let samples = positions("sample_brr_pool").unwrap();
        let atoms = positions("synth_atom_pool").unwrap();
        let voice = positions("voice_setup_table").unwrap();
        let free = positions("free").unwrap();
        assert!(driver < srcdir);
        assert!(srcdir < samples);
        assert!(samples < atoms);
        assert!(atoms < voice);
        assert!(voice < free);

        // Voice setup table is exactly 22 bytes.
        let voice_region = &r.map_report.regions[voice];
        assert_eq!(voice_region.bytes, VOICE_SETUP_TABLE_M2_BYTES);
        // Atom pool sized = 8*9 + 4*9 = 108.
        let atom_region = &r.map_report.regions[atoms];
        assert_eq!(atom_region.bytes, 8 * 9 + 4 * 9);

        // Atoms summary populated.
        let atoms_summ = r.map_report.atoms.as_ref().expect("atoms summary");
        assert_eq!(atoms_summ.total_atoms, 2);
        assert_eq!(atoms_summ.total_brr_bytes, 108);
        // SRCN ordering: samples first, then atoms in declaration
        // order. So atom_a -> SRCN 1, atom_b -> SRCN 2.
        assert_eq!(atoms_summ.per_atom[0].atom_id, "atom_a");
        assert_eq!(atoms_summ.per_atom[0].source_index, 1);
        assert_eq!(atoms_summ.per_atom[1].atom_id, "atom_b");
        assert_eq!(atoms_summ.per_atom[1].source_index, 2);
    }

    #[test]
    fn pack_v2_atom_source_directory_entries_correct() {
        let project = project_v2_multi_voice();
        let r = pack_v2(PackInputV2 {
            project,
            encoded_samples: vec![EncodedSample {
                sample_id: "lead".to_string(),
                bytes: brr_zeros(2),
                loop_entry_block: None,
            }],
            encoded_atoms: vec![
                EncodedSample {
                    sample_id: "atom_a".to_string(),
                    bytes: brr_zeros(8),
                    loop_entry_block: Some(0),
                },
                EncodedSample {
                    sample_id: "atom_b".to_string(),
                    bytes: brr_zeros(4),
                    loop_entry_block: Some(0),
                },
            ],
            driver_code: driver_zeros(0),
            sequence_data: None,
            voice_setup_table: Some(vec![0u8; VOICE_SETUP_TABLE_M2_BYTES as usize]),
        })
        .unwrap();

        // Source directory at $1200. Layout: 3 entries × 4 bytes = 12
        // bytes. SRCN 0 = sample (start $1300), SRCN 1 = atom_a, SRCN
        // 2 = atom_b. Sample ends at $1300 + 18 = $1312; atom_a starts
        // at $1312 (since no padding between sample pool and atom
        // pool); atom_b starts at $1312 + 72 = $135A.
        let srcn0_start = u16::from_le_bytes([r.aram_image[0x1200], r.aram_image[0x1201]]);
        let srcn1_start = u16::from_le_bytes([r.aram_image[0x1204], r.aram_image[0x1205]]);
        let srcn1_loop = u16::from_le_bytes([r.aram_image[0x1206], r.aram_image[0x1207]]);
        let srcn2_start = u16::from_le_bytes([r.aram_image[0x1208], r.aram_image[0x1209]]);
        let srcn2_loop = u16::from_le_bytes([r.aram_image[0x120A], r.aram_image[0x120B]]);
        assert_eq!(srcn0_start, 0x1300);
        assert_eq!(srcn1_start, 0x1300 + 18); // sample ends at $1312
        assert_eq!(srcn1_loop, srcn1_start, "atom loop_addr = start_addr");
        assert_eq!(srcn2_start, srcn1_start + 72); // atom_a is 72 bytes
        assert_eq!(srcn2_loop, srcn2_start, "atom_b loop_addr = start_addr");
    }
}
