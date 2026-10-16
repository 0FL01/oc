# T44 recovery V01 — asynchronous prompt acceptance and MCP diagnostics

**Latest status:** the independent-review follow-up at the end of this report
supersedes the initial handoff's manual-compression/ownership limitations and
source hashes. Earlier failures and qualification records are retained below.

## Result

Implemented and targeted-tested in the uncommitted worktree based on
`679e683121722c0a00aac7acd95f2be91190ad9e` (2026-09-22). This report covers V01's
prompt submission path and the relevant S01/S05/S06 regressions. It does **not**
qualify full S06, VIS01–VIS24, pixel parity, T44, or the product goal.

### Observed failure before implementation

Added the stalled-initialize actual-binary PTY test before production changes.
On the starting HEAD plus that test, it failed with:

```text
UI frozen before acceptance: edit/resize not rendered within 2s
test result: FAILED. 0 passed; 1 failed; 7 filtered out; finished in 2.19s
```

The fake is a fixture-owned Python stdio MCP child, stalled indefinitely inside
`initialize` until a fixture file exists. Configured MCP timeout is 10 seconds;
the test's interaction/cancellation bound is 2 seconds. It never calls
`CoreApp::cancel()` directly. HOME/XDG/project/config/provider/MCP are isolated,
synthetic fixtures; no actual user MCP configuration was accessed.

Raw sequence through `PtyProcess`:

1. At 100×28, write `pending draft\r`; wait for fake's initialize marker.
2. Write duplicate `\r`, perform `TIOCSWINSZ` to 120×40, write ` edited`.
3. Before releasing fake, require rendered edited draft and VT cursor output for
   row 40. Write `\x1b` (Esc); require the owned PID to be gone within the same
   2-second bound. Final qualified run: **26.632722 ms** from duplicate/resize
   through cancellation/reap (not a latency guarantee).
4. Assert release absent, provider requests = 0, durable turns = 0, messages = 0.
5. Create release file, wait for cancellation notice, write `\r` explicitly to
   retry the retained `pending draft edited`. Require its provider answer and
   completed durable turn; write `\x03` (Ctrl+C). Exactly 1 provider request,
   1 turn, 2 messages; no duplicate submission.
6. Remove release file and launch a fresh isolated session using the same fixture
   DB. Submit again, wait for initialize, write Ctrl+C; require clean shutdown
   within 2 seconds without release or an additional durable turn/request.
   All three recorded owned child PIDs must be absent before fixture fallback
   cleanup runs.

A second pre-fix observation arose while strengthening request-ownership checks:
with receipt-based UI responsiveness already implemented, cancellation of a
remote initialize left the actual TCP request open. The new raw PTY test failed
`cancel leaked an HTTP request`. Inspection of pinned rmcp 3.4.0 showed startup
POST/initial SSE awaits outside its worker cancellation select. The final test
observes TCP closure before the fake ever sends an initialize response, retains
the draft, and asserts one initialize, zero provider requests and zero turns.

### Exact implementation areas

- `oc-core/src/core_app.rs`: bounded nonblocking `request_submit` and an
  acceptance-only `SubmissionReceipt`; no detached submit task. Existing async
  APIs remain available. Mock and native application owners send acceptance
  receipt before publishing the corresponding turn event.
- `oc-tui/src/app.rs`: one `PendingSubmission`, immutable draft snapshot,
  request/session/generation identity and input revision. Duplicate Enter is
  refused. Editing remains possible. Acceptance clears only the corresponding
  unedited/non-cancelling draft; rejection retains current draft. Esc targets
  the pending session. Receipt reconciliation precedes event application.
  Existing busy gates now include pending acceptance.
- `oc/src/tui_cmd.rs`: poll receipts while driving frames/events, reject events
  for another session, preserve Quit over already queued terminal events.
  Tests check visible session/Location switch refusal while pending and stale
  session events. TUI unit tests cover A→B→A receipt/delta/tool defenses.
- `oc-adapters/src/mcp_stdio.rs`: cancellation is processed within the handshake
  owner, with bounded process-group/stderr cleanup awaited before rejection.
