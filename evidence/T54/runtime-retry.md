# T54 runtime retry — frozen coupled atomic

## Coordinator qualification before delivery

The coordinator independently reviewed the scoped production/test diff, the native
fixture and the full workspace log at
`/home/opencode/.local/share/opencode/tool-output/tool_0f01e69e0001c8Ye4T94bqB80K`:
42 successful result summaries, **1321 passed / 0 failed / 10 unchanged opt-in
ignored**. Two read-only reviewers inspected durable retry/continuation and lane
accounting. Neither found a remaining concrete introduced violation; a proposed
title-accounting issue was withdrawn after distinguishing explicit manual title
regeneration from the automatic title's captured accepted root operation.

Independent coordinator commands, each exit 0 on unchanged compiled input:

- `cargo test -p oc-adapters --lib --locked ret01_`: **17 passed**.
- `cargo test -p oc-adapters --test runtime --locked ret01_`: **4 passed**.
- `cargo test -p oc-tui --lib --locked retry`: **3 passed**, including both new
  span/event ordering scenarios and one existing draft-rejection scenario.
- `cargo build --locked`, followed without further Cargo builds by
  `python3 -B evidence/T54/native_runtime.py target/debug/oc` and the release
  counterpart: **nine cases per ELF passed**. Actual main/child/compaction/title
  sockets were **18/2/4/8 (32)** for debug and **18/2/4/9 (33)** for release;
  one Bash append and one subagent intent per ELF, no effect replay. Kill/send title
  race is observed and counted, not normalized.
- Workspace fmt and strict all-target locked Clippy; Python four suites **47
  passed**; docs/progress/diff and advisory changed-code checks: all exit 0.
  No affected handwritten source exceeds 5000 physical lines.

Normal debug/release hashes below remained unchanged after the independent native
proofs and final checks. Association remains **base plus reviewed dirty source**
until the implementation commit. No Rust/helper edit followed qualification.
R1's `native_typed.py` is an immutable physical-attempt-phase fixture, not a current
runtime assertion that retryable main requests issue only one POST. Its backend
classifier checks remain in the current provider suite; current native counting
is `native_runtime.py`. No live requests or campaign resets occurred. RET01/common
Responses backend does not qualify T53's future wires or paused T44/VIS43 visuals.

Source base: `a7ddde9f8c69805aafafd3852f9b210f9f0932af`.
Donor: `2670273ff17da96f85c5826ced57aa1b368754fa`.
Freeze precedes RED. R1 remains one physical attempt, typed safe facts and genuine
prior-done length tools. No T44/VIS43 or T53/GO03 qualification is implied.

| Obligation | Required observation | Initial result |
|---|---|---|
| R2 allowance | Initial + at most ten retries per logical step, shared across pre-output retry and partial continuation; retries consume no rounds | NOT_RUN |
| R2 timing | 2/4/8/10 seconds then seven 10-second gaps, uniform jitter [0.8,1.2), ceil milliseconds, bounded rate/internal provider minimum; publish epoch due before cancellable wait | NOT_RUN |
| R2 eligibility | Typed observed exact override only; accepted read/incomplete post-output may continue despite false; terminal auth/quota/policy/invalid default; local failures and overflow never generic retry | NOT_RUN |
| R2 continuation | Same span before output; partial failed span settles non-success, next span new ID with durable prior partial history; incomplete function JSON never admitted | NOT_RUN |
| R2 effects | Physical stream closes before tool admission; committed/unknown effects never replay across retries or later steps | NOT_RUN |
| R3 durable facts | Span-owned attempt/epoch due/safe error in existing TurnLog JSON, storage event and bounded query; semantic same-span start clears only its retry, new span clears only latest unfinished span | NOT_RUN |
| R3 lifecycle | Completed failed span keeps retry across later success/cancel; recovery non-success and no cached due dispatch; scoped focused/parked/stale/reopen/headless presentation | NOT_RUN |
| R4 lanes/counts | Actual dispatch accounting at owning operation includes root, child, compaction and title; root/child own policy allowance, auxiliary shared allowance, title one physical call | NOT_RUN |
| R4 invariants | Fixed admitted generation/model/auth/headers; DCP/media/fork/history/caps/cancel/join/checkpoint boundaries and existing quality gates retained | NOT_RUN |
| Binary evidence | Rebuilt normal debug/release direct headless + PTY retry/park/reopen/restart ordering and four LLM lanes, actual socket/effect counts | NOT_RUN |

## Owner seam and durable-fact necessity

