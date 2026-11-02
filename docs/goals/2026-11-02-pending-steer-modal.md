# Goal: Pending steer modal in Rust TUI

Status: active
Source: user request (handover + approved plan v2, `.local/tmp/pending-steer-plan-v2.md`), user instruction "Реализовать и коммит пуш"
Last updated: 2026-11-02

## Objective
Clicking a concrete pending steer row in the oc2 TUI transcript opens a "Pending steer" modal (Search, Move to queue, Delete; keyboard, mouse, Esc) acting on that exact inbox item, without changing the composer draft or interrupting the agent; committed and pushed to `origin/master`.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R1: Click on a specific pending steer row opens `Pending steer` for that inbox ID (not by text/index); two identical texts with different IDs are distinguished; existing click guards (selection, drag, scroll, resize, overpaint, stale paint) still apply; MessageActions/Redo unchanged.
  - Source: user request; plan v2 edits 1-2.
  - Acceptance: TUI test clicks second of two identical steer rows → panel for id2.
  - Primary evidence: `cargo test -p oc-tui --lib app::tests::inbox`
  - Status: verified
  - Evidence: oc-tui inbox 14/14 and existing transcript/Select guards 7/7; exact second identical row targets id2. Actual-binary busy-inbox PTY 1/1 after isolating ancillary title publication.
- R2: Modal has Search and actions Move to queue (`Deliver{id, Queue}`) / Delete (`Remove{id}`), both via `PanelIntent::Inbox{restore:false}`; keyboard Up/Down/Enter, mouse, Esc (closes, no intent); empty Search result → no intent; draft unchanged; mouse click must match last painted options (guard vs repaint lag, drag, Search change).
  - Source: user request; plan v2 D1, D6, edits 3-4.
  - Acceptance: TUI tests a, b, d, e from plan v2.
  - Primary evidence: `cargo test -p oc-tui --lib app::tests::inbox`
  - Status: verified
  - Evidence: inbox tests cover Move/Delete, raw terminal Up/Down/Enter, Search/no results, Esc, stale paint, drag and press/query/repaint/release; composer prompt/cursor/selection preserved.
- R3: With the modal open, the window never closes by itself; exact in-turn delivery (`InboxChanged.delivered` with that ID, routed view incl. parked) shows "Accepted by the agent" and forbids Move/Delete; disappearance without confirmation shows "No longer pending"; while an approval is active the modal is hidden/inert but stays open.
  - Source: agreed edge case; plan v2 D3, D5, edit 3 (approval), edit 5.
  - Acceptance: TUI tests c, f; binary test for delivered routing.
  - Primary evidence: `cargo test -p oc-tui --lib app::tests::inbox`; `cargo test -p oc2 --bin oc2 tui_cmd::tests`
  - Status: verified
  - Evidence: inbox tests cover sticky exact acceptance, neutral disappearance, no actions and approval overlap; binary tui_cmd tests 135/135 include active and parked InboxChanged routing.
- R4: A stale Queue→Queue `Deliver` no longer clears the error of a failed queued item; Queue→Steer retry still clears it.
  - Source: plan v2 D2.
  - Acceptance: storage test.
  - Primary evidence: `cargo test -p oc-adapters --lib storage::inbox::tests`
  - Status: verified
  - Evidence: storage::inbox::tests 12/12, including stale Queue→Queue failure retention and explicit Queue→Steer retry/payload preservation.
- R5: Change committed and pushed to `origin/master`, excluding foreign diff.
  - Source: user "коммит пуш".
  - Acceptance: `git log origin/master` contains the commit; foreign files untouched.
  - Primary evidence: `git status`, `git log -1 origin/master`
  - Status: in_progress
  - Evidence: reviewed/tested implementation ready for explicit-path commit and normal push; foreign diff remains untouched.

### Constraints
- C1: No wire/protocol change (private wire stays version 9); no new `InboxAction` variant.
- C2: No replay, no Cancel, no composer restore from modal actions.
- C3: Preserve foreign uncommitted diff: `docs/TEST_PLAN.md`, `scripts/test.sh`, `progress/`, `scripts/test_test_runner.py` — never stage/commit/modify them.
- C4: AGENTS.md cargo rules: `CARGO_BUILD_JOBS=4`, `--test-threads=4`, one Cargo owner, no parallel cargo, no `cargo clean`.

### Non-goals
General inbox event/ACK/snapshot ordering; gap repair for all retained roots; initial-acceptance signal; persistent unknown-outcome lifecycle; paint guard for other panels; approval pre-paint key gap (pre-existing, approval owner); queue Undo lost-ACK; projection_floor.

## Change Envelope
- Target: crates/oc-tui (history.rs HistoryRow, messages.rs UserMessageTarget, app.rs TuiPanel + press receipt, app/inbox.rs, app/input.rs, app/transcript.rs if needed, dialog.rs, views.rs), crates/oc/src/tui_cmd.rs (InboxChanged handler), crates/oc-adapters/src/storage_inbox.rs (inbox_set_delivery), their nearest tests, pty_t39 interaction test.
- Forbidden: wire bump, new deps, migrations, general refactors.

## Current Checkpoint
- Closes: R5
- Smallest next action: commit the reviewed implementation and this contract, push to origin/master, then record delivery evidence.
- Expected evidence: origin/master contains the implementation commit; only foreign changes remain.
- Stop or replan if: normal push is rejected or foreign changes would be affected.

## Current State
- Resolved: R1-R4; current actual diff Review-Sol PASS after fixing its PTY-fixture title-wait finding.
- Last relevant evidence: targeted tests, strict scoped Clippy and diff check PASS. Full offline gate ran once: 2582/2585 passed, 3 failed, 25 skipped (244.904s); it is not a PASS. Changed PTY case now passes nextest 1/1 (0.921s); untouched auth_tui passes isolated 1/1. Untouched repository-root search test still fails its discovery entry cap (10001 > 10000); no search/limit/baseline changes made. Unexecuted gate tail completed separately: doctests (0 cases), debug build/help/version PASS. Logs: `.local/tmp/pending-steer-*.log`.
- Next: normal commit/push; no adjacent search/auth work and no whole-product READY claim.

## Material Decisions
- 2026-11-02: plan v2 approved; no wire change; close-on-dispatch; initial acceptance shows neutral "No longer pending".
- 2026-11-02: modal press receipt uses `Cell` so hiding Pending steer under an approval retires the press during immutable paint, including approval resolution by another client. Other panels keep their existing receipt semantics.

## Checkpoint History
- 2026-11-02: R1-R4 implemented and targeted tests PASS (TUI 14, guards 7, storage 12, binary 135, PTY 1). Strict scoped/workspace Clippy PASS. Full gate non-PASS recorded above. Sol confirmed latest correction PASS; only delivery remains.

## Completion
