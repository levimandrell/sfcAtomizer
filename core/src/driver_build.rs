//! M1.5 driver builder — produces the assembled `sample_basic`
//! driver bytes from a [`ProjectV1`] and the matching
//! [`AramMapReport`].
//!
//! Pipeline:
//!
//! 1. Resolve `m1.active_sample_id` against `sample_pool`.
//! 2. Compute every per-project DSP register value (voice 0
//!    pitch / volume / envelope, master volume, source-directory
//!    page, echo registers, FLG, status flags).
//! 3. Render `m1_constants.inc` as asar `name = $XX` lines.
//! 4. Copy `m1_sample_basic.asm` and the generated `.inc` into
//!    `working_dir`.
//! 5. Invoke [`AsarBackend::assemble`] (same `--no-title-check
//!    --fix-checksum=off` invocation as M0.3).
//! 6. Slice the driver bytes out of the 64 KB image: starting at
//!    `$0200`, ending immediately before the four-byte sentinel
//!    `$DE $AD $BE $EF` the driver source emits at `driver_end`.
//! 7. Bound-check against `DRIVER_CODE_BUDGET_M1` (4 KiB) and
//!    SHA-256 the result.
//!
//! Rationale for the sentinel: asar produces a 64 KB image with
//! the driver at offset `$0200` and zeros everywhere else. A
//! single-byte sentinel could collide with an instruction byte;
//! the four-byte pattern is distinctive enough to be safe in
//! practice and lets us avoid teaching this module about
//! per-instruction encoding.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

/// The canonical M1.5 driver assembly source, embedded at compile
/// time so the host doesn't depend on the workspace layout at
/// runtime.
pub const DRIVER_ASM_SRC: &str = include_str!("../fixtures/asm/m1_sample_basic.asm");

use crate::asm::{sha256_hex, AsarBackend, AssembleError, AssembleInput, AssemblerBackend};
use crate::packer::DRIVER_CODE_BUDGET_M1;
use crate::pitch::{pitch_register, split_pitch};
use crate::project::{Envelope, ProjectV1};
use crate::report::AramMapReport;

/// Sentinel pattern emitted by `m1_sample_basic.asm` after the
/// last instruction. Used to find the driver's end byte.
pub const DRIVER_END_SENTINEL: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];

/// Driver version byte sent in `driver_out_2` of the ready
/// signature (SPEC §20.1). M1 driver = 1.
pub const DRIVER_VERSION_M1: u8 = 1;

/// Hard-coded master volume for M1. The project schema (§16) does
/// not currently expose a project-level master vol field; M1
/// pins both channels to `$7F` (max). Document deviation in
/// STATUS decisions log if a future pass adds a master-vol field.
pub const MASTER_VOLUME_M1: u8 = 0x7F;

/// FLG bit 5 (ECEN) — echo write disable.
const FLG_ECHO_WRITE_DISABLE: u8 = 0x20;

#[derive(Debug, Clone)]
pub struct DriverBuildInput<'a> {
    /// Project that has already passed [`ProjectV1::validate`].
    pub project: &'a ProjectV1,
    /// Map report from the M1 packer for the same project.
    /// `source_directory.start_addr` and `echo.esa` flow into the
    /// generated constants.
    pub map_report: &'a AramMapReport,
    /// Optional override for the driver `.asm` source. `None` uses
    /// the canonical [`DRIVER_ASM_SRC`] embedded in this crate.
    /// Tests use this to inject mutated sources (e.g. an oversized
    /// driver to force [`DriverBuildError::OverBudget`]).
    pub source_override: Option<&'a str>,
    /// Working directory for the constants `.inc` and the asar
    /// scratch image. Caller manages lifetime (typically a
    /// `tempfile::TempDir`).
    pub working_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DriverBuildOutput {
    pub driver_code: Vec<u8>,
    pub constants_inc_path: PathBuf,
    pub backend_version: String,
    pub driver_code_sha256: String,
}

