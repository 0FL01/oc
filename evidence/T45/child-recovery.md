# T45 R3(g) — frozen atomic child recovery

## Coordinator qualification — 2026-10-05

The parent independently reviewed the existing storage, application, Jobs and
runtime recovery seams and their permanent tests. Fresh checks passed: 17 child
owner tests, workspace fmt, strict locked all-target Clippy, and the normal locked
debug build. After that last build, both normal ELFs passed all 40 recovery cases
with no intervening Cargo: 172 physical RPCs matched durable dispatches, 40 original
jobs produced 40 one-time notices, eight completed and 32 quarantined Unknown,
ten resume claims, and zero workspace effects. Same job/turn/accepted input and
committed read/HOT retention were verified; further reopen dispatched nothing.
Owned native processes and HTTP/PTY workers joined before exact fixture cleanup.

The before/after ELF hashes match the values below. These artifacts correspond to
base `e8484f3a14ecda1274e9e6a51c9b6c8137b191a6` plus the reviewed implementation,
not the clean base. The existing full-source workspace gate remains 1523 passed,
zero failed and ten unchanged opt-in ignores. Python47, documentation/progress
structural checks and diff checks also passed independently; structural checks
are not product acceptance by themselves.

The sequencing exception below is acknowledged: claim-COMMIT/crash tests followed
the additive events, rather than preceding them. Their permanent RED/GREEN and
atomic rollback/attempt-bound proofs now substantiate the seam; this report does
not retroactively claim the requested pre-edit ordering was followed.

Recovery captures the current admitted MCP request view before independent initial
MCP startup completes. That view may be empty/pending even for configured enabled
services; it does not grant connected capability or replay an earlier MCP call.
The static review found no effect-replay or authority counterexample. This atomic
does not qualify connected-MCP recovery capability parity, and adds no global
startup barrier. Conservative Unknown refusal for unsafe history remains intact.

This delivers only safe unfinished background recovery. Commands, selected packs,
host behavior, child DCP, donor3.2 and whole T45 remain open; T44 stays PAUSED and
the exhausted T27 allowance is unchanged. No whole-product READY claim is made.

Base: `e8484f3a14ecda1274e9e6a51c9b6c8137b191a6`. Scope: existing
schema-10 child jobs, turn journal/events, application and Jobs owners only.
Frozen before RED or production edits.

## Exact outcomes

1. On application startup, shell recovery precedes child inspection. A committed
   child terminal turn settles its original job and admits exactly one durable
   generation/delivery-keyed parent notice without execution or parent replay.
2. An unfinished background child may resume its SAME admitted job and SAME turn,
   without accepting input or copying its original task a second time. Provider-only
   work before/after dispatch and fully committed native read-only call/result
   groups are eligible. Restore current committed HOT plus current prior history;
   never read old RAW segments to reconstruct forgotten input.
3. Structural safety only: pending/unanswered calls, started/unknown operations,
   any patch/edit/write/shell/MCP or other non-read-only operation in the admitted
   task quarantine Unknown. Completed mutation continuation is deliberately NOT
   admitted in this narrow atomic: exact causal replay has not been proved.
   No prose/regex/model safety classifier and no automatic execution of pending
   calls. Committed read outcomes remain on wire and are not executed again.
4. Verify durable job/parent/child/operation/turn/location/generation/model/profile,
   immutable admission/source and current admitted configuration fingerprint.
   Missing/corrupt/mismatched/unavailable facts, model, credentials or policy refuse
   without fallback or effects. Existing legacy jobs without positive recovery
   facts quarantine. Configuration comes only from admitted product owners;
   credentials are never persisted in recovery facts.
5. Recovery claim and per-task attempt increment commit together before dispatch.
   At most ten recovery attempts (not successful steps or lifetime work). Process
   death before/after claim or during provider execution remains recoverable under
   the exclusive data-root lock. Failure/quarantine is sticky. Physical dispatch
   events count actual at-least-once requests; no exactly-once execution claim.
