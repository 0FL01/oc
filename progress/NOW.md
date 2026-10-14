# NOW — актуальный handoff

State updated: 2026-10-14T16:01:12+00:00
Active: T61

Сверить Git status/diff до выполнения команд.
Task: T61 — Hot reload и native shared service
Spec: docs/goals/2026-10-14-hot-reload-shared-service.md
Evidence target: evidence/T61/report.md

Owner-approved audited plan 2026-10-14, R1-R5/HOT01-HOT04: one task/five slices. Coherent source publication and issued request/full-batch leases; fresh same-task root/child preparation, captured retry/summary boundaries and latest-HOT provenance. One admitted watcher/manual pipeline, whole last-good, persistent Location MCP supervisor with unchanged reuse/per-instance retirement/shared quarantine. Native local IPC service, one Db write/root-lock owner, bounded Locations and foreground-root single-flight per Location; durable admission lookup, scoped consumers/auto/SIGINT/PTY, recovery, detach/resnapshot/joined restart. Preserve guarded readonly models/no-spawn helpers. Independent client settings precedence/env overlay/theme/bindings, no cosmetic runtime rebuild. Composed actual debug/release fixtures and final gates; no HTTP/SDK parity, scheduler/store/framework/plugin host or paid campaign. Qualified seams, not whole T44/T45/T57 dependencies; T30 requires T61. Plan registration todo/implementation NOT_STARTED, other execution states and historical evidence unchanged.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

- T61 active; code `8fc15275e` consolidates config/workspace/DCP/readiness in one private immutable publication.
- Reproduced a captured G1 publication rebuilding its lane with G2 text, then fixed capture ownership.
- Public PublishedGeneration shape, existing idle guards, frozen child snapshots and resource owners remain unchanged.
- R1 remains in_progress; no complete HOT01–HOT04, watcher, shared daemon or fresh-boundary adoption claim.

## Checks

- cargo test --locked -p oc-adapters: 1031 passed / 0 failed / 5 existing opt-in ignored (19 result records).
- cargo clippy --locked -p oc-adapters --all-targets -- -D warnings: PASS.
- cargo fmt --all -- --check; git diff --check; python3 scripts/check_docs.py: PASS.
- Red/green and full adapter logs: .local/evidence/t61-publication-{red,green,adapters,clippy,fmt}.log.

## Risks

- Issued tools/admission/output/instructions still have ambient publication reads; mid-turn publication is not enabled before replacing these reads and qualifying authority fences.
- Child current publications are deliberately still frozen; source-handle sharing and immutable launch ceilings remain R1 work.
- No paid/live call, env/credential read, push/auth retry, foreign staging or T44/T45/T57/overall READY claim.
- T44 remote write-auth blocker and separate VIS46 handoff unchanged.

## Next

- Pass the captured ExecutionGeneration through execute_units, admit_tool, Code Mode and instruction hooks; retain real active interaction/CAS/path ownership instead of latest-counter equality.
- Then implement/qualify same-task fresh G2 adoption and child/retry/summary boundaries before R2 watcher/MCP work.


Ready (до 5): нет
Blocked: T27, T44, T45, T57

Done в журнале не означает READY всего продукта; см. GOAL.md.
