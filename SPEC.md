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

**Core features:** `core_tick_loop`, `core_dsp_write`, `core_sequence_wait`, `core_note_on_off`, `core_pitch_table`, `core_source_directory`, `core_key_on_delay_safety`. A profile enables only the core features it actually implements; `core_pitch_table` is not mandatory for profiles whose pitch values are compile-time-seeded (e.g. `multi_voice_atom`, where the voice setup table seeds the pitch register at driver init). Profiles that use WAIT/slide/sequence opcodes require `core_tick_loop` and `core_sequence_wait`; polling-only profiles (`sample_basic` in M1) omit both.

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

`core_tick_loop` indicates the driver runs a 60 Hz nominal timer-tick (T0 driven). Polling-only profiles (`sample_basic` in M1) set this `false`. Profiles that use WAIT/slide/sequence opcodes require it `true`.

#### Feature dependency graph (canonical)

Locked at M2.4 prelude. Every feature whose flag is `true` MUST have all its declared prerequisites also `true`; the compiler validates this at every entry point (see §5.4 Manifest enforcement). The same edges apply to `sample_basic`, `multi_voice_atom`, and any future profile.

```
multi_voice_playback      -> core_note_on_off, core_dsp_write
synth_static_atom         -> sample_playback, core_source_directory
synth_atom_sequence       -> synth_static_atom, core_sequence_wait
synth_source_step         -> synth_atom_sequence, sample_runtime_src_change,
                             core_key_on_delay_safety
volume_slide              -> volume_set, core_tick_loop
sample_runtime_src_change -> core_source_directory, core_note_on_off
```

The compiler crate exposes the same graph as data in `core::capability_manifest::dependencies_of`; SPEC and code stay in sync via test-side enumeration.

#### M2 profile: `multi_voice_atom`

M2 introduces a second driver profile alongside `sample_basic`. Feature set:

```
core_tick_loop, core_dsp_write, core_sequence_wait, core_note_on_off,
core_pitch_table=false, core_source_directory, core_key_on_delay_safety,
sample_playback, sample_runtime_src_change, volume_set, volume_slide,
pitch_set, adsr, gain, pan_set, echo_enable, echo_static_params,
echo_per_voice_mask, synth_static_atom, synth_atom_sequence,
synth_source_step, multi_voice_playback
```

Dependencies follow the canonical feature graph above; every flag this profile enables has its prerequisites enabled.

Limits:

```json
{
  "max_music_voices": 2,
  "reserved_sfx_voices": 0,
  "max_sources": 128,
  "max_dsp_writes_per_tick": 24,
  "min_keyoff_to_keyon_ticks": 1,
  "max_sequence_bytes": 1024,
  "max_atom_sources": 32,
  "max_simultaneous_volume_slides": 1
}
```

UI gating: atom-pool / atom-sequence editors are hidden when `synth_static_atom = false`; volume-slide controls hidden when `volume_slide = false`; voice-1 controls hidden when `multi_voice_playback = false`.

#### Manifest enforcement

Capability-manifest enforcement is **authoritative at compile time** (`compile_sequence`, `pack_v2`, `compile-spc`, `compile-sfc`, `validate-project`, `m1-acceptance`, `m2-acceptance`). A capability mismatch is a hard error naming the missing capability (e.g. `feature \`synth_atom_sequence\` required but not in driver_profile=\`sample_basic\``).

