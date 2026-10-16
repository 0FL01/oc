# T45 R3 — frozen background child atomic

## Coordinator qualification — 2026-10-04

The application-owned background lifecycle and its two review repairs are
independently reviewed and verified. This qualifies the delivered subset, not
the separately required safe unfinished resumption, foreground conversion,
linked-child consumer/navigation or whole R3/T45.

- Reviewed atomic fresh-child admission with Location/job/launch facts, existing
  continuation, versioned migration10, bounded snapshot ownership and cancellation
  safety. The capacity race reproduced orphan admission before its repair; dropped
  shutdown/retirement waiters reproduced lost ownership. Permanent tests now
  refuse the losing admission without orphan rows and retain authoritative joins
  and MCP owners until actual completion. Sticky cleanup failures remain errors.
- Fresh independent checks: **3 child-owner +5 storage-owner +24 subagent tests
  passed**; workspace fmt, strict locked all-target Clippy and normal build passed.
  The recorded full **1513/0/10** gate applies to unchanged product source.
- After the last normal build, both retained normal ELFs passed seven background
  and two foreground cases each: **18 actual-native cases**. Combined receipts:
  **108 physical POSTs**, 24 physical effects and 20 original durable notices.
  Accepted child turns precede immutable running-result persistence. Idle reopens
  dispatch nothing; unknown effects are not replayed. Original running launch
  results remain immutable, and Fork/Revert do not transfer job authority.
- One release foreground cancellation run first failed only in fixture cleanup:
  the process exited naturally between the initial liveness observation and the
  interactive-send assertion. The shared owned-PTY stop helper now writes directly
  and tolerates a write error only after confirmed process exit. Original wait,
  join and expected-exit assertions are unchanged. A mock regression reproduced
  **RED1**, then **3 GREEN**, including unsuppressed live errors/nonzero exits.
  Both foreground ELF suites subsequently passed; earlier background receipts
  remain valid. No production code or cancellation deadline changed for this race.
- Both ELF hashes stayed unchanged across all final probes:
  debug `b83a3587d08b644bb3b6c56e98481e3e8ffcdbddb5e29a5ecce025d5f8001e85`;
  release `7c52f8665b06fe55469c03292a3b9ebfae3afacbb8ea69dba59ae2a95e3eabec`.
  All owned native groups, shell leaves, HTTP handlers and PTY readers joined
  before exact temporary cleanup. Python47/docs/progress/diff checks passed.

The compiled Rust association is base `c8efe735a1b35d4bf3052bc9ca9076109d650c88`
plus the reviewed implementation, not a clean-base artifact claim. The parent
fixture-only cleanup edit is later than the original helper-diff receipt; the
product source is unchanged. Literal peer-POST arrival before running and safe
unfinished native resumption remain **NOT_PROVEN** below, not quietly waived.
No actual HOME/config/authing credentials, paid API or exhausted T27 ledger was
accessed. T44 stays PAUSED and no product READY claim is made.

Base: `c8efe735a1b35d4bf3052bc9ca9076109d650c88`, branch
`agent/oc-rust-port`. Frozen before RED or production edits, 2026-10-04.
T45/R3/SUB01/SUB02 only; no whole-task/READY or T44 visual claim.

## Obligations and qualification

| Obligation | Required observable proof | Initial status |
|---|---|---|
| Genuine immediate launch | Child provider POST precedes running result; child effects and terminal occur while parent remains active | NOT_RUN |
| Fresh context / continuation | Own profile/model/policy/Location, no parent transcript; continuation retains own history; same-child busy rejected without hidden fork | NOT_RUN |
| Authority and effect safety | Actual Ask/Deny and independent parent/profile ceilings; depth/General/Explore, closed response, prepared mutation preimage, provider/MCP leases retained | NOT_RUN |
| Durable lifecycle | Immutable launch stays running; separately committed generation/terminal; exact typed outcomes independent of prose | NOT_RUN |
| Notice | One native source=subagent notice with child/agent/state/description/bounded result/source Location/job generation/delivery identity; safe busy boundary or next turn, no idle generation/polling | NOT_RUN |
| Recovery | Verify identities; committed terminal delivery after reopen without replay; uncommitted/unknown effects quarantined; COMMIT faults roll back and durable identity deduplicates | NOT_RUN |
| Ownership/bounds | Global and per-session inflight/queue admission before effect; distinct jobs overlap/reverse-complete; cancel/shutdown/fatal/drain joins provider/tool/PID leaves; owner join/wake races | NOT_RUN |
| Typed controls | Bounded CoreApp family/state query and exact selected interrupt; reject stale/foreign session/job/generation without parent/sibling effects | NOT_RUN |
| Native proof/gates | Normal debug/release headless and PTY loopback/stdio receipts, immutable raw/fork/revert regressions; required serial Cargo/Python47 gates | NOT_RUN |

Immediately subsequent distinct atomics: linked-live-child TUI navigation and
visual controls, command routing, and Ctrl+B foreground-to-background conversion.
This atomic preserves borrowed concurrent foreground futures; conversion cannot
be claimed from a background flag or kill/restart. Family facts and selected
interrupt here must be real owner operations.

## Minimum justified seam

