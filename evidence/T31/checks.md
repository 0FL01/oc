# T31 — executed checks

All network traffic below uses temporary fixtures and loopback fake endpoints.
No paid/live probe was requested or executed.

## Red baseline

`cargo test --locked -p oc --test application -- --nocapture` — exit **101**
on runtime baseline `e4c7036`; see [regression.md](regression.md). The real binary
never contacted its configured endpoint.

## Final runtime/test diff

`cargo test --locked -p oc --test pty -- --test-threads=1` — exit **0**:

```text
running 13 tests
aud02_store01_persist_resume_across_restart ... ok
pty_escape_cancels_heartbeat_request ... ok
pty_long_history_starts_and_pages ... ok
pty_panic_restores_terminal ... ok
pty_resize_redraws_full_frame ... ok
pty_slow_consumer_stall_then_drain ... ok
pty_smoke_type_echo_quit ... ok
pty_ssh_like_term_variants ... ok
pty_tiny_screen_survives ... ok
pty_unicode_and_paste_roundtrip ... ok
tui_without_tty_is_usage_error ... ok
ui05_interrupted_exit_is_nonsuccess ... ok
ui05_ndjson_stdout_only_and_slow_consumer ... ok
test result: ok. 13 passed; 0 failed; 0 ignored; finished in 30.47s
```

`cargo test --locked -p oc --test application` — exit **0**:

```text
aud01_binary_sends_configured_responses_request ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; finished in 0.13s
```

After clippy requested a safety-comment placement and equivalent let-chain
format in the key handler, the following final workspace gates passed:

- `cargo fmt --all -- --check` — exit **0**.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — exit **0**.
- `cargo test --workspace --locked` — exit **0**. All executed test groups passed:
  oc unit 1; binary application 1; binary PTY 13; adapter unit 97; offline E2E 3;
  remote MCP 15; stdio MCP 6; Runtime 12; soak 4; core unit 16; TUI unit 30.
  Three pre-existing opt-in external harnesses remained ignored:
  `live_workflow_harness`, `live_search_harness`, `real_server_smoke`.
  No newly ignored test and no live PASS claim.
- `cargo build --locked` — exit **0**.
- `target/debug/oc --help` — exit **0**, lists run/sessions/tui/help and data-dir.
- `git diff --check` — exit **0**.

Earlier compilation errors (ProtectedGlobs constructor, ToolPolicy Sync) and
clippy diagnostics were corrected, not suppressed. `ToolPolicy` has three
existing immutable implementations; all-target compilation checks their direct
consumers. Package/dependency count and Cargo.lock are unchanged.

These results qualify T31 wiring only. Old module-level PASS results do not
resolve F02–F18. In particular, the heartbeat cancellation checks prove cancel
after a visible incremental delta, not cancellation while a peer is silent.
