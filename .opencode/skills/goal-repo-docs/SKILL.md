---
name: goal-repo-docs
description: Create or update a minimal repo-local execution contract for one durable, resumable objective with a measurable finish line. Use for explicit multi-stage work whose next action depends on evidence learned during execution. Do not use for one-turn tasks, routine reviews, open-ended improvement, unrelated backlog items, or merely because a PRD or PLAN exists.
---

# Goal Repository Contract

Create a small repo-local document that helps a coding agent finish one durable objective across sessions.

The user request and cited source specification remain authoritative. The repository document is an operational snapshot, not a new product specification. It records the frozen finish line, boundaries, evidence, current state, and next checkpoint. It must not invent requirements, risks, or work.

The repository document is the durable execution state. Do not rely on hidden conversation memory to determine scope, status, completion, or the next action.

## Core Rules

Create goal documents only in Build* modes. In Plan mode, prepare the plan without creating a goal; existing goal documents are read-only.

1. **Freeze the finish line before implementation.** Record required outcomes, affected constraints, non-goals, change envelope, and primary evidence.
2. **Evidence cannot create requirements.** Tests, fixtures, synthetic reproductions, reviews, tools, and subagents may verify an existing requirement but cannot turn a hypothesis into one.
3. **Use minimum sufficient evidence.** Prefer one direct evidence surface per required outcome. Add another only when the first cannot prove it.
4. **Every checkpoint must move the objective.** It must close a required outcome or produce concrete evidence that changes the next in-scope action. Two consecutive checkpoints that do neither are a stop condition.
5. **Discoveries do not expand scope by default.** An adjacent issue or review finding is not objective work unless it is an omitted explicit source requirement or a regression caused by the current diff.
6. **Completion is terminal.** Once the closure check passes, mark the document complete and stop. Do not begin cleanup, hardening, extra coverage, architecture review, or another audit.

Authority order: latest explicit user instruction, cited source specification, applicable repository instructions and existing contracts, then this document.

## Fit Gate

Use this skill only when there is one durable objective, an observable end state, concrete completion evidence, clear boundaries, and expected multi-stage or resumable work.

Use a normal task workflow for one-off edits, explanations, small fixes, routine reviews, or work that can be completed and verified in one turn. Do not create a durable contract for “improve,” “clean up,” “make robust,” “productionize,” or “refactor” without a fixed observable result. Split independently shippable outcomes instead of absorbing a whole backlog.

If ambiguity does not change behavior or scope, use the narrowest compatible interpretation and record it. If it changes the desired outcome or allowed boundary, keep the document in `draft` and record the smallest blocking decision. Do not invent product intent.

Before creating a document, look only in the repository's normal planning locations for an existing `draft`, `active`, or `blocked` contract with the same objective. Update the nearest matching document instead of creating a duplicate. Create a new document only when the user requests durable task tracking or the work clearly requires a resumable execution contract.

## Document Status

The status in the repository document is authoritative for execution state:

- `draft`: the finish line cannot yet be frozen because a material input or boundary is missing.
- `active`: the contract is frozen and at least one required outcome remains unresolved.
- `blocked`: no approved in-scope action can progress an unresolved outcome without external input or state change.
- `complete`: every required outcome is resolved with current evidence and the closure check passed.
- `unmet`: the requested finish line cannot be achieved inside the authoritative constraints or approved change envelope.

Changing status never changes the source requirements. Do not use `complete` merely to stop work, and do not use `blocked` while an approved in-scope action with a falsifiable expected result remains.

## Minimal Preflight

Read the user-named source first, then only the nearest applicable repository instructions, relevant package or task file, directly relevant docs, and CI job that defines a required gate. Stop discovery when the contract can be frozen; do not inventory the repository for completeness.

Reuse the repository's documentation convention. Otherwise store each goal document at `docs/goals/<YYYY-MM-DD>-<slug>.md` relative to the project root (e.g., `docs/goals/2026-07-21-reduce-project-complexity.md`); a bare `GOAL.md` filename is forbidden. Never store secrets, credentials, private dumps, generated databases, or sensitive logs in it.

## Freeze the Contract

### Required outcomes: `R*`

Create an `R*` item only for an explicit user outcome, acceptance criterion, mandatory deliverable, or mandatory behavior from the authoritative source.

Do not promote examples, options, future work, stretch goals, advisory text, or generic qualities such as robust, clean, scalable, secure, maintainable, production-ready, and future-proof unless the source makes them measurable and mandatory.

Each item contains:

- `Source`: exact instruction or source section.
- `Acceptance`: one observable end state.
- `Primary evidence`: the smallest direct command, artifact, runtime observation, or explicit user acceptance.
- `Status`: `pending`, `in_progress`, `verified`, `blocked`, `waived_by_user`, `not_applicable`, or `superseded`.
- `Evidence`: filled after verification or status resolution.

The agent cannot waive a requirement. `not_applicable` needs evidence that a source condition does not hold. `superseded` needs a newer authoritative instruction.

### Constraints and non-goals

