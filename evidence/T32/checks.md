# T32 executed checks

Date: 2026-09-21. Base HEAD: `e92b614`; checked runtime diff is the T32
implementation commit identified in report.md. All product tests offline.

## Narrow regression / integration

Initial red command/results: [regression.md](regression.md).

Final command (exit **0**):

```sh
cargo fmt --all && cargo test --locked -p oc-adapters --test patch_audit && cargo clippy --locked --workspace --all-targets -- -D warnings
```

Patch integration: **10 passed**, 0 failed/ignored, 0.03s. Clippy: exit 0, 3.14s.

```text
aud03_add_file_decodes_the_patch_plus_prefix ... ok
aud03_pinned_upstream_grammar_independent_bytes ... ok
aud03_ambiguous_hunks_and_overlapping_paths_are_preflight_conflicts ... ok
aud04_predictable_temp_symlink_must_not_modify_outside_file ... ok
aud04_parent_symlink_and_replacement_do_not_escape ... ok
aud04_concurrent_add_and_move_targets_are_not_overwritten ... ok
aud05_late_stale_hunk_must_not_commit_earlier_file ... ok
aud05_concurrent_preimage_is_preserved ... ok
aud05_io_failure_after_commit_reports_precise_partial_outcome ... ok
aud05_late_filesystem_permission_failure_is_preflight ... ok
```

The parent-handle race test additionally stages a file, replaces its pathname
parent with an outside symlink, then commits through the pinned handle; outside
sentinel bytes remain unchanged. Included in adapters lib/workspace tests.
The schema/tool route regression rejects old keys and creates exact bytes via
`patchText`; partial-output regression asserts full hashes and failure prefix.
TUI/DCP consumers parse canonical move order and include target paths.

An intermediate original TOOL04 test failed because it expected stale-hunk
partial writes. T32 explicitly supersedes that erroneous expectation: it now
requires no commits; an actual syscall I/O failure separately proves partial
outcome. Original test IDs retained. No failing test suppressed.

## Required wider gates (shared descriptor and consumers changed)

Command (exit **0**):

```sh
cargo test --workspace --locked && cargo fmt --all -- --check && cargo build --locked && target/debug/oc --help
```

Bounded command output summary:

| Suite | Executed result |
|---|---|
| oc unit | 1 passed |
| actual binary application AUD01 | 1 passed |
| actual binary PTY, AUD02/restart/UI05 | 13 passed, 8.10s |
| adapters unit, including TOOL02–04/schema/handle race/DCP | 100 passed, 2.49s |
| offline coding/config/DCP E2E | 3 passed |
| remote MCP fixture | 15 passed, 1 existing external test ignored |
| stdio MCP fixture | 6 passed, 1 existing external test ignored |
| T32 integration | 10 passed |
| Runtime integration | 12 passed |
| soak | 4 passed, 11.89s |
| core | 16 passed |
| TUI | 31 passed |
| doc tests | 0 tests, successful |
| fmt check | exit 0 |
| locked build | exit 0, 2.60s |
| actual oc --help | exit 0; run/sessions/tui/help shown |

Three pre-existing external harnesses remain **NOT RUN**, not PASS:
`live_workflow_harness`, `live_search_harness`, `real_server_smoke`.
No new ignored test. No paid/live probe. Test counts are bookkeeping, not product
readiness; existing suites do not close other audit findings.

`git diff --check` exit 0; final runtime/schema/consumer diff reviewed once after
successful gates. Cargo.lock, dependency lists and four-package boundary unchanged.
