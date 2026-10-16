# T50/R2 TOOL13 supplementary shell controls

## Coordinator qualification — 2026-10-01

Independent review and current retained-ELF checks verify all eight frozen
supplementary obligations below. The original implementation receipt remains
unchanged as history; its pending parent-delivery wording is superseded here.
Source is `43671a7b4fa29e647bc6ced89cd9e7af15ef9518` plus the reviewed
shell-control diff; the intervening owner commit contains no Rust changes.

- Reviewed supervisor joins, conversion/completion serialization, exact selected
  source-session authority, durable foreground/background notice distinction,
  bounded live/final drain snapshots and original viewer identity. Two read-only
  scoped reviews found no concrete introduced counterexample; those reviews are
  not substitutes for native process evidence.
- Fresh `cargo test -p oc-adapters --lib --locked shell::jobs::`: 6 PASS;
  `cargo test -p oc-tui --lib --locked shell_jobs_view::`: 2 PASS. Workspace fmt,
  strict locked all-target Clippy and normal locked debug build exited 0.
- After that final build, direct `native_shell_controls.py` checks passed all
  five cases on each normal ELF: same-PID conversion, child-selected cancellation
  with sibling/parent progress through Location move, converted cancellation,
  converted-crash unknown recovery, and terminal-removal/final-flush race.
  Per ELF: 13 provider requests; conversion has one admission/result/notice/effect
  and zero idle POST; selected cancellation never interrupts the parent. All ten
  PTYs restored their modes; fixture-owned groups and readers joined.
- Current complete workspace gate remains 1419 PASS / 0 failed / 10 existing
  ignored in `t50-shell-workspace-accepted.log`, unchanged compiled inputs.
  Python helper suites: 47 PASS; docs/progress structure and diff checks exit 0.
- Normal fingerprints before/after direct proofs remained exactly
  debug `1e106ce25bb926a05f859987eb114ea7426604e68f7f545e6768972ff1b07843`
  and release `5a239246e863093e183146650354accf3bf77068962265ba6e0f2ff444c11c53`.
  No Cargo invocation intervened during direct native checks.

No paid/native live campaign, user configuration or inherited `.opencode/` was
accessed. R10 cold artifacts/configuration, catalog-only CLI, T45, T56, paused
T44 visual qualification and aggregate product readiness remain separate.

Frozen before RED, source base `0504649d6e773b16bc5deff6a32b2ad32a368c6e` plus foreign owner documentation changes. This is one supplementary atomic, not whole T50/R10/T45/T56/T44 acceptance.

| Obligation | Falsifiable receipt |
|---|---|
| Authoritative bounded running inventory | Real held owned process, exact original source session/Location/operation/generation/model/PID, terminal removal; no inference from immutable running result |
| Live bounded recent output | Supervisor drain bytes while process held; selected open viewer reads final flush after inventory removal |
| Selected Ctrl+D | Exactly selected job/process group cancelled, sibling progresses and unrelated parent remains active; child source preserved |
| Foreground Ctrl+B | Foreground blocks first; same PID, operation, admission, output and original execution context after conversion; exact call-ID running result releases await and natural continuation |
| Races/idempotence | Repeated conversion/kill, completion/cancel/conversion settle once without fork/result/notice duplication |
| Routing/recovery | Parent move/navigation/reopen keeps execution provenance; restart/unknown never replays effects; idle notice causes zero generation POST |
| Existing invariants | Pinned cwd/selected shell/minimal env/common permissions, 8 active jobs and 1 MiB per stream, bounded 500ms teardown, shared Db owner and immutable tool results |
| Required verification | Nearest owned-process RED/GREEN, targeted/full workspace gates, rebuilt normal debug/release ELF PTY/fake-response effect/Db proof, retained-ELF regressions and unchanged hashes |

R10 common cold capture/artifact configuration is deferred; no artifact completion claim. T56 interactive PTYs and T44 VIS39 paired visuals remain separately owned/paused. T45 consumes these owner facts rather than introducing another shell manager.

## Results

**PASS for this frozen supplementary atomic.** Parent review/delivery is still required.
T50 remains active; this report does not finish the task or the full Goal.

Source receipt: code base `0504649d6e773b16bc5deff6a32b2ad32a368c6e`,
current HEAD `43671a7b4fa29e647bc6ced89cd9e7af15ef9518`, plus the explicit
uncommitted paths below. The intervening owner commit is documentation/planning
only. These are **BASE + DIRTY** receipts, not clean-commit qualification.
Foreign owner documents were preserved and excluded; inherited `.opencode/`
was never inspected, edited or staged. No stage/commit/push/progress/spec or
acceptance edits were made by this coordinator.

