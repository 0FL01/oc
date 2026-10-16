# T34 initial regressions

Base HEAD: `4e07cc7` (T33 closeout). Temporary fixtures only; no live requests.

## Provider red (before provider implementation)

Executed by provider workstream:
`cargo test --locked -p oc-adapters provider::tests::aud -- --nocapture`
Exit 101: both new regressions failed. Complete text-event EOF incorrectly
returned success; cancellation before response headers exceeded 500 ms.

## Actual-binary red (before runtime continuation implementation)

After provider API compilation integration only:
`cargo test --locked -p oc --test responses aud09_aud10_binary_exact_typed_tool_history_survives_restart -- --exact --nocapture`
Exit 101; compile 10.09 s, test 0.10 s; 0 passed, 1 failed.

```text
strict ordered input at round 0
left: [{"content":"user: strict first prompt","role":"user"}]
right: [{"content":[{"text":"strict first prompt","type":"input_text"}],"role":"user","type":"message"}]
```

Later call-id/restart assertions were not reached by this failing run. Their
execution is recorded separately, not inferred from this failure.

## Mandatory-gate blocker: inherited ownership lock

The first final workspace run failed `aud07_rejected_input_has_no_turn_or_event`
at immediate Db reopen with `DataRootBusy`. A concurrent child fork retained the
parent's flock until exec; closing the parent's descriptor alone was insufficient.
No retry or single-threaded test gate was substituted.

Before the lock correction, the synchronized pre-exec regression
`cargo test --locked -p oc-adapters --test storage_lock` failed (exit 101):
`parent drop must release the inherited flock: DataRootBusy`, 0 passed/1 failed.
Afterward it passes; parent independently executed it and the final workspace.
The private successfully-acquired lock now explicitly unlocks after SQLite closes.

## Fixture corrections, not weaker expectations

- T33 crash fixture formerly required `turns.result = NULL` before completion.
  T34 intentionally journals completed provider items before execution. The test
  now requires the exact user/call intent, no result/assistant, and byte-identical
  wire journal after unknown recovery; operation outcome remains NULL.
- PROV03 previously sent a textual `function_call_output` marker and incomplete
  tool SSE. It now sends actual typed canonical calls/results with distinct IDs.
- The terminal fault trigger now fires only for `NEW.status = 'completed'`, so
  new intermediate wire checkpoints cannot accidentally satisfy that regression.
- A later workspace run exposed soak cancellation before its future was ever
  polled (`Err(Cancelled)`). It now polls concurrently and synchronizes on durable
  acceptance before cancellation, preserving the required Cancelled report.
- Soak's two terminal events in one HTTP response became separate real rounds.
  Original thresholds/baseline/test IDs were not changed. The dormant flattened
  assembler test was replaced by active typed protocol/bounds evidence.
