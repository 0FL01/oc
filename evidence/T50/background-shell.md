# T50 — frozen owned-background shell slice

## Parent independent verification and delivery scope

At planning-only HEAD `a0fbbd406a87b7f2934381af2b9a0aac84ecf5e1` plus the reviewed
T50 implementation, the parent reviewed the foreground/background invocation,
central admission and saved-grant boundaries, shared connection/root lock,
supervisor launch/teardown, actual shared join, scoped delivery/output/cancellation,
visible-conversation lookup and exact fork-prefix/DCP reference projection.
Independent targeted checks passed: two private join/aborted-shutdown tests, one
nested fork/DCP test and all three real-application background lifecycle cases.
After restoring the normal debug build, independent actual-native counterexamples
also passed: idle capacity **4 requests / 9 jobs / 9 notices / 0 idle requests**,
Revert **5 / 2 / 2**, and Fork **7 / 1 / 1**, with zero fork jobs and one source
effect. Both final ELF hashes in the current table remained unchanged. An initial
mistyped `--review-only idle-capacity` was rejected by argparse before execution;
the supported `capacity` selector was then used. No failing invocation is PASS.
Fresh parent fmt, strict workspace/all-targets Clippy, Python **47**, documentation,
read-only progress and diff checks exited 0. The parent independently checked all
42 result summaries in the current complete workspace log: **1334 / 0 / 10**.

The three review regressions were repaired before delivery; original historical
evidence below remains intact. This checkpoint closes only the frozen background
slice. The newly registered T55 has priority at the next safe journal handoff;
T50's remaining R1 inventory/R3–R8 and PAUSED T44 are not marked complete.

## Independent-review correction — repaired slice qualified

The 1329-test qualification below is **historical and incomplete**, not the final
PASS for this slice. Before delivery, independent review found three introduced
defects: an early worker wake could strand a capacity slot before the join became
ready; raw notice lookup bypassed Revert's visible conversation; forked logs kept
source notice-message references. The repair is restricted to actual joined-worker
readiness, the existing conversation view, and exact fork-prefix message remapping.
The directed RED/GREEN and fresh full gates/both ELF receipts immediately below
supersede those historical results. No additional schema, resource ceilings or
product scope was introduced by the repair. The original frozen obligations and
historical qualification remain below unchanged.

### Three counterexamples and minimal repairs

| Defect | Actual RED | Repaired proof / result |
| --- | --- | --- |
| Premature worker wake stranded join/capacity | Private owner test consumed a notification while its blocking worker was held before return; old idle `changed()` timed out after release, with no second notification. | **PASS**. `Work` retains one shared future around the actual raw join. Idle/busy waits clone that same completion receipt; no sleep/yield/timer polling. `deliver` removes only joined work. The deterministic test reuses all eight slots. A second directed panic test aborts a pending shutdown wait, proves the worker remains owned, then freezes one durable unknown notice and sticky non-success cleanup. The actual native App completes eight jobs while idle with **0 idle requests**, then admits job nine: 9 effects, 9 terminal outcomes, 9 notices, one effect each. |
| Revert resurrected hidden raw notice | Previous debug ELF `8db45574…` with `--review-only revert` failed the first accepted replacement request's no-old-notice assertion. | **PASS**. `shell_notices_after` joins the existing `conversation_messages` view by exact message and source session, retaining its upper-sequence/exclusion rules. Real App and actual TUI Revert hide the old delivered notice; first replacement request contains none, a new job's visible notice appears once. 5 main requests, 2 jobs/outcomes/notices, 2 effects, oldVisible=0/newVisible=1; no external replay. |
| Fork copied notice text but kept source message references | Previous debug ELF `8db45574…` with `--review-only fork` sent the captured delivery identity twice in the fork's actual request. | **PASS**. Existing `storage_fork` message map rebases only exact retained-prefix `shell_notice_messages` IDs and drops missing/hidden/outside-prefix refs. Real App and actual TUI fork after a busy turn captured the notice: fork request exactly once, original request exactly once, **0 fork jobs**, 1 original effect. 7 main requests, 1 job/outcome/notice. Storage coverage also verifies nested rebasing and existing DCP compressed-prefix projection. |

