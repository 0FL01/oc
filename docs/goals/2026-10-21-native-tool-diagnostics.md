# Goal: usable native file tools and attributable subagent failures

Status: active
Source: user bug report and audited plan, 2026-10-21; implementation/commit/push/build explicitly requested.
Last updated: 2026-10-21

## Objective

Fix Rust OC2's opaque `grep` / `read` / `subagent` failures while preserving useful bounded search evidence and honest execution state. Match the inspected local TS behavior where compatible with native security/resource guarantees, not its permissive symlink behavior. This is not whole-product or Long Horizon qualification.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work only at a proven external blocker, an approved budget boundary, or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: Project-root `read` works through runtime normalization/admission.
  - Source: bug report, audited plan step 2.
  - Acceptance: `.`, `./`, absolute root and normalized aliases list sorted/paged directory entries; original empty input stays invalid, read Deny and external admission unchanged.
  - Primary evidence: runtime regression through fake provider, durable operation and next request.
  - Status: verified
  - Evidence: `cargo test -p oc-adapters --test runtime --locked root_read_aliases_keep_nonempty_resource_and_original_provider_arguments` (original failure reproduced, then PASS); external read/admission regression PASS.
- R2: Bounded grep retains useful verified matches instead of discarding them on resource exhaustion.
  - Source: audited plan step 4 and original root `Preview limited` query.
  - Acceptance: exact root query yields both real source/test locations within existing caps; resource omissions yield failed + verified matches + `coverage:incomplete`, no false absence/global prefix/continuation cursor; ordinary pages retain coherent ordering/pagination. Cancellation discards partial results and joins worker.
  - Primary evidence: file-owner regressions, bounded repository-root query and runtime result projection.
   - Status: verified
   - Evidence: file owner suite 28 PASS, tools owner suite 39 PASS; actual bounded repository-root query finds both TUI locations; incomplete evidence/compile-budget and active cancellation/join regressions PASS. Runtime projection is additionally covered by R5.
- R3: Native errors and output publication preserve trusted state, actionable bounded diagnostics and field associations.
  - Source: bug report shared builtins concern; audited plan step 3.
  - Acceptance: tool/code/message/detail/hint survive safe redaction/limiting; assembly keeps announced name; state does not depend on text; DCP uses exact durable operation identity and does not purge successful error-looking content; match text remains associated with its own path/line. Existing rich shell/media/effect semantics remain intact.
  - Primary evidence: tools/output/context tests and fake-provider continuation.
  - Status: in_progress
  - Evidence: tools owner suite (38 tests), output suite (9), exact-occurrence storage/planner regressions and runtime DCP strategy/root-read tests PASS. Detailed file error mapping follows in R2.
- R4: Subagent failure/cancellation retains session ID and evidence-based cause across terminal commit/reopen.
  - Source: bug report and audited plan step 5.
  - Acceptance: policy refusal, cancellation, provider/admission/storage failure remain distinct; unknown provenance stays unknown; reason precedes signal and terminal freeze; exact terminal TurnLog is authority, no duplicate diagnostic store; background notices/recovery preserve available facts; cleanup failure remains sticky/fatal; no unknown-effect replay.
  - Primary evidence: child/runtime/storage/authority regressions, committed-turn/reopen and cancellation race fixtures.
  - Status: pending
- R5: Original composed workflow succeeds and parent can continue after recoverable tool failure.
  - Source: audited plan step 1.
  - Acceptance: admission/execution/persistence/next-provider-request exercise root grep, read-only Explore and root read, with no cancellation solely due to Plan mode.
  - Primary evidence: composed deterministic runtime/subagent integration fixture.
  - Status: pending
- R6: Deliver verified commits, ordinary push to verified origin, and release binary.
  - Source: user implementation request.
  - Acceptance: relevant tests + full offline gate green, commits pushed without force, `target/release/oc2` built from delivered source; no live owner restart.
  - Primary evidence: command receipts and Git remote/HEAD agreement.
  - Status: pending

### Constraints

- One mutation/Cargo owner; preserve others' diff. Start HEAD `5f2951bbb8a4cc738e88554a7f4609c5ed1c5f66`, clean worktree; verified origin `git@github.com:0FL01/oc.git`, branch `master` tracks origin/master.
- Preserve no-follow/nonblocking regular-file access, data-root exclusion, permissions, Plan/child authority, captured request/batch bindings, RAW immutability, cleanup and no unknown-effect replay.
- Preserve time/entry/depth/file/aggregate/regex budgets. No live paid requests, user-client launch, owner restart/kill/lock removal, credentials/config changes, target deletion or target-dir override.
- Existing missing cancellation provenance must not be invented; provider deadline is not an overall subagent lifetime deadline.

### Non-goals

