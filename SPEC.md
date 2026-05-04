\# SFC Wave Compiler — Specification



\## 0. Premise



SFC Wave Compiler is a standalone Rust application for SNES/Super Famicom audio authoring. It combines a C700-style BRR sample workflow with a compile-time synthesizer that emits SNES-valid BRR atoms, atom sequences, wavetable frames, and SPC700 playback bytecode.



It is not a soft synth, a DAW replacement, or an audio-to-SNES resynthesizer. It is a hardware-constrained authoring system targeting a custom SPC700 driver assembled from granular feature modules.



\*\*Core idea:\*\* author instruments at a higher level, compile them into an exact 64 KB ARAM layout — driver code, source directory, sequence bytecode, BRR samples, generated atoms, tables, echo buffer.



\### Guiding principle: compiler-side over runtime-side



Any feature that can be rendered into BRR atoms, tables, or bytecode offline does not become real-time SPC700 logic. Oscillator shapes, additive timbres, and wavetable frames render offline to BRR atoms. Pitch LFOs, tremolo, slides, and morph trajectories are bytecode events. Filtering and pre-emphasis are applied at compile time. The driver is a small, deterministic dispatcher: tick → bytecode → S-DSP register writes.



\---



\## 1. Goals



1\. Standalone Rust application for SNES audio composition and instrument design.

2\. Traditional BRR sample instruments.

3\. Compile-time synth instruments that generate BRR atoms.

4\. Constrained wavetable synthesis as a first-class authoring path: authored frames compiled to BRR atoms plus playback events.

5\. `.spc` preview file per compiled module.

6\. Minimal `.sfc` test ROM per compiled module.

7\. Game-ready module blobs for 65816 SNES projects.

8\. Small custom SPC700 driver assembled from granular compile-time modules.

9\. UI exposes only features supported by the active driver.



Secondary: Reaper-like sequencing view (not a DAW); C700-like sample tooling and accurate preview; clear compile reports for ARAM, voice, S-DSP write rate, source count, echo, driver size; future Reaper export/import.



\---



\## 2. Non-goals



\- Audio-to-wavetable decomposition from complex inputs.

\- VST/AU plugin.

\- SNES audio middleware stack.

\- Runtime sample streaming.

\- Driver hot-swap during playback.

\- Tracker clone.

\- Serum clone. (A \*constrained\* authored wavetable compiler is in scope.)

\- Real-time subtractive/wavetable synthesis on SPC700.

\- MOD/XM/IT/S3M effect import.

\- Dynamic linker for SPC700 code sections.

\- Free-form EQ pre-emphasis editor (presets only; see §10.5).



\---



\## 3. Hardware constraints



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



\---



\## 4. Architecture



```

Rust GUI / CLI

&#x20; ├─ Project model + file I/O + schema migration

&#x20; ├─ Sequencer (bars/beats/ticks)

&#x20; ├─ Sample editor

&#x20; ├─ Synth atom editor

&#x20; ├─ Wavetable/morph editor

&#x20; ├─ Driver feature resolver

&#x20; ├─ BRR encoder/decoder (core crate)

&#x20; ├─ Atom compiler

&#x20; ├─ Sequence compiler

&#x20; ├─ ARAM packer

&#x20; ├─ Voice-pair allocator

&#x20; ├─ SPC700 assembler invoker

&#x20; ├─ SPC / SFC / module exporters

&#x20; ├─ Internal preview renderer

&#x20; └─ Oracle bridge (snes\_spc)



SPC700 assembly driver

&#x20; ├─ boot/init

&#x20; ├─ S-DSP register helpers

&#x20; ├─ 60 Hz tick loop

&#x20; ├─ bytecode interpreter

&#x20; ├─ sample playback core

&#x20; ├─ optional feature handlers (synth atom / SFX / game)

```



Host: Rust. Driver: SPC700 assembly via WLA-DX (default); asar is a credible later option.



\---



\## 5. Driver capabilities



\### 5.1 Profiles



Profiles are presets over granular flags: `sample\_basic`, `sample\_fx`, `synth\_static`, `synth\_events`, `synth\_xfade`, `synth\_wavetable`, `game\_runtime`, `spc\_compo`.



\### 5.2 Feature flags



\*\*Mandatory core:\*\* `core\_tick\_loop`, `core\_dsp\_write`, `core\_sequence\_wait`, `core\_note\_on\_off`, `core\_pitch\_table`, `core\_source\_directory`, `core\_key\_on\_delay\_safety`.



\*\*Sample:\*\* `sample\_playback`, `sample\_multisample`, `sample\_keysplit`, `sample\_velocity\_layers`, `sample\_runtime\_src\_change`.



\*\*Envelope/expression:\*\* `adsr`, `gain`, `volume\_set`, `volume\_slide`, `pan\_set`, `pan\_slide`, `pitch\_set`, `pitch\_slide`, `portamento`, `vibrato`, `tremolo`, `detune`, `noise\_mode`, `pitch\_modulation`, `surround\_invert`.



