# VIS46 — Execute UI and actual paired interaction

## Result

**UI IMPLEMENTED; paired interaction facets qualified; visual closure OPEN.**
The worktree adds the typed Execute renderer, per-call input expansion and a live,
centered Code/Output dialog. Legacy records without valid execute facts retain
Generic rendering. The dialog follows the typed pending-to-canonical operation
link, then current progress and terminal facts; it does not reconstruct code or
calls from RAW text. Warnings/Logs retain their structured producer provenance.

Actual attempt `evidence/tui/execute-preview-20261016-07` completes20 stages on
both the pinned original and rebuilt native binary. It exercises Receiving,
Running and Completed in the same dialog; actual OSC52 Code/Output transport;
horizontal pan, End/Logs and80x24/120x40/160x48 modal geometry; child hover/input
toggle; failed execution; reopen and process restart. Both generations on each
side exit naturally with code0. Five provider requests, two execute operations,
three nested MCP calls and one controlled local GET remain unchanged on replay.
Native final event/persisted facts retain three completed calls for success and
one error call for failure, without nested durable operations or RAW changes.

The40 full unmasked comparisons yield **4 PASS /36 DIFFERENT**: both Receiving
transcript and dialog are complete styled-cell/cursor and PNG matches. No mask,
crop or clock/hidden-cursor normalization is used. Attempt04 previously completed
the original18 common stages, with3/36 complete comparisons matching.
Attempt06 also completed20 stages per side; its947-cell resize discrepancy falls
to two actual elapsed-label cells on attempt07. The hidden terminal pen still
differs there; the full grid comparison correctly remains FAIL, not a partial PASS.
The captured native ELF is
`df6535757202d8119fdd10567c0b097ebeccb0e581c1d2f8dc8425c314611940`;
`capture.lock.json` records the rebuilt dirty source on base `a2f166fd0`.

The two error-interaction stages are explicitly **not equivalent states**. The
pinned original returns interpreter errors as a completed tool plus
`metadata.error` (`core/codemode/tool.ts:114–137`); its parent opens Failed detail.
The frozen native contract requires a failed parent with inline error expansion
and re-collapse, not a dialog (amendment2523–2525/2573). Both actual interactions
are checked, and the probe records `error_state_equivalence:false`; their full
frames remain different. This is an open comparability issue, not an approved
exception or whole VIS46/T44 visual PASS. Native policy was not weakened.

## Checks

- Existing producer qualification remains in `execute-producer-20261016.md`.
- Current capture Python suite:11 PASS, with existing SQLite ResourceWarnings
  visible; JS entry points parse successfully. Log:
  `.local/evidence/t44-vis46-capture-python-03.log`.
- Latest source, including persistent resize clamping:5 Execute UI tests,
  548 full TUI tests and strict workspace/all-targets Clippy PASS. Logs:
  `.local/evidence/t44-vis46-ui-{unit-08,all-06,clippy-06}.log`.
  The9 actual runtime Code Mode tests also pass after the captured fetch display
  input order correction (`t44-vis46-runtime-order-01.log`).
- Real paired traces exposed a stale projection after child expansion. A unit
  following production viewport observation reproduces it; invalidating the
  existing transcript cache after the typed toggle fixes it without loosening
  mouse/selection/paint-generation guards. Attempt04 verifies the actual action.
- A further first-frame hover regression reproduces `#b5b5b5` instead of the
  actual hovered foreground. Current painting publishes its unhovered geometry
  before deriving child hover; the latest target/full/Clippy rerun passes without
  loosening any pointer, selection or paint-generation guard.
- Attempt06 exposes947 differing cells after160→120 resize: an old detail scroll
  offset resurrects despite a wide frame having painted offset0. An actual
  TestBackend80→160→120 regression reproduces11 versus0. Persisting the actually
  painted scroll/pan in the existing detail owner fixes the cause; no global
  resize framework or comparison normalization is added. The latest five-test,
   full-TUI and strict gate above passes this regression.
- Unchanged real PTY regression target:60 PASS in142.88s, including S07/VIS38
  archive resources, VIS31 idle/burst pacing, question and selection ownership,
  terminal restoration and existing Shell lifecycle. Log:
  `.local/evidence/t44-vis46-ui-pty-regressions-01.log`.

Failed immutable attempts01–03 are retained: incorrect fixture loopback opt-in,
an opt-in placed before its environment owner, closed-versus-open child selectors
and the stale projection were diagnosed, not waived or called external blockers.
The controlled fetch uses the existing test-only loopback admission flag, never
a weaker production network policy. No real provider credentials are used.

## Risks

Attempt07 verifies the resize correction with both actual binaries. It also
exercises corrected gutter gap style, immediate
typed JSON-to-Logs flow, captured fetch input order, error indentation and hover.
Runtime elapsed values and
hidden terminal last-paint coordinates remain actual; neither is normalized or
painted to make a comparison pass. OSC52 delivery proves terminal transport, not
acceptance by a desktop clipboard. Shell success omission, Explored-group anchors,
bounded redaction/retention, authority and all remaining T44 gates are unchanged.

## Next

The current Execute UI target/full/strict gate, actual20-stage probe and unchanged
60-case real-PTY regression target pass their functional checks. Commit/push the
verified major UI slice, retaining the remaining full-frame differences and error
state incomparability. Continue every frozen R1–R6/VIS01–VIS46 outcome; do not mark
T44 complete from this functional facet.
