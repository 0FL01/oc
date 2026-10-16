# T45/R9 eligible-child DCP — frozen atomic

## Coordinator qualification — 2026-10-05

The eligible-child atomic is independently qualified. This supersedes the
historical handoff wording below, not its frozen requirements or failed receipts.
Reviewed source/configuration, shared permission ceilings, own-session direct
queries and captured child previews, plus the shared synchronous allocator
refresh/measure/commit boundary. The scoped read-only review found no concrete
introduced counterexample; that review alone is not runtime qualification.

Fresh parent checks exited zero: six DCP10 owner scenarios and three subagent
integration scenarios, workspace fmt check, strict locked all-target Clippy and
normal locked build. After that last build, both ordinary debug/release binaries
passed all twelve native cases each, with no intervening Cargo command.
The 24 cases made 210 synthetic loopback POSTs (186 main, 24 title), committed
14 child blocks and two root-only blocks, used six genuine Once approvals,
saved zero grants and admitted zero file/shell/write effects. Own-model 40%/55%
thresholds, unchanged parent/spectator RAW, actual continuation after reopen,
child-only false and global/tool/manual/central/profile/parent refusals passed.
Each owned native process and HTTP/PTY worker joined before exact TempDir cleanup.

Before/after retained normal ELF SHA256 values remained identical:

- Debug: `073a84304c3d2ef86127da6c275264d47599ccc06286a7217447c0833ac64558`.
- Release: `830d20195daaf5f85a245c07d77508c3e1824076ccf1bea0a444251d5552eeb2`.

These artifacts associate the base below with the reviewed dirty implementation,
not an unmodified base checkout. The current full source gate remains 1536 passed,
zero failed and ten unchanged opt-in ignores; no Rust changed after that gate.
Parent Python47, docs/progress structural checks and diff check passed too.

Earlier exploratory cleanup identity gaps and the historical broad defaults
helper's unsupported full-PASS claim remain explicitly documented below.
This is not whole T45, donor3.2, selected-task/pack forgetting or visual readiness.
T44 stays PAUSED and the exhausted T27 ledger is untouched; no paid/live or
authoring-configuration access occurred.

Base: `6055e1c61ffef93f7391476a4fee25400f0baf17`, branch
`agent/oc-rust-port`, active T45/latest0015. Initial tracked tree clean;
inherited untracked `.opencode/` excluded from all inspection and mutations.

## Frozen outcomes and constraints (before source edits)

Default `experimental.allowSubAgents=true`; explicit false suppresses child
compression schemas, managed guidance, compression annotations, nudges and new
automatic strategies only. Independent stable text IDs remain available. Root
explicit typed manual admission, global/tool switches, genuine central/profile/
parent Deny/Ask, successful-only cooldown, native 40%/55%/false accounting and
model-budget fallback remain authoritative. Built-in Explore's existing narrow
compress permission must use PermissionRules and keep its native readonly ceiling.
General, Explore and custom own-model children use their own task/history/model;
compression changes only the child's committed HOT projection, never RAW, parent
or sibling history/accounting. Causal tool-call/results and durable reopen without
replay are required, with real Ask consumer and no-consumer pre-effect refusal.

Expected seams: `dcp_auto.rs` config/default/gate; `runtime/turn.rs` captured child
lane and subagent preview; existing `permissions.rs::explore_defaults`; existing
runtime/context/query direct-entry policy. No new storage/schema/dependencies or
framework. Behavior RED precedes implementation; substantive permanent tests live
in separate nearest owner packs. Serial offline Cargo, full workspace gates,
normal debug/release builds and direct normal-ELF owned loopback fixtures follow.
Every receipt associates BASE plus current dirty paths; failed logs preserved
losslessly in approved cache (raw <=16MiB each, total new evidence <=1MiB).

Sources: spec R9 lines110–125; `docs/DCP.md` frozen controls, child contract and
CTX02 independent IDs. Compiled donor remains DCP3.1.15
`11f6517780a502512a3467645074be447cb0369e`, AGPL-3.0-or-later. Native child default
true is owner policy, not donor default/parity. Full3.2 upgrade, task/pack renewal,
host/whole-past qualification and other scopes remain NEXT. No paid/live calls,
progress/contract/planning changes, commit/stage/push or T44 resume.

## Qualification

Implemented and qualified for this eligible-child atomic; the failed exploratory
fixture limitations below remain explicit. This is not whole-T45/Goal readiness.

