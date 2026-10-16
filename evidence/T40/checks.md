# T40 checks and measurements

Host/toolchain: this Linux host, `rust-toolchain.toml` pinned, debug profile,
system allocator; scripted loopback peers only (no live calls).

## Workload (fixed before the run)

Component (`crates/oc-adapters/tests/context_bounds.rs`):
archive pairs 8 -> 2000 x 16 KiB with durable turn logs, active pairs
2 x 1 KiB behind a prune mark; bound `PEAK_DELTA_BOUND = 8 MiB`.

Actual binary (`crates/oc/tests/memory_bounds.rs`):
archive pairs 8 -> 3000 x 16 KiB (+ turn logs), active pairs 2 x 1 KiB behind
a prune mark; bound `PEAK_RSS_BOUND_KB = 64 MiB`, chosen before measuring.

## AUD32 — equal active context, growing archive

Component, repaired code:

```
AUD32 workload: archive pairs 8 -> 2000 x 16384 B, active pairs 2 x 1024 B;
construction peak small=847689 large=1091090 bytes (bound 8388608)
```

Actual binary, repaired code (`cargo test -p oc --test memory_bounds -- --nocapture`):

```
AUD32 small: baseline_rss_kb=12 peak_hwm_kb=26296 end_rss_kb=26296 end_pss_kb=24009 peak_pss_kb=24009 children=0 db_bytes=528384    wal_bytes=0
AUD32 large: baseline_rss_kb=96 peak_hwm_kb=27928 end_rss_kb=27772 end_pss_kb=24099 peak_pss_kb=25641 children=0 db_bytes=152457216 wal_bytes=0
```

(Representative of repeated runs; per-run values move by a few hundred KiB,
far below the 64 MiB bound.)

- equal active context: the provider request history is byte-identical in
  both runs (asserted), and the archive never appears in it;
- peak RSS delta: small 25.7 MiB, large 27.2 MiB for a 152 MiB archive
  (pre-repair: 169.9 MiB peak, 132.2 MiB end, 167.6 MiB PSS — `red-binary.txt`);
- no child processes were spawned during either turn (the turn is
  text-only); `children=0` is reported, not assumed.

## AUD33 — no silent truncation

`cargo test -p oc-adapters --test context_bounds aud33` — a 96 KiB prompt and
two 96 KiB live history messages (inside the 1M-token model budget) reach the
provider whole and stay whole in the durable row.

`cargo test -p oc-tui aud33_paste` — a 100 KiB multibyte paste is kept whole
(the old 64 KiB transport cut is gone), and a paste beyond the core input
budget is bounded at `MAX_INPUT_BYTES` with a visible note naming the dropped
bytes and the limit. Typing past the limit also reports a note instead of
dropping the key silently.

## AUD34 — output retention versus model preview

`cargo test -p oc-adapters --test context_bounds aud34`:
- the tool-operation list row is a bounded preview (`…[+N]`, `output_bytes`
  exact, `output_truncated` set) and never carries the whole result;
- `read_tool_op_output(op, offset, limit)` returns the fact after the first
  2048 bytes from the single durable copy, with byte-accurate `next_offset`;
- the follow-up provider request still carries the full tool output, so the
  preview never replaces model data.

## Gates

```
cargo test --workspace --locked      -> 322 passed; 0 failed; 3 ignored
cargo clippy --locked --workspace --all-targets -- -D warnings -> exit 0
cargo fmt --all -- --check           -> exit 0
cargo build --locked                 -> exit 0
target/debug/oc --help               -> exit 0
python3 scripts/progress.py check    -> exit 0
python3 scripts/check_docs.py        -> exit 0
git diff --check                     -> exit 0
```

`Cargo.lock` gained one edge: `oc`'s dev-dependency on `rusqlite` (already in
the lock through `oc-adapters`) so the actual-binary test can seed a synthetic
archive through a second connection.

## Build note

Two intermediate builds reported stale-type errors (`queries` module/fields
missing) after same-second edits; `touch`ing the edited sources cleared it.
This is a cargo mtime-fingerprint artifact, not a source defect — the final
gates above all ran clean from the touched tree.
