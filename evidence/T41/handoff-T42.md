## Result

T40 (F15) and T41 (F18) are closed, committed and pushed. This checkpoint
hands T42 over before implementation starts: the session's context budget is
exhausted, so T42 must begin from a fresh context.

Done in this session:

- T40 `142173e` + `9e529a5`: bounded active projection (active_history,
  project_active_rows, wire_logs_for_window), independent 16 MiB byte budget
  with explicit ContextOverflow, tool-op preview + read_tool_op_output
  continuation, TUI input budget parity with visible notes, AUD32-34 tests
  and real-binary RSS/PSS measurements (RED 169.9 MiB -> 27.9 MiB peak HWM
  for a 152 MiB archive).
- T41 `b565993` + `e239315`: strict binary golden campaign
  (`crates/oc/tests/golden_binary.rs`, 11/11 checks), bounded live campaign
  harness (`crates/oc/tests/live_bounded.rs`) with offline dry-run branches
  (with/without MCP) and machine-readable BLOCKED semantics for explicit
  runs without credentials; `e2e_live.rs` early return removed.
- Workspace gate at T41 close: 324 passed / 0 failed / 4 ignored; clippy,
  fmt, build, help, progress/check_docs, diff-check all exit 0.

## Checks

`python3 scripts/progress.py show` -> Ready: T42. `git log --oneline -3`
-> e239315 (T41 closeout), b565993 (T41 implementation), 9e529a5 (T40
closeout). Working tree clean; origin/agent/oc-rust-port is up to date.

## Risks

- Ignored test count is now 4 (mandated live campaign); documented in
  `evidence/T41/report.md`.
- T42 (mandatory offline qualification), T27 (bounded live) and T30 (FINAL)
  are not started.

## Next

1. Read `audit/repairs/T42.md` and follow its order: capture RED first,
   minimal fix, targeted tests, then the full offline qualification run.
2. `python3 scripts/progress.py start T42` is already applied; continue with
   `checkpoint`/`finish` as usual.
3. Reuse the T40/T41 harnesses: `context_bounds.rs`, `memory_bounds.rs`,
   `golden_binary.rs`, `live_bounded.rs` (dry run) and the PTY suites.
