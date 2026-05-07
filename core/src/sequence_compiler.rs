//! M2.4 sequence bytecode compiler — lowers `atom_sequences[].steps`
//! to SEQ2 bytecode per SPEC §14.3.
//!
//! For each step in declaration order:
//!
//! - **First step** (`transition: initial_kon`) lowers to:
//!
//!   ```text
//!   SET_SRC voice, src_index_for(atom_id)
//!   SET_VOL voice, vol_l, vol_r
//!   KON     (1 << voice)
//!   WAIT    duration_ticks
//!   ```
//!
//! - **Subsequent step** (`transition: fade_to_zero_retrigger {
//!   fade_out_ticks, fade_in_ticks }`) lowers to the §14.3
//!   source-step pattern:
//!
//!   ```text
//!   VOL_SLIDE voice, 0, 0, fade_out_ticks
//!   WAIT      fade_out_ticks
//!   KOFF      (1 << voice)
//!   WAIT      1                                  ; gap before SET_SRC
//!   SET_SRC   voice, src_index_for(next_atom_id)
//!   SET_VOL   voice, 0, 0
//!   KON       (1 << voice)
//!   VOL_SLIDE voice, target_l, target_r, fade_in_ticks
//!   WAIT      fade_in_ticks
//!   WAIT      duration_ticks                     ; sustain at target
//!   ```
//!
//! The full opcode stream is wrapped with the SEQ2 region header
//! ("SEQ2" magic + bytecode_version=2 + reserved + payload_len_le)
//! and terminated with `END` ($00).
//!
//! **No `SET_PITCH` in M2.4 default lowering.** The voice setup table
//! (SPEC §15.7) seeds the pitch register from the atom's
//! `root_midi_note` at driver init; the sequence compiler does not
//! re-emit `SET_PITCH`. Reserved for future passes.
//!
//! **Step duration semantics.** `duration_ticks` is the sustain time
//! at target volume AFTER any transition completes. Total step time
//! = `transition_ticks + duration_ticks`. For `initial_kon`,
//! `transition_ticks = 0`. For `fade_to_zero_retrigger`,
//! `transition_ticks = fade_out_ticks + 1 + fade_in_ticks`.
//!
//! **Capability enforcement.** The compiler rejects compilation when
//! the supplied capability manifest is missing one of:
//!
//! - `core_sequence_wait`
//! - `synth_atom_sequence`
//! - `synth_source_step` (only required if any step uses
//!   `fade_to_zero_retrigger`)
//! - `sample_runtime_src_change` (same)
//! - `volume_slide` (same)
//!
//! Single-step pure-`initial_kon` sequences compile under fewer
//! features (per Phase C of the M2.4 brief).

use std::collections::BTreeMap;

use thiserror::Error;

use crate::bytecode::{BytecodeOpcode, SequenceHeader, BYTECODE_VERSION_M2, SEQUENCE_HEADER_LEN};
use crate::capability_manifest::CapabilityManifest;
use crate::driver_build::playback_to_voll_volr;
use crate::project_v2::{AtomSequence, AtomTransition, ProjectV2};

/// Source-directory view: SRCN for sample = `sample_pool` index;
/// SRCN for atom = `sample_count + atom_pool` index. Mirrors the
/// M2.3 packer's source-directory ordering.
#[derive(Debug, Clone)]
pub struct SourceDirectory {
    pub sample_count: u32,
    pub atom_index_by_id: BTreeMap<String, u32>,
}

impl SourceDirectory {
    /// Build the source-directory view from a validated v2 project.
    pub fn from_project(project: &ProjectV2) -> Self {
        let mut atom_index_by_id = BTreeMap::new();
        for (i, atom) in project.atom_pool.iter().enumerate() {
            atom_index_by_id.insert(atom.id.clone(), i as u32);
        }
        Self {
            sample_count: project.sample_pool.len() as u32,
            atom_index_by_id,
        }
    }

    /// SRCN for `atom_id` (= `sample_count + atom-pool index`).
    pub fn src_index_for_atom(&self, atom_id: &str) -> Option<u32> {
        self.atom_index_by_id
            .get(atom_id)
            .map(|i| self.sample_count + *i)
    }
}

#[derive(Debug, Clone)]
pub struct SequenceCompileInput<'a> {
    pub project: &'a ProjectV2,
    pub manifest: &'a CapabilityManifest,
    pub source_directory: &'a SourceDirectory,
    pub sequence: &'a AtomSequence,
}

#[derive(Debug, Clone)]
pub struct SequenceCompileOutput {
    /// Full SEQ2 region (8-byte header + payload + END terminator).
    pub bytecode: Vec<u8>,
    /// Length of the payload (everything after the 8-byte header).
    pub bytecode_payload_len: u16,
    pub bytecode_sha256: String,
    pub max_writes_per_tick_estimate: u32,
    pub per_step: Vec<StepLowering>,
    /// Sum of all `WAIT` durations + transition ticks. Excludes
    /// final-tick KON/KOFF cycles past the last WAIT.
    pub total_ticks: u32,
    pub active_slides: Vec<ActiveSlideInterval>,
}

