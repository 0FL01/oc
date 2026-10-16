# VIS37 — current question/form and clean restart qualification

## Result

Current source `462543894720b3923955ed864bc246160dad988a` completes the existing
question/form workflow against pinned Original `2670273ff17da96f85c5826ced57aa1b368754fa`
at80x24,120x40 and160x48. This qualifies the measured functional/form subset,
**not whole VIS37 or T44 visual parity**. No question renderer or backend change
was needed. The fixture's ordered deny-all/question-allow policy is valid under
the completed T65 semantics; no duplicate actor configuration was introduced.

The driver now verifies natural code0 shutdown for BOTH generations, including
the final cancelled-question session. It no longer treats forced final teardown
as clean-exit evidence. No existing action, reply, replay or comparison assertion
was removed.

## Checks

- Fresh immutable captures: `evidence/tui/question-resume-20261016-02-small`,
  `-02-wide`, `-02-tall`; logs
  `.local/evidence/t44-question-resume-02-{small,wide,tall}.log`.
- Each actual binary/geometry passes18 stages: multiselect mouse/focus, long
  question, empty custom Tab/BackTab, Unicode paste/edit/review, explicit answer,
  answered card, switch/reopen/restart, explicit dismissal and cancelled reopen.
- Each side/geometry retains exactly four provider requests, two question calls,
  two user submissions and one explicit answer. Across the campaign:24 provider
  requests,12 question calls and six answer submissions. Exact ordered answers,
  durable completed/cancelled outcomes and unchanged snapshots are asserted.
  No MCP, Shell, artifact or effect replay is permitted or observed.
- All12 process exits (two generations × two binaries × three geometries) are
  natural/code0. All54 cursor pairs are identical.
- Full unmasked comparisons: small18 PASS/18 DIFFERENT; wide16 PASS/20 DIFFERENT;
  tall16 PASS/20 DIFFERENT. Total **50 PASS /58 DIFFERENT** across108 grid/PNG
  comparisons. All eight first-form stages match complete frames at every size.
- The three captures share actual Rust source manifest
  `e1c4dd101bbf32b6fee4c381661aa890ca69a72efed27a2c0e40794af73d5c90`
  and rebuilt ELF `df6535757202d8119fdd10567c0b097ebeccb0e581c1d2f8dc8425c314611940`.
  Actual per-capture build, source and binary provenance remain in capture.lock.
- `node --check scripts/tui_capture/question_forms.mjs` passes. Rust is unchanged
  from the preceding548-TUI/60-PTY/strict-workspace-Clippy verified slice.

## Risks

Complete-frame differences remain visible: the one-to-three-cell answer/cancel
differences are actual elapsed labels; Home/switch frames also retain rotating
examples and product version differences. No clocks, source records, pixels,
cursor fields or comparison criteria are normalized. Historical capture012 and
the first current wide baseline remain separate immutable evidence; their forced
final teardown is not promoted to the new clean-final proof.

## Next

Keep all frozen R1–R6/VIS01–VIS46 requirements. Diagnose the reproduced Execute
busy-footer usage timing through its runtime event owner: measured accounting
must remain intact while UI publication follows settled local tool execution.
Then return to current-source paired TUI qualification and the remaining slices.
