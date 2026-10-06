# Goal: Codex / ChatGPT OAuth и обычный OpenAI API key

Status: active (T57 explicitly started after completed T56; implementation in progress)
Source: владелец 2026-10-06 потребовал полноценный Codex OAuth, headless авторизацию,
обычный OpenAI API key, донорскую логику, полный backend и визуальный паритет,
исправление конфликтов плана и commit/push текущей ветки.
Task: T57; backend AUTH01–AUTH06. T44 отдельно владеет mandatory VIS45.
Donor: OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`.

## Objective

Native `oc` подключает built-in `openai` тремя настоящими методами: ChatGPT Pro/Plus
(browser), ChatGPT Pro/Plus (headless/device), API key. OAuth использует ChatGPT/Codex
subscription endpoint, Key — обычный OpenAI API. CLI и TUI используют один credential/
attempt owner; вход, выбор аккаунта/модели, запрос, refresh и restart работают реально.
Сегмент `/connect` → method/accounts → authorization → `/models` имеет полный
визуальный и интерактивный паритет pinned TUI, с явно перечисленными security
отличиями ниже. Storage-only OAuth, ссылка без polling, HTTP-only smoke вместо
донорского transport или native-only golden не являются завершением.

Headless здесь означает device authorization: `oc` показывает URL/код, пользователь
подтверждает вход в браузере другого устройства. Не обещается unattended вход без
пользователя. Это отдельный CLI flow, а не требование запускать TUI без терминала.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: настоящий browser OAuth с PKCE и loopback callback.
  - Source: полный donor auth-flow; `core/src/plugin/provider/openai.ts:50–167,319–335,377–401`.
  - Acceptance: AUTH01 доказывает donor method ID/label, authorize parameters,
    callback/state/code validation, token exchange, truthful terminal status и cleanup.
  - Primary evidence: source-derived fake issuer + real loopback tests, затем rebuilt
    native binary с callback/error/cancel/occupied-port barriers; никакого реального входа.
  - Status: in_progress
  - Evidence: evidence/T57/attempts.md / application.md — real owned loopback/fake issuer PKCE, callback/error/ports/cancel/expiry and durable single-account ack; Core actions and joined application lifetime across Location/busy stream verified, rebuilt native consumer qualification remains pending.
- R2: полноценный device OAuth и headless CLI.
  - Source: явный запрос headless; `openai.ts:169–228`, CLI `handlers/auth/login.ts`.
  - Acceptance: AUTH02 доказывает usercode → pending polling → code exchange → stored
    account, no-TTY target/method flow, cancel/expiry/failed и отсутствие local listener
    или browser launch в no-TTY. Login не сообщает success до durable owner ack.
  - Primary evidence: fake-clock/issuer + actual binary с pipes и PTY; inspect request
    counters/intervals, CLI exit/status и account reopen.
  - Status: in_progress
  - Evidence: evidence/T57/attempts.md / application.md — actual usercode/pending polling/exchange/cancel/expiry/reopen owner proof and typed Core/application method/action bridge; no-TTY CLI/PTY consumer qualification remains pending.
- R3: общие accounts, OAuth metadata и automatic refresh.
  - Source: полный backend parity; donor `integration.ts`/`credential.ts`, existing T53 GO01.
  - Acceptance: AUTH03 доказывает transactional add/activate/rename/remove, Key/OAuth
    restart, method/account metadata, refresh за пять минут до expiry, rotation persistence,
    concurrent resolution и stale completion protection без второй credential DB.
  - Primary evidence: synthetic credentials + owning storage/resolver tests с barriers,
    fake clock/refresh counters и native reopen; safe DTO/redaction assertions.
  - Status: in_progress
  - Evidence: evidence/T57/credentials.md / attempts.md / application.md / preparation.md — native token/metadata parser, shared refresh/CAS/reopen/no unknown replay, store-once login/per-namespace stale fences, safe acknowledged method/account projection and selected per-request native refresh/capture; end-to-end consumers and full AUTH03 remain pending.
- R4: subscription-authorized requests, catalog и donor transport.
  - Source: полный backend parity; `openai.ts:230–317`, `session/model-transport.ts`,
    AI `openai-responses.ts`/`open-responses-channel.ts`, T50/T53 captured binding.
  - Acceptance: AUTH04 доказывает Codex route/headers/auth/catalog transform, real
    text/tool roundtrip, default Responses WebSocket и admitted HTTP mode/fallback,
    root/follow-up/title/summary/child/retry bindings; account/key switch, DCP/fork/
    restart не переносят чужие opaque/checkpoints или credentials и не повторяют tools.
  - Primary evidence: fake HTTPS/WS issuer/provider captures + runtime/storage barriers,
    rebuilt headless/PTY roundtrip и cleanup/resource receipts.
  - Status: in_progress
  - Evidence: evidence/T57/bindings.md / catalog.md / preparation.md / websocket.md / runtime-ws.md — admitted Key/OAuth authority/catalog, selected all-lane preparation, checked local native channel ownership and actual runtime read/result/final/fork/opaque/retry/child/summary effects; rebuilt headless/PTY consumers and live provider qualification remain pending.
- R5: обычный OpenAI API key и shared functional connect/account/model consumers.
  - Source: явный API-key запрос и полный parity; AI `providers/openai.ts`, TUI
    `dialog-integration.tsx`, CLI auth handlers, T53 GO05.
  - Acceptance: AUTH05 доказывает key form/env/config auth, обычный OpenAI route и
    каталог без Codex-only filters/headers, working `/connect`/accounts → provider
    picker, CLI list/login/logout/switch и restart. Key↔OAuth выбор явный, без fallback.
  - Primary evidence: source-derived key/OAuth separation fixtures + actual binary
    PTY/pipe actions и captured requests; token/key never enters transcript/composer.
  - Status: in_progress
  - Evidence: evidence/T57/cli.md / tui.md — shared native CLI plus mounted TUI methods/pending/cancel/accounts and acknowledged provider-filtered picker/no model commit; metadata-only previews leave held requests intact. Rebuilt TUI PTY/release and real provider qualification remain pending. Presentation — only T44/VIS45.
- R6: current offline и bounded real authorization/request qualification.
  - Source: полноценный backend и визуальный результат, existing A01–A13/live rules.
  - Acceptance: AUTH06 требует current nearest/impacted/final gates и opt-in native
    browser OAuth, headless device OAuth и ordinary API-key login/request/reopen
    receipts. Full auth-segment parity заявляется только вместе с T44/VIS45 PASS.
  - Primary evidence: factual report с commits/commands/exits, sanitized durable
    campaign counters и ссылкой на independent paired VIS45 evidence.
  - Status: pending
  - Evidence: pending; этот plan delivery не запускает live login или generation.

### Donor behavior to port

**Scenario ownership.** Generic SQLite account lifecycle/storage and functional
Go/custom connect/picker baseline remain GO01/GO05 under T53. AUTH03/AUTH05 reuse
their qualified evidence/regressions; new proof here covers OpenAI method/account
metadata, OAuth refresh, mixed Key/OAuth account switching and OpenAI-specific
CLI/TUI consumers. No generic T53 scenario is reassigned or copied to a second suite.
VIS45 qualifies presentation independently, including shared screens; AUTH06 owns
only this auth segment's current live/closure proof, not T53 GO06 or global T27.

**Browser.** `chatgpt-browser`, label `ChatGPT Pro/Plus (browser)`. Public client ID
`app_EMoamEEZ73f0CkXaXp7hrann`, issuer `https://auth.openai.com`. PKCE S256, 43-character
verifier, random 32-byte base64url state. `/oauth/authorize`: response_type=code,
scope `openid profile email offline_access`, id_token_add_organizations=true,
codex_cli_simplified_flow=true, originator=opencode, exact redirect and state/challenge.
Loopback-only `localhost:1455/auth/callback`, then1457; donor10 bind attempts per port,
200ms delays. Wrong path/state/missing code/provider error is not success. Callback
success/error HTML follows donor `oauth/page.ts`; escape unsafe content, never echo
tokens/code into diagnostics. Exchange `/oauth/token`, form authorization_code with
client_id/code/redirect_uri/code_verifier. Listener belongs to one cancellable attempt.

