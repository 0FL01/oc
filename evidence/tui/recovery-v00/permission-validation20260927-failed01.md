# Failed independent validator attempt

Command: `node scripts/tui_capture/check_permission_evidence.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-validation20260927-01.json`

Exit 1. Assertion at line 31: `3 !== 2` for the childread operation count.
The initial counter grouped title requests by their current user case, so it counted
the native child title alongside its tool request and result continuation.
Correction: count transcript lifecycle requests with `operation !== 'title'`, while
retaining all auxiliary requests in total counters. No historical capture changed.