## Frozen obligation results

| Obligation | Result and receipt |
|---|---|
| Authoritative bounded running inventory | PASS: eight resident owner slots, only durable `running` and unfinished jobs; exact original session/operation/Location/turn/generation/provider/model/PID. Family checks walk the owned source's ancestry, not historical descendants. Real held FG and child/sibling jobs appear; terminal jobs disappear. |
| Actual live bounded output | PASS: the same supervisor stdout/stderr drains provide bounded recent snapshots and absolute byte cursors before terminal. Both-stream overflow unit fixture verifies the existing 1 MiB stream cap and a combined snapshot no larger than the existing 2,048-byte read-side cap. |
| Pinned viewer/final flush | PASS: selection owns source+operation, not a list index. Open viewer survives removal and receives `FINAL-FG`, `FINAL-SIBLING`, and descendant-origin `FINAL-AFTER-LEADER-EXIT`. Same-ID Location adoption retains only that disposable Shell view. Parked open views refresh from the same existing owner events. |
| Selected Ctrl+D | PASS: child cancellation uses exact `CancelShell(source,id)`, never parent `CancelTurn`. Child effect remains absent; sibling stays running, later commits exactly one old-cwd effect; the unrelated parent's turn remains `started` and continues naturally. |
| Admitted FG Ctrl+B | PASS: pre-conversion parent is blocked with one started intent. Conversion retains the exact PID/operation/admission/drains/captured context, records one conversion event, returns one running result for `exact-fg-call`, and naturally issues the next request without a new user prompt. |
| Completion/cancel/repeat races | PASS: repeated conversion/cancel controls do not create another process, intent, result or notice. Completion before conversion returns one truthful terminal result and no background notice. Converted cancellation leaves the next unrelated parent request active. Existing failed-join/early-wake tests still pass. |
| Provenance/routing/reopen/recovery | PASS: child and sibling original generation/model/Location remain after parent move; reopen retains the same delivery identity; converted crash recovers `unknown`, emits one notice, performs zero replay/effect, and makes no idle generation POST. Existing unverified-identity/no-signal and before-fork fences pass. |
| Existing invariants | PASS: same pinned cwd/selected shell/minimal credential-free env, common Ask/Always/Deny+legacy bash domain, eight-slot capacity, per-stream bound, verified process group and existing 500ms cleanup. Existing Db owner/ledger and immutable tool results retained; no schema/crate/dependency, separate manager, worker/event bus, permanent poller or timer framework added. |

## Implementation and real consumer

- `shell/jobs.rs` is the common admitted FG/BG lifecycle owner. FG waits on
  its existing shared supervisor join and a conversion notification; control
  races serialize with terminal freeze. No Work lock or Db transaction is
  held while awaiting a join/control.
- The existing shell ledger records FG admission and accepted BG conversion
  in its events table. Only original BG and actually converted FG owe an
  automatic notice. Unconverted FG recovery retains the existing unknown
  tool-operation behavior, without an invented BG notice.
- Core ports are `shell_jobs`, `shell_snapshot`, `shell_output`,
  `background_shell`, and existing `cancel_shell`. `ShellChanged` uses the
  existing Core event channel/owner wake and actual supervisor facts.
- Minimal real lower-composer consumer: **Ctrl+S** opens Shell; arrows select
  running jobs; **Enter** opens the original job viewer; **Ctrl+B** converts
  the selected admitted FG; **Ctrl+D** cancels only the selected source/job;
  **Esc/Ctrl+C** return viewer → list → composer. Approval/question/modal
  precedence and the unsent draft are retained. Late terminal controls are
  harmless, rather than fatal application errors.
- New private owner seam: `oc-tui/src/shell_jobs_view.rs` and its focused
  `tests.rs`. `docs/CODE_MAP.md` maps it and the same common supervisor owner.

## Normal retained ELF receipts

Normal builds, not Cargo test executables:

| ELF | SHA-256 | Build/help |
|---|---|---|
| `target/debug/oc` | `1e106ce25bb926a05f859987eb114ea7426604e68f7f545e6768972ff1b07843` | build 0, 9.93s; help 0 |
| `target/release/oc` | `5a239246e863093e183146650354accf3bf77068962265ba6e0f2ff444c11c53` | build 0, 2m19s; help 0 |

