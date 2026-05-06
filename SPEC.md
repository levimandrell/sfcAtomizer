# SFC Wave Compiler — Specification

## 0. Premise

SFC Wave Compiler is a standalone Rust application for SNES/Super Famicom audio authoring. It combines a C700-style BRR sample workflow with a compile-time synthesizer that emits SNES-valid BRR atoms, atom sequences, wavetable frames, and SPC700 playback bytecode.

It is not a soft synth, a DAW replacement, or an audio-to-SNES resynthesizer. It is a hardware-constrained authoring system targeting a custom SPC700 driver assembled from granular feature modules.

**Core idea:** author instruments at a higher level, compile them into an exact 64 KB ARAM layout — driver code, source directory, sequence bytecode, BRR samples, generated atoms, tables, echo buffer.

### Guiding principle: compiler-side over runtime-side

Any feature that can be rendered into BRR atoms, tables, or bytecode offline does not become real-time SPC700 logic. Oscillator shapes, additive timbres, and wavetable frames render offline to BRR atoms. Pitch LFOs, tremolo, slides, and morph trajectories are bytecode events. Filtering and pre-emphasis are applied at compile time. The driver is a small, deterministic dispatcher: tick → bytecode → S-DSP register writes.

---

## 1. Goals

1. Standalone Rust application for SNES audio composition and instrument design.
2. Traditional BRR sample instruments.
3. Compile-time synth instruments that generate BRR atoms.
4. Constrained wavetable synthesis as a first-class authoring path: authored frames compiled to BRR atoms plus playback events.
5. `.spc` preview file per compiled module.
6. Minimal `.sfc` test ROM per compiled module.
7. Game-ready module blobs for 65816 SNES projects.
8. Small custom SPC700 driver assembled from granular compile-time modules.
9. UI exposes only features supported by the active driver.

Secondary: Reaper-like sequencing view (not a DAW); C700-like sample tooling and accurate preview; clear compile reports for ARAM, voice, S-DSP write rate, source count, echo, driver size; future Reaper export/import.

---

## 2. Non-goals

- Audio-to-wavetable decomposition from complex inputs.
- VST/AU plugin.
- SNES audio middleware stack.
- Runtime sample streaming.
- Driver hot-swap during playback.
- Tracker clone.
- Serum clone. (A *constrained* authored wavetable compiler is in scope.)
- Real-time subtractive/wavetable synthesis on SPC700.
- MOD/XM/IT/S3M effect import.
- Dynamic linker for SPC700 code sections.
- Free-form EQ pre-emphasis editor (presets only; see §10.5).

---

## 3. Hardware constraints

| Resource | Value |
|---|---|
| ARAM | 65,536 bytes |
| S-DSP voices | 8 |
| Sample format | BRR, 16 decoded samples per 9-byte block |
| Loop alignment | BRR block boundaries (16-sample units) |
| Echo buffer | 2 KB × EDL (EDL=15 ≈ 30 KB) |
| Runtime CPU | SPC700 |
| Tick rate | 60 Hz |

Tick rate aligns with NTSC frame timing, gives deterministic slide and event resolution, and keeps timing budgets simple. All `WAIT` durations, slides, and S-DSP-write-per-tick limits are calibrated against 60 Hz.

ROM is not a substitute for ARAM. A larger ROM can store many modules and driver variants, but only one active 64 KB ARAM image matters at a time.

---

## 4. Architecture

```
Rust GUI / CLI
  ├─ Project model + file I/O + schema migration
  ├─ Sequencer (bars/beats/ticks)
  ├─ Sample editor
  ├─ Synth atom editor
  ├─ Wavetable/morph editor
  ├─ Driver feature resolver
  ├─ BRR encoder/decoder (core crate)
  ├─ Atom compiler
  ├─ Sequence compiler
  ├─ ARAM packer
  ├─ Voice-pair allocator
  ├─ SPC700 assembler invoker
  ├─ SPC / SFC / module exporters
  ├─ Internal preview renderer
  └─ Oracle bridge (snes_spc)
SPC700 assembly driver
  ├─ boot/init
  ├─ S-DSP register helpers
  ├─ 60 Hz tick loop
  ├─ bytecode interpreter
  ├─ sample playback core
  ├─ optional feature handlers (synth atom / SFX / game)
```

Host: Rust. Driver: SPC700 assembly via asar for M0–M2. The assembler is invoked through an AssemblerBackend interface so WLA-DX can be added later if the project needs object-file linking, richer map files, or section-placement behavior that asar cannot provide cleanly. The compiler, not the assembler/linker, owns ARAM layout. The assembler emits code/data for addresses assigned by the ARAM packer. Build cache keys include assembler_backend, assembler version, SPC700 source hash, compiler version, and BRR encoder version.

---

## 5. Driver capabilities

### 5.1 Profiles

Profiles are presets over granular flags: `sample_basic`, `sample_fx`, `synth_static`, `synth_events`, `synth_xfade`, `synth_wavetable`, `game_runtime`, `spc_compo`.

### 5.2 Feature flags

**Mandatory core:** `core_tick_loop`, `core_dsp_write`, `core_sequence_wait`, `core_note_on_off`, `core_pitch_table`, `core_source_directory`, `core_key_on_delay_safety`.

**Sample:** `sample_playback`, `sample_multisample`, `sample_keysplit`, `sample_velocity_layers`, `sample_runtime_src_change`.

