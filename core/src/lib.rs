//! Core compiler library for the SFC Wave Compiler.
//!
//! Future home of: BRR encoder/decoder, atom compiler, sequence compiler,
//! ARAM packer, voice-pair allocator, capability manifest types, oracle
//! bridge interface. See `SPEC.md` at the repository root.

pub mod aram;
pub mod asm;
pub mod atom;
pub mod audio;
pub mod audition;
pub mod brr;
pub mod brr_encoder;
pub mod brr_fixtures;
pub mod bytecode;
pub mod driver_build;
pub mod driver_proto;
pub mod echo_validation;
pub mod import;
pub mod loop_finder;
pub mod manifest;
pub mod module_image;
pub mod module_writer;
pub mod packer;
pub mod pitch;
pub mod project;
pub mod project_v2;
pub mod report;
pub mod sfc_export;
pub mod spc;
pub mod tools;
