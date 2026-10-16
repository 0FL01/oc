# T45/R3 — safe child terminal diagnostic and bounded retry history

Date: 2026-10-07. Source: the frozen R3 2026-10-06 restricted-error
supplement in `docs/goals/2026-09-21-config-compat-and-subagents.md`.
Base: checked/pushed `3e9325d1b`. No real provider traffic or credential access.

## Reproduction and minimum owner change

`runtime/children.rs` already persists the safe `TurnReport.diagnostic` as
`ChildJob.result` and returns it for a foreground failure. The child terminal
callback in `runtime/turn.rs` instead published the fixed `child turn failed`
for every failed report. An actual `CoreApp`/native Responses peer regression
reproduced that live-event loss before the production change.

The failed **report** branch now forwards the existing owner-sanitized diagnostic
with the same fixed fallback when absent. The separate arbitrary runtime `Err`
branch retains its fixed safe message; it is not stringified. Cancellation,
warnings, job/result persistence, delivery, provider classification/redaction and
retry policy are unchanged. No DTO, schema, store or retry engine was added.

## Direct consumer proof

Existing integration target `subagent`, fixture owner
`crates/oc-adapters/tests/fixtures/background_children.rs`:
`core_child_terminal_reason_and_final_span_reopen_without_retry_replay`.

The actual application and child worker perform one settled native read, then a
partial-output retryable server rejection, then a final structured `cyber_policy`
rejection with a safe explanation and public status/access URL. An existing owned
barrier holds the final child response until the parent finishes independently;
this does not infer ordering from a retry delay or increase a watchdog. The regression asserts:

- the live child terminal event carries the final safe explanation/URL, not the
  generic fallback, earlier retry reason or successful partial prose;
- the authoritative terminal job has state Error and the same result, operation,
  delivery identity and generation as the admitted work;
- linked bounded history exposes the settled step, historical failed retry and
  distinct final failed span. The earlier retry is completed/history; the final
  span has the same error and **no** retry deadline;
- shutdown/join and reopen return the identical job and history page. There are
  still exactly five captured primary requests (two parent, three child) and one
  completed read operation; no generation/tool/retry is resurrected by navigation,
  restoration or an old deadline. Auxiliary title traffic is separate.

This reuses RET01 classification/redaction, not another policy matrix. Fixture
diagnosis established two relevant distinctions: mixing an Authorization canary
into a message correctly withholds that whole private diagnostic; a transparent
pre-output retry reuses its active span. The final fixture therefore uses a safe
public diagnostic and a genuine committed **partial-output** retry to qualify
historical-versus-current spans. Neither protection was weakened. Physical counts
come from the peer, not a child view's root-operation attribution counter.

## Checks

- Targeted command:
  `cargo test --locked -p oc-adapters --test subagent core_child_terminal_reason_and_final_span_reopen_without_retry_replay`
  — final barrier fixture PASS, 1/0/0, 3.18 s after compile.
- Final source: fmt check, strict locked workspace/all-target Clippy,
  `cargo test --locked --workspace --no-fail-fast`: **1713 passed / 0 failed /
  11 unchanged opt-in ignored**, independently summed from 46 result records.
  `cargo build --locked --release` and diff check PASS. Log:
  `/home/opencode/.cache/opencode-tmp/opencode/t45-child-diagnostic-final-barrier.log`,
  final marker `T45_CHILD_DIAGNOSTIC_FINAL_BARRIER_GATES_PASS`.
- The same production change also passes normal debug/release builds and actual
  `native_child_controls.py` (4 cases each), `native_child_recovery.py` (20 each)
  and `native_chronology.py` (6 each): joined/reaped owned work, precise controls,
  source/generation/settled-effects/reopen guards, root/child wire order intact.
  Log: `t45-child-diagnostic-current2.log` in the same approved cache, final marker
  `T45_CHILD_DIAGNOSTIC_ALL_GATES_PASS`. Subsequent test-only parent barrier was
  requalified by the final full workspace chain above; production/native binaries
  did not change. Python progress tests: 47 PASS; documentation/diff checks PASS.
- Initial strict Clippy found an unnecessary singleton clone in the new assertion;
  replaced by `std::slice::from_ref`, no lint suppression. Serial Cargo uses
  approved TMPDIR, jobs3/testthreads2, normal stacks and unchanged watchdogs;
  no ignores, baseline, resource threshold or security validation changes.

## Boundaries

This qualifies R3's typed backend/live/history consumer supplement, not paired
T44 VIS39/VIS43 presentation, which remains PAUSED. Existing SUB01/SUB02 ownership,
at-least-once donor recovery distinction and no unknown-effect replay remain.
AUTH06 is owner-deferred, T57 remains blocked/not DONE; no paid campaign, Go ledger
reset, user `.opencode/` inspection or secret search. Full T45 closure follows the
checked commit/push and frozen-outcome reconciliation.
