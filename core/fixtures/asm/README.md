# asm fixtures

`m0_smoke.asm` — trivial NOP + BRA loop. Exists to prove the asar
invocation pipeline produces a deterministic 64 KB ARAM image with
our bytes at the address we asked for.

This is **not** a real driver. M0.4+ replaces it with a real audio
driver. Until then, anyone touching `core::asm` or `sfcwc
assemble-smoke` should expect this fixture's output sha256 to stay
stable across runs; if it changes, the asar invocation or the source
moved.

Expected sentinel: file bytes `00 2F FD` at offset `0x0200`; every
other byte zero. Locked by an integration test in
`app/tests/cli.rs::assemble_smoke_when_asar_resolved`.
