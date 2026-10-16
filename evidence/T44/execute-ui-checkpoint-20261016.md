# VIS46 — implemented Execute UI, current paired qualification

## Result

Execute UI is implemented in the current worktree; **whole VIS46/T44 visual
acceptance remains OPEN**. Detailed evidence and source provenance are in
`evidence/T44/execute-ui-20261016.md` and the immutable
`evidence/tui/execute-preview-20261016-07` capture.

Both actual binaries complete20 stages: Receiving/Running/Completed live detail,
real OSC52 Code/Output, pan/scroll,80x24/120x40/160x48 modal geometry, child input
expansion, error interaction, reopen and restart. Each side retains exactly five
provider requests, two execute operations, three nested MCP calls and one local
GET; both process generations exit naturally with code0 and no effect replay.

The pinned original returns interpreter errors as completed tool metadata and
opens a Failed dialog. The frozen native contract instead requires a failed
parent's inline error toggle. These actual different states are recorded with
`error_state_equivalence:false`; they are NOT a newly approved parity exception.

## Checks

- Current Execute UI tests5 PASS; full TUI548 PASS; strict workspace/all-targets
  Clippy PASS. Logs: `.local/evidence/t44-vis46-ui-{unit-08,all-06,clippy-06}.log`.
- Runtime Code Mode9 PASS, including captured fetch display order; capture
  Python11 PASS and JS syntax checks PASS. Existing SQLite warnings stay visible.
- All40 full grid/PNG comparisons remain strict:4 PASS /36 DIFFERENT. Receiving
  transcript and dialog match completely. Resizing back to120x40 no longer
  resurrects stale scroll:947 differing cells reduce to two actual elapsed cells.
- Unchanged real PTY regression target:60 PASS, including S07/VIS38 resources,
  VIS31 idle/burst pacing, question ownership, selection and terminal restoration.
  Log: `.local/evidence/t44-vis46-ui-pty-regressions-01.log`.

## Risks

Remaining differences include actual elapsed labels, hidden last-paint cursor
coordinates, busy-footer/background presentation and the unequal error states.
No clock, pixels, cells, RAW history, security boundary or comparator is rewritten.
OSC52 proves terminal transport, not desktop clipboard acceptance. Existing Shell
success omission, Explored anchors and all resource/selection guards remain.

## Next

Commit/push the verified major UI slice. Continue current-source paired
question/form and remaining frozen R1–R6/VIS01–VIS46 qualification; do not finish
T44 from this functional facet or reopen completed T65/publication.
