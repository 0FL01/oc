# NOW — актуальный handoff

State updated: 2026-10-06T06:23:55+00:00
Active: T53

Сверить Git status/diff до выполнения команд.
Task: T53 — OpenCode Go: единые credentials, native wires и connect/model TUI
Spec: docs/goals/2026-09-29-opencode-go-and-provider-auth.md
Evidence target: evidence/T53/report.md

Frozen T53 Go/custom contract: R1-R5 verified offline through 42b49e180, report/component receipts cover native wires, shared scoped SQLite auth, canonical bindings, public catalog, all-lane metadata/chronology/retries, durable replay and qualified configless account/model/child/fork TUI flows. Prior workspace 1648/0/10 and strict final gates green. T53 resumed after owner explicitly supplied authorized OC_API_KEY name in .local/live.env; earlier expected-name preflight was incomplete, not an upstream key rejection. GO06 in progress via scripts/t53_go_live.py --run using only test-process mapping to OPENCODE_API_KEY and fixed Go HTTPS authority. Same t53-go-20261006 ledger, durable cfg(test)-only pre-DNS budget <=24 all physical main/auxiliary/retry/probe calls and smoke <=2048; no credential logs, endpoint override, sweep or paid fallback. Finish only after real exact catalog-selected Responses/Chat/Messages text/tool and dated Qwen probes plus current final gates/report. T44 PAUSED, T57 and other task contracts unchanged.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

T53 implementation through reviewed/pushed `42b49e180`; R1–R5/GO01–GO05 verified
offline. Full task BLOCKED, not DONE: R6/GO06 required real Go text/tool/protocol
qualification is NOT_RUN. Factual report: `evidence/T53/report.md`; sanitized durable
zero preflight ledger: `evidence/T53/live-campaign.json` (campaign t53-go-20261006).
No Go generation/title/summary/child/retry/probe/MCP call has been made.

## Checks

Final implementation gates: workspace 1648 passed / 0 failed / 10 existing opt-in
ignored; strict workspace all-target Clippy, fmt, locked build, oc --help and diff
check all exit 0. Actual-binary combined configless Go ACK/explicit selection then
explicit custom fake generation/cancel/reopen, qualified busy Alpha/Beta/Alpha
requests, child/fork/retry/DCP/native-checkpoint/recovery regressions are green.
Real Go is not inferred from these fake tests. See component receipts and report.

Presence-only preflight: process OPENCODE_API_KEY absent; approved .local/live.env
exists but Go key name absent. OpenProxy test input names exist, values never
printed. Public source succeeds and currently declares both Qwen conflict rows as
Messages; actual generation routes NOT_PROBED. No other credentials were scanned.

## Risks

The only unavailable dependency is an authorized real Go Console key. Foreign
OpenProxy auth, anonymous paid probes, changing Go authority, private/redirect
bypasses and authoring/browser credentials cannot substitute. Zero ledger is not
proof of a qualified dedicated fixed-authority live accounting harness. T53 remains
blocked, T44 PAUSED, T57/other tasks untouched; no finish or overall READY claim.
User-owned untracked .opencode/ remains unread/unstaged.

## Next

After authorized Go input becomes available: resume T53 with `python3 scripts/progress.py start T53`;
reuse t53-go-20261006 and the same counters. Establish verified durable pre-dial
accounting for <=24 physical generations including all auxiliary/retry/probe calls,
smoke <=2048 tokens, no all-model sweep/fallback. Execute required exact current
Responses/Chat/Messages text/tool and dated Qwen probes, record sanitized facts,
rerun affected final gates, resolve R6, then finish. No independent T53 code/gate is
left unverified; do not start unrelated T56/T57 under this T53-only user request.


Ready (до 5): T56, T57
Blocked: T27, T43, T44, T45

Done в журнале не означает READY всего продукта; см. GOAL.md.
