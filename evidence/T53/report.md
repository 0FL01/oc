# T53 — Go/custom native integration: COMPLETE

Date: 2026-10-06. Reviewed implementation/live HEAD: `bee775508`.
Overall: **COMPLETE**. Every frozen R1–R6/GO01–GO06 outcome is verified: owning
offline/actual-binary tests plus real fixed-authority Go text/tool qualification.
Campaign `t53-go-20261006` used 13/24 requests including failed diagnostics, without
reset/fallback or unknown effects. T44 stays PAUSED; T57 scope, existing OpenProxy
live evidence and other task baselines are not changed. This is T53 completion,
not an overall product READY or paired visual-parity claim.

## Frozen outcome matrix

| Outcome | Result | Direct owning evidence |
| --- | --- | --- |
| R1 / GO01 | VERIFIED offline | [Credentials](credentials.md), [account owner](accounts.md), [connect UI](connect-ui.md): existing Db additive Key/OAuth account owner, restrictive DB/WAL/SHM, transactional add/activate/rename/remove/rollback/reopen; redacted DTO/errors/input; explicit None versus missing Key/unsupported OAuth; endpoint admission before secrets; fixed Go and scoped custom precedence, no foreign credential inheritance. |
| R2 / GO02 | VERIFIED offline | [Public catalog](public-catalog.md), [config normalization](config-normalization.md): finite package mapping, provider/model/variant/API-ID/overlay precedence and exact source trust; bounded public GET/parse, persisted source-qualified last-good, TTL/single-flight/cancellation/retirement, no-key browsing; shared app/CLI/native lookup with live-WAL read-only no-mutation proof; OpenProxy discovery remains separate. |
| R3 / GO03 | VERIFIED offline | [Chat](chat-wire.md), [Messages](messages-wire.md), [wire settings](config-normalization.md), [request context](request-context.md), [timeout](transport-timeout.md), [chronology](chronology.md), [runtime retries](retry-wires.md): each native wire text/tools/reasoning/usage/terminal/error/cancel, per-wire typed options and media refusals; immutable all-lane Go identity/cache; numeric deadlines; chronological native/fallback system and effort/default/reset; one adapter attempt and counted durable runtime retry/partial continuation without partial-tool execution or effect replay. |
| R4 / GO04 | VERIFIED offline | [Durable bindings](durable-binding.md), [qualified selection](qualified-selection.md), [chronology](chronology.md): full provider/API/protocol/deployment/auth-scope receipts through raw/HOT SQL, DCP, checkpoints, fork/reopen and next requests; absent protocol is legacy Responses but missing authority is never invented; alien encrypted/signed/redacted state withheld, ordinary complete pairs and immutable raw history retained; qualified fork/child/profile/command captures use their own admitted authority. |
| R5 / GO05 | VERIFIED offline | [Configless](configless.md), [accounts](accounts.md), [connect UI](connect-ui.md), [qualified selection](qualified-selection.md): no fabricated model, usable local history/connect, acknowledged provider chooser/accounts/masked entry/filtered picker, explicit full ModelRef with ID collisions and slash IDs, drafts/variants/tabs/agents/Home/session/fork/restart; held prepared requests retain old bindings and next requests adopt the committed target; unready headless/submission refuse before acceptance/effects; unavailable reload retains exact choice while mandatory policy/storage/recovery remains fatal. |
| R6 / GO06 | VERIFIED offline + real Go | Current final commands below; [sanitized durable live ledger](live-campaign.json) and [real fixed-authority receipt](go-live.md): actual native read/function-result/final on Responses, Chat, Messages and both dated Qwen targets; 13/24 requests, smoke <=2048. Authorized OC_API_KEY mapped only in the explicit test process to the existing Go resolver. |

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

