# T40 initial regressions (RED)

Captured against `a71768d` (T40 start, before the repair). All runs are
offline with temp fixtures and scripted peers; no live calls.

## AUD32 — component: construction peak follows the archive

`crates/oc-adapters/tests/context_bounds.rs` on the pre-repair runtime
(kept verbatim in `red-context_bounds.rs`, output in `red.txt`):

- workload: archive pairs 8 (small) vs 2000 (large) x 16 KiB, active pairs
  2 x 1 KiB cut off by a prune mark, each archived pair with a durable turn
  log of the same size;
- `turn construction peak followed the archive: 99273700 bytes for 2000
  archived pairs (small run: 1113686 bytes)` — the turn materialised the
  pruned archive through `read_history_full` + `wire_logs` before projecting.

## AUD32 — actual binary: peak RSS follows the archive

`crates/oc/tests/memory_bounds.rs` run against the pre-repair binary
(`red-binary.txt`):

```
AUD32 small: baseline_rss_kb=12 peak_hwm_kb=26200  end_rss_kb=25600  peak_pss_kb=23963  db_bytes=524288
AUD32 large: baseline_rss_kb=16 peak_hwm_kb=169876 end_rss_kb=26520  peak_pss_kb=167576 db_bytes=152453120
peak RSS followed the archive: 169860 KiB for 3000 archived pairs
  (small run: 26188 KiB, bound 65536 KiB)
```

The large run's PSS peaked at ~164 MiB for a 152 MiB archive; the repaired
binary peaks at ~27 MiB with the same workload (see `checks.md`).

## AUD33 — TUI dropped input silently below the core budget

`crates/oc-tui` on the pre-repair view model: `handle_paste` cut at
`PASTE_INPUT_MAX` (64 KiB) and `handle_key(Char)` stopped at a local
`MAX_INPUT_BYTES = 4096`, both without any diagnostic, while the core
accepted `oc_core::session::MAX_INPUT_BYTES = 1 MiB`. A 100 KiB paste
therefore reached the runtime as 4 KiB with no visible note:
`assertion left == right failed: paste must be kept whole (left: 4096,
right: 102400)`.

Source-level fact: the audit-base `runtime.rs` capped the assembled request
with `INPUT_BYTES_CAP = 65_536` and dropped oldest history lines until it
fit; that cap is already gone from the current runtime (no `INPUT_BYTES_CAP`
and no oldest-dropping loop). AUD33's regression test pins the behaviour so
it cannot come back.

## AUD34 — tool-operation rows materialised the whole result

`crates/oc-adapters/tests/context_bounds.rs` on the pre-repair storage:
`the UI-facing row must be a bounded preview: 3017 bytes` — `list_tool_ops`
returned the entire durable output for every row of the page, and there was
no continuation API, so the UI/IPC path could not stay bounded without
losing the fact after the first 2048 bytes.