> `sample_runtime_src_change` — selects a new source-directory entry for a voice before key-on; does not perform live SRCN swap on a sustained voice (which is unsafe at unaligned BRR block boundaries).

**Envelope/expression:** `adsr`, `gain`, `volume_set`, `volume_slide`, `pan_set`, `pan_slide`, `pitch_set`, `pitch_slide`, `portamento`, `vibrato`, `tremolo`, `detune`, `noise_mode`, `pitch_modulation`, `surround_invert`.

**Echo:** `echo_enable`, `echo_per_voice_mask`, `echo_static_params`, `echo_mid_song_param_changes`, `fir_filter_editing`.

**Synth (compile-time):** `synth_static_atom`, `synth_two_osc_collapsed_atom`, `synth_dual_voice_atom_pair`, `synth_atom_sequence`, `synth_source_step`, `synth_volume_ramp`, `synth_pitch_event_lfo`, `synth_tremolo_event_lfo`, `synth_paired_voice_crossfade`, `synth_wavetable_morph`.

`synth_dual_voice_atom_pair` schedules two generated BRR atoms as two physical S-DSP voices. It is not a runtime oscillator engine — the SPC700 plays BRR sources and writes S-DSP registers; the "dual voice" lives in compiled atoms, not driver code.

**Voice allocation:** `voice_pair_allocator`, `voice_reservation`, `protected_music_pair`.

**Game/runtime:** `sfx_queue`, `sfx_priority`, `sfx_one_channel_limit`, `sfx_uninterruptible_flag`, `music_ducking`, `async_loader_api`, `module_reload_api`.

`async_loader_api` and `module_reload_api` are limited to whole-module replacement between songs or explicit game-state transitions. They do not support mid-song sample streaming, mid-song driver replacement, or runtime code overlay loading. Those remain on the Forbidden list.

**Forbidden:** `runtime_sample_streaming`, `runtime_code_overlay_loader`, `runtime_dynamic_linker`, `arbitrary_wav_resynthesis`, `runtime_oscillator_engine`.

### 5.3 Dependencies

```
synth_static_atom               → sample_playback
synth_two_osc_collapsed_atom    → synth_static_atom
synth_atom_sequence             → sample_runtime_src_change, core_sequence_wait
synth_paired_voice_crossfade    → synth_atom_sequence, volume_slide, voice_pair_allocator
synth_wavetable_morph           → synth_paired_voice_crossfade, voice_pair_allocator
vibrato                         → pitch_slide or per_tick_pitch_delta
portamento                      → pitch_slide
tremolo                         → volume_slide
panbrello                       → pan_slide
echo_mid_song_param_changes     → echo_static_params
protected_music_pair            → voice_pair_allocator, voice_reservation
```

The compiler resolves dependencies automatically and shows the explanation when enabling a feature pulls in others.

### 5.4 Capability manifest

The driver build emits a manifest read by every other component (instrument editor, sequencer, compiler, preview):

```json
{
  "driver_profile": "synth_static",
  "driver_hash": "...",
  "bytecode_version": 1,
  "tick_rate_hz": 60,
  "features": { "sample_playback": true, "adsr": true, "echo_enable": true, "...": "..." },
  "limits": {
    "max_music_voices": 8,
    "reserved_sfx_voices": 0,
    "max_sources": 128,
    "max_sources_note": "profile/tool policy; the ARAM packer enforces actual source-directory footprint",
    "max_dsp_writes_per_tick": 24,
    "min_keyoff_to_keyon_ticks": 1
  },
  "measured": {
    "driver_code_bytes": 0,
    "runtime_state_bytes": 0,
    "source_directory_bytes": 0,
    "assembler_backend": "asar"
  }
}
```

The `measured` block is populated by the build process and consumed by UI displays and the build cache.

`max_dsp_writes_per_tick` is a compiler scheduling policy, not a hardware maximum. One logical S-DSP write means one target register update via $F2/$F3. The default soft budget is 24 logical writes/tick for M0–M2 to preserve interpreter headroom and avoid bursty event schedules. Later calibration may raise this per profile after cycle measurement. Compile reports show both the soft budget and measured peak writes/tick.

---

## 6. Live-compile model

Most edits change data, not driver code. The compiler reassembles the driver only when the feature set changes.

**No driver rebuild:** add/remove sample, edit loop, change ADSR, change notes or clip contents, change pan/volume, change echo delay within enabled echo features, regenerate wavetable frames within enabled morph features.

**Driver rebuild:** enable any handler (vibrato, source-step, paired-crossfade, morph, SFX queue, mid-song echo, dual-voice oscillator, voice reservation/ducking).

Two feedback paths run continuously: a fast live estimate using cached driver-module sizes, and an exact compile report after debounce or manual compile. Driver size always comes from the assembler/linker map.

```
User edit → classify
  feature set unchanged → reuse driver, rebuild data, repack ARAM
  feature set changed   → resolve deps, rebuild driver, rebuild data, repack ARAM, refresh manifest, refresh UI
```

Debounce: drag = estimate only; note entry = fast sequence recompile; feature toggle = delayed full rebuild; manual Compile = exact full rebuild.

Build cache keys: driver feature-set hash, assembler_backend, assembler version, SPC700 source hash, compiler version, BRR encoder version.

---

## 7. UI visibility

