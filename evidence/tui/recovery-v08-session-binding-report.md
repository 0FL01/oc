# T44 — atomic root creation and Location binding

## RECON and contract

Pinned v2.0.12 add-tab flow navigates to a sessionless Home placeholder
(`packages/tui/src/context/session-tabs.tsx:384-397`) and creates its
durable session only on first prompt submission
(`packages/tui/src/component/prompt/index.tsx:1203-1234`). Native currently
creates sessions eagerly, so the earlier adaptive tab geometry cannot yet
enable the `+` affordance honestly. An immediate storage prerequisite had
an independent failure: `Db::create_session` inserted a root row and then
`session_created` in separate autocommits; `Runtime::create_session` wrote
its `tui.session_location.<id>` binding separately. A failure in either
later write could strand a root unusable through `open_session`, preventing
safe retry. These are the existing paths in `storage.rs:328-337` and
`runtime.rs:1186-1201` before commit `0980a33`.

## Test-first correction

An injected SQLite `BEFORE INSERT ON events` failure initially left the
new session row (`1`, expected `0`). A separate injected
`BEFORE INSERT ON prefs` failure likewise left a root row (`1`, expected
`0`). Commit `0980a33` moves root row/event and, for Location-scoped
creation, its binding into one SQLite transaction. After either injected
failure no candidate root, creation event or binding remains; a retry
creates one bound session. Same-Location creation remains idempotent,
cross-Location reuse returns `LocationMismatch`, and an already existing
unbound root remains a duplicate rather than being silently claimed.
Tests: `storage::tests::root_creation_rolls_back_when_event_insert_fails`
and `root_location_creation_rolls_back_on_pref_failure_and_retries`
(`crates/oc-adapters/tests/runtime.rs`). Existing public `Db::create_session`
remains available for standalone storage/test callers; application-owned
`Runtime::create_session` uses the transactional bound path.

## Gates and limits

`cargo test --locked -p oc-adapters --lib storage::tests` (19 passed),
`cargo test --locked -p oc-adapters --test runtime` (46 passed), full
serialized workspace tests (zero failures, 189 TUI tests; existing opt-in
live ignores), `cargo fmt --all -- --check`, workspace all-target Clippy
`-D warnings`, `cargo build --locked`, documentation and journal checks,
and `git diff --check` passed. This is a state-integrity correction, not
a new visual surface. No new original/native paired capture was needed to
diagnose a SQLite rollback; the previous immutable paired capture in
`evidence/tui/recovery-v08-tab-geometry-01/` still reports DIFFERENT.

**Not yet solved:** this transaction does not include turn/user-message
acceptance or sessionless Home selection. A true fresh-submit operation
must run exact model/variant/admission and agent permission validation
before atomically persisting the new root/binding/turn/message, and only
then publish the accepted receipt and in-memory Location map. Do not
enable `+` or claim multi-tab parity until that action and retained tab
state exist. T44 VIS01–VIS24 and V08–V09 remain open.
