# T53 — fixed-authority bounded Go qualification

2026-10-06, base `a33092f32`. Owner explicitly supplied authorized test input name
`OC_API_KEY` in `.local/live.env`. Presence-only verification is nonempty; no value
was printed. Earlier absence of `OPENCODE_API_KEY` was an incomplete input-name
preflight, not a rejected Go key. T53 resumed, same `t53-go-20261006` ledger/count0.

## Harness checkpoint (before real calls)

- `scripts/t53_go_live.py --run` is the explicit opt-in entry point. It reads only
  the approved file/name and maps material into the child test process's existing
  `OPENCODE_API_KEY` resolver, never config/Git/logs/arguments. Production auth policy
  does not gain an ambient foreign-env fallback.
- `provider/go_live_tests.rs` runs the actual native runtime/Db/read tool using the
  sanitized current public Go catalog and captured Go bindings. It selects one
  cheapest declared tool-capable representative per implemented protocol, then the
  two dated Qwen rows if not already represented. No model-name routing/defaults or
  automatic paid fallback; existing common retry policy remains the owner.
- A **cfg(test)-only** captured ledger hook in the common send path reserves before
  DNS/dial. Fixed HTTPS Go URL/routes, redirects-disabled/DNS pinning/public peer,
  authoritative metadata/auth and native parsing remain unchanged. All main,
  follow-up, auxiliary or retry attempts that use this captured config share the
  same 24-request cap; the harness launches no title/child/summary/MCP lane.
- Reservations/results contain model/protocol/count/status only, never key, headers,
  raw responses, prompts or provider prose. File lock + fsynced atomic replacement
  + parent-directory fsync precede upstream; restart reads the same count, including
  uncertain/failed reservations. Each request's selected native max-token field must
  be 1..2048. Foreign URLs, malformed budgets, mismatched ledger/count and request25
  refuse without forwarding. Production binaries contain no campaign hook/cap.

## Current checks

`cargo test --locked -p oc-adapters --lib go06_`: **2 PASS / 0 FAIL / 1 explicit live
opt-in ignored**. Durable restart/failed-attempt accounting and 25th refusal, foreign
authority and >2048 refusal; actual common send with all three wires refuses before
transport dispatch when exhausted. This new opt-in is additional to the ten existing
workspace ignores, not a disabled regression.

Full adapter library: **619 PASS / 0 FAIL / 1 new explicit live opt-in ignored**;
strict adapter all-target Clippy (`-D warnings`), workspace fmt and diff check PASS.
Compilation experiments fixed the actual `ProtectedGlobs` import/`InputItem::message`
owner and handled provider-state publication result, without API/validation changes.
No real generation has run at this checkpoint; live outcome follows separately.
