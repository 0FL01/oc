## Result

T40 (F15, AUD32–AUD34) implemented and verified in the working tree:

- bounded active projection: `Db::active_history` (prune-bounded, covered
  rows skipped), `Db::block_positions`/`message_seqs` + `dcp::project_active_rows`
  (summary placement equal to `project_rows` over the full history),
  `Db::wire_logs_for_window` (newest-first scan down to the anchor floor);
  `read_history_full` no longer runs on the per-turn path (only for the
  explicit manual `/dcp-compress` over the visible transcript);
- independent byte safety budget `ACTIVE_CONTEXT_BYTES_CAP = 16 MiB` with the
  explicit `RuntimeError::ContextOverflow`; token admission and the bytes/4
  heuristic are documented as separate, approximate limits;
- tool-operation rows return a bounded preview (`TOOL_OP_PREVIEW_BYTES`,
  `output_bytes`, `output_truncated`, `…[+N]` marker) plus the continuation
  `Db::read_tool_op_output(op, offset, limit)`; the durable result and the
  model-visible turn log keep the full text;
- TUI input budget raised to the core limit (`oc_core::session::MAX_INPUT_BYTES`),
  paste/typing overflow reported in a visible note, transport-side silent
  64 KiB cut removed; `truncate` is char-boundary safe.

## Checks

- `cargo test --workspace --locked`: 322 passed / 0 failed / 3 ignored.
- clippy `-D warnings`, fmt check, build, `oc --help`, progress check,
  check_docs, `git diff --check`: exit 0.
- RED evidence: component peak 99.3 MB vs 1.1 MB; actual binary peak HWM
  169.9 MiB (end RSS 132 MiB, PSS 167.6 MiB) for a 152 MiB archive; tool-op
  row 3017 bytes; TUI paste cut to 4096 bytes silently.
- Repaired: component peak 1.09 MB with a 2000-pair archive; actual binary
  peak HWM 27.9 MiB (PSS 25.6 MiB) for the same archive; identical provider
  request history in both runs.

## Risks

- Manual `/dcp-compress` still materialises the addressed transcript by
  design (ranges may address pruned rows); it is explicit, owner-initiated
  and off the per-turn path.
- The byte budget is a memory guard, not a token counter: a model whose real
  tokenisation differs from bytes/4 is admitted/refused by the same heuristic
  as before, now with an explicit byte backstop.
- `InboxMsg::Read` (owner query, used by oc-core tests only) still returns the
  full transcript; the product paths use `history_page`.

## Next

Commit the T40 slice, write `evidence/T40/report.md`, run `progress.py finish`,
push, then start T41 (actual-binary evidence) per the repair order.
