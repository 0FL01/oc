# Live context usage — settle tools before publishing the UI measurement

## Result

The backend and live projection fixes pass their focused/affected gates, the
fresh paired Execute attempt09 and the first full workspace gate. The16 busy-footer
cells now match. A further retained-history regression reproduced the same leak.
Its correction exposed a stale in-flight status on synthetic finished metadata.
That owner classification is corrected; the old captured-limit regression, the
new retained-page regression, all549 TUI tests and strict workspace Clippy pass.
Final-source paired attempt10 repeats the corrected footer and functional proof;
the final isolated workspace gate passes2012 tests with zero failures and12
unchanged opt-in ignored cases across46 targets. This is not a whole VIS46 or T44
visual PASS.

Execute attempt07 exposed16 busy-footer cells: native displayed the current
step's `1.3K (1%)` while Original displayed the agents hint during a held nested
fetch. Pinned Original joins its local tool fibers before publishing
`session.step.ended`; its message updater assigns assistant tokens from that
event. Native had two earlier publication paths:

1. `TurnContextUsage` was sent before `execute_units`.
2. Tool-start publication also supplied `TurnPresentation`, whose durable
   checkpoint already contained the measured usage. The TUI adopted that
    in-flight checkpoint measurement independently of the dedicated event.

The retained-history footer fallback also accepted a started checkpoint as a
settled measurement after page adoption. It now skips only that known in-flight
status, retaining terminal/legacy history and the actual settled live event.

The first fix alone passed the runtime regression but did not fix actual
attempt08. The second path was then reproduced independently at the TUI owner.
The runtime now sends the dedicated measurement after local tools settle,
including failed execution. In-flight presentation checkpoints do not advance
live footer usage; the preceding settled measurement remains available, and a
terminal projection can recover final measured usage.

Accounting, billed usage, usage anchors and the durable pre-effect checkpoint
remain unchanged. No context estimate, fake value, new event, store or worker
was introduced. Permissions, execution ordering and RAW history are unchanged.

## Checks

- Runtime RED: the existing real two-MCP-plus-fetch Execute test observed
  premature usage `[(10,5)]`; log
  `.local/evidence/t44-usage-settlement-reproduce.log`.
- After the backend fix, the same actual operation retains all progress, final
  facts, RAW, exact request-count and reopen/no-replay assertions. It requires no
  usage during progress and exactly two settled `(10,5)` records afterward.
  Code Mode9 tests pass; the full runtime integration target passes130 cases.
  Logs `.local/evidence/t44-usage-settlement-runtime-01.log` and
  `t44-usage-settlement-runtime-all-01.log`.
- Actual paired attempt08 passes20 functional stages on both binaries, with
  natural restart and no replay, but still has the16-cell running-footer
  difference. It is retained as evidence that the backend-only fix was
  insufficient: `evidence/tui/execute-preview-20261016-08`.
- TUI RED: a started checkpoint incorrectly changed context to1298 tokens;
  `.local/evidence/t44-usage-checkpoint-reproduce.log`. The new owner regression
  also checks retention of a preceding settled measurement and recovery from a
  terminal projection. All549 TUI tests pass after the fix.
- Strict workspace/all-targets Clippy passes. Latest logs:
  `.local/evidence/t44-usage-settlement-tui-01.log` and
  `t44-usage-settlement-clippy-03.log`.
- Fresh actual `evidence/tui/execute-preview-20261016-09` passes20 functional
  stages on BOTH binaries, with exact five provider requests, two Execute
  operations, three nested MCP calls and one GET. Both process generations exit
  naturally/code0; actual Code/Output clipboard transport, final facts, RAW and
  reopen/restart/no-replay checks remain intact.
- Running-dialog and copied-Code frames each retain all4800 styled cells with
  zero differences and all647040 PNG pixels identical. The copied-Code frame
  also has an equal cursor state and passes the complete grid comparison.
  Running-dialog hidden pen coordinates still differ, so its unchanged complete
  grid comparator remains FAIL. No cursor field was normalized.
