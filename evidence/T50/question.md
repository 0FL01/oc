# T50 R4 / TOOL15 — frozen behavioral obligations

## Coordinator review and current qualification

R4/TOOL15 behavioral slice **PASS**. Reviewed the complete question owner, admission,
Core exchange, durable metadata and native consumer diff. Concurrent commits through
`37738b6b9dc01ea421d935daddef5e3a92069400` changed planning documents only; compiled
source is the original base below plus this reviewed implementation, not clean HEAD.
T44/VIS37 remains separately PAUSED; R5–R8/full T50 and product readiness remain open.

The full workspace log is
`/home/opencode/.local/share/opencode/tool-output/tool_0f42de50e0013Cu1zpilUtyqkp`:
42 successful summaries, **1364 passed / 0 failed / 10 existing opt-in ignored**,
original command exit 0. Strict all-target locked Clippy and normal debug/release
builds/help succeeded. The implementation's recorded Rust patch digest below precedes
the coordinator's additional test-only Ctrl+C sequence; it is historical association,
not a digest of the final test diff. Production source did not change after that gate.

Independent current checks exited 0: Core `question::` **4**, TUI `question_view::`
**4**, actual Core/application `question_real_core_binding_commit_and_restart_no_reask`
**1**, fmt, strict workspace all-target Clippy, normal locked build, Python **47**,
docs/progress structure and diff. Extended the existing owner test and native
`single_custom_auto` case to prove nonempty custom-editor Ctrl+C clears text, the next
Ctrl+C closes editing without answering, and reopening submits only the final answer.
Both normal ELFs passed all **14 native cases / 30 main-child requests each** with
that sequence, without intervening Cargo. No bounds, assertions, ignored tests or
production error/permission behavior were weakened.

A review hypothesis about empty textual forms was withdrawn: the pinned question
producer always supplies `options`, including the truthy JS empty array, so its
custom editor is the non-textual branch. No unsupported general-form behavior was
invented. Admission/binding/cancellation/durability review found no concrete violation.

Hashes remained identical before/after independent native checks:
debug `3fbf74fbdb651252d9b3b322f57f4c02c47fec0871dfca56700f9d6ba785abaa`;
release `b0081f0e8b5216bd5917705c42c6f562a2b57e8892dab5edd1f64a1019ab6ba2`.
No live requests, exhausted-campaign changes or inherited `.opencode/` inspection.

Source base: `b88dc0fe84f3dae2c619ef666e8b439242b82489`, tracked tree clean at admission;
inherited untracked `.opencode/` untouched. T50 active 0004. Sole mutation/Cargo/native
fixture owner for this slice. Pinned donor: `2670273ff17da96f85c5826ced57aa1b368754fa`,
`tool/plugin/question.ts`, `schema/question.ts`, `core/form.ts`, `tui/.../form.tsx`.
Frozen before RED; this report does not claim implementation or visual PASS.

| Obligation | Qualification required |
| --- | --- |
| Canonical truthful definition | Nonempty ordered questions, headers, options, typed optional multiple default false, automatically available custom answer; bounded schema validation before permission/effects. Common Allow/Ask/Deny admission, central Deny and conservative General/Explore catalogs/dispatch. Permission autoaccept cannot answer. |
| Single owned exchange | Exact session/turn/call/tool operation/input/generation/request binding, bounded pending/replies, typed late/foreign/stale/duplicate refusal; existing Core inbox/events. No DB transaction across await. Durable terminal ToolOp/raw result before next provider call; unanswered restart unknown, no reask/replay. No new DB table or alternate history. |
| Real native answer consumer | Existing lower composer region, independent from permission modal. Single, multiple toggles plus explicit submit, custom text, ordered questions/tabs/review/navigation and dismissal. Pending/submitting/answered/error reflects owner; reply errors retain answers/address. Request-keyed process-memory drafts and ordinary composer draft/chips/cursor/focus preserved. Permissions retain priority. Route forms to originating root/child session, not arbitrary current screen. Replay uses bounded typed metadata through the existing tool presentation owner. |
| Non-success boundaries | Headless without consumer fails actionably before waiting; --auto never answers. Deny/General/Explore malicious call has zero wait/answer. Dismiss cancels relevant execution/queued effects and forbids subsequent model continuation. Switch/close/cancel/generation side channels reject stale controls; existing R2 cleanup remains. |
| Actual native proof | Direct normal debug/release ELF with held fake Responses and real PTY input: single/multiple/freeform/multi-question exact ordered answers in next wire request and same persisted ToolOp; headless/auto/negative/dismiss queued mutation zero effects, answered reopen/restart no reask, unanswered restart unknown; actual Core late/foreign/duplicate refusals; root/child policy. Provider/process readiness barriers, no sleep-only cancellation evidence. |