Focused Rust proofs are private `shell/jobs/tests.rs`, the existing runtime target's
`fixtures/background_lifecycle.rs`, and `storage_fork_dcp_tests.rs`; five new tests
extend the prior 1329-test suite. No public test constructor, new controller/store,
extra runner, deadline/cap/golden/ignore suppression or permission change. Production
repair paths are only `shell/jobs.rs`, `storage_shell_jobs.rs`, `storage_fork.rs`.

### Current source / final normal ELF association

Rust implementation base remains
`592eb1887f15aa853700ba25eea7e042cdaf3d0e` plus the uncommitted background/repair
paths. During qualification the authorized planning-only T55 delivery advanced
HEAD to `a0fbbd406a87b7f2934381af2b9a0aac84ecf5e1`; its 13 committed paths contain
no Rust change. Native T54 receipts correctly report that current HEAD and dirty
source. No staging, commit, push or planning/journal mutation was performed by
this coordinator. Inherited `.opencode/` was never inspected or edited.

Final normal debug and release builds and `--help` exited 0 after all source/test
changes; release compilation was a separate command. No Cargo invocation followed
the release build. Hashes were rechecked after all direct native campaigns:

| ELF | Final SHA256 | Background | Foreground current mode | T54 retry |
| --- | --- | --- | --- | --- |
| `target/debug/oc` | `e4ac976855e374804a7e616deeb7feb2e15ae1ea543e7104eca9a2121939e6aa` | 26 PASS | 21 PASS | 9 PASS |
| `target/release/oc` | `7b0b651d1828a3144cf3dc0157d63bebda5227328bd19df79d6de6e4d1b51af2` | 26 PASS | 21 PASS | 9 PASS |

Per ELF, current background receipts total **68 main/child requests**, **33 admitted
jobs**, **33 terminal outcomes** and **33 stable automatic notices**. All prior 23
cases remain green, including verified/no-signal recovery, pre-fork crash,
outcome-before-publication restart, output pressure, timeout/cancel/reap, admission
failure's non-success shutdown, permission aliases and child ceilings. The three
new native cases contribute 16 requests and 12 jobs/outcomes/notices. Foreground
receipts remain **45 main/child requests, 23 terminal operations, 0 jobs/grants**.
T54's 9 cases retain real socket/dispatch/span accounting and a single committed
legacy-argv effect across retry; auxiliary title sockets are classified separately.
These are offline synthetic loopback functional receipts, not visual or live gates.

### Fresh complete gates and exact logs

- `cargo fmt --all --check`: exit 0.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: exit 0.
  First pass found two needless test-only clones; `std::slice::from_ref` corrected
  them without changing assertions, then strict Clippy passed.
- `cargo test --workspace --locked --no-fail-fast`: exit 0, **1334 passed / 0
  failed / 10 existing ignored**. This includes all five directed Rust regressions,
  real MCP cleanup/security, CFG09/CFG10/UI07, subagent, DCP and retry targets.
- `cargo build -p oc --locked` / debug `--help`, and separately
  `cargo build -p oc --release --locked` / release `--help`: all exit 0.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: exit 0, **47 PASS**.
- `python3 scripts/check_docs.py`, `python3 scripts/progress.py check`: exit 0,
  read-only structural checks (55 tasks/163 acceptance specifications after T55
  planning delivery), not task acceptance.
- `python3 scripts/code_size.py --base 592eb1887f15aa853700ba25eea7e042cdaf3d0e --changed`:
  exit 0; **25 Rust paths / 40632 physical lines / 1606489 UTF-8 bytes**,
  largest touched file `application.rs` 4213 lines; no touched Rust file >5000.
- `git diff --check`: exit 0.

