//! Core compiler library for the SFC Wave Compiler.
//!
//! Future home of: BRR encoder/decoder, atom compiler, sequence compiler,
//! ARAM packer, voice-pair allocator, capability manifest types, oracle
//! bridge interface. See `SPEC.md` at the repository root.

pub mod aram;
pub mod asm;
pub mod brr;
pub mod brr_fixtures;
pub mod manifest;
pub mod report;
pub mod spc;
pub mod tools;