`file` verified normal x86-64 GNU/Linux ELF executables (debug contains debug
info; release stripped). Build IDs were respectively
`6e28e0d95a57d3812424ee2d86aee4690814f1d0` and
`d456118f375ec373abfe3b28cff7f848099408bc`.
Both were fingerprinted after the last normal build; every subsequent proof
ran directly, with **no Cargo build between or after**. Final fingerprints
are unchanged. Only fixture/report edits followed those builds; Rust code
was unchanged.

### New real-process/PTY receipts

All captured models below are `fixture/gpt-shell-controls`, original
generation **1**, original cwd/Location equals the synthetic fixture's
`project`. Operation suffixes name the exact call below; complete operation,
source, Location, generation and PID receipts are in the two controls logs.
Every row asserts genuine Db facts and real file/process barriers, not text
inference.

| ELF/case | Shell PID(s) | Requests | Intent/result/notice/effect facts |
|---|---|---:|---|
| debug conversion, `exact-fg-call` | 3907442 | 2 | 1 intent, 1 original-call running result, 1 conversion, 1 notice, `FG.admission=one`, `FG.effect=1`, idle POST 0 |
| release conversion, `exact-fg-call` | 3907811 | 2 | Same facts; unchanged PID before/after conversion and reopen |
| debug child kill/move | child 3907498; sibling 3907482 | 5 | 2 jobs; child cancelled/no notice/`CHILD.effect=0`; sibling completes/1 notice/`SIBLING.effect=1` in original cwd; parent not interrupted |
| release child kill/move | child 3907876; sibling 3907865 | 5 | Same exact-source, original-context and parent/sibling isolation facts |
| debug converted cancel, `cancel-fg` | 3907610 | 2 | 1 job, 1 notice, `CANCEL.effect=0`; next parent request remains started |
| release converted cancel, `cancel-fg` | 3907970 | 2 | Same facts after repeated Ctrl+B/Ctrl+D |
| debug converted crash, `crash-fg` | 3907684 | 2 | 1 admission/job/notice; unknown; `CRASH.effect=0`; no replay/idle request across repeated reopen |
| release converted crash, `crash-fg` | 3907994 | 2 | Same original durable identity and no replay |
| debug completion, `flush-fg` | leader 3907733; descendant 3907741 | 2 | 1 terminal result with final descendant bytes; 0 conversion/notice; `FLUSH.effect=1` |
| release completion, `flush-fg` | leader 3908044; descendant 3908052 | 2 | Same final-flush-before-terminal truth after list removal and repeated controls |

Original child sessions are respectively
`t50-background-sub-1790893454018-0` and
`t50-background-sub-1790893461758-0`; both siblings belong to
`t50-background`. Their source pairing is checked against `sessions.parent_id`.
The selected child viewer stays on its child identity after parent same-ID
move, and the sibling effect is absent from the destination Location.
Marker fixtures execute `/bin/sh` through the real supervisor: write PID and
one admission, emit LIVE, block on an actual release file, then emit FINAL
and create the effect. The flush fixture's descendant emits final output
from its TERM handler after the leader exits.

All ten new cases assert restored PTY ICANON/ECHO on orderly shutdown.
PTY readers and fake HTTP owners join before their exact owned TempDirs
are cleaned. All 14 recorded shell/descendant PIDs above were subsequently
checked with `ps` and were absent. No owned Cargo/fake-fixture/native process
remains. Five pre-existing foreign TUI processes found in the shared account
were left untouched.

## Gates, commands and retained logs