The editor cannot expose controls unsupported by the active driver. Three feature classes:

- **Runtime feature** — requires SPC700 driver handler.
- **Compiled feature** — costs sequence bytes / S-DSP writes, no driver code.
- **Pre-rendered feature** — costs BRR/atom bytes, little runtime code.

If a control could be either runtime or compiled, prefer compiled.

---

## 8. Instrument model

A voice plays a BRR source with pitch, volume, envelope, and flags. There is no hardware "sample mode" vs "synth mode." The project model is `Track → Instrument → compiled physical voice plan`.

### 8.1 Sample Pool, Atom Pool, and Source Directory View

- **Sample Pool** — imported assets (WAV/AIFF/BRR), authored by the user, edited in the sample editor. Each entry tracks source path and SHA-256.
- **Atom Pool** — compiler-generated artifacts: synth atoms, atom-sequence frames, paired-voice atoms, wavetable frames. Authored via synth/wavetable editors; materialized by the compiler. Shows quality reports, BRR cost, post-decode metrics.
- **Source Directory View (debug)** — the unified SNES source list as it lives in ARAM, regardless of origin. Exposes hardware truth (addresses, loop points, source indices) but is not the primary authoring surface.

### 8.2 SampleInstrument

```yaml
type: sample
name: flute
source: flute_c4.wav         # → resolved via Sample Pool
root_key: C4
key_range: [C3, C6]
loop: true
loop_start: auto
loop_end: auto
adsr: { attack: 9, decay: 4, sustain: 5, release: 12 }
pan: center
echo: true
pre_emphasis: off            # off | gentle | strong
```

### 8.3 CompiledSynthInstrument — collapsed two-oscillator atom (Level 1)

```yaml
type: synth
name: two_osc_soft_saw
mode: collapsed_atom
osc_a: { shape: saw, octave: 0, semitone: 0, fine_cents: 0, level: 1.0 }
osc_b: { shape: triangle, octave: 1, semitone: 0, fine_cents: 0, level: 0.35 }
mixer: { normalize: true, soft_clip: false }
atom: { allowed_lengths: [64, 128, 256], preferred_length: 128 }
amp: { envelope: adsr, attack_ms: 80, release_ms: 600 }
echo: true
pre_emphasis: gentle
```

#### Detune and beating constraint

Arbitrary fine detune produces beating with period ≈ `1/Δf`, which generally does not terminate at any reasonable atom length. In collapsed-atom mode the UI does one of: (1) gray out fine detune entirely; (2) snap visibly to ratios that fit the chosen atom length, with a tooltip; (3) escalate the patch to `synth_dual_voice_atom_pair` (2 voices) with a one-click upgrade. Silent quantization is forbidden.

Higher synth levels are described in §9.

---

## 9. Synth path: levels and atom compiler

| Level | Name | Voices | Frame motion | Default frame cap |
|---|---|---|---|---|
| 1 | Static atom | 1 | none | 1 |
| 2 | One-voice atom sequence | 1 | source-step | 32 |
| 3 | Paired-voice crossfade | 2 | smooth | 64 |
| 4 | Wavetable / morph | 2 | many | 64 (warn >96, hard cap 128) |

All levels share the BRR atom compiler and encoder policy (§10).

### 9.1 Level 1 — Static atom

Two oscillators collapsed to one periodic atom. Allowed shapes: sine, triangle, saw, pulse. Allowed relationships: harmonic octave/semitone ratios, fixed phase. No free-running detune (§8.3).

**Pipeline:**

```
read patch
resolve oscillator periodicity
render ideal high-resolution cycle
apply optional pre-emphasis
try candidate atom lengths: 32, 64, 128, 256
phase-rotate to minimize loop discontinuity
BRR encode (exhaustive filter/shift search)
BRR decode
score waveform and spectral error AFTER decode
choose smallest atom satisfying quality threshold
emit BRR atom + source directory entry + metadata
```

**Atom report:** name; atom length; BRR byte size; root key; recommended pitch range; post-decode waveform error; spectral error; loop click score; HF loss estimate; BRR filter distribution; pre-emphasis applied; ARAM cost.

### 9.2 Level 2 — One-voice atom sequence

Multiple authored frames played sequentially via runtime source change. Useful for choppy/digital wave-sequence timbres. Compiler emits an atom dictionary plus `SET_SRC` / `NOTE_ON` / source-step events. **No true crossfade is possible on a single physical voice** — `XFADE_SRC` belongs to Level 3. Per-step transition discontinuity appears in the compile report.

Required: `synth_atom_sequence`, `sample_runtime_src_change`.

### 9.3 Level 3 — Paired-voice crossfade

Two voices crossfaded between atoms. Targets *Aquatic Ambience*-class smooth pads. Compiler emits per-voice volume slides phased for constant-power crossfade. Echo can mask transitions but is budgeted separately.

Required: `synth_paired_voice_crossfade`, `volume_slide`, `voice_pair_allocator`. Recommended: `protected_music_pair`.

### 9.4 Level 4 — Wavetable / morph editor

Constrained authored wavetable surface.

**In scope:** multi-frame editor; oscillator/additive/formula frame generation; per-frame BRR atom rendering with quality slider; frame pruning (near-duplicate detection); atom clustering (predictor-structure grouping); morph automation lane; quality-vs-ARAM slider; A/B ideal-vs-SNES preview.

