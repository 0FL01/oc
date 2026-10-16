# T03 — Core и MockProvider

Status: PASS. Implementation commit: `3261fda475999508fda8cf85696f1c04fd07bd73`. Method: offline `cargo` execution with scripted provider; no network, no SQLite, no live requests, no Docker.

## Work — PASS

- Typed slice added additively in `oc-core` (T01 smoke untouched): `session.rs` (`TurnId`, `MessageId`, `Role`, `Message`, `CoreError` with `SessionAlreadyExists`, caps `MAX_QUEUE_ITEMS 256` / `MAX_QUEUE_BYTES 8MiB ref` / `MAX_INPUT_BYTES 1MiB smoke`), `core_app.rs` (`CoreApp` bounded inbox, `MockProvider` echo/fixed, single-turn `SessionWorker` with `select!` inbox vs chunk timer, `CoreEvent` Started/Delta/Finished/Interrupted).
- Vertical `input→response` without network: `create_session → submit → TextDelta* → TurnFinished → read_history` verified; user message committed on accept, assistant committed only on finish; interrupted partial never committed.
- Cancellation path: `CancelTurn` during streaming clears the active turn, emits `TurnInterrupted` with partial, worker stays usable (`t0002` after cancel in test). Queries (`list_sessions`, `read_history`) stay responsive during streaming via the same `select!`; second `submit` while active gives typed `TurnBusy`, not silent queueing.
- Bounded channels: inbox `mpsc::channel(256)` (test override `spawn_with_capacity`), events `broadcast::channel(256)`; `try_submit` maps full inbox to `QueueFull`; single-input `InputTooLarge` checked before enqueue. Full queue-pressure/soak deferred to T28 per TEST_PLAN.
- No premature STORE01 claimed: persistence/resume remain T04/T05; this is an in-memory component slice per `roadmap/M1.md`.

## Checks

- `cargo fmt --all -- --check` — exit 0.
- `CARGO_BUILD_JOBS=2 cargo clippy --workspace --all-targets -- -D warnings` — exit 0 (fixed `map_entry` via `Entry::Vacant`).
- `CARGO_BUILD_JOBS=2 cargo test -p oc-core --locked` — exit 0, 13 tests (6 T01 smoke + 2 session + 5 worker: vertical, cancel, busy, errors, responsive-queries).
- `CARGO_BUILD_JOBS=2 cargo test --workspace --locked` — exit 0 (20 total: 13 core + 5 adapters + 2 tui).
- `cargo build --locked` and `cargo build` — exit 0; `target/debug/oc --help` — exit 0.
- `python3 scripts/check_docs.py` — exit 0; `git diff --cached --check` exit 0.

## Scope and limitations

- Single active turn globally (worker-wide), not per-session concurrency; sequential tool execution and multi-session parallelism arrive later.
- In-memory history only; no SQLite durability, blobs, crash recovery, headless CLI or TUI wiring yet (T04–T06).
- Mock delays (5–50 ms) are cooperative cancellation points; real provider timeouts/cancel arrive in M3.