Log root **B**:
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Cargo prefix **C** (each invocation, one Cargo at a time):
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 CARGO_NET_OFFLINE=true TMPDIR=$B`.
Each tool invocation had a 900000ms outer timeout; no timeout increase,
Cargo clean, target relocation, dependency upgrade, baseline/golden/caps
relaxation, or new ignored tests was used.

Preflight: uid **1003**, MemAvailable initially **4354 MiB**, disk available
**182443 MiB**. Final heavy-gate preflight: MemAvailable **6454 MiB**,
disk available **180821 MiB**. All fake inputs use private synthetic HOME,
config, credentials-free environment and loopback peers. No real HOME,
authoring config, `.local/live.env`, service or paid/live input was used;
T27 allowance was neither reset nor extended.

| Command (with C where Cargo) | Actual result | Log under B |
|---|---|---|
| `cargo test -p oc-adapters --lib --locked tool13_held_owned_shell_output_is_readable_before_terminal` (baseline RED) | 101; 0 passed, 1 failed, 428 filtered; held real bytes unavailable before terminal | `t50-shell-red.log` |
| `cargo test -p oc-adapters --lib --locked shell::jobs::` (final targeted) | 0; 6 passed, 426 filtered; 3.37s | `t50-shell-owner-final.log` |
| `cargo test -p oc-tui --lib --locked shell_jobs_view` | 0; 2 passed | `t50-shell-tui.log` |
| `cargo test -p oc --test durability --locked aud06_binary_kill_after_side_effect_recovers_unknown_without_replay` | 0; 1 passed, .56s | `t50-shell-durability.log` |
| `cargo fmt --all -- --check` | 0 | `t50-shell-fmt-accepted.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0; strict, 16.00s | `t50-shell-clippy-accepted.log` |
| `cargo test --workspace --locked --no-run` | 0; separate compilation | `t50-shell-workspace-compile-final.log` |
| `cargo test --workspace --locked --no-fail-fast` | 0; **1419 passed, 0 failed, 10 existing ignored** | **`t50-shell-workspace-accepted.log`**, complete current 1681-line workspace log |
| `cargo build --locked` | 0 | `t50-shell-debug-build.log` |
| `cargo build --release --locked` | 0 | `t50-shell-release-build.log` |
| `target/debug/oc --help`; `target/release/oc --help` | both 0 | `t50-shell-debug-help.log`, `t50-shell-release-help.log` |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 0; **47 passed**, including existing bounded-live infrastructure tests | `t50-shell-python.log` |
| `python3 scripts/progress.py check` | 0; structure only, including final report check | `t50-shell-progress-final.log` |
| `python3 scripts/check_docs.py` | 0; 56 tasks/166 acceptance specs; final structure check | `t50-shell-docs-final.log` |
| `python3 scripts/code_size.py --base 0504649d6e773b16bc5deff6a32b2ad32a368c6e --changed` | 0; advisory; max touched file 4607 lines; Jobs 794, tests 443, new view 210+83 | `t50-shell-code-size-accepted.log` |
| `git diff --check` | 0 | final terminal receipt |

The 1419 total is the prior 1413 baseline plus four meaningful adapter
scenarios and two TUI scenarios. Workspace includes existing captured-view,
active-turn admission, next-request budget/history, cancellation, recovery,
Revert/Fork/DCP and read-cap tests.

### Direct retained-ELF fixture commands

Each command below ran with `TMPDIR=$B`, once with `$ELF=target/debug/oc`
and once with `$ELF=target/release/oc`, after the final normal builds.
All helper exits are **0**. Total **90 cases per ELF / 180 accepted cases**:

| Exact command after `TMPDIR=$B` | Cases per ELF | Accepted logs under B |
|---|---:|---|
| `python3 evidence/T50/native_shell_controls.py $ELF` | 5 | `t50-shell-controls-debug.log`, `t50-shell-controls-release.log` |
| `python3 evidence/T50/native_foreground.py $ELF --background-supported --question-supported --shell-controls-supported` | 21 | `t50-shell-fg-debug.log`, `t50-shell-fg-release.log` |
| `python3 evidence/T50/native_background.py $ELF --question-supported --shell-controls-supported` | 26 | `t50-shell-bg-debug-accepted.log`, `t50-shell-bg-release-accepted.log` |
| `python3 evidence/T54/native_runtime.py $ELF` | 9 | `t50-shell-t54-debug.log`, `t50-shell-t54-release.log` |
| `python3 evidence/T50/native_live_model_switch.py $ELF` | 7 | `t50-shell-switch-debug.log`, `t50-shell-switch-release.log` |
| `python3 evidence/T50/native_file_mutations.py $ELF` | 22 | `t50-shell-r9-debug.log`, `t50-shell-r9-release.log` |

The BG suite includes real Revert (2 effects/notices, old visible 0/new 1),
Fork (one original effect/notice, captured fork notice, zero fork jobs),
capacity 8 + joined idle reuse (9 effects/terminal/notices, idle requests 0),
original shell/cwd/env, crash/unknown/reopen, unverified-identity no-signal,
permission/child ceilings and group closure. Its freeze-error subcase
intentionally expects native shutdown exit **1** after verified reap.
T54's quota subcase intentionally expects native exit **1**; the overall
asserting helper exits 0. Live-switch rechecks captured FG tooling while
busy and same-task continuation across stream/tool/Ask/retry/compact/child
boundaries, including old captured view/new next-request budgets.
Current-mode FG/BG helper switches account for new authoritative admitted
FG rows and the current GPT file family; historical reports/default modes
were not rewritten.