**Out of scope:** importing complex audio and auto-decomposing it into a wavetable; real-time morphing on the SPC700.

Required: `synth_wavetable_morph`, `synth_paired_voice_crossfade`, `voice_pair_allocator`.

### 9.5 Frame-count caps

Frame caps are guardrails, not hardware constants. The compiler may prune or cluster frames below the requested count to fit ARAM. **Frame count is a budget, not a promise to retain every authored frame.** Pruning decisions appear in the compile report.

When frames are pruned or clustered, the compiler remaps morph automation onto the retained frame set and emits a report showing authored-frame → retained-frame mapping. No frame is dropped silently. Preview uses the retained compiled set, not the pre-pruned ideal set.

### 9.6 Per-level reports

Every level's compile output includes voice cost, ARAM cost, post-decode quality, and emitted events. No level degrades silently when its budget is exceeded. Per-level reports use the stable schema defined in §10.4.

---

## 10. BRR encoder policy

The encoder is a core compiler component, not an implementation detail.

### 10.1 Implementation

Rust-native encoder and decoder in the `core` crate. No external binary dependencies for the internal scoring path. Decoder is bit-exact relative to S-DSP BRR decode behavior including filter rounding and clamping. Encoder and decoder share a single fixture set; round-trip determinism is a unit-test gate. External validation against snes_spc lives in §16.

Raw BRR decode equivalence is a bit-identical gate at M0 — no tolerance negotiation. Tolerances against the oracle for S-DSP voice render and full-module render are calibrated by the harness (§18) at M0 and recorded as provisional; they are not quality gates until M1 freezes the first accepted tolerance table (§21).

#### Scoring modes

The compiler exposes three scoring modes for different concerns. Pre-emphasis, HF-loss, and pitch-transposition checks must use the S-DSP render mode, not raw BRR decode — the audible signal is shaped by 4-point Gaussian interpolation at variable sample rates, not by raw BRR decode alone.

| Mode | Signal path | Used for |
|---|---|---|
| Raw BRR decode | BRR block decode only | loop alignment, click detection, per-block predictor error |
| S-DSP voice render | BRR decode → Gaussian interpolation at the target pitch | HF loss, pre-emphasis validation, pitch-transposition error |
| Full module render | voice render → mixing → echo → output | sequencing, voice interactions, peak/clipping estimates |

### 10.2 Search strategy

Per 9-byte BRR block: exhaustive search over filters 0–3 and shift/range; pick the (filter, shift) pair minimizing per-block error subject to clamping.

#### Loop-state handling

Filters 1–3 reference previous samples, so a looped atom's quality depends on predictor state across the loop boundary, not just within a single decode pass. The encoder evaluates the decoded loop over **repeated iterations**, and loop scoring includes predictor history continuity from one iteration into the next.

Optional strategies the encoder may apply, especially for tiny atoms where loop-boundary error dominates:

- phase rotation of the source cycle (see §10.3);
- a loop warm-up block before the loop point;
- duplicating the leading block at the tail to seed predictor state;
- rejecting filter selections that produce unstable transients across iterations even when single-pass error looks low.

### 10.3 Phase rotation for synth atoms

Try N phase rotations of the source cycle (e.g. N=16 or 32). BRR-encode each. Score after BRR decode. Pick the rotation with lowest combined waveform-discontinuity + spectral-error + loop-click score. Cheapest rotation pre-encode is often not cheapest post-decode.

### 10.4 Post-decode scoring

All loop quality, click, spectral, and waveform scoring is measured on the decoded BRR result. Reports include per-block BRR error, selected filter distribution, post-decode loop-click score, FFT-based spectral error weighted by perceptual band, HF-loss estimate, pre-emphasis parameters.

Quality reports use stable field names so tests and UI consume a fixed schema:

- `loop_click_score` — post-decode discontinuity at the loop boundary
- `spectral_distance_db` — FFT spectral error, perceptually weighted
- `hf_loss_db` — HF roll-off estimate vs ideal
- `render_rms_error_db` — post-render PCM error vs ideal
- `predictor_history_score` — loop-iteration predictor stability
- `brr_filter_distribution` — per-filter block counts, 0–3

Thresholds attached to these fields may tune over the calibration period (§18); field names are stable from M3 onward.

### 10.5 Pre-emphasis

Pre-emphasis compensates for S-DSP gaussian-interpolation HF dulling. It is part of the compiler, not the runtime. Three presets:

- **Off** — no pre-emphasis. Default for imported samples.
- **Gentle** / **Strong** — see playback-frequency definitions below.

Preset names describe the target playback result, not a fixed source-rate EQ curve. Gentle means roughly +1.5 dB compensation around 5 kHz in S-DSP voice-render output at the asset's root pitch. Strong means more aggressive compensation for very dull material. The compiler derives the source-rate pre-emphasis filter from the root key, intended pitch range, and render target, then validates the result across the instrument's compiled pitch range.

Per-sample and per-atom selection. Applied before BRR encode. **Pre-emphasis is validated using S-DSP voice render mode** (§10.1), not raw BRR decode — the dulling it compensates for is interpolation behavior, not decode behavior. Preview plays the post-render result, not the ideal pre-emphasized waveform. Every pre-emphasis choice appears in the compile report.

---

## 11. Voice allocation and SFX

### 11.1 Voice groups