`Runtime` currently borrows `Db`; `TurnSubagent` also borrows catalog/provider,
parent lane/cancel and an immutable MCP request lease. Detached work cannot retain
these borrows. Use a private borrowed-or-owned Db handle in this same Runtime,
with owned snapshots retaining **Db::shared_handle's same connection and flock**,
captured publication/workspace/Location/policy and shared application approval,
question, shell and MCP owners. No unsafe, leaked/self-referential allocation,
second database/catalog or new generic task framework. One bounded child owner
retains task completion receipts and child-local cancellation; snapshot construction
must not reopen storage or independently connect MCP. Parent publication changes
must not retarget captured child work.

Existing `tool_operations` rows describe the immutable invocation/result and turn
checkpoint. Replacing their running result on completion would falsify launch
history and break provider call/result pairing. They have neither a child-job
generation fence nor an independent terminal/delivery identity. Therefore one
additive child-job relation in existing Db is justified **before schema edits**:
operation/parent/child/source/generation/admission, running/terminal outcome,
stable delivery identity and committed message link. Same-child live uniqueness
and bounds must be checked before child prompt/effects. Terminal and notice writes
are transactional; message+delivery link commit together. Queries use bounded
rows, not full family archives. Reuse conversation visibility for delivery inputs
so fork/Revert never replay effects or inject reverted notices.

## Pinned donor questions / native safety boundary

Pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`:
`tool/plugin/subagent.ts:153–255` retains child continuation, admits prompt then
starts owned job, backgrounds immediately, and uses typed job error/cancel;
`session/subagent-job.ts:20–58` observes each child generation;
`session/subagent-completion.ts:20–45` synthetic notice includes source/child/agent/
state and notification identity. `session/execution/restart.ts:137–188` validates
lineage, delivers terminal directly, otherwise resumes session at-least-once.
U25–U30 locators in `tui-recovery/SOURCES.json` define profile defaults/selection,
not permission to migrate an admitted child or widen its parent ceiling.

Native `docs/CONTRACTS.md` requires write-ahead intents, no unknown mutation/shell/
MCP replay, real permission waits and pinned execution context. Current native
turn checkpoints are **not** donor durable session-inbox/drain recovery; a
nonterminal checkpoint does not prove no external effect between dispatch and
COMMIT. Until a safely resumable prefix is actually proven, recovery records an
authoritative unresolved/unknown state requiring explicit recovery, never silently
reruns the prompt or claims exactly-once effects. A committed terminal child turn
may be reconciled without any provider/tool dispatch. Safe unfinished resumption
remains an explicit qualification obligation, not an invented PASS label.

## Current execution report

Implementation and all proofs NOT_RUN at freeze. No source/schema edits yet.
No paid API or T27 ledger use. Root inherited `.opencode/` excluded from work.

## Current qualification — unstaged handoff, 2026-10-04

The frozen section above is retained verbatim. Application-owned background
execution, durable terminal/delivery/recovery facts and typed selected interrupt
are implemented and qualified below. This is **not whole T45/R3/SUB01/SUB02,
READY, safe unfinished-turn resume, Ctrl+B conversion or T44 visual PASS**.

Current HEAD remains the base above. Current Rust source association (sorted
`crates/**/*.rs`, relative path + NUL + bytes + NUL) is
`72f8ff17c708aeaf9566cb93b440155eb748d0feba39ad3910aedfea80d4b51f`.
No source edits follow the current gates/normal builds. `docs/CODE_MAP.md` records
the actual ownership seam; concurrent owner's progress/spec/acceptance documents
have not been edited.

### Frozen obligation outcomes

| Frozen obligation | Current outcome / actual proof |
|---|---|
| Genuine immediate launch | **Owned start PASS; stronger frozen transport-order assertion NOT_PROVEN.** The real child future accepts/commits its own turn before the running result is released. Physical child POSTs, effects and terminal notices occur while the parent's physical follow-up is blocked. Peer-handler arrival order versus receipt of running output is not guaranteed; the frozen wording is not relabelled PASS. |
| Fresh context / continuation | **PASS.** Actual own profile/model/system/workspace/policy and captured source Location; parent transcript absent. Two generations continue one child with own history/model preserved; distinct operation/delivery identities. Busy same-child continuation is refused before prompt/effect, without a fork. |
| Authority / effect safety | **PASS for this lifecycle seam.** Real profile Ask + concurrent sibling preimage change causes approved patch recheck refusal. Retained Deny/Ask/parent ceilings/General/Explore/depth/model-budget/closed-response/native mutation, provider and MCP lease/quarantine regressions pass in the full workspace. Source snapshots share existing queues/resources, never grant new authority. |
| Durable lifecycle | **PASS.** Additive same-Db relation stores admission/running/current terminal/generation/delivery. Original tool operation/output stays running. Real `response.failed` becomes typed Error while successful `error:` prose becomes Completed; no LLM-text error classifier. |
| Notice | **PASS.** One transactional native source=subagent notice per generation, bounded result, exact child/agent/state/description/op delivery/source identity. Two reversed completions produce one safe parent continuation batch with original call IDs once. Idle/reopen never generates or polls. Existing eight-notice batch cap retained. |
| Recovery | **Committed recovery, rollback/dedup and unknown-effect quarantine PASS; unfinished safe resumption NOT_PROVEN.** Native COMMIT faults recover the real committed child assistant output/deliver once without provider/tool replay. Actual crash after shell effect produces typed Unknown and refuses unsafe explicit continuation before child prompt. No automatic unfinished-turn replay or exactly-once external-effect claim. |
| Ownership / bounds | **PASS for exercised native lifecycle.** Global eight inflight and eight undelivered admissions, per-parent four, zero queued background work. Six-call fixture admits four before effects and refuses two. Independent/reverse jobs, parent cancel, selected cancel, normal shutdown, fatal delivery/terminal faults and join/wake completion paths are exercised. Owned shell leader/PID leaves are reaped. |
| Typed controls | **PASS.** Actual `CoreApp::child_jobs` (bounded sixteen current/recent generations) and `interrupt_child` reach the owner. Foreign caller and stale child/op/generation/Location/delivery selections fail without parent/sibling cancellation. Source work survives actual parent Location switch; own shell effect remains at source. Conversion is absent. |
| Native proof / gates | **PASS for current atomic evidence.** Fourteen actual normal-ELF cases, both debug/release, headless/PTy, loopback/stdio only; fork/Revert/native-origin proof below. Required current serial Cargo/Python47 and read-only docs/progress/size checks pass. |

### Implementation / ownership review

- `runtime/children.rs`: 476-line focused owner and private borrowed/owned Db
  seam. Owned Runtime snapshots retain the **same** shared connection/data lock,
  captured config/Location/model/lane, existing approval/question/shell/MCP owners.
  Task owns the snapshot; retained Work uses Weak Runtime plus native MCP owner,
  avoiding a terminal job/runtime ownership cycle. No unsafe/self-reference,
  reopened store, duplicate catalog or generic orchestration framework.
- `storage_children.rs`: 195-line additive relation, transactional admission,
  bounded terminal result, committed-assistant recovery, message/link/event
  delivery transaction. No launch-row rewrite. Pending/live checks precede child
  creation/prompt/effects. Unknown generations cannot continue implicitly.
- Application worker shares owners across moves/reloads; retired source MCP is
  joined after its last job. Original-source approval replies use actual request
  bindings. Busy and idle wake paths join completed work and persist notices;
  outer worker wrapper joins children on every return/error/publication failure.
- Borrowed scoped foreground join/predecessor serialization is preserved.
  Singleton subagent calls use the same admitted executor so immutable background
  launch is correctly recorded as running rather than completed.
- Native `subagent_launch` provenance is committed with admission. Fork copies
  validated immutable launch facts/native origin, **not child jobs or capability**.
  Recursive-fork/foreign-origin/earliest-hidden-prefix Revert private tests and
  normal PTy selected-fork continuation/Revert prove branch input visibility and
  immutable source history. Existing raw/hot notice input machinery is reused.
- New substantial scenarios are in the existing `subagent` target's separate
  `tests/fixtures/background_children.rs` and private `storage_children/tests.rs`.
  Obsolete explicit background-Unsupported cases alone were removed and replaced
  by actual launch/lifecycle proofs; other old guards/assertions/ignored tests,
  product deadlines and caps remain.

### Current gates and exact receipts

All logs are exclusive lossless gzip under
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`.
Below, `NAME` means exact `t45-background-NAME.log.gz`. Each modern log ends in
`RECEIPT` with exact argv/exit/counts/raw SHA256/base/source before+after and
owned watchdog PID/PGID/startticks/reaped. Watchdog is `timeout --kill-after=2s
1798s`; tool timeout 1800000 ms. Cargo is serial with `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`, `CARGO_NET_OFFLINE=true`, and TMPDIR equal to this cache.