Native bounded request/reply limits are explicit differences from donor's unbounded
pending cache. Text is untrusted ordinary safely styled content, never parsed as
service errors/authority. Existing DCP active-tool protections remain authoritative.

## Required gates

Targeted parser/exchange/runtime/TUI/Core routing, direct ELF PTY/effect/restoration,
headless/auto/negative/reopen; nearest affected crate checks and full cross-crate:
fmt all check, strict workspace all-target clippy locked -D warnings, workspace locked
tests --no-fail-fast (compile --no-run separately if cold), normal debug and release
locked builds/help, Python47, read-only docs/progress validation, diff/code-size review.
Cargo serial jobs=3 / test threads=1 / offline / existing owned disk TMPDIR / command
timeout 900000ms. Preflight uid=1003, available RAM 6430MiB, disk 176142MiB.
Small reports only; fixture TempDirs removed only after owned processes/workers joined.

T44 remains PAUSED: no paired VIS37 campaign/visual parity claim. T27 exhausted
24/24: no live/new campaign/API/browser or credential/env/HOME inspection. R5–R8 later.
No stage/commit/progress/GOAL/spec/historical evidence edits in this delegation.

## Execution results

RED: direct pre-change normal debug ELF sha256
`6757df310a03ef6501e1e4dbef20875c3661f056d810171f2541bfd975572f17`:
`native_question.py target/debug/oc` failed the first headless assertion: exit 0
instead of required 1 for a real model `question` call. Owned child and fake HTTP
peer joined and TempDir cleaned. This establishes the current false-success gap.

## R4 / TOOL15 qualification result

**PASS — frozen behavioral backend and actual native answer-consumer obligations.**
The header above records the pre-RED state; the following results are subsequent
qualification. T50 remains active; R5–R8 remain open. T44 remains PAUSED and VIS37
paired visual qualification is not claimed.

| Frozen obligation | Actual owners and proof | Result |
| --- | --- | --- |
| Truthful schema and effective authority | `oc-core/src/question.rs`; adapters `tools.rs`, `runtime.rs`, `defs.rs`, `runtime/turn.rs`. Strict bounded input before admission/effects; automatic custom option; multiple defaults false. Existing Allow/Ask/Deny admission remains authoritative; configured General/Explore cannot widen their question ceiling. Parser owner suite and native headless/Ask-auto/Deny/child/invalid-input cases. | PASS |
| Application-owned bound exchange and durability | `oc-core/src/question.rs`, `core_app.rs`; adapters `composition.rs`, `application.rs`, `runtime/turn.rs`, `storage.rs`. Exact immutable ApprovalBinding plus request ID, typed refusals, short-lock oneshot holder, common ToolOp/turn transaction before continuation. Actual Core fixture `oc-adapters/tests/fixtures/question_lifecycle.rs` checks wrong session/generation/operation and invalid reply counts, valid reply once, duplicate/late refusal, committed ordered result before the next HTTP request, shutdown/join/reopen with no reask. | PASS |
| Actual lower-composer answer consumer and presentation | `oc-tui/src/question_view.rs`, `app/input.rs`, `shell.rs`, `history.rs`, `tools.rs`; `oc/src/tui_cmd.rs`. Real single/multiple/custom/question-order/review/Tab/navigation/Esc controls, submitting/error states, request-bound process-memory drafts, reply-error retention, permission priority, root/child routing, bounded typed completed history. Four UI owner tests include ordinary composer draft/chips/cursor preservation, remount retention and existing history-budget eviction. Direct PTY answers and actual answered reopen corroborate the consumer. | PASS |
| Actionable non-success and cancellation boundaries | Existing runtime admission/cancellation and Core generation/location checks. No consumer produces durable failed question plus `question_required`/interactive-TUI guidance; `--auto` never answers. Malicious denied calls do not wait or answer. Real Esc with a queued `apply_patch` yields cancelled execution, zero file effect and no provider continuation. Unanswered crash/reopen is unknown, not an active restored form. Full existing approval/child/cancel suites and native R2 regression remain green. | PASS |
| Direct actual-binary evidence | `evidence/T50/native_question.py`: 14 cases on each normal debug/release ELF. Fake Responses receives the exact ordered typed answers and the same already-committed ToolOp result; answered reopen has zero new RPCs/reasks. Actual Core binding/restart fixture complements PTY proof. Existing search14, foreground21 and background26 cases pass on each same ELF with explicit current catalog mode. | PASS |