Some instruments need a voice group, not a single voice: `synth_dual_voice_atom_pair` (2), `synth_paired_voice_crossfade` (2 for the phrase), `synth_wavetable_morph` (2 for the morph).

### 11.2 SFX policies

- `no_sfx` — all 8 voices to music; no game integration.
- `reserved_sfx` — N voices reserved; music compiles against `8 - N`.
- `interruptible_pair` — music pairs may be stolen by SFX; warns at compile time.
- `protected_pair` — music pairs cannot be stolen.

### 11.3 Allocator rules

The voice-pair allocator: refuses to allocate a pair across an SFX-reserved boundary; refuses to schedule a paired instrument with fewer than 2 contiguous-eligible voices; rejects `interruptible_pair` on `protected_music_pair` instruments; emits hard errors on unsatisfiable allocations; warns when music sustain reaches `max_music_voices - 1`.

### 11.4 Track voice policy

`compiler_assigned`, `fixed_voice_N`, `fixed_voice_pair_N_M`, `reserved_for_sfx`. A paired instrument on a `fixed_voice_N` track is a hard error.

---

## 12. Sample editor and auto-loop

Sample mode accepts WAV/AIFF/BRR.

Auto-loop generates **ranked candidates**. It does not claim a perfect loop. The user picks among A/B/C with audible preview.

```
load → mono → optional normalize → estimate root pitch
→ select candidate windows → snap to 16-sample boundaries
→ optional resample for alignment → BRR encode → BRR decode
→ score click + spectral + waveform + decode error → rank → audition
```

Loop editor UI: waveform display; loop start/end handles; 16-sample grid; candidate list; BRR-decoded preview; raw preview; click-risk meter; BRR byte cost; root pitch estimate.

---

## 13. Sequencer

Reaper-like track-and-item workflow, SNES-constrained.

### 13.1 Internal timing model

The sequencer represents musical positions as **bars/beats/ticks**, not floating-point seconds. The compiler converts musical positions to driver ticks at compile time. The UI may visually behave like Reaper's free clip placement, but the schema stores quantized musical positions; free placement is UI sugar that snaps to the internal grid.

### 13.2 Track model

Per-track: name; instrument; voice policy; volume; pan; echo send; mute/solo; clips; automation lanes (volume, pan, pitch, morph position).

### 13.3 Compile diagnostics panel

Always visible: ARAM free; voice count; driver profile; enabled feature count; sequence byte cost; sample byte cost; synth atom byte cost; **echo buffer cost** (prominent); max simultaneous voices; max S-DSP writes per tick; errors and warnings.

---

## 14. Sequence bytecode

Boring and deterministic. Each command's cost is known in 60 Hz ticks. The compiler does difficult work offline.

**Core:**

```
WAIT ticks
NOTE_ON voice, source, pitch_index, volume_l, volume_r, adsr_or_gain
NOTE_OFF voice
SET_SRC voice, source
SET_PITCH voice, pitch_value
SET_VOL voice, left, right
SET_PAN voice, pan
SET_ADSR voice, adsr1, adsr2
SET_GAIN voice, gain
SET_ECHO_MASK mask
LOOP_BEGIN count
LOOP_END
END
```

**Optional (gated by feature flag):**

```
VOL_SLIDE voice, target_l, target_r, ticks
PAN_SLIDE voice, target_pan, ticks
PITCH_SLIDE voice, target_pitch, ticks
PORTAMENTO voice, target_pitch, velocity, ticks
SET_NOISE voice, noise_freq
SET_PITCH_MOD mask
ECHO_PARAM_SET reg, value          ; only with echo_mid_song_param_changes
XFADE_SRC voice_a, voice_b, src_next, ticks
MORPH_STEP pair_id, frame_index, ticks   ; compiler IR, see §14.1
```

The compiler rejects bytecode commands unsupported by the active driver feature set.

### 14.1 MORPH_STEP lowering

`MORPH_STEP` is a compiler-level IR command. By default it lowers into existing primitives — `SET_SRC` + `VOL_SLIDE` + `WAIT` against the assigned voice pair — keeping the runtime small. A dedicated driver-side morph handler is only emitted when a feature flag explicitly opts in (e.g., a future `morph_runtime_handler`); without that flag, no `MORPH_STEP` opcode appears in the final driver bytecode.

This preserves the boring-driver principle: high-level musical intent stays in the compiler, the driver dispatches primitives.

### 14.2 Bytecode ABI

The bytecode itself is versioned independently of the project file.

```
bytecode_version          monotonic integer; bumped on any breaking change
endianness                little-endian (matches SPC700)
command encoding          variable-length; opcode byte followed by 0..N operand bytes
max_command_length        16 bytes (driver-enforced; overlong commands are hard errors)
invalid_command_behavior  driver halts the sequence and sets an error flag readable
                          by the host tool via the SPC export; never silent
```

**Compatibility rule:** every compiled module pairs a `bytecode_version` with a `driver_hash`. The driver loader validates that `bytecode_version` is within its supported range; mismatch is a hard error at upload time, not at runtime. The capability manifest (§5.4) carries `bytecode_version` alongside `driver_hash`.

When `bytecode_version` is bumped, older modules either continue to work (if the new driver maintains backward compatibility) or require a recompile from the source project (§16). Modules are not migrated; projects are.

---

## 15. ARAM packing