\*\*Echo:\*\* `echo\_enable`, `echo\_per\_voice\_mask`, `echo\_static\_params`, `echo\_mid\_song\_param\_changes`, `fir\_filter\_editing`.



\*\*Synth (compile-time):\*\* `synth\_static\_atom`, `synth\_two\_osc\_collapsed\_atom`, `synth\_dual\_voice\_atom\_pair`, `synth\_atom\_sequence`, `synth\_source\_step`, `synth\_volume\_ramp`, `synth\_pitch\_event\_lfo`, `synth\_tremolo\_event\_lfo`, `synth\_paired\_voice\_crossfade`, `synth\_wavetable\_morph`.



`synth\_dual\_voice\_atom\_pair` schedules two generated BRR atoms as two physical S-DSP voices. It is not a runtime oscillator engine — the SPC700 plays BRR sources and writes S-DSP registers; the "dual voice" lives in compiled atoms, not driver code.



\*\*Voice allocation:\*\* `voice\_pair\_allocator`, `voice\_reservation`, `protected\_music\_pair`.



\*\*Game/runtime:\*\* `sfx\_queue`, `sfx\_priority`, `sfx\_one\_channel\_limit`, `sfx\_uninterruptible\_flag`, `music\_ducking`, `async\_loader\_api`, `module\_reload\_api`.



\*\*Forbidden:\*\* `runtime\_sample\_streaming`, `runtime\_code\_overlay\_loader`, `runtime\_dynamic\_linker`, `arbitrary\_wav\_resynthesis`, `runtime\_oscillator\_engine`.



\### 5.3 Dependencies



```

synth\_static\_atom               → sample\_playback

synth\_two\_osc\_collapsed\_atom    → synth\_static\_atom

synth\_atom\_sequence             → sample\_runtime\_src\_change, core\_sequence\_wait

synth\_paired\_voice\_crossfade    → synth\_atom\_sequence, volume\_slide, voice\_pair\_allocator

synth\_wavetable\_morph           → synth\_paired\_voice\_crossfade, voice\_pair\_allocator

vibrato                         → pitch\_slide or per\_tick\_pitch\_delta

portamento                      → pitch\_slide

tremolo                         → volume\_slide

panbrello                       → pan\_slide

echo\_mid\_song\_param\_changes     → echo\_static\_params

protected\_music\_pair            → voice\_pair\_allocator, voice\_reservation

```



The compiler resolves dependencies automatically and shows the explanation when enabling a feature pulls in others.



\### 5.4 Capability manifest



The driver build emits a manifest read by every other component (instrument editor, sequencer, compiler, preview):



```json

{

&#x20; "driver\_profile": "synth\_static",

&#x20; "driver\_hash": "...",

&#x20; "bytecode\_version": 1,

&#x20; "tick\_rate\_hz": 60,

&#x20; "features": { "sample\_playback": true, "adsr": true, "echo\_enable": true, "...": "..." },

&#x20; "limits": {

&#x20;   "max\_music\_voices": 8,

&#x20;   "reserved\_sfx\_voices": 0,

&#x20;   "max\_sources": 128,

&#x20;   "max\_sources\_note": "profile/tool policy; the ARAM packer enforces actual source-directory footprint",

&#x20;   "max\_dsp\_writes\_per\_tick": 24,

&#x20;   "min\_keyoff\_to\_keyon\_ticks": 1

&#x20; }

}

```



\---



\## 6. Live-compile model



Most edits change data, not driver code. The compiler reassembles the driver only when the feature set changes.



\*\*No driver rebuild:\*\* add/remove sample, edit loop, change ADSR, change notes or clip contents, change pan/volume, change echo delay within enabled echo features, regenerate wavetable frames within enabled morph features.



\*\*Driver rebuild:\*\* enable any handler (vibrato, source-step, paired-crossfade, morph, SFX queue, mid-song echo, dual-voice oscillator, voice reservation/ducking).



Two feedback paths run continuously: a fast live estimate using cached driver-module sizes, and an exact compile report after debounce or manual compile. Driver size always comes from the assembler/linker map.



```

User edit → classify

&#x20; feature set unchanged → reuse driver, rebuild data, repack ARAM

&#x20; feature set changed   → resolve deps, rebuild driver, rebuild data, repack ARAM, refresh manifest, refresh UI

```



Debounce: drag = estimate only; note entry = fast sequence recompile; feature toggle = delayed full rebuild; manual Compile = exact full rebuild.



Build cache keys: driver feature-set hash, assembler version, SPC700 source hash, compiler version, BRR encoder version.



\---



\## 7. UI visibility



The editor cannot expose controls unsupported by the active driver. Three feature classes:



\- \*\*Runtime feature\*\* — requires SPC700 driver handler.

\- \*\*Compiled feature\*\* — costs sequence bytes / S-DSP writes, no driver code.

\- \*\*Pre-rendered feature\*\* — costs BRR/atom bytes, little runtime code.



