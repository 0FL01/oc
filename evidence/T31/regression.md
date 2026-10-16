# T31 — initial actual-binary regression

Runtime baseline: `e4c7036` (runtime unchanged from audited `fc796830`).
Test: `crates/oc/tests/application.rs::aud01_binary_sends_configured_responses_request`.

## Executed check

`cargo test --locked -p oc --test application -- --nocapture` — **exit 101**.

```text
running 1 test
binary never contacted configured endpoint
configured request must reach endpoint: Any { .. }
test aud01_binary_sends_configured_responses_request ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.01s
```

The test starts the actual Cargo-built `oc` as a child with cleared environment,
temporary HOME/XDG/project, a configured unknown static model, env-substituted
fixture credential and a bounded loopback HTTP listener. It expects a Responses
POST with the configured prefix/model/credential and a non-echo answer. The first
assertion failed: the child never contacted the endpoint. Later assertions for
the response, own XDG data directory and missing-config rejection were **not
reached** and are not PASS evidence.

After recording this failure, the fixture model received explicit context/output
limits required by the existing Runtime admission contract. No runtime changed;
the command has not yet been rerun. This fixture adjustment does not change the
failure under test (no HTTP request from the normal binary).

## Confirmed cause and next edit

`bootstrap.rs` passes `MockProvider::echo()` to headless; `tui_cmd.rs` independently
spawns the same mock worker. Neither normal path constructs the existing
`oc_adapters::runtime::Runtime`. Headless defaults to `temp_dir()/oc-t05-<uid>`.
Both frontends write storage themselves and treat any SQLite session-create
error as duplicate; TUI additionally suppresses read/append errors and persists
input even when the application rejected it.

Next: expose the existing core command/event boundary to an adapter-owned worker,
construct config/catalog/Responses/Runtime once through a shared factory, and
make both frontends consumers rather than persistence owners. Then execute
AUD01 and extend actual PTY regression for AUD02, including restart context.
T31 remains active; no finding is claimed fixed. No paid/live call or file-tool
mutation on user data was performed.
