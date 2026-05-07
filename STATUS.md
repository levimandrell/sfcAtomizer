# SFC Wave Compiler — Status

## Current milestone

**M2.7 — GUI atom / sequence / track editing shipped.**
Read-only v2 panels from M2.1 are now editable: atom_pool,
atom_sequences, tracks, and the driver profile dropdown
all live on a new `app/src/v2_editor.rs` state model that
wraps `ProjectV2` + selection bookkeeping + cached
validation. The egui code in `app_main.rs` is a view layer
over this model; live validation runs after every edit and
surfaces a per-frame summary with red highlight when the
project isn't currently saveable.

The load-bearing acceptance gate is **round-trip parity**:
a project edited through the model and saved via
`V2EditorModel::save_to` is byte-identical to the same
project saved through `ProjectV2::save_to_path` directly.
Slider widgets snap their f64 outputs to 4-decimal
precision to keep JSON serialization deterministic across
edit sessions; the model itself trusts the caller.

Profile switching: dropdown at the top of the v2 panel.
Going to `sample_basic` from `multi_voice_atom` with
non-empty atom data is destructive — clears `atom_pool`,
`atom_sequences`, all `atom_sequence` tracks, and
`m2.active_sequence_id`. The `SwitchProfileEffect` enum
distinguishes the destructive case from the additive case
(sample_basic → multi_voice_atom adds nothing); the
status bar surfaces the cleared counts post-switch.
`bytecode_version` auto-syncs with the chosen profile.

Atom preview button: renders the currently selected atom
through `core::atom::render_to_brr`, decodes through the
M2.2 PCM path, and writes a 2 s looped mono WAV next to
the project for audition. No runtime synthesis introduced
(all rendering is offline / compile-time per SPEC §0).

