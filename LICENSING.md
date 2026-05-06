# Licensing

This repository uses a tri-license model. The boundaries are deliberate;
do not blur them.

## 1. Host application source code (this repository)

**Apache License 2.0.** See `LICENSE` for the full text.

Covers all Rust source, build scripts, documentation, and tooling in this
repository. Contributions are accepted under the same license.

## 2. Generated outputs (produced by running this tool)

**0BSD (BSD Zero Clause License).**

Applies to all artifacts the compiler emits when a user compiles their
own project: `.spc` previews, `.sfc` test ROMs, driver object code,
`module.bin` blobs, `audio_symbols.inc`, `loader_stub_65816.asm`, and
any other compile-time output.

- No attribution is required.
- Free for commercial and noncommercial game use.
- The 0BSD scope **does not** cover user-supplied audio assets or
  user-authored project data — those remain under the user's own rights.

```
Copyright (c) the project authors.

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

## 3. snes_spc (used as oracle / calibration only)

**LGPL-2.1-or-later** — upstream license, not changeable.

snes_spc is integrated as an out-of-process oracle for validation and
calibration (SPEC §17, §18). It is **never** linked into, embedded in, or
distributed as part of generated game outputs. The integration boundary is
either a separate helper executable or a clean dynamic-link surface so
LGPL obligations stay scoped to the oracle component and do not propagate
to the host app or to user game code.

If a future change tries to embed snes_spc into the compiler binary or
into a `.spc` / `.sfc` / module export, that change is rejected on
licensing grounds, not engineering grounds.
