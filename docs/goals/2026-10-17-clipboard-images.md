# Goal: Clipboard PNG input

Status: complete
Source: owner request and approved RECON plan, 2026-10-17; implementation/commit/push/debug build explicitly requested.
Last updated: 2026-10-17

## Objective

Linux TUI clipboard PNG becomes a highlighted `[Image N]` draft attachment,
actual provider image input and durable user message with `file clipboard` chip.
This is a T44 slice, not whole-task parity/READY.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and
Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements
from reviews, tests, tools, speculative risks, or optional source text. Finish when
every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: Bounded explicit Linux clipboard read, effective `prompt.paste` (Ctrl+V),
  text fallback, empty bracketed paste, stale-result/cancel/shutdown guards and
  pending-paste submit barrier.
  - Source: approved plan steps 2–3.
  - Acceptance: PNG/text reach only the captured editable chat draft; failed or
    obsolete reads do not alter it; helper processes are joined.
  - Primary evidence: focused clipboard process and TUI routing tests.
  - Status: verified
  - Evidence: helper caps/deadline/cancel/reap and pending-Enter/stale-edit join
    tests PASS; actual ELF PNG/text fallback, bracketed paste, restart/no-replay
    scenario PASS in 2.705s (`/tmp/opencode/clipboard-lifecycle-pty.log`).
- R2: Atomic image marker editing and submission snapshot.
  - Source: approved plan steps 1, 4.
  - Acceptance: multiple/image-only prompts, selection, Unicode, delete,
    Undo/Redo, pending receipt and retained Location drafts preserve attachments.
  - Primary evidence: editor/TUI tests with tiny PNG fixtures.
  - Status: verified
  - Evidence: atomic Unicode marker delete/Undo/Redo, opaque draft restoration,
    multiple images, trim offsets, pending receipt/chips and text-only recall
    tests PASS (`/tmp/opencode/clipboard-targeted.log`).
- R3: Validated image input is durable before acknowledgement and reaches the
  existing provider wires without counting base64 as text tokens.
  - Source: approved plan step 5.
  - Acceptance: PNG/ranges/capabilities/budgets fail before effects; initial
    journal and user anchor share acceptance transaction; old text API works.
  - Primary evidence: atomic admission and captured fake-provider tests.
  - Status: verified
  - Evidence: fresh/existing accepted callbacks inspect the committed journal
    through an independent read-only SQLite connection before provider/instruction
    checkpoint; exact image input reaches fake Responses. Refusal/accounting
    test PASS; combined admission/lifecycle test PASS in 1.015s (nextest
    `02f82376-13c1-4516-8827-a5c3bdb06563`).
- R4: Live/reopened transcript chips and original image recovery.
  - Source: approved plan step 6.
  - Acceptance: history projects metadata only; continuation/restart preserve
    images; Revert/Fork recover selected original bytes after RAW/HOT retirement.
  - Primary evidence: one durable media lifecycle fixture plus TUI assertions.
  - Status: verified
  - Evidence: same lifecycle test checkpoints a valid closed response on an
    admitted started turn, seals RAW, forgets images from HOT, reopens and restores
    exact selected Fork/Revert PNG bytes. TUI chips and actual ELF restart pass.
- R5: Actual binary and delivery.
  - Source: approved plan step 7 and owner implementation instruction.
  - Acceptance: bounded PTY fake-clipboard/wire/restart proof; narrow paired
    draft/sent visual qualification at 80/120; checks, debug build, commit/push.
  - Primary evidence: PTY/capture artifacts, gate output and Git delivery.
  - Status: verified
  - Evidence: fresh isolated 80/120 draft/sent captures pass both binaries'
    PNG/wire/natural-exit checks in `.local/evidence/clipboard-images/fresh/`.
    All eight unmasked full-grid/PNG comparisons remain DIFFERENT; cursors match.
    Required image marker/chip styling agrees on inspected frames. Final
    `scripts/test.sh` exits 0: 2062 PASS/0 FAIL/12 original opt-in ignores,
    fmt/strict Clippy/doctests/debug build/help/version pass. Implementation
    `a4b456652d472e3d863abf390284056ca7b5e315` pushed to verified origin/master.
    Factual receipt: `evidence/T44/clipboard-images-20261017.md`.

