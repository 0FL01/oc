# T56 application owner/admission slice (2026-10-06)

Base `88e6631d0`. R1/R4 application integration, not full TERM01/frontend or VIS39.
`CoreApp::terminal(session, TerminalAction)` is acknowledged through the existing
single application worker; no model tool, turn acceptance or provider request.
One native owner survives Location/config runtime replacement. Create checks the
captured Location and worker-wide selection epoch plus the stored source session
before OS effects; all existing controls compare the immutable full PTY reference
and actor. After reload/switch, an existing PTY remains in its original cwd/env.

Linux capability is projected as `TuiChrome.session_terminal`, without a config
flag or startup spawn. Failed terminal recovery leaves local Home/history/catalog
and enabled controls available, refuses terminal actions and fails final cleanup;
it cannot invent a successful terminal or disable the feature silently. Actual
exit/remove publishes typed `TerminalChanged`, with current facts queried by the
frontend. Normal stop/error paths explicitly join/close/reap the owner. Root-family
deletion refuses while a live PTY retains its recovery identity; it is not a hidden
terminal cancellation. Explicit remove remains available.

Migration correction: review of the committed source found the first slice used
version11 (already T53 credentials), despite its receipt intending migration12.
This slice fixes the registration to12 and proves both distinct rows in SQLite;
tables/data are additive, no historical baseline/receipt is rewritten.

Separate `application/terminal_tests.rs` proves actual source/foreign/stale controls,
two PTYs/no startup spawn, configless provider-unready usability, create epoch refusal
after reload, original cwd and synthetic-key exclusion after a Location switch,
persisted select/hide without kill, deletion refusal, explicit remove/event and
shutdown/process absence, no tool operations, migration identities. Corrupt pending
terminal metadata retains local application usability but returns terminal and join
errors. Scripted CoreApp explicitly refuses native terminal operations.

Checks (normal stacks, TMPDIR approved cache, jobs3/test threads2, no parallel Cargo):
- `cargo test --locked -p oc-adapters --lib term01_`: **7 passed/0 failed**, including
  the four prior real PTY/VT tests and three application scenarios.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: exit0.
- workspace fmt, journal and diff checks: exit0.

Initial compile diagnosed missing picker-helper owner parameters and an incorrect
StorageError/CoreError diagnostic conversion. Corrected the typed seams; recovery
errors stay terminal-local rather than accidentally blocking local UI startup.
No new dependencies/live calls, user `.opencode/` untouched. Next: real frontend
Terminals composer/right pane, raw-key/focus/leader routing, VT cells and resize/
cursor attach, then TERM01 debug/release actual binary and lifecycle/resource gates.
