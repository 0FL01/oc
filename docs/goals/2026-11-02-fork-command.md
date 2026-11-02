# Goal: `/fork` command (Full session + Before message), parity with OC2 TS

Status: active
Source: user instruction 2026-11-02 — approved plan «/fork для native OC2» (plan v3 below); «Реализовать и коммит пуш, так же починить тесты все если будут падать, за одно баги исправить если они есть при тестах».
Last updated: 2026-11-02

## Objective
In the native TUI, `/fork` opens a "Fork session" picker; "Full session" forks the whole visible history (incl. last completed answer) into a new root with an empty composer; picking a user message forks history strictly before it and restores its text/images unsent. Committed and pushed to `origin/master` with the offline gate green.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

TS reference (read-only): `.local/opencode/packages/tui/src/routes/session/dialog-fork.tsx`, `routes/session/index.tsx:887-894,1079-1093`, `ui/dialog-select.tsx`; server `packages/core/src/session.ts:339-380`, `session/projector.ts:135-223`.

### Required Outcomes
- R1: `/fork` command and picker
  - Acceptance: palette "Fork session", id `session.fork`, group Session, slash `fork`, no keybind. Opens panel "Fork session", `DialogSize::Large` (88 cols, as TS). Row 1 "Full session", then user messages of the loaded `HistoryWindow`, newest→oldest, one row per message ID, `\n`→space. Fuzzy search via existing `SelectList` (may reorder/filter Full, like TS). Selecting value changes on a message → transcript scrolls to that message (preview); on Full — no scroll. Unavailable on Home / no session / read-only or linked child (same gate as `session.rename`, incl. palette exclusion).
  - Primary evidence: oc-tui unit tests (input/transcript, TestBackend).
  - Status: verified
  - Evidence: oc-tui `fork` slice: 3 passed; picker selected-value preview, pending rendering and availability covered.
- R2: Before (message) fork via picker
  - Acceptance: new root with history strictly before message; its text+images restored unsent in the new tab's composer. Existing Before semantics (incl. adjacent model/agent switch notice exclusion) and Message Actions fork unchanged.
  - Primary evidence: existing fork tests stay green + lifecycle test via picker.
  - Status: verified
  - Evidence: storage existing Before cases passed; application fork tests 2 passed; executor picker adoption preserves unsent prompt and source draft (4 fork lifecycle tests passed).
- R3: Full session fork
  - Acceptance: new root with all visible history (`conversation_*` views) at transaction time, incl. last completed assistant answer; composer empty; hidden Revert tail not resurrected; undelivered inbox/steer not copied; DCP terminal context = source's current tracked context (Revert → `/compact` → Full keeps checkpoint). Empty source → explicit refusal. Unsettled/unknown turn or unproven tool outcome in copied graph → explicit refusal, no root.
  - Primary evidence: storage_fork / storage_fork_dcp_tests / application_fork_tests; pty_t39 scenario.
  - Status: verified
  - Evidence: storage fork 19 initial passes plus corrected refusal fixture 1 pass; Revert→compact→Full→reopen regression passed. PTY Full fork 1 passed, no generation/title request.
- R4: Async lifecycle
  - Acceptance: while pending, panel shows "Forking session…" and ignores Enter (no second fork). Esc closes panel, does not cancel accepted op. Success: panel closes, toast "Forked session" (normal and parked adoption). Any error incl. synchronous admission refusal (tab limit) closes panel and reports error as today. Late ACK after tab switch parks fork, does not hijack current view. Source, its draft and running work untouched; Before works beside active source.
  - Primary evidence: `crates/oc/src/tui_cmd/tests/lifecycle.rs` tests.
  - Status: verified
  - Evidence: executor `fork` slice 4 passed (pending duplicate Enter, Esc, late ACK/parked adoption, keyboard/mouse admission refusal and async refusal).