**Device.** `chatgpt-headless`, label `ChatGPT Pro/Plus (headless)`. POST JSON
`/api/accounts/deviceauth/usercode` with client_id; present `/codex/device` and
`Enter code: <user_code>`. Poll POST `/api/accounts/deviceauth/token` with returned
device_auth_id/user_code. Parse interval/default5s/min1s + donor3s safety margin;
403/404 are pending, other non-success statuses fail. Success supplies authorization_code
and code_verifier; exchange at `/oauth/token` with redirect
`https://auth.openai.com/deviceauth/callback`. User-Agent follows native application
identity through existing owner. No imaginary device-cancel endpoint.

**Attempt lifecycle.** Donor pending/complete/failed/expired, default10min lifetime,
terminal retention1min, cleanup cadence30s. Preserve expiry and immediate cancel/close
semantics with bounded owned tasks/current-state projections; no perpetual loop or
persisted live attempt label after restart. Store the account once, after validated
tokens and durable ack; late duplicate completion/expiry/cancel cannot create two rows.
Unknown token-exchange result after interruption is not automatically replayed.

**Tokens/accounts.** Extend existing tagged OAuth material additively with methodID
and optional account metadata; legacy Key/OAuth rows remain readable, unknown method
stays unsupported. expiry = now + (expires_in ??3600)s. Account ID: ID-token claim,
then access-token claim; each uses chatgpt_account_id → nested
`https://api.openai.com/auth.chatgpt_account_id` → first organizations[].id. Donor
decodes JWT payload, does not verify its signature: this metadata is routing data
from the trusted token exchange, not an independent authorization grant. Validate
structured payloads; do not parse free LLM/error text. Refresh `/oauth/token` with
grant_type=refresh_token/refresh_token/client_id, preserve old metadata only when
the refreshed tokens yield none. Resolver refreshes when expiry ≤ now+5min. Single
credential owner coalesces concurrent refreshes; rotation/activation/removal rechecks
account identity before commit. Failed/revoked refresh is explicit unavailable/reauth,
not fallback to API key, another account, OpenProxy or anonymous access.

