# Goal: subagent empty optional arguments and admitted continuation scheduling

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete
Source: owner bug report and audited plan, accepted 2026-10-18 by
«делай копию плана в цель и итеративно реализовать и коммит пуш, билд дебаг».
Last updated: 2026-10-18
Task: T69 / M8

## Objective

The reported `subagent` call with `sessionID: ""`, `model: ""`, `background: false`
and `context_message_ids: []` creates a real fresh child with normal model selection.
Explicit continuation, concurrency, admission and original history remain correct.

## Frozen Contract

### Required Outcomes

- R1: empty optional arguments mean absence, not an empty lookup.
  - Source: reported reproduction and accepted plan A.
  - Acceptance: omitted or exactly empty `sessionID` creates a fresh child; exactly
    empty `model` selects no explicit override. Profile→parent selection for fresh
    children and saved-child continuation selection remain unchanged. Nonempty
    strings, including whitespace, are passed unchanged; null/wrong types remain
    invalid. The existing validator is reused without new rules.
  - Primary evidence: compact parser table and one fake-Responses runtime fresh
    regression with the complete reported argument shape, nonempty child ID,
    parent-model fallback and exact durable/replayed original arguments.
  - Status: verified
- R2: only admitted continuations participate in same-child scheduling.
  - Source: accepted plan B, including its explicit ownership correction.
  - Acceptance: normalized absent IDs do not share a queue; nonempty same-child
    continuations remain ordered and join actual execution after FG→BG conversion.
    Refused leaves neither join nor replace a predecessor nor touch another child.
  - Primary evidence: existing foreground overlap/reverse-result/one-effect test
    with empty fields; saved-model continuation and background fixtures; one held
    foreign-child regression with two refused continuation leaves.
  - Status: verified
- R3: preserve optional schemas on the Responses wire.
  - Source: accepted plan C and newer donor `open-responses.ts::lowerTool`.
  - Acceptance: both Responses constructors emit `strict: false` on all ordinary
    function definitions without rewriting parameters. Subagent `required` stays
    `["agent", "description", "prompt"]`; the shared WS body preserves this policy.
  - Primary evidence: assertions in existing PROV01, runtime subagent-schema and
    nonempty Key/OAuth WS runtime captures.
  - Status: verified

### Constraints and non-goals

- Preserve permissions, parent-child narrowing, depth/context/resource limits,
  cancellation/cleanup, result formats, model precedence and immutable RAW/replay.
- No new whitespace rejection/trim, native session-prefix constraint, general
  nullable repair, description-required correction or validation framework.
- No new `multi_tool_use.parallel` dispatcher, Jobs/store/schema/migration, config
  switch, provider/model-name routing, Chat/Messages changes or JS runtime.
- No new CLI fresh/resume fixture, release qualification, paid/live campaign,
  user-config/data/env changes or `oc2-dev` launch. Existing binary tests run in the
  full gate. T44/T61 remain paused; T45 detailed acceptance ownership/live blocker
  is unchanged. Reuse SUB01/SUB02 checks without claiming whole-task completion.

## Source notes

- Donor oracle for this correction: `.local/opencode` at
  `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6`, empty-field hook and Responses lowering.
  Existing donor/TUI/config pins and historical evidence are not rewritten.
- Incident schema transformation remains unproven without its original wire;
  explicit non-strict policy is independently supported by donor and Responses docs.
