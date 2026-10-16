# T45 R3/SUB01 — frozen foreground atomic

## Coordinator qualification — 2026-10-04

The frozen foreground atomic is independently reviewed and verified. The original
pre-edit table and implementation receipts below are retained as historical evidence.

- Reviewed scoped concurrent futures, admission before execution, original result
  ordering, duplicate explicit-child serialization, child-local cancellation,
  original session/turn event identity, mutation preimages and durable outcomes.
- A proposed rejection counterexample was withdrawn: plain `Rejected(None)` must
  interrupt the parent under CONTRACTS; feedback refusal and an admitted child's
  Question dismissal are distinct and retain their existing scoped semantics.
- Fresh complete adapter subagent target: **19 passed**. Workspace fmt, strict
  locked all-target Clippy and normal locked build passed. No source change
  followed the recorded **1500/0/10** complete workspace gate.
- After that last build, both normal ELFs passed the reverse-completion and actual
  shell-leaf cancellation cases: **four native cases**. Each reverse case observed
  six physical/durable dispatches (parent2, maker2, reader1, title1), both children
  before barrier release, original call-ID result order, three completed turns,
  two delegation operations and one patch/effect with exact `x\n` bytes.
- Each cancellation case observed four dispatches, three cancelled turns, two
  cancelled delegations and one cancelled shell operation. The exact owned leaf
  PGID/startticks was reaped; no later effect or reopen dispatch occurred.
- Native groups, HTTP handlers and PTY readers joined before temporary cleanup.
  Normal fingerprints stayed identical before/after all direct proofs:
  debug `11e3b0be7f973cada9cb1f0e88a139a9359a3134a6867a6664822ec2286ed16b`;
  release `bdf82f90a833db41ec26f057988912b98b9e2bc102fdd4fa8073dfe8f25d22c4`.
- Python **47 passed**; docs, progress structure and diff checks passed. No Cargo
  intervened during direct ELF qualification.

Association is base `21893da4b87f7d60575e56860bfdd18181d20a47` plus the reviewed
implementation, not a clean-base artifact claim. No paid/live API, real user HOME,
authoring configuration or exhausted campaign was accessed. Background children,
family controls/navigation, child DCP, packs, host and donor upgrade remain required
separate work. This is not complete T45/SUB01/SUB02, T44/V09 or product READY.

Frozen before source edits, base `21893da4b87f7d60575e56860bfdd18181d20a47`.
Scope: closed-response concurrent foreground child calls, existing Runtime/Db owner.
Qualification is offline; this atomic does not complete T45/SUB01/SUB02.

| Scenario | Required observable outcome | Initial status |
| --- | --- | --- |
| Distinct fresh children | Both actual child provider requests dispatch before either independent barrier is released | PENDING |
| Foreground batch join | No next parent request until every admitted foreground child settles; reverse completion still emits original call order with exact call IDs and durable outcomes | PENDING |
| Fresh context/model | Own profile/workspace/delegated prompt only; no parent transcript/system; override → profile → parent model precedence, request schemas/guidance and ceilings captured | PENDING |
| Explicit continuation | Own child history/selection preserved; duplicate same-child calls serialize in original order while distinct children can progress | PENDING |
| Independent failure | One child error does not cancel parent or sibling; terminal outcomes remain truthful | PENDING |
| Mutation | One deliberate custom-profile effect only after policy/preimage/durable intent; existing shared guards remain authoritative | PENDING |
| Admission controls | Deny/Ask/depth/invalid budget/model/closed response remain pre-effect guards | PENDING |
| Cancellation/shutdown | Cancel propagates to admitted children; owner settles/joins provider/tool/process work before return; no later effects | PENDING |
| Reopen | Exact durable child rows/turns/outcomes; no child/tool replay | PENDING |
| Background | Foreground stays default; background:true remains explicit Unsupported with no child admission | PENDING |

