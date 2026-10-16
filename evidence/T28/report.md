# T28 — Soak и ресурсы (LOAD01–04)

Status: PASS (offline synthetic; no live needed).

## Workload (frozen, deterministic)

`crates/oc-adapters/tests/soak.rs`, scripted SSE fake, zero delay:
6 sessions × 10 mixed turns (text 2 KiB / read 300 KiB / bash /
patch / short text) across 2 epochs on one `Db`; compress every 12th
turn; one mid-stream cancel; one loud MCP-attach failure per epoch.
Kinds, order, and bytes fixed — no time/randomness in assertions.

## LOAD01 Soak — PASS

- 120 turns Completed, 36 tool calls/epoch, round-robin switches,
  compress blocks non-empty, cancel → Cancelled, MCP failure →
  `McpAttach{server:"codex"}` with no partial turn.
- Crash injection (watchdog drop mid-stall): user message durable,
  recovery runs, fresh runtime restarts and completes on the same Db —
  clean shutdown (test exit = no hang/leak of tasks).

## LOAD02 Output pressure — PASS

- 500×1 KiB deltas, 300 KiB read, 10 KiB bash arg: all Completed.
- One transcript: 3 user + 3 assistant messages, no second transcript.
- Report/prior carryover capped at 2048+marker; durable log keeps full
  outputs by design (archive unbounded, context bounded).
- Blob quota: over-quota → `StorageFull`, store usable after (dedup hit).

## LOAD03 Measurements — PASS (observed `--nocapture` values)

- `rss_baseline_kb=8304`, `rss_end_kb=26124`, `rss_hwm_kb=26596`,
  `peak_rss_kb=76520` (ru_maxrss, allocator-independent max);
  bound asserted: end ≤ baseline + 512 MiB.
- `db_baseline_bytes=181096`, epoch deltas 7.75 MiB → 3.69 MiB
  (sqlite+wal+shm); rows added identical 121/121.
- Conclusion is slope-based (LOAD04), not RSS-alone.

## LOAD04 Regression — PASS

- Same frozen workload both epochs: rows 121 == 121; db delta epoch2
  (3.69 MiB) ≤ 2× epoch1 (page lumpiness margin) — no steepening.
- Projected post-compress context ≤ 64 KiB on all 6 sessions
  (covered members collapse instead of accumulating).
- Fixed seeds/toolchain/machine recorded here: seed = fixed kind cycle,
  toolchain pinned by `rust-toolchain.toml`, machine = this host;
  re-qualification must reuse this file.

## Product finding (blocking → fixed in-slice)

`save_compression_block` allocated ids by per-session COUNT with a
global PRIMARY KEY: the second session's first block collided on
`b0001` (UNIQUE violation) — multi-session compress was broken.
Single-session suites never caught it. Fix: global
`MAX(CAST(SUBSTR(id,2) AS INTEGER))+1` sequence (`storage.rs`);
existing `b0001`/`b0002` single-session expectations unchanged.

## Checks

- `cargo fmt --check` exit 0;
  `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test --workspace --locked`: 194 passed (soak 4/4 in ~13s),
  3 ignored (live harnesses), 0 failed.
- `cargo build --locked` exit 0; `check_docs.py` exit 0.

## Risks

- Synthetic provider (scripted SSE): real-model output shapes/stalls
  covered by T16/T25 live evidence, not here.
- RSS margin (512 MiB) is a test-env bound, not a product SLO.