If a control could be either runtime or compiled, prefer compiled.



\---



\## 8. Instrument model



A voice plays a BRR source with pitch, volume, envelope, and flags. There is no hardware "sample mode" vs "synth mode." The project model is `Track → Instrument → compiled physical voice plan`.



\### 8.1 Sample Pool, Atom Pool, and Source Directory View



\- \*\*Sample Pool\*\* — imported assets (WAV/AIFF/BRR), authored by the user, edited in the sample editor. Each entry tracks source path and SHA-256.

\- \*\*Atom Pool\*\* — compiler-generated artifacts: synth atoms, atom-sequence frames, paired-voice atoms, wavetable frames. Authored via synth/wavetable editors; materialized by the compiler. Shows quality reports, BRR cost, post-decode metrics.

\- \*\*Source Directory View (debug)\*\* — the unified SNES source list as it lives in ARAM, regardless of origin. Exposes hardware truth (addresses, loop points, source indices) but is not the primary authoring surface.



\### 8.2 SampleInstrument



```yaml

type: sample

name: flute

source: flute\_c4.wav         # → resolved via Sample Pool

root\_key: C4

key\_range: \[C3, C6]

loop: true

loop\_start: auto

loop\_end: auto

adsr: { attack: 9, decay: 4, sustain: 5, release: 12 }

pan: center

echo: true

pre\_emphasis: off            # off | gentle | strong

```



\### 8.3 CompiledSynthInstrument — collapsed two-oscillator atom (Level 1)



```yaml

type: synth

name: two\_osc\_soft\_saw

mode: collapsed\_atom

osc\_a: { shape: saw, octave: 0, semitone: 0, fine\_cents: 0, level: 1.0 }

osc\_b: { shape: triangle, octave: 1, semitone: 0, fine\_cents: 0, level: 0.35 }

mixer: { normalize: true, soft\_clip: false }

atom: { allowed\_lengths: \[64, 128, 256], preferred\_length: 128 }

amp: { envelope: adsr, attack\_ms: 80, release\_ms: 600 }

echo: true

pre\_emphasis: gentle

```



\#### Detune and beating constraint



Arbitrary fine detune produces beating with period ≈ `1/Δf`, which generally does not terminate at any reasonable atom length. In collapsed-atom mode the UI does one of: (1) gray out fine detune entirely; (2) snap visibly to ratios that fit the chosen atom length, with a tooltip; (3) escalate the patch to `synth\_dual\_voice\_atom\_pair` (2 voices) with a one-click upgrade. Silent quantization is forbidden.



Higher synth levels are described in §9.



\---



\## 9. Synth path: levels and atom compiler



| Level | Name | Voices | Frame motion | Default frame cap |

|---|---|---|---|---|

| 1 | Static atom | 1 | none | 1 |

| 2 | One-voice atom sequence | 1 | source-step | 32 |

| 3 | Paired-voice crossfade | 2 | smooth | 64 |

| 4 | Wavetable / morph | 2 | many | 64 (warn >96, hard cap 128) |



All levels share the BRR atom compiler and encoder policy (§10).



\### 9.1 Level 1 — Static atom



Two oscillators collapsed to one periodic atom. Allowed shapes: sine, triangle, saw, pulse. Allowed relationships: harmonic octave/semitone ratios, fixed phase. No free-running detune (§8.3).



\*\*Pipeline:\*\*



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



\*\*Atom report:\*\* name; atom length; BRR byte size; root key; recommended pitch range; post-decode waveform error; spectral error; loop click score; HF loss estimate; BRR filter distribution; pre-emphasis applied; ARAM cost.



\### 9.2 Level 2 — One-voice atom sequence



Multiple authored frames played sequentially via runtime source change. Useful for choppy/digital wave-sequence timbres. Compiler emits an atom dictionary plus `SET\_SRC` / `NOTE\_ON` / source-step events. \*\*No true crossfade is possible on a single physical voice\*\* — `XFADE\_SRC` belongs to Level 3. Per-step transition discontinuity appears in the compile report.



Required: `synth\_atom\_sequence`, `sample\_runtime\_src\_change`.



\### 9.3 Level 3 — Paired-voice crossfade



Two voices crossfaded between atoms. Targets \*Aquatic Ambience\*-class smooth pads. Compiler emits per-voice volume slides phased for constant-power crossfade. Echo can mask transitions but is budgeted separately.



Required: `synth\_paired\_voice\_crossfade`, `volume\_slide`, `voice\_pair\_allocator`. Recommended: `protected\_music\_pair`.



\### 9.4 Level 4 — Wavetable / morph editor



Constrained authored wavetable surface.



\*\*In scope:\*\* multi-frame editor; oscillator/additive/formula frame generation; per-frame BRR atom rendering with quality slider; frame pruning (near-duplicate detection); atom clustering (predictor-structure grouping); morph automation lane; quality-vs-ARAM slider; A/B ideal-vs-SNES preview.



\*\*Out of scope:\*\* importing complex audio and auto-decomposing it into a wavetable; real-time morphing on the SPC700.



