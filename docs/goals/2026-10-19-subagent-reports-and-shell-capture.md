# Goal: complete subagent reports and truthful shell capture admission

Status: active
Source: owner evidence, four parallel source audits, corrected plan accepted by
«Делай копию плана в цель и итеративно реализовать и коммит пуш дебаг билд».
Last updated: 2026-10-19
Task: T71 / M8

## Objective and Execution Directive

Complete admitted child reports reach durable background notices and the parent
without an artificial report cap. Foreground keeps ordinary tool-output limiting;
UI/status reads remain bounded. Duplicate redaction material cannot falsely reject
shell capture. Deliver checked commits, ordinary push and the default debug build.

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: complete durable terminal reports and TS-compatible FG/BG delivery.
  - Source: accepted plan A/B and owner rejection of arbitrary 2 KiB limits.
  - Acceptance: child_jobs.result stores the full terminal body after execution and
    cleanup; BG persists the full synthetic notice without tool_output/artifact.
    FG uses the existing common limiter once and exact winning stored outcome,
    not an independent memory copy or last16 full results.
  - Primary evidence: storage settlement/delivery faults and actual FG/BG parent
    requests with an admitted report over 64 KiB and tail sentinel.
  - Status: verified
  - Evidence: children16 and child-runtime7 pass; actual >64KiB BG user-message
    and FG artifact/read cases pass. The integration target exposed three obsolete
    inventory-body assertions; these now read canonical notice/child history. The
    last Location case passes through its original cross-Location ReadChild fence.
- R2: bounded projections and truthful resource admission.
  - Source: accepted plan C; A10 and existing context/security guards.
  - Acceptance: inventory/refresh carries identity/history links, not report copies;
    large result does not hide an otherwise fitting card. Full bodies are processed
    individually. Notice input respects the existing aggregate active-context budget,
    including escaping; overflow is explicit, not clipping or advancing past input.
    Full result/notice remains durable.
  - Primary evidence: card result over RECORD_BYTES and escape-heavy/overflowing
    notice assertions at the actual parent-input boundary.
  - Status: verified
  - Evidence: fitting cards and bounded refreshes omit report bodies.17storage and
    2context tests pass: whole-page preflight does not skip an oversized prefix;
    an actual escaped request over16MiB is refused without checkpointing rejected
    bodies/IDs, while the complete result/notice and prior read outcome stay durable.
- R3: complete recovery without rerun or premature success.
  - Source: accepted plan D and source audit of post-canonical MCP cleanup.
  - Acceptance: remove recovery's 8192-character cut; resolve the exact linked
    committed assistant, not latest/history concatenation. Keep first settlement,
    dedup, safe checkpoints and cancel/error/unknown rules. No settlement inside
    Db::commit_turn; unpersisted diagnostics are not claimed recovered. No backfill.
  - Primary evidence: committed-assistant crash-gap and first-writer/reopen tests
    with long UTF-8 text; existing conversion/lineage/checkpoint regressions.
  - Status: verified
  - Evidence: full126017-byte canonical crash-gap recovery and91027-byte terminal/
    delivery statement+commit fault/reopen tests pass. Existing checkpoint/unknown,
    conversion/lineage and exact oldest-of18 foreground tests pass without rerun.
- R4: correct shell redaction admission and safe capture cause.
  - Source: accepted independent shell plan E and observed generic capture Io.
  - Acceptance: mcp_redactions removes only empty/exact duplicates, preserving order
    and all distinct secrets. Checked unique-material65536-byte guard stays. Existing
    diagnostic retains first observed boundary/safe code, including facts/lookup/reopen;
    errno only when available. Exit/effects remain truthful, no replay or secrets.
  - Primary evidence: synthetic collector-to-real-Jobs duplicate-heavy regression,
    unique aggregate65536/65537 boundaries and existing capture fault/redaction tests.
  - Status: verified
  - Evidence: actual collector-to-silent-Jobs regression passes (261raw entries become
    10distinct values, exit0, reopenable zero-byte Complete capture). All11capture
    scenarios pass, including real errno, facts/rename faults, cap/quota, interruption,
    split-secret redaction and fail-closed aggregate65536/65537 boundaries. Original
    exit17, effect-once and safe diagnostic survive database reopen without execution.
- R5: verified delivery and debug build.
  - Source: latest owner instruction and accepted checks.
  - Acceptance: targeted checks and bash scripts/test.sh pass; normal
    cargo build -p oc2 --bin oc2 --locked builds target/debug/oc2; reviewed commits
    and ordinary push to verified origin/master; factual evidence/task closeout.
  - Primary evidence: actual commands/exits, Git refs and evidence/T71/report.md.
  - Status: in_progress
  - Evidence: contract and first implementation commits are pushed; final subagent
    target44/44 passes28.50s. Full offline gate, current debug binary and closeout pending.

### Constraints and Non-goals

- One native Db/resource owner; immutable RAW/original assistant; exact captured
  identity/source/permissions, parent-child narrowing and no unknown-effect replay.