Phase 0 cleanup hygiene shipped alongside: `voice_setup.rs`
TODO replaced with spec-aligned wording (consultant M2.4
#4); `DRIVER_CODE_BUDGET_M1` renamed to
`DRIVER_CODE_BUDGET_4KIB` workspace-wide (the M1 tag was
misleading after `build_m2` started using the same cap;
SPEC §15.5 makes the budget profile-agnostic); STATUS
M2.4 narration's "+8 net delta" replaced with the per-file
breakdown the actual delta of ~+27 demands.

**M2.8 next.** Release prep / m2-acceptance reproducer /
final M2 polish. PM to brief.

## Last pass

**Pass M2.7 — GUI atom / sequence / track editing + cleanup hygiene.**

- **Phase 0 (cleanup hygiene):** voice_setup.rs TODO →
  spec-aligned wording per SPEC §15.7;
  `DRIVER_CODE_BUDGET_M1 → DRIVER_CODE_BUDGET_4KIB`
  workspace rename (pure rename, no behavior change);
  STATUS M2.4 narration fix; M2.6-postlude `--out-wav`
  plumbing on `verify-spc-stereo` carried forward.
- **Phase A–D (GUI editor proper):** new
  `app/src/v2_editor.rs` state model with full atom /
  sequence / track CRUD APIs + per-field setters that
  re-validate after every change. Profile switch handler
  (`switch_profile`) returns `SwitchProfileEffect`
  variants distinguishing no-op / additive / destructive
  clears. Slider snap helper (`snap_f64_4dp`) for f64
  fields. Egui view layer in `app_main.rs` consumes the
  model; widgets land on collapsing headers (Atom Pool /
  Atom Sequences / Tracks). Step transitions are locked
  in the model (initial_kon for step 0, fade_to_zero_retrigger
  for steps 1+); the UI hides the transition picker for
  step 0 and shows fade_in/out sliders otherwise. Save
  button is disabled while validation is non-empty;
  dirty-flag tracking surfaces unsaved-changes state.
- **Phase E (round-trip parity test, load-bearing):**
  `round_trip_parity_gui_save_byte_identical_to_json_save`
  builds a v2 project, saves twice (once via
  `ProjectV2::save_to_path`, once via the editor model),
  asserts byte-identical SHA-256. Companion
  `round_trip_parity_after_no_op_edit_then_revert` proves
  reverting a slider edit round-trips to the original
  bytes. Plus `loading_v1_through_editor_model_save_matches_independent_migration`
  for the M1-baseline-preservation invariant.
- **Phase F (atom preview button):** wires
  `core::atom::render_to_brr` → loop the cycle PCM to
  ~2 s at 32 kHz → write through
  `core::audition::write_pcm16_mono_wav_pub`. Output WAV
  lands next to the project file. No runtime synthesis;
  reuses M2.2 infrastructure verbatim.
- **Phase G (this entry).**
- **Tests, +17:** all in `app/src/v2_editor.rs::tests`.
  Round-trip parity (3); slider snap determinism (1);
  atom CRUD + duplicate (3); cross-pool ID collision (1);
  step-transition locks (2); track voice conflict (1);
  profile switch destructive / additive / no-op (3);
  add-track voice fallback (1); dirty-flag lifecycle (1);
  v1 migration through model byte-identical (1).
- **Cargo gates:** `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` all green. **505 tests
  workspace-wide** (was 488 at M2.6; +17 net delta — the
  v2_editor test module).

### Decisions log additions (M2.7)

- GUI v2 atom / sequence / track editing implemented;
  live validation per-field; save uses existing
  `ProjectV2::save_to_path` for byte-identity preservation.
- Profile switching surfaces destructive / additive
  effect via `SwitchProfileEffect`; the GUI reports
  cleared counts in the status bar (no modal popup at
  M2.7 — can come at M2.8 if user feedback wants it).
- Round-trip parity test locks GUI vs JSON byte-identity.
- GUI editing logic factored as state model independent
  of egui rendering; UI is a view layer; tests exercise
  the model directly without an egui harness.
- Atom preview button uses the existing M2.2 render
  pipeline; no runtime synthesis introduced.
- Sequence preview deferred (would require a runtime
  interpreter in the GUI process — forbidden per SPEC §0).
- f64 slider widgets snap to 4-decimal precision before
  storing into the model so JSON serialization stays
  byte-stable across edit sessions.
- `DRIVER_CODE_BUDGET_M1 → DRIVER_CODE_BUDGET_4KIB`
  workspace rename; the budget is profile-agnostic per
  SPEC §15.5 and the M1 tag was misleading after
  `build_m2` started consuming the same cap.

## Previous passes

Pre-M2.8 pass log archived at
[`docs/history/M0-M2-passes.md`](docs/history/M0-M2-passes.md) —
M0 through M2.7. STATUS.md keeps the current milestone, last
pass summary, decisions log additions for the current pass, and
current baselines. Historic entries land in the archive as
they age out.

## Current baselines

Machine-readable baselines + classification (identity-gated /
behavior-gated / documentary snapshot / retired) live in
[`baselines/m2.json`](baselines/m2.json). The canonical M2 SEQ2
bytecode + voice setup table hex dumps + per-byte breakdowns
live in [`baselines/m2_canonical_fixtures.md`](baselines/m2_canonical_fixtures.md).

Top-line locked values (mirror of `baselines/m2.json`):

```
M1_LOADER_SIZE_BYTES                  = 588
M1_LOADER_SHA256                      = 955f525c...873f40
M1_DRIVER_CODE_SHA256                 = (see baselines/m2.json)
M2_CANONICAL_SEQUENCE_TOTAL_TICKS     = 249   (sum-of-WAIT-operands)
M2_CANONICAL_SEQUENCE_ELAPSED_TICKS   = 254   (wall-elapsed under SPEC §14.3)
M2_CANONICAL_SEQUENCE_BYTECODE_SHA256 = f9fa6ea8...0fd24f0
M2_CANONICAL_VOICE_SETUP_TABLE_SHA256 = f2faaed8...089ad5
M2_AUDIBILITY_FLOORS                  = max_abs >= 1000, rms >= 200
M2_SILENCE_CEILING                    = max_abs <= 50  (hard-panned silent side)
M2_SOURCE_STEP_ZCR_RATIO_FLOOR        = 1.5  (post.right.zcr / pre.right.zcr)
M2_MODULE_CAP_BYTES                   = 32768  (SPEC §15.6, > triggers ModuleTooLarge)
```

Behavior-gated thresholds carry forward unchanged across M2 releases.
Identity-gated SHAs shift only on intentional driver / loader / fixture
edits and require coordinated baseline updates. Documentary snapshots
(M2 driver size, atom BRR SHAs) are informational and expected to
shift at M3 BRR-encoder-quality work.