Required: `synth\_wavetable\_morph`, `synth\_paired\_voice\_crossfade`, `voice\_pair\_allocator`.



\### 9.5 Frame-count caps



Frame caps are guardrails, not hardware constants. The compiler may prune or cluster frames below the requested count to fit ARAM. \*\*Frame count is a budget, not a promise to retain every authored frame.\*\* Pruning decisions appear in the compile report.



\### 9.6 Per-level reports



Every level's compile output includes voice cost, ARAM cost, post-decode quality, and emitted events. No level degrades silently when its budget is exceeded.



\---



\## 10. BRR encoder policy



The encoder is a core compiler component, not an implementation detail.



\### 10.1 Implementation



Rust-native encoder and decoder in the `core` crate. No external binary dependencies for the internal scoring path. Decoder is bit-exact relative to S-DSP BRR decode behavior including filter rounding and clamping. Encoder and decoder share a single fixture set; round-trip determinism is a unit-test gate. External validation against snes\_spc lives in §16.



\#### Scoring modes



The compiler exposes three scoring modes for different concerns. Pre-emphasis, HF-loss, and pitch-transposition checks must use the S-DSP render mode, not raw BRR decode — the audible signal is shaped by 4-point Gaussian interpolation at variable sample rates, not by raw BRR decode alone.



| Mode | Signal path | Used for |

|---|---|---|

| Raw BRR decode | BRR block decode only | loop alignment, click detection, per-block predictor error |

| S-DSP voice render | BRR decode → Gaussian interpolation at the target pitch | HF loss, pre-emphasis validation, pitch-transposition error |

| Full module render | voice render → mixing → echo → output | sequencing, voice interactions, peak/clipping estimates |



\### 10.2 Search strategy



Per 9-byte BRR block: exhaustive search over filters 0–3 and shift/range; pick the (filter, shift) pair minimizing per-block error subject to clamping.



\#### Loop-state handling



Filters 1–3 reference previous samples, so a looped atom's quality depends on predictor state across the loop boundary, not just within a single decode pass. The encoder evaluates the decoded loop over \*\*repeated iterations\*\*, and loop scoring includes predictor history continuity from one iteration into the next.



Optional strategies the encoder may apply, especially for tiny atoms where loop-boundary error dominates:



\- phase rotation of the source cycle (see §10.3);

\- a loop warm-up block before the loop point;

\- duplicating the leading block at the tail to seed predictor state;

\- rejecting filter selections that produce unstable transients across iterations even when single-pass error looks low.



\### 10.3 Phase rotation for synth atoms



Try N phase rotations of the source cycle (e.g. N=16 or 32). BRR-encode each. Score after BRR decode. Pick the rotation with lowest combined waveform-discontinuity + spectral-error + loop-click score. Cheapest rotation pre-encode is often not cheapest post-decode.



\### 10.4 Post-decode scoring



All loop quality, click, spectral, and waveform scoring is measured on the decoded BRR result. Reports include per-block BRR error, selected filter distribution, post-decode loop-click score, FFT-based spectral error weighted by perceptual band, HF-loss estimate, pre-emphasis parameters.



\### 10.5 Pre-emphasis



Pre-emphasis compensates for S-DSP gaussian-interpolation HF dulling. It is part of the compiler, not the runtime. Three presets:



\- \*\*Off\*\* — no pre-emphasis. Default for imported samples.

\- \*\*Gentle\*\* — modest high-shelf compensation (≈ +1.5 dB at 5 kHz). Allowed default for generated synth atoms only when the A/B preview makes the choice explicit.

\- \*\*Strong\*\* — more aggressive compensation for very dull material.



Per-sample and per-atom selection. Applied before BRR encode. \*\*Pre-emphasis is validated using S-DSP voice render mode\*\* (§10.1), not raw BRR decode — the dulling it compensates for is interpolation behavior, not decode behavior. Preview plays the post-render result, not the ideal pre-emphasized waveform. Every pre-emphasis choice appears in the compile report.



\---



\## 11. Voice allocation and SFX



\### 11.1 Voice groups



Some instruments need a voice group, not a single voice: `synth\_dual\_voice\_atom\_pair` (2), `synth\_paired\_voice\_crossfade` (2 for the phrase), `synth\_wavetable\_morph` (2 for the morph).



\### 11.2 SFX policies



\- `no\_sfx` — all 8 voices to music; no game integration.

\- `reserved\_sfx` — N voices reserved; music compiles against `8 - N`.

\- `interruptible\_pair` — music pairs may be stolen by SFX; warns at compile time.

\- `protected\_pair` — music pairs cannot be stolen.



\### 11.3 Allocator rules



The voice-pair allocator: refuses to allocate a pair across an SFX-reserved boundary; refuses to schedule a paired instrument with fewer than 2 contiguous-eligible voices; rejects `interruptible\_pair` on `protected\_music\_pair` instruments; emits hard errors on unsatisfiable allocations; warns when music sustain reaches `max\_music\_voices - 1`.