- `oc-adapters/src/mcp_remote.rs` and new `mcp_http_lifecycle.rs`: cancellable
  attach, including pending HTTP POST/SSE operations. HTTP-owner completion is
  observed before failed/cancelled attach returns, with bounded cleanup failure
  surfaced. rmcp continues to own the protocol. Normal close still uses rmcp's
  service shutdown; no replacement MCP stack or detached submission/reaper task.
- `oc-adapters/src/runtime.rs`: transport still comes from `entry.kind`.
  The explicit `codex_web` profile retains exact URL, required bearer and
  `2025-11-25`; other remote entries use optional configured bearer and SDK
  protocol negotiation. `oauth:false` does not force bearer on anonymous servers.
  Invalid supplied auth remains an error, not an anonymous fallback.
- `RuntimeError::McpAttach` carries server, stage, allowlisted safe code and
  retryable flag through the existing application error boundary to the TUI.
  Server labels are bounded/control-safe. Raw URL/header/body/catalog/SDK error
  strings are not formatted into this diagnostic. Required MCP failures still
  prevent provider execution. Example fixture diagnostic:
  `mcp required initialize: unauthorized (retryable=false)`.
- Tests in `oc/tests/mcp_application.rs`, TUI app/shell, binary `tui_cmd`, and
  adapter MCP/runtime tests cover these behaviors. Existing shell goldens keep
  their content; their setup now waits for asynchronous acceptance. The soak
  test's expected typed error was updated for the added fields (compiled by
  all-targets clippy, not executed in this slice).
- Cargo manifests add direct `futures-util` use for cancellable SDK SSE streams.
  Cargo.lock adds that existing package to oc-adapters' dependency list only;
  no dependency version changed.

## Checks

Commands ran from `/home/opencode/ai/oc`. Aliases below expand to exact commands:

```text
P = cargo test --locked -p oc --test mcp_application v01_pending_initialize_raw_pty_cancel_edit_duplicate_and_retry -- --nocapture
V = cargo test --locked -p oc --test mcp_application v01_ -- --nocapture
M = cargo test --locked -p oc --test mcp_application -- --nocapture
U = cargo test --locked -p oc-tui --lib
A = cargo test --locked -p oc-adapters --test mcp_remote --test mcp_stdio --test runtime
C = cargo clippy --locked -p oc-core -p oc-adapters -p oc-tui -p oc --all-targets -- -D warnings
```

Chronological check ledger (separate parallel commands appear on separate rows):

| Command | Exit | Observed result |
|---|---:|---|
| P, before production fix | 101 | Frozen UI, 1 failure, 2.19 s; reproduction above. |
| P, receipt + stdio fix | 0 | 1 passed. Initial printed 226.10712 ms covered the full scenario; later instrumentation isolates cancellation. |
| V, before diagnostic/auth fix | 101 | 1 passed, 2 failed: anonymous rejected with server-only error; staged TUI diagnostic missing. |
| V, after diagnostic/auth fix | 101 | 2 passed, 1 failed: raw substring matcher missed ratatui's unchanged-cell output. |
| M, text screen decoder added | 101 | 9 passed, 1 failed: expected one diagnostic row, actual toast correctly wrapped over two. Matcher corrected to two row predicates. |
| U, after async implementation | 101 | 87 passed, 3 shell tests failed because their setup assumed synchronous acceptance. Setup now explicitly reconciles receipt. |
| A, first run | 0 | remote 21 passed/1 existing ignored; stdio 10 passed/1 existing ignored; runtime 35 passed. |
| `cargo fmt --all` | 0 | Formatted slice. |
| `cargo fmt --all && M` | 101 | fmt 0; tests 10 passed/1 failed: remote request still open after cancellation. |
| `cargo check --offline -p oc` | 0 | HTTP lifecycle wrapper compiled; lock updated for already-locked futures-util. |
| M, HTTP lifecycle fix | 101 | 10 passed/1 failed: `TUI exit timeout`. Queued completion could overwrite Quit; input loop now exits before applying queued events. |
| `cargo fmt --all && M` | 0 | 11 passed; cancellation/reap 35.083432 ms. |
| `U && cargo test --locked -p oc-core && cargo test --locked -p oc --bin oc` | 0 | Each exit 0: 90 TUI, 17 core, 0 core doctests, 2 binary tests passed. |
| A, HTTP lifecycle + quit fixes | 0 | remote 21+1 ignored; stdio 10+1 ignored; runtime 35 passed. |
| C, first run | 101 | Two test unsafe-block comments were outside assertion macros. Moved comments directly before the blocks; no lint suppression. |
| `cargo fmt --all -- --check && C && git diff --check` | 0 | Each exit 0 on final source. |
| `cargo build --locked -p oc` | 0 | Actual final binary built. |
| `cargo test --locked -p oc-core -p oc-tui --lib && cargo test --locked -p oc --bin oc --test mcp_application -- --nocapture` | 0 | Final source: 17 core, 90 TUI, 2 binary, 11 integration passed. PTY cancellation/reap 26.632722 ms; integration 2.02 s. |
| A, final source | 0 | remote 21 passed/1 ignored (0.56 s), stdio 10 passed/1 ignored (1.83 s), runtime 35 passed (4.89 s). |

