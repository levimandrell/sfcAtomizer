//! ARAM-image inspection for the M0 acceptance harness.
//!
//! [`map_from_image`] is a stopgap. It walks the assembled 64 KB
//! image looking at the `$0200..=$FFBF` driver-code region for the
//! first/last nonzero byte, reports that span as `driver_code`, and
//! treats everything else inside that range as `free`. The three
//! SPEC §15.1 fixed regions (direct page, hardware I/O, stack) are
//! emitted as-is.
//!
//! This is valid for "single contiguous driver, no sample pool, no
//! atom pool yet" — i.e. M0.3's smoke output. The real ARAM packer
//! lands in M3+ and replaces this whole module.

use crate::report::{AramKind, AramMapReport, AramRegion, SCHEMA_VERSION};

/// Total ARAM size, echoes [`AramMapReport::TOTAL_ARAM`] but typed
/// as `usize` for indexing.
pub const ARAM_LEN: usize = AramMapReport::TOTAL_ARAM as usize;

/// Driver code region: SPEC §15.1 places it at `$0200..` — the
/// first byte after the fixed runtime regions.
pub const DRIVER_CODE_START: usize = 0x0200;

/// Top of the driver-allocatable range. `$FFC0..$FFFF` is the IPL ROM
/// shadow per SPEC §15.1 and is not driver-allocatable in M0.4's
/// stopgap walk.
pub const DRIVER_CODE_END_EXCLUSIVE: usize = 0xFFC0;

/// Walk an ARAM image and produce a [`AramMapReport`].
///
/// The driver-code extent is the smallest span covering every
/// nonzero byte in `$0200..$FFC0`. If no nonzero byte exists, no
/// `driver_code` region is emitted; the entire `$0200..$FFC0` range
/// becomes one `free` region.
pub fn map_from_image(aram: &[u8; ARAM_LEN]) -> AramMapReport {
    let mut regions: Vec<AramRegion> = Vec::new();

    // Fixed regions (SPEC §15.1).
    regions.push(AramRegion {
        name: "direct_page".to_string(),
        start: format_addr(0x0000),
        end: format_addr(0x00EF),
        bytes: 0xF0,
        kind: AramKind::FixedRuntime,
    });
    regions.push(AramRegion {
        name: "hardware_io".to_string(),
        start: format_addr(0x00F0),
        end: format_addr(0x00FF),
        bytes: 0x10,
        kind: AramKind::FixedHardware,
    });
    regions.push(AramRegion {
        name: "stack".to_string(),
        start: format_addr(0x0100),
        end: format_addr(0x01FF),
        bytes: 0x100,
        kind: AramKind::FixedRuntime,
    });

    // Driver-code extent + free fillers within $0200..$FFC0.
    let scan = &aram[DRIVER_CODE_START..DRIVER_CODE_END_EXCLUSIVE];
    let first_nz = scan.iter().position(|&b| b != 0);
    let last_nz = scan.iter().rposition(|&b| b != 0);

    if let (Some(first), Some(last)) = (first_nz, last_nz) {
        let code_start = DRIVER_CODE_START + first;
        let code_end = DRIVER_CODE_START + last; // inclusive

        // Free region before driver code, if any.
        if code_start > DRIVER_CODE_START {
            regions.push(free_region(DRIVER_CODE_START, code_start - 1));
        }

        // Driver code itself.
        regions.push(AramRegion {
            name: "driver_code".to_string(),
            start: format_addr(code_start),
            end: format_addr(code_end),
            bytes: (code_end - code_start + 1) as u32,
            kind: AramKind::DriverCode,
        });

        // Free region after driver code, if any.
        if code_end + 1 < DRIVER_CODE_END_EXCLUSIVE {
            regions.push(free_region(code_end + 1, DRIVER_CODE_END_EXCLUSIVE - 1));
        }
    } else {
        // Image is entirely zero in the driver-code range.
        regions.push(free_region(
            DRIVER_CODE_START,
            DRIVER_CODE_END_EXCLUSIVE - 1,
        ));
    }

    // Top-of-ARAM IPL ROM shadow ($FFC0..$FFFF). Reported as fixed
    // hardware: only usable as RAM when the driver explicitly unmaps
    // the IPL ROM (SPEC §15.1).
    regions.push(AramRegion {
        name: "ipl_rom_shadow".to_string(),
        start: format_addr(0xFFC0),
        end: format_addr(0xFFFF),
        bytes: 0x40,
        kind: AramKind::FixedHardware,
    });

    let free_bytes: u32 = regions
        .iter()
        .filter(|r| r.kind == AramKind::Free)
        .map(|r| r.bytes)
        .sum();

    AramMapReport {
        schema_version: SCHEMA_VERSION,
        report_type: AramMapReport::REPORT_TYPE.to_string(),
        total_aram: AramMapReport::TOTAL_ARAM,
        regions,
        free_bytes,
        collisions: Vec::new(),
        // Byte-scan path has no project context to derive these from.
        echo: None,
        source_directory: None,
        samples: None,
        atoms: None,
        warnings: Vec::new(),
    }
}