#[derive(Debug, Clone)]
pub struct StepLowering {
    pub step_index: u32,
    pub atom_id: String,
    pub voice: u8,
    /// Byte offset relative to the payload start (i.e. after the
    /// 8-byte header).
    pub bytecode_offset_start: u32,
    pub bytecode_offset_end: u32,
    pub max_writes_in_step: u32,
    pub tick_offset_start: u32,
    pub tick_offset_end: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveSlideInterval {
    pub voice: u8,
    /// Inclusive tick the slide first writes on.
    pub tick_start: u32,
    /// Exclusive tick the slide stops writing on.
    pub tick_end: u32,
}

#[derive(Debug, Error)]
pub enum SequenceCompileError {
    #[error("capability missing: {feature}")]
    CapabilityMissing { feature: String },
    #[error("atom_id {atom_id:?} not found in atom_pool")]
    AtomIdNotInPool { atom_id: String },
    #[error("first step transition must be initial_kon (got {actual:?} at step 0)")]
    FirstStepNotInitialKon { actual: String },
    #[error(
        "non-first step transition must be fade_to_zero_retrigger in M2 (got {actual:?} at step {step_index})"
    )]
    NonFirstStepWrongTransition { step_index: u32, actual: String },
    #[error(
        "write budget exceeded: tick {tick_index} would need {writes} DSP writes (limit {limit})"
    )]
    WriteBudgetExceeded {
        tick_index: u32,
        writes: u32,
        limit: u32,
    },
    #[error(
        "overlapping slides: voice {voice_a} slide ends at tick {end_a}, voice {voice_b} slide starts at tick {start_b}"
    )]
    OverlappingSlides {
        voice_a: u8,
        voice_b: u8,
        end_a: u32,
        start_b: u32,
    },
    #[error("bytecode payload {bytes} exceeds max_sequence_bytes capability limit {limit}")]
    BytecodeTooLarge { bytes: u32, limit: u32 },
    #[error(
        "step {step_index} duration_ticks={duration} too short for transition consuming {consumed} ticks"
    )]
    StepTooShortForTransition {
        step_index: u32,
        duration: u32,
        consumed: u32,
    },
    #[error("total source count {actual} exceeds max_sources capability limit {limit}")]
    TooManySources { actual: u32, limit: u32 },
    #[error("src_index {src_index} > 255; cannot fit in SET_SRC byte operand")]
    SrcIndexTooLarge { src_index: u32 },
}

