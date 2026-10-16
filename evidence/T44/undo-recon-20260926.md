# Parked T44 / direct undo RECON — 2026-09-26

**Superseded planning interpretation:** the subsequent owner clarification and approval
require conversation/context-only undo/redo with unchanged workspace. The per-file
preimage/rollback proposal below is historical RECON, **not the execution plan**.
Authoritative approved plan: `tui-recovery/T44_CONTRACT_AMENDMENT.md`, owner amendment
2026-09-26. No filesystem snapshot or replacement preimage journal is to be implemented.

## Result

Owner instruction: current implementation scope is paused; park the worktree and provide a plan after RECON for possible removal of Git checkpoints and direct `/undo`. No approval to implement the replacement is inferred. Product edits, Cargo, commits and pushes stopped. HEAD at parking: `85780c8` (owner documentation amendment); latest independently verified implementation commits are `c6c02a2` (concurrent titles) and `577ce84` (painted user hover), both pushed. The inherited `.opencode/` was not inspected or staged.

Uncommitted work is preserved: durable history identities; owner historical fork plus acceptance migration; snapshot prerequisite and typed/supervised shell execution. New files include `application_fork_tests.rs`, `storage_fork.rs`, and `snapshots.rs`. The snapshot correction changed the experimental Git backend into native full-tree SQLite manifests, but the current correction is NOT compiled or qualified. It has no restore API. Do not mistake saved snapshot evidence or a fork API for completed Message Actions/Revert.

## Checks

Read-only RECON inspected pinned donor `opencode/packages/core/src/snapshot.ts`, `session/runner/step.ts`, `session/revert.ts`, `git.ts`; native `patch.rs`, `patch/fs.rs`, runtime capture hooks, snapshots and storage. `git status`, HEAD/log, diff stat and `git diff --check` ran after pause; diff check passed. No Cargo, benchmark, capture, commit, push or product fix ran after pause. The prior full regression gate for `577ce84` passed: `/home/opencode/.local/share/opencode/tool-output/tool_0dc5c25c1001nUr2e8fxEv7nai`; this DOES NOT qualify the current dirty diff.

Source findings:

- Donor snapshots run before and after model attempts, maintain a separate Git index/tree, enumerate/stat candidate paths and hash changed/new bytes. Its 2 MiB limit applies to untracked files; the inspected capture path has no total byte/file/time budget. No measured performance comparison was made.
- Dirty native hooks run before and after each admitted `apply_patch`/`bash`. The corrected backend no longer invokes Git, but still walks and reads the eligible workspace: approximately O(entries + eligible bytes) per phase. File bytes are keyed by manifest/path, not deduplicated across distinct manifests. Caps: 32 MiB/scan, 2 MiB/file, 10,000 visited entries, 10 seconds; no snapshot-specific retention/GC. Moving this workload from Git into SQLite alone does not solve long-session cost.
- `patch.rs:807–1067` already prepares precise paths, preimage bytes/modes and intended postimages; the plan is capped at 64 MiB before+after bytes. `:1070–1168` commits per file and records partial effects, including an update before a failed move. `patch/fs.rs:162–244` already supplies pinned, no-follow, stale-preimage-checked mutation primitives. Thus patch undo needs no worktree scan.
- Arbitrary bash/MCP effects are not fully observable from tool output. Hashes alone cannot recover overwritten/deleted bytes. A no-snapshot implementation cannot truthfully guarantee reversal of arbitrary shell or remote effects.

## Risks

The owner proposal is tentative; no goal or acceptance specification was changed. Direct `/undo` is not synonymous with donor staged Revert/Clear/Commit or editor undo. Chosen plan assumption: `/undo` immediately undoes the latest accepted user turn, restores its draft, and changes active conversation projection; there is no staged revert/redo in the first implementation. This assumption is for planning only.

Full file rollback requires before-state. Efficient guaranteed rollback is feasible for native `apply_patch`; bash, MCP, external editors, writes outside the trusted Location and legacy turns without a journal require explicit refusal or separately approved coverage. Never report a successful full rollback when only conversation text changed. Raw history remains append-only; undo changes owner projection with durable compensating events.

## Next

Recommended replacement plan, not executed:

1. Separate the dirty identity/fork work from snapshot-specific hooks/schema/shared-lock changes using reviewed hunks; preserve owner amendments and unrelated shell correctness fixes. Remove automatic whole-worktree capture from the normal path rather than merely swap its backend. No reset of the current worktree before review.
2. Add a narrow patch write-ahead journal at the prepared-plan boundary. Persist original existence, bytes/blob hash, mode, source/target path, expected post-state and operation/turn ordering BEFORE the first mutation; journal failure means no patch writes. Reuse existing blob quota/content addressing where compatible, with undo-specific retention and explicit unavailable status for expired/legacy data. Do not collect unrelated files.
3. Persist actual per-file completion/intermediate move states. Recovery distinguishes unchanged preimage, expected postimage and a third/conflicting state; do not infer all planned paths were changed after partial patch failure.
4. Add a typed owner `/undo` operation. Refuse active/unknown work; identify the latest accepted user-turn boundary; preflight all affected current paths and permissions; restore journaled changes in reverse order with pinned no-symlink primitives and expected-postimage checks. External edits cause a conflict, never silent overwrite. Keep a durable rollback intent for crash/reconciliation; partial rollback is visible/incomplete and blocks claiming success.
5. Commit a durable history-projection boundary and restore the unsent original prompt only after the owner result. All history pages/totals, next provider wire input, tool pairs, model-switch baseline and DCP blocks/prune state must observe the same causal cut; keep raw messages/turns/events unchanged. Do not conflate this with Ctrl-minus editor undo.
6. Treat arbitrary bash/MCP as coverage barriers, not reversible operations. Default full undo refuses a boundary with uncovered effects; a separate conversation-only action must say files/external effects remain. General shell rollback would require an independently scoped write tracker/COW isolation or pre-effect captures, not a keyword guess that a command is read-only.
7. Verify patch add/update/delete/move/mode, repeated writes to the same path, partial failure, pre-journal storage fault, crash before/after file rename, external-edit conflict, current-policy/symlink refusal, restart, concurrent undo/submit, raw-history immutability and actual next provider input. Measure hot-path files/bytes/time and retained undo storage against touched bytes, including many turns on a large worktree. No PASS or numerical performance claim before those tests.

This plan has not been implemented. Keep T44 paused pending the owner's replacement-scope decision; do not run the parked experimental snapshot code or promote its old tests to current verification.
