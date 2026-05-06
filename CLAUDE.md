# SFC Wave Compiler — Engineer notes

Standalone Rust app that compiles SNES audio (samples + compile-time synth
atoms + wavetables) into a complete 64 KB ARAM image — driver, source
directory, sequence bytecode, BRR pools, echo — exporting `.spc` previews,
`.sfc` test ROMs, and game-ready module blobs.

## Spec

`SPEC.md` at the repo root is the source of truth. Read it before changing
behavior. Patches go in via PM brief, not freelance edits.

## Current milestone

See `STATUS.md`. Currently: **M0 — Research harness**, not yet started.

## Build commands

```
cargo check          # compile the whole workspace
cargo fmt            # format
cargo fmt --check    # CI-friendly: fail if unformatted
cargo test           # run all tests across the workspace
cargo build --release
```

The workspace has two member crates:

- `core/` — library crate. Future home of BRR encoder/decoder, atom
  compiler, sequence compiler, ARAM packer, voice-pair allocator,
  capability manifest types.
- `app/` — binary crate. Future home of GUI/CLI host.

Adding new dependencies is a PM-approved decision per pass. Don't pull
crates in unprompted.

## Commit convention

`type(scope): subject`

Types: `feat`, `fix`, `docs`, `chore`, `test`, `refactor`. Subject in
imperative, lowercase, no trailing period. Body explains the *why* when
non-obvious.

Examples (real commits in this repo):

- `chore(spec): unescape markdown and normalize whitespace`
- `docs(spec): apply consultant review patches (M0 readiness)`
- `chore: bootstrap cargo workspace, status doc, license`

## Forbidden

Driver/runtime features that are explicitly out of scope (SPEC §2 and §5.2
forbidden flags). Do not add code that would enable any of these:

- `runtime_sample_streaming`
- `runtime_code_overlay_loader`
- `runtime_dynamic_linker`
- `arbitrary_wav_resynthesis`
- `runtime_oscillator_engine`
- Audio-to-wavetable decomposition from complex inputs.
- VST/AU plugin packaging.
- SNES audio middleware stack ambitions.
- Driver hot-swap during playback (`async_loader_api` is whole-module
  between songs only — see SPEC §5.2 narrowing).
- Tracker / Serum clones, MOD/XM/IT/S3M effect import.
- Dynamic linker for SPC700 code sections.
- Free-form EQ pre-emphasis editor (presets only; SPEC §10.5).
- Real-time subtractive/wavetable synthesis on SPC700 — anything that can
  be rendered offline to BRR atoms or bytecode events MUST be (SPEC §0
  guiding principle).

## Hard constraints to never forget

- 64 KB ARAM total. Echo eats `2 KB × EDL`.
- 8 S-DSP voices.
- BRR is 9 bytes per 16 decoded samples; loop alignment is mandatory.
- 60 Hz tick rate.
- First 512 bytes of ARAM are CPU/runtime fixed (§15.1).
- The compiler — not the assembler — owns ARAM layout.

## Authorship

This repo pushes under the `levimandrell` GitHub account via the
`github-alt` SSH host alias. Don't rewrite the remote URL.
