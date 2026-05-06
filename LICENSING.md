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

## 4. Other host dependencies

Each crate the host links must be license-compatible with Apache-2.0,
or — when copyleft — used in a way that does not propagate the
copyleft to host source code.

### 4.1 egui / eframe (M1.1+)

**Dual-licensed MIT OR Apache-2.0.** Compatible with the host's
Apache-2.0 licensing. Used as a normal crate dependency; no
modifications to upstream source. No additional obligations beyond
preserving copyright notices on redistribution.

### 4.2 symphonia (M1.1 declared, M1.2 used)

**MPL-2.0**, file-scope copyleft. Only direct modifications to
`symphonia` source files would inherit MPL — the licence does not
propagate to code that merely uses `symphonia` as a dependency. We
use it as a normal crate dependency without modification, so:

- The host application source remains under Apache-2.0.
- Generated outputs (`.spc`, `.sfc`, module blobs) remain under 0BSD.
- Anyone redistributing the host binary must comply with MPL-2.0
  notice obligations for `symphonia` itself (which Cargo handles
  implicitly via the standard licence-attribution flow).

If a future change ships a fork or local patch to `symphonia` source,
that fork's modified files become MPL-licensed and must be
distributed under MPL — flag for review before merging.