The two existing ignored adapter tests require live codex_web credentials and
explicit real-server argv. No newly ignored or suppressed failing tests.
Initial handoff executed scope: **186 passed, 0 failed, 2 existing ignored**.

Auxiliary command/cleanup ledger:

- `git status --short && git rev-parse HEAD && git diff --stat`: exit 0 on entry;
  only the supplied untracked recovery ZIP was initially present.
- `pgrep -af '^/usr/bin/python3 /tmp/.*stalled-mcp$'`: tool denied before execution,
  no process exit. `ps -C python3 -o pid,ppid,args`: exit 0, but did not select the
  shebang fixture's process name.
- `ps -C stalled-mcp -o pid,ppid,comm`: exit 0; discovered PID 144326 left by the
  deliberately failing baseline test, after its TUI process was killed by the
  harness. `kill -TERM -- -144326`: exit 0, cleaned that owned fixture group.
  Added a fixture-only panic cleanup guard; success assertions precede this guard.
- `ps -C stalled-mcp -o pid,ppid,state,comm`: exit 0, no fixture processes listed
  after the subsequent passing checks.
- `git status --short && git diff --stat && git diff -- Cargo.lock`: exit 0.
  Focused `git diff --` reviews of core/TUI/application and MCP/runtime: exit 0.
- `git rev-parse HEAD && git diff --binary HEAD | sha256sum && sha256sum ...`:
  exit 0; the complete source path list/output is below. This hashes tracked
  diff plus separately names/hashes the new untracked lifecycle source.
- `sha256sum target/debug/oc && git diff --numstat`: exit 0.
- Final `git diff --check && git diff --binary HEAD | sha256sum && sha256sum target/debug/oc && git status --short`:
  exit 0. Source diff hash unchanged; final test-built binary hash recorded below.
- Patch-tool bookkeeping: one empty patch call was aborted; two context-mismatch
  patch attempts were rejected atomically and reapplied with actual context.
  These are not executed shell checks and changed no files on failure.

### Source association

Base HEAD: `679e683121722c0a00aac7acd95f2be91190ad9e`.
SHA-256 of `git diff --binary HEAD` for the final tracked implementation/test
changes: `2bdec30f0864ba1fed405b6511c68924aa450906928d6b0412bf135f24dc9b7f`.
The new lifecycle source is untracked at handoff and therefore separately hashed
below. The report itself and the supplied ZIP are excluded from source identity.
`target/debug/oc` SHA-256 after `cargo build --locked -p oc`:
`abfa6b52e7128bbd1f31a673a15aad0d7007a6ac6a0bc3f67c05f627eafc3481`.
Final test-built `target/debug/oc` SHA-256 (the last integration-test artifact):
`d7ea658f2ee2e3cb76979c046066b8f3ff9fe0b9f02bf07723c7d23dfd29b61b`.
The test invocation rebuilt the artifact; the source diff stayed identical.

