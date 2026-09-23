# T44 V07 S05 — MCP attach failure does not bypass policy

## RECON and bounded result

Base `d20f9c5`, test commit `0abe005`. Isolated fake Responses and HTTP MCP
servers under fixture-owned HOME/XDG/project; no real credentials, owner data,
external MCP endpoints or upstream screenshot involved. No production defect was
reproduced on this HEAD. In this profile, an enabled unavailable server is
**visibly degraded**, not mandatory-fatal: the `required` fixture key is a
server name, not a `required` config field. D13 / current runtime preserves
the turn and emits a staged warning while withholding the failed server's tools.
This is not a claim that a required/fatal config mode exists.

New real-binary regression
`v07_s05_disabled_and_failed_mcp_only_retry_after_explicit_repair` covers:

- Disabled entry: zero HTTP initialize/list/probe both before and after repair.
- Configured unavailable entry: bad bearer produces one failed initialize,
  `warning: mcp required initialize: unauthorized (retryable=false)` and no
  `required__` tools in the provider request. The turn still answers (the
  project's explicit per-server degradation policy); the diagnostic omits the
  fixture token, response body and URL.
- Editing the isolated config does not replay a prior turn. A **new explicit**
  `oc run` uses the repaired bearer, completes initialize/initialized/list,
  advertises `required__search` but not disabled `disabled__probe`, with exactly
  one new provider turn. Both server generations use the same fake endpoint.

Existing independent tests cover the PTY presentation of the staged warning
and redaction (`v01_degraded_remote_diagnostic_is_staged_and_redacted_in_tui`),
disabled local child zero-spawn (`aud23_tui_two_turns_own_one_stdio_child_and_disabled_entry_zero_spawns`),
generic remote with no bearer versus strict `codex_web` requiring bearer
(`v01_anonymous_remote_and_codex_required_auth`), and remote wire/protocol
negotiation/cancellation (`mcp_remote` suite). The fixture server named
`required` is not silently made optional by this test; absent a schema field
expressing required/fatal attach, the available configured-server contract is
the visible degradation policy; a stronger requirement would need its own
explicit config contract, not a change of test expectation.

## Executed checks

- `cargo test --locked -p oc --test mcp_application v07_s05_disabled_and_failed_mcp_only_retry_after_explicit_repair -- --exact --nocapture`: **PASS**, 1.
- `cargo test --locked -p oc --test mcp_application v01_`: **PASS**, 5.
- `cargo test --locked -p oc --test mcp_application aud23_tui_two_turns_own_one_stdio_child_and_disabled_entry_zero_spawns -- --exact`: **PASS**, 1.
- `cargo test --locked -p oc-adapters --test mcp_remote`: **PASS**, 24, 1 opt-in live ignored.
- `cargo test --locked -p oc-adapters --test mcp_stdio disabled_spawns_nothing -- --exact`: **PASS**, 1.
- `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet`, `cargo build --locked`, `git diff --check`: **PASS** on code commit contents, zero workspace test failures; existing opt-in live cases ignored.

No paired visual fixture in this S05 safety slice. S06, S08, S07 and remaining
VIS01–VIS24 are **open**; no TUI parity claim. The only pre-existing untracked
worktree path `.opencode/` was not read or modified.
