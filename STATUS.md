# SFC Wave Compiler — Status

## Current milestone

**M0 — Research harness** — not started. Pass 1 (this pass) covered repo
hygiene and spec truth only; no Rust logic yet.

## Last pass

**Pass 1 — Repo hygiene + spec truth.**

- Phase A: SPEC.md unescaped (markdown backslash-escapes and `&#x20;` HTML
  entities stripped; tables/code fences/lists tightened).
- Phase B: 14 consultant-review patches applied (M0 readiness).
- Phase C: Cargo workspace skeleton created (`core/`, `app/`); STATUS,
  CLAUDE, LICENSE, LICENSING docs added; `.gitignore` covering Rust,
  Windows, editors, `build/`, `.claude/`.
- Phase D: `cargo check` and `cargo fmt --check` clean; pushed to `main`.

## Decisions log

- **License model:** Apache-2.0 for the host application source; 0BSD for
  generated outputs (`.spc`, `.sfc`, driver/module blobs); snes_spc kept
  out-of-process to avoid LGPL propagation. See `LICENSING.md`.
- **Assembler:** asar primary for M0–M2, behind an `AssemblerBackend`
  interface so WLA-DX can be added later without rewiring the compiler
  (SPEC §4).
- **`max_sources`:** 128 (SPEC §5.4); ARAM packer enforces actual
  source-directory footprint.
- **M0 acceptance split** (resolved this pass per consultant Patch 5): raw
  BRR decode is bit-identical at M0; voice-render and full-module-render
  tolerances are provisional at M0 and frozen at M1 (SPEC §10.1, §21, §23).

## Open questions remaining

Cross-reference SPEC §23. Of the four questions there:

1. Final atom quality thresholds — empirical, deferred to calibration runs.
2. Embed snes_spc for live preview vs keep oracle-only — decision target M0.
3. Practical wavetable frame caps — empirical, may move from 32/64/96/128.
4. Whether a future version adds a free pre-emphasis EQ editor.

## Next pass

**M0 implementation** — TBD by PM. Likely starts with Rust BRR
encoder/raw-decoder and asar-backed hello-sample driver.