### Constraints

- Existing text/attachment/active-context/request limits, capability metadata,
  one runtime/Db owner, RAW immutability, DCP forgetting and no unknown replay.
- No SQLite schema migration, new blob store, crate dependency or clipboard poll.
- Tests use tiny images/readiness, no sleeps or paid generation; existing test
  budgets and the separately recorded unmet soak budget remain truthful.
- Initial HEAD: `54d1b3f58763e9814e11d5f8d4c083ac07e58b99`; clean worktree,
  active T44, verified `github.com/0FL01/oc` origin, non-root execution.

### Non-goals

- Non-PNG clipboard images, previews, PDF/files/drop/URL acquisition, other OSes,
  remote clipboard protocol, image deduplication, CLI `--image`, Shell/command
  attachments, structured durable Up/Down input history, global parity repinning.
- Donor `.local/opencode` is 2.0.31 `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6`;
  it is supplementary evidence, not a replacement of pinned 2.0.12 acceptance.

## Change Envelope

- Existing core submission DTO/inbox/receipts and query DTOs; adapters application,
  runtime image admission/accounting, existing turn JSON/RAW/history/Revert/Fork;
  existing editor/input/live/history rendering and binary clipboard/job routing.
- Local responsibility-specific modules/tests and narrow existing PTY/capture
  extensions; CODE_MAP and factual goal/evidence/progress updates.
- Linux `wl-paste` or `xclip`, explicit invocation only, 1s total read deadline;
  no automatic installation or user-config/desktop clipboard mutation.

## Current Checkpoint

- Closed: R1–R5. No further feature implementation or qualification is pending.
- Existing successful gate, paired captures and implementation push are recorded
  in the factual receipt; whole T44 and the unrelated soak objective stay open.

## Current State

- Resolved: R1–R5; all new scenarios pass, including exact RAW recovery and delivery.
- Last relevant evidence: final `scripts/test.sh` exits 0, 399s whole / 362.924s
  nextest, run `218c6aa3-df14-487a-b3a6-b378af88ee8f`, 2062 PASS/0 FAIL/12 original
  ignores; fmt/strict Clippy/doctests/debug build/help/version pass. Seven new
  scenarios take 0.009–3.003s each. Fresh visual pairs pass draft/sent/real-PNG/
  natural-exit checks at 80/120. Full styled-cell differences are 50/22 (80
  draft/sent) and 50/21 (120 draft/sent), cursors equal. Earlier failures remain
  in separate logs; one failure trigger was moved back after atomic admission,
  and three PTY fixtures now observe idle before idle-only actions. Assertions,
  timeouts and production guards are unchanged. Soak is still over budget
  (33.580s); UI07's 32.471s fits its existing owner-approved 40s exception.
- Blocker: none established for implementation.
- Next: stop this objective. T44 parity and the unrelated soak-budget objective
  remain separate and incomplete.

## Material Decisions

- 2026-10-17: reuse provider InputImage and turn JSON/RAW; no new media store.
- 2026-10-17: retain text-only durable prompt history; explicit Revert/Fork and
  unfinished draft restoration carry images. Slash/Shell image input refuses.
- 2026-10-17: supplementary 2.0.31 capture uses a fixture-private headless KWin
  and PNG owner, never the user's desktop clipboard. Clipboard/prompt/attachment
  release sources match the supplied donor; session-route delta is only unrelated
  copy commands. Global pinned 2.0.12 capture admission is unchanged.

## Completion

- R1–R5 verified: current-source functional gate succeeds and the reviewed
  implementation is pushed. Debug executable built by the same gate is ready.
- Scope/constraints reviewed: no new schema/store/dependency or user data/config
  mutation; source/RAW/security limits remain. Later closeout edits are factual
  documentation/progress only and do not invalidate runtime evidence.
- Final status: complete for clipboard PNG input only. Full-frame parity,
  whole T44 and unrelated soak-budget qualification remain incomplete.