Serial Cargo environment: `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`,
`CARGO_NET_OFFLINE=true`, approved
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`, UID1003,
900000-ms bound per command. Cold compile plus workspace tests reached that outer
bound during TUI tests with no observed test failure. First completed warm suite
had one existing `aud12_binary_delta_before_terminal_and_silent_stream_sigint`
950-ms exit timeout; unchanged targeted rerun passed in 0.58 s and the subsequent
whole workspace passed. No product/fixture deadline or ignored assertion changed.

Logs under `/home/opencode/.cache/opencode-tmp/opencode/`:

- `t50-background-review-workspace-20260930.log`: cold outer interruption.
- `t50-background-review-workspace-final-20260930.log`: completed warm run with
  the single SIGINT timeout (not the qualification PASS).
- **`t50-background-review-workspace-qualified-20260930.log`**: full 1334/0/10 PASS.
- **`t50-background-review-{native,foreground,retry}-{debug,release}-final-20260930.log`**:
  six final direct-ELF receipts, all exit 0.

Owned `t50-background*.log` allocation, including historical runs, is **800 KiB**;
largest individual log is **124 KiB**, well below the 16-MiB bound.

Current direct commands are the three commands in the historical section below,
for each final supplied ELF; the foreground command keeps `--background-supported`.
The expanded background checker has 26 cases and optional `--review-only` focused
counterexample selection. PTY tail remains bounded at 16 KiB with no frame files.
All fixtures have exited, loopback peers closed, temporary roots removed and owned
process groups cleaned. Process inventory after the final campaign shows no owned
Cargo/native fixture/worker process; pre-existing user services/TUIs were untouched.

### Current repair paths and scope

Additional review coverage: `crates/oc-adapters/src/shell/jobs/tests.rs` (121 lines),
`src/storage_fork_dcp_tests.rs` (+114 lines), existing
`tests/fixtures/background_lifecycle.rs` (now 532 lines),
`evidence/T50/native_background.py` (778 lines), and `docs/CODE_MAP.md`'s actual
join/conversation/fork owner seam. Background owners are now `shell/jobs.rs` 430
lines and `storage_shell_jobs.rs` 224 lines. All original source paths listed in
the historical section remain in the reviewed dirty background diff. Migration 5
is still the only justified schema extension; the review repair adds none.

The six frozen obligations below are PASS for this repaired foreground/background
slice, including truthful canonical shell schema, independent bounded supervision,
durable source-session notices, branch/fork causality, honest restart and unchanged
authority. Full R1 inventory and R3–R8/full T50 remain pending; T44 remains PAUSED.
No R8 movement, visual or full-GOAL completion claim. All temporary mutation/Cargo/
owned-fixture ownership is released in the accompanying final handoff.

## Historical pre-independent-review qualification (superseded)

Base: `592eb1887f15aa853700ba25eea7e042cdaf3d0e`. Frozen before the first
actual-binary RED. R2/TOOL13 only; full R1 and R3–R8 remain pending. T44 is paused.

## Atomic obligations

1. Canonical `background:true` returns `status:running` and `shellID` only after
   durable admission and actual supervisor launch. Hidden literal `bash(argv)`
   remains foreground. Background default/zero disable execution timeout only;
   explicit positive timeout obeys the existing 600000-ms native ceiling.
2. One application-owned bounded set of jobs survives normal calling-turn
   completion, idle/busy periods and ordinary Location viewing changes. Each
   invocation freezes cwd descriptors, selected shell/environment, source session,
   operation, turn, Location/generation and agent/model provenance. No model polling.
3. The existing Shell supervisor retains 1 MiB per stream, truncation, exit/signal,
   timeout/cancellation and TERM/KILL/wait/reap. Explicit session/job cancellation
   and application shutdown drain groups; cleanup failure remains non-success.
4. Freeze terminal facts durably before committing one automatic session-history
   notice per stable delivery identity. Publish typed core events only after commit.
   Next provider causality boundary receives those same committed facts, without
   duplicating the original function-call/result graph or issuing idle generations.
5. Reopen delivers committed results once without execution. Interrupted admitted/
   running jobs become honest unknown, never completed/replayed. Persisted PID alone
   grants no signal authority: recovery uses boot/start identity and owned group facts.
6. Existing alias Deny/Ask/Always, command-grant isolation, parent-child ceilings,
   no-follow pinned cwd, credential-free environment and pre-effect validation stay
   authoritative. Invalid/denied/unapproved calls create no job or execution intent.

## Pinned donor / declared differences

OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`:
`packages/core/src/tool/plugin/shell.ts:22–99,153–278`, `job.ts:359–465`,
`shell/result.ts:5–59`, `session/shell.ts:10–26`.
Donor job background records carry recovery and notification IDs; notification
uses session synthetic admission and clears recovery only after that admission.
Job cancellation closes its owned scope; it is distinct from a normal tool return.
Native shutdown owns cleanup rather than creating a detached daemon. Linux trust,
minimal child env, bounded retention and positive timeout ceiling remain differences.
R8 session movement is not implemented or qualified here; original job provenance
is immutable and available to its future consumer.

