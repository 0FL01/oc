# T31 — native application wiring

Implementation commit: `7812379` (`fix(app): connect CLI and TUI to native runtime`).
Audit baseline: `fc796830cf336655bebf63b591fe0fd36cdd4310`.
T31 baseline: `e4c7036`; runtime was unchanged from the audit.

**Task result: PASS for F01 / AUD01 / AUD02. Product readiness: NOT READY.**
No finding outside F01 is closed by this report; T32–T42 remain required.

## Implemented path

Both normal `oc run` and `oc tui` call `oc_adapters::application::spawn`:

1. `composition::load` loads global then project then project `.opencode`
   JSON/JSONC, substitutes configured environment values, validates the selected
   provider, and resolves exact `provider/model-id` against the catalog. Native
   discovery reuses the existing discovery adapter; no model ID is hardcoded.
2. A single native worker owns Db and Runtime behind the existing CoreApp typed
   inbox/events. No MockProvider is constructed by normal CLI/TUI paths.
3. Runtime acknowledges accepted input only after durable user append. It owns
   permission checks, turn transitions and assistant persistence. Frontends no
   longer append history, suppress history read errors, or classify every SQLite
   error as a duplicate. The exact session primary-key collision has a typed
   storage error. Failed accepted turns emit a distinct failure event.
4. SSE text is observed as it arrives and forwarded to both frontends, rather
   than waiting for the complete report. This preserves the prior UI streaming
   behavior during the switch from the mock path. The remaining provider memory,
   typed-protocol and silent-peer cancellation repairs belong to T34/T40.
5. Default data root is `$XDG_DATA_HOME/oc`, falling back to
   `$HOME/.local/share/oc`. `--data-dir` remains available. The upstream OpenCode
   database is not opened; the old `oc-t05-*` temporary default is gone.

The core still provides its explicitly selected scripted worker for unit tests;
it is not a normal-run fallback. The obsolete headless test-only persistence
driver was removed. STORE01/UI05 were moved onto the actual-binary fake HTTP
fixture rather than left passing through a duplicate test runtime.

## Regression / finding mapping

| Finding / acceptance | Executed evidence | Result |
|---|---|---|
| F01, AUD01 | `crates/oc/tests/application.rs::aud01_binary_sends_configured_responses_request` | Initial exit 101: no HTTP request. On implementation, actual subprocess sends POST to configured prefix with unknown fixture model and env-derived Bearer; non-echo answer; missing config fails; own XDG storage exists. |
| F01, AUD02, STORE01 | `crates/oc/tests/pty.rs::aud02_store01_persist_resume_across_restart` | Three separate CLI→PTY→CLI processes share project/data/session. PTY renders prior user/assistant history. Captured second and third HTTP requests contain exact persisted history, and final Db contains six expected messages. |
| T31 UI acceptance invariant | `pty_escape_cancels_heartbeat_request` | Visible partial answer arrives while the turn is active. Busy input is rejected and never persisted; cancel retains only accepted user input. |
| UI05 preserved on actual binary | `ui05_ndjson_stdout_only_and_slow_consumer`, `ui05_interrupted_exit_is_nonsuccess` | NDJSON is parseable and diagnostics separate; slow reader works. SIGINT after first observed delta exits 130, with accepted user input but no completed assistant persisted. |
| Existing PTY behavior | Nine original PTY tests, now configured native HTTP | Unicode/paste, resize, tiny/SSH-like terminal, history, slow output consumer, panic restoration and no-TTY behavior pass without product echo provider. |

All binary tests clear inherited config/environment and use temporary project,
HOME and XDG/data roots. Loopback access requires explicit test-only
`OC_TEST_ALLOW_LOOPBACK=1`; normal URL checks were not relaxed. Fixture credentials
are non-secret constants. No mutation tool ran on valuable user data.

## Verification

Command results and bounded output excerpts: [checks.md](checks.md).
Initial failure: [regression.md](regression.md).

- Actual binary AUD01 — PASS.
- Actual binary/PTY suite — PASS, all 13 executed tests.
- `cargo fmt --all -- --check` — exit 0.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — exit 0.
- `cargo test --workspace --locked` — exit 0; all executed groups pass.
- `cargo build --locked` and `target/debug/oc --help` — exit 0.
- `git diff --check` and documentation/journal structural checks — exit 0.

Three pre-existing external-only test harnesses remained ignored, not passed:
live workflow, live codex_web search, selected real stdio server. No new ignored
tests, no paid/live probes, no changes to acceptance expectations or Cargo.lock.
Four packages and the core→no-UI dependency boundary are preserved.

## Remaining product gaps / exact continuation

- **T32/T33 P0:** patch filesystem safety/grammar and durable mutation/recovery
  remain unresolved. Test these only on temporary fixtures.
- **T34:** typed Responses items/call IDs, complete terminals, faithful tool and
  restart continuation, bounded buffering and cancellation while the peer is
  silent still need their audit regressions. Current cancellation evidence uses
  incoming heartbeats; it is not AUD09–AUD13. Rerun AUD01/AUD02 after that repair.
- **T35/T36/T37:** composition currently reuses existing config/discovery/native
  profile semantics. Full trust/definitions/AGENTS/commands/skills/plugin mapping,
  real DCP request integration and MCP generation lifetime remain their repairs.
  File substitution is refused without explicit source trust rather than allowed.
- **T38–T40:** tool, UI panel routing and byte/lifetime/history bounds are not
  qualified by this wiring task. Runtime and initial UI history still materialize
  whole history, and the old 64 KiB prompt assembly remains.
- **T41/T42:** replace remaining module-only qualification with actual-binary
  evidence, then run mandatory offline audit qualification. Existing workspace
  suite success does not override unresolved audit findings.

Next task: **T32**, starting with its supplied patch safety/grammar regressions.
T27 remains blocked by product qualification, not credentials. No READY or
BUILD_READY_LIVE_BLOCKED claim; no push performed in T31.
