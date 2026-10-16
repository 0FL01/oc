# T45 R3/SUB01/SUB02 — frozen child controls atomic

## Coordinator qualification — 2026-10-05

Independently reviewed the existing Jobs/runtime lifetime, exact conversion fences,
terminal transactions, linked-view routing and family-safe move boundary. Two scoped
read-only reviews found no introduced counterexample; those are source reviews, not
runtime or visual qualification. The frozen implementation record below is preserved.

Fresh coordinator commands all exited 0: runtime child-owner tests **4**, storage
child tests **7**, owned-child terminal/move boundary **1**, and binary linked-control
tests **2**; workspace fmt, strict locked all-target Clippy and normal locked build.
The recorded complete workspace gate remains **1519 passed / 0 failed / 10 unchanged
opt-in ignored**; its Rust inputs have not changed during this review.

After the last normal build, both shipped ELFs independently passed all four actual
PTY cases: conversion, selected cancellation, selected Shell routing, and terminal
conversion/stale controls. **8 cases, 50 actual POSTs = 50 durable dispatches**, eight
auxiliary requests, 16 child operations, eight shell operations, six child mode events,
six notices and six effects. All 16 launched/reopened native processes and eight shell
leaves were reaped; PTY/HTTP workers joined before temporary cleanup. Reopen dispatched
zero RPCs. No Cargo command intervened between the direct proofs and fingerprint checks.

Normal hashes matched before/after every proof:

```text
debug   36cb0677c76192cb6e6597751efff1f0f660d964c8500ec16bfbf32f7179b325
release f97230003f0735236cec4b4e27e052e5bf3754d70b45766716a522dc422d9b90
```

Association: base `3a29b997f21c15a9ca45d08920fb2a3a7297ba86` plus the reviewed dirty
implementation, not clean-base artifacts. Python **47**, docs/progress structural
checks and reviewed diff checks passed. No real HOME/config/env, authoring credentials,
paid request or exhausted T27 allowance was accessed. Safely unfinished resumption,
command routing, child DCP, packs/host/donor and whole T45 remain separate. T44 remains
PAUSED; this functional consumer is not paired visual qualification or READY.

Frozen before RED/production edits, 2026-10-04. Base
`3a29b997f21c15a9ca45d08920fb2a3a7297ba86`, `agent/oc-rust-port`.
Existing inherited untracked `.opencode/` is excluded. T44 remains PAUSED.

| Outcome | Required observable proof | Initial |
|---|---|---|
| Owned foreground/current state | Default foreground and background use the same bounded Jobs/Db/source owner; two actual admitted child turns/physical requests before parent completes. Current mode/state differs from immutable launch. | NOT_RUN |
| Same-work conversion | Exact family/child/op/Location/generation conversion releases only its foreground waiter. Same child session, request/stream, shell PID and operation continue; no kill/restart/duplicate dispatch. Parent waits for remaining foreground work. | NOT_RUN |
| Settlement races | Conversion/interrupt/terminal repeat/races settle once with typed status, one background notice, zero unconverted-foreground notices, no effect replay. Stale/foreign identity rejected. | NOT_RUN |
| Functional consumer | Actual normal debug/release PTY opens a linked child before parent completion, receives actual text/reasoning/tool/terminal, opens real Shell, returns retaining parent draft/deck/focus and pinned Location. Selected interruption leaves sibling active. | NOT_RUN |
| Authority/source | Child new-turn/model/profile/Undo guards remain; navigation never approves/answers/cancels. Existing descendant approval/question bindings remain. Captured source A survives actual parent move B and late completion. | NOT_RUN |
| Ownership/recovery | Explicit same-child continuation serialized; aborted shutdown retains joins; own shell leaf reaped before scoped cleanup; restart displays committed state without RPC/replay. Existing admission/preimage/grants/MCP/unknown safeguards retained. | NOT_RUN |
| Full gates/artifacts | Serial offline jobs3/threads1 nearest, fmt, strict locked all-target Clippy, separate no-run/full workspace, separate normal debug/release/help; both final ELFs after last build with unchanged hashes/no Cargo between; Python47 and read-only docs/progress/diff/size. | NOT_RUN |

