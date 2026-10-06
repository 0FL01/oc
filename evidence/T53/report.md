# T53 — Go/custom native integration: offline verified, live BLOCKED

Date: 2026-10-06. Reviewed implementation HEAD: `42b49e180`.
Overall: **BLOCKED, not complete**. R1–R5/GO01–GO05 are verified offline;
R6/GO06 offline gates pass, but required real Go qualification is **NOT_RUN**.
`progress.py finish` has deliberately not been run. T44 stays PAUSED; T57 scope,
existing OpenProxy live evidence and other task baselines are not changed.

## Frozen outcome matrix

| Outcome | Result | Direct owning evidence |
| --- | --- | --- |
| R1 / GO01 | VERIFIED offline | [Credentials](credentials.md), [account owner](accounts.md), [connect UI](connect-ui.md): existing Db additive Key/OAuth account owner, restrictive DB/WAL/SHM, transactional add/activate/rename/remove/rollback/reopen; redacted DTO/errors/input; explicit None versus missing Key/unsupported OAuth; endpoint admission before secrets; fixed Go and scoped custom precedence, no foreign credential inheritance. |
| R2 / GO02 | VERIFIED offline | [Public catalog](public-catalog.md), [config normalization](config-normalization.md): finite package mapping, provider/model/variant/API-ID/overlay precedence and exact source trust; bounded public GET/parse, persisted source-qualified last-good, TTL/single-flight/cancellation/retirement, no-key browsing; shared app/CLI/native lookup with live-WAL read-only no-mutation proof; OpenProxy discovery remains separate. |
| R3 / GO03 | VERIFIED offline | [Chat](chat-wire.md), [Messages](messages-wire.md), [wire settings](config-normalization.md), [request context](request-context.md), [timeout](transport-timeout.md), [chronology](chronology.md), [runtime retries](retry-wires.md): each native wire text/tools/reasoning/usage/terminal/error/cancel, per-wire typed options and media refusals; immutable all-lane Go identity/cache; numeric deadlines; chronological native/fallback system and effort/default/reset; one adapter attempt and counted durable runtime retry/partial continuation without partial-tool execution or effect replay. |
| R4 / GO04 | VERIFIED offline | [Durable bindings](durable-binding.md), [qualified selection](qualified-selection.md), [chronology](chronology.md): full provider/API/protocol/deployment/auth-scope receipts through raw/HOT SQL, DCP, checkpoints, fork/reopen and next requests; absent protocol is legacy Responses but missing authority is never invented; alien encrypted/signed/redacted state withheld, ordinary complete pairs and immutable raw history retained; qualified fork/child/profile/command captures use their own admitted authority. |
| R5 / GO05 | VERIFIED offline | [Configless](configless.md), [accounts](accounts.md), [connect UI](connect-ui.md), [qualified selection](qualified-selection.md): no fabricated model, usable local history/connect, acknowledged provider chooser/accounts/masked entry/filtered picker, explicit full ModelRef with ID collisions and slash IDs, drafts/variants/tabs/agents/Home/session/fork/restart; held prepared requests retain old bindings and next requests adopt the committed target; unready headless/submission refuse before acceptance/effects; unavailable reload retains exact choice while mandatory policy/storage/recovery remains fatal. |
| R6 / GO06 | BLOCKED; offline VERIFIED, Go live NOT_RUN | Current commands below; [sanitized live ledger](live-campaign.json). Missing authorized Go Console key prevents required actual text/tool representatives for all three protocols and endpoint-conflict probes. Public source reads and fake requests are not live generation evidence. |

### Actual-binary R5 boundary (not a disguised Go live test)

`pty_t39::accounts::go05_configless_go_then_explicit_custom_generation_cancel_and_reopen_share_owner`
starts truly configless with source-qualified fake public metadata. It exercises
cancelled/masked input, Go account ACK and explicit Go selection without a request.
Then an **explicit fixture config/reload and explicit custom connection/selection**
are used for one fake Responses completion and one held/cancelled request, followed
by actual-binary restart/history/accounts. The unavailable Go choice remains saved;
there is no hidden fallback, key sharing, Go URL override, or paid Go generation.
This verifies the frozen R5 primary offline flow and common native owners; it does
**not** verify that a Console key works against the actual fixed Go authority.
Other PTYs cover account activation/rename/confirmed removal, dismissal without
selection, Go-only picker from a different connection and exact persisted choices.

## Current final gates

