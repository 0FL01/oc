# T04 — SQLite и outputs

Status: PASS. Implementation commit: `09320d44fd1377fd8e05e2bd78f9288d61f052e3`. Method: offline `cargo` execution with `tempfile` isolation; no network, no live requests, no Docker, no upstream DB touched.

## STORE02 Lock/atomicity — PASS

- Own data-root lock: `oc.lock` advisory exclusive `try_lock_exclusive` held for `Db` lifetime, never deleted by PID; second `Db::open` on same root gives typed `DataRootBusy` (`second_owner_is_refused`).
- Owned no-follow root + restrictive permissions: absolute-only, refuse `/`, `/tmp`, `..`, symlink root; existing dirs keep their mode and `0700`-only passes (`mode & 0o077 == 0`), fresh dirs created `0700`; owner uid equals `getuid` (`// SAFETY: getuid` noted). Permissive `0777` existing root refused; fixed-before-check bug found by test and corrected (no silent chmod repair).
- State/event transactional: `append_message` inserts message + `message` event in one SQLite transaction; `create_session` inserts session + `session_created`; durable ack only after commit. WAL + `synchronous=FULL` + `foreign_keys=ON`; schema v1 (`sessions/messages/turns/tool_operations/events/blobs`).

## STORE03 Crash operations — PASS

- Intent before side effect, outcome after: `begin_turn`/`record_tool_intent(started)` precede work; `finish_turn`/`record_tool_outcome` follow it.
- Kill after intent before/after outcome: `crash_intent_recovers_unknown_without_replay` drops `Db` with `started` op, reopens, `recover_interrupted_tools` marks exactly 1 row `unknown`, second recovery 0 rows; no autoreplay (no new execution, state stays `unknown` until explicit new op id).
- Unknown/partial never replayed by recovery path.

## STORE04 Blob/root failures — PASS

- Blob-before-DB orphan: `write_blob` order quota → temp/write/fsync/rename → DB row; crash file-without-row is a safe orphan. `blob_orphan_collected_referenced_kept` proves manual orphan collected with `ZERO` grace while referenced digest survives; second GC 0 removals (no referenced deletion).
- Unsafe root + cleanup escape refused: `/`, `/tmp`, relative, `..` refused; symlink root refused; symlink inside `blobs/` pointing outside is skipped by GC and outside victim survives (`symlink_root_and_escape_not_followed`).
- Quotas enforced: `open_with_quota(16)` writes 8 bytes OK, second 9+ bytes gives `StorageFull`; default quota 2 GiB. Real `ENOSPC` injection deferred to soak; quota is the bounded-failure proxy in unit scope.
- Read-only/misuse: permissive-mode refusal covered; `BlobNotFound` on bad digest; `SessionNotFound` on unknown session. New deps licenses: `fs2 0.4.3 MIT/Apache-2.0`, `sha2 0.10.9 MIT OR Apache-2.0`, `libc 0.2.189 MIT OR Apache-2.0`, `tempfile 3.27.0 MIT OR Apache-2.0` (dev-only in production closure except test use).

## Checks

- `cargo fmt --all -- --check` — exit 0.
- `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets -- -D warnings` — exit 0 (fixed `suspicious_open_options` via `truncate(false)`, `to_string_in_format_args`, `collapsible_if`).
- `CARGO_BUILD_JOBS=2 cargo test -p oc-adapters --locked storage` — exit 0, 7/7 (busy, durable, crash, blob, unsafe, symlink, quota).
- `CARGO_BUILD_JOBS=2 cargo test --workspace --locked` — exit 0, 27 total (core 13 + adapters 12 + tui 2).
- `cargo build --locked`, `target/debug/oc --help` — exit 0. `python3 scripts/check_docs.py` — exit 0.

## Scope and limitations

- Sync `Db` API; async worker-queue wrapping and headless/resume wiring remain T05. No second-history copy; UI/prompt assembly untouched.
- Disk-full is quota-proxy in unit tests; physical `ENOSPC`/`SQLITE_FULL` fault injection belongs to T28 soak.
- No upstream DB migration performed or claimed.