## Minimum lifetime and durable mode seam (before edits)

Borrowed scoped foreground execution cannot outlive a released waiter. Move its
admitted child execution into the existing owned Runtime snapshot/Jobs lifetime,
using the same shared connection/flock and captured provider/catalog/profile/MCP/
Location. Retain ordered common admission/publication and explicit same-child
predecessor joins; no competing scheduler/store/queue/dependency.

No additive schema is necessary: use the existing `child_jobs` relation for all
admitted child generations and existing `events` for background mode (initial or
conversion), like the T50 shell mode seam. The immutable identity/launch output is
never rewritten. Current queries project mode from durable events. Foreground
terminal generations neither consume undelivered-notice capacity nor produce
background notices. Mode transition and terminal publication serialize on the
existing Db mutex/transaction; conversion after terminal is refused. A retained
shared join and mode notification release a waiter without replacing actual work.

Before the current-turn DTO addition: expose the existing nullable `child_turn`
column as optional current `ChildJob.turn` alongside current mode. This is an
additive query field, not a migration or new authority. A linked viewer opened
between acceptance and the next delta can bind the real accepted turn without
creating input or waiting for another acceptance event.

Safely unfinished auto-resumption, command/subtask routing, child DCP/context packs/
host/donor upgrades and full visuals remain NEXT. Historical stronger
peer-POST-before-running assertion remains NOT_PROVEN. This atomic qualifies
actual admitted execution/barriers, not transport timing or whole T45/READY.

## Execution receipts

Completed qualification of this atomic only, 2026-10-04. HEAD/base remains the
commit above. All source edits used apply_patch. No staging/commit/push/progress/
GOAL/spec/planning/acceptance writes. The only map update is `docs/CODE_MAP.md`.
Inherited `.opencode/` was never inspected, edited or staged. No concurrent Rust
mutation was observed: receipt source associations before/after each command agree.

### Frozen outcomes → current result

| Outcome | Current qualification |
|---|---|
| Owned foreground/current state | **PASS, atomic scope.** Same bounded Jobs/Db/snapshot owns FG and BG. Each actual ELF case has two admitted/running children, three accepted turns and four physical dispatches (one auxiliary) before root completion. Immutable identity and original operation/accepted turn stay unchanged. |
| Same-work conversion | **PASS.** Live child Ctrl+B twice commits one mode event without another request, cancellation, new session/op or shell PID. B remains foreground/running and blocks parent continuation until its provider barrier releases. Explicit same-child successors join actual execution, not the released waiter. |
| Settlement races | **PASS.** Existing transaction owner serializes conversion/terminal; nearest packs cover both orders and actual concurrent threads, all foreign/stale fence fields, dropped shutdown/admission ownership and duplicate finish. Both normal ELFs also race actual leaf completion against Ctrl+B, then issue four stale terminal controls while B remains foreground. Typed current state, exact notice/event/effect counts and no replay are asserted. |
| Functional consumer | **PASS, minimal functional UI.** Ctrl+G list/filter, painted-row click and Enter open actual linked child before root completes. Actual accepted/reasoning/Shell read/text/terminal reconcile into the live view once; committed text appears once on restart (real mouse-wheel history read when late Shell notice is below it). Esc returns root draft. Parked root keeps its actual state/deck; linked capture stays pinned while parent moves. Real Shell Ctrl+B retains its existing owner priority. |
| Authority/source | **PASS, atomic scope.** Actual attempted arbitrary child turn is visibly refused without RPC/turn/selection mutation. Existing profile/model/Undo/Location/permission guards remain; exact read/control fence has no root selection authority. Empty filtered selection cannot cancel/background unrelated work. Existing approval/question owner has priority; permanent question test forwards exact binding despite read-only child and preserves its draft on typed owner rejection. Captured A survives actual parent move B and late notices/leaf effect. |
| Ownership/recovery | **PASS for delivered safeguards and committed display.** Nearest subagent24, owner4, storage7, move1 and binary owner packs retain actual admission, explicit continuation, grants/preimage, child-local dismissal versus plain Reject(None), dropped-shutdown real joins/MCP retirement, unknown-effect refusal and zero-RPC committed reopen. Unfinished automatic resumption remains NEXT. |
| Full gates/artifacts | **PASS for this atomic.** Current workspace **1519 passed / 0 failed / 10 ignored**, fmt, strict all-target locked Clippy, separate no-run, normal debug/release builds/help, post-last-build actual PTY proofs for both unchanged ELFs. Python47 and read-only structural checks recorded below. Whole T45/READY is not claimed. |