- Keep real generation/context/model/transport/security guards. No replacement
  16/64 KiB BG report cap or global preview/UI budget increase.
- Current TS delivery oracle: .local/opencode at
  bc5ebf00a4c169ece10f276e6ce02eaf18c326e6 (2.0.31): full Job.output to BG synthetic
  notice; configured common limiter for FG. Other donor pins remain unchanged.
- Technical Completed/finish=length is not semantic success. Preserve warnings and
  outcome distinctions without LLM prose classification.
- No schema/migration/store/event protocol/publication DTO, BG artifact, output-policy
  snapshot/fingerprint, credential registry, paid/live campaign, user config/env edit
  or oc2-dev launch. No T44/T45/T61 resume or whole READY.
- Already delivered notices/RAW and previously clipped terminal receipts are not
  backfilled. Full original child history remains available. Generic historical shell
  Io cannot prove a particular failure stage or redaction dictionary size.

## Change Envelope and Ordered Plan

1. storage_children.rs finish_child_job: full result, existing transaction/first writer
   after cleanup. runtime/children.rs Work/launch/wait_foreground: exact stored outcome,
   no competing memory body. Existing FG rendering/common preparation stays once.
2. child_tool_jobs/deliver_child_notices and Jobs::deliver: identity/history-only UI,
   scan IDs then process one full body at a time. BG message remains complete;
   message_id/subagent_notice remain atomic. Existing identity budgets unchanged.
3. recover_child_jobs: full exact assistant per job; existing checkpoint/unknown rules.
   storage_shell_jobs.rs shell_notices_after and runtime/turn.rs: bounded preflight
   plus escaped-input admission under existing aggregate cap, explicit overflow.
4. runtime/mcp.rs stable exact dedup; shell/jobs/output.rs and jobs.rs first-observed
   safe cause and explicit facts/lookup/reopen handling. No exhaustive errno plumbing.
5. Existing storage/runtime/subagent/shell tests: long UTF-8/reopen/first-writer,
   >64KiB actual BG/FG request, large-card, conversion, escaped overflow, redaction
   collection and aggregate boundary/fault checks. No new harness/provider matrix.
6. Targeted checks, reviewed implementation commits, full offline gate, debug build,
   ordinary push and factual closeout. Tests >30s or warmed gate >10min require RECON,
   not relaxed guards/assertions/timeouts. No concurrent Cargo processes.

Allowed supporting artifacts: this goal, GOAL amendment, task/progress registration,
brief current TEST_PLAN/CODE_MAP pointers and evidence/T71/report.md. No dependencies.

## Current Checkpoint

- Closes: R5.
- Next: commit/push the reviewed admission/shell slice, then run the required full
  offline gate/debug build and factual task closeout.
- Evidence expected: workspace gates, default-path debug binary, ordinary push and
  complete R1–R5 evidence, without changing production after successful qualification.

## Current State and Material Decisions

- Base: clean master b7d460e98e61ab4bf86584da88ebf40492398fe1.
- Read-only incident evidence: four results exactly prefix their full assistant at
  2048 bytes; twelve shell jobs retain exit0/Io without capture resource rows.
- BG is synthetic conversation input, not a second generic tool result. Existing
  child_jobs owns settlement; canonical success precedes fallible MCP cleanup.
- R1/R2/R3/R4 verified; final qualification remains in progress.
- Blocker: none observed. No schema/migration or owner restart.
- runtime/turn.rs already exceeded the soft5k guide (5614baseline lines); keep this
  bounded admission change with its existing request owner (5722lines). The next
  natural seam is captured request preparation/admission, not a line-count split.

## Checkpoint History

- 2026-10-19: copied audited plan, froze R1–R5 and started T71; no paused tasks reopened.
- 2026-10-19: reproduced exact2048-byte cuts against120015/91027/126017-byte results.
  Full settlement/recovery and identity-only views are green (16storage/7runtime).
  Actual BG/FG long reports pass. Integration43/44 passed after view-assertion
  corrections; the remaining Location case is now green1/1 without weakening its
  trust guard. Logs: .local/evidence/T71/{children,child-runtime,subagent,location-history}-*.log.
- 2026-10-19: reproduced aggregate admission (old code issued an escaped over-budget
  request) and duplicate collector (261rather than10values). Staged admission and
  collector regressions now pass.17storage/2context and10capture cases are green;
  the remaining new boundary check was an incorrect empty-view expectation, fixed
  to require only the exact native control prefix and zero admitted producer bytes.
  Largest capture case21.36s; no test budget exception. Full qualification pending.
- 2026-10-19: corrected aggregate control-view assertion passes0.70s; full current
  subagent target44/44 passes28.50s. R1–R4 targeted evidence is green. No production
  change was needed for the control-view assertion; full offline gate is next.

## Completion

- R1/R2/R3/R4 verified; R5 pending closure. No final PASS/delivery claim yet.
