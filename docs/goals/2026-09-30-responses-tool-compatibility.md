# Goal: completed Responses tool calls survive sparse terminal output

Status: active (frozen repair plan; implementation NOT_STARTED)
Source: owner request 2026-09-30: diagnose non-working native application,
collect repair context, ensure an assigned fix and real-API E2E plan.
Last updated: 2026-09-30
Task: T55 (todo); diagnostic evidence: `evidence/T55/diagnosis.md`.

## Objective

Native `oc` accepts a genuinely completed Responses stream containing validated
`output_item.done` tool calls followed by `response.completed` with sparse/empty
`output`, executes the tools only after safe response closure, and completes a
real OpenProxy apply_patch/read/result/final/reopen cycle. Invalid or actually
truncated streams remain truthful failures without unsafe tool dispatch.

This is a repair of existing A04 behavior, not a new provider/protocol. The owner
also requires a real coding workflow: T27 retains E2E02/A09, after T55's current
wire/tool-cycle prerequisite. Neither plan publication nor T55 alone means READY.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and
Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements
from speculative risks or optional source text. Finish T55 when its required
outcomes are verified and affected constraints remain satisfied. Preserve T27's
existing seeded coding finish line; do not claim it from a wire-only smoke.

## Frozen Contract

### Required Outcomes

- **R1 — trustworthy completed-output reconciliation.**
  - Source: owner failure before file tools; GOAL A04/PROV03; live structural
    observations and native differential in `evidence/T55/diagnosis.md`.
  - Acceptance: completed done calls are not discarded by terminal `output:[]`
    or an omitted/partial snapshot. Reconcile independently validated done and
    terminal items in stable output order, without duplicate calls or lost
    messages/opaque reasoning/usage. Require real successful response termination
    and successful response closure before local admission. Never reconstruct a
    call solely from partial deltas or manufacture terminal success.
  - Primary evidence: PROV09 owner tests using normalized fixture, empty/omitted/
    full/partially repeated terminal output, and direct Generation assertions.
  - Status: pending
  - Evidence: current binary reproduces failure; no fix implemented.

- **R2 — validation and diagnostics remain honest.**
  - Source: GOAL A04/A07/A08/A10; T54 typed failure contract and no unknown-effect
    replay; current local validation is indistinguishable from transport EOF.
  - Acceptance: deterministic conflicts/malformed local output have bounded safe
    structural diagnostics and a typed local failure, not generic transient read
    retry. Distinguish actual missing terminal/EOF from failed/incomplete provider
    response and local reconciliation rejection. Validate item ID versus call_id,
    name, final JSON arguments, index/order and duplicate identities across done/
    terminal observations. An identical repeated completed observation may be
    coalesced; conflicting observations may not be silently overwritten. Preserve
    argument/event/request/output caps, existing length/content-filter handling,
    opaque isolation/replay, permission/trust and unknown-effect quarantine.
  - Primary evidence: PROV09 negative owner fixtures + existing PROV03/04/06,
    RET01 and affected safety tests; inspect safe diagnostics for canary leakage.
  - Status: pending
  - Evidence: `provider.rs::read_failure` currently conflates local rejection/EOF.

- **R3 — prove the actual shipped runtime, not only parser success.**
  - Source: owner non-working application; GOAL A04/A07/A08 and TEST_PLAN.
  - Acceptance: rebuilt retained debug/release binaries against deterministic
    fake Responses run apply_patch → read → final in headless and PTY through
    the real application owner. Valid sparse completion has no spurious retry,
    correct call_id/result pairing and one execution per call. Independent file
    bytes and durable tool outcomes prove effects. Reopen/restart preserves
    completed history and does not replay settled or unknown operations. Negative
    unclosed/conflicting streams execute zero tools. Affected quality gates pass.
  - Primary evidence: PROV09 actual-binary request/file/SQLite/PTY receipts, affected
    crate tests, workspace fmt/clippy/tests/build as required by TEST_PLAN.
  - Status: pending
  - Evidence: diagnostic existing-ELF RED/full/omitted differential only; not a
    rebuilt/fixed qualification or visual T44 PASS.

- **R4 — current bounded real-API qualification and coding handoff.**
  - Source: explicit owner request for real-API E2E; GOAL A04/A09; PROV08/E2E02
    and live envelope in AGENT_RUNBOOK/TEST_PLAN.
  - Acceptance: PROV10 actual rebuilt native binary in isolated fixtures completes
    apply_patch → read → function_call_output → final and same-history reopen.
    Qualify the catalog-admitted incident binding `cx/gpt-6-luna`/`high` and the
    explicitly configured `.local/live.env` model/default variant as separate
    bounded runs; no automatic fallback or claim for other models. Verify file
    bytes, durable successful tool operations, continuation IDs and final success,
    not model prose or a direct SSE probe. Continue the existing diagnosis campaign
    without resetting its <=24 physical generation allowance (4 already consumed),
    including title/retries/children/compaction. Establish the actual binary's
    enforced relay integration offline before external dispatch. Hand off build,
    exact selection, counters and receipts to T27 for seeded Rust E2E02/A09.
    T27's workflow must pass before full product readiness; T55 cannot close it.
  - Primary evidence: PROV10 safe native relay/request/operation/file/reopen report,
    continuing durable ledger; T27 separately produces E2E02 evidence.
  - Status: pending
  - Evidence: 4 direct generation probes diagnosed wire; native real-API tool
    execution NOT_RUN and E2E02 remains unresolved.

