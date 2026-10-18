# Goal: subagent empty optional arguments and admitted continuation scheduling

Status: complete
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
  - Status: verified
  - Evidence: baseline reproduced `Some("")`, unvalidated null and the exact blank-ID
    error. Both parser tests and the complete fresh runtime regression now pass;
    actual durable/replayed arguments retain both empty fields. Logs under
    `.local/evidence/T69/`: `parser-red.log`, `fresh-red.log`,
    `tools__tests__subagent-green.log`, `subagent-green.log`.
- R2: only admitted continuations participate in same-child scheduling.
  - Source: accepted plan B, including its explicit ownership correction.
  - Acceptance: normalized absent IDs do not share a queue; nonempty same-child
    continuations remain ordered and join actual execution after FG→BG conversion.
    Refused leaves neither join nor replace a predecessor nor touch another child.
  - Primary evidence: existing foreground overlap/reverse-result/one-effect test
    with empty fields; saved-model continuation and background fixtures; one held
    foreign-child regression with two refused continuation leaves.
  - Status: verified
  - Evidence: parser-only baseline still failed fresh overlap (5.97s) and foreign
    refusal (4.38s). With admitted normalized keys, all 43 subagent tests pass
    (25.734s suite; max 21.403s case), including existing concurrency/model/BG
    guards and the held foreign child surviving the other parent's cancellation.
    Actual FG→BG join unit also passes (0.65s). See `subagent-green.log` and
    `foreground_conversion_releases_only_waiter_and_retains_actual_join_and_fences-green.log`.
- R3: preserve optional schemas on the Responses wire.
  - Source: accepted plan C and newer donor `open-responses.ts::lowerTool`.
  - Acceptance: both Responses constructors emit `strict: false` on all ordinary
    function definitions without rewriting parameters. Subagent `required` stays
    `["agent", "description", "prompt"]`; the shared WS body preserves this policy.
  - Primary evidence: assertions in existing PROV01, runtime subagent-schema and
    nonempty Key/OAuth WS runtime captures.
  - Status: verified
  - Evidence: missing wire policy reproduced in PROV01 and nonempty WS captures;
    after both lowering edits, PROV01 passes (0.00s), Key/OAuth WS initial/follow-up
    assertions pass (2.91s), and runtime schema still has exactly three required
    fields. Logs: `provider__tests__prov01_exact_url_auth_store_cachekey-green.log`,
    `provider__websocket__tests__auth04_runtime_channels_settle_read_followup_fork_and_partition_opaque_authority-green.log`
    and `subagent-green.log` under `.local/evidence/T69/`.
- R4: verified, committed/pushed delivery and debug build.
  - Source: latest owner instruction and accepted verification plan.
  - Acceptance: targeted checks and `bash scripts/test.sh` pass; normal
    `cargo build -p oc2 --bin oc2 --locked` produces `target/debug/oc2`; reviewed
    source and factual closeout are committed and normally pushed to verified
    `github.com/0FL01/oc` origin on the current branch.
  - Primary evidence: actual command exits, test counts, Git commits/upstream and
    `evidence/T69/report.md`.
  - Status: verified
  - Evidence: targeted and settled full offline gates pass (2089/2089, 12 existing
    skips). The warmed complete gate takes 399.34s, nextest 389.131s; the unchanged
    storage matrix passes in 13.032s. Only UI07 exceeds 30s (32.669s), within its
    existing approved 40s exception; all ordinary cases are <=27.920s. Default-path
    debug build/help/version pass. Reviewed implementation `df2358176` is normally
    pushed to verified origin/master; report and generated finish leaf record the
    associated factual closeout. See `evidence/T69/report.md` and Git delivery.

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

R4 evidence-driven envelope extension (2026-10-18): the existing 19-fixture storage
safety matrix took 33.059s among concurrent tests, 13.01s alone. Owned-process syscall
timing measured 1361 `fsync` calls taking 13.032s (96.38% of traced syscall time),
confirming durable-I/O contention rather than an intentional measurement window.
Allow only its exact filter in the existing full-pool `.config/nextest.toml` override;
preserve every fixture, assertion, WAL/FULL setting and production storage path.