### Actual REDs and corrections

- `controls-native-red`, exit1: normal baseline debug ELF had both actual child
  provider requests/accepted turns, but no Subagents/open consumer. This is the
  observed RED; the receipt does not independently claim a successful baseline
  Ctrl+B control attempt. The owned lifetime change is necessary to make real
  conversion possible, not a synthetic replacement result.
- `controls-nearest1`, exit101: foreground completion warning was lost during the
  owned move. Preserve full foreground result and warning text (including failed
  reports), while the existing durable job preview remains bounded.
- `controls-nearest2`, exit101: generic child permission-rejection state also
  represented question dismissal. Mark only actual `AdmissionFailure::Rejected(None)`;
  selected interrupt/dismissal stays child-local. Capture original parent turn so
  late old-child rejection cannot cancel a newer unrelated root turn.
- `controls-consumer2`, exit124/1798s: hard-coded Ctrl+G stole an explicitly
  configured leader and blocked a scripted owner query. Configured conversation
  bindings now precede child/Shell default shortcuts. Existing binary/TUI tests pass.
- `controls-native-dev14`, exit1: source root terminal incorrectly made a pending
  move ready while an owned child turn was still started; unchanged transactional
  family guard correctly refused it. Readiness now waits for actual family terminal.
  The guard is not weakened, and pending move does not cancel the live child.
- `controls-native-dev19`, exit1: late source-A Shell notice through linked child
  after root move B attempted current-Location history. Exact captured read is used
  for child terminal/child notice/Shell notice; parent SessionMoved routes into the
  retained root, never replacing linked A or dropping its read-only guard.
- Final review caught read-only Enter also covering active question forms. The
  outer new-turn guard excludes active questions; separate nearest binary test
  preserves exact real owner binding/reply/error/draft semantics.
- Other failed receipts preserve compile/format errors and fixture diagnostics,
  including raw VT cursor positioning versus contiguous strings, wrong Shell
  title, immutable JSON `status`, pending move timing, events `rowid`, existing
  Shell current-state update, and offscreen late-notice history. A real wheel read
  is used rather than assuming PageUp controls the ordinary transcript. Failures
  were not waived, baseline/deadlines/caps/ignored counts were not raised.

### Current serial gates