```text
17bebba5cccc68cfd8942f7cc60edd9b3026a613491434da2bfe615978b1e091  Cargo.lock
e5e3d488f5381f9b7452d51981a42096fc370f5e2f074c56c37e525210b9e76a  Cargo.toml
282db8142f682e2d0467c6ab56f8a9d3d50c8b46d382752478664f237fbf0fbf  crates/oc-adapters/Cargo.toml
92a574af80ddc88ea767c617420da060d1cee2cdfa9e5858a2e5fd13f1d54a25  crates/oc-adapters/src/application.rs
2d9d3b8e0c6359e343df14fedd7b6e8c3868bfaac65972c6abba5e82ab506e83  crates/oc-adapters/src/mcp_remote.rs
e97cad2a2b4294d6a523b94880290cfb048c47f720b2c48ce0086027d9c295f0  crates/oc-adapters/src/mcp_http_lifecycle.rs
ba28e439e5b5c045acc962305373e976838681c962a2db52da8dfe913cbf6f74  crates/oc-adapters/src/mcp_stdio.rs
7dd61e6210f593f3c4be3561cdb0729790a01635335ae428d979eca577a42d01  crates/oc-adapters/src/runtime.rs
b7545f3a5906c46175ce276ec80358570f13d218b5eba4a9942394674c6a3565  crates/oc-adapters/tests/mcp_remote.rs
be683aeead8e4a3f7aa15f94b7bbd104a1b10620bafcea254f10b2798d2ae3f5  crates/oc-adapters/tests/runtime.rs
14cd6d5c00c1ce9c0448a2524a23e23f61658724f9a1cb02f0b7fd362fa38f1f  crates/oc-adapters/tests/soak.rs
b331c4e441c3357c25970bdb1d6bfa5ebcc4090db03ae0308123f55e47e621c9  crates/oc-core/src/core_app.rs
3533406836d8d2822383111ace1f4972c38cf7291d9dc9a1cce7c7747c60719c  crates/oc-tui/src/app.rs
fab7f7b53bc896cf200217245bccb8c7ea35814881ad13ee3aabc9db61a3afbc  crates/oc-tui/src/shell.rs
62bdd7261bdc0c8ac4db17713a57b6a22bee1f57318880ff259827677cf28ce2  crates/oc/src/tui_cmd.rs
59f5a00d0eb2cbeae38adf372e419ef5b05c085ca5bbaa08e93fefd96b92125a  crates/oc/tests/mcp_application.rs
```

## Risks

- The actual cause of the user's `crw` failure remains unknown: no actual config
  or live endpoint was inspected. Diagnostics now distinguish safe typed causes;
  generic transport errors still cannot identify a more precise HTTP/TLS cause
  than the existing lower-layer type provides.
- PTY decoding here is a small ASCII text assertion helper, not a styled-cell
  or general Unicode terminal emulator. Raw bytes are inspected in the running
  test, not retained as a visual capture. V00's paired actual captures remain in
  `evidence/tui/recovery-v00`; parity remains unverified.
- S06 coverage here is pending-switch refusal and late event/receipt isolation.
  History tool-card replay, broader metadata/Location parity and other recovery
  slices remain open. Keymap and config-boundary behavior were not changed.
- This changes ordinary prompt submission. Manual DCP compression retains its
  existing synchronous intent/acceptance path; it was not covered by the new
  stalled-initialize prompt PTY scenarios at initial handoff. **Resolved by the
  independent-review follow-up below.**
- Remote HTTP cancellation ownership depends on the pinned rmcp lifecycle. The
  wrapper is deliberately limited to forwarding SDK HTTP operations with
  cancellation and owner-completion observation; reassess it on SDK upgrades.
- No full workspace/soak/live qualification or fresh visual comparison was run.
  Source comments were corrected after the first passing tests; the final-source
  test runs above followed those corrections and no production edits followed.

## Next

Parent owns review, workflow records and any commit/checkpoint. Include the new
`mcp_http_lifecycle.rs` as well as tracked changes and this report when recording
delivery. No commit/checkpoint, planning/progress, capture-script, active-goal or
historical-report changes were made by this implementation slice.