- R5: Delivery
  - Acceptance: offline gate `bash scripts/test.sh` green (failing tests fixed; real bugs found by tests fixed at owner), scoped Clippy clean, commit(s) pushed to `origin/master`.
  - Primary evidence: gate output, `git log origin/master`.
  - Status: in_progress
  - Evidence: current fork and R5-delta Review-Sol PASS; full gate fmt/Clippy passed, nextest 2588 passed/5 failed/25 skipped. All five failed cases subsequently passed in failed-only runs; search 8/8, quit 9/9, strict scoped Clippy and remaining doctest/debug CLI gates passed. Release build PASS (cold 14m06s). Commit/push remain; no claim of one clean full-gate run.

### Constraints
- C1: AGENTS.md (4 build jobs, sequential Cargo, no cargo clean, no CARGO_TARGET_DIR override, TMPDIR outside worktree `/home/opencode/.cache/opencode-tmp/opencode`, do not touch/restart user's owner).
- C2: Native wire exact-version handshake stays fail-closed (`ProtocolMismatch`); no auto-retry of unknown fork outcome.
- C3: No storage schema migration; no provider request during fork; no busy guard added.
- C4: Foreign uncommitted diff (`docs/TEST_PLAN.md`, `scripts/test.sh`, `progress/`, `scripts/test_test_runner.py`) is preserved and NOT committed unless it is needed to fix a failing gate (then report explicitly).

### Non-goals
- CLI `--fork`, ACP, filesystem snapshots, configurable keybind, changing Message Actions fork flow.
- Deliberate TS deviations: title stays `Title (fork)` (not `(fork #N)`); no timestamp footer (`HistoryRow` has no time); Full refuses instead of skipping incomplete rows; picker limited to loaded window.
- Budget counting over physical `messages` (inherited Before behavior) — not changed.

## Change Envelope (approved plan v3)
1. Core `crates/oc-core/src/core_app.rs`: `InboxMsg::ForkSession { source, before: Option<MessageId>, ack }`; keep `fork_session(source, before)` → `Some`; add `fork_full_session(source)` → `None`. `ForkSessionSnapshot` unchanged (empty prompt/images for Full). `scripted_unsupported` `{ack, ..}` unchanged.
2. Wire `crates/oc-adapters/src/native/wire.rs:116` `Request::ForkSession { source, before: Option<MessageId> }`; `native/endpoint.rs:14` VERSION 9→10; `crates/oc/tests/pty_t42/shared_runtime.rs:766` hard-coded `"version":9`→10; docs `CONTRACTS.md:350` (v9) and `ARCHITECTURE.md:61` (v8) → v10.
3. Application `application.rs:3417-3443`: same admission; pass `before.as_ref().map(|m| m.0.as_str())` (`MessageId` has no `as_str`).
4. Storage `storage_fork.rs`: `fork_session_admitted`/`fork_transaction` take `before: Option<&str>`; test-only `Db::fork_session(&str)` wraps `Some`. `None`: inside same IMMEDIATE tx `MAX(seq)` over `conversation_messages` for source; NULL → explicit refusal (empty session); `cutoff = max.checked_add(1)`; no switch-notice shift, no user validation, prompt `""`, images `[]`. Everything else reuses `cutoff`. Event `{"source","before"}` with `before:null` for Full (no readers).
5. DCP `storage_fork_context.rs`: in `fork_transaction` resolve once, before copying, `terminal_context: Option<String>` — Before: existing boundary pre-context lookup (now at `storage_fork_context.rs:53`/`:472`); Full: `Some(Self::save_context(&tx, source)?)`. Replace `before: &str` with `terminal_context: Option<&str>` in `standalone_prefix_operations` (`:32-37`) and `copy` (`:456-467`); include digest in digest sets (`:54,480-482`); in `copy` its import takes priority over `last_post` (`:530-537`). Genuine points/rebasing/validation/budgets unchanged; no fabricated points; no live DCP table reads.
6. TUI `crates/oc-tui`: `commands.rs` `CommandAction::OpenFork` + REGISTRY; availability in `app/input.rs:404-503` like Rename (incl. linked-child `425-443`, Home `480-487`, palette exclusion `130-131`). `app.rs`: `TuiPanel::Fork { pending: bool }`, `PanelIntent::ForkFullSession`; messages reuse `ForkMessage`. Options from `window.rows()` reversed, user rows with ID, dedup; values `"full"` / `"message:<id>"`; Enter/preview read selected value and re-resolve the row (never index window by filtered cursor). Preview: `app/transcript.rs` `jump_to_message(id)` — resolve row's `Arc<MessageId>`, stop `wheel_motion`, invalidate press targets, `scroll = scroll.max(1)`, set `CompletionAnchor { message, part: 0, component: None, row: 0, requested_scroll: scroll, pending: true }`, call `invalidate_transcript()` on every jump (projection cache ignores anchor); called from Fork branch in `input.rs::sync_modal_cursor` on selected-value change. `dialog.rs`: `size_for` → Large for Fork; `render` pending → "Forking session…" instead of search/list.
7. Executor `crates/oc/src/tui_cmd.rs`: `ForkFullSession` in same guard arms as `ForkMessage` (3355-3389, 3404-3428) and job arm (3634) calling `fork_full_session`. Synchronous rejection of a fork intent in both outcome wrappers (keyboard 3208-3229, mouse 3261-3278) closes Fork panel. `finish_conversation`: close origin's Fork panel before parked early-return split (1295-1308) and on async error (1337-1342). Success → `push_transient_note("Forked session", NoteVariant::Success)` on `next` in both normal and parked branches; keep refresh-failure feedback.
8. Docs: `docs/DECISIONS.md` D30 — Before may run beside active source; Full copies through last visible message and refuses immediately on unsettled graph.
- Tests: extend existing modules listed in Primary evidence; one pty_t39 `interaction.rs` scenario `/fork` → Full → new tab has last answer, no new provider request.
- Forbidden: migrations, new deps, TS changes, committing foreign diff (C4), secrets.

## Checks (sequential, TMPDIR=/home/opencode/.cache/opencode-tmp/opencode)
```
CARGO_BUILD_JOBS=4 cargo test -p oc-adapters --lib --locked storage::fork -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo test -p oc-adapters --lib --locked application::fork_tests -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo test -p oc-tui --lib --locked fork -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo test -p oc2 --bin oc2 --locked fork -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo test -p oc2 --test pty_t39 --locked fork -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo test -p oc2 --test pty_t42 --locked srv0 -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo clippy -p oc-core -p oc-adapters -p oc-tui -p oc2 --all-targets --locked -- -D warnings
CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 bash scripts/test.sh   # once before acceptance
git diff --check
```
Keep green: `message_actions_focus_intents_clipboard_result_and_escape`, `fork_owner_result_adopts_real_root_with_unsent_prompt_and_preserved_source`, `quit_pending_fork_saves_accepted_identity_even_when_refresh_fails`, `srv04_running_picker_adoption_is_not_a_removed_tab`, `srv01_*`.

## Current Checkpoint
- Closes: R1–R5
- Smallest next action: explicit task-only commit/push, then record verified delivery.
- Expected evidence: all gate cases pass with no weakened assertions/deadlines/caps, current delta review and release build; origin/master contains delivery commit.
- Stop or replan if: Full requires schema migration or changes Before semantics.

## Current State
- Resolved: R1–R4 implemented and targeted checks passed; scoped all-target Clippy clean; current Review-Sol PASS after singleton-slot guard fix (session ses_e6459377fffec6dpNNqFHDehhs).
- Last relevant evidence: fork slices storage 19+1/application 2/TUI 3/executor 4/PTY39 1/PTY42 9 passed. Gate-fix search 8/8, quit 9/9, failed PTY cases PASS; strict scoped Clippy, doctests (0 examples), debug build/help/version and release build PASS. Review-Sol current fork and R5-delta PASS (tracked diff SHA-256 `01f32147151d4a97bd5a49689e37c273eb524b04f669a3b1173b9b8a196f3e0f`).
- Blocker: none; orchestrator confirmed foreign diff was reverted externally and must not be restored or committed. `progress/` stays untracked.
- Next: final diff check, explicit task-only commit and ordinary push origin/master, then completion record.

## Material Decisions
- 2026-11-02: Full = optional `before` on existing RPC (not new variant), wire v10.
- 2026-11-02: Full DCP terminal cut = `save_context(tx, source)` digest (review found last post-context loses checkpoint after Revert → `/compact`).
- Operational risk: v10 client cannot attach to an already running v9 owner; do not stop/restart user's owner.
- 2026-11-02: `views.rs` is a direct exhaustive consumer of `TuiPanel`; its two presentation match arms include Fork (required for R1 compilation/cursor presentation), no separate behavior change.
- 2026-11-02: final review proved a second tab could replace the singleton conversation job while a fork ACK was pending. R4 requires serializing both writers (Fork/ChangeConversation) without blocking navigation; added one admission guard and extended the held-ACK lifecycle regression.
- 2026-11-02: orchestrator resolved C4: original foreign changes were reverted externally; do not restore/commit them. Pre-gate own tracked diff matches final review SHA-256 `54f6afd19c2fa315c3fd5c88b8338fcd9d8227b9659a06fe7767c706573b5fd6`.
- 2026-11-02: R5 gate evidence requires minimal envelope expansion: `oc-adapters/src/files/{search.rs,tests.rs}` to preserve discovered grep candidates on entry-cap exhaustion (caps/cancellation/incomplete preserved); `tui_cmd.rs` and lifecycle tests to settle queued auxiliary history before lease release with bounded local quit; `pty_t39/interaction.rs` and `provider_readiness/custom_connections.rs` to use actual transcript/key-form readiness rather than ambiguous text. Existing resource-sample case passed failed-only rerun unchanged; no speculative baseline/lifecycle fix.

## Checkpoint History
- 2026-11-02: plan frozen.
- 2026-11-02: implementation batch complete; Full captures one terminal context inside the transaction, picker previews selected values and executor handles normal/parked/error completion. Targeted verification started.
- 2026-11-02: storage/DCP implementation validated by 19 passing cases, including Revert→compact→Full→reopen; one new fixture corrected without changing production guards.
- 2026-11-02: targeted application/TUI/executor/PTY and scoped Clippy passed; implementation frozen for independent review before offline gate/delivery.
- 2026-11-02: Review-Sol found cross-tab conversation-slot replacement (P2); minimal guard/regression applied. Reviewer also observed original foreign tracked diff and untracked test runner disappeared while HEAD stayed f38ffe8d6; delivery/full-gate pause for parent confirmation, no restoration or foreign-file edits performed.
- 2026-11-02: P2 fix passed 4 lifecycle tests and scoped Clippy; same reviewer gave current PASS. C4 unblocked by orchestrator; full offline gate started.
- 2026-11-02: full offline gate ran once: fmt/Clippy passed; nextest 2588/2593 passed, five failures (search entry budget, PTY resource sample, quit prompt history, pending-steer click, custom-provider key-change). Failed-only rerun confirmed search/history/steer failures; resource sample and key-change passed. Independent read-only cause diagnosis started; no baseline/guard weakening.
- 2026-11-02: gate fixes passed search 8/8 (retained bounded candidates), quit 9/9 (all-view receipt settlement, 1s bound, no replay or skipped deck save), and failed PTY cases. Temporary mouse diagnostics proved late catalog projection generation changed between Down/Up; real hover-paint ACK fixes test synchronization without touching press guards. Key-form/masked-input readiness fixed rekey race. Resource sample passed unchanged; exact thread-identity cause remains unproven. Diagnostics removed. Doctests/debug CLI and strict scoped Clippy passed; delta review/release/delivery next.
- 2026-11-02: same Review-Sol approved actual R5 delta without further findings; release build passed. Final scope check: 26 tracked task files plus this goal, no migration/dependency/TS change, no foreign-file changes or included artifacts. Delivery next.

## Completion
- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