**Binding/catalog.** Integration/provider ID remains `openai`, not a second `codex`
provider catalog. Active stored account (Key or admitted OAuth method) → own
OPENAI_API_KEY → own configured Key after endpoint authority admission; no implicit
env override of an active OAuth account. Existing custom/OpenProxy configured-first
priority remains unchanged. OAuth → `https://chatgpt.com/backend-api/codex`, Bearer
access token, originator=opencode, x-codex-beta-features=remote_compaction_v2,
optional chatgpt-account-id, per-session session-id; API key → normal
`https://api.openai.com/v1`, Bearer key, admitted organization/project settings, no
Codex account/header injection. Public `openai` models.dev slice is credential-free
and shares existing catalog/cache owner, not a new registry or guessed /models probe.

Port the pinned **subscription-only** catalog transform exactly against `modelID ?? id`:
disable reasoning.mode=pro; allow gpt-5.5/gpt-5.3-codex-spark; deny gpt-5.5-pro/gpt-5.6;
otherwise donor numeric gpt major>5 or major=5/minor>4 predicate. Eligible costs=[],
context400000/input272000; normal Key retains unmodified catalog/limits/costs.
These pinned donor auth rules are a narrow approved exception to the no-model-name
routing/allowlist policy, not a static model catalog or reasoning capability heuristic
for Go/OpenProxy/custom providers. Unknown eligible IDs do not require a Rust edit.
Account switches rebuild the effective view; unavailable saved choice remains explicit.

Default OpenAI Responses transport is WebSocket in the donor plugin for **both**
OAuth and Key; explicit HTTP remains respected. Port native session-owned handshake,
request framing, connection affinity/rotation, acknowledged checkpoint publication,
error/cancel cleanup and source-derived same-route HTTP fallback. Connect failure/
provably not-sent and explicit rejection are different from ambiguous delivery or
partial output. Do not replay a sent request/effect simply because the channel died.
Use T54's single logical-step retry owner, not another generation retry loop.
Transport fallback cannot change credential/model/protocol/endpoint. This narrow
WebSocket scope is required for AUTH04, not a generic daemon/serve/attach service.

