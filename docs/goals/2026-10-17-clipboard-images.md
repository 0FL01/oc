# Goal: Clipboard PNG input

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete
Source: owner request and approved RECON plan, 2026-10-17; implementation/commit/push/debug build explicitly requested.
Last updated: 2026-10-17

## Objective

Linux TUI clipboard PNG becomes a highlighted `[Image N]` draft attachment,
actual provider image input and durable user message with `file clipboard` chip.
This is a T44 slice, not whole-task parity/READY.

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
- R2: Atomic image marker editing and submission snapshot.
  - Source: approved plan steps 1, 4.
  - Acceptance: multiple/image-only prompts, selection, Unicode, delete,
    Undo/Redo, pending receipt and retained Location drafts preserve attachments.
  - Primary evidence: editor/TUI tests with tiny PNG fixtures.
  - Status: verified
- R3: Validated image input is durable before acknowledgement and reaches the
  existing provider wires without counting base64 as text tokens.
  - Source: approved plan step 5.
  - Acceptance: PNG/ranges/capabilities/budgets fail before effects; initial
    journal and user anchor share acceptance transaction; old text API works.
  - Primary evidence: atomic admission and captured fake-provider tests.
  - Status: verified
- R4: Live/reopened transcript chips and original image recovery.
  - Source: approved plan step 6.
  - Acceptance: history projects metadata only; continuation/restart preserve
    images; Revert/Fork recover selected original bytes after RAW/HOT retirement.
  - Primary evidence: one durable media lifecycle fixture plus TUI assertions.
  - Status: verified
- R5: Actual binary and delivery.
  - Source: approved plan step 7 and owner implementation instruction.
  - Acceptance: bounded PTY fake-clipboard/wire/restart proof; narrow paired
    draft/sent visual qualification at 80/120; checks, debug build, commit/push.
  - Primary evidence: PTY/capture artifacts, gate output and Git delivery.
  - Status: verified
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
  it is supplementary evidence for that scoped check. Old seals below remain
  provenance of their own runs.

## Material Decisions

- 2026-10-17: reuse provider InputImage and turn JSON/RAW; no new media store.
- 2026-10-17: retain text-only durable prompt history; explicit Revert/Fork and
  unfinished draft restoration carry images. Slash/Shell image input refuses.
- 2026-10-17: supplementary 2.0.31 capture uses a fixture-private headless KWin
  and PNG owner, never the user's desktop clipboard. Clipboard/prompt/attachment
  release sources match the supplied donor; session-route delta is only unrelated
  copy commands. Old capture admission records below are historical provenance.