- Complete attempt09 totals: grid3 PASS/17 DIFFERENT and PNG4 PASS/16 DIFFERENT,
  **7 PASS/33 DIFFERENT** across40 comparisons. Both sides explicitly retain
  `error_state_equivalence: false`. The native source manifest is
  `8cf0c2b01fdac3507335efa0286e28d2f2e0ed7a5890dc9665edcb78c9e6b3a9`;
   its actual rebuilt ELF is
   `f781ab6c81959dd03145ff9651c98663db269140eea1668a7e9d2d130536cde8`
   (`oc2 0.2.0`). The complete source/build identity remains in capture.lock.
- First isolated full workspace gate:2012 PASS, zero failures and12 unchanged
  opt-in ignored tests across46 targets;
  `.local/evidence/t44-usage-settlement-workspace-01.log`.
- Retained-history RED: adopting the actual started HistoryTurn page leaked1298
  tokens despite the live-projection guard;
  `.local/evidence/t44-usage-retained-reproduce-02.log`. The existing owner test
  now also checks the retained path and the preceding terminal-history fallback.
   The correction leaves all accounting and stored checkpoints intact.
- The first retained-path TUI run passed548 tests but failed the existing
  captured-context-limit regression: finished metadata retained the earlier
  checkpoint's `started` status and was incorrectly skipped. Its required225000
  limit assertion was not changed. `apply_presentation` now assigns
  `live_terminal_status` only from a terminal projection; the actual finish event
  therefore preserves its measured context and captured limit without a stale
  in-flight classification. Log
  `.local/evidence/t44-usage-settlement-tui-02.log`; that stopped pipeline did not
  execute its Clippy or workspace commands.
- Both unchanged captured-limit and new retained-page owner regressions pass
  after the terminal-status correction. The full549-TUI suite and strict
  workspace/all-targets Clippy also pass on the final source. Logs
  `.local/evidence/t44-usage-terminal-owner-01.log`,
  `t44-usage-retained-fixed-01.log`, `t44-usage-settlement-tui-03.log` and
   `t44-usage-settlement-clippy-05.log`.
- Final-source `evidence/tui/execute-preview-20261016-10` again passes20 stages
  on each binary, including actual clipboard transport, exact request/effect
  graph, no replay and both natural code0 process exits. Running-dialog and
  copied-Code remain zero-difference complete styled-cell/PNG frames; the
  running dialog still fails its full grid comparison on hidden pen coordinates.
  All40 strict comparisons remain **7 PASS/33 DIFFERENT**, with explicit
  `error_state_equivalence: false`. Source manifest
  `cd9936dfd8c51db3904da82aadb6099f963b9f9f49e3e899e2c6a65911443d5f`,
  actual rebuilt ELF
  `02bc17bb65e31ee86839117614c44657b31d8139296b1a9403d1f06e37edd4bf`
   (`oc2 0.2.0`); log `.local/evidence/t44-usage-settlement-capture-10.log`.
- Final-source isolated full workspace gate passes2012 tests, zero failures and12
  unchanged opt-in ignored cases across46 targets. All46 target summaries pass;
  fmt also passes. Log `.local/evidence/t44-usage-settlement-workspace-02.log`.

## Risks

Unit-level event ordering alone previously missed the projection path; the new
paired witness closes this specific footer difference. Actual elapsed labels,
hidden read-only terminal pen positions and the explicitly incomparable metadata-error/failed-parent
interaction remain unmodified and are not silently accepted as full parity.
Historical question and Execute receipts are not rewritten.

## Next

The final-source gates pass. Commit and push the reviewed verified slice.
The owner's latest instruction is “доделывай, коммит и пуш, далее пауза”:
pause after delivery and do not begin another TUI slice. Full T44 acceptance
remains open and unchanged; this is an owner scheduling pause, not an external
technical blocker.