### Bounded native differences and ownership

- Input/questions and replies are capped at 64 KiB; question count is 1–16,
  options at most 16 per question, header at most 30 characters, custom answer
  at most 4096 bytes. Pending requests/drafts are capped at 64. Strict terminal
  metadata decoding is capped at 131136 bytes. These are declared native bounds,
  rather than donor's unbounded pending cache.
- The existing application inbox/events and immutable approval identity are reused.
  The owned question waiter is separate from permission autoaccept. No database
  schema/table, dependency, crate, toolchain/lock change, generic form framework,
  parallel event bus or second history loader was added.
- Completed typed question metadata is charged to the existing history byte budget.
  Raw ToolOp/turn history remains the persistence source; restart never reconstructs
  active question forms from result text. Draft retention is process-memory remount
  behavior, not crash-persistent draft storage.
- Question text is ordinary untrusted safely styled content. Presentation does not
  infer service errors, permissions or answers from LLM text/regex. Existing active
  tool/DCP protections are reused.
- `docs/CODE_MAP.md` identifies the actual DTO/waiter/runtime/application/TUI owners.

### Gates and current ELF association

All Cargo commands used offline mode, jobs=3, test threads=1 and
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`, serially,
with command timeout 900000 ms. The final normal builds followed all Rust checks;
all subsequent native checks invoked the existing ELFs directly, with no intervening
Cargo command or Rust-source edit.

| Gate | Factual final result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| Question owner tests | Core 4 + TUI 4 + actual Core/adapters lifecycle fixture 1 = 9 PASS |
| `cargo test --workspace --locked --no-run` | PASS; separate compile before the full run |
| `cargo test --workspace --locked --no-fail-fast -- --test-threads=1` | 1364 passed, 0 failed, 10 unchanged opt-in ignored; 42 target/doc-test summaries. Runtime integration target 109 passed. |
| `cargo build --locked`; `cargo build --release --locked`; both `oc --help` | PASS; normal dev and release profiles |
| Direct native question | Debug 14/14; release 14/14 |
| Direct native search, `--question-supported` | Debug 14/14; release 14/14; search schemas unchanged |
| Direct native foreground, `--background-supported --question-supported` | Debug 21/21; release 21/21 |
| Direct native background, `--question-supported` | Debug 26/26; release 26/26; original reap/recovery/notice/effect/capacity contracts retained |
| Native aggregate | 75 cases per ELF, 150 final passing case invocations; all fake/local, no paid calls |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 47 PASS |
| `python3 scripts/check_docs.py`; `python3 scripts/progress.py check` | PASS, read-only structural checks; tasks55/acceptance163 |
| `git diff --check`; `scripts/code_size.py --base b88dc0fe84f3dae2c619ef666e8b439242b82489 --changed` | PASS; 35 changed/new Rust files, largest4248 lines; new question310/view448/Core tests160/UI tests189/runtime fixture238 |

| Normal ELF | SHA256 before and after direct qualification |
| --- | --- |
| `target/debug/oc` | `3fbf74fbdb651252d9b3b322f57f4c02c47fec0871dfca56700f9d6ba785abaa` |
| `target/release/oc` | `b0081f0e8b5216bd5917705c42c6f562a2b57e8892dab5edd1f64a1019ab6ba2` |

These are **DIRTY-source builds**, not clean-HEAD artifacts. Admission/source code
base is `b88dc0fe84f3dae2c619ef666e8b439242b82489`. At final review Git HEAD is
`37738b6b9dc01ea421d935daddef5e3a92069400`: external documentation-only commits
`4ac332f10`, `26ef8ab92`, `37738b6b9` appeared during this delegation. Inspection of
`b88dc0fe8..HEAD` confirms no crates/Cargo/toolchain change, so the Rust build-source
association is unchanged. This owner made no commits or stages.

Reproducible dirty Rust code/test patch SHA256:
`3039c8c7b28d34e690a84d20bcd6057fa265076066398ee0ed98149402fbc18e`.
The byte stream is `git diff --binary b88dc0fe8 -- crates Cargo.toml Cargo.lock
rust-toolchain.toml`, followed by `git diff --no-index --binary -- /dev/null PATH`
for these five new files, in this order:

1. `crates/oc-adapters/tests/fixtures/question_lifecycle.rs`
2. `crates/oc-core/src/question.rs`
3. `crates/oc-core/src/question/tests.rs`
4. `crates/oc-tui/src/question_view.rs`
5. `crates/oc-tui/src/question_view/tests.rs`

Current native-producer patch SHA256:
`3a31ed6c89bf5c125b8d40e5b0475f631804d2fda9446dea2d180d84bab30554`.
The stream is `git diff --binary b88dc0fe8 -- evidence/T50/native_background.py
evidence/T50/native_foreground.py evidence/T50/native_search.py`, followed by
`git diff --no-index --binary -- /dev/null evidence/T50/native_question.py`.
Inherited `.opencode/` is excluded and untouched. No checksum manifest was created.

### Meaningful failures, corrections and conclusions

1. The pre-change direct headless RED above established real false success for
   an unsupported question. The implemented consumer/no-consumer boundary corrects it.
2. Initial actual Core fixture bookkeeping counted an auxiliary title response as
   the main continuation, then double-panicked during cleanup. The fake peer now
   recognizes the existing title wire boundary and always joins without double panic;
   actual binding/DB-before-wire/restart assertions subsequently passed.
3. An initial native reopen command put `--session` at the wrong CLI level and
   timed out externally. It was corrected to `oc --data-dir ... tui --session ...`;
   only subsequent genuine reopen runs are PASS evidence.
4. Resource RED: 20 bounded completed question results did not trigger existing
   history-byte eviction because typed metadata was not charged. The retained-byte
   owner now counts question/options/answer strings; the regression and full suite pass.
5. The prior canonical tool-list regression expected the historical list. Its expected
   list now includes implemented `question`; unimplemented-tool assertions remain.
6. One full-suite attempt hit an existing MCP synthetic HTTP `BrokenPipe`/server-join
   failure in `v07b_stdio_definitive_is_error_remains_failed_and_reusable`. Its unchanged
   targeted rerun passed, then the unchanged full suite passed1364. No MCP test/product
   edit, disabled test or raised deadline/baseline was used.
7. Existing native producers defaulted to the frozen question-absent catalog. Explicit
   `--question-supported` current modes preserve historical defaults and prior reports;
   background propagates that mode to its imported foreground checker. Plan's real
   question authority is distinguished from conservative General/Explore ceilings.
8. Current background reruns exposed producer readiness races: DB turn settlement
   preceded consumer reconciliation for Esc, `/new`, and message actions. The current
   producer now requires the rendered completed-turn footer (below the targeted user
   block for message actions) and the real `/new` Home acknowledgement. Existing 10s
   assertions remain unchanged. Fake HTTP I/O is bounded and HTTP/PTY workers join
   before fresh owned TempDirs are removed. Final full debug/release background26
   and foreground21 pass; no cancellation proof relies on sleeping.

Only small source/report material and bounded tool logs were retained; no repeated
Cargo fixture targets or screen matrices were retained. No live/API/browser/new
campaign, `.local` env, actual HOME or runner-auth inspection was performed.
Final targeted process check found no native ELF actor under this owner's T50
question/foreground/background/search TMPDIR prefixes.

## Handoff

R4/TOOL15 behavioral implementation and qualification are ready for parent review.
R5–R8 are not done; T44/VIS37 remains a separate paused paired qualification; full
GOAL is not claimed. Progress/GOAL/spec/acceptance/historical evidence were not edited
by this owner. All mutation, Cargo and native-fixture ownership is released on return.