## Diagnosed failed approaches

1. The nearest baseline RED was a real missing live-output observation,
   not a proposed API/test-only mock. Own groups joined before assertion.
2. Early compilation caught exhaustive Core event arms, private imports,
   an input block initially placed in paste routing, and test SQL integer
   decoding. They were corrected; no lint allow/baseline changes masked them.
3. The first full workspace run exposed **AUD06**: an unconverted recovered
   FG job incorrectly owed an unknown BG notice, contaminating accepted
   input. Production notice/busy predicates were fixed; AUD06 was unchanged.
   `t50-shell-workspace.log` retains that failure.
4. One post-fix workspace run hit the unchanged **900s** outer timeout after
   recompilation and green completed targets, before TUI completion. It is
   **incomplete**, not PASS (`t50-shell-workspace-final.log`). Warm identical
   rerun passed (`t50-shell-workspace-final2.log`), then the final separately
   compiled current run passed in `t50-shell-workspace-accepted.log`.
5. The initial final-flush PTY proof lost unchanged screen cells when parsing
   a rolling ANSI tail, despite the Db storing the exact final bytes. The new
   focused fixture now maintains a bounded incremental 40x110 terminal grid
   with a 4096-byte partial-sequence cap. No product correction was needed.
6. First release BG regression attempt failed `history continuation failed`
   (`t50-shell-bg-release.log`, helper exit 1): it typed the next prompt after
   DB settlement but before the consumer's actual completion ACK. Current
   mode now waits for an exact per-turn response marker followed by the
   real completed footer, rather than increasing timeout or weakening facts.
   Both complete BG suites then pass on the same retained ELFs.

No failed receipt was deleted or presented as a passing run. All new
`t50-shell-*.log` files totaled **616935 bytes** before this report/final
structure checks, with the largest raw log **132469 bytes**; total new
logs/reports remains below 1 MiB and each raw log below 16 MiB. Only exact
private owned TempDirs were cleaned after confirmed process/reader/peer
joins; retained Cargo trees, screenshots, history and foreign Temps remain.

## Exact coordinator changed paths (25)

```text
crates/oc-adapters/src/application.rs
crates/oc-adapters/src/runtime/turn.rs
crates/oc-adapters/src/shell.rs
crates/oc-adapters/src/shell/jobs.rs
crates/oc-adapters/src/shell/jobs/tests.rs
crates/oc-adapters/src/storage.rs
crates/oc-adapters/src/storage_shell_jobs.rs
crates/oc-adapters/tests/runtime.rs
crates/oc-adapters/tests/runtime/turns.rs
crates/oc-core/src/core_app.rs
crates/oc-core/src/queries.rs
crates/oc-tui/src/app.rs
crates/oc-tui/src/app/input.rs
crates/oc-tui/src/app/live.rs
crates/oc-tui/src/events.rs
crates/oc-tui/src/lib.rs
crates/oc-tui/src/shell.rs
crates/oc-tui/src/shell_jobs_view.rs
crates/oc-tui/src/shell_jobs_view/tests.rs
crates/oc/src/tui_cmd.rs
docs/CODE_MAP.md
evidence/T50/native_background.py
evidence/T50/native_foreground.py
evidence/T50/native_shell_controls.py
evidence/T50/shell-controls.md
```

## Remaining scope and handback

- **R10** common cold artifact/output-limit capture pipeline is not implemented
  here. Existing terminal outcome storage and minimal original identity/byte
  receipts are not a new artifact store or full R10 acceptance.
- **CLI**: this atomic supplies Core ports and the required minimal real TUI
  consumer; no standalone shell-list/control CLI or whole T50 CLI qualification
  is claimed. The separate `oc models`/catalog CLI obligations remain with
  their own slice.
- **T45** aggregated/background child controls must consume this existing
  Shell/jobs owner and its original facts; no competing root state engine
  or future child orchestration was introduced.
- **T56** interactive PTYs, **T44 VIS39** exact paired visuals and full Goal
  acceptance remain separate. T44 stays PAUSED and T50 ACTIVE; no paid/live
  campaign or whole-task completion claim.

On handback the coordinator releases **all sole mutation, Cargo and owned
fake-fixture authority** to the parent. No hidden continuation or owned
process/worker remains. Parent performs independent review and delivery.
