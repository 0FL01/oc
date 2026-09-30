# NOW — актуальный handoff

State updated: 2026-09-30T02:56:47+00:00
Active: T54

Сверить Git status/diff до выполнения команд.
Task: T54 — Provider-error retry and safe continuation parity (OC2 v2.0.12)
Spec: docs/goals/2026-09-29-provider-retry-parity.md
Evidence target: evidence/T54/report.md

Owner-approved 2026-09-29 provider-error retry plan: typed HTTP/Responses SSE errors, quota versus transient throttle, one cancellable finite per-logical-step policy and one physical attempt per adapter call; truthful pre-output retry versus post-output durable continuation, no tool/effect replay or failed-attempt success. Main/child/compaction auxiliary lanes, pinned bindings, physical dispatch accounting and actual-binary headless/PTY qualification in spec. T53/GO03 reuses the policy on future Chat/Messages wires; T44/VIS43 owns paired retry visuals after explicit resume. Consume minimal qualified T51 seam, no whole-task T51/T53/T44 dependency, daemon, JS hooks or new retry framework. Plan only: pending/NOT_RUN, historical statuses/PASS unchanged.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T54
## Result
R2–R4 verified: finite retry, durable spans and lane counts.
## Checks
Workspace1321; owner17+4+3; native9 per ELF; Python47 PASS.
## Risks
VIS43/GO03 separate; no full product claim.
## Next
Task closeout, then independent ready backend task.


Ready (до 5): T45, T50, T53
Blocked: T27, T43, T44

Done в журнале не означает READY всего продукта; см. GOAL.md.
