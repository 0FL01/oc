# Session MCP sidebar (2026-10-17)

## Result

The owner authorized the audited MCP-sidebar slice, commit/push and a debug build.
Base: `a0666a2b56381648a194b3af96b5db835d897c20`.

The session sidebar shows the existing Location inventory after Context: typed
status, colored dots, bold safe names and right-aligned labels. Empty hides MCP;
original fold/summary semantics remain. Opaque-ID row clicks reuse the existing
list/Failed details, never connect/retry/authenticate/generate.

Pinned title/Location surround one bounded Context/MCP body. Paint/input share
clamped geometry; stale gestures cannot survive snapshot/route/resize/key/paste.
Sidebar wheel, including pinned/padding areas, never scrolls history.

One global `tui.sidebar.mcp.open` bool uses existing prefs/query owners: bounded
reads, no bad-content repair, real errors propagated, writes projected after ACK
without draft loss. No migration, lifecycle/provider/DCP/poll/plugin-host/user
config changes. MIT attribution is retained.

## Checks

- Renderer, actual row opening, capture invalidation, existing Home/modal tests:
  `cargo test -p oc-tui --lib --locked mcp` — 14 passed.
- Real-owner bounded/global/restart preference and ACK/activation routing:
  1 passed each.
- Actual-binary sidebar lifecycle PTY — 1 passed: Connecting to Connected without
  opening a modal, disabled zero-spawn, positive transcript/body wheel controls,
  pinned-area exclusion, fold/restart, no provider requests or MCP tool calls,
  and owned process cleanup.
- Renderer, separator and paste regressions: RED before fixes, then GREEN.
- Python AST/Node syntax pass; no test/security assertions or limits weakened.
- Final current-source workspace fmt and strict workspace/all-target Clippy:
  PASS. `cargo test --locked --workspace --no-fail-fast`: **FAILED**, 2031 passed,
  10 failed, 12 unchanged opt-in ignored cases across 46 targets. Full TUI,
  binary-routing and MCP-application targets pass, including the new scenarios.
- Three non-path failures: shell job conversion times out at
  `shell/jobs/tests.rs:105`; context/children captures 8 requests instead of 10 at
  `tests/runtime/context.rs:183`; cancellation sees 0 children instead of 1 at
  `tests/subagent.rs:1553`. These exact scenarios are not edited by this slice.
  Their cause is not attributed to owner edits or load without a control run.
- Seven other failures are the existing hardcoded other-account path fixtures:
  auth CLI/TUI, malformed-config and Location diagnostics, startup/discovery,
  and terminals. No failing tests are excluded or waived.
- Final `cargo build --locked`, `target/debug/oc2 --help`, and `--version`: PASS;
  version is `oc2 0.2.0`. Debug artifact:
  `/home/stfu/ai/agent-frameworks/oc/target/debug/oc2`.

### Supplementary source-compatible observation

The requested donor is `bc5ebf00a4c169ece10f276e6ce02eaf18c326e6`, version 2.0.31,
not frozen T44 2.0.12. The separately pinned official npm executable is from
release `22728d67461260d56bb9796b9b8f36e1de47202e`; its four relevant sidebar/dialog
sources were byte-compared with the supplied checkout and match. Archive SRI was
verified; executable SHA-256:
`2df936a20cf74af5706020342164628f9a31978c50c2b0ad306b42b614fe245d`.
The old frozen capture guards and acceptance are unchanged.

Current complete capture:
`evidence/tui/mcp-sidebar/supplementary-attempt-003/`.
Both origins pass all six scenario checks and exit naturally with code zero.
Each starts exactly one healthy peer and initializes that same PID; neither
disabled peer starts, and no MCP tool is called. Folding adds no provider calls.
Actual captured row labels, positions and dot/name/status style observations are
identical. Four complete stable unmasked styled grids/PNGs/cursors are retained.

Whole-frame results are separately **DIFFERENT** for all four comparisons:
expanded/folded each differ in 459/7680 styled cells and 61620/1036032 PNG pixels;
cursor pairs match. This is not frozen qualification or whole-frame parity PASS.
No cropped, masked or normalized comparison replaces these full results.

Failed attempt 001 and superseded 002 remain intact; their exit/label-fixture
issues were diagnosed without relaxing native masking or capture criteria.
Raw artifacts are local and gitignored, as in existing capture profiles.

## Risks

R6 remains FAILED; older/interrupted runs and individual controls do not replace
the completed workspace result. No other HOME/host workaround, assertion/limit
weakening or full VIS/T44/READY claim.

Application is 5517 lines (+16); query dispatch is its next natural seam, not a
one-method module for this scalar operation.

Development/captures used two CPUs/jobs; the owner then authorized all cores.
Final run: 16 jobs/16 test threads, no affinity cap, sequential Cargo, no target
cleanup/override. Logs: `/home/stfu/.cache/opencode-tmp/opencode/mcp-sidebar-aCAnG0/`.

## Next

Commit/push before investigating the failures, as requested; this note precedes
delivery, whose record is Git. T44 stays open. The owner's interactive alias was
not launched or asserted to resolve here.