\### 11.4 Track voice policy



`compiler\_assigned`, `fixed\_voice\_N`, `fixed\_voice\_pair\_N\_M`, `reserved\_for\_sfx`. A paired instrument on a `fixed\_voice\_N` track is a hard error.



\---



\## 12. Sample editor and auto-loop



Sample mode accepts WAV/AIFF/BRR.



Auto-loop generates \*\*ranked candidates\*\*. It does not claim a perfect loop. The user picks among A/B/C with audible preview.



```

load → mono → optional normalize → estimate root pitch

→ select candidate windows → snap to 16-sample boundaries

→ optional resample for alignment → BRR encode → BRR decode

→ score click + spectral + waveform + decode error → rank → audition

```



Loop editor UI: waveform display; loop start/end handles; 16-sample grid; candidate list; BRR-decoded preview; raw preview; click-risk meter; BRR byte cost; root pitch estimate.



\---



\## 13. Sequencer



Reaper-like track-and-item workflow, SNES-constrained.



\### 13.1 Internal timing model



The sequencer represents musical positions as \*\*bars/beats/ticks\*\*, not floating-point seconds. The compiler converts musical positions to driver ticks at compile time. The UI may visually behave like Reaper's free clip placement, but the schema stores quantized musical positions; free placement is UI sugar that snaps to the internal grid.



\### 13.2 Track model



Per-track: name; instrument; voice policy; volume; pan; echo send; mute/solo; clips; automation lanes (volume, pan, pitch, morph position).



\### 13.3 Compile diagnostics panel



Always visible: ARAM free; voice count; driver profile; enabled feature count; sequence byte cost; sample byte cost; synth atom byte cost; \*\*echo buffer cost\*\* (prominent); max simultaneous voices; max S-DSP writes per tick; errors and warnings.



\---



\## 14. Sequence bytecode



Boring and deterministic. Each command's cost is known in 60 Hz ticks. The compiler does difficult work offline.



\*\*Core:\*\*



```

WAIT ticks

NOTE\_ON voice, source, pitch\_index, volume\_l, volume\_r, adsr\_or\_gain

NOTE\_OFF voice

SET\_SRC voice, source

SET\_PITCH voice, pitch\_value

SET\_VOL voice, left, right

SET\_PAN voice, pan

SET\_ADSR voice, adsr1, adsr2

SET\_GAIN voice, gain

SET\_ECHO\_MASK mask

LOOP\_BEGIN count

LOOP\_END

END

```



\*\*Optional (gated by feature flag):\*\*



```

VOL\_SLIDE voice, target\_l, target\_r, ticks

PAN\_SLIDE voice, target\_pan, ticks

PITCH\_SLIDE voice, target\_pitch, ticks

PORTAMENTO voice, target\_pitch, velocity, ticks

SET\_NOISE voice, noise\_freq

SET\_PITCH\_MOD mask

ECHO\_PARAM\_SET reg, value          ; only with echo\_mid\_song\_param\_changes

XFADE\_SRC voice\_a, voice\_b, src\_next, ticks

MORPH\_STEP pair\_id, frame\_index, ticks   ; compiler IR, see §14.1

```



The compiler rejects bytecode commands unsupported by the active driver feature set.



\### 14.1 MORPH\_STEP lowering



`MORPH\_STEP` is a compiler-level IR command. By default it lowers into existing primitives — `SET\_SRC` + `VOL\_SLIDE` + `WAIT` against the assigned voice pair — keeping the runtime small. A dedicated driver-side morph handler is only emitted when a feature flag explicitly opts in (e.g., a future `morph\_runtime\_handler`); without that flag, no `MORPH\_STEP` opcode appears in the final driver bytecode.



This preserves the boring-driver principle: high-level musical intent stays in the compiler, the driver dispatches primitives.



\### 14.2 Bytecode ABI



The bytecode itself is versioned independently of the project file.



```

bytecode\_version          monotonic integer; bumped on any breaking change

endianness                little-endian (matches SPC700)

command encoding          variable-length; opcode byte followed by 0..N operand bytes

max\_command\_length        16 bytes (driver-enforced; overlong commands are hard errors)

invalid\_command\_behavior  driver halts the sequence and sets an error flag readable

&#x20;                         by the host tool via the SPC export; never silent

```



\*\*Compatibility rule:\*\* every compiled module pairs a `bytecode\_version` with a `driver\_hash`. The driver loader validates that `bytecode\_version` is within its supported range; mismatch is a hard error at upload time, not at runtime. The capability manifest (§5.4) carries `bytecode\_version` alongside `driver\_hash`.



When `bytecode\_version` is bumped, older modules either continue to work (if the new driver maintains backward compatibility) or require a recompile from the source project (§16). Modules are not migrated; projects are.



\---



\## 15. ARAM packing



\### 15.1 Standard layout