### 15.1 Standard layout

The first 512 bytes of ARAM are fixed CPU/runtime territory, not ordinary packer space:

```
0000–00EF  direct-page runtime variables
00F0–00FF  SPC700 hardware registers / I/O / timers / DSP address/data ports; never allocated
0100–01FF  hardware stack page; never allocated for driver code or data
0200–????  SPC700 driver code and driver constant tables
????–????  source directory (page-aligned for S-DSP DIR)
????–????  pitch tables
????–????  sequence bytecode
????–????  instrument metadata
????–????  BRR sample pool
????–????  generated synth atom pool
????–FFFF  echo buffer (top of ARAM, if enabled)
```

The packer prevents overlap and emits a hard error on collision. The top 64 bytes $FFC0–$FFFF are usable RAM only after the IPL ROM is unmapped; the driver/exporter must make that state explicit before placing data there.

### 15.2 ARAM report

```json
{
  "total_aram": 65536,
  "driver_code": 6144,
  "runtime_state": 512,
  "source_directory": 512,
  "pitch_tables": 768,
  "sequence_data": 2401,
  "sample_brr_pool": 16820,
  "synth_atom_pool": 1242,
  "echo_buffer": 8192,
  "free": 28945
}
```

### 15.3 Echo

Echo memory cost is `2 KB × EDL`, EDL 0–15.

| EDL | Echo bytes |
|---|---|
| 0 | 0 |
| 4 | 8 KB |
| 8 | 16 KB |
| 12 | 24 KB |
| 15 | 30 KB (~46% of ARAM) |

When echo is disabled, the driver must set FLG echo-write-disable and must not rely on EDL=0 alone. Hardware EDL=0 still causes a 4-byte echo write region at ESA*0x100 if echo writeback is enabled; ESA=0, EDL=0 can corrupt $0000–$0003.

Echo parameters (delay, feedback, FIR, volumes) are project-global and static. Per-voice echo enable/mask is allowed at any time. Mid-song parameter changes require `echo_mid_song_param_changes` and are out of default scope.

UI: ARAM meter shows echo as a labeled region (not generic "used"); echo cost displayed numerically next to the EDL slider in bytes and percent; default presets favor modest EDL (≈4); EDL ≥ 10 requires explicit opt-in with a confirmation noting it halves available sample ARAM. Echo controls live on a project-level panel.

#### Echo safety

Echo can self-oscillate or clip unpleasantly, especially through headphones during authoring. The tool:

- warns on high feedback values;
- warns on FIR/feedback combinations likely to clip or self-oscillate;
- starts preview at reduced output gain after any echo-setting change, restoring full gain on the next user action;
- includes peak-level and clipping estimates in the compile report when full-module render (§10.1) is available.

### 15.4 Budget policy

**Hard errors:** ARAM overflow; >8 simultaneous voices; unsupported bytecode; source directory overflow; BRR loop misalignment; echo buffer overlap; voice-pair allocation conflict.

**Warnings:** <2 KB free; S-DSP writes/tick near limit; one synth patch >2 voices; echo >16 KB; atom degraded post-decode; loop click risk high; sustain reaches `max_music_voices - 1`.

---

## 16. Project file format

The project file is the source of truth. Compiled artifacts are derived.

JSON, UTF-8, stable key ordering for Git diffs. Explicit top-level `schema_version`. Source data only — compiled BRR, atoms, and ARAM images live in a `build/` cache keyed by content hashes. Source audio referenced by path with recorded SHA-256.

```json
{
  "schema_version": 1,
  "project": {
    "name": "example",
    "tick_rate_hz": 60,
    "tempo_bpm": 120,
    "time_signature": [4, 4]
  },
  "driver": {
    "profile": "synth_static",
    "features": { "...": "..." }
  },
  "instruments": [
    { "type": "sample", "name": "flute", "...": "..." },
    { "type": "synth",  "name": "soft_saw", "...": "..." }
  ],
  "tracks": [
    {
      "name": "lead",
      "instrument": "soft_saw",
      "clips": [
        { "start_bar": 1, "start_beat": 1, "start_tick": 0, "length_beats": 4, "...": "..." }
      ]
    }
  ],
  "echo": {
    "scope": "project_global_static",
    "edl": 4,
    "params": { "...": "..." }
  },
  "compiler": {
    "atom_quality_threshold": 0.92,
    "pre_emphasis_default_sample": "off",
    "pre_emphasis_default_synth": "gentle",
    "frame_caps": { "level2": 32, "level3": 64, "level4": 64, "expert_override": false }
  },
  "asset_refs": [
    { "path": "samples/flute_c4.wav", "sha256": "..." }
  ]
}
```

**Migration:** higher `schema_version` than the tool supports → refuse with clear error; lower → run an explicit named migration (`migrate_v1_to_v2`, etc.); no migration → refuse. Migrations are forward-only, may drop or refactor fields, and log every change in a migration report shown to the user.

**Acceptance:** older schema fails safely or migrates explicitly, never silently. Compiled artifacts regenerate byte-identically from `project.json` plus referenced assets given the same compiler/encoder/assembler versions. Project files diff cleanly: stable key order, no embedded binary, no embedded compile timestamps.

---

## 17. Preview and emulator strategy

The compiler needs bit-exact BRR decode and S-DSP-relevant scoring behavior. It does not need a full SNES emulator embedded.

