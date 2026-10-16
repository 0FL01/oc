## Result

T31 finished: F01 wiring repaired in implementation commit `7812379`. Actual
`run` and `tui` share config/catalog/Responses/Runtime through one application
owner. No normal mock fallback or frontend persistence. Durable acceptance,
typed errors, incremental text delivery and own persistent XDG namespace wired.
Evidence and scope: evidence/T31/report.md.

## Checks

AUD01 red baseline exit 101 → final PASS. AUD02 CLI→PTY→CLI restart verifies
prior history in actual requests and durable data. Busy input not persisted;
actual CLI UI05 observes a delta then cancels with exit 130. All 13 PTY tests
pass. fmt, workspace clippy -D warnings, workspace tests, locked build, --help,
diff and journal/docs checks exit 0. Three external-only harnesses remain ignored,
not PASS. Bounded results in evidence/T31/checks.md.

## Risks

Only F01/AUD01/AUD02 close here. T32/T33 P0 and F02–F18 remain product gaps.
Silent-peer cancellation/typed Responses/buffering need T34; whole-history and
64KiB assembly need T40. Config definitions/trust and real MCP/DCP still require
their tasks. No paid/live probe, no valuable-data tool mutation, no push.
Product is NOT READY, not BUILD_READY_LIVE_BLOCKED.

## Next

Start ready T32 with scripts/progress.py start T32; read audit/repairs/T32.md and
its directly related regression sketches. Reproduce first on temp fixtures,
repair patch owner, then targeted gates/checkpoint. Do not re-merge fragments.
After T34 rerun the same AUD01/AUD02 binary tests against structured protocol.
Implementation HEAD is 7812379; this closeout changes evidence and journal only.
