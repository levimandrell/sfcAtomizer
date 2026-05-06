//! Embedded BRR fixture corpus.
//!
//! Every JSON file under `core/fixtures/brr/` is `include_str!`'d at
//! compile time so the corpus travels with the binary and the
//! `decode-fixtures` CLI subcommand can run without filesystem
//! access. See `core/fixtures/brr/README.md` for the provenance
//! discipline.

use serde::Deserialize;

use crate::brr::{decode_blocks, BrrDecoderState};
use crate::report::BrrFixtureResult;

/// One fixture as it lives in the binary at runtime.
pub struct EmbeddedFixture {
    pub name: &'static str,
    pub json: &'static str,
}

/// The M0.2 raw-decode corpus. Order is stable; tests may depend on
/// it. Adding a fixture is a documented change in
/// `core/fixtures/brr/README.md`.
pub const M0_RAW_DECODE_FIXTURES: &[EmbeddedFixture] = &[
    EmbeddedFixture {
        name: "filter0_basic",
        json: include_str!("../fixtures/brr/filter0_basic.json"),
    },
    EmbeddedFixture {
        name: "filter0_shift_clamp",
        json: include_str!("../fixtures/brr/filter0_shift_clamp.json"),
    },
    EmbeddedFixture {
        name: "filter1_zero_history",
        json: include_str!("../fixtures/brr/filter1_zero_history.json"),
    },
    EmbeddedFixture {
        name: "filter1_nonzero_history",
        json: include_str!("../fixtures/brr/filter1_nonzero_history.json"),
    },
    EmbeddedFixture {
        name: "filter2_nonzero_history",
        json: include_str!("../fixtures/brr/filter2_nonzero_history.json"),
    },
    EmbeddedFixture {
        name: "filter3_nonzero_history",
        json: include_str!("../fixtures/brr/filter3_nonzero_history.json"),
    },
    EmbeddedFixture {
        name: "multi_block_predictor_history",
        json: include_str!("../fixtures/brr/multi_block_predictor_history.json"),
    },
    EmbeddedFixture {
        name: "loop_boundary_history",
        json: include_str!("../fixtures/brr/loop_boundary_history.json"),
    },
    EmbeddedFixture {
        name: "flags_end_loop_ignored_by_raw_decode",
        json: include_str!("../fixtures/brr/flags_end_loop_ignored_by_raw_decode.json"),
    },
];

/// Run one fixture through `core::brr::decode_blocks` and produce a
/// [`BrrFixtureResult`]. JSON-parse errors and hex-decode errors are
/// reported as `passed: false` rather than as runtime errors so the
/// CLI's `decode-fixtures` command gets a uniform per-fixture report
/// shape.
pub fn run_fixture(fx: &EmbeddedFixture) -> BrrFixtureResult {
    match try_run_fixture(fx) {
        Ok(result) => result,
        Err(message) => BrrFixtureResult {
            name: fx.name.to_string(),
            passed: false,
            failure: Some(message),
        },
    }
}

fn try_run_fixture(fx: &EmbeddedFixture) -> Result<BrrFixtureResult, String> {
    let parsed: FixtureFile =
        serde_json::from_str(fx.json).map_err(|e| format!("json parse error: {e}"))?;
    if parsed.name != fx.name {
        return Err(format!(
            "name mismatch: registered={}, file={}",
            fx.name, parsed.name
        ));
    }
    let mut blocks = Vec::with_capacity(parsed.blocks_hex.len());
    for (i, s) in parsed.blocks_hex.iter().enumerate() {
        blocks.push(parse_hex_block(s).map_err(|e| format!("blocks_hex[{i}]: {e}"))?);
    }
    let mut state = BrrDecoderState {
        prev1: parsed.initial_history.prev1,
        prev2: parsed.initial_history.prev2,
    };
    let actual = decode_blocks(&blocks, &mut state);
    if actual == parsed.expected_pcm {
        Ok(BrrFixtureResult {
            name: parsed.name,
            passed: true,
            failure: None,
        })
    } else {
        Ok(BrrFixtureResult {
            name: parsed.name,
            passed: false,
            failure: Some(diff_message(&actual, &parsed.expected_pcm)),
        })
    }
}

fn diff_message(actual: &[i16], expected: &[i16]) -> String {
    if actual.len() != expected.len() {
        return format!(
            "length mismatch: actual={}, expected={}",
            actual.len(),
            expected.len()
        );
    }
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        if a != e {
            return format!("first divergent sample at index {i}: actual={a}, expected={e}");
        }
    }
    "no diff".to_string()
}

fn parse_hex_block(s: &str) -> Result<[u8; 9], String> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() != 9 {
        return Err(format!("expected 9 hex bytes, got {}", tokens.len()));
    }
    let mut out = [0u8; 9];
    for (i, t) in tokens.iter().enumerate() {
        out[i] = u8::from_str_radix(t, 16).map_err(|e| format!("bad hex byte '{t}': {e}"))?;
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct FixtureFile {
    name: String,
    #[serde(default)]
    #[allow(dead_code)] // round-trip-only; not used by the runner.
    description: String,
    initial_history: HistoryState,
    blocks_hex: Vec<String>,
    expected_pcm: Vec<i16>,
    #[serde(default)]
    #[allow(dead_code)]
    provenance: String,
    #[serde(default)]
    #[allow(dead_code)]
    provenance_notes: String,
}

#[derive(Debug, Deserialize)]
struct HistoryState {
    prev1: i16,
    prev2: i16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_has_nine_fixtures() {
        assert_eq!(M0_RAW_DECODE_FIXTURES.len(), 9);
    }

    #[test]
    fn corpus_names_unique() {
        let mut names: Vec<&str> = M0_RAW_DECODE_FIXTURES.iter().map(|f| f.name).collect();
        names.sort_unstable();
        let original_len = names.len();
        names.dedup();
        assert_eq!(names.len(), original_len, "duplicate fixture names");
    }

    #[test]
    fn parse_hex_block_accepts_canonical_form() {
        let b = parse_hex_block("00 12 34 56 78 9A BC DE F0").unwrap();
        assert_eq!(b, [0x00, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);
    }

    #[test]
    fn parse_hex_block_tolerates_whitespace_runs() {
        let b = parse_hex_block("  00\t12  34  56\n78 9A\tBC DE F0  ").unwrap();
        assert_eq!(b, [0x00, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);
    }

    #[test]
    fn parse_hex_block_rejects_wrong_token_count() {
        assert!(parse_hex_block("00 11 22").is_err());
        assert!(parse_hex_block("").is_err());
    }
}
