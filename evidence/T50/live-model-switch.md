# T50/R1/TOOL12 — same-task committed model switching

## Coordinator qualification — 2026-10-01

**The frozen same-task live-switch atomic slice is verified.** Independent scoped
reviews of admission/acknowledgements and request/history preparation found no
concrete introduced counterexample. Those source reviews are not substituted for
the real consumer and native effects below. Whole T50, T45 and T44/V09 remain open.

Source association: BASE `0804b7dbb9c2dccd4e6502191ba52bb4eb85b9ed` plus the reviewed
implementation; current HEAD `d6e4d549c2451623c2729d72685dcc0acee2339b` differs from
BASE only in owner documentation before this implementation is committed. The
recorded current full workspace remains **1413 passed / 0 failed / 10 existing
ignored**, with 42 successful summaries in `t50-live-workspace.log` below. No Rust
source was changed after that complete gate.

Fresh coordinator commands all exited **0**, sequentially with the prescribed
offline jobs3/threads1/TMPDIR environment:

- `cargo test -p oc-adapters --lib --locked tool12_`: **5 passed**, including real
  busy owner commit and exact scope/no-op/atomic storage refusal.
- `cargo test -p oc-tui --lib --locked model_selection`: **2 passed**, including
  captured submission and newer drafts surviving late success/failure.
- Workspace fmt, strict locked all-target Clippy and normal `cargo build --locked`.
- After the last build, `python3 -B evidence/T50/native_live_model_switch.py ELF`
  for **both** normal debug/release ELFs: **7 + 7 passed**, each with 24 primary,
  seven title and one summary POST, 22 tool rows and no commit-induced POST.
  Stream/tool/Ask A→B→A, variant, retry, compaction and independent child barriers
  all passed; owned process/PTY/HTTP owners joined before exact temporary cleanup.
- Python **47 passed**; docs/progress structural checks and `git diff --check`.

The normal ELF fingerprints below matched before and after all direct coordinator
proofs. No Cargo intervened during those proofs. No paid request, actual user
configuration/credential inspection or exhausted campaign access occurred. The
original frozen outcomes and coordinator's detailed evidence follow unchanged.

## Frozen before RED

