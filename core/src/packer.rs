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
}
