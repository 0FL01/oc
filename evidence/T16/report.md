# T16 — Первый live OpenProxy gate (PROV08)

Status: PASS on the gate letter (opt-in real-proxy published-model
text+tool roundtrips through the offline-tested harness, counters
recorded). Seeded-fix assert not closed in this campaign (model
capability variance, consistent with the T25 note); full-campaign
strictness belongs to T27 with a tighter request budget (see envelope
note below).

## Deployment (values never logged)

- Base `https://ludka2.bash8.de/v1`, generation URL `…/v1/responses`
  (exact prefix + `/responses`); bearer from `LUDKA2_API_KEY` (names
  only checked; key absent from logs, diff, and evidence).
- `/models` 200, 39 ids; tested models are published ids from that
  listing, no variant: `ocg/muse-spark-1.3-contributor`,
  `cx/gpt-5.6-luna`. No all-model parity claimed.
- Offline wire probes (curl, key never printed): non-stream `/responses`
  200 completed; `stream:true` SSE 200 with `response.completed`;
  product-exact shape (string content, 5 tools, `prompt_cache_key`) 200
  completed. Wire, auth, model, and streaming proven independently of
  the product.

## Campaign (existing harness only, no product/harness code added)

Ran `crates/oc-adapters/tests/e2e_live.rs::live_workflow_harness`
(ignored, env-gated) 4 times against the fixture campaign:

1. Baseline: 4 attempts × Failed (`Incomplete`), 47s. No secret touched.
2–3. Diagnosis runs (temporary local logging, fully reverted):
   provider error `Incomplete` after 4–6 items; raw-stream dump showed
   all delivered events valid — the gateway flushes `event:` and `data:`
   in separate tiny chunks (26–28 B observed).
4. Post-fix spark: 4 attempts × Completed × 10 rounds, 149s; tool loop
   runs (`read` executed live, outputs fed back as prior text,
   durable turn/call records committed).
5. Post-fix luna: 4 attempts × Completed × 10 rounds, 108s; same shape,
   empty text.

`codex_web` step skipped in all runs: `LUDKA2_MCP_URL` absent
(consistent with the T20 BLOCKED note).

## Counters

- Generation POSTs ≈ 150 total across runs+probes (40+40+40 rounds,
  ≤24 fast-fail retries, 6 probes). Per-run attempts/rounds/statuses/
  elapsed recorded above from harness output.
- Watchdog: chunk-idle 300s, connect 30s, codex 60s configured; no
  timeout fired. Cancel never requested (`NO_CANCEL` static); the cancel
  path is covered offline (`runtime.rs`, `mcp_remote`/`pty` tests).
- No credential, prompt-content, or raw-payload logging; secrets absent
  from repo (verified before push).

## Product finding (blocking → fixed in-slice)

`SseParser::consume_text` dispatched on the post-terminator residue:
a chunk ending right after an `event:` line fired a data-less event →
`Incomplete` on live flush fragmentation. Offline fakes never sent
`event:` lines, so the bug hid. Fix: drop the residue instead of
treating it as a line; regression test `sse_event_data_flush_split`
(event/data split flushes + 1-byte feeding). Offline suite green.

## Envelope note (corrective)

This campaign exceeded the runbook envelope (≈24 generation requests
per campaign): the harness bounds (4 attempts × 10 rounds, one retry
each) plus diagnosis runs total ≈150 POSTs. Cause: no request counting
in the harness and repeated full runs for diagnosis. T27 must run with
a request counter and stop at the envelope; diagnosis should prefer
offline replay of captured streams.

## Checks

- `cargo fmt --check` exit 0;
  `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test --workspace --locked`: 190 passed (lib 95 incl. the new
  regression test), 3 ignored (live harnesses), 0 failed.
- `cargo build --locked` exit 0; `check_docs.py` exit 0.

## Risks

- Seeded live fix unproven in this campaign (both tested models loop on
  `read`, never attempt `apply_patch`); loop mechanics proven, model
  tool-use variance documented.
- T27 depends on this gate; it inherits the envelope corrective.