/// Compile an atom sequence to SEQ2 bytecode.
pub fn compile_sequence(
    input: SequenceCompileInput<'_>,
) -> Result<SequenceCompileOutput, SequenceCompileError> {
    let SequenceCompileInput {
        project,
        manifest,
        source_directory,
        sequence,
    } = input;

    // -- Capability gating -----------------------------------------------
    // Transitive deps first (consultant blocker M2.5 Phase 1A): a
    // manifest with `volume_slide=true` and `core_tick_loop=false`
    // is structurally impossible at runtime. Surface as
    // CapabilityMissing so direct + transitive failures share one
    // error type.
    if let Err(crate::capability_manifest::CapabilityDepError::MissingDep { feature, missing }) =
        manifest.validate_dependencies()
    {
        return Err(SequenceCompileError::CapabilityMissing {
            feature: format!("{missing} (transitive dep of {feature})"),
        });
    }

    let has_multi_step = sequence.steps.len() > 1;
    let needs_source_step = sequence
        .steps
        .iter()
        .skip(1)
        .any(|s| matches!(s.transition, AtomTransition::FadeToZeroRetrigger { .. }));
    require_feature(manifest, "core_sequence_wait")?;
    require_feature(manifest, "synth_atom_sequence")?;
    if needs_source_step {
        require_feature(manifest, "synth_source_step")?;
        require_feature(manifest, "volume_slide")?;
    }
    if has_multi_step {
        require_feature(manifest, "sample_runtime_src_change")?;
    }

    // Source-count budget (consultant blocker M2.5 Phase 1B):
    // total_sources must fit max_sources, and each SRCN must fit a
    // single SET_SRC operand byte (0..=255). Validation rule 9 caps
    // sample_pool at 128 but doesn't check the cross-pool sum.
    let total_sources =
        source_directory.sample_count + source_directory.atom_index_by_id.len() as u32;
    if total_sources > manifest.limits.max_sources {
        return Err(SequenceCompileError::TooManySources {
            actual: total_sources,
            limit: manifest.limits.max_sources,
        });
    }

    let voice = sequence.voice;
    let voice_mask: u8 = 1 << voice;

    let mut payload: Vec<u8> = Vec::new();
    let mut per_step: Vec<StepLowering> = Vec::with_capacity(sequence.steps.len());
    let mut active_slides: Vec<ActiveSlideInterval> = Vec::new();
    let mut tick_cursor: u32 = 0;

    for (step_index, step) in sequence.steps.iter().enumerate() {
        let bytecode_offset_start = payload.len() as u32;
        let tick_offset_start = tick_cursor;
        let mut writes_in_step: u32 = 0;

        let atom = project
            .atom_pool
            .iter()
            .find(|a| a.id == step.atom_id)
            .ok_or_else(|| SequenceCompileError::AtomIdNotInPool {
                atom_id: step.atom_id.clone(),
            })?;
        let src_index = source_directory
            .src_index_for_atom(&step.atom_id)
            .ok_or_else(|| SequenceCompileError::AtomIdNotInPool {
                atom_id: step.atom_id.clone(),
            })?;
        let (target_l, target_r) = step_target_volumes(step, atom);

        match (step_index, &step.transition) {
            (0, AtomTransition::InitialKon) => {
                // SET_SRC, SET_VOL, KON, WAIT duration.
                emit_set_src(&mut payload, voice, src_index)?;
                writes_in_step += 1;
                emit_set_vol(&mut payload, voice, target_l, target_r);
                writes_in_step += 2;
                emit_kon(&mut payload, voice_mask);
                writes_in_step += 1;
                emit_wait(&mut payload, step.duration_ticks);
                tick_cursor += step.duration_ticks as u32;
            }
            (0, AtomTransition::FadeToZeroRetrigger { .. }) => {
                return Err(SequenceCompileError::FirstStepNotInitialKon {
                    actual: "fade_to_zero_retrigger".to_string(),
                });
            }
            (
                _,
                AtomTransition::FadeToZeroRetrigger {
                    fade_out_ticks,
                    fade_in_ticks,
                },
            ) => {
                let consumed = (*fade_out_ticks as u32) + 1 + (*fade_in_ticks as u32);
                if (step.duration_ticks as u32).saturating_add(consumed) > 255 + 255 + 1 + 255
                    && step.duration_ticks == 0
                {
                    // Defensive: schema validation rule 43 already
                    // disallows duration_ticks=0; this just guards
                    // against out-of-band callers.
                    return Err(SequenceCompileError::StepTooShortForTransition {
                        step_index: step_index as u32,
                        duration: step.duration_ticks as u32,
                        consumed,
                    });
                }

                // Fade-out.
                emit_vol_slide(&mut payload, voice, 0, 0, *fade_out_ticks);
                // 0 writes when issuing VOL_SLIDE on this tick; the
                // first slide write fires on the NEXT tick (per
                // SPEC §14.3: slide-advance runs before opcode-read,
                // so the just-issued slide isn't yet "active" on the
                // bytecode-read tick). Window: [T+1, T+1+ticks).
                active_slides.push(ActiveSlideInterval {
                    voice,
                    tick_start: tick_cursor + 1,
                    tick_end: tick_cursor + 1 + (*fade_out_ticks as u32),
                });
                emit_wait(&mut payload, *fade_out_ticks);
                tick_cursor += *fade_out_ticks as u32;

                // KOFF + 1-tick gap.
                emit_koff(&mut payload, voice_mask);
                writes_in_step += 1;
                emit_wait(&mut payload, 1);
                tick_cursor += 1;

                // Re-trigger.
                emit_set_src(&mut payload, voice, src_index)?;
                writes_in_step += 1;
                emit_set_vol(&mut payload, voice, 0, 0);
                writes_in_step += 2;
                emit_kon(&mut payload, voice_mask);
                writes_in_step += 1;

                // Fade-in.
                emit_vol_slide(&mut payload, voice, target_l, target_r, *fade_in_ticks);
                active_slides.push(ActiveSlideInterval {
                    voice,
                    tick_start: tick_cursor + 1,
                    tick_end: tick_cursor + 1 + (*fade_in_ticks as u32),
                });
                emit_wait(&mut payload, *fade_in_ticks);
                tick_cursor += *fade_in_ticks as u32;

                // Sustain.
                emit_wait(&mut payload, step.duration_ticks);
                tick_cursor += step.duration_ticks as u32;
            }
            (_, AtomTransition::InitialKon) => {
                return Err(SequenceCompileError::NonFirstStepWrongTransition {
                    step_index: step_index as u32,
                    actual: "initial_kon".to_string(),
                });
            }
        }

        per_step.push(StepLowering {
            step_index: step_index as u32,
            atom_id: step.atom_id.clone(),
            voice,
            bytecode_offset_start,
            bytecode_offset_end: payload.len() as u32,
            max_writes_in_step: writes_in_step,
            tick_offset_start,
            tick_offset_end: tick_cursor,
        });
    }

    // -- Sequence terminator ---------------------------------------------
    payload.push(BytecodeOpcode::End as u8);

    // -- Capability limits -----------------------------------------------
    let limit = manifest.limits.max_sequence_bytes;
    if (payload.len() as u32) > limit {
        return Err(SequenceCompileError::BytecodeTooLarge {
            bytes: payload.len() as u32,
            limit,
        });
    }

    // -- One-active-slide enforcement ------------------------------------
    check_slide_overlap(&active_slides)?;

    // -- Per-tick write budget -------------------------------------------
    let max_writes_per_tick_estimate = walk_writes_per_tick(&payload, manifest, &active_slides)?;

    // -- Wrap with SEQ2 header -------------------------------------------
    let header = SequenceHeader {
        bytecode_version: BYTECODE_VERSION_M2,
        bytecode_len: payload.len() as u16,
    };
    let mut bytecode = Vec::with_capacity(SEQUENCE_HEADER_LEN + payload.len());
    bytecode.extend_from_slice(&header.to_bytes());
    bytecode.extend_from_slice(&payload);
    let bytecode_sha256 = crate::asm::sha256_hex(&bytecode);

    Ok(SequenceCompileOutput {
        bytecode,
        bytecode_payload_len: payload.len() as u16,
        bytecode_sha256,
        max_writes_per_tick_estimate,
        per_step,
        total_ticks: tick_cursor,
        active_slides,
    })
}