**Layer 1 — Internal renderer.** Rust-native BRR block decoder and a simple WAV render path. Drives atom scoring, A/B previews, and unit tests. This is what the atom compiler scores against.

**Layer 2 — Oracle validation.** The internal decoder is validated against blargg's `snes_spc` (LGPL; accurate S-DSP path passes hardware validation tests). Mismatches between the internal renderer and the oracle are reported, not silently ignored. This layer also renders generated `.spc` files via snes_spc as a full-APU validation.

**Layer 3 — External full-system validation.** ares (ISC, accuracy-focused) is the recommended external comparison target for full-ROM validation. Ares is not embedded — the compiler emits standard `.spc` and `.sfc` outputs; comparison harnesses live outside the host tool.

Three concerns are kept separate: compiler scoring (internal Rust path, three modes per §10.1), preview playback (internal path with optional oracle), and validation (oracle and ares).

---

## 18. Calibration harness

Atom quality thresholds — waveform error, spectral error, loop click score, HF loss — are not desk-solvable. The spec defines where they live and how they are tested.

Thresholds live in `compiler` settings in the project file. Defaults are conservative; warnings fire early. As listening tests accumulate, thresholds are tuned and recorded per project.

The harness compares three signals side by side with numeric metrics and audible playback:

1. **Ideal** — pre-encode target.
2. **Internal** — Rust BRR-decoded result.
3. **Oracle** — snes_spc BRR-decoded result.

M0 ships harness scaffolding; M3 ships a working harness that produces stable, reproducible measurements across the fixture set. The thresholds themselves are not locked at M3 — only the measurement infrastructure.

---

## 19. SPC and ROM export

**`.spc` export** — 64 KB ARAM image, S-DSP register state, SPC700 register state, metadata tags. Driver profile is baked in.

**`.sfc` test ROM** — initializes SNES, uploads driver/module to the APU, starts playback, optionally displays ARAM/voice/debug status.

**Game module export** — `driver.bin`, `module.bin`, `module_map.json`, `audio_symbols.inc`, `loader_stub_65816.asm`. Larger ROMs may store many driver/module combinations; the active module is still bound by the 64 KB ARAM limit.

### 19.1 Debug build outputs

Every compile additionally writes human-readable debug artifacts to `build/debug/`. These are essential for diagnosing M0–M3 issues and for letting the host tool (or a coding agent) reason about why something sounds wrong without guessing.

```
build/debug/dsp_writes.csv             tick-by-tick S-DSP register write trace
build/debug/voice_timeline.csv         per-voice note-on/off, source, pitch, slides
build/debug/aram_map.html              labeled ARAM layout with hover ranges
build/debug/source_directory.csv       final unified source list (Sample Pool ∪ Atom Pool)
build/debug/bytecode_disassembly.txt   sequence bytecode, decoded, with tick offsets
build/debug/atom_quality_report.html   per-atom scoring across all three modes (§10.1)
```

Debug output generation is fast and on by default; it can be disabled in `compiler` settings for batch builds.

### 19.2 .sfc test ROM loader contract

The `.sfc` test ROM includes a 65816-side loader stub responsible for the SPC700 upload protocol via the IPL ROM ($FFC0–$FFFF). The loader uploads the driver, source directory, sequence bytecode, BRR pools, and any other non-zero ARAM regions to the APU using the standard APU port handshake, then transfers control to the SPC700 entry address.

The loader contract is exposed via `audio_symbols.inc` and `loader_stub_65816.asm` and is fully specified before M1 ships. Mid-load timing budget, port-handshake validation, and recovery on NAK are part of the contract; silent upload failure is forbidden.

---

## 20. Driver build strategy

Build complete driver images per feature set. No runtime overlays, compressed loaders, self-modifying bundles, or in-ARAM dynamic linking. Driver code is small enough to duplicate in ROM; ARAM is the binding constraint.

Later optimization candidates (not in scope now): shared kernel + optional handler blocks; compressed ROM driver storage; overlay loader for rare commands.

---

## 21. Milestones

### M0 — Research harness

**Deliverables:**

- Rust toolchain bootstrap and project skeleton.
- Rust BRR encoder + raw decoder in the core crate.
- asar-backed minimal SPC700 hello-sample driver.
- Minimal `.spc` and `.sfc` exporters; ARAM map report.
- Oracle bridge spike against snes_spc (not gating).
- Calibration harness scaffolding and report-format definition (provisional tolerances allowed).

**Acceptance:**

- Known WAV fixture encodes and decodes deterministically; byte-identical across runs.
- Looped BRR atom round-trips through the decoder with legal loop alignment.
- Encoder exposes per-block error, filter selection, shift, post-decode loop-click score.
- Raw BRR decoder passes deterministic fixture tests with exact PCM equality for filter, shift/range, rounding, and clamp behavior. No tolerance negotiation; bit-identical or fail.
- Oracle bridge can render a fixed fixture corpus through snes_spc and produce a calibration report.
- The first calibration report records provisional tolerances for S-DSP voice render and full-module render. Provisional tolerances are not yet quality gates.
- M1 freezes the first accepted tolerance table; regressions against it become CI failures from M1 onward.
- A generated `.spc` renders successfully through snes_spc.
- One looped BRR sample plays in an SPC player and the test ROM.