Executed after the Go Messages auth correction and live harness fixes committed as
`bee775508`, with
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`, `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=2`, normal default thread stacks and no parallel Cargo runs:

| Command | Result |
| --- | --- |
| `cargo test --locked --workspace` | Exit 0; 42 result records, **1651 passed / 0 failed / 11 ignored**. Includes actual-binary headless/PTY, core/TUI, native runtime, child, retry, DCP/fork/recovery, discovery and storage targets. |
| `python3 -B scripts/t53_go_live.py --run` | Exit 0 on final live run; explicit opt-in **1 passed / 0 failed**, all five exact catalog-selected models below PASS. Every earlier failed attempt remains charged in the same ledger. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit 0. |
| `cargo fmt --all -- --check` | Exit 0. |
| `cargo build --locked` | Exit 0; native `target/debug/oc`. |
| `target/debug/oc --help` | Exit 0. |
| `git diff --check` | Exit 0. |

Raw final local gate output:
`/home/opencode/.local/share/opencode/tool-output/tool_110080bec001VkIPufJKhg8L7n`.
The ten existing opt-in live/internal ignores are unchanged. The eleventh is the
new explicit Go opt-in, actually run and qualified separately above; being ignored
offline is not its live proof. After a comment/test-expression readability-only
review, all three `go06_` offline tests and strict workspace all-target Clippy/fmt
were rechecked, and the native binary rebuilt/help checked, all exit 0. No dependency,
baseline, failing-test disable, timeout increase or stack-size override was used.
Each slice receipt records failures, diagnosis and rechecks rather than hiding them.

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
- `43f836eb6`: fixed-authority cfg(test)-only pre-DNS durable Go campaign guard and explicit authorized-input wrapper.
- `bee775508`: source/live-correct native Go Messages x-api-key, unchanged-body smoke fixture and all real protocol/tool/conflict qualification.

Concurrent T57 plan-only commit `6e7fa2ed4` was preserved independently, not counted
as T53 implementation. User-owned untracked `.opencode/` was neither read nor staged.
Production owners and nearest tests are mapped in `docs/CODE_MAP.md`; component
receipts above document paths, contracts, failure experiments and commands.

## Real fixed-authority Go results

Each row is the actual native runtime, one settled native Db/read-tool operation,
then its complete function-result/final request. The final text contained the fixture
token; no raw response/prose/key/header is published.

| Exact catalog ID | Declared protocol | Explicit variant | Native read count | Last-round input / summed output usage | Result |
| --- | --- | --- | --- | --- | --- |
| `muse-spark-1.2-contributor` | Responses | default (`null`) | 1 | 1329 / 348 | PASS |
| `longcat-2.5-preview-free` | Chat | default (`null`) | 1 | 756 / 98 | PASS |
| `qwen3.8-flash` | Messages | `none` | 1 | 863 / 36 | PASS |
| `qwen3.8-max` | Messages | `none` | 1 | 863 / 36 | PASS |
| `qwen3.7-plus` | Messages | `none` | 1 | 863 / 36 | PASS |

The first three were cheapest declared tool-capable current representatives of the
three protocols; the last two are explicitly required dated conflict targets, not
production ID routing. Both Qwen rows now declare Messages in models.dev, and actual
Messages read/final exchanges confirm that authority. No alternate-wire retry was
used. The public-only 33-row observation at `2026-10-06T04:15:56.116023+00:00` was a
source snapshot, not live proof; the table above supplies the missing actual proof.

Ledger: **13/24 physical requests**, 11 complete and 2 failed; main 6, ordinary
follow-up 3, dated probes 4. Every request's native token cap was 2048. Title,
summary/compaction, child, runtime retry and MCP counts were zero, as were unknown
effects. Native all-lane identity/retry/cancel guarantees are additionally qualified
by the owning fake tests, not inferred from uninvoked live lanes.

The pre-DNS guard uses locked/fsynced atomic accounting on the same ledger across
process restart and refuses request 25 or >2048/foreign authority before dial.
Reservations before failure remain charged. The hook exists only under cfg(test):
no production successful-step cap, transport bypass, proxy, alternate endpoint or
new retry owner. Passed models are skipped when resuming the same campaign; no
automatic all-model sweep, budget reset or paid fallback occurred.

## Material failures and corrections (historical, resolved)

The initial presence-only preflight printed no values:

```text
process_OPENCODE_API_KEY_present=False
approved_file_exists=True
approved_file_OPENCODE_API_KEY_name_present=False
foreign_OpenProxy_test_input_names_present=True
```

It checked `OPENCODE_API_KEY` and missed the owner-authorized Go input `OC_API_KEY`.
The owner corrected the name, nonempty presence was confirmed without printing,
and only the child test process mapped it to the existing Go resolver. The earlier
blocked checkpoint was an incomplete name check, not a revoked/missing-key finding.
Native fixture accounts remain synthetic. The invalid alternatives were not used:

- Reusing OpenProxy material on Go would violate credential/authority isolation.
- Anonymous/free generation would neither qualify Console auth nor authorize the
  required paid representatives; no such probe was sent.
- Authoring-agent, browser, home or donor credentials are not approved test inputs;
  they were not searched or copied.
- Changing the Go base URL or adding a private-network/redirect bypass to reuse a
  fake/envelope endpoint would invalidate fixed Go admission; it was not done.
- Public catalog fetching succeeds without a key, but cannot prove paid
  authorization or actual generation route compatibility.

Physical request 1 completed text without tool exposure: the smoke fixture scalar
permissions did not replace its inherited ordered rules. The read-only fixture
authority and a no-body-mutation exposure guard fixed this; one further local
refusal consumed no request. Physical request 2's temporary forced choice returned
typed HTTP 400; all forced-choice mutation was removed. Physical request 7's Go
Messages Bearer returned typed HTTP 401. Pinned server `go/v1/messages.ts:9` reads
x-api-key, whereas Responses and Chat read Bearer. Production now preserves the
native scheme after Go authoritative-header overlay, and custom static Messages
apiKey/authToken behavior is unchanged. Requests 8–13 then succeeded. All failures
and nonqualifying result rows remain in the ledger and [live receipt](go-live.md).
Earlier blanket Go-Bearer claims are superseded by this source/live correction.

## Closure and boundaries

All frozen outcomes are resolved with current primary evidence; no known blocker
or failed gate remains. Changed production paths are the contracted native
config/auth/catalog/wire/runtime/storage/core/TUI consumers; tests/evidence and the
test-only bounded harness stay inside the approved envelope. One Db/auth/cache
owner, immutable raw history, complete settled tool pairs, trust/DNS/peer/redirect
guards, cancellation/retry ownership and captured-request concurrency are retained.
Key material is absent from Git/logs/arguments; `.opencode/` is untouched. No Node/Bun
production host, dependencies, OAuth execution, auth CLI, guessed fallback, T44
resume or adjacent-task completion was introduced. T53 is complete; stop substantive
T53 work rather than expanding into T57/T44 or speculative cleanup.
