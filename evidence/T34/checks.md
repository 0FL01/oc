# T34 executed checks

All endpoints were offline loopback fixtures; all data roots temporary. No live,
paid probes or real credentials. Initial failures: [regression.md](regression.md).

## Narrow checks

- First integrated slice: `cargo fmt --all && cargo test --locked -p oc --test responses && cargo test --locked -p oc-adapters --test runtime && cargo test --locked -p oc-adapters --lib provider::tests` exit 0: 3 binary, 16 runtime, 13 provider tests.
- Extended runtime/consumer check: `cargo fmt --all && cargo test --locked -p oc-adapters --test runtime --lib && cargo clippy --locked --workspace --all-targets -- -D warnings` exit 0: 107 adapter unit, 19 runtime; clippy 5.43 s.
- Actual CLI/PTY/crash smoke rerun after typed wire: application 1, durability 1,
  PTY 13 passed. Initial old crash/PROV03 fixture failures corrected as documented.
- New actual-binary oversized-line diagnostic: responses 4/4, exit 0, 0.42 s.
- Root-lock regression: parent `cargo test --locked -p oc-adapters --test storage_lock`
  exit 0, 1/1, 0.05 s (prior red executed by delegated workstream).

## Final gate after the last code/test edit

```sh
cargo fmt --all && cargo test --locked -p oc-adapters --test soak && cargo test --workspace --locked && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo fmt --all -- --check && cargo build --locked && target/debug/oc --help
```

Exit **0**. Narrow soak 4/4 in 25.86 s. Workspace compile 1.77 s;
bounded test-summary transcription from captured tool output:

| Executed suite | Passed | Time |
|---|---:|---:|
| oc unit | 1 | .04 s |
| actual binary application | 1 | .15 s |
| actual binary SIGKILL durability | 1 | .24 s |
| PTY / CLI restart | 13 | 8.06 s |
| actual binary Responses | 4 | .42 s |
| adapters unit | 107 | 2.22 s |
| blob audit | 11 | .29 s |
| offline E2E | 3 | .66 s |
| remote MCP | 15 | .48 s |
| stdio MCP | 6 | 1.32 s |
| patch audit | 10 | .03 s |
| runtime | 19 | 3.32 s |
| soak | 4 | 25.67 s |
| ownership lock regression | 1 | .05 s |
| core | 16 | .02 s |
| TUI | 31 | .26 s |

All executed tests passed. Exactly three pre-existing external-only harnesses
remain ignored: `live_workflow_harness`, `live_search_harness`, `real_server_smoke`.
They are NOT RUN, not PASS. No new ignored test. Doc tests passed (0 cases).
Clippy exit 0 (4.85 s); fmt check exit 0; locked build exit 0 (3.59 s).
`oc --help` exit 0, commands run/sessions/tui/help.

Shared protocol/storage consumers justify the mandatory workspace expansion.
Earlier workspace failures were investigated, not hidden by retries/serialization
or altered resource thresholds. Existing soak PASS does not qualify T40/A10.

Final code/diff review covered provider parser/request/options, wire persistence,
durable tool outcomes, lock ownership, application/CLI cancellation and all direct
fixtures. `git diff --check` exit 0. No Cargo/dependency/schema changes. No source
or secret dump was saved in evidence. Registry checks are structural only.
