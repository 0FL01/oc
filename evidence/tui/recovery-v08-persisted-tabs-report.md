# T44 — Location-scoped retained tabs across restart (paired diagnostic)

## RECON and implementation

Pinned OpenCode v2.0.12 `packages/tui/src/context/session-tabs.tsx:70-76,106-112,128-148,176-199,322-344` persists ordered real tab IDs; Home is a route, not a fabricated session. Native previously retained up to 16 views only until process exit. Commit `f8e5540` adds a `CoreApp`/single-owner `TabDeckSnapshot` that reads/writes a versioned, bounded Location-scoped SQLite preference. It stores ordered real root IDs and active ID (or sessionless Home), **not** drafts, transcripts or whole `TuiState`s. Startup validates actual root/Location bindings and rebuilds bounded history/catalog views. Explicit `--session` remains authoritative; a child ID opens an explicitly read-only standalone history view and cannot be saved as a root tab. Failed Location publication keeps the old deck; successful publication loads only the new Location's saved deck.

The owner rejects malformed/oversize, foreign, duplicate, stale and over-capacity save requests. Its read is byte-bounded before SQLite materializes a preference; a partially filtered/unreadable saved deck remains available where possible but cannot silently overwrite omitted IDs. A scope-bound revision and an immediate SQLite compare-and-set prevent stale writers from replacing a new Location's or concurrent caller's preference. An accepted fresh root gets a bounded pending-adoption marker **in the same transaction** as root/Location/turn/user message; after a process crash, the owner projects it into the deck with no provider request or new root. A successful CAS save retires included markers atomically. Fresh acceptance refuses a malformed or full stored deck before committing anything. A real close persists its complete deck to retire pending markers, then persists the removal **before** changing the visible view; failed writes refuse the close. Normal immediate Quit reconciles a pending fresh receipt before owner shutdown. Unsafe actions leave a fixed, value-free diagnostic, not raw SQLite or persisted contents.

## Real-binary verification and independent original/native capture

`crates/oc/tests/pty_t42.rs` drives actual PTYs and SQLite: reopen the ordered two-tab deck and selected route, restore Home without a root/provider request, explicit root authority, durable close/reopen, Location A/B isolation and failed switch, corrupted preference with no overwrite, stale concurrent writer, partial parked-view failure, invalid IDs and foreign/unbound/child sessions, and immediate Ctrl+C after accepted first Home turn. Adapter tests force marker insert/write failures, 15+1 capacity, malformed/oversize stored preferences, marker/CAS races, and crash-before-deck-save restoration. These native tests do **not** constitute an independently replayed original restart comparison.

Independent pinned original/native 120x40 Reader-profile fake Responses captures are immutable at `evidence/tui/recovery-v08-persisted-tabs-{01,qualified,qualified-02}/`; `qualified-02` locks the final source after the all-or-nothing close fix. Command:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --tab-click true --tab-close true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-persisted-tabs-qualified-02
```

Final lock records upstream v2.0.12 commit `2670273ff17da96f85c5826ced57aa1b368754fa` and executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`; native SHA-256 `61e7e507c54e946c0d46693052545c26393bc07b1b1831d5aa14940e675da2b2`; fixture `d18132f88c4a7a638a244b0ea92007163246fc3ce0bbe5bcb2b90df02a67bb39`, terminal profile `e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`. Both PTYs report `provider_contract=true`, `TAB_INTERACTION_CHECKS_PASS`, `TAB_CLOSE_CHECKS_PASS` and exactly three provider completions; `check_region.py --rect 0 0 70 1` reports **0/70** differing styled cells on the closed-after tab row. The runner exits **1**: the whole closed-after frame remains **DIFFERENT** (200/4800 styled cells, 4540/647040 PNG pixels); no masks or frozen elapsed time. The runner tests fresh tabs and mouse interactions, **not paired restart of seeded identical history**. The first two attempts record pre-final logic and remain unmodified.

## Gates and remaining scope

Final serialized `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet` passed with 0 failures (existing opt-in live ignores; 204 TUI tests), plus `cargo fmt --all -- --check`, workspace all-target Clippy `-D warnings`, `cargo build --locked`, docs/progress checks and `git diff --check`. Two initial workspace targets had stale assertions: MCP Location B/C waits expected an eager root; V03 geometry expected bare Home after earlier explicit runs in the same data root. They were corrected to assert actual Location publication/no root until accepted turn and the restored tab/history; both targeted and full serial reruns passed.

This is **not** a VIS01–24 or T44 PASS. Independent upstream/native paired restarts at the full size/state matrix, remaining VIS and V08–V09, genuine app version/time/Location differences, and remaining S07 instrumentation are still open. The single-owner view deck has a 16-slot cap; invalid or conflicting preferences fail closed with diagnostics and intact durable roots. The pre-existing untracked `.opencode/` was not accessed.
