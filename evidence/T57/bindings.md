# T57 — admitted OpenAI request bindings (partial AUTH03/AUTH04)

Date: 2026-10-06. Base: `4a4364cde`. Frozen outcomes remain active; this is
binding/admission proof, **not** WS, catalog-transform, actual login/model or live PASS.

## Owner and authority

`AuthScope` recognizes only provider `openai` at the admitted fixed API authority.
Composition's selected and independent provider templates now resolve asynchronously
through the existing OpenAI resolver: stored Key/OAuth → own OPENAI_API_KEY → configured
Key; failed/unknown/stale refresh becomes explicit unavailable, not key/env fallback.
Foreign endpoints (even named openai), Go and custom priority retain their own rules.
Account metadata reads remain local, not server-validation or refresh operations.

`ResolvedAuth` gives immutable request templates the captured account scope and
subscription metadata. OAuth uses the pinned Codex base and native Bearer/originator/
beta/account headers; `RequestContext` supplies that request's session-id. Ordinary
Key restores the original API base/endpoint and receives no Codex header injection;
admitted organization/project headers remain. User header overlays cannot replace
native auth/session/account fields. Generic OAuth remains unsupported; subscription
execution is Responses-only. Original unsupported/endpoint/auth inputs are retained
so an OAuth→Key switch does not leave a stale Codex route or unsupported flag.

Replay authority partitions Key/OAuth and local accounts. OAuth token rotation keeps
the same admitted opaque account scope; a captured old request retains its old token.
Session affinity is not opaque authority. Existing Go/custom fingerprint tuples are
unchanged byte-for-byte, rather than receiving a new null field.

Token/device POSTs also reuse native endpoint DNS/peer admission: fixed issuer and
three owned paths, all addresses checked and pinned, no proxy/redirect/automatic
retry, 10s DNS/connect and 30s HTTP bounds, bounded JSON. Endpoint overrides/clients
remain private unit fixtures; production has no issuer override. Callback cancellation
and durable unknown-refresh reservation are unchanged.

## Verification

New owning scenario `auth/openai/binding_tests.rs` constructs real SQLite credentials
and captured request headers (no generation request): stored OAuth beats env/config,
native headers cannot be injected, separate session contexts have distinct session-id,
same-account token rotation preserves provenance/old capture, another account and
Key partition provenance, Key restores normal route, foreign scope never reads own
OpenAI environment, generic OAuth refuses and Go/custom scope encoding stays exact.
Debug assertions exclude synthetic access/refresh/routing canaries.

Approved TMPDIR, CARGO_BUILD_JOBS=3, RUST_TEST_THREADS=2, serial Cargo, normal stacks:

- `cargo test --locked -p oc-adapters --lib auth0`: **16/0** before the final
  unchanged-fingerprint review; the new `auth04_` scenario was rechecked afterward
  **1/0**, then the full impacted crate qualified the final source below.
- `cargo test --locked -p oc-adapters --lib`: **645 passed / 0 failed / 1 unchanged
  Go opt-in ignored**, 98.03s after preserving original diagnostic stages/authority
  fields during the shared resolver review. Local full output:
  `/home/opencode/.local/share/opencode/tool-output/tool_1118395ac001fdEvw3003e5OAv`.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: exit0 after
  final source; fmt/diff checks green. No dependency, test disable, timeout/stack
  override, baseline rewrite or validation weakening.

The new fixture initially used the wrong zero-argument Db::create_session signature;
fixed to explicit source IDs without widening production APIs or changing behavior.
Earlier application account assertion that known OpenAI OAuth was unsupported was
updated to the new admitted stored-account contract, not a connectivity assertion.
All existing Go/catalog/replay/retry/security/storage/PTY regressions remain green.

## Remaining

Public OpenAI models.dev slice/subscription transform, default native WS/explicit HTTP
and safe same-route fallback, per-request preparation/auxiliary/child capture and
stream/retry/checkpoint qualification remain AUTH04 work. CLI/TUI and actual binary
AUTH01/AUTH02/AUTH05, then dedicated AUTH06 live prerequisites/gates remain unresolved.
No real API call, browser profile/runner credential read or T53 campaign mutation.
T44 stays PAUSED and owns independent VIS45; no product/visual READY claim.