```

0000–????  SPC700 driver code

????–????  zero page / stack / runtime state

????–????  source directory

????–????  pitch tables

????–????  sequence bytecode

????–????  instrument metadata

????–????  BRR sample pool

????–????  generated synth atom pool

????–FFFF  echo buffer (top of ARAM)

```



The packer prevents overlap and emits a hard error on collision.



\### 15.2 ARAM report



```json

{

&#x20; "total\_aram": 65536,

&#x20; "driver\_code": 6144,

&#x20; "runtime\_state": 512,

&#x20; "source\_directory": 384,

&#x20; "pitch\_tables": 768,

&#x20; "sequence\_data": 2401,

&#x20; "sample\_brr\_pool": 16820,

&#x20; "synth\_atom\_pool": 1242,

&#x20; "echo\_buffer": 8192,

&#x20; "free": 29073

}

```



\### 15.3 Echo



Echo memory cost is `2 KB × EDL`, EDL 0–15.



| EDL | Echo bytes |

|---|---|

| 0 | 0 |

| 4 | 8 KB |

| 8 | 16 KB |

| 12 | 24 KB |

| 15 | 30 KB (\~46% of ARAM) |



Echo parameters (delay, feedback, FIR, volumes) are project-global and static. Per-voice echo enable/mask is allowed at any time. Mid-song parameter changes require `echo\_mid\_song\_param\_changes` and are out of default scope.



UI: ARAM meter shows echo as a labeled region (not generic "used"); echo cost displayed numerically next to the EDL slider in bytes and percent; default presets favor modest EDL (≈4); EDL ≥ 10 requires explicit opt-in with a confirmation noting it halves available sample ARAM. Echo controls live on a project-level panel.



\#### Echo safety



Echo can self-oscillate or clip unpleasantly, especially through headphones during authoring. The tool:



\- warns on high feedback values;

\- warns on FIR/feedback combinations likely to clip or self-oscillate;

\- starts preview at reduced output gain after any echo-setting change, restoring full gain on the next user action;

\- includes peak-level and clipping estimates in the compile report when full-module render (§10.1) is available.



\### 15.4 Budget policy



\*\*Hard errors:\*\* ARAM overflow; >8 simultaneous voices; unsupported bytecode; source directory overflow; BRR loop misalignment; echo buffer overlap; voice-pair allocation conflict.



\*\*Warnings:\*\* <2 KB free; S-DSP writes/tick near limit; one synth patch >2 voices; echo >16 KB; atom degraded post-decode; loop click risk high; sustain reaches `max\_music\_voices - 1`.



\---



\## 16. Project file format



The project file is the source of truth. Compiled artifacts are derived.



JSON, UTF-8, stable key ordering for Git diffs. Explicit top-level `schema\_version`. Source data only — compiled BRR, atoms, and ARAM images live in a `build/` cache keyed by content hashes. Source audio referenced by path with recorded SHA-256.



```json

{

&#x20; "schema\_version": 1,

&#x20; "project": {

&#x20;   "name": "example",

&#x20;   "tick\_rate\_hz": 60,

&#x20;   "tempo\_bpm": 120,

&#x20;   "time\_signature": \[4, 4]

&#x20; },

&#x20; "driver": {

&#x20;   "profile": "synth\_static",

&#x20;   "features": { "...": "..." }

&#x20; },

&#x20; "instruments": \[

&#x20;   { "type": "sample", "name": "flute", "...": "..." },

&#x20;   { "type": "synth",  "name": "soft\_saw", "...": "..." }

&#x20; ],

&#x20; "tracks": \[

&#x20;   {

&#x20;     "name": "lead",

&#x20;     "instrument": "soft\_saw",

&#x20;     "clips": \[

&#x20;       { "start\_bar": 1, "start\_beat": 1, "start\_tick": 0, "length\_beats": 4, "...": "..." }

&#x20;     ]

&#x20;   }

&#x20; ],

&#x20; "echo": {

&#x20;   "scope": "project\_global\_static",

&#x20;   "edl": 4,

&#x20;   "params": { "...": "..." }

&#x20; },

&#x20; "compiler": {

&#x20;   "atom\_quality\_threshold": 0.92,

&#x20;   "pre\_emphasis\_default\_sample": "off",

&#x20;   "pre\_emphasis\_default\_synth": "gentle",

&#x20;   "frame\_caps": { "level2": 32, "level3": 64, "level4": 64, "expert\_override": false }

&#x20; },

&#x20; "asset\_refs": \[

&#x20;   { "path": "samples/flute\_c4.wav", "sha256": "..." }

&#x20; ]

}

```



\*\*Migration:\*\* higher `schema\_version` than the tool supports → refuse with clear error; lower → run an explicit named migration (`migrate\_v1\_to\_v2`, etc.); no migration → refuse. Migrations are forward-only, may drop or refactor fields, and log every change in a migration report shown to the user.



\*\*Acceptance:\*\* older schema fails safely or migrates explicitly, never silently. Compiled artifacts regenerate byte-identically from `project.json` plus referenced assets given the same compiler/encoder/assembler versions. Project files diff cleanly: stable key order, no embedded binary, no embedded compile timestamps.