`runtime/turn.rs` owns the logical step and tool effects; provider owns exactly one
physical exchange. `runtime_compaction.rs` shares the narrow pure policy. Existing
`TurnLog` currently has no assistant-span identity, settlement, or retry due fact;
these are irreducible for pre-output same-ID versus partial new-ID and historical
failed retry. Add a bounded span list to the existing `turns.result` JSON journal,
with legacy absent-list compatibility, and use existing events/storage projection.
No new SQL column/table, schema version, store, trait or framework is necessary.
The dispatch fact uses existing `events` with owning operation and lane. An
operation-scoped count must include child and late title events across sessions.
Pre-index SQLite `EXPLAIN QUERY PLAN` on the existing events shape returns
`SCAN events`; add one partial expression index on the existing event payload's
operation for this new event kind, before qualifying bounded projections. This is
an existing-owner index, not a new table/column/store or schema-version change.
The existing NativeCompaction capability also receives the physical dispatch
callback: a strategy returning None or a purely local checkpoint must not count
an HTTP request. Automatic title retains its database-free task and requests
dispatch accounting through its existing owner channel before send.
Partial text is an explicitly non-success span and wire continuation input, never
a fabricated completed assistant message or recovered function call.

## Source review

Read complete T54 195-line contract and pinned runner retry/step/llm, updater
213–255/402–409 and projector 298–313, plus compaction native/shared generated
allowance and title single stream. Native response-close-before-tool remains an
intentional product constraint. RET01 backend facts are separate from visual parity.

## Execution results

The frozen initial results above are historical. Current implementation and
qualification below are against the source base plus the reviewed, uncommitted
working-tree changes. Parent independent review/delivery remains the next gate;
task/progress/spec/acceptance execution statuses were not changed.

### Per-obligation result

| Obligation | Current evidence | Result |
|---|---|---|
| R2 allowance | `runtime/retry/tests.rs` proves exactly ten shared decisions followed by exhaustion, including alternating pre-output/internal and post-output/read-incomplete failures. Runtime mixed fixture and both ELFs issue three main POSTs in one logical round. | PASS |
| R2 timing | Explicit epoch/sample inputs prove the ten nominal gaps `[2000,4000,8000,10000,10000,10000,10000,10000,10000,10000]`, jitter bounds, ceil and 900000 ms maximum provider minimum. Actual waits exercise persisted-before-wait publication, cancellation and PTY due/restart behavior. | PASS |
| R2 eligibility | Pure typed matrix covers quota/auth/policy/invalid/payload defaults and observed override, context overflow exclusion, local parser/config/private-host/byte/dispatch/cancel refusal, and accepted read continuation despite false. R1 provider suite retains exact header/date/status tests; both ELFs exercise observed 401 override, terminal 429 quota and HTTP-200 partial EOF with false and a discarded delay header. | PASS |
| R2 continuation | Pre-output requests have identical input and the same span; failed partial span has safe error, `finish=error`, completed epoch and retained retry. Next request reloads durable history and has a new span ID. Canonical tool/result pairs survive; incomplete/terminal-only lookalike calls do not dispatch. Continuation budget refusal settles both spans after one POST. | PASS |
| R2 effects | Runtime committed Bash effect fixture and both direct ELFs execute exactly one intent/one append across three main POSTs. Continuation contains the original completed call/result pair. Existing unknown-effect/recovery, permissions and response-close-before-admission regressions pass. | PASS |
| R3 durable facts | Existing `TurnLog` JSON holds bounded span ID/step/status/start/completion/error/finish/retry; `checkpoint_retry` atomically stores journal plus span-addressed event before broadcast/wait. Legacy absent-list defaults, bounded query and operation-count index are verified. | PASS |
| R3 lifecycle | Semantic start clears current retry/error/finish; failed completed history remains. Owner tests reject foreign turn/span, lower attempts, delayed same-attempt events after semantic start, and older snapshots. Both ELFs show retry versus failure, parked completion/reopen and recovery without dispatch after cached due. | PASS |
| R4 lanes/counts | Root and child have independent logical-step allowances. Native/generated compaction and one correction share the auxiliary policy; title is one physical call, including observed-true 503. Dispatch records agree with actual sockets in both ELFs; registered native-compaction HTTP fixture issues two sockets/counts. Family/index test keeps an old child's count on its original operation after a newer root turn starts. | PASS |
| R4 invariants | Full provider/runtime/media/DCP/fork/compaction/permissions/cancel/checkpoint/resource/workspace regressions pass. Native server verifies fixed model/path/synthetic credential/header on every lane. Existing caps, queue capacities, test deadlines, ignored tests and campaign budgets were not raised. | PASS |
| Binary evidence | Normal debug/release builds, help/smoke and nine direct headless/PTY cases each pass. Actual counts below; no Cargo invocation between final builds and ELF checks. | PASS |