Pinned comparison: OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`,
`packages/core/src/session/runner/step.ts:100–145` (scoped fibers/batch join),
`tool/plugin/subagent.ts:29–62,117–265` (fresh/continuation/model/launch).
Native closed-response admission, parent-child narrowing, finite retry, MCP quarantine,
resource guards and no unknown-effect replay remain authoritative.
Background jobs/notices/recovery/Ctrl+B, family controls/navigation, T50 shell, T56
terminals and T44 visuals are separate follow-ups.

## Qualification — 2026-10-04

**Frozen foreground atomic verified; implementation remains unstaged for parent review.**
The original pre-edit table above is retained unchanged. This qualifies the next
bounded foreground atomic, not all T45, SUB01/SUB02 or product readiness.

| Frozen scenario | Result and exact proof |
|---|---|
| Distinct fresh children | PASS: actual independent HTTP peers both arrive before either barrier releases, in the owner test and both normal ELFs. |
| Foreground batch join | PASS: release B first and observe its durable completion; parent request count stays one until A settles. Results remain `original-call-A`, `original-call-B`, with exact child IDs and durable outcomes. |
| Fresh context/model | PASS: physical requests carry own profile/workspace/task, no parent transcript/system; maker uses profile model `fixture/gpt-child-a`, reader override `fixture/child-b` precedes decoy profile. Captured schemas include patch only for maker and no mutation/shell for reader. Existing precedence/parent-ceiling regressions pass. |
| Explicit continuation | PASS: duplicate calls for one explicit session serialize; second physical request contains the first committed child answer and its own model/history. No new child row. Existing continuation regression also passes. |
| Independent failure | PASS: genuine child Question dismissal cancels that child only; sibling remains held and parent token stays false. Invalid model/budget child fails while valid sibling completes. |
| Mutation | PASS: native admitted custom maker creates exact `x\n` once, with one completed patch op. Owner test proves sibling mutation invalidates approved patch preimage, preserving exact `before\nforeign\n` and a failed patch outcome. |
| Admission controls | PASS: Deny/Ask/depth/model/budget/background and unclosed-response owner cases, plus complete existing regression suite. Ask without consumer has zero group intents. Unclosed response exhausts 11 existing finite attempts as Incomplete with zero child rows/intents. Tiny-budget child may retain its pre-existing fresh session row but has no accepted child turn/provider/tool effect. |
| Cancellation/shutdown | PASS: real Ctrl+C settles all three accepted turns as cancelled and both delegation ops plus actual shell op as cancelled; actual owned leaf PGID/startticks is reaped before cleanup. Physical count stays four, with no parent follow-up/later effects. |
| Reopen | PASS: exact children/turns/operations and counted generation dispatches remain identical after no-input reopen in both completion and cancel cases. |
| Background | PASS: default foreground remains; explicit background true is Unsupported with zero child admission. No background-subagent functionality claimed. |

### Implementation and source association

Branch `agent/oc-rust-port`, unchanged HEAD/base
`21893da4b87f7d60575e56860bfdd18181d20a47` **plus DIRTY reviewed source**.
All final source gates/build/native receipts carry:

`c85652e3d602cf9d9aa6be3d57e467e94833337215b200e383bcbfd2935ad796`.

`run_foreground_check.py::source` hashes sorted tracked `crates` paths plus the
two new Rust owner files as path NUL contents NUL. Documentation edits are not
compiled source. Current HEAD/status/diff were checked before native proof;
there were no concurrent owner changes visible. No clean-base artifact claim.

- `runtime/turn/foreground.rs` (244 lines): private two-pass ordered common
  admission and durable intents before child effects; borrowed scoped futures,
  `join_all` without short-circuit and same-explicit-child predecessor chain;
  original-order common redaction/artifact/output/durable publication.
- `runtime/turn.rs`: adjacent closed-response delegation groups reuse the
  original executor owner; ordinary tools preserve serial barriers. Child turns
  bypass the parent-held global single-flight lease, with local cancellation
  propagated from parent while joining. Existing child events carry original
  session/turn IDs; no family/navigation framework or additional Runtime/Db.
- `application.rs`: extracted shared root/child tool-event projection, retaining
  durable presentation publication.
- `tests/subagent.rs` includes separate 725-line
  `tests/fixtures/foreground_children.rs` under the existing Cargo target.
  Five meaningful scenarios include actual independent provider barriers,
  duplicate continuation, local cancellation and root cancellation, seven
  admission negatives, and shared approved-preimage conflict.

### Normal native artifacts and measured counters

Both are normal x86-64 PIE Linux ELFs, dynamically linked; debug has debug info,
release is stripped. No Cargo command ran after the final normal release build.

| Artifact | SHA256 before/after every direct check | ELF BuildID |
|---|---|---|
| `target/debug/oc` | `11e3b0be7f973cada9cb1f0e88a139a9359a3134a6867a6664822ec2286ed16b` | `a9b94cd8eba831d6864dc0beedcaa72d8ea9668f` |
| `target/release/oc` | `bdf82f90a833db41ec26f057988912b98b9e2bc102fdd4fa8073dfe8f25d22c4` | `75db6b23fce9e451f25df139080cf596b439347c` |

`native-final` has four actual cases (reverse and cancel per artifact):

- Barrier physical POSTs **4**: parent1, child A1, child B1, title auxiliary1.
  This actual auxiliary dispatch is included honestly, even with title profile
  disabled in the synthetic configuration.
- Reverse final physical/durable dispatches **6**: parent2, maker2, reader1,
  title1. Exactly two fresh child rows; three completed turns; delegation ops2
  completed; patch op1 completed; effect2 bytes. B's durable completion precedes
  A's release, but original output call order stays A/B. Raw stdout100 bytes,
  stderr23 bytes, two JSON frames (delta1/done1), with bounded frame metadata/hash.
- Cancel final physical/durable dispatches **4**: parent1, maker1, reader1,
  title1. Exactly two fresh children; three cancelled turns; delegation ops2
  cancelled; actual shell op1 cancelled. Owned native leaf PIDs debug3553206 and
  release3553280 have PGID/startticks/reaped receipts. No later provider/tool
  effects; no replay on reopen.
- Native processes, PGIDs/startticks, HTTP handlers, PTY readers and watchdog
  owners are joined/reaped before exact TempDir cleanup. Receipts retain exact
  generated session/turn IDs, CallIds, SQL rows, physical request views and owner
  identities. Assertions compare counters and graph identities, not goal prose.

### Final gates and immutable lossless logs

Exact receipt directory:
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`.
Name `NAME` below means `t45-foreground-NAME.log.gz`. Each gzip retains full
merged output, argv, exit, HEAD/source before/after, raw byte counts, elapsed
seconds and owned runner PGID/startticks/reaped receipt; files are immutable.
All rows below exited **0**. Cargo was serial/offline/locked with jobs3,
testthreads1 and approved TMPDIR. Operational timeout1798 seconds and tool
timeout1800000 ms did not alter product/test deadlines, caps or assertions.

