# Goal: complete subagent reports and truthful shell capture admission

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete
Source: owner evidence, four parallel source audits, corrected plan accepted by
«Делай копию плана в цель и итеративно реализовать и коммит пуш дебаг билд».
Last updated: 2026-10-19
Task: T71 / M8

## Objective

Complete admitted child reports reach durable background notices and the parent
without an artificial report cap. Foreground keeps ordinary tool-output limiting;
UI/status reads remain bounded. Duplicate redaction material cannot falsely reject
shell capture.

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
- R3: complete recovery without rerun or premature success.
  - Source: accepted plan D and source audit of post-canonical MCP cleanup.
  - Acceptance: remove recovery's 8192-character cut; resolve the exact linked
    committed assistant, not latest/history concatenation. Keep first settlement,
    dedup, safe checkpoints and cancel/error/unknown rules. No settlement inside
    Db::commit_turn; unpersisted diagnostics are not claimed recovered. No backfill.
  - Primary evidence: committed-assistant crash-gap and first-writer/reopen tests
    with long UTF-8 text; existing conversion/lineage/checkpoint regressions.
  - Status: verified
- R4: correct shell redaction admission and safe capture cause.
  - Source: accepted independent shell plan E and observed generic capture Io.
  - Acceptance: mcp_redactions removes only empty/exact duplicates, preserving order
    and all distinct secrets. Checked unique-material65536-byte guard stays. Existing
    diagnostic retains first observed boundary/safe code, including facts/lookup/reopen;
    errno only when available. Exit/effects remain truthful, no replay or secrets.
  - Primary evidence: synthetic collector-to-real-Jobs duplicate-heavy regression,
    unique aggregate65536/65537 boundaries and existing capture fault/redaction tests.
  - Status: verified

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
