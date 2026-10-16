# T37 executed checks

All MCP/provider endpoints were loopback fakes or local fixture processes; all
data roots were temporary. No live/paid calls, real credentials or valuable user
files were used. Initial failure evidence: [regression.md](regression.md).

## Targeted suites after the final hardening edit

```sh
cargo fmt --all && \
cargo test --locked -p oc-adapters --test runtime --test mcp_remote --test mcp_stdio && \
cargo test --locked -p oc --test mcp_application && \
cargo test --locked -p oc-core --lib && \
cargo test --locked -p oc-adapters --lib
```

Exit 0: runtime 31/31, remote MCP 20 + 1 pre-existing ignored live harness,
stdio MCP 10 + 1 pre-existing ignored real-server harness, actual-binary MCP
7/7, core 17/17, adapter unit 124/124.

New regressions in this slice, each failing before the fix:

- `aud23_tool_list_changed_relists_only_the_dirty_server`: a dirty server must
  not force an unrelated relist (stable server stays at one `tools/list`), and a
  notification arriving during relist stays pending (dirty server lists 3 times
  across 3 turns).
- `aud23_aborted_turn_releases_lease_and_shutdown_reaps_child`: dropping an
  in-flight turn future must release the single-flight lease, and the
  generation-owned stdio child must be reaped by shutdown.
- `aud23_server_cap_blocks_spawn_before_first_child`: 9 enabled servers fail
  with an actionable diagnostic before any child is spawned and before input is
  accepted.
- `worker_guard_surfaces_cleanup_failure` (oc-core): a worker cleanup error or a
  panicked worker is reported, never silent success.

## Final mandatory gate

```sh
cargo test --workspace --locked && \
cargo clippy --locked --workspace --all-targets -- -D warnings && \
cargo fmt --all -- --check && cargo build --locked && target/debug/oc --help && \
python3 scripts/progress.py check && python3 scripts/check_docs.py && git diff --check
```

Exit 0. Workspace results:

| Suite | Passed | Ignored |
|---|---:|---:|
| oc unit | 1 | 0 |
| actual application | 1 | 0 |
| configured workspace | 6 | 0 |
| actual DCP runtime/crash | 2 | 0 |
| actual durability | 1 | 0 |
| actual MCP application | 7 | 0 |
| PTY | 13 | 0 |
| actual Responses | 4 | 0 |
| adapter unit | 124 | 0 |
| blob audit | 11 | 0 |
| DCP atomic | 4 | 0 |
| offline E2E | 3 | 0 |
| remote MCP | 20 | 1 |
| stdio MCP | 10 | 1 |
| patch audit | 10 | 0 |
| runtime | 31 | 0 |
| soak | 4 | 0 |
| storage lock | 1 | 0 |
| core | 17 | 0 |
| TUI | 31 | 0 |

Exactly three pre-existing external harnesses were NOT RUN, not PASS:
`live_workflow_harness`, `live_search_harness`, `real_server_smoke`. No T37 test
is ignored. Doc-tests have zero cases and pass. Workspace clippy, fmt check,
locked build, binary help, progress/docs structural checks and diff check pass.

No Cargo manifest/lockfile, package count, SQLite schema version or public
`Runtime` API used by the application was changed. The `WorkerGuard::join`
signature now returns `Result<(), String>` so cleanup failures surface instead
of being discarded.
