# T57 — owned browser/device authorization, partial native slice

Date: 2026-10-06. Base: reviewed/pushed `85d3f5ea2`. R1/R2/R3 owning offline
behavior is checked; CLI/TUI/application admission, generation transport and live
authorization are still pending. This is not AUTH01–AUTH06 completion or VIS45.

## Source and implementation

Pinned OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`,
`opencode/packages/core/src/plugin/provider/openai.ts:50–228` and
`core/src/oauth/page.ts:21–49,112,140–276` supply the method labels, PKCE/state,
authorize fields, loopback ports/retries, device JSON/interval/pending statuses,
exchange and self-contained callback-page messages/theme/wordmark. Native
`auth/attempts.rs` owns actual Tokio sockets, bounded issuer HTTP futures, typed
state and joined cancellation; `queries/auth.rs` exposes no verifier/tokens/socket.
The callback HTML adaptation preserves the upstream MIT notice.

- Browser: cryptographic 32-byte state and PKCE verifier (43 base64url characters),
  SHA-256 challenge, exact pinned client/parameters, loopback-only 1455 then 1457,
  ten binds per port with 200ms retry delays. No arbitrary `/cancel` probe or foreign
  process control. Replacement cancels and joins only an attempt known to this owner.
- Callback: bounded 8KiB GET/headers, wrong path 404/pending, duplicate/missing code,
  provider error or state mismatch never succeed. Error details are fixed typed
  messages, not arbitrary callback text/code. HTML escaping, light/dark native
  branded page, offline SVG and 2500ms close follow the source; success is deliberately
  sent only after validated tokens and durable account acknowledgement.
- Device: JSON usercode request, explicit `/codex/device` / `Enter code: ...`,
  immediate first token poll; 403/404 remain pending, all other errors fail. The
  structured string interval has donor parseInt/default-five/minimum-one semantics
  plus the three-second safety margin. A longer-than-lifetime wait expires rather
  than overflowing a timer. No listener, browser launch or invented cancel endpoint.
- Both: the same admitted token client, no redirect/retry, 64KiB response bound;
  method metadata and native seconds persist only in the existing credential table.
  No live attempts or unknown token exchange are persisted/restarted/replayed.
- Lifecycle: maximum eight retained attempts, ten-minute monotonic deadline,
  one-minute terminal retention, 30-second cleanup only while the registry is nonempty.
  Typed pending/complete/failed/expired snapshots redact URL/state/instructions in
  Debug; active auth consumers alone can present them. Explicit cancel awaits the
  owned worker/socket release; shutdown rejects further admission, cancels and joins
  all work. Drop defensively cancels/aborts. No daemon or perpetual idle loop.
- Store-once: one worker and locked terminal transition gate the durable insertion.
  Shared-Db per-namespace ephemeral selection epochs fence late login against account
  switch/removal and empty→Key→empty ABA in the same transaction/connection owner.
  Rename and idempotent activation do not invalidate selection; unrelated namespaces
  remain independent. These guards are not another credential database or a restart
  ticket; no attempt survives reopen.

## Actual offline evidence

Six new owning scenarios use real loopback listeners and fake issuer TCP requests,
not mocked authorization success or real login:

1. Browser exact authorize fields/state/PKCE→actual callback→form exchange→durable
   account, success HTML, redirect/verifier/challenge equality, duplicate refused,
   single row and native Db reopen.
2. Wrong path/state/missing code/provider error, safe error HTML, occupied foreign
   listener receives zero connections, fallback binds, owned replacement/cancel,
   expiry, retention/idle timer restart and listener closure.
3. Device 403 then 404 then code/verifier exchange; observed request order/JSON and
   actual one-second fake-interval gaps, exact production three-second margin/defaults,
   no connection to occupied callback ports, method persistence/reopen.
4. Held token exchange plus cancellation or concurrent shared-handle empty-account
   ABA: no late activation/duplicate row, one exchange and no restart continuation.
5. Device nonpending failure, pending cancellation and deadline expiry stop polling,
   retain truthful terminal status and store no account.
6. Eight pending device operations; ninth refused before admission, acknowledged
   shutdown joins all and cannot be reused for another start.

Current commands (approved TMPDIR, CARGO_BUILD_JOBS=3, RUST_TEST_THREADS=2,
normal stacks, sequential Cargo):

| Command | Actual result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib auth0` | 12 passed / 0 failed. Includes existing refresh/key/legacy CAS scenarios. |
| `cargo test --locked -p oc-adapters --lib` | 641 passed / 0 failed / 1 existing Go opt-in ignored; 93.66s test execution. All existing scoped credentials/GO, schema, provider, runtime, security, PTY and storage owner regressions passed. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit 0 after final test-scoping correction. |
| `cargo fmt --all -- --check`, `git diff --check` | Exit 0. |

Full adapter output: local tool log
`/home/opencode/.local/share/opencode/tool-output/tool_1113319c2001pKCtPc4qxPjL25`.
Clippy initially rejected two test MutexGuard scopes crossing awaited shutdown;
bounded request observations now clone before await, without suppressions or new API.
One page/lifecycle patch failed atomically on rustfmt context, then was applied against
actual lines. No baseline/validation weakening, timeout/stack override, dependency,
real API call, browser/profile/runner credential read or live-key extraction occurred.

## Next contracted checkpoint

Wire these typed actions into the application/Core owner, then admit built-in OpenAI
Key/OAuth catalog/bindings and native Responses WS/HTTP with all-lane capture and
replay guards. CLI/TUI real binary and dedicated authorized live proof remain mandatory.
T44 stays PAUSED and separately owns VIS45. T53/T56 evidence, campaign and user-owned
`.opencode/` are unchanged; T57 remains active, not complete.