| Receipt | Exact command | Current result |
|---|---|---|
| nearest8 | `cargo test --locked -p oc-adapters --test subagent` | 24 passed / 0 failed / 0 ignored; 128.419 s |
| fmt7 | `cargo fmt --all -- --check` | exit 0 |
| clippy7 | `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0 |
| norun4 | `cargo test --locked --workspace --no-run` | separate exit 0 |
| workspace3 | `cargo test --locked --workspace --no-fail-fast` | **1508 / 0 / 10 unchanged ignored**; 1085.037 s; raw 139564 bytes, SHA256 `a386da2018bbcc230cbb0918d91aecf1d40f2db71dc768cad379a96dcea4e9af` |
| debug-build3, debug-help3 | `cargo build --locked -p oc`; separately `target/debug/oc --help` | both exit 0 |
| release-build3, release-help3 | `cargo build --locked --release -p oc`; separately `target/release/oc --help` | both exit 0 |
| native14 | `python3 evidence/T45/native_background_children.py target/debug/oc target/release/oc` | **14 actual cases PASS**, exit 0, 20.383 s; raw 51354 bytes, SHA256 `99e6df6afe7cee25d2243d6b796b9d2e8f4d75b52c205638df2f147821d25ae7` |

Workspace current included adapter lib 504, core 120, TUI 434 and subagent 24
passing cases. All ten ignores are retained pre-existing live/environment gates.
Private storage child fault/fork scenarios number three; current workspace covers
them. Source associations/hash verification are receipts, not Git commits.

Python/documentation receipts: `python47b` already proves 47/0 via
`python3 -m unittest discover -s scripts -p 'test_*.py'`; final read-only
receipts after this report are `python47c`, `docs-final`, `progress-final2`,
`size-final`, `diff-final`, `cleanup-final`. Their commands and source/counts are
in the retained receipts. Documentation validation is structural, not product
acceptance; `progress.py check` performs no journal mutation.

### Retained normal native ELFs / physical counters

Before = after current normal proof, no intervening Cargo:

- `target/debug/oc`: `14529faf67653e151709a080ef232b1898fa1e1ed2bd1c67bc630a1074443a42`
- `target/release/oc`: `71ba7f905e715996a6fdf2cfc8ce05348206fd441938556125e43ff9cb7e4e52`

Exact counts below are **per ELF**, identical in debug and release. Main means
physical non-auxiliary requests, including parent and children. Tool ledger rows
include genuine immutable copied fork metadata, separately from physical effects.

| Actual normal case | Physical POSTs (parent / child / auxiliary) | Child jobs | Tool ledger rows | Real child / parent effects | Native notices |
|---|---|---:|---:|---:|---:|
| Reverse headless | 7 (3 / 3 / 1) | 2 | 4 | 1 / 1 | 2 |
| Reverse PTy + actual Fork/selected continuation/Revert/replacement | 10 (6 / 3 / 1) | 2 | 7 | 1 / 1 | 2 |
| Real typed child failure versus successful `error:` prose | 7 (3 / 3 / 1) | 2 | 4 | 1 / 1 | 2 |
| Parent cancel + actual owned shell leaf | 4 (2 / 1 / 1) | 1 | 2 | 1 / 0 | 1 |
| Terminal COMMIT fault / committed child recovery | 5 (2 / 2 / 1) | 1 | 2 | 1 / 0 | 1 |
| Delivery COMMIT fault / rollback + delivery after reopen | 5 (2 / 2 / 1) | 1 | 2 | 1 / 0 | 1 |
| Unknown shell effect crash + refused unsafe continuation | 6 (4 / 1 / 1) | 1 | 3 | 1 / 0 | 1 |
| **Per ELF total** | **44 (22 / 15 / 7)** | **10** | **24** | **7 / 3** | **10** |

Both ELFs total **88 physical requests**, parent44/child30/auxiliary14,
20 child job generations, 20 physical tool effects, 20 original native notices.
Per ELF there are 21 actual admitted tool invocations (including the refused
unsafe continuation), plus three immutable fork-copied ledger rows. PTy copied
notices are branch history, not additional delivery or effects. Two idle reopens
per scenario produce **zero** new physical generation/tool effect/delivery.
The unknown scenario's additional two parent requests are intentional explicit
user continuation, not recovery replay; refused child prompt is absent.

### Retained failed/development attempts — no overwritten failures

`inspect_background_receipts.py --failures` prints exact modern argv/exit/counts/
raw hash/source/owner receipts and verifies decompressed hashes. All failed logs
remain; none is presented as a current gate PASS.

| Log names | Exit / diagnosis / eventual stronger proof |
|---|---|
| red | 101, 0/1/0: real barrier RED against old Unsupported behavior. Exact command `cargo test --locked -p oc-adapters --test subagent background_child_posts_effects_and_completes_before_parent_response`. |
| compile1, compile2 | 101: initial duplicate-path apply_patch/lifetime/type fixes; no fixture effects. These early compilation logs do not retain exact argv. |
| nearest2 | 101, 18/2/0: obsolete expected background rejection, replaced only with stronger genuine lifecycle scenarios. |
| storage1 | 101, 1/1/0: deferred fault table recreated; IF NOT EXISTS fixes fixture. |
| app1 | 101 compile: missing test imports. |
| app2 | 124, fixture watchdog 1798 s: initial unbounded wait + auxiliary title request misclassification; no child job/tool-effect intent. Exact owned interrupted TempDir cleanup below. |
| app3 | 101, 0/1/0: real auxiliary tools:[] recognition fixed. |
| app4 | 101, 1/1/0: exposed selected-cancel versus five-ms proxy race; owned work now uses actual selected token directly. |
| nearest4, continuation1 | 101, 3/1/0 and 0/1/0: exposed singleton immutable-running-state bug; shared admitted executor fixes production. |
| fmt1 | 1: formatting diff only; formatter then all current strict checks pass. |
| nearest6 | 101, 23/1/0: unjustified transport-handler arrival order; retained frozen NOT_PROVEN boundary and genuine overlap/acceptance/physical-effect proof instead. |
| fork1 | 101 compile: attempted private storage call from integration target; substantive fork scenario placed in existing private owner target without public test API. |
| native1 | 1: fixture incorrectly required interrupted started tool row unchanged; stronger identity/no-new-operation proof allows only native started-to-unknown recovery. |
| native2 | 1: actual immutable running launch blocked Fork; native-origin proof added to production and qualified in current workspace/normal ELFs. |
| native3–native10 | 1 each: PTy driver anchor/height/bounded-tail/selection/restored draft/action-index errors. Correct actual Fork/Revert actions now pass. No visual acceptance inferred. |
| python47 | 1: module imports require scripts discovery root; python47b/c actual 47-case discovery succeeds. |
| progress | 2: invalid readonly subcommand `validate`; actual `progress.py check` succeeds. |
| progress-final | 2: simultaneous structural docs/progress checks collided on the existing journal lock; sequential progress-final2 exits0. No journal/lock mutation or baseline change. |

`storage2` exit 0 matched **zero** tests and is not scenario evidence. Correct
private owner filter is `storage::children::tests`, proved by storage3/fork2 and
current full workspace. Earlier workspace/normal proofs are historical only;
current claims use workspace3/build3/native14.

**Early receipt limitation:** red/compile1/compile2/nearest1 were losslessly
captured before the receipt runner existed, so their pre-run source digest and
PID/startticks were not retained; compile1/compile2 argv is also unrecorded. Do
not invent them. Base/frozen edit order and the RED argv are known above; current
strongest proofs have complete receipts. Their raw
SHA256 values are respectively:
`7c5a091d86827a67248a8c451820f361ffffed92b2a275f563c000608812efe9`,
`065ee4fd7b843d38a5f310262eee1ad8da0927ce472d5a4a3bc01311a26ea51e`,
`655234ade7fd4f4c97a778b4aabdde4b53354cd1cad73c0113e412cc1b5af909`,
`8185b90dd5623bcccc3820f1086387ffd4d288fb322560a8ae93d23024d9f68b`.

### Cleanup / authority release / remaining atomics

Current `native14` records every native owner PID/PGID/startticks/exit/reaped,
four actual owned shell leaves, and fourteen exact TempDir paths removed only
after native processes, HTTP non-daemon handlers/server workers and PTy readers
join. Fault native exits1 and intentional crash exits-9 are expected and explicit.
Crash leaf reaping uses a temporary restored child-subreaper and exact owned
PID/PGID, not broad waits/signals. All gzip logs remain; no fixture/Cargo trees
are retained by this atomic.

The old app2 watchdog left exactly
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/.tmpEKnryJ`.
Owned log timing + exact synthetic OWN_BACKGROUND_PROFILE config + journal
parent/PARENT_PRIVATE identify it; journal has one failed parent turn,
**zero child jobs and zero tool-effect intents**. PID/PGID3608777 is absent.
`aborted-cleanup-proof` retains this ownership proof; only this exact directory
was removed, then `aborted-cleanup-after` proves no candidate remains. No foreign
historical pruning/deletion was performed. The final `cleanup-final` independently
checks all retained modern runner/native/leaf PIDs and exact emitted TempDir paths.