## Current Checkpoint

- Closed: R1–R4; no remaining implementation checkpoint.
- Evidence: targeted and full workspace gates, qualified timing, default-path debug
  binary and reviewed Git delivery; factual receipt is `evidence/T69/report.md`.
- No further work within this bounded correction.

## Current State and Material Decisions

- Native base: clean `02ffba50c25549f6653f2756a68cffd8a71383bb` on `master`.
- Donor oracle for this correction: `.local/opencode` at
  `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6`, empty-field hook and Responses lowering.
  Existing donor/TUI/config pins and historical evidence are not rewritten.
- Incident schema transformation remains unproven without its original wire;
  explicit non-strict policy is independently supported by donor and Responses docs.
- T69 is a bounded correction: completed prerequisites are not reopened, and the
  broader blocked T45 live campaign is not resumed or re-authorized.
- Contract copied, registered and pushed in `5914d9458`; the associated T69 finish
  leaf closes only this correction, without changing other task blockers.
- Parser, admitted-only continuation scheduling and both Responses maps are fixed
  in reviewed implementation `df2358176`, pushed to origin/master. Targeted
  red→green evidence covers each owner; all 43 subagent integration tests pass.
  Both full gates pass: 2089 executed / 12 existing skips. The debug binary is built.
- Full wall time 772.08s includes 4m43s test rebuilding and 1m25s dev rebuilding;
  it is not a warmed-gate measurement. Storage timing RECON is resolved: the matrix
  passes in 13.032s in the warmed gate, versus 33.059s with neighbors and 13.01s
  alone. UI07 takes 32.669s within its existing approved <=40s exception; the settled
  warmed full gate completes in 399.34s. No unapproved timing violation remains.
- Blocker: none observed.

## Checkpoint History

- 2026-10-18: R1 baseline reproduced with the exact user-facing blank-ID error
  (0.88s runtime test, compilation excluded). Two parser regressions red; the next
  change reuses existing validation and normalizes only exact empty strings.
- 2026-10-18: R1–R3 targeted checks green after minimal edits to three production
  files. Parser-only experiment exposed both scheduler defects and the two missing
  Responses policies before their fixes. Full gate is the next R4 checkpoint.
- 2026-10-18: R4 full gate functional PASS, debug build/help/version PASS. Mandatory
  budget RECON narrowed storage matrix slowness from 33.059s with neighbors to
  13.01s without them; no assertions, deadlines or durability settings changed.
- 2026-10-18: R4 syscall timing confirms 1361 `fsync` calls dominate the unchanged
  storage matrix. The minimal envelope extension is one existing scheduler filter,
  not production storage initialization changes, fewer fixtures or weaker durability.
- 2026-10-18: R4 unchanged storage matrix passes in 13.032s in the warmed gate with
  the full-pool filter; all fixtures and guards remain intact. Whole-gate completion
  and Git delivery are the remaining checkpoint.
- 2026-10-18: R4 settled full gate PASS, 2089/2089 with 12 existing opt-in skips;
  whole warmed gate 399.34s, budgets qualified, debug build/help/version PASS.
  Reviewed implementation `df2358176` committed and normally pushed; factual
  report and generated T69 finish leaf close the bounded contract.

## Completion

- Resolved outcomes: R1–R4 verified; no remaining required outcome or known blocker.
- Commands/artifacts: targeted parser/provider/FG-to-BG checks, all 43 subagent
  cases, both `bash scripts/test.sh` runs and `target/debug/oc2`; detailed command
  receipts and timing RECON are in `evidence/T69/report.md`.
- Git: plan `5914d9458`, reviewed implementation `df2358176`, associated factual
  closeout; ordinary pushes to verified `github.com/0FL01/oc` origin/master.
- Constraint/scope check: three production owners plus planned regressions and one
  evidence-driven exact nextest scheduling filter; RAW, model/permission/ownership
  and durability guards preserved. No live/user-data/migration/wrapper expansion.
- Final status: complete for T69 only, not whole-product READY or T45 live PASS.
