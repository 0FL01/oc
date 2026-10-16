# T57 — BUILD_READY_LIVE_BLOCKED

Date: 2026-10-06. Reviewed/pushed implementation and current gates: `d43d084fa`.
R1–R5 / AUTH01–AUTH05 are **VERIFIED offline** with owning tests, actual native
runtime and rebuilt normal/explicit-fixture binaries. Required R6 / AUTH06 real
authorization/request/reopen is **BLOCKED, NOT_RUN**. T57 is **not DONE** and
`progress.py finish` was not run. T44/VIS45 remains independently PAUSED; no full
auth-segment visual parity or overall product READY is claimed.

## Frozen outcome matrix

| Outcome | Status | Primary current evidence |
| --- | --- | --- |
| R1 / AUTH01 | VERIFIED offline | [Attempts](attempts.md), [Core/application](application.md), [CLI](cli.md), [normal TUI binary](tui-binary.md), [fixture binary](binary-fixture.md), [current qualification](qualification.md): exact method/PKCE/state/parameters, owned real loopback/fallback/foreign-port refusal, callback validation and safe pages, one native exchange/durable ack, cancel/expiry/join. Fake issuer approval is explicitly not real account authorization. |
| R2 / AUTH02 | VERIFIED offline | Attempts/application plus actual fixture ELF pipes: structured user code, immediate 403/404 pending polling with source interval+margin, fake human-shaped approval/code exchange, durable success/reopen, cancellation/failure/deadline, no no-TTY browser or local listener. Current normal CLI requires explicit target/method and refuses non-TTY key entry. |
| R3 / AUTH03 | VERIFIED offline | [Credentials](credentials.md), attempts/application/[preparation](preparation.md), CLI/TUI/binary receipts: one shared SQLite owner, migration13/legacy method refusal, safe method/account metadata, atomic selection/newest fallback, five-minute refresh, coalesced rotation/CAS, stale remove/ABA/login protection, immutable prepared tokens and durable unknown-refresh refusal across restart. |
| R4 / AUTH04 | VERIFIED offline | [Bindings](bindings.md), [catalog](catalog.md), preparation/[WebSocket](websocket.md)/[runtime effects](runtime-ws.md), fixture ELF/current qualification: native Codex versus Key routes/headers, shared public metadata and exact subscription-only overlay, all-lane captured account/model/actor, default Key/OAuth WS/explicit HTTP/affirmative same-route fallback, 55-minute affinity, fully consumed bounded continuation, no ambiguous/partial/policy/effect replay, real read/result/final, child/retry/summary/fork/opaque barriers. Local synthetic captures do not prove real server acceptance. |
| R5 / AUTH05 | VERIFIED offline | CLI/[TUI](tui.md)/normal TUI binary/current qualification: ordered methods, masked key form, own-env/config headless sources without stored accounts, metadata-only list/login/logout/switch, source accounts actions and provider-filtered picker without model commit/collision retarget, explicit open/copy, late Begin/unmount ownership, restart and safe held-request previews. No key/token enters ordinary composer/history/undo. |
| R6 / AUTH06 | Offline VERIFIED; real BLOCKED | Current gates below are green. Dedicated owner-operated real browser **and** device confirmation plus explicitly permitted ordinary OpenAI test-key authorization/request/reopen have not been supplied or performed. [Live preflight ledger](live-campaign.json) records NOT_STARTED/zero, not live PASS or a qualified pre-dial guard. Independent paired VIS45 remains T44-owned, NOT_RUN. |

## Current final gates

Serial Cargo, approved TMPDIR, CARGO_BUILD_JOBS=3, RUST_TEST_THREADS=2, normal
stacks. No failing-test disable, new ignore, timeout increase, baseline rewriting
or validation/security weakening was used.

| Command | Observed result |
| --- | --- |
| `cargo test --locked --workspace` | Exit0; **1711 passed / 0 failed / 11 unchanged opt-in ignored**, independently summed over **46 result records**. Adapter lib665/0/1, TUI446/0, binary97/0; normal debug actual CLI/TUI and all prior provider/runtime/PTY/storage/recovery/security/soak gates included. |
| `cargo build --locked`, `cargo build --locked --release -p oc` | Exit0; current normal debug/release production ELF. |
| `python3 -B crates/oc/tests/support/auth_cli.py target/release/oc` | PASS: normal release pipes, masked controlling PTYs, acknowledged account actions/restart and owned callback/cancel/opening rules. |
| `python3 -B crates/oc/tests/support/auth_tui.py target/release/oc` | PASS: normal release methods/masked keys/accounts/filter/no commit/restart, explicit open/copy and owned failure/cancel/quit cleanup. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit0. |
| `CARGO_TARGET_DIR=target/auth-fixture cargo build --locked --release -p oc --features auth-fixture` | Exit0; explicitly non-default fixture ELF. |
| `python3 -B crates/oc/tests/support/auth_oauth.py target/auth-fixture/release/oc` | PASS: fake browser/device durable approval; native Key/OAuth WS and HTTP read/result/final/reopen, separate headless own-env/config roots. |
| `CARGO_TARGET_DIR=target/auth-fixture cargo clippy --locked -p oc -p oc-adapters --all-targets --features oc/auth-fixture -- -D warnings` | Exit0. |
| `cargo fmt --all -- --check`, normal debug/release `oc --help`, `git diff --check`, journal check | Exit0. |