- Universal success envelopes, executor framework, provider protocol changes, core/UI DTO changes, SQL migration, diagnostic event for every child, new deadlines/retries/round stops.
- New search cursor/persistent scan state, streaming traversal rewrite, hardcoded target/.local/references exclusions, large ordinary-file read semantic changes, broad shell/MCP/CodeMode rewrites.

## Change Envelope

- Owners: `crates/oc-adapters/src/{tools.rs,tools/,files.rs,files/,runtime.rs,runtime/,storage_children.rs,storage_children/,storage_tool_output/,application/authority.rs}` and direct state/identity consumers; relevant unit and integration fixtures.
- Docs: this goal, root GOAL link, `docs/TOOLS_MCP.md` for explicit incomplete-search contract. No new dependencies, services, stores or generated evidence in Git.
- Validation: targeted owner tests then affected integration targets, one sequential `bash scripts/test.sh`, `git diff --check`, `cargo build -p oc2 --bin oc2 --locked --release`.

## Audited implementation plan (frozen copy)

1. Add composed reproducer through admission -> execution -> stored output -> next provider request. Assert useful results and continued parent work, not only error formatting.
2. Fix only the internal normalized root-read resource: empty project-relative suffix becomes `.`; keep invalid caller empty path rejected.
3. Introduce one small adapter-local outcome (trusted state + text) and bounded diagnostic projection. Carry announced tool name and typed errors through guards/dispatch/foreground futures. Preserve existing control/effect/media owners. Fix JSON clipping associations; redact before clipping. DCP uses bounded exact-occurrence durable states, never output heuristics. Missing metadata does not justify purge. Consumers change before new error producers.
4. Keep gather/sort, ignore semantics and caps. For size-admitted content stop at first NUL-containing I/O chunk and charge actual reads; oversized content candidates are unsearched omissions, no content read. Mandatory ignore parsing cannot be skipped. Keep verified whole-file-validated matches on aggregate/time exhaustion, return failed + incomplete coverage + no continuation. Discovery exhaustion returns no fabricated content matches. Distinguish directory I/O from entry exhaustion and regex syntax from compile resource failure. Ordinary successful pagination unchanged. Never publish partial matches on cancellation.
5. Preserve child ID and safe concrete failure information. Keep cancellation flag with small synchronized cause/freeze record at source. Do not infer user dismissal from generic QuestionDecision::Cancelled. No late cancel rewrite of selected success/failure. Commit terminal diagnosis before report construction; use exact linked terminal TurnLog for foreground/new notices/recovery, no unconditional event duplication. Missing pre-acceptance provenance stays unknown. Preserve committed outcome separately from fatal cleanup health.
6. Qualify root workflow, partial-search usefulness, error-looking success, bounded/redacted associated JSON, permission/no-follow/data-root, child authority/cause races, reopen and cleanup. Apply common diagnostic mappings only to confirmed affected native routes; no universal payload refactor. Run gates, review scope, commit/push/build.

## Current Checkpoint

- Closes: R4.
- Smallest next action: preserve child identity/source cause and commit the terminal diagnosis before report construction; exact-turn recovery and sticky cleanup health.
- Expected evidence: child stop/foreground/storage/reopen regressions and composed workflow.
- Stop or replan if: concurrent edits touch owned files, or a required outcome needs a material envelope expansion.

## Current State

- Resolved: R1, audited source/contract and repository/origin/resource preflight.
- Last relevant evidence: R3 transport/output/DCP tests PASS; root-read remains green.
- Blocker: none.
- Next: R4 child terminal provenance.

## Material Decisions

- 2026-10-21: Failed incomplete search carries useful bounded matches; no new execution state or cursor. Exact terminal child TurnLog avoids duplicate event storage. Resource/security boundaries remain mandatory, not obstacles to remove.

## Checkpoint History

- 2026-10-21: Goal frozen from user-approved audited plan; clean current HEAD and origin verified.
- 2026-10-21: R1 runtime regression reproduced `error: read requires a bounded path`; nonempty normalized root fix passes all aliases, original RAW arguments and external approval regression.
- 2026-10-21: R3 explicit state survives dispatch/foreground; assembly retains names, JSON clipping retains associations, DCP uses indexed exact operation states. Initial build timeout left corrupt incremental objects (undefined hidden linker symbols); verified with nonincremental Cargo, without deleting target or changing target directory. Remaining file mappings belong to R2.
- 2026-10-21: R2 file budgets now identify phase/cap/counters; admitted binaries stop early, oversized candidates are explicit omissions, failed incomplete grep retains verified matches with no continuation. Actual root query finds both requested locations. Files (28), tools (39), runtime root-alias regression PASS; cancellation assertions migrated to structured output without weakening drain/no-replay checks.

## Completion

- Pending.