#[derive(Debug, Error)]
pub enum DriverBuildError {
    #[error("active sample not found in pool: {0:?}")]
    ActiveSampleMissing(String),
    #[error("source_directory missing from map_report (M1.4 packer should always populate it)")]
    SourceDirectoryMissing,
    #[error("driver code {0} bytes exceeds M1 budget {1}")]
    OverBudget(u32, u32),
    #[error("driver_end sentinel {0:02X?} not found in assembled image")]
    SentinelMissing([u8; 4]),
    /// M2.0 (consultant #7): the sentinel pattern occurs inside the
    /// driver code itself, before the canonical `driver_end:` marker.
    /// Future M2 drivers are larger and increasingly likely to
    /// produce a collision by chance; resolve by adjusting the
    /// affected instruction sequence or rotating the sentinel in
    /// `core::driver_build::DRIVER_END_SENTINEL` and the driver
    /// `.asm`.
    #[error("driver sentinel collision: pattern {0:02X?} occurs at offset {1} within driver code")]
    SentinelCollision([u8; 4], usize),
    #[error("asar: {0}")]
    Assemble(#[from] AssembleError),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverConstants {
    pub voice0_voll: u8,
    pub voice0_volr: u8,
    pub voice0_pitchl: u8,
    pub voice0_pitchh: u8,
    pub voice0_srcn: u8,
    pub voice0_adsr1: u8,
    pub voice0_adsr2: u8,
    pub voice0_gain: u8,
    pub master_voll: u8,
    pub master_volr: u8,
    pub src_dir_page: u8,
    pub echo_efb: u8,
    pub echo_evoll: u8,
    pub echo_evolr: u8,
    pub echo_eon: u8,
    pub echo_esa: u8,
    pub echo_edl: u8,
    pub echo_fir: [u8; 8],
    pub flg_running: u8,
    pub status_flags_initial: u8,
}

pub fn build(input: DriverBuildInput<'_>) -> Result<DriverBuildOutput, DriverBuildError> {
    let constants = compute_constants(input.project, input.map_report)?;
    let inc_text = render_constants_inc(&constants, input.project);

    let constants_inc_path = input.working_dir.join("m1_constants.inc");
    std::fs::write(&constants_inc_path, &inc_text).map_err(|source| DriverBuildError::Io {
        path: constants_inc_path.clone(),
        source,
    })?;

    let asm_dest = input.working_dir.join("m1_sample_basic.asm");
    let asm_src_text = input.source_override.unwrap_or(DRIVER_ASM_SRC);
    std::fs::write(&asm_dest, asm_src_text).map_err(|source| DriverBuildError::Io {
        path: asm_dest.clone(),
        source,
    })?;

    let backend = AsarBackend::from_resolution()?;
    let backend_version = backend.version().unwrap_or_else(|_| "unknown".to_string());
    let image_path = input.working_dir.join("m1_driver.aram.bin");
    let asm_input =
        AssembleInput::for_spc700_aram(asm_dest, image_path.clone(), input.working_dir.clone());
    backend.assemble(&asm_input)?;

    let image = std::fs::read(&image_path).map_err(|source| DriverBuildError::Io {
        path: image_path.clone(),
        source,
    })?;

    let driver_start: usize = 0x0200;
    let sentinel_offset = find_sentinel(&image, driver_start)
        .ok_or(DriverBuildError::SentinelMissing(DRIVER_END_SENTINEL))?;
    let driver_code = image[driver_start..sentinel_offset].to_vec();
    if driver_code.len() as u32 > DRIVER_CODE_BUDGET_M1 {
        return Err(DriverBuildError::OverBudget(
            driver_code.len() as u32,
            DRIVER_CODE_BUDGET_M1,
        ));
    }
    // M2.0 (consultant #7): if the driver code accidentally
    // contains the sentinel pattern, the slicer above stops at the
    // first occurrence and silently truncates the real driver.
    // Detect this by scanning the image past what we treated as
    // `driver_end` — asar zero-fills the rest of the 64 KB image,
    // so any nonzero byte after the chosen sentinel + its 4 bytes
    // means the actual driver continues. As M2 drivers grow the
    // accidental-match probability goes up; this is the canary.
    let after_sentinel = sentinel_offset + DRIVER_END_SENTINEL.len();
    let scan_end = 0xFFC0; // skip IPL ROM shadow region
    for (i, &b) in image[after_sentinel..scan_end].iter().enumerate() {
        if b != 0 {
            return Err(DriverBuildError::SentinelCollision(
                DRIVER_END_SENTINEL,
                after_sentinel + i,
            ));
        }
    }

    let driver_code_sha256 = sha256_hex(&driver_code);

    Ok(DriverBuildOutput {
        driver_code,
        constants_inc_path,
        backend_version,
        driver_code_sha256,
    })
}

/// Compute the per-project register values consumed by
/// `m1_sample_basic.asm`. Pure function so it's unit-testable
/// without invoking asar.
pub fn compute_constants(
    project: &ProjectV1,
    map_report: &AramMapReport,
) -> Result<DriverConstants, DriverBuildError> {
    let active_id = &project.m1.active_sample_id;
    let (active_idx, active) = project
        .sample_pool
        .iter()
        .enumerate()
        .find(|(_, s)| &s.id == active_id)
        .ok_or_else(|| DriverBuildError::ActiveSampleMissing(active_id.clone()))?;

    // Per-voice volume / pan (constant-power, SPEC §16.4).
    let (voll, volr) = playback_to_voll_volr(active.playback.volume, active.playback.pan);

    // Per-voice pitch (SPEC §16.7 with M1 collapse: desired = root, cents = 0).
    let pitch_u16 = pitch_register(
        active.source.sample_rate_hz,
        active.root_midi_note,
        active.root_midi_note,
        0,
    );
    let (pitchl, pitchh) = split_pitch(pitch_u16);

    // Envelope mapping (SPEC §16.4 ADSR / GAIN register layout).
    let (adsr1, adsr2, gain) = match active.playback.envelope {
        Envelope::Adsr {
            attack,
            decay,
            sustain_level,
            sustain_rate,
        } => {
            let adsr1 = 0x80 | ((decay & 0x07) << 4) | (attack & 0x0F);
            let adsr2 = ((sustain_level & 0x07) << 5) | (sustain_rate & 0x1F);
            (adsr1, adsr2, 0x00)
        }
        Envelope::GainRaw { gain_byte } => (0x00, 0x00, gain_byte),
    };

    // Source directory page (high byte of map's start_addr).
    let src_dir = map_report
        .source_directory
        .as_ref()
        .ok_or(DriverBuildError::SourceDirectoryMissing)?;
    let src_dir_page = (src_dir.start_addr >> 8) as u8;

    // Echo registers.
    let me = &project.master_echo;
    let echo_eon = if active.playback.echo && me.enabled {
        0x01
    } else {
        0x00
    };
    let echo_esa = match map_report.echo.as_ref() {
        Some(e) if e.enabled => e.esa,
        _ => 0,
    };
    let echo_edl = me.edl;
    let echo_fir: [u8; 8] = [
        me.fir[0] as u8,
        me.fir[1] as u8,
        me.fir[2] as u8,
        me.fir[3] as u8,
        me.fir[4] as u8,
        me.fir[5] as u8,
        me.fir[6] as u8,
        me.fir[7] as u8,
    ];

    // FLG running. ECEN bit clear = echo write enabled.
    let flg_running = if me.enabled {
        0x00
    } else {
        FLG_ECHO_WRITE_DISABLE
    };

    // Status flags initial: voice0_active=1 (we KON), echo_enabled
    // mirrors master_echo.enabled.
    let mut status_flags_initial = 0x01u8;
    if me.enabled {
        status_flags_initial |= 0x02;
    }

    Ok(DriverConstants {
        voice0_voll: voll,
        voice0_volr: volr,
        voice0_pitchl: pitchl,
        voice0_pitchh: pitchh,
        voice0_srcn: active_idx as u8,
        voice0_adsr1: adsr1,
        voice0_adsr2: adsr2,
        voice0_gain: gain,
        master_voll: MASTER_VOLUME_M1,
        master_volr: MASTER_VOLUME_M1,
        src_dir_page,
        echo_efb: me.efb as u8,
        echo_evoll: me.evol_l as u8,
        echo_evolr: me.evol_r as u8,
        echo_eon,
        echo_esa,
        echo_edl,
        echo_fir,
        flg_running,
        status_flags_initial,
    })
}

/// Constant-power pan to `(VxVOLL, VxVOLR)` (SPEC §16.4). Both
/// outputs are in `0..=127`.
pub fn playback_to_voll_volr(volume: f64, pan: f64) -> (u8, u8) {
    use std::f64::consts::PI;
    let pan = pan.clamp(-1.0, 1.0);
    let volume = volume.clamp(0.0, 1.0);
    let theta = (pan + 1.0) * PI / 4.0;
    let l = 127.0 * volume * theta.cos();
    let r = 127.0 * volume * theta.sin();
    let li = (l + 0.5).floor().clamp(0.0, 127.0) as u8;
    let ri = (r + 0.5).floor().clamp(0.0, 127.0) as u8;
    (li, ri)
}

/// Render the asar `.inc` file. Format mirrors the layout
/// `m1_sample_basic.asm` expects.
pub fn render_constants_inc(c: &DriverConstants, project: &ProjectV1) -> String {
    let mut s = String::with_capacity(1024);
    s.push_str("; Auto-generated by core::driver_build for project \"");
    s.push_str(&project.project.name);
    s.push_str("\"\n");
    s.push_str("; Active sample: ");
    s.push_str(&project.m1.active_sample_id);
    s.push('\n');
    s.push_str("; Driver version (M1) = 1\n\n");

    push_eq(&mut s, "voice0_voll", c.voice0_voll);
    push_eq(&mut s, "voice0_volr", c.voice0_volr);
    push_eq(&mut s, "voice0_pitchl", c.voice0_pitchl);
    push_eq(&mut s, "voice0_pitchh", c.voice0_pitchh);
    push_eq(&mut s, "voice0_srcn", c.voice0_srcn);
    push_eq(&mut s, "voice0_adsr1", c.voice0_adsr1);
    push_eq(&mut s, "voice0_adsr2", c.voice0_adsr2);
    push_eq(&mut s, "voice0_gain", c.voice0_gain);
    push_eq(&mut s, "master_voll", c.master_voll);
    push_eq(&mut s, "master_volr", c.master_volr);
    push_eq(&mut s, "src_dir_page", c.src_dir_page);
    push_eq(&mut s, "echo_efb", c.echo_efb);
    push_eq(&mut s, "echo_evoll", c.echo_evoll);
    push_eq(&mut s, "echo_evolr", c.echo_evolr);
    push_eq(&mut s, "echo_eon", c.echo_eon);
    push_eq(&mut s, "echo_esa", c.echo_esa);
    push_eq(&mut s, "echo_edl", c.echo_edl);
    for (i, b) in c.echo_fir.iter().enumerate() {
        let name = format!("echo_fir_{i}");
        push_eq(&mut s, &name, *b);
    }
    push_eq(&mut s, "flg_running", c.flg_running);
    push_eq(&mut s, "status_flags_initial", c.status_flags_initial);
    s
}

fn push_eq(out: &mut String, name: &str, value: u8) {
    use std::fmt::Write as _;
    let _ = writeln!(out, "{name:24} = ${value:02X}");
}

fn find_sentinel(image: &[u8], from: usize) -> Option<usize> {
    if image.len() < DRIVER_END_SENTINEL.len() {
        return None;
    }
    image[from..]
        .windows(DRIVER_END_SENTINEL.len())
        .position(|w| w == DRIVER_END_SENTINEL)
        .map(|off| from + off)
}

/// SHA-256 helper duplicated so callers don't need to import
/// [`sha2`] directly.
pub fn sha256_of_constants(c: &DriverConstants) -> String {
    let mut h = Sha256::new();
    h.update([
        c.voice0_voll,
        c.voice0_volr,
        c.voice0_pitchl,
        c.voice0_pitchh,
        c.voice0_srcn,
        c.voice0_adsr1,
        c.voice0_adsr2,
        c.voice0_gain,
        c.master_voll,
        c.master_volr,
        c.src_dir_page,
        c.echo_efb,
        c.echo_evoll,
        c.echo_evolr,
        c.echo_eon,
        c.echo_esa,
        c.echo_edl,
    ]);
    h.update(c.echo_fir);
    h.update([c.flg_running, c.status_flags_initial]);
    let d = h.finalize();
    let mut s = String::with_capacity(64);
    for b in d {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Test-only helper: workspace-relative path to the canonical
/// driver assembly source. Pre-`DRIVER_ASM_SRC` callers used this
/// to feed `source_asm_path`; the build path now embeds the
/// source via [`DRIVER_ASM_SRC`], but the test fixture path
/// remains useful for "the .asm file is committed" checks.
pub fn workspace_driver_asm_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("asm")
        .join("m1_sample_basic.asm")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{
        Driver, M1Block, MasterEcho, Project, SampleFormat, SampleLoop, SamplePlayback, SampleSlot,
        SampleSource,
    };
    use crate::report::{AramMapReport, AramSourceDirSummary};

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

    fn project_one(env: Envelope) -> ProjectV1 {
        ProjectV1 {
            schema_version: 1,
            project: Project {
                name: "demo".to_string(),
                tick_rate_hz: 60,
            },
            driver: Driver {
                profile: "sample_basic".to_string(),
                bytecode_version: 1,
            },
            master_echo: echo_off(),
            sample_pool: vec![SampleSlot {
                id: "lead".to_string(),
                name: "lead".to_string(),
                source: SampleSource {
                    path: "audio/lead.wav".to_string(),
                    sha256: "0".repeat(64),
                    format: SampleFormat::Wav,
                    sample_rate_hz: 32_000,
                    channels: 1,
                    frames: 256,
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
                    envelope: env,
                },
            }],
            m1: M1Block {
                active_sample_id: "lead".to_string(),
            },
        }
    }

    fn map_with(src_dir_addr: u16) -> AramMapReport {
        let mut m = AramMapReport::stub();
        m.source_directory = Some(AramSourceDirSummary {
            source_count: 1,
            bytes: 4,
            padding_bytes: 252,
            start_addr: src_dir_addr,
        });
        m
    }

    #[test]
    fn pan_to_voll_volr_center() {
        let (l, r) = playback_to_voll_volr(1.0, 0.0);
        assert_eq!((l, r), (90, 90)); // 127 * cos(π/4) ≈ 89.8 → 90
    }

    #[test]
    fn pan_to_voll_volr_hard_left() {
        assert_eq!(playback_to_voll_volr(1.0, -1.0), (127, 0));
    }

    #[test]
    fn pan_to_voll_volr_hard_right() {
        assert_eq!(playback_to_voll_volr(1.0, 1.0), (0, 127));
    }

    #[test]
    fn pan_to_voll_volr_half_volume_center() {
        let (l, r) = playback_to_voll_volr(0.5, 0.0);
        // 127 * 0.5 * cos(π/4) ≈ 44.9 → 45
        assert_eq!((l, r), (45, 45));
    }

    #[test]
    fn envelope_adsr_register_mapping() {
        let p = project_one(Envelope::Adsr {
            attack: 9,
            decay: 4,
            sustain_level: 5,
            sustain_rate: 12,
        });
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        // ADSR1 = $80 | (4 << 4) | 9 = $80 | $40 | $09 = $C9
        assert_eq!(c.voice0_adsr1, 0xC9);
        // ADSR2 = (5 << 5) | 12 = $A0 | $0C = $AC
        assert_eq!(c.voice0_adsr2, 0xAC);
        assert_eq!(c.voice0_gain, 0x00);
    }

    #[test]
    fn envelope_gain_register_mapping() {
        let p = project_one(Envelope::GainRaw { gain_byte: 127 });
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        assert_eq!(c.voice0_adsr1, 0x00);
        assert_eq!(c.voice0_adsr2, 0x00);
        assert_eq!(c.voice0_gain, 127);
    }

    #[test]
    fn flg_running_with_echo_enabled() {
        let mut p = project_one(Envelope::GainRaw { gain_byte: 127 });
        p.master_echo = MasterEcho {
            enabled: true,
            edl: 4,
            efb: 0,
            evol_l: 0,
            evol_r: 0,
            fir: [0; 8],
        };
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        assert_eq!(c.flg_running, 0x00);
    }

    #[test]
    fn flg_running_with_echo_disabled() {
        let p = project_one(Envelope::GainRaw { gain_byte: 127 });
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        assert_eq!(c.flg_running, FLG_ECHO_WRITE_DISABLE);
    }

    #[test]
    fn status_flags_initial_with_echo() {
        let mut p = project_one(Envelope::GainRaw { gain_byte: 127 });
        p.master_echo = MasterEcho {
            enabled: true,
            edl: 4,
            efb: 0,
            evol_l: 0,
            evol_r: 0,
            fir: [0; 8],
        };
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        assert_eq!(c.status_flags_initial, 0x03); // voice0_active | echo_enabled
    }

    #[test]
    fn status_flags_initial_without_echo() {
        let p = project_one(Envelope::GainRaw { gain_byte: 127 });
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        assert_eq!(c.status_flags_initial, 0x01); // voice0_active only
    }

    #[test]
    fn pitch_for_32k_at_root_is_0x1000() {
        let p = project_one(Envelope::GainRaw { gain_byte: 127 });
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        assert_eq!(c.voice0_pitchl, 0x00);
        assert_eq!(c.voice0_pitchh, 0x10);
    }

    #[test]
    fn src_dir_page_from_map_report() {
        let p = project_one(Envelope::GainRaw { gain_byte: 127 });
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        assert_eq!(c.src_dir_page, 0x12);
    }

    #[test]
    fn active_sample_missing_errors() {
        let mut p = project_one(Envelope::GainRaw { gain_byte: 127 });
        p.m1.active_sample_id = "nope".to_string();
        let err = compute_constants(&p, &map_with(0x1200)).unwrap_err();
        assert!(matches!(err, DriverBuildError::ActiveSampleMissing(_)));
    }

    #[test]
    fn constants_inc_text_format() {
        let p = project_one(Envelope::GainRaw { gain_byte: 127 });
        let c = compute_constants(&p, &map_with(0x1200)).unwrap();
        let text = render_constants_inc(&c, &p);
        // A few spot checks on shape.
        assert!(text.contains("voice0_voll"));
        assert!(text.contains("voice0_pitchl"));
        assert!(text.contains("voice0_pitchh"));
        assert!(text.contains("echo_fir_7"));
        assert!(text.contains("flg_running"));
        // No double-percent or stray $$ formatting.
        assert!(!text.contains("$$"));
    }
}