6. Jobs retains authoritative Work/shared join through dropped observers/shutdown,
   per-child ownership and existing global eight/per-parent four bounds. Parent
   crash may become Unknown; no parent effects are revived. UI/history reads do
   not dispatch. Immutable source provenance remains through terminal delivery,
   Location retirement/reopen; fork gets no inherited job/grant capability.

## Permanent nearest matrix and qualification

Provider-only before/after dispatch; committed read result; pending/unknown native
mutation/shell/MCP; terminal settlement/delivery fault; fresh versus resumed input;
bounded attempts/sticky failures; identity/config/model/profile/policy/corrupt HOT;
claim crash boundaries; normal debug AND release process SIGKILL/restart with fake
provider barriers and real SQLite, request/job/notice/effect/PID receipts and reopen
zero extra requests. Existing selected-child, mode conversion, original Location,
parent draft and root foreground regressions remain required.

Pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`: U39 plugin
`subagent.ts` immediate background/foreground conversion; U40 `subagent-job.ts`
generation observer/session.resume; U41 `subagent-completion.ts` notificationID
and synthetic metadata; U42 `session.ts` background control. Additionally
`session/execution/restart.ts`: recoverShell before recoverSubagent, terminal
notification without rerun, prepareResume durable <=10 per-execution attempts,
and same child execution.resume. Native deliberately narrows donor at-least-once
effects with positive structural safety and does not resume the interrupted parent.

## Results

Implementation and functional gates PASS for the frozen narrow outcomes. This is
an uncommitted atomic handoff, not completion of T45 or the overall Goal. A
workflow sequencing exception is recorded below for the parent reviewer.

### Owners and durable facts

- `crates/oc-adapters/src/storage_children.rs`: unchanged schema 10; admission
  captures `subagent_recovery_fence` in the existing admission transaction. Only
  configuration/credential-route digests are persisted, never credentials.
  The immutable launch fingerprint covers original input, parent turn, call ID,
  occurrence and original input index. The fence also captures exact parent lane,
  job identity, configuration/profile/source fingerprint and its integrity digest.
- The same owner structurally inspects current `TurnLog.working` plus resident
  input, without loading RAW segments. Pending calls refuse. Completed read-only
  groups must match original native operation/session/turn/call ID/input index,
  parsed exact arguments and exact committed output. Any non-read-only operation,
  including completed mutation, quarantines. This also conservatively refuses a
  read result whose retained output cannot be matched exactly to its operation.
- `subagent_resume_claim` is one atomic existing-journal event containing task,
  same turn, attempt and current checkpoint digest. Claim checks current exact
  checkpoint, active accepted user-message anchor/model, child/parent/operation,
  session metadata and original Location. Ten claims per admitted task are allowed;
  an eleventh is refused and the runtime settles Unknown. Successful steps have
  no new lifetime quota. Deferred-COMMIT failure spends zero claims.
- `runtime/children.rs`: startup-only inspection through current admitted runtime;
  exact model/variant/provider/credential route and policy/profile/source validation,
  eight global/four parent slots, same Jobs reservation/Work/shared join/MCP lineage.
  Neither cached retry nor passive UI/history reads invoke this path.
- `runtime/turn.rs`: an explicit resume argument restores SAME turn and current HOT,
  filters its already accepted anchor from prior history, closes an interrupted
  assistant span as Unknown and issues a new accounted physical attempt. It does
  not accept new input, reconstruct `turns.prompt`, re-execute retained read calls,
  or execute pending calls. Fresh child admission remains a distinct path.
- `application.rs` invokes recovery after publishing admitted workspace, provider
  state and event owner. Storage startup still performs shell recovery first;
  `storage.rs` leaves only positively inspected running child turns available for
  their owner and terminalizes interrupted parent turns. Unavailable facts become
  sticky Unknown without provider dispatch. Terminal notice delivery now also
  checks durable parent/child/operation/delivery identities in its transaction.
- `runtime.rs`/`permissions.rs`: serialization of the existing private captured lane
  and profile/rules types; no new public API, dependency, crate, queue or store.

### RED and nearest matrix

The executed nearest RED was one failure (`Unknown` versus expected `Running`)
before production edits. An earlier `--exact` filter ran zero tests; that receipt
is retained and was corrected immediately, not counted as RED.

Permanent `storage_children/tests.rs` scenarios cover provider-only before/after
dispatch; failed claim COMMIT; crash after claim/reopen; same accepted input and
latest HOT; stale parent/child/location/model/delivery source; ten-attempt bound;
sticky Unknown; pending read; started/unknown/completed patch/write/edit/shell/bash/
MCP; existing terminal/delivery faults and fork isolation. The final nearest
`cargo test --locked -p oc-adapters --lib children` passed **17/0/0**.
Existing owner scenarios retain dropped shutdown observers, actual shared joins,
concurrent mode/terminal conversion and retired source leases.

**Workflow sequencing exception:** the concrete claim-COMMIT/crash regression was
added after the minimal additive fence/claim events were implemented, rather than
before that fact addition as requested. The initial recovery RED did precede
production edits. No schema migration was added. The events are necessary to
distinguish captured configuration/authority from a reused numeric generation and
to retain per-task crash-attempt accounting; their atomic failure behavior is now
tested. Parent review must explicitly assess this sequencing exception.

### Final gates (serial Cargo, offline)

All used `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`, `CARGO_NET_OFFLINE=true`,
approved-cache TMPDIR and the 1798-second owned subprocess watchdog. Every final
command exited 0:

| Gate | Command/result |
|---|---|
| fmt | `cargo fmt --all -- --check` |
| strict lint | `cargo clippy --locked --workspace --all-targets -- -D warnings` |
| separate compilation | `cargo test --locked --workspace --no-run` |
| full suite | `cargo test --locked --workspace --no-fail-fast`: **1523 passed, 0 failed, 10 ignored** |
| normal debug | `cargo build --locked`; `target/debug/oc --help` |
| normal release | `cargo build --locked --release`; `target/release/oc --help` |
| Python | four `scripts/test_*.py` suites, **47 passed** |
| read-only docs/progress | `scripts/check_docs.py`; `scripts/progress.py check` |
| advisory/review | `scripts/code_size.py`; `git diff --check`; reviewed owner diff |

The attempted nonexistent `progress.py validate` command exited 2; its exact
receipt remains retained. The correct read-only `check` passed. Development
compile/fixture failures are also retained losslessly, not suppressed or reset.

Final Rust-source association:
`56f7ac2b2aa19f9382b79055558354add8c8ea9e929221cc747d04cee4604a67`.

Final normal ELF SHA-256, unchanged before/after every native proof and final cleanup:

- debug: `634d49b05188aafca533acf1af8b714c6081b2cfb3ba88585e8ce93daa260ced`
- release: `49a66a52a293f9ecffb6c85f3b2a14323f44ccaab504c44ec3fbd0920dcaa58f`

No Cargo invocation occurred between these final hashes/builds and the native
proofs. Native tests used actual normal executables, PTY, loopback provider and
real SQLite; no test-only executable path or live/paid service.

### Native results — both normal ELFs

`native_child_recovery.py` passed **40 cases**, twenty per ELF. Each fixture starts
one admitted delegated job, SIGKILLs the actual application, then restarts it:

| Case (per ELF) | Total physical RPC / dispatch events | Same job / notice / effects |
|---|---:|---|
| provider-only | 5 / 5 | completed / 1 / 0 |
| committed read | 6 / 6 | completed / 1 / 0; one durable read operation, retained exact call/output |
| crash during resumed provider execution | 6 / 6 | completed / 1 / 0; two durable resume claims |
| selected current HOT fixture | 5 / 5 | completed / 1 / 0; original task absent from recovered wire |
| sixteen refusal cases | 4 / 4 each | Unknown / 1 / 0; zero recovery dispatch |

Refusals: patch, write, edit, shell, MCP, completed mutation, corrupt HOT,
exhausted claims, wrong Location, session metadata, fingerprint, profile, policy,
model, credential and generation. Native refusal operations are owned synthetic
SQLite faults; actual shell-effect crash evidence is independently below.

Across these forty cases: **172 physical RPCs**, matching dispatch events; **40
original jobs**, **40 one-time notices**, **8 completed / 32 Unknown**, **10 resume
claims**, **0 workspace effects**. Accepted input rows, turn IDs/prompts, launch
operation, child/parent IDs and delivery IDs remain unchanged. Every interrupted
parent is Unknown and is not re-executed. A further reopen produces **0 extra RPCs**
and no extra notice. The HOT case installs an owned committed-journal fixture; it
qualifies recovery selection, not an additional child-DCP feature.

Existing actual-native proofs also pass against these exact ELFs:

- `native_background_children.py`: **14 cases** — real headless/PTY concurrent
  jobs, committed patch effects, committed terminal receipt and delivery COMMIT
  faults, truthful failure versus successful error-prefixed prose, cancellation,
  fork provenance, and **actual shell effect + PID/PGID/startticks + SIGKILL**.
  The shell crash retains exactly its initial `x` byte, quarantines Unknown,
  dispatches **0 automatic recovery requests**, delivers one notice and rejects
  explicit unsafe child continuation without admitting its input or rerunning it.
- `native_child_controls.py`: **8 cases** — selected child read-only consumer,
  background conversion, original Location after parent movement, parent draft,
  unrelated root/sibling foreground protection and terminal/stale-control races.
- `native_foreground_children.py`: **4 cases** — reverse completion/causal IDs and
  actual owned leaf cancellation/join; reopen creates no child work.

Total final normal-ELF cases: **66**, all PASS. No exactly-once provider execution
claim: crash during resume demonstrably spends another physical request.

### Donor/source comparison

Donor worktree is clean at the exact pin. `restart.ts` SHA-256:
`cbbe2fcae4852cdf69aa1866e2f52db85f5dc7c35b9b0249e5f438496025bedb`.
Reviewed `prepareResume` lines 77–98, `recoverSubagent` 137–189 and startup ordering
191–229, plus U39–U42 ownership paths listed above. Native compilation and gates
validate the native owner changes. No donor Bun/npm tooling was executed.
Both paths retain same child identity, shell-first recovery, committed-terminal
notification without execution, durable per-execution ten-attempt accounting and
at-least-once requests. Native adds positive structural effect quarantine and does
not revive the interrupted parent. The counter is not a successful-step limit.

### Cleanup and handoff

Approved-cache lossless receipts use `t45-child-recovery-*.log.gz`; final receipts
include exact argv/exit/counts/source associations, raw output hash/size and
watchdog PID/PGID/startticks/reap facts. At cleanup, 34 earlier receipts retained
**74,311 bytes**, plus the **974-byte** cleanup receipt; final documentation/review
receipts add only bounded small output, well below the **1 MiB** atomic limit.

Cleanup verified **288 recorded PID receipts / 286 distinct PIDs**, zero survivors,
**74 native TempDir receipts**, zero surviving paths. Native fixtures joined HTTP
handlers/workers and PTY readers before exact TempDir deletion; crash leaf was
explicitly adopted, signalled by its recorded owned PGID and reaped. Three exact
workspace-test SQLite directories for reaped test PID `99963` and two owned
development bytecache files were removed. Inherited caches/directories were not
pruned. The cleanup watchdog itself was subsequently reaped, as its receipt shows.

HEAD remains the stated base. No staging/commit/push or progress/Goal/spec/planning/
acceptance edits. Inherited `.opencode` was never inspected or edited. T44 remains
PAUSED and T27 counters/resources were untouched. All temporary mutation, Cargo,
offline-fixture and process authority is released to the parent on this handoff.

### Narrow limitations

Automatic resume requires the same admitted Location/generation/configuration,
model/variant, profile/policy and credential route. Legacy jobs without a positive
fence, unavailable sources or uncertain causal output quarantine; no fallback.
Completed mutation/shell/MCP continuation is deliberately quarantined. Foreground
unfinished jobs are not silently converted into recovered background work.
Before-dispatch and claim-COMMIT boundaries have nearest storage tests; the normal
ELF SIGKILL proofs exercise after-dispatch and during-resumed-dispatch boundaries.
This atomic does not qualify command routing, selected packs, host behavior,
additional child-DCP, donor 3.2, T56 or T44 visuals.
