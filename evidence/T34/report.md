# T34 — Responses protocol and streaming

Status: **PASS for T34 offline repair scope**, not A01–A13/product READY.
Implementation: `c5e33d4` on `agent/oc-rust-port`; base `4e07cc7`, not audit rollback.
Findings: **F02, F03**. Contract: `audit/repairs/T34.md`.
Initial red: [regression.md](regression.md). Executed gates: [checks.md](checks.md).

## Delivered application behavior

The actual binary sends typed user/assistant items and complete canonical
Responses outputs, including encrypted reasoning and assistant phase. Function
item ID is separate from call ID; outputs use the latter. Existing private turn
result storage owns replay journals, anchored to immutable raw user/assistant
message IDs. Completed tool output and its wire result share one transaction.
Restart does not rerun unanswered calls or invent results. No schema/dependency
or fifth package was introduced; the core remains UI/adapter-independent.

Only a successful complete terminal permits execution/completion. EOF, failed,
incomplete, partial call and exhausted rounds produce non-success. Text deltas
reach application consumers during generation. Cancellation interrupts silent
DNS/headers/body waits and MCP initialize; headless cancellation can occur while
submission awaits attach. Sanitized error kinds, including byte-limit failures,
are visible without remote error bodies, credentials or opaque continuation.

Native configuration supplies effective idle timeout, max output, cache-key flag
and extra headers. The old runtime flattened-string assembler and silent 64 KiB
history drop are removed. The legacy direct text-provider helper remains for
existing direct consumers; normal application does not use it.

## Acceptance mapping — executed, not assertion inventory

| ID / finding | Evidence and result |
|---|---|
| AUD09 / F02 | Actual `oc` strict fake requires full typed arrays on every request, item `fc_A` with call `call_B`, then a second distinct tool round; both full calls and correctly linked outputs present. `aud09_aud10_binary_exact_typed_tool_history_survives_restart` PASS. PROV03 now also sends actual typed output rather than a text marker. |
| AUD10 / F02 | Same binary test restarts a separate process and requires exact canonical history including opaque encrypted reasoning and assistant phase. No opaque bytes in stdout/stderr. Actual `--image` rejects unsupported modality with exit 2 before network/storage. Adapter image serialization passes separately; no application image-support claim. |
| AUD11 / F03 | Runtime tests exercise complete text-event EOF, response.failed/incomplete, partial call even with parseable JSON, and round-limit exhaustion after a real shell effect. No Completed/partial execution. Reopened runtime carries the exact durable call/result and does not repeat the effect. Actual-binary SIGKILL recovery rerun: operation remains unknown, accepted message stable, no fabricated result/replay. |
| AUD12 / F03 | Actual binary emits a delta before terminal, then receives no more bytes; SIGINT exits 130 within 950 ms. Same test covers no response headers. Runtime MCP test waits for initialize receipt, cancels and verifies child disappearance under one second with zero accepted input/turn/provider calls. Existing shell and MCP-call cancellation tests pass. |
| AUD13 / F03 | Actual oversized pending-line stream fails with visible `SSE line byte limit exceeded`, no completed answer/retry. Provider tests execute line/event/argument/generation ceilings and an oversized typed network request rejected before dial. Captured requests check max_output_tokens=789, configured extra header, auth precedence and both cache flag branches. Effective 30 ms idle budget returns under 500 ms; 400 sends exactly one request. |

All binary regressions use isolated HOME/XDG/project/data and explicit loopback
opt-in. Real network/paid probes and real credentials were not used.

## Bounds and protocol decisions

- Pending line/event: 2 MiB each; per-call arguments: 1 MiB; generation accounting:
  32 MiB (`4 × payload + 256/event`); request serialized bytes: 32 MiB;
  events: 10,000; observed text fragments: 16 KiB on UTF-8 boundaries.
- Default idle remains 6,000,000 ms (100 minutes), not a total deadline.
  Successful terminal returns without waiting for socket EOF. At most two
  attempts, and only pre-commit transient failures; no 400/terminal retry.
- Canonical terminal output wins, otherwise ordered complete output_item.done.
  Reasoning is never reconstructed from visible text.
- Extra headers remain provider-scoped. Native Authorization/Accept/Content-Type
  override case-insensitively; transport-header overrides and redirects refused.

References reviewed: pinned OpenProxy
[`lean-proxy.md`](https://raw.githubusercontent.com/0FL01/openproxy/4ef76dbce2cdbb85206cbe5e59acbad9d96ae387/contracts/lean-proxy.md),
[`api/mod.rs`](https://raw.githubusercontent.com/0FL01/openproxy/4ef76dbce2cdbb85206cbe5e59acbad9d96ae387/src/server/api/mod.rs),
official [Responses](https://developers.openai.com/api/reference/resources/responses/methods/create),
[reasoning](https://developers.openai.com/api/docs/guides/reasoning),
[function calling](https://developers.openai.com/api/docs/guides/function-calling).
No production protocol translation layer or copied upstream implementation.

## Verification and mandatory-gate blocker

Final targeted soak, `cargo test --workspace --locked`, workspace/all-target
clippy `-D warnings`, fmt check, locked build and `oc --help`: **exit 0**.
Exact bounded suite results and prior failures are in checks/regression above.
Three pre-existing external harnesses are ignored/NOT RUN, not PASS; no new
ignored tests or changed thresholds. Documentation/registry checks also pass
(structure only). Final diff reviewed; Cargo files/schema unchanged.

Workspace testing exposed an inherited-flock reopen failure. A synchronized
pre-exec regression failed before correction, passed afterward. Private acquired
RootLock explicitly unlocks after SQLite closes; concurrent second-owner refusal
remains tested. This was a reproduced gate blocker, not speculative cleanup.

## Remaining scope / supported differences

This proves T34, not full product readiness. Image application input is explicitly
unsupported as permitted by AUD10. Different provider/model within the same wire
session currently refuses rather than reusing alien opaque state; UI switching
is T39. Full archive loading, per-turn journal size/lifetime and overall retained
memory are T40; accounting ceilings are not measured RSS. Model-driven DCP and
tool-graph projection are T36, configured definitions/mappings T35, MCP generation
lifetime/auth T37, remaining tools/UI T38–T39. Live qualification remains T27
after T42. Old module-only PASS claims do not discharge these tasks.

Next: start ready **T35** through existing progress engine. Audit fragments were
already merged once. No push performed; implementation is committed locally.