M0 is complete only when internal decode, oracle render, ARAM layout, loop alignment, and SPC export all agree on deterministic fixtures. A BRR file playing is necessary but not sufficient.

M0 is complete when the raw BRR decoder is byte-exact on fixtures, asar produces a Mesen2-loadable `.spc`, the ARAM packer rejects overlap, and the calibration harness produces a structured report — even if the report's tolerance numbers are provisional. WLA-DX, embedded snes_spc preview, and final S-DSP render equivalence are out of scope until later milestones.

### M1 — Sample mode

**Deliverables:** sample slot UI; WAV/AIFF/BRR import; root key; ADSR/GAIN; pan; echo enable; loop candidate finder; BRR preview; ARAM meter with prominent echo cost; project file v1 with migration scaffolding; Sample Pool view.

**Acceptance:** create a C700-lite sample instrument; export `.spc` and `.sfc`; loop points snapped to BRR boundaries; compiler refuses ARAM overflow; project file round-trips; internal preview and oracle preview agree.

### M1.5 — Pattern sequencer harness

**Deliverables:** primitive sequencing surface — pattern grid using bars/beats/ticks; note on/off; fixed voice assignment; instrument assignment.

**Purpose:** test instruments musically; exercise the capability manifest in real sequence compilation; surface voice overflow and sequence byte cost early.

**Acceptance:** multi-note pattern compiles to deterministic bytecode; voice overflow → hard error; pattern exports as `.spc` and previews correctly.

### M2 — Driver capability system

**Deliverables:** granular feature flags; profile presets; dependency resolver; capability manifest; UI show/hide; assembler cache.

**Acceptance:** vibrato toggle pulls/removes correct handler code and UI; sample edits do not rebuild driver; paired-crossfade enable does rebuild driver.

### M3 — Static synth atom mode (Level 1)

**Deliverables:** two-oscillator patch editor with detune-constraint UI; collapsed periodic atom renderer; BRR atom encoder integration; atom quality report; pre-emphasis presets; calibration harness functional.

**Acceptance:** two-oscillator patch compiles to a looped BRR source; generated atom plays through the sample playback path; compiler reports atom size, post-decode error, pre-emphasis selection; detune UI matches §8.3 (no silent quantization); harness produces stable measurements across the fixture set.

### M4 — Reaper-like sequencer

**Deliverables:** track list; timeline items (free-look UI snapping to bars/beats/ticks); piano roll; automation lanes; voice assignment; compile diagnostics panel.

**Acceptance:** compose an 8-voice valid module; voice overflow rejected; voice-pair conflicts rejected; deterministic bytecode.

### M5 — Synth event mode

**Deliverables:** compiled pitch LFO; compiled tremolo; volume slides; pitch slides.

**Acceptance:** time-varying events emitted as bytecode without runtime oscillator code; S-DSP-write-rate warnings.

### M6 — One-voice atom sequence (Level 2)

**Deliverables:** atom dictionary editor; source-step bytecode; per-step transition click reporting; default cap 32 frames.

**Acceptance:** wave-sequence patch compiles; per-step discontinuity reported.

### M7 — Paired-voice crossfade pads (Level 3)

**Deliverables:** voice-pair allocator; paired source crossfade command; atom sequence editor; pad compile report; A/B ideal-vs-SNES preview; SFX coexistence policies; default cap 64.

**Acceptance:** smooth pad on two voices; voice and ARAM cost reported; echo, crossfade, and frame timing budgeted; illegal SFX/pair conflicts rejected.

### M8 — Wavetable / morph editor (Level 4)

**Deliverables:** multi-frame wavetable editor; oscillator/additive/formula frame generation; morph automation lane; frame pruning; atom clustering; quality/size slider; A/B ideal-vs-SNES preview; default cap 64, warn >96, hard cap 128.

**Acceptance:** authored wavetable patch compiles to optimized BRR atom dictionary plus morph events; quality/ARAM trade-off works; no audio decomposition required.

---

## 22. Testing

**Unit:** BRR encode/decode round trips; loop alignment; feature dependency resolver; ARAM packer overlap detection; echo buffer placement; voice-pair allocator (with SFX policies); sequence bytecode encoding; post-decode atom scoring; project schema migration; bars/beats/ticks ↔ driver-tick conversion.

**Golden:** known sample → known BRR; known feature set → known driver size; known project → known ARAM map; known synth patch → stable atom report; known wavetable → stable atom dictionary.

**Oracle:** internal Rust render vs. snes_spc render across the fixture set; mismatches reported.

**Emulator/audio:** render `.spc` through snes_spc and (later) ares; compare expected duration; detect stuck note, silence, severe clipping; check max voice count; check S-DSP write trace.

**Hardware (later):** flashcart playback; real SNES capture; emulator-vs-hardware comparison; headphone-safe echo feedback tests.

---

## 23. Open questions

These are empirical and resolved through use, not design:

1. Final atom quality thresholds — to be tuned via the calibration harness through listening tests. Raw BRR decode is bit-identical at M0 (no tolerance); voice-render and full-module-render tolerances are provisional at M0 and frozen at M1 (§21).
2. Whether to embed snes_spc directly for live preview or keep it as a validation-only oracle — decided after M0.
3. Practical wavetable frame caps — the 32 / 64 / 96 / 128 numbers are provisional; empirical testing may move them.
4. Whether a later version adds a free pre-emphasis EQ editor.