- Source BASE: `0804b7dbb9c2dccd4e6502191ba52bb4eb85b9ed`.
- Donor: OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`, U95–U102;
  local picker draft, captured admission commit, busy switch without wake,
  per-attempt preparation and compatible ordinary call/result projection.
- Scope: the entire mandatory live-switch atomic slice in R1 and
  `docs/TEST_PLAN.md` TOOL12. T50 remains active; T44 visual qualification remains
  paused. Shell inventory controls and broader prompt/steering work are separate.

## Required observable outcomes

1. Real session/agent-scoped picker/variant draft is not an owner commit.
   Captured normal submission/command preparation preserves admission order;
   blank Enter in an authorized ordinary existing-session composer commits while
   busy without a user row. Identical commit is a no-op. Matching scoped
   acknowledgement/event cannot erase a newer draft; failed/stale commit is not
   presented as success. Existing profile/config/Location/child/read-only guards
   remain.
2. One request-preparation boundary resolves the latest committed model/variant
   before every natural primary request, admitted retry and compact/overflow
   rebuild. Provider binding, budget, DCP thresholds, file schemas, managed
   guidance, compatible history, estimates and fingerprints agree. Configuration,
   Location and agent generation stay pinned; finite retry/cancel rules stay.
3. Already prepared A streams, calls and approval waits retain A's identity and
   advertised allowset after B commits. A's confirmed effects happen once; calls
   excluded by their own issuing view have paired failed results and no effects.
   Independent own-model child selection/authority stays isolated.
4. Retained closed model-neutral pairs and outcomes reach B, without alien opaque
   state/checkpoints, translation, archive reload or execution replay. Raw history
   stays immutable. Actual A/B/A request and assistant attribution is durable and
   consumed by history/live presentation, independently of composer selection.
5. Actual normal debug AND release ELF fixtures hold stream/tool/Ask barriers,
   prove draft-only and busy blank-Enter commit with zero extra POST, complete
   A→B→A in one working task, and cover retry/compact/restart/terminal no-wake.
   Durable graph, real file bytes, actual requests and typed receipts are oracles.

## Ownership and persistence decision

Reuse the existing application inbox/selection prefs, runtime owner and bounded
turn JSON/checkpoint/presentation records. Inspect existing per-span identity
capacity before edits; a schema migration is permitted only if those existing
records cannot truthfully encode per-request identity. No second database,
selection registry, queue/event bus, retry layer, provider routing or test-only
production API. One mutation/Cargo/owned-fixture coordinator; preserve concurrent
owner-only documents. No actual HOME/env/config/API or exhausted paid ledger.

## Execution evidence

### Result and source association

The five frozen outcomes above pass for this runtime/real-consumer atomic slice.
Source is BASE `0804b7dbb9c2dccd4e6502191ba52bb4eb85b9ed` plus the reviewed dirty
changes. Qualification HEAD was `d6e4d549c2451623c2729d72685dcc0acee2339b`;
intervening `70de7a`, `8a4291d` and `d6e4d54` were owner-only documentation plans
and were preserved. This coordinator made no commit/stage/push or task/progress/
acceptance edit. T50 remains active; this is not whole-T50, full-GOAL, T45 PRM01
or paused T44 paired-visual qualification.

Existing selection prefs plus one existing SQLite transaction store the changed
choice and `session_model_selected` fact atomically. Existing turn JSON now carries
per-attempt request receipts and per-span attribution; existing compaction snapshot
JSON carries actual auxiliary model origin. **No SQLite schema migration**, second
database/registry/event bus, execution queue, retry layer or dependency was needed.
Model-neutral projection changes only provider input, not the raw journal. Legacy
unprovable foreign native checkpoint routes retain their fail-closed guard; the
current generated-summary adapter and proven model-only opaque exclusion are covered.

### Obligation evidence

| Frozen outcome | Observed result |
| --- | --- |
| Draft, admission order and acknowledgement | Real busy picker/Ctrl+T changes only local session/agent draft. Blank Enter commits on the existing owner port with no new user row, interruption or POST; identical choice is a true no-op. Actual consumer tests retain a newer draft across older/other-caller events, pending acceptance, failure and catalog refresh. Scope/retired-choice/storage-refusal tests preserve prefs including timestamps and emit no false commit. Captured message/workspace-command choice is applied synchronously in inbox admission order. |
| Next same-task prepared view | Stream/tool/Ask flows perform A→B→A without another prompt. The next request has the committed model/variant, matching family/schema/guidance/output cap, DCP thresholds and provenance fingerprints. Admitted retry and overflow/compaction rebuild use the same preparation boundary; existing finite retry/cancel and pinned config/Location/agent authority remain. |
| Captured issuance and effects | An A patch executes once under its A issuing allowset after B commits; a B edit completes under committed A. Real pending-Ask owner tests commit while approval is held. Excluded A write/B patch calls have matched failed outputs and zero effects. Foreground held tools and independent own-model children preserve their captured view; B background-job provenance remains B after A recommits. R9 stale-preimage/grants/path/durable-fault regression fixture passes. |
| Compatible history and actual attribution | B receives closed original A call/result groups and confirmed outcomes. Alien A/B opaque items are withheld, same-model compatible opaque items remain, and raw history is unchanged. Request receipts and assistant/tool part labels record A/B/A (and B.low/B.Default separately). Restart restores committed selection with no automatic POST/tool replay, including after current file bytes change. Bounds/prune floors and public legacy command anchors remain. |
| Normal ELF qualification | Both final normal debug/release ELFs pass all seven new live-switch barrier cases plus directed adjacent-risk fixtures: 64 scenarios per ELF, 128 total, zero final failures. Terminal-only selection commit creates no generation. |

### RED and resolved failures

- `t50-live-red.log`: expected exit **101**, real busy owner commit returned
  `CoreError::TurnBusy` before the fix.
- `t50-live-provenance-red.log`: expected exit **101**, issued B background work
  incorrectly recorded initial A provenance; the existing owner test then passed
  both Allow and Ask flows after provenance used the captured request identity.
- Development compile/fixture failures were corrected: optional legacy fields,
  declared variant fixtures, reserved Default normalization, full-turn settlement
  before DB assertions, and retained variant-dialog synchronization. Final native
  chooser reads rendered cells through the existing stdlib terminal helper; actual
  wire/DB/bytes, not chooser prose, establish model selection and effects.
- Earlier workspace runs hit the unchanged **900000 ms** boundary while obsolete
  immediate-picker/busy-refusal expectations waited or inflated PTY runtime; a
  later cold compilation also consumed part of that boundary. Those incomplete
  runs were not PASS. Focused finite-provider-failure testing passed unchanged
  (11 attempts); old assertions were reconciled only where the owner explicitly
  superseded immediate commit/busy refusal/model-only fresh-lane behavior.
- Full regression testing caught and fixed public legacy-command anchor projection
  and post-output retry budget-refusal span preservation. No tests were deleted or
  newly ignored; no deadlines, caps, retry allowances, baseline or golden thresholds
  were weakened.

### Gates and exact current logs

All Cargo commands used `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1
CARGO_NET_OFFLINE=true TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`
with one Cargo owner and a 900000 ms command timeout. Log root for every basename
below is:

`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`

| Command | Final exit/result | Full log basename |
| --- | --- | --- |
| `cargo fmt --all --check` | 0 | No output; final gate before normal builds |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `t50-live-clippy.log` |
| `cargo test --workspace --locked --no-run` | 0 | `t50-live-precompile.log` |
| `cargo test --workspace --locked --no-fail-fast` | **0; 1413 passed, 0 failed, 10 existing ignored; 42 target summaries** | `t50-live-workspace.log` |
| `cargo build --locked -p oc` | 0, normal debug, 14.75 s | `t50-live-debug-build.log` |
| `cargo build --release --locked -p oc` | 0, normal release, 2 m 28 s | `t50-live-release-build.log` |
| Both direct normal ELFs `--help`, isolated synthetic HOME | 0/0 | `t50-live-debug-help.log`, `t50-live-release-help.log` |
| `PYTHONDONTWRITEBYTECODE=1 PYTHONPATH=scripts python3 -m unittest scripts.test_bounded_live scripts.test_code_size scripts.test_progress scripts.test_check_docs` | 0; **47** (13+5+15+14) | `t50-live-python.log` |
| `python3 scripts/check_docs.py`; `python3 scripts/progress.py check` | 0/0, read-only structure checks | `t50-live-docs.log`, `t50-live-progress.log` |
| `python3 scripts/code_size.py --base 0804b7dbb9c2dccd4e6502191ba52bb4eb85b9ed --changed` | 0, advisory; 46 Rust files, none above 5000 lines | `t50-live-size.log` |
| `git diff --check` | 0 | No output |

New Rust scenarios: two application owner tests and two actual TUI consumer tests,
in existing targets. Existing R9 tests retain their scenarios; old compatibility
fixtures/assertions and optional-field literals were updated without adding targets.
Targeted logs include `t50-live-tool12.log` (7 focused tests),
`t50-live-provenance.log`, `t50-live-history.log`, `t50-live-budget.log`,
`t50-live-pending.log`, `t50-live-cycle.log`, `t50-live-pty39-directed.log` and
`t50-live-var01.log`.

### Final normal ELF association and direct fixtures

| ELF | SHA256 before and after all direct qualification |
| --- | --- |
| `/home/opencode/ai/oc/target/debug/oc` | `5422c37fc19f7c5c740532cae09cfb2900e3a8816d72f40ea3d7ea8b878d568d` |
| `/home/opencode/ai/oc/target/release/oc` | `e3bfcb3eb9ac10e1a18e259f4547bfcc0d5e198064d488ea8f5347b3fd435094` |

No Cargo invocation or Rust source edit occurred between the final normal builds/
hashes and these direct fixtures. `PYTHONDONTWRITEBYTECODE=1` and the owned TMPDIR
above were used; each command exited **0** for both ELFs. `ELF` below was each of
`target/debug/oc` and `target/release/oc`; `{profile}` is `debug`/`release`.

| Direct command | Per ELF | Full log basename |
| --- | ---: | --- |
| `python3 evidence/T50/native_live_model_switch.py ELF` | **7** | `t50-live-native-{profile}.log` |
| `python3 evidence/T50/native_file_mutations.py ELF` | **22** | `t50-live-r9-{profile}.log` |
| `python3 evidence/T54/native_runtime.py ELF` | **9** | `t50-live-t54-{profile}.log` |
| `python3 evidence/T55/native_completed.py ELF --reasoning-replay` | **7** | `t50-live-t55-{profile}.log` |
| `python3 evidence/T50/native_question.py ELF` | **14** | `t50-live-question-{profile}.log` |
| `python3 evidence/T50/native_read.py ELF --case CASE` for `nested_lifecycle`, `unsupported_model`, `image_dcp`, `image_compaction` | **4** | `t50-live-read-{profile}.log` |
| `python3 evidence/T50/native_session_move.py ELF --case boundary` | **1** | `t50-live-move-{profile}.log` |
| **Total** | **64** | **128 final passing scenarios across both ELFs** |

The seven new cases are `stream`, `variant`, `tool`, `ask`, `retry`, `compact`,
`child`. Per ELF they observe **24 primary + 7 title + 1 summary = 32 actual POSTs**,
**22 actual tool rows**, and truthful root/child/auxiliary receipts. Stream restart
has a later explicit user prompt only after the original A/B/A task has completed;
the switching itself has no second user prompt. Compact has a real prior exchange
and records prepared summary B while the following rebuilt primary uses committed A.
Historical paid A09 is not relabelled as this synthetic current-family qualification.

### Reviewed ownership and remaining work

- Selection owner: `application_selection.rs`, `application.rs`, existing `storage.rs`
  transaction. Typed port/scope/receipt/request identity: `oc-core/{core_app,queries}.rs`.
- Prepared view/retry/history: `runtime/turn.rs`, `runtime/context.rs`,
  `runtime_compaction.rs`, existing compaction JSON; private `tools/model_history.rs`
  preserves original instruction indices through `instructions.rs` projection.
- Actual composer consumer: private `oc-tui/src/app/model_selection.rs`,
  `app/input.rs`, `app/live.rs`, `picker.rs`, binary `tui_cmd.rs`; persisted/live
  part attribution in storage/history/transcript. Safe retired-choice display uses
  the existing model metadata owner; raw choices remain admission inputs.
- Substantial tests are separate existing-owner modules:
  `application/live_switch_tests.rs`, `app/tests/model_selection.rs`; native fixture
  `evidence/T50/native_live_model_switch.py`. `docs/CODE_MAP.md` tracks these seams.
  Changed-path inventory is 46 Rust files plus CODE_MAP and two evidence files.
- No actual HOME/config/env/credentials/live API or exhausted paid ledger was used.
  All fake process groups, PTY readers and HTTP workers joined before exact owned
  TempDir removal; foreign processes/history were untouched. Final owned logs were
  231607 bytes before the last read-only checks/report, with largest log 129582 bytes.
  Mutation/Cargo/fake-fixture ownership is released on return.
- Remaining product work includes shell inventory controls and the other owner-plan
  slices; T44 stays PAUSED. No full-T50/GOAL/READY claim is made here.