Final independent cleanup audit checked **307 recorded owned PIDs**, all absent,
and **40 exact emitted native TempDirs**, all removed, including fourteen from
the current native14 proof. It also rechecked the retained normal ELF hashes.
Modern logs total **213216 compressed bytes**. Atomic evidence + every retained
gzip log is **below 270000 bytes**, comfortably below **1 MiB**;
`inspect_background_receipts.py --budget` gives actual metadata totals.
No paid API/real user HOME/env/.local/authoring credentials, T27 ledger changes,
staging/commit/push, progress/spec/GOAL/planning/acceptance writes or T44 visual
claim. Root inherited `.opencode/` was excluded.

Next distinct atomics remain linked child TUI/family navigation/visual controls,
command routing and genuine same-job foreground/background conversion; other
ordered T45 DCP/context-pack work remains. **Safe unfinished native resumption
still requires pinned donor semantics plus actual prefix/effect-safety proof**;
typed Unknown/explicit recovery is the delivered truthful boundary here.
Parent independently reviews this diff and delivers/continues the full plan.

At handoff, **ALL mutation, Cargo and owned-offline-fixture authority is released**
to the parent. No owned worker/provider/tool/PID/PGID/HTTP/PTy work remains active.

## Reacquired narrow repair — frozen before repair RED/source edits