All receipts are exclusive, lossless gzip under
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`, named
`t45-background-<name>.log.gz`. Each includes exact argv, HEAD, before/after source
association, exit/counts/raw length/SHA, timing and owned watchdog PID/PGID/startticks/
reaped flag. Existing wrapper sets offline, jobs3, threads1 and the approved TMPDIR;
operational watchdog1798/kill-after2s, tool timeout1800000. Product/test deadlines
and assertions remain unchanged. Receipt names below omit the common prefix/suffix.

| Receipt | Exact command / result |
|---|---|
| controls-nearest-final | `cargo test --locked -p oc-adapters --test subagent` — 24/0/0; later current workspace repeats all24 |
| controls-owner3 | `cargo test --locked -p oc-adapters --lib runtime::children::tests` — 4/0/0; current workspace includes these |
| controls-storage3 | `cargo test --locked -p oc-adapters --lib storage::children::tests` — 7/0/0; current workspace includes these |
| controls-move2 | `cargo test --locked -p oc-adapters --lib owned_child_terminal_boundary` — 1/0/0 |
| controls-routing2 | `cargo test --locked -p oc --bin oc tui_cmd::` — 81/0/0 before additive question test |
| controls-question-nearest | `cargo test --locked -p oc --bin oc tui_cmd::child_controls::tests` — 2/0/0 |
| controls-consumer3 | `cargo test --locked -p oc-tui -p oc` — all targets pass, fixed original ignored counts |
| controls-fmt-complete2 | `cargo fmt --all -- --check` — exit0 |
| controls-clippy-complete | `cargo clippy --workspace --all-targets --locked -- -D warnings` — exit0 |
| controls-no-run-complete | `cargo test --workspace --locked --no-run` — exit0, separate compile gate |
| controls-workspace-complete | `cargo test --workspace --locked --no-fail-fast` — **1519/0/10**, exit0, 1064.072s; all target counts retained |
| controls-debug-delivery / controls-debug-help-delivery | `cargo build --locked`; `target/debug/oc --help` — separate exit0 |
| controls-release-delivery / controls-release-help-delivery | `cargo build --release --locked`; `target/release/oc --help` — separate exit0 |
| controls-native-delivery | `python3 evidence/T45/native_child_controls.py target/debug/oc target/release/oc` — eight real normal-ELF PTY cases, exit0, 16.443s |
| controls-python47-delivery | `python3 -m unittest discover -s scripts -p 'test_*.py'` — **47/0**, exit0, includes test_code_size/test_progress/test_check_docs/test_bounded_live |
| controls-docs-delivery | `python3 scripts/check_docs.py` — exit0, read-only |
| controls-progress-delivery | `python3 scripts/progress.py check` — exit0, structural/read-only, no checkpoint/state mutation |
| controls-size-delivery | `python3 scripts/code_size.py --base 3a29b997f21c15a9ca45d08920fb2a3a7297ba86 --changed` — exit0, no changed Rust file exceeds5000 |
| controls-diff-delivery | `git diff --check` — exit0 |

No Rust edit follows current gates/builds. No Cargo ran between the final release
build and the two-ELF delivery proof. All failures remain lossless, including
zero-test wrong-filter receipts `controls-storage1` / `controls-move1` (not test
qualification). Earlier successful `*-final` / `*-qualified` names predate the
final warning/question preservation changes and are not current ELF attribution.

### Native request/effect/control ledger (each ELF, identical final counts)

| Actual PTY case | Physical POST = Db dispatch | Auxiliary | Child ops | Shell ops | Child mode events | Child notices | Own file effects |
|---|---:|---:|---:|---:|---:|---:|---:|
| conversion, repeated live/terminal Ctrl+B | 7 | 1 | 2 | 1 | 1 | 1 | 1 |
| selected child cancel twice, B continues | 5 | 1 | 2 | 1 | 0 | 0 | 0 |
| real selected Shell Ctrl+B twice, then child conversion | 7 | 1 | 2 | 1 | 1 | 1 | 1 |
| actual terminal/conversion race + four stale selected controls | 6 | 1 | 2 | 1 | 1 | 1 | 1 |
| **Per ELF total** | **25** | **4** | **8** | **4** | **3** | **3** | **3** |

Two ELFs: **50 physical POST/dispatch, 8 auxiliary, 16 child operations, 8 actual
Shell operations, 6 mode events/notices, 6 file effects**, zero reopen RPC/effect
replay. Eight leaf PID/PGID/startticks identities and sixteen application process
identities (initial/reopen) are recorded as joined/reaped, exit0. Each successful
fixture joins its PTY reader and owned HTTP handlers/server before TempDir cleanup.
Terminal race permits either serialized winner with corresponding typed launch/
notice counts; final actual runs both observed conversion winning. Opposite
terminal-first outcome is proven in the nearest owner/storage pack, not invented
as a second native timing outcome. All foreign/malformed fence fields are tested
at the typed owner/API seam; the native stale case is the actual pre-terminal
selected generation becoming terminal, not injected malformed UI identities.

Provider assertions use fixed protocol IDs and typed Db/JSON facts. The deliberately
successful `error: typed successful child output` is Completed, displayed/committed
once; prose/regex is never success classification. Literal display assertions are
only about actual TUI consumption. Private parent transcript/system never enters
child requests; original maker/reader profile/model/source guidance is verified.

Move qualification respects the existing family guard: root completes with A still
live, move remains pending without killing A, then actual family terminal allows
move B before child notice delivery. Shell-route additionally leaves the same owned
BG Shell PID live through actual move B and completes its effect/reap at source A.
It does not claim running-child relocation through the guard. Linked A remains
visible/pinned while root adopts B; terminal/current reopen uses exact linked read.

Final ELF hashes, each checked before/after all four cases:

- debug `36cb0677c76192cb6e6597751efff1f0f660d964c8500ec16bfbf32f7179b325`
- release `f97230003f0735236cec4b4e27e052e5bf3754d70b45766716a522dc422d9b90`

### Changed paths / ownership limits

23 Rust files:

```text
crates/oc-adapters/src/application.rs
crates/oc-adapters/src/runtime.rs
crates/oc-adapters/src/runtime/children.rs
crates/oc-adapters/src/runtime/children/tests.rs
crates/oc-adapters/src/runtime/turn.rs
crates/oc-adapters/src/runtime/turn/foreground.rs
crates/oc-adapters/src/storage/session_move_tests.rs
crates/oc-adapters/src/storage_children.rs
crates/oc-adapters/src/storage_children/tests.rs
crates/oc-adapters/src/storage_session_move.rs
crates/oc-adapters/tests/fixtures/background_children.rs
crates/oc-core/src/core_app.rs
crates/oc-core/src/queries.rs
crates/oc-tui/src/app.rs
crates/oc-tui/src/app/input.rs
crates/oc-tui/src/app/live.rs
crates/oc-tui/src/child_view.rs
crates/oc-tui/src/events.rs
crates/oc-tui/src/lib.rs
crates/oc-tui/src/shell.rs
crates/oc/src/tui_cmd.rs
crates/oc/src/tui_cmd/child_controls.rs
crates/oc/src/tui_cmd/child_controls/tests.rs
```

Plus `docs/CODE_MAP.md`, this report and `evidence/T45/native_child_controls.py`.
No schema migration/dependency/crate/new scheduler/queue/store. Physical owner sizes
remain below5000: application4839, runtime turn4355, binary TUI3679; new consumer
modules248/283 lines and separate tests. Existing owner caps8 global/4parent/8
undelivered, list/cache16, existing history/output/admission/security budgets remain.
Foreground terminal without conversion emits no BG notice and frees notice capacity.

At delivery-proof time92 retained receipts occupy244751 bytes; this report/driver
and final structural receipts keep the complete atomic under300KiB (limit1MiB).
Read-only global evidence size is not a new artifact: HEAD already owns
12035532281 bytes of evidence, including12022349337 bytes in evidence/tui; that
tree has no Git changes and is not pruned/edited. No extra Cargo/tree copies were
made. Mandatory Python documentation tests use their existing owned TempDirs and
exclude evidence/tui; scoped cleanup is not shared/foreign cleanup.

Only synthetic owned HOME/config/source snapshots, loopback provider and owned
shell paths were used. No actual user HOME/environment/config/.local/authing-runner
credentials or paid/live inputs. T27 EXHAUSTED24/Gen15/control1/MCP ledger was not
read/reset/refunded/reused. T44 remains PAUSED; no pixel/SOURCES/ACCEPTANCE edits or
resume. Safely unfinished auto-resumption, command.subagent/subtask routing, full
visuals, packs/host/donor/child DCP remain NEXT, not waived. Historical stronger
peer-POST-before-running remains NOT_PROVEN.

Final handoff releases all mutation/Cargo/owned-native/HTTP/stdio/PTY/watchdog
authority to the parent for independent review/delivery. No whole T45 or READY
claim and no commit/push are made by this coordinator.

Final read-only audit:97 receipts,247426 gzip bytes plus38759 report/driver bytes
=286185 atomic bytes at that audit (below300KiB and1MiB). Every receipt's full
decompressed raw length/hash and reaped watchdog identity was independently
checked, including all failures. No owned watchdog process group remains. All24
final native identities (16 applications +8 leaves) are absent; both final ELF
hashes still match. Current workspace raw target counts sum to1519/0/10, counting
the output once rather than double-counting the receipt's embedded count strings.
Git HEAD remains the base, staged diff is empty, and only the26 listed task/map/
evidence paths plus inherited untracked `.opencode/` are present. No authority is
retained for another mutation, Cargo invocation or native campaign.