| Obligation | Result / permanent proof |
|---|---|
| Default true, strict false child-only | 2 config owner tests; 8-case actual child gate matrix; native false child refusal **and real root commit** |
| Switches, manual, Deny/Ask and parent/profile ceilings win | Actual issuing-request schema/refusal pairs, no pending consumer/grants/blocks; genuine typed Once consumer; native Plan parent and explicit profile/wildcard refusals |
| Explore narrow grant + readonly ceiling | Owner preview/policy pack, actual seeded builtin Explore with/without central compress authority; no file/shell/question/delegation schema |
| Preview/query/schema/dispatch/IDs coherent | 4 runtime owner tests; independent source IDs when annotations off; direct child query/API cannot borrow root authority |
| Own models/history, causal results, RAW/HOT isolation and reopen | 3 actual-child integration tests (default is 3-worker multi-thread); native General/Explore/custom + spectator, own thresholds, causal pairs, immutable RAW prefixes, own blocks, fresh-process continuation/no replay |
| Concurrent allocator risk | RED in multi-thread integration and real normal ELF; refreshed identical approved selection, shared family synchronous commit guard; DB CAS/graph/gain/security fences retained |
| Existing controls/budgets | Full workspace, 12 native controls cases, 4 native known/missing-budget cases; 40%/55%/false, typed root manual and successful-only cadence retained |

### Source association and changed paths

Final gates from `parallel-green` onward refer to BASE above plus this reviewed,
uncommitted DIRTY tree. `nearest2` and `parallel-red` precede the concurrent-family
commit fix; their receipts retain the captured earlier dirty paths. Final full
workspace includes every nearest target again. No source changes after final workspace gate/builds; only this report and
offline Python fixture refinements. No source hash manifest, storage/schema,
dependency, crate, provider/model routing or framework added.

Changed existing paths (prefix `crates/oc-adapters/`):

- `src/composition.rs`
- `src/config.rs`
- `src/dcp.rs`
- `src/dcp_auto.rs`
- `src/permissions.rs`
- `src/runtime.rs`
- `src/runtime/builtin_tests.rs`
- `src/runtime/children.rs`
- `src/runtime/context.rs`
- `src/runtime/controls_tests.rs`
- `src/runtime/turn.rs`
- `tests/subagent.rs`

New paths:

- `crates/oc-adapters/src/dcp_auto/child_tests.rs`
- `crates/oc-adapters/src/runtime/child_dcp_tests.rs`
- `crates/oc-adapters/tests/fixtures/child_dcp.rs`
- `evidence/T45/child-dcp.md`
- `evidence/T45/run_child_dcp_check.py`
- `evidence/T45/native_child_dcp.py`

The family guard is shared into existing child runtimes, while stats, nudges,
history and accounting remain child-local. No provider IO or approval wait holds
the guard. Approval equivalence excludes only globally allocated new IDs and
derived measurement numbers; addressed coverage, summaries, HOT/protections,
revision and graph remain compared. Gain is remeasured using the captured child
lane and the adopted plan; final storage CAS remains unchanged.

### Exact serial commands and full logs

Log directory **B**:
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Every row is invoked as
`python3 evidence/T45/run_child_dcp_check.py <name> <command>`;
full combined output and command/exit/BASE/dirty paths/counts/owner receipt are in
`B/t45-child-dcp-<name>.log.gz`, immutable lossless gzip. Environment:
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 CARGO_NET_OFFLINE=true TMPDIR=B`.
Tool timeout 1800000 ms; owned `timeout --kill-after=2s 1798s` is operational only.

| Name | Exact command after runner | Exit / counts |
|---|---|---|
| nearest2 | `cargo test -p oc-adapters --locked --lib --test subagent --test storage_lock --test dcp_atomic --test permissions` | 0; 567/0 |
| parallel-red | `cargo test -p oc-adapters --test subagent --locked dcp10_actual_own_model` | 101; 0/1 |
| parallel-green | `cargo test -p oc-adapters --test subagent --lib --locked dcp10_` | 0; 6 owner + 3 integration |
| fmt2 | `cargo fmt --all` | 0 |
| clippy3 | `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| no-run2 | `cargo test --workspace --locked --no-run` | 0 |
| workspace2 | `cargo test --workspace --locked --no-fail-fast` | 0; **1536 passed / 0 failed / 10 ignored** |
| debug2 | `cargo build --locked` | 0; normal debug |
| release2 | `cargo build --release --locked` | 0; normal release |
| native-final | `python3 evidence/T45/native_child_dcp.py target/debug/oc target/release/oc` | 0; 24 child cases + 2 isolated help |
| controls-debug | `python3 evidence/T45/native_dcp_controls.py target/debug/oc` | 0; 6 cases |
| controls-release | `python3 evidence/T45/native_dcp_controls.py target/release/oc` | 0; 6 cases |
| budgets | `python3 evidence/T45/native_child_dcp.py --budgets-only target/debug/oc target/release/oc` | 0; 4 budget cases + help |
| python2 | `env PYTHONPATH=scripts python3 -m unittest test_progress test_check_docs test_code_size test_bounded_live` | 0; 47/0 |
| docs | `python3 scripts/check_docs.py` | 0 |
| progress | `python3 scripts/progress.py check` | 0; read-only check |
| size | `python3 scripts/code_size.py --base 6055e1c61ffef93f7391476a4fee25400f0baf17 --changed` | 0; advisory, largest changed owner 4597 lines |
| diff | `git diff --check` | 0 |

### Normal ELF fingerprints and physical facts

After LAST normal builds, before/after successful native qualification and final
independent fingerprint: identical, **no intervening Cargo**.