The GUI editor enforces profile / atom-data consistency through schema validation (`ProjectV2::validate`'s rules 56-57 — `sample_basic` forbids atom data; `multi_voice_atom` requires at least one atom_sequence track) and through the editor's profile-switch handler (clears atom data on switch to `sample_basic`). The capability check at compile time is the source of truth; the GUI's job is to keep the project shape consistent so the compile-time check has nothing left to flag.

Cosmetic UI gating (showing / hiding atom-pool editors, volume-slide controls, voice-1 controls based on the active profile's feature set) is best-effort and informational — the compile-time check still runs regardless.

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

Rust-native encoder and decoder in the `core` crate. No external binary dependencies for the internal scoring path. Decoder is bit-exact relative to S-DSP BRR decode behavior including filter rounding and clamping. Encoder and decoder share a single fixture set; round-trip determinism is a unit-test gate. External validation against snes_spc lives in §17 and §18.

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

### 10.6 Loop-click metric (M3)

Formalizes the `loop_click_score` field from §10.4 into two
deterministic, encoder-independent metrics. M3 sub-passes compute
both; M3 gates only on the integer metric. The diagnostic windowed
metric is reports-only at M3 and may promote to a gate at M4+ after
it has stabilized.

**Definition (gated metric).** For a sample/atom whose decoded BRR
PCM is `decoded[]` of length `decoded_len`, with loop region
`[loop_start, loop_end)`:

```
loop_click_abs = abs((decoded[loop_start] as i32) - (decoded[loop_end - 1] as i32))
```

Type: `i32`. Computed on the raw BRR-decoded waveform (§10.4's "Raw
BRR decode" path), NOT on source PCM and NOT on S-DSP-rendered
output. Smaller is better; perfect seam = 0.

For generated single-cycle atoms (M2 atom v0, §16.9):

```
loop_start = 0
loop_end   = decoded_len
```

**Definition (diagnostic metric, reports-only).** A windowed
extension useful for catching seam shape problems the single-sample
metric misses:

```
loop_window_rms_delta(window = 8):
  pre  = decoded[loop_end - window .. loop_end]
  post = decoded[loop_start .. loop_start + window]
  sum_sq_delta = sum_i((pre[i] as i32 - post[i] as i32) ^ 2)   // accumulated in i64
  return sqrt(sum_sq_delta as f64)                              // f64; reports-only
```

The squared-difference accumulation is widened to `i64` to avoid
overflow on i16-range inputs (max `(2 * 32767)^2 * 8 ≈ 3.4 × 10^10`).
The final `sqrt` is computed once per report and is the only
floating-point operation in the metric. Cross-platform deterministic
in practice (single `f64::sqrt` of a deterministic i64 value).

**Gating policy.** M3 sub-passes gate on `loop_click_abs` only.
`loop_window_rms_delta` is reported in encode reports as diagnostic.
M4+ may promote it to a gate after the metric has stabilized.

**Pre-existing snapshots.** The M2 atom loop-click scores recorded
in `baselines/m2.json` (`M2_ATOM_128_SINE_LOOP_CLICK_SCORE = 1197`,
`M2_ATOM_64_SINE_LOOP_CLICK_SCORE = 2407`) were computed with this
formula pre-§10.6 and remain valid; M3 carries them forward as the
pre-encoder-improvement reference points. M3.1 records the
post-M3 atom loop-click scores under
`documentary_snapshot` in `baselines/m3.json`.

### 10.7 Phase rotation (M3 encoder optimization)

Refines §10.3 into a concrete encoder contract.

**Definition.** Phase rotation is encoder-input rotation of an
already-rendered atom PCM cycle. The atom render formula (§16.9)
runs unchanged; the encoder's input is a rotated copy of the
rendered PCM:

```
rotated[n] = pcm[(n + rotation_offset) mod cycle_len_samples]
```

The encoder selects the `rotation_offset` that minimizes the
loop-click metric (§10.6) under the lexicographic objective defined
below. The chosen offset is reported in the encode report.

The atom PCM SHAs (§16.9) are NOT affected by phase rotation —
rotation operates on a transient encoder input, not on the stored
atom render. This is the load-bearing invariant of the §16.9 atom
PCM stability rule: M3 encoder work changes BRR outputs but never
the rendered PCM the encoder consumes.

**Candidate set.** The encoder considers only block-aligned rotation
offsets:

```
candidate_offsets = [0, 16, 32, ..., cycle_len_samples - 16]
```

Yielding 4 candidates for cycle 64, 8 for cycle 128, 16 for cycle
256. Non-block-aligned offsets are reserved for future encoder
refinement (M4+).

**Objective.** The encoder picks the offset minimizing this
lexicographic tuple:

```
(loop_click_abs, peak_abs_error, rms_error, rotation_offset)
```

Lower is better at each level; later levels break ties at earlier
levels. NOT a weighted score — audit-friendly and deterministic.

```
peak_abs_error = max_i(abs((decoded[i] as i32) - (rotated[i] as i32)))
rms_error      = sqrt(mean_i(((decoded[i] as i32) - (rotated[i] as i32))^2))
```

Final tie-breaker: smaller `rotation_offset` wins (so the encoder
defaults to no-rotation when all candidates score identically).

**Error comparison sources (locked at M3.3 prelude).** For
candidate offset `r`, the secondary and tertiary lex levels
compare the decoded BRR PCM against the **rotated** source PCM,
NOT the unrotated original:

```
rotated_source[n] = source_pcm[(n + r) mod cycle_len_samples]
peak_abs_error    = max_i(|rotated_source[i] - decoded[i]|)        // i32 in widened arithmetic
rms_error         = sqrt(mean_i((rotated_source[i] - decoded[i])^2)) // f64 from i64 sum-of-squares
```

Comparing decoded against the unrotated source would penalize
phase displacement (which is the literal definition of rotation),
making rotation appear artificially worse. The decoded signal IS
the candidate signal; the rotated source IS the candidate's input;
their difference IS the encoder error for that candidate.

**Numeric types (locked at M3.3 prelude).**

```
loop_click_abs   : i32  (per §10.6, widened arithmetic)
peak_abs_error   : i32  (widened arithmetic on i16-range deltas)
rms_error        : f64  (computed from i64 sum-of-squares; sqrt at end)
rotation_offset  : u32
```

The lex tuple compares smaller-wins at each level. `f64` comparison
uses `f64::total_cmp` (stable since Rust 1.62) to avoid NaN
ordering ambiguity. The formula guarantees finite non-negative
`rms_error` (sum-of-squares ≥ 0; sqrt of finite non-negative is
finite non-negative), so `total_cmp` behaves identically to
`PartialOrd` here, but the explicit choice documents the intent
and removes any latent NaN-ordering footgun.

**Tie-break to offset 0.** When all candidates produce
bit-identical lex tuples (e.g. the `amplitude_zero` atom: all-zero
PCM across all rotations → identical scores), the smallest offset
wins. Iteration order MUST NOT affect the chosen offset; the
canonical `(loop_click, peak_err, rms_err, offset) < ...`
comparison handles this naturally because `offset` is the final
tie-break. A dedicated regression test
(`amplitude_zero_atom_phase_rotation_picks_offset_zero` in
`core/tests/atom_edge_cases.rs`) pins this behavior.

**Reporting.** Encode reports include `rotation_offset` (u32) and
`rotation_objective` (a struct of the four lexicographic components
for diagnostics).

**Bytecode / pitch invariance.** Phase rotation changes BRR sample
phase only. It MUST NOT alter sequence bytecode, pitch register
values, source index, or any event semantics — the contract is
"same atom, better BRR".

### 10.8 Predictor optimization (M3 conditional)

**Definition.** Predictor optimization is BRR encoder search
improvement at the per-block filter/shift decision layer. The M2.2
encoder is greedy (§10.2): each block independently selects the
filter/shift minimizing per-block error.

**Optional M3.4 cross-block search.** A bounded beam (or
Viterbi-style) search may improve loop-history behavior by
considering N-block lookahead:

```
beam_width = 4 (recommended starting point)
state      = previous decoded samples + cumulative score
candidates = filter ∈ {0,1,2,3} × shift ∈ {0..12} per block
score      = loop_click-aware + cumulative-error-aware
```

**Conditional ship.** M3.4 ships only if:

1. M3.3 phase-rotation gains are insufficient against the M3.0
   loop-click target, AND
2. the beam search produces measurable additional improvement over
   phase-rotation-alone, AND
3. the search runtime stays bounded (engineer's call on the bound;
   recommend ≤ 2× M2.2 encode time).

If any of (1)/(2)/(3) is not met, M3.4 defers to M4 and M3 ships
with M3.3 phase rotation only.

**No source PCM changes.** Predictor optimization changes
filter/shift selection only. Atom PCM SHAs (§16.9) and the render
formula stay unchanged.

### 10.9 Pre-emphasis (M3 stretch)

Refines §10.5 into a milestone-bound contract.

**Definition.** Pre-emphasis is a host-side filter applied to atom
PCM before BRR encoding to compensate for S-DSP gaussian
interpolation dulling on playback. Decode-side de-emphasis is NOT
implemented (the SPC700 driver doesn't filter on decode; see SPEC
§2 forbidden runtime features).

**Characterization required first (M3.5).** Before any pre-emphasis
preset ships, the encoder analysis pass must measure S-DSP gaussian
dulling by comparing raw BRR decode (host-side, §10.1) vs snes_spc
oracle render of the same SPC. The measurement characterizes a
frequency-response curve that pre-emphasis presets target.

**Presets only (M3.6, conditional).** If M3.5 characterization
yields a clear target, M3.6 ships these presets (matching §10.5's
preset names):

```
pre_emphasis: "off" | "gentle" | "strong"
```

Filter coefficients per preset are locked in SPEC at M3.6 land. No
free EQ editor in M3 (or M4); §10.9 is preset-bound (SPEC §2
forbids `free_eq_pre_emphasis_editor`).

**Conditional ship.** M3.6 ships only if M3.5 characterization is
clear and a preset audibly improves perceived quality without
making atoms harsher. If unclear, M3.6 defers to M4.

**Pre-emphasis interaction with phase rotation (§10.7) and predictor
optimization (§10.8).** Pre-emphasis runs BEFORE rotation/predictor
search; the encoder treats the pre-emphasized PCM as its new "input
PCM" for those passes. The atom PCM stability rule (§16.9) gates on
the rendered PCM before pre-emphasis — pre-emphasized PCM is a
transient encoder input, not stored, and its SHAs are not pinned.

**Pre-emphasis pipeline order (locked at M4.0 per consultant M4
plan #12).** When pre-emphasis presets exist (M4.5+
conditional), the encoder pipeline runs in this exact order:

```
atom PCM render (SPEC §16.9; identity-gated, unchanged)
  ↓
optional pre-emphasis filter applied to transient encoder input
(SPEC §10.9 preset; produces filtered_pcm)
  ↓
phase rotation (SPEC §10.7; operates on filtered_pcm)
  ↓
BRR encode (4-bit ADPCM)
```

The atom PCM SHA (§16.9) refers to the pre-filter render
output and remains identity-gated. Pre-emphasis is an
encoder-side transformation that operates on a transient
buffer; it MUST NOT shift atom PCM SHAs.

Phase rotation candidate offsets remain block-aligned
(SPEC §10.7) and operate on the **filtered** PCM. The lex
objective compares decoded BRR PCM against the
**rotated-filtered** source, NOT the rotated unfiltered
source. SPEC §10.10 noise-floor metrics likewise compare
decoded BRR against the rotated-filtered source when pre-
emphasis is active; against the rotated unfiltered source
when pre_emphasis = "off".

When pre-emphasis ships, `AtomBrrOutput` gains these fields
(documentation here at M4.0; populated at M4.5):

- `pre_emphasis_applied`: `"off" | "gentle" | "strong"`
- `rotation_offset_after_pre_emphasis`: `u32`
- `loop_click_abs_after_pre_emphasis_rotation`: `i32`

**Pre-emphasis preset is per-atom**, declared in the
`atom_pool[].pre_emphasis` field (default `"off"`). Schema rule
finalized at M3.6 land.

**Characterization report format (locked at M3.5 prelude per
consultant M3.3 audit #12; expanded at M3.5 per audition audit
#13; expanded again at M3.5.1 per consultant M3.5 audit #3, #4;
schema bumped to v4 at M4.0 for the alignment-validity fields
per consultant M4 plan #5, #6).** The characterization pass
emits a JSON report with this shape:

```json
{
  "schema_version": 4,
  "report_type": "gaussian_characterization",
  "fixture_set": "m3_5_canonical",
  "sample_rate_hz": 32000,
  "tool": {
    "snes_spc_oracle_sha256": "<runtime>",
    "rust_version": "<runtime>"
  },
  "test_signals": [
    { "name": "...", "kind": "sine|atom_fixture", "cycle_len_samples": 128 }
  ],
  "measurements": [
    {
      "name": "harmonic_8_cycle_64",
      "frequency_hz": 4000.0,
      "raw_decoded_pcm_sha256": "<sha>",
      "oracle_pcm_sha256": "<sha>",
      "raw_rms": 0.0,
      "oracle_rms": 0.0,
      "gain_delta_db": 0.0,
      "gain_delta_db_aligned": 0.0,
      "peak_abs_error_oracle_vs_raw": 0,
      "peak_abs_raw_vs_source": 0,
      "zcr_raw": 0.0,
      "zcr_oracle": 0.0,
      "clipping_count_raw": 0,
      "clipping_count_oracle": 0,
      "alignment_best_offset": 0,
      "aligned_raw_rms": 0.0,
      "aligned_oracle_rms": 0.0,
      "normalized_correlation": 0.0,
      "zcr_ratio": 0.0,
      "first_8_zero_crossings_raw": [],
      "first_8_zero_crossings_oracle": [],
      "peak_abs_error_after_gain_normalization": 0,
      "_phase_or_delay_note": "optional"
    }
  ],
  "subjective_audition": null,
  "_methodology_audit_m3_5_1": null,
  "summary": {
    "clear_target_for_pre_emphasis": false,
    "recommended_next": "defer",
    "decision_rule_reasons": []
  }
}
```

Field semantics:

- `raw_decoded_pcm_sha256` (`String`): SHA-256 of host-side BRR
  decode output (`core::brr::decode_blocks`); per-measurement
  deterministic reference.
- `oracle_pcm_sha256` (`String`): SHA-256 of the `snes_spc`
  oracle render (the DSP-interpolated playback).
- `raw_rms` (`f64`): RMS of host-side BRR decode.
- `oracle_rms` (`f64`): RMS of the oracle render.
- `gain_delta_db` (`f64`): `20 * log10(oracle_rms / raw_rms)`
  using the raw-window form (`raw_rms` over the full tiled raw
  buffer, `oracle_rms` over the aligned oracle window). Kept for
  backward comparability with M3.5 baselines; the M3.5.1
  `gain_delta_db_aligned` form (below) is the methodologically
  cleaner number.
- `gain_delta_db_aligned` (`f64`, M3.5.1 per consultant M3.5
  audit #3): `20 * log10(aligned_oracle_rms / aligned_raw_rms)`.
  Both RMSes are computed over the same aligned window. Removes
  the window-length bias the original `gain_delta_db` carries.
  Recommended reference if a future pass designs pre-emphasis
  presets.
- `peak_abs_error_oracle_vs_raw` (`i32`): max-abs sample-wise
  delta between oracle render and raw decode (gaussian + DSP
  error magnitude).
- `peak_abs_raw_vs_source` (`i32`): max-abs sample-wise delta
  between BRR-decoded PCM and the original source PCM (BRR
  encoder error magnitude). Separates encoder error from DSP
  error in the gain measurement.
- `zcr_raw` (`f64`): zero-crossing rate per second of the raw
  decode.
- `zcr_oracle` (`f64`): zero-crossing rate per second of the
  oracle render. Large `zcr_oracle - zcr_raw` delta indicates
  gaussian smoothing is flattening transients.
- `clipping_count_raw` (`i32`): count of samples at ±32767 (±1
  LSB) in the raw decode.
- `clipping_count_oracle` (`i32`): same for the oracle render;
  tracks whether DSP scaling introduces new clipping.
- `_phase_or_delay_note` (optional `String`): documentary note
  for any phase/delay alignment performed before the RMS / peak
  comparison; omit when no alignment was needed.
- `subjective_audition` (optional): see "Optional subjective
  audition field" below; omit (or `null`) when no audition has
  been performed for this characterization run.
- `recommended_next` (`"defer"` | `"pending_preset_eval"` |
  `"gentle_preset"` | `"strong_preset"` | `"methodology_review"`):
  outcome of applying the §10.9 decision rule below. The
  `"methodology_review"` outcome (M3.5.1) is set when the
  precondition #0 sanity check fails.

**M3.5.1 methodology diagnostic fields (consultant M3.5 audit
#4).** The following per-measurement fields surface methodology
artefacts (alignment phase, ZCR doubling, shape vs gain
differences) so they can be flagged in the report without
redesigning the underlying alignment / RMS pipeline:

- `alignment_best_offset` (`u32`): sample offset chosen by
  `align_oracle_to_raw`. Surfaces the gaussian delay.
- `aligned_raw_rms` (`f64`): RMS of the raw buffer over the
  aligned window only.
- `aligned_oracle_rms` (`f64`): RMS of the oracle buffer over
  the aligned window.
- `normalized_correlation` (`f64`, `[-1.0, 1.0]`): Pearson
  correlation between aligned raw and aligned oracle.
  Expected close to 1.0 when the oracle is a clean amplitude-
  scaled version of the raw decode; lower values indicate
  waveform shape differences (gaussian doing more than scaling,
  OR methodology artefact such as oracle-side aliasing).
- `zcr_ratio` (`f64`): `zcr_oracle / zcr_raw`. Expected ≈ 1.0
  for a clean sine through gaussian interpolation. Values ≥ 1.5
  or ≤ 0.67 indicate the oracle waveform has additional zero
  crossings the raw decode doesn't have — methodology
  suspicion. The precondition #0 sanity band is `[0.9, 1.1]`.
- `first_8_zero_crossings_raw` (`Vec<u32>`): sample indices of
  the first 8 zero crossings in the aligned raw buffer.
- `first_8_zero_crossings_oracle` (`Vec<u32>`): same for the
  aligned oracle buffer. Visual cross-reference with the raw
  series exposes inserted / removed crossings.
- `peak_abs_error_after_gain_normalization` (`i32`):
  `max |raw[i] - oracle[i] * (raw_rms / oracle_rms)|`. If this
  drops sharply from `peak_abs_error_oracle_vs_raw` the
  difference is gain-only; if it stays high, the difference is
  in shape.

**Optional `_methodology_audit_m3_5_1`** (top-level documentary
field, M3.5.1). When the M3.5.1 re-run surfaces methodology
anomalies, the report may include a top-level
`_methodology_audit_m3_5_1` object recording the anomalies
observed, the audit actions taken in this pass, and the next
steps. Field is optional (`null` when no audit applies).

**Test signal set `m3_5_canonical` (locked at M3.5, expanded
based on M3.5 audition audit #9).** The original Phase 0 set of
six signals was thin for characterizing a frequency-response
curve. Audition-driven amendment expands the harmonic series so
the curve has multiple anchor points between low fundamentals
and near-Nyquist.

Frequency-response anchors (single-cycle sines):

- `sine_cycle_64` — effective fundamental = 32000 Hz / 64 = 500 Hz.
- `sine_cycle_128` — effective fundamental = 32000 Hz / 128 = 250 Hz.
- `sine_cycle_256` — effective fundamental = 32000 Hz / 256 = 125 Hz.

Intermediate harmonics over `cycle_64` (probes the gain curve at
2× / 4× / 8× / 16× the fundamental):

- `harmonic_2_cycle_64` — 1 kHz.
- `harmonic_4_cycle_64` — 2 kHz.
- `harmonic_8_cycle_64` — 4 kHz.
- `harmonic_16_cycle_64` — 8 kHz (near-Nyquist for the 32 kHz
  S-DSP sample rate).

Full partial bank as a complex-signal anchor:

- `all_8_partials_max_amp_harmonics_1_to_8` — full partial-bank
  stress.

Stress reference (clipping-only; NOT a frequency-response anchor):
`normalize_false_multi_partial_clamp_safety` — included in the
report's `measurements` array but the `summary` decision rule
does NOT consider it (clipping introduces nonlinear distortion
that mis-shapes frequency-response measurement; audition
confirmed the metric-vs-perception masking on this fixture).

The frequency axis covers 125 Hz to 8 kHz with monotonic spacing
on the `cycle_64` harmonic series (1, 2, 4, 8, 16 kHz). Pre-emphasis
decisions need this curve, not a single high-frequency point.

**M3.6 decision rule (locked at M3.5, refined per audition audit
#10, #12; methodology precondition added at M3.5.1 per consultant
M3.5 audit #8).**

**Condition #0 — Methodology sanity (precondition, M3.5.1).**
Before evaluating conditions #1–#4, the characterization must
satisfy:

```
characterization_valid =
  zcr_ratio ∈ [0.9, 1.1] for ALL monotonicity-anchor signals
  (sine_cycle_64/128/256, harmonic_2/4/8/16_cycle_64),
  OR a documented methodology explanation exists in the
  report's _methodology_audit_m3_5_1 field.
```

If `characterization_valid` is false, `recommended_next` is set
to `"methodology_review"` and conditions #1–#4 are NOT
evaluated. The characterization is treated as informational
only; no preset ships from it. This precondition prevents
preset design against measurements that contain unresolved
methodology artefacts (e.g. the M3.5 ZCR-doubling anomaly that
motivated M3.5.1).

If `characterization_valid` is true, proceed to conditions #1–#4.

M3.6 ships pre-emphasis presets ONLY IF all four conditions
hold:

1. **Monotonic `gain_delta_db`.** Across the `cycle_64` harmonic
   series (`harmonic_2_cycle_64` → `harmonic_4_cycle_64` →
   `harmonic_8_cycle_64` → `harmonic_16_cycle_64`),
   `gain_delta_db` MUST be monotonically non-increasing. Higher
   frequencies attenuate at least as much as lower frequencies.
   Confirms the gaussian-dulling hypothesis.

2. **`harmonic_16` specifically responds.** The proposed gentle
   pre-emphasis preset MUST reduce `gain_delta_db` at
   `harmonic_16_cycle_64` by at least 25% of the measured raw
   loss. If `harmonic_16` characterization cannot show measurable
   improvement under any reasonable preset, pre-emphasis defers
   — this is the primary perceptual stress fixture per audition.

3. **Anti-worsening on canonical sines.** The gentle preset MUST
   NOT increase `peak_abs_error` or `rms_error` on
   `sine_cycle_64` or `sine_cycle_128` by more than 10% relative
   to the no-preset baseline. A high-frequency fix that degrades
   the canonical case is not a useful default.

4. **No clipping introduction.** The gentle preset MUST NOT
   cause any of the 11 atom fixtures to exceed `i16` saturation
   (no new clamping at ±32767 that wasn't already present at
   M3.3).

If any of (1) / (2) / (3) / (4) fails, M3.6 defers to M4+. The
report's `recommended_next` records the outcome:

- `methodology_review` — precondition #0 failed; conditions
  #1–#4 not evaluated. Details in `decision_rule_reasons`.
- `defer` — precondition #0 held but at least one of #1–#4
  failed (or M3.5-era raw form: `harmonic_16` did not attenuate
  past the -0.5 dB threshold); details in
  `decision_rule_reasons`.
- `pending_preset_eval` — M3.5 raw form: monotonicity and
  raw-`harmonic_16` response both hold but no preset has been
  evaluated yet. The "go signal" for M3.6 preset design.
- `gentle_preset` — all four conditions hold under a proposed
  preset; M3.6 implements `gentle` only.
- `strong_preset` — all four conditions hold AND the gentle
  preset closes ≥ 75% of the measured HF loss; M3.6 also
  implements `strong`.

**Optional subjective audition field (added at M3.5 per audition
audit #7).** The characterization report MAY include a top-level
`subjective_audition` field recording a perceptual A/B audition
performed against a separate set of reference renders. Subjective
audition data is kept structurally separate from the deterministic
`measurements` array: an audition documents which metrics
correspond to audible change and which are perceptually masked,
without altering measurement values.

Shape:

```json
"subjective_audition": {
  "audition_ref": "build/audition/m3.5-prelude/",
  "auditioned_at": "2026-MM-DD",
  "auditioned_by": "PM",
  "fixtures": [
    {
      "name": "sine_128",
      "perceived_change_axis": "harmonic_content",
      "masked_by_signal_content": false,
      "note": "Post-rotation reduces high harmonics; no audible click in either"
    },
    {
      "name": "normalize_false_clamp",
      "perceived_change_axis": "none",
      "masked_by_signal_content": true,
      "note": "87% metric improvement perceptually masked by clipping"
    }
  ]
}
```

Field semantics:

- `audition_ref` (`String`): repository-relative path or URI to
  the audition asset directory the audition was performed
  against.
- `auditioned_at` (`String`): ISO-8601 date.
- `auditioned_by` (`String`): identity of the auditioner (typically
  `"PM"`).
- `fixtures[].perceived_change_axis` enum:
  - `"seam_click"` — audible loop-point discontinuity.
  - `"harmonic_content"` — audible change in high-frequency
    content / brightness.
  - `"harshness"` — audible piercing / aliasing character.
  - `"none"` — no audible difference.
- `fixtures[].masked_by_signal_content` (`bool`): `true` when a
  measured metric improvement did NOT produce perceptible
  change (e.g. clipping or dense harmonics dominate).
- `fixtures[].note` (`String`): free-form auditioner note.

Omit the field entirely (or set to `null`) when no audition has
been performed. Do not populate stubs; absence means
"perceptual data not collected", not "audition produced empty
result".

**Reliable alignment criteria (locked at M4.0 per consultant
M4 plan #5, #6).** For the `m3_5_canonical` signal set,
alignment is considered RELIABLE only if ALL of the following
hold for every monotonicity-anchor signal
(`sine_cycle_64/128/256`, `harmonic_2/4/8/16_cycle_64`):

1. `zcr_ratio ∈ [0.9, 1.1]` (preserved from M3.5.1
   precondition #0).
2. `normalized_correlation ≥ 0.90` — Pearson correlation
   between aligned raw and aligned oracle PCM. M3.5.1
   measured 0.013–0.056 on low-frequency sines (well below
   this threshold), confirming the alignment methodology
   failure.
3. `alignment_best_offset < alignment_search_limit` — the
   chosen offset must not sit at the search-range boundary,
   which would indicate the true offset lies beyond the
   searched range.
4. `peak_abs_error_after_gain_normalization` materially lower
   than the unaligned `peak_abs_error_oracle_vs_raw` — gain
   normalization should substantially reduce the error if the
   raw/oracle difference is mostly amplitude; remaining error
   indicates waveform-shape divergence.

If any anchor signal violates any criterion,
`methodology_precondition_passed` is set to `false` in the
report; `recommended_next` is `"methodology_review"`; the
preset-design conditions (§10.9 #1–#4 plus M3.5.1
precondition #0) are NOT evaluated.

The correlation threshold of `0.90` is locked at M4.0.
Loosening requires explicit PM review.

**Alignment search range (locked at M4.0 per consultant M4
plan #7).** The search range used by
`align_oracle_to_raw` is:

```
alignment_search_limit = max_i(cycle_len_samples_i)
```

across the signals in the characterization run. For the
`m3_5_canonical` set with cycle lengths `{64, 128, 256}` this
yields `alignment_search_limit = 256`. M3.5.1's
`max_offset = 32` is superseded; the rationale is documented
in M3.5.1 STATUS and in SPEC §24.

If a future characterization set introduces longer cycle
lengths, the search range scales accordingly.

**Schema v4 (locked at M4.0).** The characterization report
schema bumps to `schema_version: 4` to reflect the alignment
contract. New top-level / per-measurement fields:

- `alignment_search_limit` (`u32`): the `max_offset` used for
  the run; surfaces the actual searched range so future
  readers know the upper bound the search could have
  resolved.
- `alignment_boundary_hit` (`bool`): `true` if any anchor
  signal's `alignment_best_offset == alignment_search_limit - 1`
  (within a small implementation-defined tolerance). Indicates
  the true offset may lie beyond the searched range.
- `alignment_valid` (`bool`): the AND of all four
  reliable-alignment criteria across monotonicity anchors.
- `methodology_precondition_passed` (`bool`): M3.5.1
  precondition #0 result; surfaced explicitly at v4 so
  consumers don't have to infer it from
  `recommended_next == "methodology_review"`.

These four fields land on the report struct at M4.0
(documentation here); they are populated by the
characterization pipeline at M4.1 / M4.2 once the alignment
fix and re-run land. The schema bump itself is locked at
M4.0.

### 10.10 BRR encoder noise floor metrics (M4)

**Definition.** For each atom or test signal, compute four
metrics on the raw BRR-decoded PCM compared against the
encoder INPUT (the rotated source PCM per SPEC §10.7;
NOT the pre-rotation original):

```
peak_abs_raw_vs_source = max_i(|rotated_source[i] - decoded[i]|)
                       : i32 in widened arithmetic

rms_raw_vs_source      = sqrt(mean_i((rotated_source[i] - decoded[i])^2))
                       : f64 from i64 sum-of-squares; sqrt at end

snr_db                 = 20 * log10(source_rms / rms_raw_vs_source)
                       : f64; if rms_raw_vs_source < epsilon,
                         snr_db = f64::INFINITY (encoded exactly)

clipping_count_raw     = count_i((decoded[i] as i32).abs() >= 32767)
                       : u32; widened to i32 so i16::MIN.abs() does
                         not overflow. Counts samples at the i16
                         saturation boundary on either polarity:
                         32767, -32767, and -32768.
```

**Reporting.** These metrics ship as `#[serde(default)]` fields
on `AtomBrrOutput` and `AtomRenderReport` at M4.0 (struct
fields land alongside this contract; the gaussian
characterization report adds per-measurement copies under the
same names). Values are populated by the encoder path at M4.3;
they default to `0` (or `f64::INFINITY` for `snr_db` when
encoded exactly) prior to wiring.

**Comparison source.** `rotated_source` per SPEC §10.7. The
M3.3 phase-rotation pass made rotation a transient encoder
input; the noise-floor metrics compare decoded BRR against the
post-rotation source so the measurement is faithful to what the
encoder actually saw. Comparing against the pre-rotation original
would conflate phase displacement with encoder error.

**Use as a research-spike exit criterion (M4.4).** Per consultant
M4 plan #17, the M4.4 encoder-improvement spike ships a
production encoder change ONLY IF at least one representative
fixture improves `rms_raw_vs_source` OR `peak_abs_raw_vs_source`
by `≥ 10%` AND:

- no fixture's `loop_click_abs` worsens (M3.3 improvement gate
  retained),
- no M2 behavioral gate regresses (audibility, silence,
  source-step ratio, module cap),
- encode runtime stays within `2×` the M3.3 phase-rotation
  baseline.

A negative finding ("BRR encoder near local optimum under
current constraints") is an acceptable M4.4 outcome and closes
the sub-pass without a production change.

**Determinism.** Pure integer + `f64` arithmetic. Sum-of-squares
in `i64` to avoid overflow on i16-range inputs (max
`(2 × 32767)² × N` for `N` samples). `sqrt` and `log10` are the
only `f64` ops. Cross-platform deterministic.

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

### 14.3 Sequence bytecode v2 (`SEQ2` — M2 multi_voice_atom)

Region header (placed in ARAM by the M2 packer between source directory and BRR pool):

```
u8[4]  magic = "SEQ2"
u8     bytecode_version = 2
u8     reserved = 0
u16    bytecode_len_le        ; length of the following bytecode payload
u8[]   bytecode               ; opcode stream
```

#### Opcodes (locked at M2.0)

```
$00  END
       args: none
$01  WAIT
       args: u8 ticks (1..=255)
       semantics: no further bytecode read until ticks elapsed
$10  SET_SRC
       args: u8 voice, u8 src_index
       validation: voice <= 1 in M2; src_index < source_count
$11  SET_VOL
       args: u8 voice, u8 vol_l, u8 vol_r
       validation: vol_l/vol_r 0..=127 in M2 (no phase inversion)
$12  KON
       args: u8 voice_mask
       validation: voice_mask & !0b00000011 == 0 in M2
$13  KOFF
       args: u8 voice_mask
       validation: voice_mask & !0b00000011 == 0 in M2
$20  VOL_SLIDE
       args: u8 voice, u8 target_l, u8 target_r, u8 ticks
       validation: ticks 1..=255; no overlapping active slide
$30  SET_PITCH
       args: u8 voice, u16 pitch_le
       validation: pitch <= $3FFF
```

#### WAIT execution model

On each timer tick, the driver applies one full tick step in this order:

1. If a slide is active for any voice, advance the slide accumulator one tick (write VOLL/VOLR to DSP — see slide accumulator state below).
2. If `wait_counter > 0`, decrement `wait_counter` by 1 and return for this tick.
3. Else, read and execute opcodes from the bytecode stream until:
   - (a) a `WAIT k` is encountered, which sets `wait_counter = k` and returns;
   - (b) `END` is encountered, which halts the sequence; or
   - (c) the per-tick DSP write budget would be exceeded by the next opcode, in which case the driver sets `write_budget_exceeded` in `status_flags` and returns (the unprocessed opcode resumes on the next tick).

`WAIT 1` therefore means "resume bytecode reading on the next tick after this one," NOT "wait zero ticks." `WAIT 0` is invalid: the M2.4 compiler MUST NOT emit it (trivially satisfied — `duration_ticks` validation rule 43 enforces 1..=255), and driver behaviour on `WAIT 0` is undefined.

#### Slide accumulator state

The driver maintains exactly one active slide at a time (per `multi_voice_playback` capability limit `max_simultaneous_volume_slides = 1`). A slide is described by:

```
slide_voice         u8     ; which voice (0 or 1) is being slid
slide_ticks_total   u8     ; original ticks param from VOL_SLIDE
slide_ticks_done    u8     ; ticks elapsed; advances 1..=slide_ticks_total
slide_start_l       u8     ; vol_l at slide start
slide_start_r       u8     ; vol_r at slide start
slide_target_l      u8     ; target vol_l from VOL_SLIDE operand
slide_target_r      u8     ; target vol_r from VOL_SLIDE operand
slide_active        bit    ; in active_voice_mask or status_flags
```

On each slide tick the driver writes:

```
elapsed = slide_ticks_done           ; just-incremented value, 1..=slide_ticks_total
dl      = slide_target_l - slide_start_l   ; signed i16 in compiler estimate
dr      = slide_target_r - slide_start_r
new_l   = slide_start_l + (dl * elapsed + sign(dl) * (slide_ticks_total/2)) / slide_ticks_total
new_r   = slide_start_r + (dr * elapsed + sign(dr) * (slide_ticks_total/2)) / slide_ticks_total
```

`+ sign(d)*(N/2) / N` is integer round-to-nearest with round-half-AWAY-from-zero (matching the atom render rounding mode from §16.9; same convention throughout). When `slide_ticks_done == slide_ticks_total`, the slide ends and `slide_active` clears. The final tick writes exactly `target_l` / `target_r` (the formula above evaluates to that exactly when `elapsed == total`).

Compiler's job: compute the slide writes-per-tick estimate (always 2 writes for an active slide tick — VOLL + VOLR). Driver's job: implement the formula deterministically in SPC700 assembly.

**Slide first-write timing.** A `VOL_SLIDE` opcode read on tick N registers a slide whose first VOLL/VOLR write occurs on tick N+1. This follows from the §14.3 execution model (slide-advance runs before opcode-read on each tick): on tick N, VOL_SLIDE is read and registers state, but the slide-advance step has already run for tick N, so the slide's first write happens at the start of tick N+1.

**Last slide write coincides with subsequent opcode read.** With `fade_out_ticks = K`, a `VOL_SLIDE` registered on tick N writes on ticks N+1, N+2, ..., N+K. The accompanying `WAIT K` expires at tick N+K. On tick N+K, the slide advance writes the final VOLL/VOLR, then wait-decrement reduces wait_counter to zero, then opcode-read consumes the next opcode (typically KOFF). The last slide write and the subsequent opcode therefore occur on the same tick. This is intentional and matches the canonical fixture's tick trace.

#### Source-step lowering

Switching the source register on a sounding voice is forbidden — it produces a click. Compilers that need to swap atom A → atom B on voice 1 lower the step into the following bytecode pattern:

```
VOL_SLIDE   voice=1, target=(0,0),                 ticks=4
WAIT        4
KOFF        voice1
WAIT        1
SET_SRC     voice1, next_src
SET_VOL     voice1, 0, 0
KON         voice1
VOL_SLIDE   voice=1, target=(target_l, target_r), ticks=4
WAIT        4
```

The capability flag `synth_source_step` enables this lowering; without it, sequence steps that imply a source change are rejected at compile time.

When `bytecode_version` is bumped, older modules either continue to work (if the new driver maintains backward compatibility) or require a recompile from the source project (§16). Modules are not migrated; projects are.

#### Driver-side `sequence_addr` semantics

The driver constants emitted by the build pipeline (`m2_constants.inc` `sequence_addr_lo` / `sequence_addr_hi`) MUST point at the bytecode payload start, i.e. `region_start + 8` bytes (skipping the SEQ2 region header `magic` / `bytecode_version` / `reserved` / `bytecode_len_le`). The driver does not re-validate the magic at runtime; the build pipeline is responsible for emitting a well-formed header and pointing the constant past it. The map report's `sequence_data` region addresses report the region start (header included), and individual driver builds offset by the 8-byte header.

#### KON / KOFF latching

The S-DSP `KOFF` register is level-sensitive: while a bit is set in `KOFF`, the corresponding voice is held in release every DSP frame, so a `KON` write after a `KOFF` will key the voice on for one frame and immediately key it back off — the voice reads as silence even though it is "playing." The driver MUST therefore clear `KOFF` (write `KOFF = $00`) before writing the `KON` mask in the `KON` opcode handler. The standard ordering inside `op_kon` is: write `KOFF = $00`, then write `KON = mask`. Bytecode authors do not emit explicit `KOFF = $00` opcodes; the driver handles latch-clear implicitly. (M2.5 baked this in after a source-step lowering produced silence on the post-step window in the canonical combined fixture.)

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

**Hard errors:** ARAM overflow; >8 simultaneous voices; unsupported bytecode; source directory overflow; BRR loop misalignment; echo buffer overlap; voice-pair allocation conflict; `module.bin` exceeds 32 KiB (one LoROM bank, see §15.5).

**Warnings:** <2 KB free; S-DSP writes/tick near limit; one synth patch >2 voices; echo >16 KB; atom degraded post-decode; loop click risk high; sustain reaches `max_music_voices - 1`.

### 15.5 Region-list policy

The M1 packer (`core::packer`) uses a fixed sample-only region list: driver code → source directory → BRR sample pool → free → optional echo buffer → IPL pad → IPL ROM shadow. This shape is locked for M1.

The M2 packer extends the region list per Appendix A.3 (driver evolution): driver code at `$0200` (4 KiB budget, unchanged) → source directory (page-aligned) → sequence data → sample BRR pool → synth atom pool → voice setup table → free → echo (top of usable, ending at `$FF00` per M1.4 layout). New region kinds are added through the M2 packer; M1 packer is not patched ad hoc.

### 15.6 module.bin size cap

`module.bin` (§19.4) is hard-capped at 32 KiB — one LoROM bank, the size embedded between banks 1 and 2 in the `.sfc` test ROM. The compiler emits `ModuleTooLarge` and exits with non-zero status when a project produces a module that exceeds this. M2 acceptance fixtures must fit within the cap.

### 15.7 Voice setup table (M2)

The M2 packer adds a small table the driver consults during init to seed each voice's DSP registers. One 11-byte entry per voice; M2 emits a 22-byte table for two voices.

**Per-entry byte map (binary ABI between packer and driver):**

```
byte 0   voice                   ; physical voice number, 0..=1 in M2
byte 1   src_index               ; SRCN; $FF = unused (see below)
byte 2   pitch_l                 ; 14-bit pitch register low byte
byte 3   pitch_h                 ; 14-bit pitch register high byte (low 6 bits)
byte 4   vol_l                   ; signed 8-bit, M2: 0..=127 (no phase invert)
byte 5   vol_r                   ; signed 8-bit, M2: 0..=127
byte 6   adsr1                   ; SPC700 ADSR1 register byte
byte 7   adsr2                   ; SPC700 ADSR2 register byte
byte 8   gain                    ; SPC700 GAIN register byte
byte 9   flags_reserved          ; M2: must be $00
byte 10  pad_reserved            ; M2: must be $00 (ABI byte explicit)
```

**Unused-voice sentinel.** When a voice is unused, the packer emits `src_index = $FF` and `$00` for every other field except `voice` (which records the physical voice number). The driver MUST detect `src_index = $FF` and skip all DSP setup for that voice; it MUST NOT KON the voice. This is the binary ABI between the M2 packer and the M2.5 driver.

The table lives in its own packer region between the synth atom pool and `free`; the driver init sequence reads each entry, programs the voice's S-DSP registers, and stops. M3+ profiles may extend `flags_reserved` with bits for runtime behaviour.

---

## 16. Project file format (v1)

The project file is the source of truth. Compiled artifacts are derived. M1 ships schema version 1; this section is the canonical contract.

### 16.1 Format basics

JSON, UTF-8, stable key ordering for Git diffs. Explicit top-level `schema_version`. Source data only — compiled BRR, atoms, and ARAM images live in a `build/` cache keyed by content hashes. Source audio referenced by path with recorded SHA-256.

### 16.2 Root schema

```json
{
  "schema_version": 1,
  "project": { "name": "m1_single_sample", "tick_rate_hz": 60 },
  "driver": { "profile": "sample_basic", "bytecode_version": 1 },
  "master_echo": { "...": "see §16.3" },
  "sample_pool": [ { "...": "see §16.4" } ],
  "m1": { "active_sample_id": "sample_0001" }
}
```

Required root fields:

- `schema_version`: integer, allowed value `1`.
- `project`: object, required.
- `driver`: object, required.
- `master_echo`: object, required.
- `sample_pool`: array, required, length `1..=128` for M1.
- `m1`: object, required.

`project`:

- `name`: string `1..=64`, no path separators, printable UTF-8.
- `tick_rate_hz`: integer, allowed `60`.

`driver`:

- `profile`: string. M1 allowed value: `"sample_basic"`.
- `bytecode_version`: integer. M1 allowed value: `1`.

### 16.3 master_echo block

```json
"master_echo": {
  "enabled": false,
  "edl": 0,
  "efb": 0,
  "evol_l": 0,
  "evol_r": 0,
  "fir": [127, 0, 0, 0, 0, 0, 0, 0]
}
```

- `enabled`: boolean.
- `edl`: integer `0..=15`. If `enabled = false`, must be `0`. If `enabled = true`, must be `1..=15`.
- `efb`: integer `-128..=127` (raw signed byte → DSP `EFB`).
- `evol_l`, `evol_r`: integer `-128..=127` each (raw signed bytes → DSP `EVOLL` / `EVOLR`).
- `fir`: array of 8 integers, each `-128..=127`.

**Trap.** `EDL = 0` with echo writeback enabled corrupts 4 bytes at `ESA*0x100`. M1 forbids `enabled=true` with `edl=0`; cross-validation in §16.6.

**Trap.** When `master_echo.enabled = false`, the driver must write FLG with the echo-write-disable bit set and `EON = $00`.

### 16.4 sample_pool entries

```json
{
  "id": "sample_0001",
  "name": "lead_sample",
  "source": {
    "path": "audio/lead.wav",
    "sha256": "hex...",
    "format": "wav",
    "sample_rate_hz": 32000,
    "channels": 1,
    "frames": 44100
  },
  "root_midi_note": 60,
  "loop": {
    "enabled": true,
    "start_sample": 1024,
    "end_sample": 32768,
    "snap": "brr_block_16"
  },
  "playback": {
    "volume": 1.0,
    "pan": 0.0,
    "echo": false,
    "envelope": {
      "type": "adsr",
      "attack": 9,
      "decay": 4,
      "sustain_level": 5,
      "sustain_rate": 12
    }
  }
}
```

`id`: string, length `1..=64`, pattern `^[a-z0-9_]+$` (ASCII lowercase letters, digits, underscore). Globally unique within the project. The strict shape lets compiled artifacts treat ids as filename-safe identifiers without escaping.

`name`: string, length `1..=64`, printable UTF-8, no control characters. Spaces, non-ASCII letters (e.g. `世界`), and slashes are allowed — sample names are display-only, not paths.

`source`:

- `path`: string. Relative path preferred; absolute allowed only with warning.
- `sha256`: lowercase hex string, length 64, characters `[0-9a-f]`.
- `format`: string. Allowed: `"wav" | "aiff" | "brr"`. M1 AIFF scope: PCM AIFF only — AIFF-C is rejected.
- `sample_rate_hz`: integer `8000..=96000` for WAV/AIFF. For BRR import: `32000` default, or explicit user-provided.
- `channels`: integer `1..=2` for M1.
- `frames`: integer `>= 1`.

`root_midi_note`: integer `0..=127`. C4 = 60 (see §16.7).

`loop`:

- `enabled`: boolean.
- `start_sample`: integer `>= 0`. Must be a multiple of 16. Required if `enabled = true`. Domain: mono PCM sample index after import conversion.
- `end_sample`: required if `enabled = true`. Constraints: `end_sample > start_sample`, `end_sample - start_sample >= 16`, `end_sample` multiple of 16, `end_sample <= source.frames`. End-exclusive: looped samples = `[start_sample, end_sample)`.
- `snap`: string. M1 allowed value: `"brr_block_16"`.

`playback`:

- `volume`: number `0.0..=1.0`.
- `pan`: number `-1.0..=1.0` (-1.0 = hard left, 0.0 = center, +1.0 = hard right).
- `echo`: boolean → DSP `EON` bit. If `echo = true`, `master_echo.enabled` must be `true`.
- `envelope`: tagged union, exactly one of ADSR or GAIN variant.

**Pan mapping** — constant-power, no phase inversion in M1:

```
theta       = (pan + 1.0) * PI / 4.0
vol_l_float = 127.0 * volume * cos(theta)
vol_r_float = 127.0 * volume * sin(theta)
VxVOLL      = clamp(round_half_up(vol_l_float), 0, 127)
VxVOLR      = clamp(round_half_up(vol_r_float), 0, 127)
```

**Envelope — ADSR variant**:

```json
"envelope": {
  "type": "adsr",
  "attack": 9,
  "decay": 4,
  "sustain_level": 5,
  "sustain_rate": 12
}
```

- `type`: required `"adsr"`.
- `attack`: integer `0..=15`.
- `decay`: integer `0..=7`.
- `sustain_level`: integer `0..=7`.
- `sustain_rate`: integer `0..=31`.

Register mapping:

```
ADSR1 = $80 | (decay << 4) | attack
ADSR2 = (sustain_level << 5) | sustain_rate
GAIN  = $00
```

**Trap.** Do not call the final field `release`. The S-DSP ADSR registers are `attack`, `decay`, `sustain_level`, `sustain_rate`. Key-off release is not a programmable ADSR field. Bit layout: `ADSR1 = E DDD AAAA`, `ADSR2 = SSS RRRRR`.

**Envelope — GAIN variant**:

```json
"envelope": {
  "type": "gain_raw",
  "gain_byte": 127
}
```

- `type`: required `"gain_raw"`.
- `gain_byte`: integer `0..=255`.

Register mapping:

```
ADSR1 = $00
ADSR2 = $00
GAIN  = gain_byte
```

`gain_raw` is the raw DSP byte. M1 deliberately does not expose a high-level GAIN envelope model; that lands in a later milestone after listening tests.

### 16.5 m1 block

```json
"m1": { "active_sample_id": "sample_0001" }
```

- `active_sample_id`: string. Must match exactly one `sample_pool[].id`.

### 16.6 Validation rules

Validation runs at project load and at compile entry. Every failed rule produces a `ValidationError { path, kind }` carrying a JSON-pointer path to the offending field (e.g. `/sample_pool/0/loop/end_sample`). The loader collects every failure rather than bailing on the first.

**Root.**

- `schema_version` must equal `1` (§16.2). Older schemas migrate via a named function (§16.8); other values reject.

**`project`.**

- `tick_rate_hz` must equal `60` (M1 only).
- `name` length `1..=64` (counted in Unicode codepoints), no control characters, no path separators (`/`, `\`, `:`).

**`driver`.**

- `profile` must equal `"sample_basic"` (M1 only).
- `bytecode_version` must equal `1` (M1 only).

**`master_echo`.**

- `edl` ∈ `0..=15`.
- `enabled = false` ⇒ `edl = 0`.
- `enabled = true` ⇒ `edl` ∈ `1..=15` (the §15.3 trap: `EDL = 0` with echo writeback enabled corrupts 4 bytes at `ESA*0x100`).

**`sample_pool`.**

- Length `0..=128`. (Empty pool is valid: a `sample_basic` project with no samples produces a silent SPC — a degenerate but well-formed module. M2.5 also requires this for `multi_voice_atom` atom-only fixtures whose oracle gates assert silence on the unused channel.)
- Each entry's `id` matches `^[a-z0-9_]+$`, length `1..=64`, globally unique within the pool.
- Each entry's `name` length `1..=64`, no control characters. Path separators allowed (sample names are display-only).
- `source.format` ∈ `{wav, aiff, brr}`.
- `source.sample_rate_hz` ∈ `8000..=96000`.
- `source.channels` ∈ `1..=2` (M1).
- `source.frames` ≥ `1`.
- `source.sha256`: lowercase hex, exactly 64 chars, characters `[0-9a-f]`.
- `root_midi_note` ∈ `0..=127`.
- If `loop.enabled = true`: `start_sample` and `end_sample` both multiples of 16; `end_sample > start_sample`; `end_sample - start_sample ≥ 16`; `end_sample ≤ source.frames`. `loop.snap` must equal `"brr_block_16"` (M1 only).
- `playback.volume` ∈ `0.0..=1.0`; NaN rejected.
- `playback.pan` ∈ `-1.0..=1.0`; NaN rejected.
- `playback.echo = true` ⇒ `master_echo.enabled = true`.

**Envelope.**

- Exactly one of ADSR or GAIN variant (the serde tagged-union shape from §16.4 enforces this at the format level).
- ADSR field ranges: `attack` ∈ `0..=15`, `decay` ∈ `0..=7`, `sustain_level` ∈ `0..=7`, `sustain_rate` ∈ `0..=31`.
- GAIN: `gain_byte` ∈ `0..=255` (always satisfied by `u8`; documented for completeness).

**`m1`.**

- `m1.active_sample_id` must match exactly one `sample_pool[].id`.

**M2 acceptance — fixture asset paths.** Every `sample_pool[].source.path` consumed by `m2-acceptance` must be relative to the project directory. Absolute paths emit a warning at import (existing M1 behaviour) AND fail validation when the project is used as the input to `m2-acceptance`. Rationale: M2 acceptance bundles must reproduce on a clean clone; absolute paths break reproducibility.

### 16.7 Pitch and MIDI convention

**MIDI numbering** (locked):

```
C4 = MIDI note 60
A4 = MIDI note 69
A4 tuning = 440.0 Hz
valid root_midi_note range = 0..=127
```

Frequency helper:

```
frequency_hz(note) = 440.0 * 2^((note - 69) / 12.0)
```

Project files store integer MIDI note numbers, not note strings (`"root_midi_note": 60`, not `"C4"`). UI may display `C4`; serialized data must not.

**Pitch register formula.** SNES voice pitch is a 14-bit value split across `VxPITCHL` (low byte) and the low 6 bits of `VxPITCHH`:

```
pitch_float =
  4096.0
  * (source_sample_rate_hz / 32000.0)
  * 2^((desired_midi_note - root_midi_note + cents_offset / 100.0) / 12.0)

pitch_u16 = clamp(round_half_up(pitch_float), 0x0000, 0x3FFF)
```

Rounding: `round_half_up(x) = floor(x + 0.5)`.

Register split:

```
VxPITCHL = pitch_u16 & $FF
VxPITCHH = (pitch_u16 >> 8) & $3F
```

**M1 collapse** — no transposition, no detune. `desired_midi_note = root_midi_note`, `cents_offset = 0`:

```
pitch_float = 4096.0 * source_sample_rate_hz / 32000.0
```

A 32 kHz sample at its root key yields `pitch_u16 = $1000`. A 22050 Hz source not resampled yields `pitch_u16 = round_half_up(4096 * 22050 / 32000) = 2822 = $0B06`.

**Trap.** If the importer resamples audio to 32 kHz before BRR encoding, `source_sample_rate_hz` for pitch purposes becomes `32000`, not the original file's rate.

### 16.8 Migration and acceptance

**Migration.** Higher `schema_version` than the tool supports → refuse with clear error. Lower → run an explicit named migration (`migrate_v1_to_v2`, etc.). No migration available → refuse. Migrations are forward-only, may drop or refactor fields, and log every change in a migration report shown to the user.

**Acceptance.** Older schema fails safely or migrates explicitly, never silently. Compiled artifacts regenerate byte-identically from `project.json` plus referenced assets given the same compiler/encoder/assembler versions. Project files diff cleanly: stable key order, no embedded binary, no embedded compile timestamps.

### 16.9 Project file format v2 (M2)

V2 introduces synth atoms, atom sequences, multi-track playback, and the M2 driver profile. The v1 keys (`schema_version`, `project`, `driver`, `master_echo`, `sample_pool`) carry forward unchanged; `m1.active_sample_id` is removed.

```json
{
  "schema_version": 2,
  "project": { "name": "...", "tick_rate_hz": 60 },
  "driver": { "profile": "multi_voice_atom", "bytecode_version": 2 },
  "master_echo": { "...": "same as v1, see §16.3" },
  "sample_pool": [],
  "atom_pool": [],
  "atom_sequences": [],
  "tracks": [],
  "m2": { "active_sequence_id": "seq_main" }
}
```

**`atom_pool[]` — synth atom v0** (kind `additive_single_cycle_v0`):

```json
{
  "id": "atom_0001",
  "name": "sine_128",
  "kind": "additive_single_cycle_v0",
  "root_midi_note": 60,
  "cycle_len_samples": 128,
  "amplitude": 0.75,
  "partials": [
    { "harmonic": 1, "amplitude": 1.0, "phase_cycles": 0.0 }
  ],
  "render": {
    "normalize": true,
    "force_filter_0_first_block": true,
    "force_filter_0_loop_entry": true
  },
  "playback": {
    "volume": 0.8,
    "pan": 1.0,
    "echo": false,
    "envelope": { "type": "gain_raw", "gain_byte": 127 }
  }
}
```

Validation:

- `cycle_len_samples`: 64, 128, or 256; must be a multiple of 16 (BRR alignment).
- `partials`: length 1..=8; each `harmonic` 1..=16; `amplitude` 0.0..=1.0; `phase_cycles` 0.0..1.0 (mod 1).
- top-level `amplitude`: 0.0..=1.0.

Render formula (compile-time PCM, then BRR-encoded through §10):

```
for n in 0..cycle_len_samples:
  x[n] = Σ partial.amplitude
         * sin(2π * (partial.harmonic * n / cycle_len_samples
                     + partial.phase_cycles))
if normalize:
  x[n] /= max_abs(x)
pcm_i16[n] = round_ties_away_from_zero(x[n] * amplitude * 32767)
```

Encode through the existing M1 BRR encoder. M2 atoms do not use phase rotation, spectral scoring, or pre-emphasis; those land at M3+ alongside Level-1 synth atom mode.

**Atom PCM stability across milestones (locked at M3.0).** The atom
render formula above (f64 additive sum of partials, normalize-then-
scale, round-half-away-from-zero, fixed cycle lengths {64, 128,
256}) is locked at M2.0 and MUST NOT change at M3 or later
milestones unless this section is explicitly reopened in a future
PM-approved pass. Atom PCM SHAs are identity-gated across
milestones; any drift indicates an unintentional change to the
render formula and is a regression.

The following PCM SHAs are stable across M2.0 → M3 → M4+:

- `M2_ATOM_128_SINE_PCM_SHA256`
- `M2_ATOM_64_SINE_PCM_SHA256`
- (Plus any atom PCM SHAs added at M3.2 from expanded edge-case
  coverage; M3.1 reclassifies the current entries from
  `documentary_snapshot` to `identity_gated` in
  `baselines/m3.json`.)

BRR SHAs derived from these PCMs MAY shift across milestones. M3
specifically targets BRR encoder quality (phase rotation §10.7,
predictor optimization §10.8, pre-emphasis §10.9) — all of those
change BRR bytes intentionally without touching the rendered PCM
the encoder consumes.

**`atom_sequences[]`:**

```json
{
  "id": "atomseq_0001",
  "name": "two_step_atom_sequence",
  "voice": 1,
  "steps": [
    {
      "atom_id": "atom_0001",
      "duration_ticks": 120,
      "target_volume": 0.8,
      "transition": { "type": "initial_kon" }
    },
    {
      "atom_id": "atom_0002",
      "duration_ticks": 120,
      "target_volume": 0.8,
      "transition": {
        "type": "fade_to_zero_retrigger",
        "fade_out_ticks": 4,
        "fade_in_ticks": 4
      }
    }
  ],
  "loop": false
}
```

**`tracks[]`:**

```json
[
  { "id": "track_sample", "name": "sample voice", "voice": 0,
    "kind": "sample_sustain", "sample_id": "sample_0001" },
  { "id": "track_atom",   "name": "atom voice",   "voice": 1,
    "kind": "atom_sequence", "atom_sequence_id": "atomseq_0001" }
]
```

**Validation rules (v2 additions to §16.6):**

- `schema_version`: 1 (load only — migrates) or 2.
- `driver.profile`: allowed values `"sample_basic"` (carried-forward v1) or `"multi_voice_atom"`.
- `driver.bytecode_version`: 1 (`sample_basic`) or 2 (`multi_voice_atom`).
- `atom_pool[].id`: same regex as v1 sample id (`^[a-z0-9_]+$`).
- `atom_pool[].cycle_len_samples`: 64, 128, or 256 (matches the atom v0 design).
- `tracks[].voice`: 0..=1 in M2; unique across `tracks[]`.
- `atom_sequences[].voice`: must equal the `voice` of the referencing `atom_sequence` track. M2 allowed value: `1`.
- `atom_sequences[].steps`: length 1..=32; `duration_ticks` 1..=255 in M2.
- `transition`: first step must be `initial_kon`; subsequent steps must be `fade_to_zero_retrigger` in M2.
- Echo rules unchanged from v1: per-sample `playback.echo=true` requires `master_echo.enabled=true`.

### 16.10 Migration v1 → v2

```
schema_version            : 1 → 2
project                   : carry forward
driver.profile            : carry forward (typically "sample_basic")
driver.bytecode_version   : carry forward (typically 1)
master_echo               : carry forward
sample_pool               : carry forward
atom_pool                 : added, value = []
atom_sequences            : added, value = []
tracks                    : added, value = [
  { "id": "track_sample_0",
    "voice": 0,
    "kind": "sample_sustain",
    "sample_id": "<old m1.active_sample_id>" }
]
m1                        : DROPPED
m2                        : added, value = { "active_sequence_id": null }
```

Migrations are explicit named functions (`migrate_v1_to_v2`); load-time silent upgrades are forbidden. The tool emits a migration report listing every transformed field; the user accepts the migration before the project saves with `schema_version: 2`.

---

## 17. Preview and emulator strategy

The compiler needs bit-exact BRR decode and S-DSP-relevant scoring behavior. It does not need a full SNES emulator embedded.

**Layer 1 — Internal renderer.** Rust-native BRR block decoder and a simple WAV render path. Drives atom scoring, A/B previews, and unit tests. This is what the atom compiler scores against.

**Layer 2 — Oracle validation.** The internal decoder is validated against blargg's `snes_spc` (LGPL; accurate S-DSP path passes hardware validation tests). Mismatches between the internal renderer and the oracle are reported, not silently ignored. This layer also renders generated `.spc` files via snes_spc as a full-APU validation.

**Layer 3 — External full-system validation.** ares (ISC, accuracy-focused) is the recommended external comparison target for full-ROM validation. Ares is not embedded — the compiler emits standard `.spc` and `.sfc` outputs; comparison harnesses live outside the host tool.

Three concerns are kept separate: compiler scoring (internal Rust path, three modes per §10.1), preview playback (internal path with optional oracle), and validation (oracle and ares).

### 17.1 Tool discovery

Host-side external tools are resolved via environment variables, with PATH as fallback:

| Tool                              | Env var                  | Fallback                                       |
|-----------------------------------|--------------------------|------------------------------------------------|
| asar (assembler)                  | `SFCWC_ASAR`             | `asar` / `asar.exe` on PATH                    |
| snes_spc oracle wrapper           | `SFCWC_SNES_SPC_ORACLE`  | `tools/snes_spc_oracle` next to the workspace  |
| Mesen2 (manual verification only) | `SFCWC_MESEN2`           | `Mesen.exe` / `Mesen` / `Mesen2.exe` on PATH; not auto-launched, user opens manually |

The host app's `doctor` command reports which tools were resolved, their versions, and their resolution paths. Missing tools produce diagnostic warnings, not crashes; commands that strictly require a missing tool fail with a clear error pointing at the env var.

**asar invocation split.** asar is invoked with two distinct flag sets depending on what's being built:

- **SPC700 ARAM image build** (`m1_sample_basic.asm`, future driver assemblies): `asar --no-title-check --fix-checksum=off`. The output is a flat 64 KB ARAM image with no SNES ROM header; `--fix-checksum=off` prevents asar from injecting LoROM checksum bytes into the flat region.
- **LoROM `.sfc` build** (`m1_loader_65816.asm`, future ROM assemblies): `asar --no-title-check --fix-checksum=on`. The output is a power-of-two LoROM ROM file; asar fills the header checksum and complement at `$7FDC..$7FDF`. After embedding the module(s) into the bank-1 / bank-2 regions, the host re-fixes the checksum (asar's pre-embed checksum goes stale once the module bytes land).

**Process-boundary oracle rule.** snes_spc is invoked across a process boundary; the host application never links against it, and it is never embedded in any compiled output (`.spc` / `.sfc` / module blob / driver image). Live preview at M3+ uses the internal Rust BRR decoder, never the oracle. The oracle remains the validation second-source.

### 17.2 Audition path (M1.3)

Sample audition at M1.3 lands as a side-effect-free file write, not an audio-device engine.

```
Command / UI action: Preview BRR
Output:              build/m1/previews/<sample_id>_decoded_brr.wav
Format:              WAV PCM16, mono, sample rate = imported source.sample_rate_hz
                     unless explicitly resampled
Samples:             decoded BRR PCM (post encode/decode), NOT the original WAV/AIFF
```

The user opens the resulting `.wav` in any media player or DAW. Reasons: cross-platform; auditionable without a host audio engine; no Mesen2 dependency for loop audition; UI playback stays out of the critical path.

**Trap.** Preview must play **decoded BRR**, not the original source. Auditioning the source lets the user approve loops and quality the SNES will not actually play. The compiled BRR is the ground truth for what ships.

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

The `.sfc` test ROM includes a 65816-side loader stub responsible for the SPC700 upload protocol via the IPL ROM (`$FFC0–$FFFF`). The loader uploads the sparse blocks listed in `module.bin` (§19.4) using the standard APU port handshake, then transfers control to the SPC700 entry address (`$0200` for M1).

The loader contract is exposed via `audio_symbols.inc` and `loader_stub_65816.asm`.

**APU port direction.** The 65c816-side APU communication ports are `$2140..$2143`; each side sees the value most recently written by the other side, and each direction has separate one-way storage that reuses the same numeric port names per direction. Logical convention:

```
Host → driver:
  65c816 writes $2140–$2143
  SPC700 reads  $F4–$F7
Driver → host:
  SPC700 writes $F4–$F7
  65c816 reads  $2140–$2143
```

**Upload handshake.** All host-side communication is 8-bit-only writes to `$2140–$2143`. IRQ and NMI are disabled around the tight upload routine.

```
1. Host waits for IPL ready signature in $2140 = $BB.
2. For each block in module.bin (sorted ascending by dest_addr):
   a. Host writes destination port mapping to $2142/$2143 (block start address).
   b. Host writes kickoff byte to $2141 = $CC and the byte counter / start byte
      sequence per the IPL upload protocol.
   c. Host writes block bytes one at a time, advancing the IPL ack counter on
      each byte; final-byte acknowledgement is observed before moving on.
3. After all blocks, host writes $2141 = $00 (jump-to-entrypoint marker) along
   with the entrypoint address ($0200 for M1) on $2142/$2143, signalling the
   IPL ROM to transfer control to the driver.
```

**Bounded host-side spin counts** (so a missed handshake fails fast rather than deadlocks the host):

```
WAIT_IPL_READY_POLLS        = 0x0020_0000
WAIT_BLOCK_KICK_ACK_POLLS   = 0x0002_0000
WAIT_BYTE_ACK_POLLS         = 0x0000_4000
WAIT_DRIVER_READY_POLLS     = 0x0020_0000
WAIT_COMMAND_ACK_POLLS      = 0x0002_0000
WAIT_RESET_TO_IPL_POLLS     = 0x0020_0000
```

Mid-load timing budget, port-handshake validation, and recovery on NAK are part of the contract; silent upload failure is forbidden.

**Trap.** Multi-byte communication-register writes and read/write timing hazards are documented in SNESdev errata; final-byte acknowledgement is easy to miss. Use 8-bit writes only, and disable IRQ/NMI across the tight upload routine.

**Spin count semantics.** The bounded-spin constants are semantic wait budgets, not exact iteration counts. Implementations may use any bounded loop yielding equivalent wall-clock duration (`WAIT_*_POLLS = 0x0020_0000` corresponds to roughly 50 ms at 21 MHz). The 65816 M1 loader uses 16-bit double loops with effective wait ≈ 50 ms — this satisfies the budget. Future loaders may switch to 32-bit multi-loops for tighter pacing without changing the contract.

**Ack verification.** Command-ack wait loops MUST verify both the ack code on `$2141` AND the round-trip token on `$2140`. Verifying only the code allows a stale ack from a prior driver run to pass the gate. The M1.6 loader was tightened in M2.0 (consultant finding #10) to enforce this; later loaders inherit the rule.

### 19.2.1 Loader fail-mode colour codes

When the 65816 loader hits one of its bounded-spin timeouts, it sets the BG colour register (`$2132`) to a recognisable colour and unblanks the display so the user sees a one-glance fail signal in the emulator:

| Colour | Cause                       |
|--------|-----------------------------|
| Red    | IPL-ready timeout           |
| Green  | Driver-ready timeout        |
| Blue   | Command-ack timeout         |
| White  | IPL byte-ack timeout        |

These are advisory; an emulator-side debugger gives a richer diagnostic. The colours are documented inline at the top of `m1_loader_65816.asm` so loader edits stay in sync with this section.

### 19.3 SPC smoke state contract (M0)

The M0 smoke `.spc` boots the SPC700 into a state that is silent, deterministic, and obviously running. Concretely:

| Field                                | Value   | Rationale                                                            |
|--------------------------------------|---------|----------------------------------------------------------------------|
| `PC`                                 | `$0200` | Start of driver code per §15.1                                       |
| `A`, `X`, `Y`                        | 0       | No assumed register state                                            |
| `PSW`                                | 0       | Direct page = `$00xx`, no flags set                                  |
| `SP`                                 | `$EF`   | SPC700 post-IPL default; stack starts at `$01EF`                     |
| DSP `$6C` (FLG)                      | `$60`   | bit 6 (Mute amp) + bit 5 (Echo write disable)                        |
| All other DSP registers              | 0       | Voices silent, KON unwritten                                         |
| Extra RAM (`$FFC0..$FFFF` shadow)    | 0       | No RAM contents behind IPL ROM                                       |
| Memory `$00F1` (CONTROL)             | 0       | Timers off, IPL ROM unmapped                                         |

Result: the SPC700 executes the driver from `$0200` while the DSP produces no audio output (mute amp). M0 smoke verification is "loads in Mesen2, no crash, no audible output." The smoke `.spc` writes ID666 indicator = absent (0x1B) and zero-fills the 210-byte ID666 region. M1+ smoke profiles will replace this with an audible-but-deterministic state.

### 19.4 module.bin binary format (M1)

The `.sfc` loader uploads only meaningful ARAM regions, not a contiguous 64 KB image. M1 uses a sparse-block module: never upload `$0000..$01FF` (zero page, I/O, stack — runtime territory). All multi-byte fields are little-endian. M1 schema version = 1.

The magic `"SFCWCM1\0"` identifies the module-binary format v1, introduced in M1. M2 reuses this format unmodified — header layout, block-table layout, and self-zeroed-SHA workaround are stable across the M1→M2 boundary.

**File layout.**

```
module.bin
Header: 64 bytes
0x00  u8[8]   magic = "SFCWCM1\0"  (hex: 53 46 43 57 43 4D 31 00)
0x08  u16     schema_version = 1
0x0A  u16     header_len = 64
0x0C  u16     block_count
0x0E  u16     entrypoint = $0200 for M1
0x10  u32     block_table_offset = 64
0x14  u32     data_offset = 64 + block_count * 8
0x18  u32     total_file_len
0x1C  u16     flags
              bit 0 = echo_enabled_for_module
              bits 1–15 = reserved, must be 0
0x1E  u16     reserved = 0
0x20  u8[32]  content_sha256_zeroed
              SHA-256 of the entire module.bin with bytes 0x20–0x3F
              set to 0 (self-reference workaround)

Block table entry: 8 bytes each
u16 dest_addr      ARAM destination address
u16 length         byte length; must be > 0
u32 data_offset    file offset of this block's data

Block data:
u8[length] data    raw bytes uploaded to ARAM[dest_addr ..]
```

**Block rules** (M1):

- Blocks are sparse.
- Blocks are sorted by `dest_addr` ascending.
- Blocks must not overlap.
- Blocks must not target `$00F0..$00FF` (DSP / I/O ports).
- M1 should avoid all blocks below `$0200`.
- Driver code is one block if contiguous.
- Source directory is one page-aligned block.
- BRR sample pool is one block.
- If echo is enabled, include a zero-filled echo-buffer block so echo memory is deterministic.
- If echo is disabled, do not include echo-buffer bytes.

**Self-reference trap.** The full-file SHA-256 cannot be stored inside the file (chicken-and-egg). The header carries `content_sha256_zeroed`: the SHA-256 of the file with the 32 SHA bytes themselves zeroed. The literal full-file SHA-256 lives in the M1 manifest as `module_file_sha256`.

---

## 20. Driver build strategy

Build complete driver images per feature set. No runtime overlays, compressed loaders, self-modifying bundles, or in-ARAM dynamic linking. Driver code is small enough to duplicate in ROM; ARAM is the binding constraint.

Later optimization candidates (not in scope now): shared kernel + optional handler blocks; compressed ROM driver storage; overlay loader for rare commands.

### 20.1 Driver command protocol (sample_basic profile, M1)

The host issues commands to the driver via the four APU communication ports while the driver replies on the same numerical ports in the opposite direction (§19.2). All values are bytes.

**Driver ready signature.** After init, the driver writes the following four bytes to `$F4..$F7` (host reads at `$2140..$2143`):

```
driver_out_0 = $A5
driver_out_1 = $5A
driver_out_2 = driver_version = $01    ; M1 = 1
driver_out_3 = status_flags
```

The host treats the `$A5 $5A` pair on `driver_out_0..1` as the "driver ready" signature.

**Status flags (`driver_out_3`).**

```
bit 0 = voice0_active
bit 1 = echo_enabled
bit 2 = stopped
bit 3 = error
bit 4 = reset_to_ipl_pending
bits 5–7 = reserved, must be 0
```

**Host command packet.** The host writes a 4-byte command packet to `$2140..$2143` (driver reads at `$F4..$F7`):

```
host_in_0 = command_token
host_in_1 = command_code
host_in_2 = arg0
host_in_3 = arg1
```

Rules:

- `command_token` must be nonzero.
- `command_token` must differ from the previous token (otherwise the driver may not observe the change).
- The driver treats a `host_in_0` change as "new command available" and only then reads `host_in_1..3`.
- For M1, `arg0` and `arg1` are `0`.

**Commands.**

| Code   | Name           | Notes                  |
|--------|----------------|------------------------|
| `$01`  | `STOP`         |                        |
| `$02`  | `RESET_TO_IPL` |                        |
| `$7F`  | `PING`         | diagnostic no-op       |

**Acks** (driver writes after acting on the host packet):

```
For STOP:
  driver_out_0 = command_token
  driver_out_1 = $81
  driver_out_2 = $01
  driver_out_3 = status_flags with stopped=1, voice0_active=0

For RESET_TO_IPL:
  driver_out_0 = command_token
  driver_out_1 = $82
  driver_out_2 = $01
  driver_out_3 = status_flags with reset_to_ipl_pending=1

For PING:
  driver_out_0 = command_token
  driver_out_1 = $FF
  driver_out_2 = $01
  driver_out_3 = status_flags

For invalid command:
  driver_out_0 = command_token
  driver_out_1 = $EE
  driver_out_2 = $01
  driver_out_3 = status_flags with error=1
```

**RESET_TO_IPL behaviour after ack.** The driver must:

1. KOFF voice 0.
2. Set FLG mute + echo-write-disable.
3. Clear EON.
4. Map IPL ROM by setting CONTROL bit 7.
5. Jump `$FFC0`.
6. Host then waits for IPL ready `$BB`.

The bounded host-side spin counts in §19.2 govern how long the host waits for each ack before declaring failure.

### 20.2 Driver: `multi_voice_atom` profile (M2)

The M2 driver runs a SPC700 timer-driven polling loop, not an interrupt. The S-SMP has two 8 kHz timers and one 64 kHz timer. M2 uses **T0** with `T0TARGET ($FA) = 133 (= $85)`, yielding a tick rate of `8000 / 133 ≈ 60.150 Hz` ("60 Hz nominal" — precision is sufficient for sequencing; tighter labels are M3+ work).

**Ticks, not seconds.** Source-step timing windows and oracle frame selection MUST be expressed in ticks or sample frames at 32 kHz, NOT in seconds. The M2.4 sequence compiler emits all durations in ticks; the M2.5/M2.6 oracle harness converts ticks-to-frames as `frames = ticks * 32000 / 60.150`.

**Init sequence:**

```
1. Write FLG mute + echo-write-disable.
2. Init DSP globals (master vol, DIR, echo regs).
3. Init voice 0 from voice setup table entry 0 (sample voice).
4. Init voice 1 from voice setup table entry 1 (default atom params).
5. Init sequence pointer (seq_ptr_lo / seq_ptr_hi → SEQ2 bytecode payload — see §14.3 "Driver-side sequence_addr semantics"; this is region_start + 8, not the SEQ2 header).
6. T0TARGET = $85.
7. Enable T0 via CONTROL register; clear T0OUT.
8. Issue `KON #init_kon_mask`. The build system computes `init_kon_mask` from the project's `tracks[]` as `OR over t in tracks where t.kind == sample_sustain of (1 << t.voice)`. Voices with `src_index = $FF` in the voice setup table are excluded (they are skipped by step 3/4 and have no DSP regs configured). `atom_sequence` track voices are NOT included in `init_kon_mask` — they are KON'd by the SEQ2 bytecode interpreter when their sequence executes its first KON opcode.
9. Enter main_loop.
```

**Main loop:**

```
main_loop:
  poll host commands (same protocol as §20.1)
  read T0OUT
  if T0OUT != 0:
    process min(T0OUT, 4) sequence ticks
    if T0OUT > 4:
      set status_flags timing_overrun bit
  bra main_loop
```

The 4-tick cap on per-iteration tick processing keeps the worst-case host-command latency bounded; tick overruns are surfaced via a status-flag bit rather than dropped silently.

**Driver state in zero page (M2):**

```
$00  seq_ptr_lo
$01  seq_ptr_hi
$02  wait_counter            ; remaining ticks before next bytecode read
$03  active_voice_mask
$04  status_flags
$05  slide_voice              ; 0 or 1; $FF = no slide active
$06  slide_ticks_remaining
$07  slide_target_l
$08  slide_target_r
$09  slide_step_l_signed
$0A  slide_step_r_signed
$0B  current_vol_l[0]
$0C  current_vol_l[1]
$0D  current_vol_r[0]
$0E  current_vol_r[1]
```

M2 supports one active VOL_SLIDE at a time. The bytecode compiler errors if a generated sequence overlaps slides; the driver does not attempt graceful handling of overlap.

**Tick semantics (matches §14.3 WAIT counting):** if `wait_counter > 0`, decrement, advance any active slide, return. Otherwise read commands until WAIT sets `wait_counter`, END is reached, or the per-tick DSP write budget would be exceeded.

**Status-flags M2 bit map (locked at M2.4 prelude):**

```
M2 driver status_flags byte (driver_version = $02):
  bit 0   voice0_active
  bit 1   voice1_active
  bit 2   echo_enabled
  bit 3   stopped                     ; set by STOP host command
  bit 4   reset_to_ipl_pending        ; set during RESET_TO_IPL ack
  bit 5   timing_overrun              ; T0OUT > 4 on a single tick
  bit 6   bytecode_error              ; END unexpectedly, bad opcode, etc.
  bit 7   write_budget_exceeded       ; per-tick DSP write budget hit
```

Single status byte; no extension byte. `driver_version = 2` (returned on `driver_out_2 = $F6`) communicates the new bit map; the M1 `driver_version = 1` status flags remain unchanged. Either driver keeps the ready signature (`$A5 $5A`) on `driver_out_0..1`. M2.5 implements; the bit positions above are now ABI.

---

## 21. Milestones

### M0 — Research harness

**Deliverables:**

- Rust toolchain bootstrap and project skeleton.
- Rust BRR encoder + raw decoder in the core crate.
- asar-backed minimal SPC700 hello-sample driver.
- Minimal `.spc` exporter and ARAM map report. (`.sfc` exporter deferred to M1 alongside the loader contract — see §19.2.)
- Oracle bridge spike against snes_spc (not gating).
- Calibration harness scaffolding and report-format definition (provisional tolerances allowed).

**Acceptance:**

- Looped BRR atom round-trips through the decoder with legal loop alignment.
- Encoder exposes per-block error, filter selection, shift, post-decode loop-click score.
- Raw BRR decoder passes deterministic fixture tests with exact PCM equality for filter, shift/range, rounding, and clamp behavior. No tolerance negotiation; bit-identical or fail.
- Generated `.spc` opens in Mesen2 and produces audio. M0 `.spc` verification is manual via Mesen2; automated playback validation comes in M1+.
- Oracle bridge can render a fixed fixture corpus through snes_spc and produce a calibration report.
- The first calibration report records provisional tolerances for S-DSP voice render and full-module render. Provisional tolerances are not yet quality gates.
- M1 freezes the first accepted tolerance table; regressions against it become CI failures from M1 onward.
- `sfcwc m0-acceptance` runs the full chain (doctor → decode-fixtures → assemble-smoke → export-spc-smoke → aram map → calibrate-oracle → manifest) and emits a `BundleSummary` whose `status` is `ok` when every required step succeeded and the optional calibration step also succeeded; `degraded` when any required step warned or the calibration step is skipped/error/warnings; `error` when any required step is skipped or errored. Required steps: doctor, decode_fixtures, assemble, spc_export, aram_map. Optional: calibration. `bundle.status == "error"` means M0 acceptance has failed and the bundle is not shippable.
- `sfcwc m0-status` re-runs the integrity check on an existing bundle without regenerating it, reporting per-step status, key cross-reference SHAs, and any drift findings. Exits 0 on `ok`/`degraded` + clean integrity, 1 on `error` or any integrity failure. Used in CI and by reviewers.

M0 is complete when the raw BRR decoder is byte-exact on fixtures, asar produces a Mesen2-loadable `.spc`, the ARAM packer rejects overlap, the calibration harness produces a structured report (provisional tolerances allowed), and `sfcwc m0-acceptance` writes a manifest whose `bundle.status` is `ok` or `degraded` in a development environment with the wrapper built. WLA-DX, embedded snes_spc preview, and final S-DSP render equivalence are out of scope until later milestones.

### M1 — Sample mode

**Deliverables:** sample slot UI; WAV/AIFF/BRR import; root key; ADSR/GAIN; pan; echo enable; loop candidate finder; BRR preview (§17.2); ARAM meter with prominent echo cost; project file v1 with migration scaffolding (§16); Sample Pool view; `module.bin` sparse-block format (§19.4); driver command protocol (§20.1); pitch register encoding (§16.7).

**Acceptance:**

- Create a C700-lite sample instrument and export both `.spc` and `.sfc`.
- Loop points snapped to BRR block boundaries (multiples of 16, §16.4).
- Compiler refuses ARAM overflow.
- Project file v1 round-trips: serialize → deserialize → re-serialize is byte-identical for the same compiler/encoder/assembler versions.
- Internal Rust BRR preview and snes_spc oracle preview agree within frozen tolerances (§10.1, §18, §21 — tolerances frozen at M1).
- `m1-acceptance` runs the M1 chain (doctor → validate-project for A and optional B → compile-spc → verify-spc-audible → compile-sfc → verify-sfc-structure → verify-sfc-modules-audible → manifest) and emits an `M1Manifest` carrying `M1BundleSummary` with the same `bundle.status` semantics as M0 (`ok` / `degraded` / `error`).
  - Required steps: `doctor` (asar must resolve), `validate_a`, `compile_spc`, `audible_spc`, `compile_sfc`, `structure_sfc`, `audible_sfc`. Optional: `validate_b` (skipped when no project B given — does not downgrade).
  - Doctor mapping: asar missing → step Error; oracle missing → step Warnings (audible steps then Skip); Mesen2 missing → step Ok (informational only).
  - Audible verification thresholds frozen at M1.7: `min_max_abs = 1000`, `min_rms = 200`. These are non-silence gates, not quality gates. Regressions become CI failures from M2 onward. M2 adds per-channel checks, source-step observability, and combined-energy consistency (Appendix A.7 / §21 M2); the M1 thresholds remain in force as a floor.
  - Cross-reference invariants enforced by `verify_m1_bundle` (`m1-status` re-runs them against the on-disk bundle to catch post-generation drift): `compile_spc.spc_file_sha256 == audible_spc.spc_sha256`, `compile_sfc.module_a_in_file_sha256 == structure_sfc.module_a.in_file_sha256`, `compile_sfc.sfc_path` matches `structure_sfc.sfc_path` (filename-equivalent), and all reports share the same `schema_version`.
  - **`verify-sfc-modules-audible` scope clarification.** This step is a module-content oracle check: it parses each embedded `module.bin`, reconstructs the 64 KB ARAM, wraps the result as an M1 SPC, and renders it through the snes_spc oracle. It does NOT execute the 65816 loader, the IPL upload handshake, or the `RESET_TO_IPL` flow. Loader execution remains a human/Mesen2 audition gate until automated emulator execution is available.
- `.spc` and `.sfc` exports use the same canonical `aram_image` and the same driver entrypoint (`$0200`). Allocated regions — driver code, source directory, BRR sample pool, driver constants, and the optional zero-filled echo buffer — must match bit-for-bit between the two artifacts. Free / unallocated regions are not parity-significant in `.sfc`: `$00F0..$00FF` and `$0100..$01FF` are runtime territory, and free ARAM is not semantically part of the module.

### M1.5 — Pattern sequencer harness

**Deliverables:** primitive sequencing surface — pattern grid using bars/beats/ticks; note on/off; fixed voice assignment; instrument assignment.

**Purpose:** test instruments musically; exercise the capability manifest in real sequence compilation; surface voice overflow and sequence byte cost early.

**Acceptance:** multi-note pattern compiles to deterministic bytecode; voice overflow → hard error; pattern exports as `.spc` and previews correctly.

### M2 — Driver capability system + multi-voice atom playback

**Deliverables:** granular feature flags; profile presets; dependency resolver; capability manifest; UI show/hide; assembler cache; project schema v2 with synth atoms / atom sequences / multi-track playback (§16.9, §16.10); `multi_voice_atom` driver profile (§20.2); sequence bytecode v2 (§14.3); voice setup table region (§15.7); `m2-acceptance` bundle command.

**Acceptance.** Capability gating exercises:

- Vibrato toggle pulls/removes correct handler code and UI.
- Sample edits do not rebuild driver; paired-crossfade enable does rebuild driver.
- Capability-manifest enforcement at every compile-spc / compile-sfc / pack / validate entry point: missing capability → hard error naming the feature.

`m2-acceptance` runs the full M2 chain and produces four oracle renders (per Appendix A.7 / consultant-locked thresholds):

```
A. sample_only.spc                — voice 1 muted
B. atom_only.spc                  — voice 0 muted
C. combined.spc                   — both voices active
D. combined.sfc                   — same project, .sfc path
```

The M2 fixture pans hard: voice 0 (sample) full left, voice 1 (atom) full right. The renders feed the following per-channel thresholds:

```
sample_only:  left.max_abs  >= 1000,  left.rms  >= 200,  right.rms <=  50
atom_only:    right.max_abs >= 1000,  right.rms >= 200,  left.rms  <=  50
combined:     left.max_abs  >= 1000,  left.rms  >= 200,
              right.max_abs >= 1000,  right.rms >= 200,
              combined.left.rms  within ±10% of sample_only.left.rms,
              combined.right.rms within ±10% of atom_only.right.rms
```

The M1 thresholds (`min_max_abs = 1000`, `min_rms = 200`) remain as the floor — the M2 per-channel checks are stricter, never looser.

**M2 oracle render frame count: 160 000 frames at 32 kHz** (= 5.0 seconds). This comfortably covers the canonical M2 sequence's 249 ticks (≈ 4.14 s at 60.150 Hz nominal). Per-channel windows for source-step zero-crossing-rate analysis use:

- Pre-step window: ticks 80..=120 → frames ~42 600..63 900 (atom A sustaining).
- Post-step window: ticks 130..=249 → frames ~69 250..132 500 (atom B sustaining post-transition).

M1's 16 384-frame render is too short for M2 sequences and is reserved for M1 acceptance only.

**Source-step observability.** The M2 fixture sets atom A = 128-sample sine, atom B = 64-sample sine at the same pitch register. After the source-step lowering pattern (§14.3) lands the bytecode for the swap, the right-channel PCM is windowed (ticks 40..100 after each KON) and compared:

```
zero_crossing_rate(post_window) >= 1.5 * zero_crossing_rate(pre_window)
normalized_correlation(pre_window, post_window) <= 0.95
```

The zero-crossing-rate ratio is the primary gate; the correlation check is informational and may be dropped from the bundle if its implementation cost outweighs the signal it provides.

**Process boundary.** Loader execution remains a human/Mesen2 audition gate; the M2 oracle renders verify module-content audio, not 65816 loader behaviour (consistent with §21 M1 sfc verification scope).

**v2-sample-only bit-identity invariant (locked at M2.4 prelude).** A v2 project that contains only sample data — empty `atom_pool`, empty `atom_sequences`, every track is `sample_sustain` on voice 0 — MUST produce byte-identical ARAM, SPC, and SFC output relative to the equivalent v1 project, unless an explicit baseline bump is approved. This invariant carries the M2.1 migration→pack bit-identity guarantee through M2.x and is enforced by `cli_pack_v2_sample_only_matches_v1_aram_sha` and `cli_migrate_project_then_compile_spc_matches_v1_baseline`.

### M3 — Static synth atom mode (Level 1)

**Deliverables:** two-oscillator patch editor with detune-constraint UI; collapsed periodic atom renderer; BRR atom encoder integration; atom quality report; pre-emphasis presets; calibration harness functional.

**Acceptance:** two-oscillator patch compiles to a looped BRR source; generated atom plays through the sample playback path; compiler reports atom size, post-decode error, pre-emphasis selection; detune UI matches §8.3 (no silent quantization); harness produces stable measurements across the fixture set.

**M3 baseline classification (locked at M3.0).** Mirrors the M2.8.1
identity-pin pattern. Three categories:

*Must NOT shift across M3 (regression if they do):*

- All atom PCM SHAs (M2.2 fixtures + M3.2 additions); see §16.9.
- `M2_CANONICAL_SEQUENCE_BYTECODE_SHA256`
- `M2_CANONICAL_VOICE_SETUP_TABLE_SHA256`
- `M2_CANONICAL_SEQUENCE_TOTAL_TICKS` / `_ELAPSED_TICKS`
- `M1_DRIVER_CODE_SHA256`
- `M1_LOADER_SIZE_BYTES` / `SHA256`

*Expected to shift at M3 (intentional changes from §10.7–§10.9):*

- All atom BRR SHAs (`M2_ATOM_*_BRR_SHA256`, etc.)
- All `loop_click_score` snapshots
- Decoded-BRR preview WAV SHAs (if any committed)

*Must remain behaviorally passing across M3:*

- M2 audibility floors (`max_abs >= 1000`, `rms >= 200`; §10.4)
- M2 silence ceiling (`max_abs <= 50` on hard-panned silent channel)
- M2 source-step zero-crossing ratio (≥ 1.5×)
- M2 32 KiB module cap (§15.6)

**M3 identity-gated baseline rule (carried from M2.8.1).** Every
new `identity_gated` baseline added to `baselines/m3.json` MUST
ship with a test that parses the baseline file via `include_str!`
and asserts the generated value equals the baseline entry. No
exception. This is the single rule the M2.8.1 audit
(`m1_driver_code_sha_matches_locked_baseline` + the M2.8.2
follow-up pass) was reverse-engineered to enforce; it is now a
forward contract.

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

1. Final atom quality thresholds — to be tuned via the calibration harness through listening tests (M3+). Raw BRR decode is bit-identical at M0 (no tolerance); the audible-render thresholds for the M1 voice / module path are **frozen at M1.7**: `min_max_abs = 1000`, `min_rms = 200`. Atom-quality (post-decode) thresholds remain open and will be set when the M3 atom encoder lands.
2. ~~Whether to embed snes_spc directly for live preview or keep it as a validation-only oracle.~~ **Resolved at M0.5/M0.6**: the host never links snes_spc; it is invoked across a process boundary (`LICENSING.md` §3, SPEC §17.1). Live preview at M3+ uses the internal Rust BRR decoder; the oracle remains the calibration second-source.
3. Practical wavetable frame caps — the 32 / 64 / 96 / 128 numbers are provisional; empirical testing may move them.
4. Whether a later version adds a free pre-emphasis EQ editor.

## 24. M4 prelude scope

Forward visibility on questions identified during M3 that may
be addressed at M4+. Not a commitment; M4 can pick and choose.
Recorded here so the M3 release notes and the M3.5.1
methodology audit have a stable forward reference.

1. **Gaussian characterization methodology resolution.**
   Expand `align_oracle_to_raw` search range to
   `≥ max_cycle_len` (currently capped at 32 samples in
   `core::characterize_gaussian`; canonical signals have
   `cycle_len_samples` up to 256). Re-run the M3.5
   characterization with reliable phase alignment. The
   M3.5.1 §10.9 decision rule precondition #0 will pass when
   `zcr_ratio` settles in `[0.9, 1.1]` for the
   monotonicity-anchor signals. Brute-force search is one
   option; cross-correlation or autocorrelation-guided
   alignment are alternatives.

2. **BRR encoder noise floor reduction.** M3.5.1 measured
   `peak_abs_raw_vs_source ≈ 18431` LSBs (over half of i16
   dynamic range) across all 9 canonical signals — BRR
   encoder distortion, not gaussian coloration. M4 may
   investigate per-block filter selection refinements
   (the cross-block beam search that M3.4 deferred per
   consultant M3.3 audit #21) or other predictor / quant
   optimizations.

3. **Pre-emphasis presets** (conditional on item 1
   resolution). If the M4 methodology audit confirms a real
   frequency-response curve worth compensating, ship
   `gentle` and / or `strong` presets per SPEC §10.9 with
   filter coefficients TBD. The §10.9 decision rule
   conditions #3 (anti-worsening on canonical sines) and
   #4 (no new clipping) require evaluation against a
   proposed preset's outputs.

4. **`rename_track_id_cascade`.** Currently no cascade
   needed since `tracks[].id` is not referenced elsewhere
   in the v2 schema (only `tracks[].voice` is referenced).
   Revisit if schema additions reference track ids.

5. **`baselines/m4.json`.** Create the file when M4 lands;
   inherit M3 by reference (mirror the M3-inherits-M2
   pattern). M3 acceptance becomes the M4 stage-1 regression
   gate the same way M2 acceptance is the M3 stage-1 gate.