Review the manual compression acceptance caveat, then continue the ordered
recovery slices and fresh final qualification. Do not promote T44/VIS/full S06
to verified from these targeted tests. Preserve required MCP failures and obtain
only safe diagnostics if investigating a real `crw` configuration later.

## Independent-review follow-up

### Result

Addressed the review on the same uncommitted HEAD/worktree. No commit/checkpoint
or workflow-file changes. Final applicable scope: **344 passed, 0 failed,
2 existing live-test ignores**; fmt, affected all-targets clippy, build and diff
checks exit 0. This remains targeted V01 evidence, not full T44/VIS qualification.

1. **Manual `/dcp-compress` now shares the nonblocking receipt lifecycle.**
   `CoreApp::request_compress` enqueues the existing application message; the
   application still expands it to a normal durable turn. `TuiState` records
   compression as submission metadata and tracks the accepted compression turn.
   The binary no longer awaits `app.compress`, eagerly clears that draft, or
   maintains a second compression-turn identity. Acceptance uses the same draft
   revision rules as ordinary prompts.
2. **Observed test-before-fix:** the new actual raw PTY manual-compression test
   failed `UI frozen before acceptance: edit/resize not rendered within 2s`
   (2.13 s). Final test submits `/dcp-compress pending draft\r`, closes the DCP
   panel with Esc and waits for its rendered dismissal, then duplicate Enter,
   resize, editing and Esc cancellation. The panel wait preserves existing
   key behavior and avoids sending an ambiguous combined ESC+CR sequence.
   Before release: no durable turn/message/provider request and child gone.
   Explicit retry includes `pending draft edited` in the compression focus and
   creates exactly one turn/request and two messages. A fresh stalled manual
   compression is also shut down via raw Ctrl+C before release, with no extra
   turn/request or surviving owned PID. Final cancellation/reap measurement:
   **35.41741 ms** manual, **22.087517 ms** ordinary; both asserted below 2 s.
3. **Stdio process ownership is now explicit through actual reap.** Pinned rmcp
   `transport/child_process.rs:45–55` spawns an unjoinable kill task on Drop.
   `StdioClient` now owns the existing `SpawnedChild` (boxed to keep the runtime
   enum small), while rmcp receives only `AsyncRwTransport` pipes. Cancellation
   or initialize failure drops those pipes, terminates the owned group and
   awaits bounded `Child::wait` plus stderr-task completion before rejection.
   Group termination errors, wait failures/timeouts, and stderr-task failures
   surface as `cleanup / cleanup_failed`, not cancellation success. The fallback
   remains armed on cleanup failure. The regression fake ignores SIGTERM and
   closes stderr *before* initialize cancellation; immediately after rejection,
   `waitpid(..., WNOHANG)` must return ECHILD and the PID must be gone. A separate
   injected stderr-worker failure must be reported after the child is reaped.
4. **Protocol mismatch observes both shutdown outcomes.** The service close
   result is retained while HTTP-owner cleanup is awaited; either failure is
   reported. The service-close bound is 2 s (below pinned rmcp's independent
   5-second DELETE cleanup timeout). A fake negotiates a mismatched version and
   a session, then stalls DELETE: connect returns `CleanupFailed` and the test
   observes its TCP request close, rather than silently returning only mismatch.
5. **DNS/connect diagnostics use available reliable distinctions.**
   `webfetch::check_host` now returns `FetchError::Dns` for resolver failure or
   no addresses; private resolved addresses still return `PrivateHost`.
   reqwest's typed `is_connect` maps to `connect / connection_failed`.
   Handshake classification no longer scans an entire debug/error body for
   `401`/`403`. Pinned rmcp drops the status into a fixed `HTTP 401 ...`/`HTTP 403 ...`
   prefix when WWW-Authenticate is absent; only that prefix is examined for
   this SDK variant. Typed auth/status errors are used where retained. Tests
   exercise a local resolver-input failure, private host refusal, bound but
   non-listening loopback socket, and existing required-auth/redaction cases.
