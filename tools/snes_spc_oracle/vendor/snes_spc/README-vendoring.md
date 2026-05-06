# snes_spc — vendored snapshot

This directory holds an unmodified snapshot of Blargg's `snes_spc`
library, used by `tools/snes_spc_oracle/main.cpp` as the reference
S-DSP / SPC700 emulator for M0.5+ calibration work.

## Source

- **Upstream**: <https://github.com/blarggs-audio-libraries/snes_spc>
- **Pinned commit**: `ec8ee2bbe30451614c1d02a83f7af1c97d497d45`
- **Pinned-commit date**: 2020-10-24
- **Vendored**: 2026-05-06

The pinned commit is the only commit on `main` at vendor time and is
itself a verbatim import of Blargg's `snes_spc 0.9.0` release.

## Why this fork

`blarggs-audio-libraries` is the community-maintained mirror that
publishes Blargg's audio libraries verbatim, including the original
`license.txt` and `readme.txt`. Other available mirrors
(`elizagamedev/snes_spc`, `jprjr/snes_spc`, `yupferris/snes_spc`)
either match this content or carry localized changes (e.g. removed
fast DSP, build-script additions). For oracle work we want the most
neutral, unmodified mirror; this one is it.

## Modifications

**None.** Every file in this directory was copied verbatim from
`<upstream>/snes_spc/` plus the top-level `license.txt` and
`LICENSE` for license preservation. If a future change requires
altering a vendored file, do **not** edit it in place — instead,
add a sibling `patches/` directory and apply patches at build time
so the upstream baseline stays inspectable.

## License

LGPL-2.1-or-later. See `license.txt` (Blargg's original text) and
`LICENSE` (the same license re-stated). The wrapper binary at
`tools/snes_spc_oracle/main.cpp` links these sources and is
distributed under LGPL-2.1+ accordingly. See
`tools/snes_spc_oracle/README.md` for the full LGPL compliance
note (separate process boundary, source preservation, relinking).

The Apache-2.0 host application in this repository **does not** link
against snes_spc. The wrapper is invoked as a standalone executable
via process boundary only (per SPEC §17.1 and `LICENSING.md` §3).