fn free_region(start: usize, end_inclusive: usize) -> AramRegion {
    AramRegion {
        name: "free".to_string(),
        start: format_addr(start),
        end: format_addr(end_inclusive),
        bytes: (end_inclusive - start + 1) as u32,
        kind: AramKind::Free,
    }
}

fn format_addr(addr: usize) -> String {
    format!("0x{addr:04X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_image() -> Box<[u8; ARAM_LEN]> {
        Box::new([0u8; ARAM_LEN])
    }

    fn smoke_image() -> Box<[u8; ARAM_LEN]> {
        let mut a = zero_image();
        a[0x0200] = 0x00;
        a[0x0201] = 0x2F;
        a[0x0202] = 0xFD;
        a
    }

    #[test]
    fn all_zero_image_has_no_driver_code() {
        let r = map_from_image(&zero_image());
        assert!(
            r.regions.iter().all(|x| x.kind != AramKind::DriverCode),
            "expected no driver_code on all-zero image: {:?}",
            r.regions
        );
        // Single big free region for $0200..$FFBF.
        let frees: Vec<&AramRegion> = r
            .regions
            .iter()
            .filter(|x| x.kind == AramKind::Free)
            .collect();
        assert_eq!(frees.len(), 1);
        assert_eq!(frees[0].start, "0x0200");
        assert_eq!(frees[0].end, "0xFFBF");
    }

    #[test]
    fn smoke_image_locates_three_byte_driver() {
        let r = map_from_image(&smoke_image());
        let driver: &AramRegion = r
            .regions
            .iter()
            .find(|x| x.kind == AramKind::DriverCode)
            .expect("driver_code region present");
        // 0x0200 is NOP = 0x00 (zero in image). The smoke image has
        // 0x00 at 0x0200, 0x2F at 0x0201, 0xFD at 0x0202. First
        // nonzero byte is at 0x0201 — driver_code starts there.
        assert_eq!(driver.start, "0x0201");
        assert_eq!(driver.end, "0x0202");
        assert_eq!(driver.bytes, 2);
    }

    #[test]
    fn smoke_image_with_explicit_nop_byte() {
        // Treat the NOP byte as nonzero by writing a sentinel. This
        // forces the driver_code extent to include 0x0200 even if the
        // assembled NOP coincidentally equals the pre-fill.
        let mut a = zero_image();
        a[0x0200] = 0x01;
        a[0x0201] = 0x2F;
        a[0x0202] = 0xFD;
        let r = map_from_image(&a);
        let driver = r
            .regions
            .iter()
            .find(|x| x.kind == AramKind::DriverCode)
            .unwrap();
        assert_eq!(driver.start, "0x0200");
        assert_eq!(driver.end, "0x0202");
        assert_eq!(driver.bytes, 3);
    }

    #[test]
    fn regions_partition_total_aram() {
        for img in [zero_image(), smoke_image()] {
            let r = map_from_image(&img);
            let total: u32 = r.regions.iter().map(|x| x.bytes).sum();
            assert_eq!(
                total, r.total_aram,
                "regions must cover total ARAM exactly: {:?}",
                r.regions
            );
            assert_eq!(r.total_aram, 65536);

            let claimed_free: u32 = r
                .regions
                .iter()
                .filter(|x| x.kind == AramKind::Free)
                .map(|x| x.bytes)
                .sum();
            assert_eq!(r.free_bytes, claimed_free);
        }
    }

    #[test]
    fn ipl_rom_shadow_region_present() {
        let r = map_from_image(&zero_image());
        let ipl = r
            .regions
            .iter()
            .find(|x| x.name == "ipl_rom_shadow")
            .expect("ipl_rom_shadow region present");
        assert_eq!(ipl.start, "0xFFC0");
        assert_eq!(ipl.end, "0xFFFF");
        assert_eq!(ipl.bytes, 0x40);
        assert_eq!(ipl.kind, AramKind::FixedHardware);
    }
}
