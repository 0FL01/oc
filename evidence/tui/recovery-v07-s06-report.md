# T44 V07 S06 — pending navigation, stale events and durable replay

Base `004cdcf`, regression commit `5c91948`. No production defect was
reproduced. All new fixtures are local, isolated HOME/XDG/project/SQLite,
loopback fake Responses and an owned fake stdio MCP child; no real credentials,
owner data or external service.

## Pending acceptance / switch refusal

New `s06_pending_submit_refuses_navigation_and_keeps_draft` in
`crates/oc/tests/mcp_application.rs` starts the actual TUI binary under PTY.
The fake MCP holds initialize before acceptance. Real Ctrl+X L refuses a
session picker; `/location <other>` entered via the editor refuses a Location
switch, both with `turn active; action unavailable`. The draft is restored and
the original session persists; provider requests and durable turns are zero.
Real Esc cancels pending acceptance before the fake is released, its owned
child is reaped, the draft remains, and neither history nor provider receives
a turn. The test does **not** claim it delivered an old worker delta across
Locations: the forbidden navigation never publishes a second generation.

Existing owner/state tests exercise invalidated acceptance and stale
turn/tool events (`app::tests::pending_receipt_preserves_edits_and_ignores_old_generation`,
`stale_turn_events_are_ignored`, `stale_tool_events_are_ignored`), plus the
intent guard `tui_cmd::tests::pending_submission_refuses_session_and_location_switches`.
Actual PTY `aud30_pty_paste_resize_error_recovery` refuses a streaming session
switch; `aud38_location_switch_is_one_lifecycle` refuses a streaming Location
switch, then proves A→B→A with global instructions retained, project-local
rules/skills/MCP replaced, and raw sessions bound to their Locations.

## Replayed operation state

Extended `aud06_binary_kill_after_side_effect_recovers_unknown_without_replay`
in `crates/oc/tests/durability.rs`. On a genuine killed process and reopened
application, the unknown bash effect is shown in the rendered transcript as
`Outcome unknown (operation was interrupted)`; `/cards` projects the same
durable op ID/input/output/unknown state and paints `bash unknown (<id>)`.
Existing assertions prove no side-effect replay or duplicate operation.
`binary_restart_projects_real_metadata_and_parts` tests partial/truncated
patch/tool replay; `v07e_bare_pty_model_and_stdio_tool_controls_are_inert_across_replay`
checks complete cards after actual PTY restart. The new unknown projection uses
the real restarted app and `TuiState` TestBackend renderer, not a restarted
second PTY for this particular unknown card.

## Executed checks

- New S06 PTY test and extended crash-recovery test: each **PASS**, 1.
- Existing intent guard, stale receipt/turn/tool state cases and streaming PTY
  `aud30` and `aud38`: **PASS**, 1 each (the first attempted `--exact` filter
  of module-qualified unit names selected zero tests; rerun without that flag
  executed one of each).
- `cargo test --locked -p oc --test recovery_v02 binary_restart_projects_real_metadata_and_parts -- --exact`
  and `cargo test --locked -p oc --test mcp_application v07e_bare_pty_model_and_stdio_tool_controls_are_inert_across_replay -- --exact`:
  **PASS**, 1 each.
- `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet`,
  `cargo build --locked`, `git diff --check`: **PASS**, workspace zero failures,
  existing opt-in live tests ignored.

No paired visual reference capture in this safety slice. S08, S07 resources
and mandatory VIS01–VIS24 remain open, T44 stays active. The old untracked
`.opencode/` was not read or modified.