fn require_feature(
    manifest: &CapabilityManifest,
    feature: &str,
) -> Result<(), SequenceCompileError> {
    if manifest.features.get(feature).copied().unwrap_or(false) {
        Ok(())
    } else {
        Err(SequenceCompileError::CapabilityMissing {
            feature: feature.to_string(),
        })
    }
}

fn step_target_volumes(
    step: &crate::project_v2::AtomSequenceStep,
    atom: &crate::atom::AtomSlot,
) -> (u8, u8) {
    // Target volume = step.target_volume scaled by the atom's
    // playback pan (constant-power). The atom's playback.volume is
    // the maximum the atom plays at; the step's target_volume scales
    // that further within the sequence. Together they pick the
    // VOLL/VOLR pair the slide ramps to.
    let pan = atom.playback.pan;
    let combined_volume = (step.target_volume * atom.playback.volume).clamp(0.0, 1.0);
    playback_to_voll_volr(combined_volume, pan)
}

fn check_slide_overlap(active: &[ActiveSlideInterval]) -> Result<(), SequenceCompileError> {
    let mut sorted: Vec<&ActiveSlideInterval> = active.iter().collect();
    sorted.sort_by_key(|s| s.tick_start);
    for w in sorted.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b.tick_start < a.tick_end {
            return Err(SequenceCompileError::OverlappingSlides {
                voice_a: a.voice,
                voice_b: b.voice,
                end_a: a.tick_end,
                start_b: b.tick_start,
            });
        }
    }
    Ok(())
}

/// Walk the bytecode tick-by-tick and count DSP writes per tick.
/// Returns the observed maximum or `WriteBudgetExceeded` on first
/// overflow.
fn walk_writes_per_tick(
    payload: &[u8],
    manifest: &CapabilityManifest,
    active_slides: &[ActiveSlideInterval],
) -> Result<u32, SequenceCompileError> {
    let limit = manifest.limits.max_dsp_writes_per_tick;
    let mut max_observed: u32 = 0;
    let mut tick_index: u32 = 0;
    let mut pc: usize = 0;

    while pc < payload.len() {
        let mut writes_this_tick: u32 = active_writes_at_tick(active_slides, tick_index);

        // Read opcodes until WAIT/END/EOF — those don't add to "this
        // tick"'s write count for the WAIT-stop case (the WAIT
        // operand itself doesn't write); they do for the immediate
        // emit cases (KON/KOFF/SET_SRC/SET_VOL).
        loop {
            if pc >= payload.len() {
                break;
            }
            let op_byte = payload[pc];
            let op = BytecodeOpcode::from_byte(op_byte).expect("valid opcode (compiler-emitted)");
            match op {
                BytecodeOpcode::End => {
                    return ok_with_max(writes_this_tick, &mut max_observed, limit, tick_index);
                }
                BytecodeOpcode::Wait => {
                    let ticks = payload[pc + 1] as u32;
                    pc += 1 + op.operand_len();
                    // Validate this tick's writes vs budget BEFORE
                    // advancing past the WAIT.
                    if writes_this_tick > limit {
                        return Err(SequenceCompileError::WriteBudgetExceeded {
                            tick_index,
                            writes: writes_this_tick,
                            limit,
                        });
                    }
                    if writes_this_tick > max_observed {
                        max_observed = writes_this_tick;
                    }
                    // Walk the WAIT-driven ticks: each one only
                    // accrues active-slide writes (no opcodes are
                    // read mid-WAIT).
                    for _ in 0..ticks {
                        tick_index += 1;
                        let w = active_writes_at_tick(active_slides, tick_index);
                        if w > limit {
                            return Err(SequenceCompileError::WriteBudgetExceeded {
                                tick_index,
                                writes: w,
                                limit,
                            });
                        }
                        if w > max_observed {
                            max_observed = w;
                        }
                    }
                    writes_this_tick = active_writes_at_tick(active_slides, tick_index);
                }
                BytecodeOpcode::SetSrc => {
                    writes_this_tick += 1;
                    pc += 1 + op.operand_len();
                }
                BytecodeOpcode::SetVol => {
                    writes_this_tick += 2;
                    pc += 1 + op.operand_len();
                }
                BytecodeOpcode::Kon | BytecodeOpcode::Koff => {
                    writes_this_tick += 1;
                    pc += 1 + op.operand_len();
                }
                BytecodeOpcode::VolSlide => {
                    // Issuing the slide doesn't write; subsequent
                    // ticks accrue writes via active_slides.
                    pc += 1 + op.operand_len();
                }
                BytecodeOpcode::SetPitch => {
                    // u8 voice + u16 pitch => 2 writes (PITCHL + PITCHH).
                    writes_this_tick += 2;
                    pc += 1 + op.operand_len();
                }
            }
        }
    }
    ok_with_max(0, &mut max_observed, limit, tick_index)
}