Parent explicitly reacquired this coordinator on the same HEAD and preserved dirty
source. All results above are historical, including 1508/0/10 and normal hashes.
No parent source changes. This repair does not complete T45/R3/READY.

| Obligation | Permanent RED and minimum owner seam | Initial status |
| --- | --- | --- |
| Seven durable terminal-undelivered jobs + two concurrent reservations | Barrier both reservations before fresh admission; eighth succeeds, rejected ninth leaves no session, Location pref, prompt, event, effect or unrelated deletion. Extract the existing fresh creation/admission sequence without changing its broken transaction order for RED, then place session + Location + job in one existing SQLite transaction. Existing-child and foreground semantics retained. | NOT_RUN |
| Dropped shutdown waiter retains work | Held shared native child join; poll shutdown, drop it, retry must still wait, reap exact owned leaf and preserve failed join as sticky failure. Keep original Work in Jobs until join and source stop settle. | NOT_RUN |
| Dropped retired-source stop retains owner | Real stdio MCP leaf plus held immutable request lease; interrupted reap followed by reap must await lease retirement/stop and reap leaf. Keep retired Arc in Jobs until actual stop returns. | NOT_RUN |
| Child job migration | Existing native `schema_migrations` convention; additive version 10, same single table/indexes, transactionally record version; reopen/upgrade preserves facts. No new dependency or migration framework. | NOT_RUN |
| Native owned-start ordering | Actual native journal acceptance/child-running precedes immutable parent running result; physical child POST/effect/terminal while parent held. Literal peer POST arrival before running remains distinct until demonstrated, never manufactured by an acceptance callback. | NOT_RUN |
| Retained lifecycle regressions | Fresh complete gates, seven background native cases and foreground reverse/cancel on both final normal ELFs; no Cargo between physical proofs and before/after hashes. | NOT_RUN |