- `target/debug/oc`: `073a84304c3d2ef86127da6c275264d47599ccc06286a7217447c0833ac64558`
- `target/release/oc`: `830d20195daaf5f85a245c07d77508c3e1824076ccf1bea0a444251d5552eeb2`

Each profile's final 12-case child matrix has **105 physical POSTs = 93 main +
12 title auxiliaries** (both profiles **210=186+24**). Default and real Ask each:
23=22+1 POSTs, root 6 requests; General/Explore/custom 5 each; spectator 1.
Each compressing child commits 1 own block, 1 read and 1 compress; root/spectator
blocks 0; spectator RAW exact unchanged; all RAW prefixes immutable. Fresh native
process continues retained summary, old assistant sentinel absent, read producer
call/result pair remains matched, no additional read/compress effects. Ask has 3
genuine approvals per profile and 0 saved grants. Explore absent-central case:
12=11+1 POSTs, 1 own block/read/compress. Child false: 7=6+1 POSTs, child stale
compress paired failed/0 blocks, root genuine compress completed/1 block. Remaining
8 refusals each 5=4+1 POSTs, 0 blocks/grants/new started compression effects.
All model-turn request facts check context/min/max respectively:
General 200000/80000/110000, Explore 300000/120000/165000,
custom 400000/160000/220000, spectator 500000/200000/275000.
No model file/shell/write effects are admitted by these fixtures.

### Failed hypotheses, preserved logs and limitations

All failed complete outputs retained under the same B/prefix:

- `red`, `policy-red`, `explore-grant-red`: old unconditional child opt-out,
  root-only direct query authority, and absent Explore authority demonstrated RED;
  nearest final owner pack GREEN.
- `green1`, `green-owner`, `green-child`: test compilation assumptions (String
  description, metadata has no id, private block-count API); corrected tests using
  existing interfaces, no test-only production API.
- `green-child2`: mistaken enabled-ID label assumption caused handler panic/poison
  and destructor SIGABRT. Full 27330-byte output retained. Joined peer drops and
  10-second turn watchdog added; source IDs checked using actual enabled anchors.
- `green-child3`, `child-diagnostic`: selected producers intentionally remain
  causal HOT facts under existing behavior. Test now proves old assistant text
  disappears and retained read graph is whole, not whole-past/task-pack forgetting.
- `child-matrix`: real Ask plans were incorrectly bound to shared allocator IDs;
  approval equivalence refresh fixed it without relaxing own-session selection.
- `native`, `native2`, `parallel-red`: real multi-thread siblings shared allocator
  race, including started operations/failed child before fix. Native first failure
  also exposed missing headless failure-screen initialization in fixture cleanup.
  Family synchronous commit guard + refresh fix; final multi-thread and both normal
  native profiles GREEN. No unknown effect replay performed.
- `native3`: reopen count expectation omitted parent follow-up (actual 5, not 4).
  `native4`: Ask headless reopen lacks consumer, so schema legitimately unavailable;
  real consumer PTY reopen used, all owned children reaped before cleanup.
- `nearest`: nonexistent storage Cargo target (correct target storage_lock).
  `clippy`: test modulo lint; idiomatic is_multiple_of fix.
  `python`: suites need PYTHONPATH=scripts; corrected invocation passes 47.
- `defaults-debug`: existing broad defaults helper passes 4 baseline cases, then
  its explicit-numeric-buffer restart expectation fails under already-delivered
  source-message cadence. Kept helper/assertion unchanged; **not a full defaults
  helper PASS**. Known/missing budget and typed controls are independently green on
  both current ELFs. Reconcile that historical helper with pinned source-derived
  cadence in its owner scope; do not weaken runtime/baseline to satisfy it.

Successful final child matrix records **36 native owners reaped**, **24 HTTP/PTY
owner cleanup receipts**, all handlers non-daemon joined, worker/readers joined,
then exact own TempDir removed; independent /proc check finds 0 known live owners.
Controls and budget fixtures likewise report exact cleanup. Rust peer owners join
all handlers. Runner watchdogs all reaped. No foreign prune or retained Cargo trees.
Audit limitation: the exploratory Rust SIGABRT and first native diagnostic failure
did not record exact temporary identities, so their exact cleanup order is not
independently reconstructible; no guessed/foreign directory deletion was performed.
All identified native failure owners subsequently have explicit cleanup receipts.

Before final diff receipt: 48 new gzip logs, 137727 retained bytes; max full raw
log 142628 bytes (<16MiB). Reports/helpers/logs together remain well below 1MiB.

### Exact remaining scope / authority

Full DCP3.2 donor upgrade, renewable active task/context packs within the same task,
host/whole-past/selected-producer forgetting, remaining prompt/context/capability
and other T45 scopes remain NEXT. T44 PAUSED; exhausted T27 ledger untouched.
No paid/live calls, authoring/user config or credentials inspected, inherited
`.opencode/` untouched, no GOAL/spec/planning/acceptance/progress mutation,
stage/commit/push or whole-T45/READY claim.

Temporary mutation/Cargo/owned-offline-fixture authority is **RELEASED** at handoff.
