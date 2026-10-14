# T59 — native custom connections, discovery and familiar connect flow

Date: 2026-10-14. Status: COMPLETE; frozen **R1–R5 / CONN01–CONN05 verified**.
Reviewed final code: `42c02605358a215c7719ac65e98d80f0988c60df`.
Source slices: `2747f9203` native owner, `0912c06b8` typed metadata,
`d700a4e63` revision-scoped onboarding, `42c026053` final discovery/publication.
Base before T59: `0d59c8e92`. Separate concurrent docs-only amendment `f3a92b599`
is preserved, not authored or qualified by this task. This report is not product
READY, fresh live-provider qualification or T44 visual parity.

## Source and interpretation

The frozen contract is `docs/goals/2026-10-12-custom-provider-connect-and-discovery.md`;
methods and mandatory filters are in `docs/TEST_PLAN.md`, CONN01–CONN05. The UI oracle
is pinned OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`, especially
`packages/tui/src/component/dialog-integration.tsx` and core integration/credential
owners. It supplies provider search/categories/source markers, accounts/methods,
single-method shortcut, default label, Rename and acknowledged connect→models.
Active-account Enter remains a no-op; account activation is not a model commit.

The donor has no equivalent endpoint wizard. Custom setup and capability details
are explicit native extensions. Key glyphs are masked rather than donor plaintext.
OpenAI list metadata cannot distinguish Responses from Chat: the user explicitly
confirms one of the existing three APIs. Anthropic catalog shape suggests a family,
not a guarantee about generation routing. Only supplied capabilities/efforts are
imported; absence/null, false and declared empty remain distinct. No moving web
documentation, model-name heuristic or generation detection POST replaces evidence.

## Frozen outcomes and direct proof

| Outcome / gate | Implementation and primary evidence |
| --- | --- |
| R1 / CONN01 — durable scoped connection | `storage_provider_connections.rs` adds migration14 and a bounded typed table to the existing SQLite owner. Connection and initial default Key commit atomically through the shared credential insertion path; anonymous creates no credential. Owner tests prove rollback including credential epochs, reopen, tagged custom versus configured scopes and literal URL negatives. `composition/custom_connections` reserves raw configured IDs before filters, including disabled IDs; config wins and whitelists remain enforced. Startup, real held Location switch and existing `session_move::prepare` attach before credentials. Actual debug/release PTYs reopen the saved binding and session. |
| R2 / CONN02 — bounded read-only discovery | `custom_discovery` reuses secured captured transport and allowlisted model parsing, GETs the literal prefix plus `/models`, uses finite Bearer/x-api-key candidates on that endpoint or explicit no-auth, and never sends generation for detection. Five owner cases cover ambiguous OpenAI lists, rich metadata under arbitrary IDs, Anthropic pagination/auth, null/false/empty, cursor/duplicate/partial failures, valid empty, 12 pages, bytes/rows/cancel/secret rejection, retries and a single monotonic30s deadline with attempts at most15s. Failed/partial attempts retain last-good data; custom empty clears it. Legacy OpenProxy parser/empty semantics are unchanged. |
| R3 / CONN03 — honest metadata through actual consumers | Typed `ModelCapabilities`, partial flags and `variants_known` cross `ModelEntry`→application snapshot→TUI reconstruction→focused picker details. Owner and narrow/wide renderer cases retain unknown/false/empty and exact custom effort. Full input modalities override partial image flags; unknown refuses images, and PDF/audio/video/image generation are not enabled. Existing three-wire effort test now takes parser-admitted selections and asserts exact Responses/Chat/Messages bodies and conditional headers. Debug/release requests carry `exact-effort` from discovery. |
| R4 / CONN04 — familiar flow and zero-model startup | Existing accounts owner supplies search/categories/effective source, direct masked Key with default label, later Rename and existing approved OAuth methods. Custom forms add name/literal base/Key-or-anonymous, asynchronous probe, explicit API, single-use Save ACK and filtered models. Revision-scoped jobs inside existing auth controls join retired work; no secret enters composer/history/Debug. Actual debug/release PTYs prove Home/history/commands without models, pre-effect submit refusal, cancel and pre-Save zero rows/POSTs, duplicate pending Enter, durable ACK without selection, explicit model/effort, fake generation and restart. Existing real auth/account fixtures retain activation/Rename/Delete/cancel/terminal cleanup assertions. |
| R5 / CONN05 — captured publication and read-only consumers | ProviderWork owns one custom/probe slot; provider/revision fences and credential-transaction invalidation reject stale results. Real held GETs prove old key, form and Location cancellation/join; repeated refresh coalesces. Runtime proof holds the old immutable generation and MCP lease: active publication refuses, idle publication changes only the requested provider entry with the same generation ID and identical MCP owner/view. Existing held prepared-request test retains all tools/approval/history assertions and now refuses custom mutation before effects. Mounted/parked/Home filtered consumers all refresh without committing selection. CLI uses the bounded existing read-only SQLite path: Ready/empty complete, Failed/Missing/mismatched revision incomplete with safe last-good IDs. Actual `oc models` runs while TUI owns the writer, without custom GET, preferences or session effects. |

### Direct scenarios and ownership

- `storage_provider_connections/tests.rs` and `composition/custom_connections/tests.rs`
  prove atomic Key/anonymous storage, scope separation, literal endpoint admission,
  raw config collision/filter precedence, revision fencing and attach-before-resolve.
  Saved-cache reads do not parse even an invalid unused credential material fixture.
- `application/provider_connections/tests.rs` covers single-use Save, Retry with a
  new receipt, harmless expired Cancel, held probe abort/join, invalid replacement,
  failed worker cleanup, anonymous typed metadata and held saved-refresh retirement
  before Location ACK. The destination resolves the exact stored key; valid empty
  leaves a retired explicit selection unavailable rather than selecting a fallback.
- Existing `storage/session_move_tests.rs` destination preparation now includes a
  saved custom Messages binding and scoped key. No LAN request is issued there.
- `runtime/tests.rs::conn05_custom_publication_keeps_captured_generation_policy_and_mcp_lease`
  compares the full config to the prior clone plus one entry, preserves the old Arc,
  refuses active publication and proves no MCP owner/request-view replacement.
- Existing `application/live_switch_tests.rs::tool12_busy_owner_commit_reloads_next_request_without_another_prompt`
  retains its captured request/tool/approval/exact effort checks; Probe/Save/Refresh
  additionally refuse while the real first request is held, leaving zero custom rows.
- `oc/src/tui_cmd/connect_controls/tests.rs` holds late probe/Save acknowledgements;
  a retired form cannot repaint or save another form. The routing test updates actual
  mounted, parked and Home provider-qualified pickers while preserving routes/drafts.
- The existing `pty_t39` target hosts two custom scenarios, with no
  `OC_TEST_ALLOW_LOOPBACK`: zero-provider connect→explicit request→restart and
  failed catalog→held Retry→key revision2→valid empty. Their fake HTTP counters,
  SQLite rows and actual CLI invocations are the effect proof, not screen text alone.
  Ready reads/restart keep the discovery count at exactly1 in the first scenario.
  Ancillary title requests are counted separately from main generation requests.

## Required current-code checks

All Cargo commands ran sequentially with normal stacks and
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`. Debug uses
`CARGO_BUILD_JOBS=3`, release2, `RUST_TEST_THREADS=2`. Rust/Cargo1.93.0, dedicated
non-root UID1003. Logs are gitignored `.local/evidence/t59-final-*.log`.