The schema need remains the previously justified generation/terminal/delivery
relation. This repair changes only its migration registration and atomic fresh
admission. SQLite shared_handle/exclusive root lock and original Jobs/MCP ownership
remain authoritative. Safely unfinished resumption, linked TUI/commands, conversion
and child DCP remain separate owned atomics, not external blockers or PASS claims.

## CURRENT repair qualification — 2026-10-04, unstaged handoff

This section supersedes the earlier **historical** current-gate/hash claims; both
pre-RED freezes and all historical results above remain intact. HEAD is still
`c8efe735a1b35d4bf3052bc9ca9076109d650c88`. Current Rust source association is
`b546d4ad4fa7b871c9482e2a2aef3f6e87ca1c095dee64264e860ebbe91650b7`
(the coordinator's sorted `crates/**/*.rs`, relative path/NUL/bytes/NUL method).
No Rust changes followed the current full gates or normal builds.

### Repair obligation outcomes and reviewed seam

| Frozen repair obligation | Current factual outcome |
|---|---|
| Seven pending facts, two reservations | **PASS.** Deterministic permanent barrier RED reproduced the orphan: both real owner reservations pass seven terminal-undelivered facts, but only one durable admission fits. GREEN commits exactly one fresh session/Location/job, leaves the losing child entirely absent, and preserves unrelated history/preferences. Genuine admission COMMIT fault also rolls back session, binding, events and job together. |
| Cancelled shutdown waiter | **PASS.** Original Work remains in Jobs through the shared child join and native source stop. Dropping the pending drain and retrying still waits; actual owned shell leaf is reaped. Both successful and failed shared joins run; failure remains sticky on later shutdown. |
| Cancelled retired-source waiter | **PASS.** Original retired Arc stays in Jobs through actual native MCP stop. Dropped reap/retry waits for a held immutable request lease; releasing it joins the actual stdio MCP leaf before retirement removal. |
| Native versioned migration | **PASS.** Same single child_jobs table/indexes registered as native schema version **10 / t45-child-jobs** in one transaction. Upgrade/reopen preserves existing facts; forced migration-marker failure rolls back and safely retries. |
| Owned native start before running result | **PASS for durable native start.** Both normal ELFs use a temporary SQLite guard at the actual immutable parent tool-result UPDATE to require the exact child job Running, own started turn, real turn_acceptance and own user message already committed. Twenty original launch results satisfy this guard; recorded launch/turn-start event sequences are included in native receipts. Reverse fixtures retain physical child POST/effect/terminal before parent completion. **Literal peer POST arrival before running remains NOT_PROVEN**, distinct from this native owned-start contract; no external blocker, synthesized acceptance callback or global wait for model output. |
| Retained lifecycle / foreground prerequisite | **PASS for this repair.** Current full workspace plus both normal ELF suites cover independent work, reverse completions, own context/policy/model, real Ask/preimage guards, typed failure versus successful error prose, exact controls/source, immutable running output, deduplicated notice/reopen, native COMMIT faults/unknown quarantine, Fork/Revert, and real parent-cancel leaf reaping. Borrowed scoped foreground reverse/cancel regressions also pass. |

Reviewed repaired paths:

- `crates/oc-adapters/src/storage_children.rs`: fresh background admission uses
  the original connection mutex and a single SQLite transaction. Capacity and
  invoking parent/subagent operation are checked **before** creating the child;
  native session row, Location binding, session-created event, immutable job and
  launch provenance share COMMIT. Existing-session continuation uses the same
  owner but retains its parent validation. No orphan deletion/compensating cleanup,
  second store, unsafe/self-reference, dependencies or migration framework.
- `crates/oc-adapters/src/runtime/turn.rs`: fresh background IDs are passed
  uncreated to that atomic owner; foreground creation remains its existing path.
- `crates/oc-adapters/src/runtime/children.rs`: retains original Work and retired
  owner entries until actual joins/stop return; interrupted waiters lose only
  clones. Sticky failure/quarantine facts are retained. Existing Work lock also
  makes late publication versus closing/snapshot atomic, without an await under
  the lock or new global supervisor/queue/retry/cap.
- Permanent new scenarios are private
  `runtime/children/tests.rs` (three) and `storage_children/tests.rs` (two added;
  five total). `storage/tests.rs` updates only two exact native migration
  inventories to include version10; all old versions/session-column/sentinel/
  history assertions remain. No ignored test, baseline or deadline weakened.
- `evidence/T45/native_background_children.py` adds the actual native owned-start
  COMMIT guard/proof; the retained foreground driver is unchanged.
  `inspect_background_receipts.py` audits current selected normal proof hashes,
  exact retired-fixture cleanup paths and all retained receipt identities.
  Existing CodeMap already describes this seam and needed no repair edit.

### RED / GREEN / failed repair receipts

Exact cache prefix remains
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`.
Here `repair-NAME` denotes exact `t45-background-repair-NAME.log.gz`.
All new logs are exclusive lossless gzip with verified trailing RECEIPT containing
exact argv/exit/count/raw bytes/SHA256/source before+after/HEAD and watchdog
PID/PGID/startticks/reaped. Commands below are the argv **inside** the existing
`python3 evidence/T45/run_background_check.py repair-NAME ...` wrapper.
Cargo remains serial, jobs3/testthreads1/offline, locked; operational
timeout1798s/kill-after2s and tool1800000ms never alter product/test deadlines.

| Receipt | Exact argv | Result / diagnosis |
|---|---|---|
| repair-red1 | `cargo test --locked -p oc-adapters --lib runtime::children::tests -- --nocapture` | exit101; **0 pass / 3 fail**; all admission/drain/retirement defects reproduced; owned leaves cleaned before failure assertions |
| repair-red2 | `cargo test --locked -p oc-adapters --lib storage::children::tests -- --nocapture` | exit101; **3 pass / 2 fail**; missing native version and orphan after admission COMMIT fault |
| repair-green1 | `cargo test --locked -p oc-adapters --lib runtime::children::tests -- --nocapture` | exit124, watchdog1798s; test-driver deadlock: retry held native stop mutex while fixture awaited a second stop instead of polling retry. Fixed fixture ordering, no product deadline change. Lossless log retained. |
| repair-green2 | same exact owner-test argv | exit0; **3 / 0**, both drain outcomes and actual lease retirement/leaf joins |
| repair-green3 | same exact storage-test argv | exit0; **5 / 0**, including the original three fault/recovery/fork scenarios |
| repair-nearest | `cargo test --locked -p oc-adapters --test subagent` | exit0; **24 / 0 / 0 ignored** |
| repair-fmtcheck / repair-clippy / repair-no-run | current fmt/Clippy/no-run argv below | exit0, preceding source association7fb9cb…; historical after the inventory fix |
| repair-workspace | `cargo test --locked --workspace --no-fail-fast` | exit101; **1511 / 2 / 10 unchanged ignored**. Both failures were the old exact migration inventories omitting new version10; updated strictly to require the native migration, preserving all other assertions. Full suite rerun below. |
| repair-migrations | `cargo test --locked -p oc-adapters --lib child_schema` | exit0; **3 / 0**, current source |

RED source associations:
`169feb2a9526de3dac02fdcad44d73b36d314716eefc2edecfee8830cd582e66`
(red1) and
`ac9fab1c4a28f7b4d511f4b4913f352a48520dc72f1dbb100cbde29a74e8f635`
(red2). Their raw SHA256 are respectively
`45c4f53799c2f757896804f90060a06cfbceda056150f83f8b658c6b2f8076bc`
and `35cf730b268f98375cfbf6084da795e477aaff544dc6372add3f88691131bef1`.
Owner RED runner PID/PGID3811920/startticks180050359 and storage RED
PID/PGID3814821/startticks180071427 are reaped. GREEN2/3/nearest pre-format
association is `b4b716869db545bdccf7fdb56b9c4a364221e0fe2fddea0f1f7c59571475aed5`;
all current gates below use the full current association stated above.

### CURRENT fresh gates

| Receipt | Exact argv | Current result |
|---|---|---|
| repair-fmt2 | `cargo fmt --all -- --check` | exit0 |
| repair-clippy2 | `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit0 |
| repair-no-run2 | `cargo test --locked --workspace --no-run` | separate exit0 |
| repair-workspace2 | `cargo test --locked --workspace --no-fail-fast` | **1513 passed / 0 failed / 10 unchanged ignored**, exit0, 1028.552s; adapter lib509/core120/TUI434/subagent24 |
| repair-debug-build | `cargo build --locked -p oc` | normal debug, exit0 |
| repair-release-build | `cargo build --locked --release -p oc` | separate normal release, exit0 |
| repair-debug-help | `target/debug/oc --help` | exit0 |
| repair-release-help | `target/release/oc --help` | separate exit0 |
| repair-native | `python3 evidence/T45/native_background_children.py target/debug/oc target/release/oc` | **14 / 0**, actual headless/PTy normal ELFs, exit0,20.048s |
| repair-native-foreground | `python3 evidence/T45/native_foreground_children.py target/debug/oc target/release/oc` | **4 / 0**, actual reverse/cancel normal ELFs, exit0,5.871s |
| repair-python47 | `python3 -m unittest discover -s scripts -p 'test_*.py'` | **47 / 0**, includes code-size/progress/docs/bounded-live tests, exit0 |
| repair-docs | `python3 scripts/check_docs.py` | exit0, read-only |
| repair-progress | `python3 scripts/progress.py check` | sequential exit0, read-only |
| repair-size | `python3 scripts/code_size.py --base HEAD` | exit0, read-only |
| repair-diff | `git diff --check -- . ':!.opencode'` | exit0 |
| repair-cleanup | `python3 evidence/T45/inspect_background_receipts.py --cleanup --normal-log=t45-background-repair-native.log.gz --exclude-log=t45-background-repair-cleanup.log.gz` | exit0; current normal hashes and no active owned workers/leaves/retained exact fixture dirs |

Current workspace raw140135 bytes SHA256
`493906be76d6526de4f18fbbc88c27d634ba878ce0c3c70f47dc8aa9b6e16502`;
runner PID/PGID3900892/startticks180476899 reaped. Background native raw55119
SHA256 `afebd2aae050179ae4b5e1c9145d7a68e5ccf4fea3e511c87a80c1ee551b6049`,
runner3926111/startticks180599071 reaped. Foreground native raw45135 SHA256
`20026bb5222bd5d8baf7ccfa23733934a0fefffb0ee475c3e8fe27399c42e0a9`,
runner3927044/startticks180601603 reaped. The unchanged foreground driver hashes
tracked Rust only (`fdfb3aa3facd09a51569c28b95e9dc2043a5e3c203ef288cb781ddeb35472fe4`);
its coordinator receipt additionally captures the full current Rust association.

### CURRENT normal ELF hashes and physical counts

After the last normal builds, both direct suites record identical before/after
hashes, **with no intervening Cargo**:

- `target/debug/oc`: `b83a3587d08b644bb3b6c56e98481e3e8ffcdbddb5e29a5ecce025d5f8001e85`
- `target/release/oc`: `7c52f8665b06fe55469c03292a3b9ebfae3afacbb8ea69dba59ae2a95e3eabec`

Per ELF, the seven background rows have exactly the physical/Db/effect/notice
counts in the historical seven-row table above: **44 POSTs**
(parent22/child15/aux7), **10 real job generations**, **24 tool ledger rows**
(21 actual invocations plus3 copied immutable fork rows), **7 child +3 parent
physical effects**, **10 original notices**. Each has two idle reopens with zero
provider/tool replay or extra delivery. Source-only copied branch facts confer no
job ownership. The new native guard records **10 genuine owned starts per ELF**
before immutable parent running result COMMIT, not invented table labels.

Per ELF, additional foreground reverse/cancel regressions have:

| Case | POSTs (parent / child / aux) | Fresh foreground children | Tool ledger rows | Physical tool effects | Background notices |
|---|---|---:|---:|---:|---:|
| Reverse headless | 6 (2 / 3 / 1) | 2 | 3 | 1 patch, exact2 bytes | 0 |
| Cancel PTy | 4 (1 / 2 / 1) | 2 | 3 | 1 actual owned shell leaf / PID-file effect, reaped | 0 |

Both current suites together: **108 physical POSTs**
(parent50/child40/aux18), **20 background job generations +8 foreground
children**, **60 ledger rows** (54 actual tool invocations +6 immutable fork
copies), **24 physical effects**, **20 original background notices**, and
**six actual current owned shell leaves reaped** (four background/two foreground).
These are functional native PTy proofs, not T44 visual acceptance.

### Cleanup, bounded evidence and remaining owned obligations

New watchdog timeout repair-green1 retained two positively identified isolated
fixture directories: `.tmppKLv2E` (exact stdio fixture mcp.pid3817888, absent) and
`.tmp25RSxz` (read-only journal: zero sessions/jobs, native schema10 marker).
`repair-timeout-inspect` retains ownership proof, raw425 SHA256
`98799517e9a26e293f34adc6f2b72316d3cc0104b0e9fd87d9c7db7230f255d8`.
Runner PGID3817246 was empty and the exact leaf absent **before** deleting only
these two owned TempDirs. Final audit also checks their absence; no foreign prune.

Current background receipts emit fourteen exact native TempDir cleanup paths,
native PID/PGID/startticks/exit/reaped, and joined non-daemon HTTP handlers/server
worker/PTy readers before exact TempDir cleanup. Retained foreground contexts
join/reap through their unchanged native fixture owner and remove their exact
TempDirs on context exit. Final read-only inspector audit additionally checks all
recorded historical/current runner/native/leaf identities and emitted retired
fixture paths. Latest direct audit: **397 owned PIDs absent, 56 exact emitted
TempDirs absent**, with the original aborted fixture and both repair timeout
fixtures separately confirmed absent; retained normal hashes still match. A final audit receipt may add its
own already-reaped runner identity to that count.

All failed logs remain lossless. The recorded pre-append budget was375332 bytes
for retained atomic gzip logs plus the four new evidence files; the final
`inspect_background_receipts.py --budget` remains authoritative and **below1MiB**.
No retained fixture/Cargo trees or historical deletion. No paid/live API, real
HOME/env/.local/authoring credentials, T27 ledger changes, staging/commit/push,
progress/GOAL/spec/planning/acceptance writes. Inherited root `.opencode` excluded.

**This is an admission/lifetime repair qualification, not whole T45/R3/READY.**
Committed-terminal reconciliation/delivery once and unresolved Unknown/no replay
are delivered. Safely unfinished native resumption still needs a separate owned
donor/native boundary and effect-safety proof; linked TUI/family/navigation,
command routing, genuine same-job foreground conversion and ordered child-DCP/
context packs remain next atomics. Literal peer-POST-before-running remains its
explicit unproven stronger assertion. Parent independently reviews and delivers.

At the completed repair handoff, **ALL mutation, Cargo and owned-offline-fixture
authority is released to the parent**. Every owned worker/provider/tool/native
PID/PGID/MCP leaf/HTTP handler/PTy reader is joined/reaped before its exact
TempDir cleanup; no coordinator work remains active.