## Smallest persistent change envelope (justified before schema editing)

`tool_operations` is the existing call/result owner, but its completion API requires
the originating turn to remain started. Updating it later would rewrite the already
committed running call result and corrupt immutable provider continuation. `events`
and `messages` are existing notice/history owners but have no atomic job phase,
process ownership or delivered-message fence; arbitrary unversioned prefs/blobs would
not supply that seam. Therefore add one versioned shell-job table, keyed/FK'd by
existing tool operation, with immutable versioned provenance, admitted/running/
terminal phase, verified process identity, bounded typed outcome and nullable
delivered message ID. Outcome and history-delivery transactions are short and never
span execution/await. Reuse the existing SQLite connection/data-root lock, not a
second DB/store. A narrow shared internal storage handle may retain that same owner
for completion workers. One existing Shell supervisor executes every job; one
application job owner bounds and joins workers. Typed core events/queries reuse the
application inbox and broadcast channel. No dependencies, generic broker or registry.

## Evidence and results

Qualified against base `592eb1887f15aa853700ba25eea7e042cdaf3d0e` plus the reviewed
uncommitted paths below. Initial actual RED used the delivered foreground debug ELF
`dc366d68fa5ea405ef56fcb19f384dbbec1a1f0a66cd8f4d933a41990c45d29e`:
`python3 evidence/T50/native_background.py target/debug/oc` failed with
`background schema absent`. The obligations and persistent-change justification
above were written before that RED and before schema editing.

### Atomic results

| Obligation | Result | Observed evidence |
| --- | --- | --- |
| 1. Durable real launch / canonical contract | PASS | Captured root catalog exposes only canonical `shell`, including Boolean `background`. Running call result carries the same existing operation/job ID after OS launch and durable PID freeze. Headless shutdown freezes cancelled result, commits one notice and reaps its leader. Fault injected at the running-phase write returns failed, never fake running; cleanup exit is 1. Foreground false waits at a FIFO execution barrier with zero jobs/notices. |
| 2. Independent application ownership / immutable provenance | PASS | Calling turn is durably completed while the job remains running at its barrier. Next prompt reaches a held real fake-provider response; completion commits during that busy response without another generation. Parked source receives its own notice while a different real session receives none. Frozen selected-shell, cwd, source Location/generation, session/turn/operation, agent/model/provider fields are inspected. |
| 3. Bounded supervision / cancellation / cleanup | PASS | Default and explicit-zero background keep cancellation. Pressure producer reaches its file barrier only after 2 MiB on each stream: retained raw output is exactly 1 MiB per stream with both truncation flags. Explicit positive timeout and idle UI cancellation freeze their real states and reap leaders. Normal leader exit closes a TERM-resistant redirected descendant's owned group; descendant is no longer running (no invented non-child reap). Eight reserved jobs are admitted; ninth creates no execution intent. |
| 4. Atomic terminal facts / automatic notice / provider boundary | PASS | Terminal outcome precedes history admission. Same transaction commits stable `shell-notice:<operation>`, one source message ID and one notice event. Held busy response causes zero idle generations; next causality boundary and subsequent prompt each contain that delivery identity once. Original function-call running result is unchanged. Core retained-output query and card consumer read separate bounded terminal output. |
| 5. Restart / no unknown-effect replay | PASS | Actual before-fork barrier crashes with admitted job, no process identity and zero effect; restart produces unknown plus one notice without forking command. Running crash preserves append-one effect and reports unknown, never completed/replayed. Committed terminal result before notice publication reopens with original completed output/exit, publishes once and remains once on second reopen. Tampered start ticks yield unverified ownership and no signal; fixture independently verifies and cleans its original owned group. |
| 6. Admission / permissions / grants / child ceiling | PASS | Alias and split-key Deny, Ask without consumer, invalid null Boolean and escaping cwd all have zero jobs, zero started intents and zero effects. Auto approval is Once with zero grants. Real Plan/General/Explore routes retain effective ceilings, no background jobs or grants. Existing foreground command/argv saved-scope isolation and no-follow pinned-cwd suites pass. Real application Always test saves one exact command grant, reopens, runs a second job without another ask, refuses foreign cancellation/output, and keeps each immutable running result separate. |

