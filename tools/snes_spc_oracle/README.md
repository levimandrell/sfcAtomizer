# snes_spc_oracle

A thin SPC render wrapper for the SFC Wave Compiler M0.5 calibration
boundary. Loads an SPC file via Blargg's `snes_spc 0.9.0`, renders N
stereo frames of 16-bit interleaved PCM at 32 kHz, and writes a JSON
report alongside the raw PCM bytes.

This binary exists exclusively as the host's process-boundary
calibration oracle (per SPEC §17.1 and `LICENSING.md` §3). It is
invoked by `sfcwc calibrate-oracle`. It is **never** linked into the
Rust host or distributed inside any compiler output.

## License

`snes_spc` is **LGPL-2.1-or-later**. Source is preserved verbatim
under `vendor/snes_spc/`; see `vendor/snes_spc/license.txt` (Blargg's
original) and `vendor/snes_spc/README-vendoring.md` for the pinned
upstream commit and provenance.

The wrapper binary statically links the LGPL `snes_spc` sources, so
the binary itself is distributed under LGPL-2.1+ accordingly. Anyone
redistributing the wrapper binary must either:

1. ship the `vendor/snes_spc/` source tree alongside it, or
2. provide written assurance that the source can be obtained on
   request, *and*
3. preserve relinking capability — i.e. make it possible for a
   downstream user to substitute a modified `snes_spc` and rebuild.

The minimal `CMakeLists.txt` in this directory plus the unmodified
vendored sources satisfy (1) and (3). The Apache-2.0 host
application in this repository does not link against `snes_spc`; the
process boundary is the LGPL containment.

## Build

Requires CMake 3.16+ and any C++17 compiler (MSVC, clang, gcc).

```sh
cmake -S tools/snes_spc_oracle -B tools/snes_spc_oracle/build
cmake --build tools/snes_spc_oracle/build --config Release
```

Output binary lands at:

- Windows / MSVC:  `tools/snes_spc_oracle/build/Release/snes_spc_oracle.exe`
- Single-config:    `tools/snes_spc_oracle/build/snes_spc_oracle`

`tools/snes_spc_oracle/build/` is ignored by git.

## CLI contract (v1)

Locked. Do **not** extend without a PM brief.

```
snes_spc_oracle --version
snes_spc_oracle render --input-spc <path>
                       --frames     <N>
                       --output-pcm <path>
                       --report     <path>
```

`--version` prints one line:

```
snes_spc_oracle <wrapper-version> (snes_spc <upstream-commit-pin>)
```

`render`:

1. Reads the input `.spc` file.
2. Loads it via `spc_load_spc` and calls `spc_clear_echo` for
   determinism (per `spc.h` recommendation: "Useful after loading an
   SPC as many have garbage in echo").
3. Renders `frames * 2` 16-bit samples (one stereo pair per frame)
   via `spc_play`.
4. Writes the PCM to `--output-pcm` as raw `frames * 4` bytes —
   no WAV header, no metadata.
5. Writes the JSON report to `--report` with these fields:

   ```json
   {
     "schema_version": 1,
     "report_type": "snes_spc_oracle_render",
     "status": "ok",
     "wrapper_version": "0.1.0",
     "snes_spc_pin": "<upstream commit hash>",
     "input_spc_path": "<absolute path>",
     "input_spc_sha256": "<hex>",
     "frames_rendered": 2048,
     "sample_rate_hz": 32000,
     "channels": 2,
     "bytes_per_sample": 2,
     "output_pcm_path": "<absolute path>",
     "output_pcm_sha256": "<hex>",
     "output_pcm_max_abs": 0,
     "output_pcm_rms": 0.0
   }
   ```

   On failure, `status: "error"` plus an `error: "<msg>"` field.

Determinism: identical input + identical wrapper build ⇒ identical
PCM bytes ⇒ identical `output_pcm_sha256`. The Rust caller uses this
property to detect drift in the oracle render across compiler
versions.

## Vendored snes_spc

- Upstream: <https://github.com/blarggs-audio-libraries/snes_spc>
- Pinned commit: `ec8ee2bbe30451614c1d02a83f7af1c97d497d45` (2020-10-24)
- Vendored: 2026-05-06
- No upstream files modified.

See `vendor/snes_spc/README-vendoring.md` for full provenance.