Current local log:
`/home/opencode/.cache/opencode-tmp/opencode/t57-current-all-offline-final2.log`,
terminal marker `T57_CURRENT_ALL_OFFLINE_GATES_PASS`. The unchanged ignores are
existing opt-ins, not evidence of a T57 live success. The feature-only binary target
runs zero tests in the default workspace because it is explicitly non-default,
not because a failing test was disabled.

Normal binaries retain fixed issuer/Codex/API routes and native DNS/peer/redirect/
retry guards. The separate `auth-fixture` feature accepts only an explicit validated
numeric loopback origin and fails closed otherwise. This closes original offline
binary gates without host/DNS/TLS/proxy interception or a default production override.
Its **13 local WS frames / 6 local HTTP model requests**, two token exchanges and
four device controls are synthetic fixture counters, never live counters. Six
native reads settle across stored Key/OAuth and empty own-env/config roots; reopening
does not repeat a settled read. No real issuer, model, browser profile or key was used.

## Reviewed/pushed slices

1. `85d3f5ea2` — scoped metadata/migration13, guarded coalesced refresh/rotation.
2. `0bdc1b47b` — owned browser/device attempts and store-once callback lifecycle.
3. `4a4364cde` — acknowledged Core/application auth methods/actions/joined lifetime.
4. `5b85cb36c` — captured native Key/OAuth authority, headers and issuer guards.
5. `7410b2ede` — one public metadata cache and scoped subscription catalog views.
6. `6ea620032` — selected all-lane native credential preparation/capture.
7. `a318fa52f` — bounded native channel owner, continuation/fallback/retry/error guards.
8. `29567936b` — actual runtime effects, child/summary/retry/fork/opaque barriers.
9. `2880a90ab` — shared native auth CLI and actual pipe/PTY controls.
10. `cd141cba9` — mounted TUI auth/accounts/picker and metadata-only preview.
11. `f5d78ab22` — actual normal TUI PTY/restart/owned browser controls.
12. `45864ce44` — explicit fixture ELF fake approval and native request proof.
13. `d43d084fa` — partial-write menu barrier and headless env/config binary sources.

Concurrent other-owner **plan-only** `8c264855a` restricted-error amendment was
preserved, not counted as T57 implementation. Its required channel consumer is
qualified through the existing common classification/redaction/retry facts; no
new failure DTO, second retry engine or adjacent T54/T45 completion was introduced.
Production owners/tests/fixtures are mapped in `docs/CODE_MAP.md`; component receipts
record failed hypotheses and exact corrections. Existing >5k application/turn
files retain documented coupled-owner/natural-seam warnings; no minification or
generic framework/file dumping was used.

## Material failures resolved, not hidden

- Legacy unknown/incomplete provider prose became visible during channel work;
  unchanged privacy regressions exposed it. Shared classification now retains
  existing withholding for unknown/incomplete while redacting admitted categories.
- Non-native precancel changed an existing compaction contract; preparation became
  a strict no-op outside admitted built-in OpenAI. Original regression stayed intact.
- Enlarged inline account state overflowed an unchanged normal-stack restoration
  test. An inline-sync hypothesis was disproved and removed; one private boxed owner
  fixed the measured restoration path without a stack override or test change.
- Fake summary/fork/retry fixture assumptions were corrected against real settled
  prefix/section/SQL facts, not by weakening runtime validation or replay protection.
- The fixture initially misnested `transport`; actual counters disproved its HTTP
  claim. Correct flattened configuration now proves physical SSE versus WS.
- A cold normal-release CLI fixture observed only the first menu write. Waiting for
  the last row within its original deadline fixed the fixture, not production input.
  Current cold rebuild/full gates replaced all stale/earlier partial evidence.

## Concrete external blocker and checked alternatives

The remaining dependency is **authorization/input**, not an unresolved local error:
the current request supplies no dedicated operator confirmation for a real ChatGPT
browser/device login and no explicitly permitted ordinary OpenAI test key/input.
No claim is made that a particular env variable is absent, a key is revoked, or
OpenAI rejected credentials: real issuer/model requests were deliberately NOT_RUN.
Source device flow requires human approval; generated PKCE/state or synthetic
tokens cannot grant that approval or replace a real server-issued authorization.

Safe alternatives were assessed against the fixed contract:

- Current public metadata and fake issuer/native fixtures prove offline behavior,
  not live authorization/request/reopen. They cannot replace AUTH06.
- User/authoring/runner/donor credentials and browser profiles are forbidden input
  sources and were not inspected or imported. No browser/account scraping occurred.
- Previously authorized Go/OpenProxy credentials have different authority/scope;
  reusing them, an anonymous/paid fallback, changing native routes or bypassing DNS/
  peer/TLS/redirect protection would invalidate the task and was not attempted.

Live campaign `t57-openai-20261006`: **NOT_STARTED, 0/24 physical generations**,
zero token/refresh/device controls and unknown effects. This durable zero receipt
is a preflight, **not** evidence that a live dispatch guard has been qualified.
On explicitly authorized operator/test-input availability, reuse this campaign,
establish locked durable pre-dial <=24 accounting across all main/follow-up/title/
summary/child/retry/WS/HTTP fallback sends and <=2048 smoke output before the first
request, track token/control traffic separately and keep the ten-minute attempt
deadline. Then record real browser/device/key request/reopen receipts and current
affected gates before any finish. Do not silently reset counters or replay unknown
effects. No independent in-scope offline blocker remains; no unrelated task was begun.

T57 stays BLOCKED, not DONE. T44/VIS45 is not resumed/waived by this backend report.
User-owned `.opencode/` remains unread/unstaged; T53's campaign and T56/other task
ownership/history are unchanged. No full product READY or visual parity claim.
