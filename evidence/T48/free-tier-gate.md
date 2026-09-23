# Zen free tier is client-gated: RECON from OpenCode v2.0.15 and bounded probes

## Result

**The deployed Zen free tier is restricted to the OpenCode client itself, so a native `oc` cannot use it without impersonating OpenCode.** This is a provider policy, not a defect in `oc`. Two things are therefore separate:

1. **Real logic bug fixed (parity).** The official client without a configured key sets `apiKey: "public"`, which goes on the wire as `Authorization: Bearer public`; the gateway maps the literal `public` back to "anonymous". Our adapter previously omitted `Authorization` entirely and the config layer rejected an explicit `public`. Now keyless and explicit `public` both send the official anonymous marker, a real key is still sent as its own bearer, and the honest `oc/<version>` UA and own session id are unchanged.
2. **Free access stays refused by policy.** The gateway answers every non-OpenCode client with `403 {"type":"error","error":{"type":"FreeTierError","message":"Error from provider (Console): OpenCode's free tier can only be used from within OpenCode"}}`, independent of the Authorization header. Matching the official request shape does not and must not change that: the only published workarounds require spoofing the OpenCode UA and a canonical `ses_` id, which our invariants forbid.

## Sources (pinned v2.0.15)

- `packages/core/src/plugin/provider/opencode.ts`: without a key the plugin sets `provider.activation = "enabled"` and `provider.settings.apiKey = "public"`, and disables models whose cost has positive input in the picker.
- `packages/console/app/src/routes/zen/util/handler.ts:106-110`: `const zenApiKey = rawZenApiKey === "public" ? undefined : rawZenApiKey`; `sessionId`, `requestId`, `ocClient`, `projectId`, `userAgent` are read and only logged.
- `handler.ts:697-702` `authenticate()`: no key + `modelInfo.allowAnonymous` → allowed; otherwise `AuthError`. `error.ts`: `AuthError`/`ModelError` → 401, `RateLimitError`/`FreeUsageLimitError` → 429, `RegionError` → 403. The observed `FreeTierError` is **not** in the public source, so the gate is deployed-only and its exact check cannot be reproduced from the repository.
- `packages/core/src/session/model-request.ts`: the official request headers are `x-session-affinity`, `X-Session-Id`, optional `x-parent-session-id`, `User-Agent` (`opencode/<channel>/<version>/<name>`), `x-opencode-project`, `x-opencode-session`, `x-opencode-client`.
- `packages/console/app/src/routes/zen/util/ipRateLimiter.ts`: the header check is commented out with `headersExist = true`; no header requirement is enforced there.

## Bounded probes (3 generation HTTP, all counted)

All three used a synthetic one-line prompt, `max_tokens` ≤ 16, honest `User-Agent: oc/0.1.0`, our own session id, and no credential; each was recorded as a manual marker in the durable campaign directory (`generation-01..03-manual-probe`, on top of the earlier harness `generation-00`).

| # | Headers | Result |
|---|---|---|
| 1 | `Authorization: Bearer public` | 403 `FreeTierError` (same message) |
| 2 | no `Authorization` | 403 `FreeTierError` (same message) |
| 3 | `Bearer public` + full official header shape (`x-opencode-project`, `x-session-affinity`, `X-Session-Id`, `x-opencode-request`) with our own values | 403 `FreeTierError` (same message) |

Conclusion: the gate does not key on the anonymous marker, and no honest third-party header set passes it. Only forged OpenCode identity (UA `opencode/...` plus canonical `ses_` id, as used by third-party "disguise" plugins) is reported to pass, which we will not do.

## Corroboration from public reports

`anomalyco/opencode` issues #49621 ("Free-tier Zen 403 for all third-party stacks despite valid session + key (only genuine client passes)"), #49587/#49610 (the same error inside the official client when internal calls lack the expected headers), #49723 (built-in subagents misclassified), and third-party trackers (opencode2api #19; "dsh-opencode-free-tier" describing itself as a disguise: "opencode User-Agent + canonical ses_ session"). These confirm the restriction is intentional and client-identity based.

## Legitimate paths that remain

- **OpenProxy** (existing native Responses path) — unchanged and still the product's real-provider lane.
- **Zen with a real Zen API key and paid models** — the Zen endpoints are documented for direct use with `Authorization: Bearer <key>`; free models stay gated. This needs owner authorization, a real key, and a scope decision to admit paid models explicitly (no automatic fallback).
- **OpenCode Go** — its documentation explicitly supports third-party coding agents with their own UA and `x-opencode-session`; paid subscription and a different endpoint (`/zen/go/v1/...`), outside the current `/zen/v1/chat/completions` scope.

## Checks

`cargo test --locked -p oc-adapters zen_ --lib` (11 passed, 2 ignored), `cargo test --locked -p oc --test zen_free` (6 passed), workspace `fmt`/`clippy`/tests/build and doc/journal validators: see the task report for exit codes. No credential was read or used; no raw response body was committed; the campaign stands at 4/24 generation HTTP with no retries.