fn active_writes_at_tick(active: &[ActiveSlideInterval], tick: u32) -> u32 {
    // Each slide active on this tick contributes 2 writes (VOLL + VOLR).
    let mut writes = 0;
    for s in active {
        if tick >= s.tick_start && tick < s.tick_end {
            writes += 2;
        }
    }
    writes
}

fn ok_with_max(
    final_writes: u32,
    max: &mut u32,
    limit: u32,
    tick_index: u32,
) -> Result<u32, SequenceCompileError> {
    if final_writes > limit {
        return Err(SequenceCompileError::WriteBudgetExceeded {
            tick_index,
            writes: final_writes,
            limit,
        });
    }
    if final_writes > *max {
        *max = final_writes;
    }
    Ok(*max)
}

fn emit_set_src(out: &mut Vec<u8>, voice: u8, src_index: u32) -> Result<(), SequenceCompileError> {
    if src_index > 0xFF {
        return Err(SequenceCompileError::SrcIndexTooLarge { src_index });
    }
    out.push(BytecodeOpcode::SetSrc as u8);
    out.push(voice);
    out.push(src_index as u8);
    Ok(())
}

fn emit_set_vol(out: &mut Vec<u8>, voice: u8, vol_l: u8, vol_r: u8) {
    out.push(BytecodeOpcode::SetVol as u8);
    out.push(voice);
    out.push(vol_l);
    out.push(vol_r);
}

fn emit_kon(out: &mut Vec<u8>, mask: u8) {
    out.push(BytecodeOpcode::Kon as u8);
    out.push(mask);
}

fn emit_koff(out: &mut Vec<u8>, mask: u8) {
    out.push(BytecodeOpcode::Koff as u8);
    out.push(mask);
}

fn emit_wait(out: &mut Vec<u8>, ticks: u8) {
    debug_assert!(ticks > 0, "WAIT 0 is invalid per SPEC §14.3");
    out.push(BytecodeOpcode::Wait as u8);
    out.push(ticks);
}