6. **Generation transitions and active-turn stale events are covered.**
   Accepted workspace/session resets invalidate pending receipts, active turn
   and compression identity and leave status Idle unless quitting. The binary
   still refuses actual busy switches. A unit test checks reset while pending;
   another performs A→B→A, accepts a new turn, edits its draft, and delivers old
   delta/tool/finished/interrupted/failed events. New draft, active turn and
   transcript remain unchanged by those events.
   Native turn IDs previously depended on session + milliseconds. They now add
   process ID and a process-wide checked atomic serial, so repeated/backward
   clocks cannot reuse an in-process ID across runtime/Location instances.
   `storage.rs` already enforces `turns.id TEXT PRIMARY KEY` before acceptance;
   it rejects historical collisions, and in-memory events do not survive process
   restart. A fixed/repeated/backward-clock identity test covers the allocation.
   No generation fields were added to every CoreEvent.

### Checks — follow-up command ledger

Aliases P/V/M/U/A/C retain the exact expansions above. Additional aliases:

```text
D = cargo test --locked -p oc --test mcp_application v01_manual_compress_raw_pty_cancel_shutdown_and_retry -- --nocapture
R = cargo test --locked -p oc-adapters --test mcp_remote protocol_mismatch_observes_stalled_session_cleanup -- --nocapture
L = cargo test --locked -p oc-adapters --lib --test mcp_remote --test mcp_stdio --test runtime
B = cargo test --locked -p oc --bin oc --test mcp_application -- --nocapture
```

| Command | Exit | Result |
|---|---:|---|
| D before manual-path fix | 101 | Observed frozen UI; 1 failed, 11 filtered, 2.13 s. |
| `cargo fmt --all && V` | 101 | fmt 0; compiler E0308: new ID helper needed `&params.session`. No tests ran. |
| `cargo fmt --all && V` after implementation | 101 | 3 passed/2 failed: ambiguous ESC+CR kept DCP panel open; typed-only classifier lost SDK's untyped 401 prefix. Both corrected, no assertion removed. |
| L, first follow-up run | 101 | 153 library passed; remote 21 passed/2 failed/1 ignored: unauthorized class and stalled-cleanup fake timeout. Stdio/runtime suites not reached. |
| `U && cargo test --locked -p oc --bin oc` | 101 | New stale-event test passed `&str` where `&CoreError` is required; corrected. Binary tests not reached. |
| R with safe fixture method/error tracing | 101 | 8-second fake timeout: optional GET connected then closed before writing a request; fake did not handle first-line EOF. Fixed fake's EOF handling and removed temporary tracing. |
| D with panel-dismissal synchronization | 0 | 1 passed, 11 filtered, 0.47 s; cancellation/reap 33.033446 ms. |
| `cargo fmt --all && L` | 0 | 153 library, 23 remote +1 ignored, 11 stdio +1 ignored, 35 runtime passed. |
| `cargo test --locked -p oc-core -p oc-tui --lib && B` | 0 | 17 core, 91 TUI, 2 binary, 12 integration passed. |
| `cargo fmt --all -- --check && C && cargo build --locked -p oc && git diff --check` | 101 | fmt 0; clippy large-enum-variant after retaining Child. Boxed the owned child; no lint suppression. Build/diff commands not reached. |
| Same fmt/clippy/build/diff chain after boxing | 0 | Every command exited 0. |
| `L && B` after boxing | 0 | All commands exit 0; library 153, remote 23+1 ignored, stdio 11+1 ignored, runtime 35, binary 2, integration 12 passed. Final PTY measurements above. |
| `git diff --stat && git diff -- crates/oc-adapters/src/webfetch.rs && git rev-parse HEAD` | 0 | Confirmed narrow DNS helper change and unchanged HEAD. |
| `git diff --binary HEAD \| sha256sum && sha256sum <source paths below> target/debug/oc` | 0 | Final association below; no source changes after these checks. |
| `ps -C stalled-mcp -o pid,ppid,state,comm` | 0 | No stalled fixture process listed. |
| `git diff --check && git status --short` after report update | 0 | Clean diff check; expected source/report changes and supplied untracked ZIP only. |

The library stderr-worker panic is intentional fault injection in a spawned,
joined test task; its resulting cleanup error is asserted, not suppressed.
Core/TUI sources did not change after their passing run; subsequent edits were
the boxed stdio owner and the PTY timing label, both rechecked above.