### Final direct ELF checks

Normal debug and release builds/help completed **after** the final source/test
changes. No Cargo invocation occurred between those builds and the following
direct ELF checks. Only the evidence fixture/report/map were subsequently tightened.

| ELF | SHA256 | Background | Foreground current mode | T54 retry |
| --- | --- | --- | --- | --- |
| `target/debug/oc` | `8db45574fdd0242da8974b7ac151f28a72713a43a941abc6b04990cfe12c9bdd` | 23 PASS | 21 PASS | 9 PASS |
| `target/release/oc` | `caf76f4ef74b4202df570c4befb11349c2415e0dba9913f15ec68d2bfa54d5e9` | 23 PASS | 21 PASS | 9 PASS |

Per ELF, background cases record **52 main/child requests**, **21 admitted jobs**,
**21 terminal outcomes** and **21 stable automatic notices**; the rejected child
routes have journal failure rows but zero execution intents/jobs. Individual
effect/barrier/reap receipts are in the checker. All native background fixtures
save zero grants; the distinct real-application Always suite saves exactly one.
Foreground current-mode cases record **45 main/child requests**, **23 terminal
operation rows**, **zero jobs/grants**. T54 preserves socket/dispatch accounting,
retry/span facts and one committed legacy-argv effect across retry; no effect replay.
Auxiliary title requests are separately classified, not counted as main requests.

Commands (isolated owned HOME/XDG/project/data, synthetic keys/traps, loopback only):

```sh
python3 evidence/T50/native_background.py target/debug/oc
python3 evidence/T50/native_foreground.py --background-supported target/debug/oc
python3 evidence/T54/native_runtime.py target/debug/oc
# same three commands with target/release/oc
```

`native_foreground.py` retains historical default mode; its explicit current-mode
flag tests the qualified Boolean field instead of rewriting the historical
foreground report. Fixture-only `fork_barrier.c` is built with existing host `cc`
in the owned temporary root to stop before an actual fork and record a faulted
launch PID. Preload/fault variables are absent from the filtered shell child env.
PTY capture is bounded at 16 KiB; no screenshot/visual campaign or live call.

### Workspace / structural gates

- `cargo fmt --all --check`: PASS.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS.
- `cargo test --workspace --locked --no-fail-fast`: **1329 passed / 0 failed /
  10 existing ignored**, including real native MCP/configuration/cleanup,
  CFG09/CFG10/UI07, subagent, DCP and retry targets.
- `cargo build -p oc --locked`, debug `--help`,
  `cargo build -p oc --release --locked`, release `--help`: PASS.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: **47 PASS**.
- `python3 scripts/check_docs.py`, `python3 scripts/progress.py check`: PASS
  (read-only structural checks, not task acceptance).
- `python3 scripts/code_size.py --base 592eb1887f15aa853700ba25eea7e042cdaf3d0e --changed`
  and `git diff --check`: PASS; no touched Rust file exceeds 5000 physical lines.

