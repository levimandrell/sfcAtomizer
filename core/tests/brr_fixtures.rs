//! Fixture-driven integration tests for `core::brr`.
//!
//! Each `#[test]` decodes one fixture from the embedded corpus and
//! asserts byte-identical equality against its frozen `expected_pcm`.
//! Per-fixture tests give individual failure lines in cargo's output;
//! a corpus-wide test wouldn't tell you which fixture broke.

use sfc_atomizer_core::brr_fixtures::{run_fixture, EmbeddedFixture, M0_RAW_DECODE_FIXTURES};

fn fixture(name: &str) -> &'static EmbeddedFixture {
    M0_RAW_DECODE_FIXTURES
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fixture named {name}"))
}

fn assert_fixture_passes(name: &str) {
    let fx = fixture(name);
    let r = run_fixture(fx);
    assert!(
        r.passed,
        "{}: {}",
        r.name,
        r.failure.as_deref().unwrap_or("(no failure message)")
    );
}

#[test]
fn filter0_basic() {
    assert_fixture_passes("filter0_basic");
}

#[test]
fn filter0_shift_clamp() {
    assert_fixture_passes("filter0_shift_clamp");
}

#[test]
fn filter1_zero_history() {
    assert_fixture_passes("filter1_zero_history");
}

#[test]
fn filter1_nonzero_history() {
    assert_fixture_passes("filter1_nonzero_history");
}

#[test]
fn filter2_nonzero_history() {
    assert_fixture_passes("filter2_nonzero_history");
}

#[test]
fn filter3_nonzero_history() {
    assert_fixture_passes("filter3_nonzero_history");
}

#[test]
fn multi_block_predictor_history() {
    assert_fixture_passes("multi_block_predictor_history");
}

#[test]
fn loop_boundary_history() {
    assert_fixture_passes("loop_boundary_history");
}

#[test]
fn flags_end_loop_ignored_by_raw_decode() {
    assert_fixture_passes("flags_end_loop_ignored_by_raw_decode");
}

#[test]
fn corpus_is_complete() {
    assert_eq!(M0_RAW_DECODE_FIXTURES.len(), 9, "missing fixtures");
}