| Command | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS after the final fixture edit. |
| `cargo test --locked -p oc-adapters custom_discovery::tests::` | 5 passed / 0 failed. |
| `cargo test --locked -p oc-adapters storage::provider_connections::tests::` | 3 passed / 0 failed. |
| `cargo test --locked -p oc-adapters application::provider_connections::tests::` | 7 passed / 0 failed. |
| `cargo test --locked -p oc-adapters application::provider_catalog::tests::` | 2 passed / 0 failed. |
| `cargo test --locked -p oc-tui accounts` | 8 passed / 0 failed. |
| `cargo test --locked -p oc --test pty_t39` | **60 passed / 0 failed / 0 ignored**;147.32s. |
| `cargo test --workspace --locked` | **1938 passed / 0 failed / 12 existing opt-in ignored**,46 result records. Includes adapters lib746/0/1ignored, TUI540/0, oc binary119/0, current auth and full PTY/configured/runtime/Code Mode regressions. |
| `cargo build --locked`; `target/debug/oc --help` | Both exit0. |
| `cargo test --release --locked -p oc --test pty_t39 conn04_` | **2 passed / 0 failed**,58 filtered; actual rebuilt `target/release/oc`, unchanged optimized profile. |
| `cargo test --release --locked -p oc --test pty_t39 accounts::` | 3 passed / 0 failed,57 filtered. |
| `cargo test --release --locked -p oc --test auth_tui` | 1 passed / 0 failed. |
| `target/release/oc --help` | Exit0. |
| `python3 -B -m unittest discover -s scripts -p 'test_progress.py'` | 15 passed. |
| `python3 -B -m unittest discover -s scripts -p 'test_check_docs.py'` | 14 passed. |
| `python3 scripts/check_docs.py`; `python3 scripts/progress.py check`; `git diff --check` | PASS; documentation/journal structure is separate from product acceptance. |