| Receipt | Command/scope | Result / seconds |
|---|---|---|
| fmt-check | `cargo fmt --all -- --check` | PASS / 2.819 |
| clippy-final | `cargo clippy --offline --workspace --all-targets --locked -- -D warnings` | PASS / 2.508 |
| no-run-final | `cargo test --offline --workspace --locked --no-run` | separate PASS / 0.669 |
| workspace-final | `cargo test --offline --workspace --locked --no-fail-fast` | **1500 passed / 0 failed / 10 unchanged ignored**, 42 summaries / 1112.210 |
| subagent-final | `cargo test --offline --locked -p oc-adapters --test subagent` | **19 passed** / 134.040 |
| debug-final | `cargo build --offline --locked -p oc` | normal debug / 6.932 |
| release-final | `cargo build --offline --locked --release -p oc` | normal release / 152.993 |
| debug-help, release-help | respective normal ELF `--help` | PASS |
| debug-run-help, release-run-help | respective normal ELF `run --help` | PASS; `--agent` retained |
| elf, fingerprints-before, fingerprints-after | actual ELF identity / SHA256 | unchanged artifacts |
| native-final | `native_foreground_children.py target/debug/oc target/release/oc` | **4 actual cases** / 6.637 |
| native-plan | `native_plan_lifecycle.py target/debug/oc target/release/oc` | **98 actual cases** / 107.962 |
| native-builtin | `native_builtin_profiles.py target/debug/oc target/release/oc` | **30 actual cases** / 75.557 |
| native-directed | `plan_regressions.py target/debug/oc target/release/oc` | **34 actual cases** / 38.714 |
| dcp-controls-debug, dcp-controls-release | `native_dcp_controls.py` respective ELF | **12 actual cases total** / 8.419, 5.090 |
| dcp-calculator-debug, dcp-calculator-release | `native_builtin_dcp_calculator.py` respective ELF | **16 actual cases, 56 physical POSTs total** / 11.919, 7.432 |
| python47 | `python3 -m unittest discover -s scripts -p 'test_*.py'` | **47 passed** / 11.404 |
| progress | `python3 scripts/progress.py check` | structural PASS only |
| docs, diff | documentation links / `git diff --check` | PASS |
| size | `python3 scripts/code_size.py --base 21893da4b87f7d60575e56860bfdd18181d20a47 --changed` | advisory PASS; application4681, turn4344, helper244, subagent1553, new tests725 lines |