### Follow-up source association (supersedes initial hashes)

HEAD remains `679e683121722c0a00aac7acd95f2be91190ad9e`.
Final tracked `git diff --binary HEAD` SHA-256:
`e4651980f2f7d393a4bf92e954e80de6afa0096a643df661420ce7e34c55c72e`.
The report and supplied ZIP are excluded. New untracked lifecycle source is
included by its separate file digest. Complete changed-source manifest:

```text
17bebba5cccc68cfd8942f7cc60edd9b3026a613491434da2bfe615978b1e091  Cargo.lock
e5e3d488f5381f9b7452d51981a42096fc370f5e2f074c56c37e525210b9e76a  Cargo.toml
282db8142f682e2d0467c6ab56f8a9d3d50c8b46d382752478664f237fbf0fbf  crates/oc-adapters/Cargo.toml
92a574af80ddc88ea767c617420da060d1cee2cdfa9e5858a2e5fd13f1d54a25  crates/oc-adapters/src/application.rs
664e513bdcf8780c9cbc513f2a06d4a7c6c6dbb644ecf3caf446465afceda20c  crates/oc-adapters/src/mcp_remote.rs
797bb9a8ff61dd98ddc95ffca10da844321bc0a2fe64c63b1c69ad1b3f9775c3  crates/oc-adapters/src/mcp_http_lifecycle.rs
67b84ee55e604ea4b73fdd9e2e225663b0870fcc0822e4133fdd82853cbc193e  crates/oc-adapters/src/mcp_stdio.rs
5738c8585e10ce662a318fab5683acb86866766a5cc4d15fc12033fedbd59b55  crates/oc-adapters/src/runtime.rs
4d82ea525e03f1c0872106327e085bbaec723360599a89f3450a7176f61e74b7  crates/oc-adapters/src/webfetch.rs
4fda5f9d97673c74aab9fe95b294a6b315585764ceb6836bef444839776dcd9b  crates/oc-adapters/tests/mcp_remote.rs
e5d6fa850b89bb1e1896674798208166c3c45f20c1f5267a092c753c0d9835e1  crates/oc-adapters/tests/mcp_stdio.rs
be683aeead8e4a3f7aa15f94b7bbd104a1b10620bafcea254f10b2798d2ae3f5  crates/oc-adapters/tests/runtime.rs
14cd6d5c00c1ce9c0448a2524a23e23f61658724f9a1cb02f0b7fd362fa38f1f  crates/oc-adapters/tests/soak.rs
0a2efe708ed03edd96f06842e8a70adeabb0b5b05989f4921523ec548b3b2e1c  crates/oc-core/src/core_app.rs
ab7a0bf89eb01a9be0621eb856f36633566511d427d330cb92030b8ac781cb6e  crates/oc-tui/src/app.rs
fab7f7b53bc896cf200217245bccb8c7ea35814881ad13ee3aabc9db61a3afbc  crates/oc-tui/src/shell.rs
5fe50b85bf4e381fedffbbf7405b2733f4ace46590531a92c46154b00acff51c  crates/oc/src/tui_cmd.rs
074db251ecadcbf130bebca260f8b2c27e8138da9f56c70cc8b672cf79d06154  crates/oc/tests/mcp_application.rs
15ede60e1d4b67d9826577383e6c3d47a3ec2de8dc0d3f0f7b794a4597030989  target/debug/oc
```

### Risks / Next

- The review's pre-existing **in-flight tools/call** cancellation issue remains
  explicitly open for **V07 / S08**: dropping the call future does not necessarily
  cancel the SDK request; inspect/use explicit `RequestHandle::cancel` there.
  This follow-up covers preacceptance initialize ownership and shutdown.
- Full S06 history/replay, other slices, full qualification and pixel parity
  remain open. The actual `crw` failure is still unknown; no real config was read.
- Parent should review this updated worktree/report and own any workflow record
  or commit. Preserve both failure histories. No suppression, fallback disabling,
  keymap/config-boundary change, or CoreEvent generation expansion was used.