\---



\## 17. Preview and emulator strategy



The compiler needs bit-exact BRR decode and S-DSP-relevant scoring behavior. It does not need a full SNES emulator embedded.



\*\*Layer 1 — Internal renderer.\*\* Rust-native BRR block decoder and a simple WAV render path. Drives atom scoring, A/B previews, and unit tests. This is what the atom compiler scores against.



\*\*Layer 2 — Oracle validation.\*\* The internal decoder is validated against blargg's `snes\_spc` (LGPL; accurate S-DSP path passes hardware validation tests). Mismatches between the internal renderer and the oracle are reported, not silently ignored. This layer also renders generated `.spc` files via snes\_spc as a full-APU validation.



\*\*Layer 3 — External full-system validation.\*\* ares (ISC, accuracy-focused) is the recommended external comparison target for full-ROM validation. Ares is not embedded — the compiler emits standard `.spc` and `.sfc` outputs; comparison harnesses live outside the host tool.



Three concerns are kept separate: compiler scoring (internal Rust path, three modes per §10.1), preview playback (internal path with optional oracle), and validation (oracle and ares).



\---



\## 18. Calibration harness



Atom quality thresholds — waveform error, spectral error, loop click score, HF loss — are not desk-solvable. The spec defines where they live and how they are tested.



Thresholds live in `compiler` settings in the project file. Defaults are conservative; warnings fire early. As listening tests accumulate, thresholds are tuned and recorded per project.



The harness compares three signals side by side with numeric metrics and audible playback:



1\. \*\*Ideal\*\* — pre-encode target.

2\. \*\*Internal\*\* — Rust BRR-decoded result.

3\. \*\*Oracle\*\* — snes\_spc BRR-decoded result.



M0 ships harness scaffolding; M3 ships a working harness that produces stable, reproducible measurements across the fixture set. The thresholds themselves are not locked at M3 — only the measurement infrastructure.



\---



\## 19. SPC and ROM export



\*\*`.spc` export\*\* — 64 KB ARAM image, S-DSP register state, SPC700 register state, metadata tags. Driver profile is baked in.



\*\*`.sfc` test ROM\*\* — initializes SNES, uploads driver/module to the APU, starts playback, optionally displays ARAM/voice/debug status.



\*\*Game module export\*\* — `driver.bin`, `module.bin`, `module\_map.json`, `audio\_symbols.inc`, `loader\_stub\_65816.asm`. Larger ROMs may store many driver/module combinations; the active module is still bound by the 64 KB ARAM limit.



\### 19.1 Debug build outputs



Every compile additionally writes human-readable debug artifacts to `build/debug/`. These are essential for diagnosing M0–M3 issues and for letting the host tool (or a coding agent) reason about why something sounds wrong without guessing.



```

build/debug/dsp\_writes.csv             tick-by-tick S-DSP register write trace

build/debug/voice\_timeline.csv         per-voice note-on/off, source, pitch, slides

build/debug/aram\_map.html              labeled ARAM layout with hover ranges

build/debug/source\_directory.csv       final unified source list (Sample Pool ∪ Atom Pool)

build/debug/bytecode\_disassembly.txt   sequence bytecode, decoded, with tick offsets

build/debug/atom\_quality\_report.html   per-atom scoring across all three modes (§10.1)

```



Debug output generation is fast and on by default; it can be disabled in `compiler` settings for batch builds.



\---



\## 20. Driver build strategy



Build complete driver images per feature set. No runtime overlays, compressed loaders, self-modifying bundles, or in-ARAM dynamic linking. Driver code is small enough to duplicate in ROM; ARAM is the binding constraint.



Later optimization candidates (not in scope now): shared kernel + optional handler blocks; compressed ROM driver storage; overlay loader for rare commands.



\---



\## 21. Milestones



\### M0 — Research harness



\*\*Deliverables:\*\* Rust BRR encoder/decoder; SPC700 hello-sample driver; minimal `.spc` and `.sfc` exporters; exact ARAM map report; oracle bridge to snes\_spc; calibration harness scaffolding.



\*\*Acceptance:\*\*



\- Known WAV fixture encodes and decodes deterministically; byte-identical across runs.

\- Looped BRR atom round-trips through the decoder with legal loop alignment.

\- Encoder exposes per-block error, filter selection, shift, post-decode loop-click score.

\- Internal Rust decoder matches oracle fixtures within agreed tolerance.

\- A generated `.spc` renders successfully through snes\_spc.

\- One looped BRR sample plays in an SPC player and the test ROM.



M0 is complete only when internal decode, oracle render, ARAM layout, loop alignment, and SPC export all agree on deterministic fixtures. A BRR file playing is necessary but not sufficient.



\### M1 — Sample mode



\*\*Deliverables:\*\* sample slot UI; WAV/AIFF/BRR import; root key; ADSR/GAIN; pan; echo enable; loop candidate finder; BRR preview; ARAM meter with prominent echo cost; project file v1 with migration scaffolding; Sample Pool view.



