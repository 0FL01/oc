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

## Real Go qualification and corrective slice

2026-10-06, implementation base `43f836eb6`. The explicit command
`python3 -B scripts/t53_go_live.py --run` now exits 0: one opt-in test PASS, zero
failures. Every selected model ran the real native runtime, executed exactly one
settled local `read`, and completed the subsequent native function-result request
with the fixture token. Db tool-operation count was one per model; no shell,
mutation, MCP, title, child or summary lane was invoked.

| Exact current catalog ID | Declared native wire / route | Selected variant | Native reads | Runtime aggregate input/output usage | Result |
| --- | --- | --- | --- | --- | --- |
| `muse-spark-1.2-contributor` | Responses `/responses` | default (`null`) | 1 | 1329 / 348 | PASS |
| `longcat-2.5-preview-free` | Chat `/chat/completions` | default (`null`) | 1 | 756 / 98 | PASS |
| `qwen3.8-flash` | Messages `/messages` | `none` | 1 | 863 / 36 | PASS |
| `qwen3.8-max` | Messages `/messages` | `none` | 1 | 863 / 36 | PASS |
| `qwen3.7-plus` | Messages `/messages` | `none` | 1 | 863 / 36 | PASS |

The first three rows are source/cost-selected protocol representatives; the last
two are the frozen dated conflict targets, not name-based route rules. The current
catalog declares both Qwen targets as Messages, and their actual native endpoint
tool/final exchanges succeeded. No Chat retry, route guess or paid fallback was used
to resolve the old catalog/docs conflict.

### Failures retained, not erased or free retries

- Physical request 1 completed Responses text but no tool: the programmatic smoke
  fixture overrode scalar permissions while retaining independently compiled default
  ordered rules, so `read` was not exposed. The fixture now uses its own coherent
  read-only permission authority. A local exposure guard additionally refused one
  attempted run before DNS; that local failure did not consume a physical request.
- Physical request 2 offered `read` with an experimental forced choice and received
  typed HTTP 400/InvalidRequest. All forced-choice mutation was removed. The current
  harness verifies actual native tool exposure and returns **unchanged request bytes**;
  an offline three-wire test proves this for initial and follow-up bodies.
- Physical request 7 used the then-incorrect Go Messages Bearer scheme and received
  typed HTTP 401/Authentication. This was a client scheme bug, not a missing/revoked
  owner key. Pinned donor server `opencode/packages/console/app/src/routes/zen/go/v1/messages.ts:9`
  reads `x-api-key`; `responses.ts:9` and `chat/completions.ts:9` read Bearer. The frozen
  T53 wire table already requires native Messages x-api-key. Production now applies
  that policy after authoritative Go metadata, removing competing auth. Custom
  Messages `apiKey`/x-api-key and `authToken`/Bearer behavior remains unchanged.
  Earlier blanket Go-Bearer checkpoint claims are superseded by this source/live proof.
- Subsequent requests 8–13 succeeded with native Messages x-api-key. Existing passed
  model rows were skipped on process restart, not rerun; both previous failures and
  all local/nonqualifying result rows remain in the sanitized ledger.

Same campaign `t53-go-20261006`: **13/24 physical requests** (11 complete, 2 failed),
including every experiment; main 6, ordinary follow-up 3, dated protocol probe 4.
The four Qwen requests were reclassified from their main/follow-up buckets as dated
probes without changing any sequence, outcome or physical total. Retry/title/summary/
child/MCP counts and uncertain effects are zero. Every native output-token cap is
2048. No key, auth header, raw response or model prose is in Git or command output.

### Current corrective gates

- `cargo test --locked --workspace`: **1651 PASS / 0 FAIL / 11 ignored**, 42 target
  result records. Ten prior opt-ins unchanged; the new Go opt-in has its separate
  explicit real PASS above, not an inferred PASS from being ignored offline.
- Strict workspace all-target Clippy (`-D warnings`), fmt check, locked build,
  native `oc --help` and diff check: exit 0. Normal default stacks; no timeout,
  ignored-regression, baseline or dependency changes.
- Full local output: `/home/opencode/.local/share/opencode/tool-output/tool_110080bec001VkIPufJKhg8L7n`.
- Fake all-wire captured-context and public-binding tests assert native Go auth and
  competing-header refusal alongside existing identity/cache guarantees. Campaign
  lock is ignored; only its sanitized durable ledger is committed.

Final report/task closure follows this reviewed implementation/live slice; this
receipt does not change other task status or waive any T53 outcome.