Record only explicit constraints that the current diff can affect. Validate one separately only when the change touches it. List non-goals as boundaries, not audit items; they need no individual statuses or dedicated tests.

After the contract is frozen and implementation begins, broaden it only when the user changes scope, an explicit source requirement was accidentally omitted with an exact citation, or the current diff directly regresses an existing contract. Reviews, tests, validators, subagents, best practices, and imagined edge cases do not qualify.

### Change envelope

Record the target behavior or artifact, expected files and symbols and direct consumers, allowed and forbidden artifact categories, and any user or harness limits on paths, files, diff size, dependencies, tools, cost, or time.

Expand the envelope only when a concrete blocker to an unresolved `R*` proves that the smallest correct change crosses it. Record the affected `R*`, evidence, and minimal expansion before editing.

A new dependency, service, worker, queue, background job, persistent store, migration, public API, auth layer, retry framework, generic abstraction, or cross-package redesign is material expansion unless explicitly required by the frozen contract. A user- or harness-supplied change budget is a hard boundary.

## Execution Directive

Use this directive in the repository document unless the source already provides a stricter compatible one:

```text
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker, an approved budget boundary, or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.
```

## Execution Loop

Keep one current checkpoint targeting one required outcome or a tightly coupled set. One checkpoint is not permission for a broad batch; its diff must be the smallest change that can satisfy the listed outcome.

1. Choose the smallest in-envelope change or experiment with a falsifiable expected result.
2. Make the logically related edits.
3. Run the primary relevant evidence once.
4. Update the affected status, evidence, and next checkpoint.
5. Continue only when the result exposes a concrete in-scope cause and the next action is materially different.

Do not create checkpoints for generic exploration, cleanup, documentation, refactoring, test expansion, architecture review, or hardening unless one is itself a frozen outcome.

Validation order: existing targeted check; affected package check; minimal runtime or manual check; broader workspace gate only when the frozen contract or changed dependency surface requires it.

Add or change a test only for a frozen outcome, reproduced regression, or explicit test requirement. A new failing test cannot establish a new requirement. Do not rerun an unchanged failed command or a successful check after unrelated edits. Investigate unrelated failures only far enough to determine whether the current diff caused them or they belong to a required gate.

A review, validator, test, or subagent may trigger another edit only when it proves an original outcome is unsatisfied, the current diff caused a regression, or an existing mandatory gate for the changed surface fails because of the diff. It cannot open a new class of work.

Update the document only after a completed checkpoint, material contract decision, or blocker. Do not log every read, grep, edit, or command. Replace stale current-state text and keep a short checkpoint history.

A blocker report contains affected `R*` IDs, exact failing command or missing external state, concise output, smallest attempted in-scope action, why no approved action remains, and the smallest unlock. A budget limit or proven blocker stops substantive work but never proves completion. Do not continue merely because another safe search, read, or speculative experiment exists.

Do not create commits unless the user, source contract, or applicable repository instruction requires them. Use no subagents by default; delegate only when at least two independent workstreams are necessary for frozen outcomes, use one round, and do not let delegated output expand scope.

## Closure Check

Run one closure check after the final implementation edit. It compares current state to the frozen contract; it is not a discovery phase.

Completion requires all of the following:

- every `R*` is `verified`, `waived_by_user`, `not_applicable`, or `superseded` with evidence;
- no item remains `pending`, `in_progress`, or `blocked`;
- evidence affected by the final diff is current and successful;
- affected constraints remain satisfied;
- the diff stays inside the approved change envelope.

When all conditions hold, set the document status to `complete`, fill `Completion`, and stop.

When no approved in-scope path remains because of external state, set status to `blocked` and record the exact evidence and smallest unlock. When the requested finish line is impossible inside authoritative constraints or the approved envelope, set status to `unmet` and record the conflicting requirement or boundary. Never use either state to hide unfinished work that still has an approved, falsifiable next action.

## Minimal Document Template

```markdown
# Goal: <title>

Status: draft | active | blocked | complete | unmet
Source: <file / issue / user instruction>
Last updated: <YYYY-MM-DD>

## Objective
<One observable end state.>

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R1: <outcome>
  - Source:
  - Acceptance:
  - Primary evidence:
  - Status: pending
  - Evidence:

### Constraints
- C1: <explicit affected constraint>

### Non-goals
- <explicit exclusion>

## Change Envelope
- Target:
- Expected paths, symbols, and direct consumers:
- Allowed and forbidden artifacts:
- User or harness budget:

## Current Checkpoint
- Closes: <R-IDs>
- Smallest next action:
- Expected evidence:
- Stop or replan if:

## Current State
- Resolved:
- Last relevant evidence:
- Blocker:
- Next:

## Material Decisions
- <date>: <scope or boundary decision>

## Checkpoint History
- <date>: <R-IDs, result, evidence, next>

## Completion
- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
```

The document is sufficient when another session can identify the frozen finish line, trusted evidence, current state, and next action without replaying the conversation. More narrative is not inherently better.