\*\*Acceptance:\*\* create a C700-lite sample instrument; export `.spc` and `.sfc`; loop points snapped to BRR boundaries; compiler refuses ARAM overflow; project file round-trips; internal preview and oracle preview agree.



\### M1.5 — Pattern sequencer harness



\*\*Deliverables:\*\* primitive sequencing surface — pattern grid using bars/beats/ticks; note on/off; fixed voice assignment; instrument assignment.



\*\*Purpose:\*\* test instruments musically; exercise the capability manifest in real sequence compilation; surface voice overflow and sequence byte cost early.



\*\*Acceptance:\*\* multi-note pattern compiles to deterministic bytecode; voice overflow → hard error; pattern exports as `.spc` and previews correctly.



\### M2 — Driver capability system



\*\*Deliverables:\*\* granular feature flags; profile presets; dependency resolver; capability manifest; UI show/hide; assembler cache.



\*\*Acceptance:\*\* vibrato toggle pulls/removes correct handler code and UI; sample edits do not rebuild driver; paired-crossfade enable does rebuild driver.



\### M3 — Static synth atom mode (Level 1)



\*\*Deliverables:\*\* two-oscillator patch editor with detune-constraint UI; collapsed periodic atom renderer; BRR atom encoder integration; atom quality report; pre-emphasis presets; calibration harness functional.



\*\*Acceptance:\*\* two-oscillator patch compiles to a looped BRR source; generated atom plays through the sample playback path; compiler reports atom size, post-decode error, pre-emphasis selection; detune UI matches §8.3 (no silent quantization); harness produces stable measurements across the fixture set.



\### M4 — Reaper-like sequencer



\*\*Deliverables:\*\* track list; timeline items (free-look UI snapping to bars/beats/ticks); piano roll; automation lanes; voice assignment; compile diagnostics panel.



\*\*Acceptance:\*\* compose an 8-voice valid module; voice overflow rejected; voice-pair conflicts rejected; deterministic bytecode.



\### M5 — Synth event mode



\*\*Deliverables:\*\* compiled pitch LFO; compiled tremolo; volume slides; pitch slides.



\*\*Acceptance:\*\* time-varying events emitted as bytecode without runtime oscillator code; S-DSP-write-rate warnings.



\### M6 — One-voice atom sequence (Level 2)



\*\*Deliverables:\*\* atom dictionary editor; source-step bytecode; per-step transition click reporting; default cap 32 frames.



\*\*Acceptance:\*\* wave-sequence patch compiles; per-step discontinuity reported.



\### M7 — Paired-voice crossfade pads (Level 3)



\*\*Deliverables:\*\* voice-pair allocator; paired source crossfade command; atom sequence editor; pad compile report; A/B ideal-vs-SNES preview; SFX coexistence policies; default cap 64.



\*\*Acceptance:\*\* smooth pad on two voices; voice and ARAM cost reported; echo, crossfade, and frame timing budgeted; illegal SFX/pair conflicts rejected.



\### M8 — Wavetable / morph editor (Level 4)



\*\*Deliverables:\*\* multi-frame wavetable editor; oscillator/additive/formula frame generation; morph automation lane; frame pruning; atom clustering; quality/size slider; A/B ideal-vs-SNES preview; default cap 64, warn >96, hard cap 128.



\*\*Acceptance:\*\* authored wavetable patch compiles to optimized BRR atom dictionary plus morph events; quality/ARAM trade-off works; no audio decomposition required.



\---



\## 22. Testing



\*\*Unit:\*\* BRR encode/decode round trips; loop alignment; feature dependency resolver; ARAM packer overlap detection; echo buffer placement; voice-pair allocator (with SFX policies); sequence bytecode encoding; post-decode atom scoring; project schema migration; bars/beats/ticks ↔ driver-tick conversion.



\*\*Golden:\*\* known sample → known BRR; known feature set → known driver size; known project → known ARAM map; known synth patch → stable atom report; known wavetable → stable atom dictionary.



\*\*Oracle:\*\* internal Rust render vs. snes\_spc render across the fixture set; mismatches reported.



\*\*Emulator/audio:\*\* render `.spc` through snes\_spc and (later) ares; compare expected duration; detect stuck note, silence, severe clipping; check max voice count; check S-DSP write trace.



\*\*Hardware (later):\*\* flashcart playback; real SNES capture; emulator-vs-hardware comparison; headphone-safe echo feedback tests.



\---



\## 23. Open questions



These are empirical and resolved through use, not design:



1\. Final atom quality thresholds — to be tuned via the calibration harness through listening tests.

2\. Whether to embed snes\_spc directly for live preview or keep it as a validation-only oracle — decided after M0.

3\. Practical wavetable frame caps — the 32 / 64 / 96 / 128 numbers are provisional; empirical testing may move them.

4\. Whether a later version adds a free pre-emphasis EQ editor.