### Constraints

- One mutation owner: current dirty T50 stays active; do not build/modify its work
  concurrently. T55 starts only at a safe scheduling handoff via progress utility.
  T44 stays PAUSED until explicit resume; PTY semantics do not claim pixel parity.
- Native Rust; no JS host, new provider, protocol, credentials store, retry engine,
  hidden paid fallback, hardcoded production model IDs or vendor-specific routes.
  Model names above are diagnostic/test selections only and must be catalog-admitted.
- Preserve immutable generations/bindings, raw history, caps, trust/permissions,
  response-close-before-tool and no partial/unknown-effect auto-replay. No extra
  side effects during validation or new generation-layer retry loop.
- Test secrets only from gitignored `.local/live.env`, not runner credentials.
  No raw live response/opaque body/auth/URL/env dump in Git/log/UI; no generation
  or test edits in the owner's actual repository. Use isolated owned fixtures.
- Existing campaign counts persist across restart; uncertain reservations remain
  spent. <=4 short MCP searches (none needed for T55), output <=2048 smoke / <=8192
  coding, bounded fixture input/time/watchdog. Do not reset campaign to get budget.

### Non-goals

401/key rotation, optional config warnings, broad plugin compatibility, T53 future
Go/Chat/Messages, donor overlapping tool fibers, T44 visual work and general retry
redesign are not this fix. The reasoning `in_progress` hypothesis was not observed
in the failing live stream: do not loosen every item status to repair sparse output.

## Change Envelope and implementation order

1. Add the smallest RED owner regression in
   `crates/oc-adapters/src/provider/tests/` using the normalized fixture topology.
   Reinspect current `provider.rs` and actual done/terminal identity semantics;
   compare pinned donor `opencode/packages/ai/src/protocols/open-responses.ts`
   done handler and completion recovery. Do not copy donor early tool admission.
2. Change `SseParser::{dispatch,complete_response,validate_output}` and adjacent
   `provider/failure.rs` only as necessary. Keep one bounded completed-item owner;
   no second registry/store. Complete_response must not treat an empty terminal
   array as proof that already completed streamed items never existed. Reconcile
   omissions, reject contradictions, preserve order/opaque items and clean closure.
3. Separate fixed local validation diagnostics from true read/incomplete failures
   using the existing typed error model. Fixed safe stage/event-kind/code/counts
   are sufficient; no raw item IDs, arguments, encrypted_content or message body.
   Touch runtime/public DTO only if an irreducible typed fact requires it; preserve
   RET01 continuation/error policy rather than changing allowance/backoff.
4. Run nearest owner tests and existing safety regressions, then R3 actual binaries.
   Reuse integration/headless/PTY owners and add focused cases, not duplicate test
   matrices on every layer. Test repeated terminal observations, partial snapshots,
   missing terminal, delta-only/invalid JSON, mismatched/duplicate IDs/call IDs,
   conflicting final arguments and failed/incomplete status. No test disabling or
   baseline relaxation. Store current code commit/build/effect evidence.
5. Verify `scripts/bounded_live.py` integration with the actual native binary
   offline, including auxiliary/title/retry dispatch and restart accounting. Run
   R4 only after the relevant offline gates. Reuse the campaign recorded below,
   reserve+fsync before upstream dial, count uncertain outcomes and inspect after.
   Record exact models/variants and all unqualified cases; no live fault campaign.
6. Deliver `evidence/T55/report.md` with PROV09/PROV10, code/build references and
   current gates; hand off to T27. T27 retains E2E02/A09 seeded Rust read/apply_patch/
   bash/test/reopen/next-command and its remaining workflow, including current
   harness validation rather than relying on a stale blocker or past PASS.

Allowed artifacts: focused adapter/tests/native diagnostic/evidence and necessary
existing error consumers; no unrelated tool/UI/config implementation. Registration
and this diagnostic delivery are documentation/evidence only. Keep old reports and
checkpoint leaves unchanged; existing detailed IDs retain owners. New detailed
IDs PROV09/PROV10 have only T55 as owner.

## Current Checkpoint / State

- Verified: reproducible live sparse-terminal topology twice, existing release
  offline RED versus full/omitted GREEN, exact owner source seam, missing plan owner.
- Next unresolved: R1 minimal owner test and bounded reconciliation after handoff.
- Plan registered T55 todo; T27 now depends on T55. Priority is next safe T50
  handoff before T53/T45/T27; this is not permission to stop/reset the active writer.
- External blocker: none for diagnosis/plan. Product fix is NOT_STARTED; previous
  T27 envelope qualification must be rechecked, not automatically relabelled PASS.
- Campaign ID: `39d44cb58b834de99544daf3c2eedab1`; resume identity and ledger paths
  are in `evidence/T55/diagnosis.md`. Diagnostic counts: generation 4, control 5,
  MCP 0, input 2720 bytes. 20 generation requests remain, not a fresh 24.

## Checkpoint History / Completion

- 2026-09-30: owner-requested bounded read-only user-state inspection, direct
  live structural capture and synthetic existing-ELF differential completed.
  Registered repair/E2E ownership; diagnostic delivery does not close R1–R4.
- Completion pending: implementation NOT_STARTED, PROV09/PROV10 NOT_RUN as
  post-fix gates, T27 E2E02 and T44 visual qualification separate/unresolved.
