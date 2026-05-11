//! M5.0 — Pitch register fixture pin per SPEC §10.11.
//!
//! The native-rate characterization harness (M5.1
//! implementation) MUST configure each characterization SPC
//! such that the DSP pitch register for the test voice equals
//! exactly `0x1000` (4096). This test asserts that contract via
//! the `baselines/m5.json` literal-pin pattern, anticipating
//! M5.1's implementation.
//!
//! Per consultant M5 plan #6. Mirrors the M4.0 Phase G pattern
//! (fixture-pin metric formulas before encoder implementation);
//! the equivalent for M5.0 is pinning the
//! "pitch register MUST equal 0x1000" baseline value at compile
//! time so an accidental edit to the baselines file is caught
//! before M5.1 lands.

#[test]
fn m5_pitch_register_constant_pinned_at_4096() {
    const BASELINES_JSON: &str = include_str!("../../baselines/m5.json");
    let baselines: serde_json::Value =
        serde_json::from_str(BASELINES_JSON).expect("baselines/m5.json must parse");

    let behavior_gated = baselines["behavior_gated"]
        .as_array()
        .expect("baselines/m5.json::behavior_gated must be an array");

    let pitch_entry = behavior_gated
        .iter()
        .find(|e| e["name"].as_str() == Some("M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER"))
        .expect(
            "baselines/m5.json must contain a behavior_gated entry named \
             M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER",
        );

    let value = pitch_entry["value"]
        .as_i64()
        .expect("M5_NATIVE_RATE_CHARACTERIZATION_PITCH_REGISTER value must be an integer");

    assert_eq!(
        value, 4096,
        "M5 native-rate characterization pitch register MUST be 0x1000 (4096) per SPEC §10.11"
    );

    // Cross-checks on the entry's shape so accidental edits to
    // the locked-at / kind / test fields are also caught.
    assert_eq!(
        pitch_entry["locked_at"].as_str(),
        Some("M5.0"),
        "pitch-register baseline must be locked_at M5.0"
    );
    assert_eq!(
        pitch_entry["kind"].as_str(),
        Some("harness_constant"),
        "pitch-register baseline kind must be harness_constant"
    );
    assert!(
        pitch_entry["test"].is_string(),
        "pitch-register baseline must carry a test: field"
    );
}

// M5.1 will add `pitch_register_equals_4096_for_native_rate_signals`
// which exercises actual harness behavior. The stub here
// documents the M5.1 obligation and ensures the locked value
// can't drift before M5.1 lands.
