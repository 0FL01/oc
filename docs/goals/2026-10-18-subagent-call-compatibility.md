# Goal: subagent empty optional arguments and admitted continuation scheduling

Status: active
Source: owner bug report and audited plan, accepted 2026-10-18 by
«делай копию плана в цель и итеративно реализовать и коммит пуш, билд дебаг».
Last updated: 2026-10-18
Task: T69 / M8

## Objective

The reported `subagent` call with `sessionID: ""`, `model: ""`, `background: false`
and `context_message_ids: []` creates a real fresh child with normal model selection.
Explicit continuation, concurrency, admission and original history remain correct.
Deliver verified code, a debug binary, commits and an ordinary push.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

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
  - Status: pending
  - Evidence: not run.
- R2: only admitted continuations participate in same-child scheduling.
  - Source: accepted plan B, including its explicit ownership correction.
  - Acceptance: normalized absent IDs do not share a queue; nonempty same-child
    continuations remain ordered and join actual execution after FG→BG conversion.
    Refused leaves neither join nor replace a predecessor nor touch another child.
  - Primary evidence: existing foreground overlap/reverse-result/one-effect test
    with empty fields; saved-model continuation and background fixtures; one held
    foreign-child regression with two refused continuation leaves.
  - Status: pending
  - Evidence: not run.
- R3: preserve optional schemas on the Responses wire.
  - Source: accepted plan C and newer donor `open-responses.ts::lowerTool`.
  - Acceptance: both Responses constructors emit `strict: false` on all ordinary
    function definitions without rewriting parameters. Subagent `required` stays
    `["agent", "description", "prompt"]`; the shared WS body preserves this policy.
  - Primary evidence: assertions in existing PROV01, runtime subagent-schema and
    nonempty Key/OAuth WS runtime captures.
  - Status: pending
  - Evidence: not run.
- R4: verified, committed/pushed delivery and debug build.
  - Source: latest owner instruction and accepted verification plan.
  - Acceptance: targeted checks and `bash scripts/test.sh` pass; normal
    `cargo build -p oc2 --bin oc2 --locked` produces `target/debug/oc2`; reviewed
    source and factual closeout are committed and normally pushed to verified
    `github.com/0FL01/oc` origin on the current branch.
  - Primary evidence: actual command exits, test counts, Git commits/upstream and
    `evidence/T69/report.md`.
  - Status: pending
  - Evidence: not run.

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

## Change Envelope and Ordered Plan

1. `crates/oc-adapters/src/tools.rs::subagent_request`: existing validation followed
   by exact empty→None for both fields; `pub(crate)` for the real scheduler seam.
   Add the parser unit table in `src/tools/tests/subagent.rs`.
2. `runtime/turn/foreground.rs::execute_foreground_children`: normalized key only
   for admitted calls; refused leaves do not enter the continuation dependency map.
   Add the fresh runtime regression in `tests/subagent.rs`; reuse
   `tests/fixtures/{foreground_children,background_children}.rs` for concurrency,
   saved-model and BG cases, original durable/replay arguments and foreign refusal.
3. `provider.rs::{request_body,stream_input_overlaid}`: add `strict: false` in the
   two existing Responses maps. Extend existing HTTP/runtime/WS assertions only.
   No serializer abstraction or `ToolDef` expansion.
4. Run nearest red→green checks, then the full offline gate once the implementation
   is settled. Tests >30s or warmed full gate >10min require owner-phase RECON,
   not higher timeouts/relaxed checks. Review diff, build debug, commit/push, and
   close only this correction task with factual evidence.

Allowed additional artifacts: this goal, the GOAL amendment, task registration,
brief TEST_PLAN/roadmap pointers, generated progress and one factual evidence report.
No new dependencies or production API beyond the actual crate-private parser seam.

## Current Checkpoint

- Closes: R1–R4.
- Smallest next action: save/check the contract, start T69, then add the smallest
  failing parser/runtime regressions before production changes.
- Expected evidence: baseline red reproduces empty lookups/queue behavior; targeted
  green proves the normalized contract without changing nonempty handling.
- Replan only for a concrete in-envelope failing requirement or gate.

## Current State and Material Decisions

- Native base: clean `02ffba50c25549f6653f2756a68cffd8a71383bb` on `master`.
- Donor oracle for this correction: `.local/opencode` at
  `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6`, empty-field hook and Responses lowering.
  Existing donor/TUI/config pins and historical evidence are not rewritten.
- Incident schema transformation remains unproven without its original wire;
  explicit non-strict policy is independently supported by donor and Responses docs.
- T69 is a bounded correction: completed prerequisites are not reopened, and the
  broader blocked T45 live campaign is not resumed or re-authorized.
- Blocker: none observed. Implementation/tests/build/delivery not yet performed.

## Completion

Pending; fill only from actual verification and Git delivery.
