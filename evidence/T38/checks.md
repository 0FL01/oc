# T38 checks

Environment: `cargo`/`rustc` 1.93.0, offline workspace, loopback-only fixtures,
temporary directories under `TMPDIR`. No live provider calls, no real
credentials, no mutation of user files.

## Targeted

| Command | Result |
| --- | --- |
| `cargo test -p oc-adapters --lib webfetch` | ok, 10 passed (incl. `aud25_html_unicode_entities_and_malformed`, `aud25_url_relative_query_ipv6`, `aud26_dial_bound_private_answer_sends_no_request`, `aud26_total_deadline_spans_redirects`) |
| `cargo test -p oc-adapters --lib shell::tests` | ok, 7 passed (incl. `aud28_strict_allowlist_drops_unlisted_names`, `aud28_symlink_cwd_escape_refused_and_toolchain_resolves`) |
| `cargo test -p oc-adapters --test shell_watchdog` | ok, 2 passed / 4.1 s (stdin-unread, flood, leader-exit descendant + marker, TERM-ignoring, all under the 30 s watchdog) |
| `cargo test -p oc-adapters --test e2e_offline` | ok, 3 passed (seeded coding fix, seeded test failure, duplicate-apply) |
| `cargo test -p oc-adapters --test soak` | ok, 4 passed |
| `cargo test -p oc-adapters --test runtime` | ok, 31 passed |
| `cargo test -p oc-adapters --test e2e_offline e2e01_seeded_coding_fix -- --exact` ×3 | ok each run, 0.61–0.65 s (stability after the transient `reap failed`) |

## Full workspace

`cargo test --workspace --locked` → exit 0, **308 passed, 0 failed**:

| Suite | Passed |
| --- | --- |
| oc unit (`src/main.rs`) | 1 |
| oc `application` | 1 |
| oc `configured_workspace` | 6 |
| oc `dcp_runtime` | 2 |
| oc `durability` | 1 |
| oc `mcp_application` | 7 |
| oc `pty` | 13 |
| oc `responses` | 4 |
| oc-adapters unit | 129 |
| oc-adapters `blob_audit` | 11 |
| oc-adapters `dcp_atomic` | 4 |
| oc-adapters `e2e_offline` | 3 |
| oc-adapters `mcp_remote` | 20 |
| oc-adapters `mcp_stdio` | 10 |
| oc-adapters `patch_audit` | 10 |
| oc-adapters `runtime` | 31 |
| oc-adapters `shell_watchdog` | 2 |
| oc-adapters `soak` | 4 |
| oc-adapters `storage_lock` | 1 |
| oc-core unit | 17 |
| oc-tui unit | 31 |

Ignored: exactly the three pre-existing external harnesses (`e2e_live`,
`mcp_remote` live, `mcp_stdio` live) — not run, not counted as passing.

## Static and structural

| Command | Result |
| --- | --- |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo build --locked` + `target/debug/oc --help` | exit 0 |
| `python3 scripts/progress.py check` | OK (journal structure only) |
| `python3 scripts/check_docs.py` | OK |
| `git diff --check` | clean |