Additional targeted current-code checks: CONN05 adapter cases7/0, existing
`tool19_admitted_generation_recovery`1/0, held-request `tool12_busy_owner_commit`1/0,
mounted/parked/Home provider-view binary case1/0. The final workspace includes all
current versions. Existing test ignores, caps, deadlines and baselines were not
disabled or raised. Source was reviewed and committed before this report.

## Findings resolved inside the frozen contract

- OpenAI `has_more: true` was wrongly accepted as complete. A failing parser test
  reproduced it; final code refuses unsupported partial lists, validates terminal
  Anthropic cursors too, and publishes only a complete snapshot. Custom deadlines
  now use `Instant`; fake timed pages/auth/retry cases fail at30000ms and succeed at29999ms.
- Ready custom catalog reads used to trigger a GET/ProviderChanged feedback loop.
  Automatic work now starts only for Pending; explicit Retry owns subsequent GETs.
  Binary counters prove the repaired behavior through Save/picker/request/restart.
- Invalid replacement and failed-join paths could retain an old probe/ACK. They now
  retire input, join work and fail the old ACK before replacement validation or a
  cleanup error. Barrier tests prove zero durable effects and peer closure.
- The final workspace exposed a legacy auth fixture still expecting mandatory Label.
  It now asserts direct masked Key→default account ACK, then explicitly Renames;
  all original lifecycle/secret/selection assertions remain. Debug and release pass.

## Scope, delivery and closure

Four existing crates, one Db/credential/discovery work owner; no new dependency,
wire, JS/Node/Bun/plugin host, arbitrary-provider OAuth, second store/framework,
config-file writer, host scan or paid detection/campaign. Literal local/LAN authority
is tagged and captured, not a test flag or broad webfetch allowance. Existing DNS,
peer, prefix/origin, redirect/proxy, metadata/link-local, permissions, immutable raw
history/DCP, retry, captured request and no-unknown-effect-replay guards remain.

All frozen outcomes are resolved with current evidence and the reviewed diff stays
inside the envelope. No known blocker was created by T59. Documentation/journal
closeout runs `check_docs.py`, `progress.py check`, Python progress/docs tests and
`git diff --check`; these validate records, not substitute for runtime evidence.

Remote delivery remains **BLOCKED_REMOTE** under T44 checkpoint147: dedicated-user
SSH publickey rejection and noninteractive HTTPS missing Username. No unchanged
push retry, credential search, foreign/root credentials, origin change or force push.
Source is preserved in the local commits above; report and normal finish form a
subsequent closeout commit. No fresh real-provider login/generation or paired donor
capture was performed. T44/VIS45/VIS46, T45/T57, T61 and A01–A13 keep their independent
requirements. T59 is complete; no further substantive T59 work remains.