Owner-approved 2026-10-06 [restricted-error contract](2026-09-29-provider-retry-parity.md#restricted-provider-errors-and-safe-details--approved-2026-10-06-pending)
extends this AUTH04 consumer: normalize source-approved `response.failed`/`error` and
bare `{error}` envelopes through the common classifier/request-local redactor. Keep
policy failure through channel close; it neither becomes retryable EOF nor authorizes
HTTP fallback. A successful handshake is not per-request retry-override evidence for
later responses; require source-approved attempt-local provenance or terminal default.
Qualify this on the minimal transport slice without a second retry loop, raw error/header
archive, full T54/T45/T44 dependency or new auth gate. New assertions remain pending/NOT_RUN.

Codex beta header does not itself authorize hosted tools or a new remote-compaction
feature; existing compact/DCP compatibility remains under its owners.

Prepared requests/tools keep cloned binding/account attribution; refresh/new account
affects a later request preparation, never rebinds an in-flight request. T50 same-task
committed switch preserves ordinary tool pairs once; subscription/key/account scopes
partition opaque state and WS affinity. Raw history/forgotten HOT/no-effect-replay
constraints remain, including auxiliary lanes and separate children.

**Consumers.** `/connect` lists real providers, existing credentials lead to accounts,
Add account opens method selection, OAuth methods precede Key and env method is not
selectable. Starting → URL/instructions → Waiting → success/error/expired; TUI `o open`
and `c copy`, Esc close cancels unsettled attempt, donor status polling500ms translated
through bounded existing events/deadlines. Accounts: Add first, labels then ID ordering,
active marker, activate/rename and two-trigger destructive delete confirmation. On
success refresh provider/model views and open provider-filtered picker; do not silently
choose a model or confuse same-ID models. Pending/auth-required is not connected;
saved key/public catalog are not server-validated authorization.

CLI: `oc auth login [target] [--method ...] [--answer key=value ...]`,
`oc auth list [--format default|json]`, `oc auth logout [target] [credential]`,
`oc auth switch [target] [credential]`, through the same native owner. For known
`openai`, methods `chatgpt-browser`, `chatgpt-headless`, `key`; no-TTY multiple-method
login requires explicit target/method. Auto OAuth prints URL/instructions, opens
browser only with stdin+stdout TTY; device no-TTY remains human-authorized and blocking.
Key CLI uses password prompt requiring TTY; headless generation can use admitted
OPENAI_API_KEY/config or stored account. Do not add plaintext key/token argv flags.
List JSON is metadata-only. Native auth commands require no selected model, sessions,
MCP/paid generation or remote daemon; general well-known/custom auth-command execution
and donor --server/--standalone compatibility remain outside this slice.

### Constraints, scope conflicts and explicit security differences

- T57 supersedes earlier blanket no-provider-OAuth/no-Codex/no-auth-CLI restrictions
  only for this built-in OpenAI flow. T53 GO01–GO06 retain ownership and their frozen
  Go/custom scope; T57 consumes qualified credential/catalog/wire/connect seams, not
  a duplicate store. MCP OAuth and other providers' OAuth remain excluded.
- No auth.json/import/dual-write, reading donor/user/runner credentials, Node/Bun/JS
  host, cloud orchestrator, daemon/serve/attach, generic auth SDK, keyring promise or
  hosted tool/image-generation expansion. No browser/profile/password scraping.
- T53 masked ephemeral secret handling is stronger than donor TUI's plaintext
  DialogPrompt. Preserve it: full auth geometry/styles/keys/state parity with one
  disclosed secret-glyph difference, never unmask real keys for a screenshot. This
  is not unconditional byte-for-byte parity of a plaintext secret field.
- Donor port-conflict `/cancel` probe must not control an unrelated local service.
  Native cancellation is allowed only for a provably owned attempt; otherwise use
  the same bind retry/fallback ports/headless guidance without the arbitrary probe.
  Scope this safety difference to occupied-port recovery, not missing browser flow.
- Secrets/state/verifier/token bodies never enter Debug/events/history/diagnostics/
  evidence; URL and device code appear only in the active auth surface and explicit
  copy/open actions, not conversation or generic diagnostics. DB/root/WAL restrictions,
  trust/endpoint/redirect guards, finite resource caps, cancellation and cleanup remain.

## Change Envelope and ordered slices

1. Freeze pinned differential fixtures/methods/CLI/TUI sources and qualify the shared
   T53 seams needed here; schedule T57 only at a safe handoff, no all-task done cycle.
2. Extend existing `storage_credentials.rs`/`auth.rs` owner and typed core/application
   actions/snapshots; method metadata/refresh/attempt lifecycle tests before consumers.
3. Native browser PKCE/callback and device polling in one coarse OpenAI auth module
   using existing transport/cancel/redaction; fake issuer/clock and real-loopback gates.
4. Admit Key/OAuth bindings, OpenAI public catalog transform, native WS/HTTP owner and
   all request/history lanes; fake tool roundtrip/switch/restart/resources before live.
5. CLI auth and actual TUI connect/accounts/model consumers through shared ack/actions;
   AUTH02/AUTH05 binary behavior first. T44/VIS45 styling/captures after explicit resume,
   using minimal qualified backend slices, not whole-T57/T44 circular dependency.
6. Current impacted/workspace gates → bounded owner-operated live → factual T57 report;
   pair with independent VIS45 result before claiming full auth-segment parity.

Expected paths: core `CoreApp` actions/queries, adapters auth/storage_credentials/
provider_catalog/models_dev/composition/provider/runtime/storage replay, binary cli/
bootstrap/headless/tui_cmd, TUI app/input/dialogs/picker and nearest separate tests.
No new crate/framework/store solely for arrangement. Sources/locators are recorded in
`tui-recovery/SOURCES.json` U116–U128; method is in `docs/TEST_PLAN.md` AUTH01–AUTH06/VIS45.

## Current State / checkpoint

- 2026-10-06: owner explicitly requested T57 after T56 closure `0879e199b`;
  T57 active, T53/T56 complete, T44 PAUSED. Frozen R1–R6 unchanged; VIS45 remains
  independent, no historical reports/baselines rewritten.
- First implementation slice: same SQLite credential table additively preserves
  legacy OAuth and adds methodID/accountID metadata, separate selection/material
  revisions and durable refresh reservation (migration13). Native resolver uses one
  shared refresh flight, exact account/epoch CAS, trusted structured claim priority,
  five-minute refresh and rotation; unknown/failed/cancelled refresh requires reauth,
  never implicit key/env fallback. Native expiry remains Unix seconds (donor ms
  converted at the boundary); public summaries contain no tokens/routing account ID.
- Current checked evidence: `evidence/T57/credentials.md`; owning auth03_ 5/0,
  auth05_ 1/0, existing go01_ 13/0 and child_schema_ 3/0; strict impacted all-target
  Clippy/fmt/diff green. This does not prove login, connected TUI, WS or live access.
- Second checked slice: owned native browser PKCE/loopback and device usercode/poll/
  exchange, typed redacted snapshots, finite deadlines/retention/idle cleanup,
  joined cancel/shutdown and store-once stale-account fences. Same SQLite owner;
  issuer/socket tests only, no browser/profile imports or real authorization.
- Current owning checks: auth0 12/0 and full adapters lib 641/0/1, strict workspace
  all-target Clippy/fmt/diff green; evidence/T57/attempts.md. This is partial owner
  evidence, not rebuilt application/CLI/TUI or WS/live qualification.
- Third checked slice: typed Core method/auth actions and safe account method ID;
  one application-lifetime attempt owner with joined cleanup after all worker exits,
  no Location/endpoint rebinding and no model cancellation during auth commands.
- Current application checks: auth0 15/0, adapters lib 644/0/1, Core 32/0, TUI 444/0;
  strict workspace all-target Clippy/fmt/diff green. `evidence/T57/application.md` is
  backend bridge evidence, not rebuilt login consumers, OAuth execution or live PASS.
- Fourth checked slice: admitted OpenAI composition templates use the shared async
  resolver; immutable subscription/Key route, native session/account headers and
  account-partitioned replay scope retain captured tokens across later rotation.
  Issuer POSTs reuse DNS/peer/redirect guards. Existing Go/custom scope encoding and
  source priority remain unchanged; `evidence/T57/bindings.md` is partial AUTH04.
- Current binding checks: final adapters lib 645/0/1, auth04_ 1/0, strict workspace
  all-target Clippy/fmt/diff green. No actual model request or WS/live proof here.
- Fifth checked slice: shared credential-free models.dev OpenAI slice/cache and exact
  subscription-only catalog transform, finite captured leaves, connection/model/lookup
  and protected metadata-only read-only listing. Independent previews do not refresh
  unrelated accounts; shared cache changes publish without an unchanged-cache loop.
- Current metadata checks: adapters lib649/0/1, auth04_5/0, go02_13/0, go05_8/0;
  strict workspace all-target Clippy/fmt/diff green. `evidence/T57/catalog.md` remains
  partial AUTH04, not native WS/actual request/login/live evidence.
- Sixth checked slice: native selected-leaf request preparation uses the existing
  refresh/capture owner before main/follow-up/retry/child/title/summary attempts.
  Issued credentials/actor/model remain immutable; subscription budgets/gate match
  catalog, profile overlays cannot replace native identity. Summary retries refuse
  changed authority rather than replaying already-projected opaque state.
- Current preparation checks: auth04_ 8/0, adapters lib 652/0/1, Core 32/0, TUI 444/0
  and strict workspace all-target Clippy/fmt/diff green; evidence/T57/preparation.md.
  A reproduced non-native pre-cancel regression was fixed without changing its test.
  No WS/default transport, rebuilt consumer, real OAuth/key or complete AUTH04 claim.
- Seventh checked slice: native session-owned Responses channel, default Key/OAuth
  WS and explicit HTTP, same-authority safe fallback/pin, 55-minute affinity and
  fully consumed continuation proof. Unknown channel delivery cannot replay;
  continuation rejection uses the existing finite retry owner. Common restricted
  classification/redaction preserves terminal policy and older unknown-message
  privacy. Foreign WS is unready before admission; joined channel cleanup is owned.
- Current channel checks: auth04_18/0, adapters lib662/0/1, strict workspace all-target
  Clippy/fmt/diff green; evidence/T57/websocket.md. Real local WS/HTTP peer captures
  are not real OpenAI/Codex, rebuilt CLI/TUI or complete AUTH04/AUTH06 evidence.
- Eighth checked slice: real Runtime WS read/function-result/final, settled receipt
  fork and changed-account opaque filtering, counted existing-owner continuation
  recovery, non-replayed delivered failures, actual child actor/policy lifecycle
  and ordinary summary checkpoint. Only the owning synthetic peer/tests changed.
- Current runtime checks: auth04_21/0, adapters lib665/0/1 and strict workspace
  all-target Clippy/fmt/diff green; evidence/T57/runtime-ws.md. These local native
  runtime effects are not real OpenAI/Codex authorization or rebuilt CLI/TUI/live.
- Ninth checked slice: native built-in OpenAI CLI login/list/logout/switch through
  the same credential/attempt owner, independent of runtime config/model/session/MCP.
  Safe explicit non-TTY method/target, masked TTY-only keys and metadata-only JSON;
  owned browser failure/cancel and durable account actions in current native ELF.
- Current CLI checks: binary unit94/0, actual auth CLI pipe/PTY1/0, adapter auth0_36/0
  and strict workspace all-target Clippy/fmt/diff green; evidence/T57/cli.md. No real
  issuer/device/model authorization, release or full AUTH05/AUTH06 claim.
- Tenth checked slice: shared mounted TUI methods/Starting/Waiting/open/copy/cancel,
  source account actions, acknowledgement-first provider picker and metadata-only
  idle/held catalog previews. Late Begin/unmount is joined and cancelled by owned ID;
  active details never reappear from parked views. One private boxed account owner
  retains normal-stack restoration without changing existing tests or stack size.
- Current TUI checks: adapters lib665/0/1, auth0_36/0, TUI446/0, binary unit97/0 and
  strict workspace all-target Clippy/fmt/diff green; evidence/T57/tui.md. No rebuilt
  TUI login, release, real authorization or VIS45/full AUTH05/AUTH06 claim.
- Next checkpoint: rebuilt normal TUI PTY and current debug/release/workspace gates,
  then bounded dedicated live authorization/request proof.
- Live prerequisites are dedicated owner-operated ChatGPT login and OpenAI test key,
  not authoring-agent auth. Missing prerequisites block only required live proof;
  independent offline work proceeds. No READY from docs, storage or fake-only PASS.

## Completion

Pending. T57 completion requires AUTH01–AUTH06 with current factual evidence; full
auth-segment backend+visual parity additionally requires T44/VIS45. Final product
READY still requires all A01–A13 and other mandatory outcomes.
