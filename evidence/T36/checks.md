# T36 executed checks

All provider/MCP endpoints were bounded loopback fixtures and all workspaces/data
roots temporary. No live/paid calls or real credentials. Initial actual-binary
failure is recorded in `regression.md`.

## Targeted final checks

```sh
cargo fmt --all && \
cargo test --locked -p oc-adapters --lib && \
cargo test --locked -p oc-adapters --test runtime && \
cargo test --locked -p oc-adapters --test dcp_atomic && \
cargo test --locked -p oc --test dcp_runtime
```

Exit 0: adapter unit 124/124 (2.40 s), runtime 26/26 (4.09 s), atomic
compression 4/4 (.08 s), actual-binary DCP 2/2 (2.96 s). The binary crash test
uses a temporary compiled `LD_PRELOAD` shim to pause the actual SQLite DB/WAL
durability sync after durable intent; it then SIGKILLs only the fixture `oc`.

A full workspace run then found the old soak workload attempting to start a
second compression at a raw member already represented by an active block. The
workload was corrected to use the effective block anchor (the public nested-range
contract), not bypass overlap validation. Targeted soak 4/4 passed in 40.15 s.

## Final mandatory gate after last edit

```sh
cargo fmt --all && \
cargo test --locked -p oc-adapters --test soak && \
cargo test --workspace --locked && \
cargo clippy --locked --workspace --all-targets -- -D warnings && \
cargo fmt --all -- --check && \
cargo build --locked && target/debug/oc --help && \
python3 scripts/progress.py check && python3 scripts/check_docs.py && \
git diff --check
```

Exit 0. Executed workspace results:

| Suite | Passed | Ignored |
|---|---:|---:|
| oc unit | 1 | 0 |
| actual application | 1 | 0 |
| configured workspace | 6 | 0 |
| actual DCP runtime/crash | 2 | 0 |
| actual durability | 1 | 0 |
| PTY | 13 | 0 |
| actual Responses | 4 | 0 |
| adapter unit | 124 | 0 |
| blob audit | 11 | 0 |
| DCP atomic | 4 | 0 |
| offline E2E | 3 | 0 |
| remote MCP | 15 | 1 |
| stdio MCP | 6 | 1 |
| patch audit | 10 | 0 |
| runtime | 26 | 0 |
| soak | 4 | 0 |
| storage lock | 1 | 0 |
| core | 16 | 0 |
| TUI | 31 | 0 |

Exactly three pre-existing external harnesses were NOT RUN, not PASS:
`live_workflow_harness`, `live_search_harness`, `real_server_smoke`. No T36 test
is ignored. Doc-tests have zero cases and pass. Workspace clippy, fmt check,
locked build, binary help, journal/docs structure and diff check all pass.

No Cargo manifest/lockfile, package count or schema migration version changed.
SQLite additions are idempotent tables in the existing adapter-owned schema.
Workspace tests do not close T37–T42 or the live campaign.