Executed after the final implementation edit at `42b49e180`, with
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`, `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=2`, normal default thread stacks and no parallel Cargo runs:

| Command | Result |
| --- | --- |
| `cargo test --locked --workspace` | Exit 0; 42 result records, **1648 passed / 0 failed / 10 ignored**. Includes actual-binary headless/PTY, core/TUI, native runtime, child, retry, DCP/fork/recovery, discovery and storage targets. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit 0. |
| `cargo fmt --all -- --check` | Exit 0. |
| `cargo build --locked` | Exit 0; native `target/debug/oc`. |
| `target/debug/oc --help` | Exit 0. |
| `git diff --check` | Exit 0. |

Raw final local gate output:
`/home/opencode/.local/share/opencode/tool-output/tool_10f95f35c001CHBm0aGHfsjQdJ`.
The ten existing opt-in live/internal ignores are unchanged, not bypassed and not
counted as live PASS. No dependency, baseline, failing-test disable, timeout increase
or stack-size override was used to obtain green gates. Each slice receipt records
its actual failures, diagnosis and narrower/broad rechecks rather than hiding them.

## Implementation and review anchors

All implementation slices were reviewed and committed/pushed on
`agent/oc-rust-port` to the verified `origin` (`git@github.com:0FL01/oc.git`):

- `1748ba73b`, `d66db2eaa`, `9b9482a7d`, `7e5ca8231`: protocol seam/native Chat and terminal/replay corrections.
- `98298dc92`: native Messages and static Key header schemes.
- `cfbc50a08`, `3ea7c9377`, `ec9dd3251`: SQLite accounts, application auth resolution and scoped shared endpoint guards.
- `593e651b6`, `29ac9af72`, `30b0d1e7c`: canonical normalization, typed wire settings and immutable effective model/variant bindings.
- `9901d36ad`, `765012c97`, `4bcdc5713`: public source/cache, app consumers and CLI/native lookup.
- `f4afe7b0a`, `d88261a0d`, `d06ea615a`: Go request/session-fork cache identity, deadlines and durable chronology.
- `5094f5e50`, `b0c5fd031`: durable authority projection and native/auxiliary/variant checkpoint fences.
- `69e2e01f0`, `d4abea7cc`, `d3c3a26ae`: configless startup, acknowledged account commands and masked TUI.
- `50615d743`, `cee7dc5bc`, `b4b9266d5`, `2a36a2202`: independent provider views/public refresh, full qualified executable selection and fork/child authority.
- `57f7c459a`, `6a3a469b9`, `42b49e180`: explicit connection chooser/actual binary flow, Chat/Messages runtime retries and exact unavailable-choice reload.

Concurrent T57 plan-only commit `6e7fa2ed4` was preserved independently, not counted
as T53 implementation. User-owned untracked `.opencode/` was neither read nor staged.
Production owners and nearest tests are mapped in `docs/CODE_MAP.md`; component
receipts above document paths, contracts, failure experiments and commands.

## Proven external live blocker

Presence-only preflight was repeated after implementation. It printed no values:

```text
process_OPENCODE_API_KEY_present=False
approved_file_exists=True
approved_file_OPENCODE_API_KEY_name_present=False
foreign_OpenProxy_test_input_names_present=True
```

The approved gitignored `.local/live.env` provides only the OpenProxy test input
names `LUDKA2_API_URL`, `LUDKA2_API_KEY`, `OC_TEST_MODEL`, not a Go key. Native fixture
accounts contain synthetic secrets, not real Go credentials. Public Go docs require
a Console subscription key. The checked alternatives cannot satisfy GO06:

- Reusing OpenProxy material on Go would violate credential/authority isolation.
- Anonymous/free generation would neither qualify Console auth nor authorize the
  required paid representatives; no such probe was sent.
- Authoring-agent, browser, home or donor credentials are not approved test inputs;
  they were not searched or copied.
- Changing the Go base URL or adding a private-network/redirect bypass to reuse a
  fake/envelope endpoint would invalidate fixed Go admission; it was not done.
- Current public catalog fetching succeeds without a key, but cannot prove paid
  authorization or actual generation route compatibility.

Sanitized campaign receipt: `t53-go-20261006`, **0 physical generation requests**,
0 title/summary/child/retry/probe requests and 0 MCP searches. No paid campaign or
unknown external side effect was started. The durable zero ledger is a preflight
receipt, **not** a claim that a dedicated fixed-authority live enforcement harness
has been qualified. Generic existing bounded-envelope tests remain green offline.

Public-only observation at `2026-10-06T04:15:56.116023+00:00`: models.dev returned
5,315,044 bytes and 33 Go rows, with all three implemented package aliases present.
`qwen3.8-max` and `qwen3.7-plus` currently declare Messages (`@ai-sdk/anthropic`),
not the historical default-Chat conflict in the RECON snapshot. Both actual routes
remain NOT_PROBED. Neither this metadata change nor docs is an endpoint success;
no ID allowlist, guessed route or automatic paid fallback was introduced.

## Smallest unlock / resume

An authorized real Go Console key supplied as `OPENCODE_API_KEY` in the approved
test inputs or an explicitly authorized native Go account is required. On resume,
reuse the existing campaign ID/ledger, establish fixed-authority pre-dial durable
accounting before any request, and count **all** main/auxiliary/retry/probe HTTP
dispatches against the same maximum 24 across restart. Smoke output is at most
2048 tokens; use only exact current catalog-selected representatives, no all-model
sweep or paid fallback. Qualify real text/tool representatives for Responses, Chat
and Messages and the dated Qwen conflict rows; record sanitized results/counters.
Then refresh affected final gates, update R6 honestly, and only then run task finish.
No other T53 implementation or offline gate is known to be blocked by this key.