Workspace passed counts in target order:
`88,4,1,14,4,1,1,15,41,1,16,48,34,4,1,1,4,1,1,501,11,3,4,0,3,27,21,10,4,8,120,3,2,4,1,19,13,32,434,0,0,0`.
The unchanged T54 `ret01_` finite retry/no-replay/provider/auxiliary tests all
passed in the full gate. Native-directed includes actual T50 stream/variant/Ask
captured-model requests (six cases across both ELFs), mutation7, Question3 and
Move4 per artifact. Total final direct-native regression cases: **194**.

### Development attempts retained, not qualification substitutions

Every earlier attempt has its own exact immutable receipt. Nonzero results:

| Receipt(s) | Exit | Diagnosis resolved before final qualification |
|---|---|---|
| compile1 | 101 | Scoped predecessor borrow/callback lifetime compilation errors; corrected lifetimes and dropped predecessor map after join. |
| targeted1 | 101 | Nonbinding lock lint in new fixture; bind guard. |
| targeted2 | 101 | Fixture assumed no-consumer Question meant cancelled; actual registered consumer now replies Cancelled. Worker panic during failure cleanup retained. |
| targeted3, targeted4 | 101 | Own custom prompt is a developer message, not top-level instructions/system; physical request assertions corrected. |
| subagent1 | 101 | Refused call was settled twice; refusal now persisted once in ordered terminal publication. |
| subagent2 | 101 | Unclosed stream returns a typed terminal report, not Err; fixture corrected. |
| preimage1, preimage2 | 101 | New fixture used wrong patch argument, then non-patch model; use exact patchText and actual patch-selected gpt model. |
| native-development1, native-development2, native-development3 | 1 | Actual title auxiliary needed explicit barrier/counter inclusion; now physical and durable totals agree. |
| native-development4 | 1 | Actual native leaf call is canonical shell rather than bash; SQL assertion corrected. |
| workspace | 101 | **1499/1/10**: new unclosed-stream assertion expected Failed instead of actual Incomplete; corrected to exact existing typed contract. Subagent19 and full1500/0/10 reruns pass. |

Other developmental receipts (all exit0) are `compile2`, `targeted5`, `format`,
`format-final`, `preimage3`, `debug-development`, `native-development5`, `clippy`,
`no-run`. Each receipt carries its historical source association; only final
current-association rows qualify the shipped artifacts. Lossless receipts and
new repository evidence remain well below 1 MiB; no extra fixture/target tree.
Exact retained totals: **50 immutable gzip receipts / 193242 bytes**; fourteen
nonzero developmental attempts remain included, with diagnoses above. The three
new repository evidence files add less than 32 KiB (combined total below 225 KiB).
Final owner audit found zero matching runner PID/startticks still alive.

### Scope and handoff

Scheduling is deliberately narrow: adjacent delegation groups in one closed
provider response, with ordinary-tool ordering barriers retained. Same-child
serialization is batch-scoped and not a permanent continuation restriction.
Shared mutation/preimage, permission/Ask, depth, budget/model, durable intent,
closed-response, MCP quarantine and no-unknown-effect-replay owners remain.
Eligible-child DCP, background jobs/notices/recovery/Ctrl+B, selected packs,
family navigation/control and other remaining T45 slices still require their
own implementation and qualification; T44 remains PAUSED.

Offline synthetic loopback only: no fresh live API claim, T27 campaign/ledger
access, real user HOME/config, authoring authentication or inherited `.opencode`
inspection/edit/stage. No staging, commit, push, progress/GOAL/spec/planning or
acceptance edits. Parent independently reviews/delivers. Temporary mutation,
Cargo and owned-offline-fixture coordination is released after the final report.
