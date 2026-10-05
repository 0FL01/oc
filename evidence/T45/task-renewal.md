# T45/R9 — task/pack HOT renewal inside the same delegation

Contract: `docs/DCP.md` "Task/pack HOT renewal and completion" (2026-10-03),
`docs/LONG_HORIZON.md` owner clarification, spec R9/DCP11. Implementation
atomic; native-binary qualification follows separately.

## RED

`compress` with `startId = endId =` the running turn's own task message was
refused by the shared range planner ("range covers the unfinished tail"), and
`TurnLog::current_working_checkpoint` always carried the task item verbatim —
a permanent protected task/pack lane.

## Behavior

- `runtime/turn.rs::task_renewal_summary`: a `compress` call whose single range
  starts and ends at the turn's accepted task message (`turn_acceptances`) is a
  task renewal; every other range keeps the existing whole-past path and its
  unfinished-tail refusal. Admission keeps request availability (manual mode)
  and Deny; an Ask compress permission is refused for renewal explicitly rather
  than widened.
- Execution validates the summary with the existing bounds, records the intent
  durably in `TurnLog.task_renewal` with the tool outcome, and answers
  `task_renewal_accepted` / "applies from next request".
- `commit_task_renewal` at the next closed boundary: `renewed_task_checkpoint`
  replaces the task (or its previous renewal) with the model-authored summary
  as a **user-role** item under a trusted envelope (task data, not
  system/developer instructions), retains every other closed group except
  projection-only `compress` groups, and `prepare_closed_segment` +
  `commit_closed_turn_segment_selected` seal the exact RAW delta (including the
  original task/pack) and install the new HOT atomically. Any failure leaves
  the previous HOT in force and clears the intent.
- RAW is never rewritten: the child's accepted user message, `turns.prompt`
  and the sealed segment keep the exact first-delivered task/pack. Restart
  reads the latest committed HOT (`turns.result`) through the existing
  current-task recovery.
- The `compress` tool description names this single exception.

## Checks

- `tests/subagent.rs::r9_child_renews_own_task_pack_hot_before_final_and_keeps_raw`:
  child first request carries the exact task + `<parent_context>` pack; after a
  closed `read` group the child renews twice before its final answer; each next
  request carries only the latest summary (no task/pack text, no previous
  summary), keeps the `read` group, never places the summary in a
  developer/system item; both compress operations record
  `task_renewal_accepted`; the child message still holds the original;
  `turns.result` holds the latest summary only; RAW segment 1 holds the
  original task.
- `tests/subagent.rs::r9_root_task_renewal_and_spanning_range_keeps_tail_refusal`:
  the same renewal for a root task; a range spanning past messages and the
  current task still fails with "unfinished tail" and leaves the task intact.
- Full workspace **1567 / 0 / 10**; fmt and strict workspace Clippy PASS.
