# T40 — Bounded active context, not truncation (F15, AUD32–AUD34)

Status: DONE (offline synthetic qualification; no live calls needed).

## What was wrong

At `a71768d` every turn materialised the whole archive: `read_history_full`
(all message rows), `wire_logs` (all turn-log JSONs) and then a projection
over both, before admission ran. The audit-base runtime additionally capped
the assembled request at 64 KiB by dropping oldest history lines — a silent
loss the earlier repairs removed, but nothing pinned it and the intermediate
memory stayed proportional to the archive.

## What changed

Active projection by pages/references (`crates/oc-adapters`):

- `Db::active_history(session, after_seq, budget)` reads only rows above the
  prune mark, skipping rows any compression block covers; covered rows are
  never materialised. Overflow releases the rows and reports the exact
  totals (`ActiveHistory { bytes, rows_read, overflow }`).
- `Db::block_positions` / `Db::message_seqs` + `dcp::project_active_rows`
  place block summaries by the first in-window member, which is
  byte-identical to `project_rows` over the full history (checked in tests
  and by debug assertions during development). Covered rows whose block has
  no placement metadata are kept, never dropped.
- `Db::wire_logs_for_window(session, floor_seq, page)` scans turn logs
  newest-first in bounded pages and stops below the anchor floor, because
  turn rows are inserted in anchor order. Logs without a parseable anchor
  are kept (unknown shapes never lose history).
- `read_history_full` now runs only in the manual `/dcp-compress` path: the
  owner addresses the *visible* transcript, whose ranges may include pruned
  rows. It is explicit, off the per-turn path and documented as such.
- `application.rs` DCP snapshot estimates from the bounded window and, above
  the budget, reports the byte-derived estimate instead of a silent zero.

Two independent limits, honestly separated (`crates/oc-adapters/src/runtime.rs`):

- `ACTIVE_CONTEXT_BYTES_CAP = 16 MiB` is a memory guard. Exceeding it returns
  `RuntimeError::ContextOverflow { bytes, cap }` — "active context is N bytes,
  above the M byte safety budget; compress the session or start a new one" —
  never a silent drop.
- Token admission is unchanged: `models::admit` against the selected entry's
  declared `limit.context`/`limit.output` using the bytes/4 estimate, now
  documented as an approximation rather than a counter for any proxy model.

Output retention versus preview (`AUD34`):

- `TOOL_OP_PREVIEW_BYTES = 2048`: `list_tool_ops`/`list_tool_ops_page`
  return a preview with an explicit `…[+N]` marker plus `output_bytes` and
  `output_truncated`; the UI-facing row is never the whole result.
- `Db::read_tool_op_output(op, offset, limit)` is the continuation: byte
  windows cut back to a UTF-8 boundary (a partial trailing char is withheld,
  so no byte is skipped) with `next_offset`.
- The full result stays in one durable place (`tool_operations.output`) and
  the model-visible turn log keeps it too: the follow-up provider request
  carries the whole tool output, so the preview never replaces model data.
- The TUI card renders the bounded preview with the stored size, and
  `oc_core::queries::ToolOpView` carries the same metadata.

No silent input truncation (`AUD33`):

- The TUI input budget is the core limit (`oc_core::session::MAX_INPUT_BYTES`,
  1 MiB) instead of a local 4096-byte cap plus a 64 KiB transport cut;
  `map_event` passes pastes through and `TuiState::handle_paste`/`handle_key`
  report a note naming the dropped bytes and the limit.
- `runtime::truncate` is char-boundary safe (`floor_char_boundary`), so a
  multibyte preview can never panic.

## Evidence (RED first)

`regression.md` + `red-context_bounds.rs` + `red.txt` + `red-binary.txt`
capture the pre-repair behaviour:

- component construction peak: `99273700 bytes` for 2000 archived pairs vs
  `1113686 bytes` for 8;
- actual binary: peak `VmHWM` 169,876 KiB, end RSS 132,164 KiB, PSS
  167,576 KiB for a 152 MiB archive (small run 26,200 KiB);
- tool-operation row: `the UI-facing row must be a bounded preview: 3017 bytes`;
- TUI paste: `paste must be kept whole (left: 4096, right: 102400)`.

Repaired (`checks.md`):

- component peak with the same 2000-pair archive: `1091090 bytes`
  (small: 847,689; bound 8 MiB);
- actual binary with 3000 archived pairs (152 MiB DB): peak `VmHWM`
  27,928 KiB, end RSS 27,772 KiB, PSS 24,099 KiB; small run 26,296 KiB;
  the provider request history is byte-identical in both runs and never
  contains the archive; `children=0` (no process spawned by the turn);
- the byte budget, admission and preview contracts are asserted by
  `crates/oc-adapters/tests/context_bounds.rs` (AUD32/33/34) and
  `crates/oc/tests/memory_bounds.rs` (actual binary).

## Workload, host, allocator

Frozen in the test sources before measuring: component 8 → 2000 archived
pairs x 16 KiB with turn logs, 2 active pairs x 1 KiB; binary 8 → 3000 pairs
x 16 KiB (+ logs), 2 active pairs x 1 KiB. Host: this Linux machine,
`rust-toolchain.toml` pinned, debug profile, system allocator, scripted
loopback peers. Bounds were fixed before the first measurement and were not
adjusted afterwards (8 MiB component, 64 MiB binary).

## Gates

`cargo test --workspace --locked` 322 passed / 0 failed / 3 ignored;
clippy `-D warnings`, fmt check, build, `oc --help`, `progress.py check`,
`check_docs.py`, `git diff --check` all exit 0.

## Remaining risk

- Manual `/dcp-compress` materialises the addressed transcript (by design,
  explicit owner action off the hot path).
- `InboxMsg::Read` (owner query used by oc-core tests) still returns the full
  transcript; product paths use `history_page`.
- The byte cap is a memory guard, not a token count: models whose real
  tokenisation differs from bytes/4 keep the previous admission behaviour
  with an explicit byte backstop.

## Next

Start T41 (actual-binary evidence) per the repair order, then T42 (mandatory
offline qualification), T27 (bounded live), T30 (FINAL).