All Cargo gates ran serially with `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`,
`CARGO_NET_OFFLINE=true`, approved `TMPDIR=.../oc-test-bench-20260924`, non-root UID
1003 and 900000-ms command bound. Cold workspace compilation plus tests hit that
outer bound (no product/test deadline changed). Completed warm run found only two
old exact migration-version lists `[1,3,4]`; they now assert `[1,3,4,5]` for the
declared T50 migration. Targeted migration tests and the final full suite pass.
Earlier native-fixture failures were diagnosed: reopened synthetic title readiness,
idle cancellation's real two-press state and Ask-without-consumer exit/request
accounting. Actual provider-history duplicate RED was fixed at the DCP journal
projection's missing captured-notice references, then busy and next-turn checks
passed. No assertion/deadline/ignore/resource-baseline suppression.

Logs under `/home/opencode/.cache/opencode-tmp/opencode/` (each below 16 MiB):

- `t50-background-workspace-20260930.log` — cold interruption.
- `t50-background-workspace-warm-20260930.log` — two version-list failures.
- `t50-background-workspace-final-20260930.log` — full green.
- `t50-background-{native,foreground,retry}-{debug,release}-final-20260930.log`
  — current direct ELF receipts (six files).

Final owned background log allocation is 400 KiB total. All supplied ELF checkers
have exited, loopback peers are closed, owned temporary roots are removed and no
owned Cargo/native fixture/process-group work remains. Final Git check still shows
base `592eb1887f15aa853700ba25eea7e042cdaf3d0e`; protected journal/spec/GOAL/planning/
Cargo manifest/lock paths have no diff.

### Reviewed source association / remaining scope

New owners: `crates/oc-adapters/src/shell/jobs.rs` (411 lines),
`crates/oc-adapters/src/storage_shell_jobs.rs` (222),
`crates/oc-adapters/tests/fixtures/background_lifecycle.rs` (226). Migration 5 adds
only the justified `shell_jobs` table/index and its migration receipt. Version-1
typed provenance/process/outcome use the existing connection and data-root lock;
no second store, dependency, crate, registry or command runner.

Other reviewed changed Rust paths:
`oc-adapters/src/{application,runtime,shell,storage,storage_dcp_view,tools}.rs`,
`oc-adapters/src/runtime/{context,turn}.rs`,
`oc-adapters/src/tools/{shell_call,tests/shell}.rs`,
`oc-adapters/src/storage/tests.rs`,
`oc-adapters/tests/{runtime,fixtures/approval_lifecycle,runtime/turns}.rs`,
`oc-core/src/{core_app,queries}.rs`, `oc-tui/src/app/{input,live}.rs`,
`oc/src/tui_cmd.rs` (all under `crates/`). Other owned paths are `docs/CODE_MAP.md`,
this report, `evidence/T50/{native_background.py,fork_barrier.c,native_foreground.py}`.

Native differences retained: Linux validated inherited compatible shell selection,
root-relative/no-follow/pinned workdir, credential-free allowlist, named positive
timeout ceiling 600000 ms, 1 MiB raw retention per stream and eight active job slots.
Foreground defaults 120000 ms; background defaults 0; hidden legacy argv defaults
30000 ms and is never joined/reinterpreted. Zero removes only execution timeout.
Shutdown terminates/joins owned groups; this is not a detached daemon. Missing or
unverified recovered leader identity never authorizes signalling another PID.
Recovered non-child leaders are not claimed reaped; interrupted execution remains
unknown even after verified quarantine. Delivery deduplication is not exactly-once
external command execution. Retained-output query is scoped to exact source session.

This qualifies the frozen background R2/TOOL13 slice and truthful canonical shell
schema. Full R1 selected-tool inventory and R3–R8/full T50 remain pending; R8 move
has no implementation/proof here. T44 stays PAUSED; no visual/full-GOAL claim.
No stage/commit/push/progress/spec/GOAL/acceptance changes; inherited `.opencode/`
was never inspected or edited. Ownership release is recorded in the final handoff.