The timing freeze's prose is resolved by the explicit ten-gap source sequence
above: seven **total** 10-second gaps, not seven additional gaps after an already
counted fourth gap. Exhaustion is tested deterministically at the policy owner;
the direct ELF fixtures exercise a few real waits rather than a full real-time
exhaustion campaign. An earlier runtime incomplete-call run also observed all
eleven physical requests before its obsolete one-request assertion failed.

### Actual native counts

`python3 evidence/T54/native_runtime.py target/debug/oc` and the corresponding
`target/release/oc` command both exited 0. Each case uses its own temporary
HOME/XDG/project and loopback HTTP server, with synthetic credentials only.
Counts are actual accepted POSTs, checked against persisted dispatch events.

| Case | Main | Child | Compaction | Title | Outcome |
|---|---:|---:|---:|---:|---|
| Mixed throttle → partial EOF → success | 3 | 0 | 0 | 1 | Exit 0; headless retry attempts 2/3, same pre-output input, two spans |
| Observed auth override | 2 | 0 | 0 | 1 | Exit 0; observed true permits retry |
| Quota | 1 | 0 | 0 | 1 | Exit 1; no main retry |
| Committed Bash effect then partial continuation | 3 | 0 | 0 | 1 | Exit 0; one intent, file exactly `effect`, no replay |
| Root/child | 2 | 2 | 0 | 1 | Exit 0; child 503 then success, one subagent intent |
| Generated summary/correction | 2 | 0 | 4 | 1 | Both explicit turns exit 0; two auxiliary retries, rejected candidate absent from correction input |
| Title failure | 1 | 0 | 0 | 1 | Main exit 0; title 503/observed true still one call |
| PTY retry → park → complete → reopen | 3 | 0 | 0 | 1 | Retry visible; background completion visible on original tab; historical failed retry retained |
| PTY kill/restart | 1 | 0 | 0 | debug 0 / release 1 | Recovery unknown/settled; zero additional POSTs after cached due |
| **Total** | **18** | **2** | **4** | **debug 8 / release 9** | **debug 32 / release 33 physical POSTs** |

The restart title difference is the observed kill/send race, not an assumed
request. Each ELF records two tool intents total: one Bash effect and one
subagent. No provider body/auth/environment/private-URL canary is present in
captured output or stored span/retry diagnostics. Native-capability auxiliary
qualification is a registered test capability using the real Responses adapter
(two actual POSTs), not a claim of a newly shipped provider-native protocol.

### Current commands and artifacts

All Cargo commands ran serially with `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`, `CARGO_NET_OFFLINE=true`, approved
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
`--locked`, and command timeout 900000 ms.

- `cargo fmt --all -- --check` — exit 0.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — exit 0.
- `cargo test --workspace --locked --no-fail-fast` — exit 0: **1321 passed,
  10 pre-existing ignored**, 39 test targets plus three empty doc-test targets.
  Adapter unit 366; runtime integration 100; TUI unit 424; native PTY T39 48.
- Targeted provider suite — 26 passed, including all R1 classifier/header,
  length prior-done, caps/cancel and coalesced-stream fairness cases.
- Targeted runtime RET01 — four passed, including mixed continuation,
  effect preservation, backoff cancel and continuation budget refusal.
- `python3 -m unittest discover -s scripts -p 'test_*.py'` — 47 passed,
  four script suites, exit 0.
- `python3 scripts/check_docs.py`, `python3 scripts/progress.py check`
  (read-only), `python3 scripts/code_size.py --base
  a7ddde9f8c69805aafafd3852f9b210f9f0932af --changed`, `git diff --check`
  — all exit 0.
- `cargo build --locked` and `cargo build --release --locked` — normal
  builds, both exit 0. Direct debug/release `--help` and `--smoke` — all exit 0.
- Direct `native_runtime.py` debug and release commands — both exit 0,
  nine cases per ELF, after the final normal builds with no intervening Cargo.

Current normal ELF SHA256:

```text
debug   be0ef6d7a14dbffcdc392ab69cbe07ba27c32cbcfaf6f8f159f42da3495469f3
release c23a8b524661ced3bbecf04cadd2f7608383ef0f80a4eaebebc943483c9c2504
```

