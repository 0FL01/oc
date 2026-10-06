# T57 — actual binary fake approval and native request captures

Date: 2026-10-06. Base: `f5d78ab22`. AUTH02/AUTH04 offline evidence only:
**non-default fixture-feature ELF**, not real issuer/provider authorization or AUTH06.

## Original gate and narrow seam

AUTH02 requires actual binary pipes through fake device approval and durable account
acknowledgement; AUTH04 requires actual binary native Key/OAuth text/tool captures.
Normal builds deliberately fix issuer and native API/Codex routes. Private unit
peers alone did not close these binary gates, and host/proxy/DNS/TLS interception
would violate the preserved trust boundary.

The change envelope was recorded before implementation: optional `auth-fixture`
Cargo feature, isolated target directory, and only `OC_AUTH_FIXTURE_ORIGIN` containing
an explicit HTTP numeric IPv4 loopback origin/positive port. Missing marker,
localhost, HTTPS, credentials, non-root path, query/fragment or foreign address
fail closed. The origin is bounded to128 bytes with no controls. No default-build
override, public runtime API, dependency, registry, store or second auth owner exists.

Only this feature build redirects the existing native issuer and admitted native
OpenAI routes to the local peer. It retains the same credential namespace, actual
account transactions, refresh/attempt/Core/runtime/channel/HTTP code, DNS-answer
pinning, peer verification, no-proxy/no-redirect/no-retry factories and byte/deadline
guards. Its issuer verifies the actual loopback peer. Invalid fixture authority
cannot fall back to cloud authority. Normal production remains fixed authority;
the test build is not a release/authentication bypass or a production configuration.

## Actual ELF effects

`oc/tests/auth_oauth.rs` / `support/auth_oauth.py` execute the rebuilt feature ELF,
fresh allowlist environment, isolated protected native SQLite/project and one owned
local threaded peer. Only synthetic token/key/account/code values are used, never
printed. Synthetic source-qualified public metadata seeds the existing shared cache;
no authenticated discovery, external public GET or model request is required.

- Six missing/invalid origin cases fail before attempt/dial; peer counters remain0.
- Native **non-TTY device** prints URL/user code and waits with no callback listener
  or account. Real local POSTs return403 then404 pending; three polls have the native
  interval1s + margin3s. Explicit fake approval permits the one form token exchange;
  `Connected` follows durable SQLite acknowledgement, not code issuance.
- Native browser PKCE/state/default owned callback uses the same owner. A matching
  fake callback receives the success page only after token validation/account commit.
  Exact authorization-code/verifier/device redirect forms and two known method IDs
  survive native CLI reopen/list/switch. This is **fake approval**, not ChatGPT login.
- Real TTY masked Key addition activates the same native account owner. Actual
  `oc run` executes read → function result → final on both Key/OAuth default WS and
  explicit HTTP/SSE, with exact API model, bearer kind, subscription-only account
  routing and captured OAuth actor. Reopening the original root adds one request,
  not another settled read. SQLite has four settled reads and three acknowledged
  accounts, with no command/tool replay.
- Current local counters: **7 WS frames + 6 HTTP requests**, two token exchanges and
  four device controls. Fixture assertions verify <=24 counted local physical model
  dispatches and <=2048 output cap; not a live pre-dial guard, production step cap
  or real-provider count.
  Owned processes/server/connections close and join; no desktop/browser profile use.

## Checked commands

Approved TMPDIR, build jobs3/test threads2, serial Cargo, normal stacks; no existing
test disabled, timeout/stack override, validation weakening or baseline change.

| Command | Result |
| --- | --- |
| `CARGO_TARGET_DIR=target/auth-fixture cargo test --locked -p oc --features auth-fixture --test auth_oauth -- --nocapture` | Current **1 passed / 0 failed**, actual feature ELF, 11.64s. |
| `CARGO_TARGET_DIR=target/auth-fixture cargo clippy --locked -p oc -p oc-adapters --features oc/auth-fixture --all-targets -- -D warnings` | Exit0. |
| `cargo test --locked -p oc-adapters --lib auth0` (default) | Current **36 passed / 0 failed**, all shared auth/capture/channel/runtime guards. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` (default) | Exit0. |
| `cargo fmt --all -- --check`, `git diff --check`, progress check | Exit0. |

Feature compile/strict gate log:
`/home/opencode/.cache/opencode-tmp/opencode/t57-auth-binary-fixture.log`.
The final owning rerun above includes stronger physical transport/counter assertions.

## Material experiment and remaining boundary

An early fixture placed `transport` under a guessed nested `wire_settings`. The
native config uses flattened **options.transport**; counters exposed13WS/0HTTP,
not HTTP proof. Correcting the fixture and requiring actual POST counts/no new WS
produced7WS/6HTTP. Production transport and original tests were not changed to fit
the mistaken fixture. Separate normal build gate initially ran1711/0/11 on45 result
records plus normal release CLI/TUI PASS; its final fmt check saw newly added,
unformatted fixture files. Formatting and current default targeted/strict gates are
green; a final current full/default release and feature release run still follows.

All current successes are offline. Dedicated owner-operated ChatGPT browser/device
authorization and an explicitly permitted ordinary OpenAI test key remain required
for AUTH06 real native request/reopen. No authoring/runner/user/donor credentials,
browser extraction, Go/OpenProxy key, anonymous paid fallback or foreign listener
probe can substitute. T57 remains active; T44/VIS45 independently remains PAUSED.