fn emit_vol_slide(out: &mut Vec<u8>, voice: u8, target_l: u8, target_r: u8, ticks: u8) {
    out.push(BytecodeOpcode::VolSlide as u8);
    out.push(voice);
    out.push(target_l);
    out.push(target_r);
    out.push(ticks);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atom::{AtomKind, AtomPartial, AtomRenderOptions, AtomSlot};
    use crate::project::{
        Driver, Envelope, MasterEcho, Project, SampleFormat, SampleLoop, SamplePlayback,
        SampleSlot, SampleSource,
    };
    use crate::project_v2::{
        AtomSequenceStep, AtomTransition, M2Block, ProjectV2, Track, TrackKind,
    };

    fn sample(id: &str) -> SampleSlot {
        SampleSlot {
            id: id.to_string(),
            name: id.to_string(),
            source: SampleSource {
                path: format!("audio/{id}.wav"),
                sha256: "0".repeat(64),
                format: SampleFormat::Wav,
                sample_rate_hz: 32000,
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
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        }
    }

    fn atom_with_pan(id: &str, cycle: u16, pan: f64) -> AtomSlot {
        AtomSlot {
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
                volume: 1.0,
                pan,
                echo: false,
                envelope: Envelope::GainRaw { gain_byte: 127 },
            },
        }
    }

    /// SPEC §16.9 canonical M2 fixture: 1 sample on voice 0, 2 sine
    /// atoms on voice 1, 2-step sequence: initial_kon atom_a then
    /// fade_to_zero_retrigger to atom_b.
    fn canonical_project() -> ProjectV2 {
        ProjectV2 {
            schema_version: 2,
            project: Project {
                name: "atomic_seq".to_string(),
                tick_rate_hz: 60,
            },
            driver: Driver {
                profile: "multi_voice_atom".to_string(),
                bytecode_version: 2,
            },
            master_echo: MasterEcho {
                enabled: false,
                edl: 0,
                efb: 0,
                evol_l: 0,
                evol_r: 0,
                fir: [0; 8],
            },
            sample_pool: vec![sample("lead")],
            atom_pool: vec![
                atom_with_pan("atom_a", 128, 1.0),
                atom_with_pan("atom_b", 64, 1.0),
            ],
            atom_sequences: vec![AtomSequence {
                id: "atomseq_0001".to_string(),
                name: "two_step_demo".to_string(),
                voice: 1,
                steps: vec![
                    AtomSequenceStep {
                        atom_id: "atom_a".to_string(),
                        duration_ticks: 120,
                        target_volume: 0.8,
                        transition: AtomTransition::InitialKon,
                    },
                    AtomSequenceStep {
                        atom_id: "atom_b".to_string(),
                        duration_ticks: 120,
                        target_volume: 0.8,
                        transition: AtomTransition::FadeToZeroRetrigger {
                            fade_out_ticks: 4,
                            fade_in_ticks: 4,
                        },
                    },
                ],
                looped: false,
            }],
            tracks: vec![
                Track {
                    id: "track_sample_0".to_string(),
                    name: String::new(),
                    voice: 0,
                    kind: TrackKind::SampleSustain {
                        sample_id: "lead".to_string(),
                    },
                },
                Track {
                    id: "track_atom_1".to_string(),
                    name: String::new(),
                    voice: 1,
                    kind: TrackKind::AtomSequence {
                        atom_sequence_id: "atomseq_0001".to_string(),
                    },
                },
            ],
            m2: M2Block {
                active_sequence_id: Some("atomseq_0001".to_string()),
            },
        }
    }

    fn compile_canonical() -> SequenceCompileOutput {
        let project = canonical_project();
        let manifest = CapabilityManifest::multi_voice_atom();
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .expect("compile must succeed")
    }

    /// Test A — byte-pinned canonical fixture.
    #[test]
    fn canonical_bytecode_byte_pinned() {
        let out = compile_canonical();

        // Construct expected: 8-byte header + payload + END.
        // pan = 1.0, vol 0.8 → vol_r per playback_to_voll_volr.
        let (vol_l, vol_r) = playback_to_voll_volr(0.8, 1.0);
        eprintln!("computed vol_l={vol_l}, vol_r={vol_r}");

        let mut expected_payload: Vec<u8> = Vec::new();
        // Step 0: SET_SRC v=1, src=1; SET_VOL v=1, l, r; KON 0b10; WAIT 120.
        expected_payload.extend_from_slice(&[0x10, 0x01, 0x01]);
        expected_payload.extend_from_slice(&[0x11, 0x01, vol_l, vol_r]);
        expected_payload.extend_from_slice(&[0x12, 0x02]);
        expected_payload.extend_from_slice(&[0x01, 0x78]);
        // Step 1: VOL_SLIDE v=1, 0,0,4; WAIT 4; KOFF 0b10; WAIT 1;
        // SET_SRC v=1, src=2; SET_VOL v=1, 0,0; KON 0b10;
        // VOL_SLIDE v=1, l, r, 4; WAIT 4; WAIT 120.
        expected_payload.extend_from_slice(&[0x20, 0x01, 0x00, 0x00, 0x04]);
        expected_payload.extend_from_slice(&[0x01, 0x04]);
        expected_payload.extend_from_slice(&[0x13, 0x02]);
        expected_payload.extend_from_slice(&[0x01, 0x01]);
        expected_payload.extend_from_slice(&[0x10, 0x01, 0x02]);
        expected_payload.extend_from_slice(&[0x11, 0x01, 0x00, 0x00]);
        expected_payload.extend_from_slice(&[0x12, 0x02]);
        expected_payload.extend_from_slice(&[0x20, 0x01, vol_l, vol_r, 0x04]);
        expected_payload.extend_from_slice(&[0x01, 0x04]);
        expected_payload.extend_from_slice(&[0x01, 0x78]);
        // END.
        expected_payload.push(0x00);

        let mut expected = Vec::new();
        expected.extend_from_slice(b"SEQ2");
        expected.push(0x02);
        expected.push(0x00);
        expected.extend_from_slice(&(expected_payload.len() as u16).to_le_bytes());
        expected.extend_from_slice(&expected_payload);

        assert_eq!(
            out.bytecode,
            expected,
            "canonical bytecode drift; observed (hex): {}",
            out.bytecode
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        assert_eq!(out.bytecode.len(), 49);
        assert_eq!(out.bytecode_payload_len, 41);
    }

    /// Test B — KOFF + WAIT 1 + SET_SRC pattern preserved on every
    /// non-first step.
    #[test]
    fn koff_wait1_set_src_pattern_preserved() {
        let out = compile_canonical();
        // The byte sequence 13 02 01 01 10 01 02 must appear once.
        let pattern = [0x13, 0x02, 0x01, 0x01, 0x10, 0x01, 0x02];
        let count = out
            .bytecode
            .windows(pattern.len())
            .filter(|w| *w == pattern)
            .count();
        assert_eq!(count, 1, "KOFF+WAIT1+SET_SRC pattern must appear once");
    }

    /// Test C — no SET_PITCH ($30) opcodes in canonical lowering.
    #[test]
    fn canonical_lowering_emits_no_set_pitch() {
        let out = compile_canonical();
        // Skip the 8-byte header; SET_PITCH is $30.
        for &b in &out.bytecode[SEQUENCE_HEADER_LEN..] {
            assert_ne!(
                b, 0x30,
                "SET_PITCH ($30) found in payload — M2.4 default lowering must not emit it"
            );
        }
    }

    /// Test F — capability missing.
    #[test]
    fn rejects_when_synth_source_step_missing() {
        let project = canonical_project();
        let mut manifest = CapabilityManifest::multi_voice_atom();
        manifest
            .features
            .insert("synth_source_step".to_string(), false);
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(
            matches!(err, SequenceCompileError::CapabilityMissing { ref feature } if feature == "synth_source_step"),
            "got: {err:?}"
        );
    }

    /// Test H — determinism.
    #[test]
    fn compile_is_deterministic_across_calls() {
        let a = compile_canonical();
        let b = compile_canonical();
        assert_eq!(a.bytecode, b.bytecode);
        assert_eq!(a.bytecode_sha256, b.bytecode_sha256);
        assert_eq!(a.total_ticks, b.total_ticks);
    }

    #[test]
    fn total_ticks_matches_lowering() {
        let out = compile_canonical();
        // Step 0 sustain 120; step 1 transition (4 + 1 + 4) + sustain 120 = 129.
        assert_eq!(out.total_ticks, 120 + 4 + 1 + 4 + 120);
    }

    #[test]
    fn max_writes_per_tick_estimate_is_at_most_4() {
        let out = compile_canonical();
        assert!(
            out.max_writes_per_tick_estimate <= 4,
            "expected <=4 writes/tick, got {}",
            out.max_writes_per_tick_estimate
        );
    }

    #[test]
    fn rejects_invalid_first_step_transition() {
        let mut project = canonical_project();
        project.atom_sequences[0].steps[0].transition = AtomTransition::FadeToZeroRetrigger {
            fade_out_ticks: 4,
            fade_in_ticks: 4,
        };
        let manifest = CapabilityManifest::multi_voice_atom();
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(matches!(
            err,
            SequenceCompileError::FirstStepNotInitialKon { .. }
        ));
    }

    #[test]
    fn rejects_non_first_initial_kon() {
        let mut project = canonical_project();
        project.atom_sequences[0].steps[1].transition = AtomTransition::InitialKon;
        let manifest = CapabilityManifest::multi_voice_atom();
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(matches!(
            err,
            SequenceCompileError::NonFirstStepWrongTransition { step_index: 1, .. }
        ));
    }

    #[test]
    fn rejects_atom_id_not_in_pool() {
        let mut project = canonical_project();
        project.atom_sequences[0].steps[0].atom_id = "ghost_atom".to_string();
        let manifest = CapabilityManifest::multi_voice_atom();
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(matches!(err, SequenceCompileError::AtomIdNotInPool { .. }));
    }

    /// Test I — full dependency graph table-driven test (consultant #5).
    #[test]
    fn dep_graph_each_edge_produces_missing_dep_when_dropped() {
        // Iterate every edge of the SPEC §5.4 dependency graph for
        // multi_voice_atom. Drop each dep in turn; expect
        // validate_dependencies to fire.
        let edges: &[(&str, &[&str])] = &[
            (
                "multi_voice_playback",
                &["core_note_on_off", "core_dsp_write"],
            ),
            (
                "synth_static_atom",
                &["sample_playback", "core_source_directory"],
            ),
            (
                "synth_atom_sequence",
                &["synth_static_atom", "core_sequence_wait"],
            ),
            (
                "synth_source_step",
                &[
                    "synth_atom_sequence",
                    "sample_runtime_src_change",
                    "core_key_on_delay_safety",
                ],
            ),
            ("volume_slide", &["volume_set", "core_tick_loop"]),
            (
                "sample_runtime_src_change",
                &["core_source_directory", "core_note_on_off"],
            ),
        ];
        for (feat, deps) in edges {
            for dep in *deps {
                let mut m = CapabilityManifest::multi_voice_atom();
                m.features.insert(dep.to_string(), false);
                let err = m
                    .validate_dependencies()
                    .expect_err("dropping a dep must surface as MissingDep");
                let crate::capability_manifest::CapabilityDepError::MissingDep { feature, missing } =
                    err;
                // The error fires on the first feature whose deps
                // aren't satisfied — could be `feat` itself or any
                // earlier feature also depending on `dep`.
                assert!(
                    missing == *dep,
                    "expected missing={dep} for dropping ({feat} -> {dep}), got feature={feature} missing={missing}"
                );
            }
        }
    }

    // ============================================================
    // Layer 1 — transitive dep + SRCN bounds blockers (M2.5).
    // ============================================================

    /// Compiler now refuses a manifest with `volume_slide=true` and
    /// `core_tick_loop=false` even though direct gating only checks
    /// `synth_source_step` / `volume_slide`. validate_dependencies()
    /// catches the structural impossibility.
    #[test]
    fn compile_sequence_rejects_volume_slide_without_core_tick_loop() {
        let project = canonical_project();
        let mut manifest = CapabilityManifest::multi_voice_atom();
        manifest
            .features
            .insert("core_tick_loop".to_string(), false);
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                SequenceCompileError::CapabilityMissing { ref feature }
                    if feature.contains("core_tick_loop")
            ),
            "got: {err:?}"
        );
    }

    #[test]
    fn compile_sequence_rejects_synth_atom_sequence_without_synth_static_atom() {
        let project = canonical_project();
        let mut manifest = CapabilityManifest::multi_voice_atom();
        manifest
            .features
            .insert("synth_static_atom".to_string(), false);
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                SequenceCompileError::CapabilityMissing { ref feature }
                    if feature.contains("synth_static_atom")
            ),
            "got: {err:?}"
        );
    }

    #[test]
    fn compile_sequence_rejects_synth_source_step_without_sample_runtime_src_change() {
        let project = canonical_project();
        let mut manifest = CapabilityManifest::multi_voice_atom();
        manifest
            .features
            .insert("sample_runtime_src_change".to_string(), false);
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                SequenceCompileError::CapabilityMissing { ref feature }
                    if feature.contains("sample_runtime_src_change")
            ),
            "got: {err:?}"
        );
    }

    /// Sanity: the canonical M2 manifest is well-formed; transitive
    /// deps don't introduce false-positives on legitimate compiles.
    #[test]
    fn canonical_compiles_after_transitive_dep_check() {
        let _ = compile_canonical(); // panics on any error path
    }

    /// SRCN bounds — synthesise a project whose source-pool sum
    /// exceeds the manifest's max_sources limit. Use a smaller
    /// manifest (limits.max_sources=2) so the canonical 3-source
    /// fixture overflows on demand.
    #[test]
    fn compile_sequence_rejects_overcapacity_source_pool() {
        let project = canonical_project();
        let mut manifest = CapabilityManifest::multi_voice_atom();
        manifest.limits.max_sources = 2; // canonical has 1 sample + 2 atoms = 3
        let source_directory = SourceDirectory::from_project(&project);
        let sequence = project.atom_sequences[0].clone();
        let err = compile_sequence(SequenceCompileInput {
            project: &project,
            manifest: &manifest,
            source_directory: &source_directory,
            sequence: &sequence,
        })
        .unwrap_err();
        assert!(
            matches!(
                err,
                SequenceCompileError::TooManySources {
                    actual: 3,
                    limit: 2
                }
            ),
            "got: {err:?}"
        );
    }

    /// Internal-call test for SrcIndexTooLarge. Inject an
    /// out-of-range atom_index so the SET_SRC emit detects it.
    /// This path is unreachable through the public API for valid
    /// projects (atom_pool[].id maps to indices < pool length, and
    /// max_sources is bounded by 128), but the bounds check is
    /// defence-in-depth before the byte-level cast.
    #[test]
    fn emit_set_src_rejects_src_index_above_255() {
        let mut payload = Vec::new();
        let err = super::emit_set_src(&mut payload, 1, 256).unwrap_err();
        assert!(matches!(
            err,
            SequenceCompileError::SrcIndexTooLarge { src_index: 256 }
        ));
        // emit_set_src should not have written any bytes when it errors.
        assert!(payload.is_empty());
    }
}