Association: source HEAD `a7ddde9f8c69805aafafd3852f9b210f9f0932af` plus
current reviewed implementation diff and new private retry/test modules.
Documentation-only final reporting follows these builds. Earlier intermediate
hashes/qualification runs are superseded. No new source hash manifest is used.

### RED/GREEN and diagnosed failed approaches

1. Initial mixed runtime RED: terminal Failed instead of Completed before
   implementation. GREEN proves three actual sockets and one logical round.
2. Borrowing `&Db` into the automatic-title task failed the required static
   lifetime. Kept the task database-free; its existing owner channel now requests
   dispatch accounting with acknowledgement before send.
3. Compile repairs covered additive exhaustive CoreEvent matches, callback/
   fixture signatures, SQLite integer conversion and two ordinary Clippy
   suggestions. No lint suppression, dependency or toolchain change.
4. Superseded expectations initially failed: adapter-one-attempt assumptions
   applied to the new runtime owner, journal-private partial reasoning absence,
   accepted-event order without dispatch facts, and envelope/title event counts.
   Replaced only expectations changed by the frozen contract. Incomplete-call
   non-effect/security checks passed even during the eleven-request old-counter
   failure; canonical admission and timeout/cap assertions remain enforced.
5. Valid compaction fixtures lacked the donor's required section heading,
   causing real correction requests; fixtures now explicitly distinguish valid
   `## Objective` summaries from two invalid correction candidates. Native lane
   detection originally used a different summary prompt; corrected to the
   actual fixed prompt after a material isolated request/count experiment.
6. Closed-port tab adoption waited for exhaustion and blocked the first full
   run. Its test now cancels on the real RetryScheduled event and handles
   interruption; it still tests adoption, without increasing its deadlines.
7. Final review added durable budget-refusal settlement after partial history
   made continuation exceed the existing admitted input budget: one socket,
   both spans settled, no second send.
8. TUI RED showed a delayed same-ID RetryScheduled could revive a retry after
   semantic start. A bounded turn/span/attempt watermark now rejects that event,
   older reschedules and stale snapshots; real newer attempts still apply.
9. One combined fmt/clippy/tests/build command hit the unchanged 900000 ms
   command limit late in TUI tests. A separate workspace run then exposed an
   actual existing high-rate PTY queue-lag assertion (15 versus 0). Isolated
   PTY passed, but investigation reproduced the owner problem: a coalesced
   Content-Length body could synchronously emit thousands of events before the
   scoped consumer ran. EOF-delimited experiments did not reproduce it;
   the actual 4096-frame Content-Length RED reported Lagged(17). The provider
   now feeds the same parser line-wise and yields after an event boundary,
   checking cancellation between lines. GREEN receives all 4096 deltas / 20480
   bytes / one POST. Queue sizes, frame/byte/event caps and deadlines are unchanged.
   Strict Clippy and the complete workspace were rerun successfully afterward.
10. The initial release PTY checker drained with a 20 ms blocking select while
    16 ms animations continually arrived, so it observed attempt 3 after the
    attempt-2 phase had ended. The checker drains only bytes ready now. Both
    normal ELFs were then requalified; restart now waits beyond the stored due
    time, strengthening the earlier startup-only check without larger deadlines.

### Paths, resources and handoff

Owners changed: adapters `runtime/turn.rs`, new private `runtime/retry.rs` and
tests, provider counted-send/fairness boundary, existing compaction/title/storage/
TurnLog owners; core retry/span DTO/event; existing headless/TUI routing,
projection and deadline scheduler. Nearest fixtures and actual seams are listed
in `docs/CODE_MAP.md`. The evidence checker is `evidence/T54/native_runtime.py`.

Final resource sample: UID 1003; MemAvailable 8269 MiB; disk available
196664 MiB; allocated `evidence/T54` approximately 1 MiB. Private captured logs
remain below 16 MiB each; owned fixture processes/servers are joined and temporary
homes removed. Code-size report: 29 changed Rust files, 48031 physical lines,
1858751 UTF-8 bytes; largest changed handwritten owner 4140 lines, turn 2747,
policy 118 with tests 214. No changed handwritten file exceeds the soft 5000-line
guide. No paid/live calls, user-config/auth/environment reads or campaign reset.

Backend R2/R3/R4 observations assigned to this atomic are qualified above;
independent parent review/delivery and task-final report/status update remain
parent-owned. T53/GO03 future protocol consumption and T44/VIS43 paired external
visual qualification remain separately pending; this report does not claim full
goal or full delivered-wire/TUI parity. All sole mutation/Cargo/native-fixture
ownership is released to the parent at handoff.
