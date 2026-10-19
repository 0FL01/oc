# NOW — актуальный handoff

State updated: 2026-10-19T09:12:54+00:00
Active: нет

Сверить Git status/diff до выполнения команд.

Последний срез: T71 [done]; сверить незакоммиченный diff.

# T71 — complete subagent reports and truthful shell capture

Status: PASS — frozen R1–R5 complete; no whole-product READY claim.
Date: 2026-10-19
Contract: docs/goals/2026-10-19-subagent-reports-and-shell-capture.md

## Result

Base: b7d460e98e61ab4bf86584da88ebf40492398fe1.
Qualification code: d1b730b76493987277767c18bbca138cf7798c51.
Reviewed commits, pushed normally to origin/master:

- db8f8c300: freeze the audited correction and start T71.
- 827e0f46a: complete durable reports/recovery; exact winning FG result; bounded views.
- 660718ba7: staged notice admission, exact redaction dedup and safe capture causes.
- d1b730b76: remove a needless test-fixture borrow found by strict workspace clippy.

R1: remove the 2048-byte child result slice. BG stores the complete synthetic notice;
FG keeps its existing common limiter/artifact once. The existing child row settles
after execution and fallible cleanup, not at the earlier canonical assistant commit.
There is no second in-memory foreground result authority or last16 lookup.

R2: omit report bodies from inventory/live refresh before allocation. Large reports
do not hide otherwise fitting cards. Delivery/recovery processes full bodies singly.
Notice paging preflights complete batch bytes. Staged bodies/IDs enter the parent
journal only after canonical escaped input/schema and existing model admission.
The existing 16MiB aggregate context guard refuses overflow explicitly; it is not a
new per-report cap. Durable reports/notices stay intact.

R3: remove the 8192-character recovery substring; follow the exact committed child
turn's assistant reference. Preserve first settlement, delivery receipt/dedup,
conversion, safe checkpoints, source/lineage, cancellation and unknown-effect rules.

R4: stable exact/nonempty collector dedup preserves all distinct protected literals.
The checked 65536-byte material guard remains fail-closed. Capture retains the first
observed safe boundary/code and returned ErrorKind/errno where available. Later
facts/lookup/reopen failures cannot replace it. Exit/effects remain independent.

## Checks

Red evidence reproduced before fixes:

- Storage returned2048 bytes for120015/91027/126017-byte reports.
- An escaped over-budget notice reached an actual second parent request.
- The duplicate-heavy collector returned261 entries instead of10 distinct values.

Targeted green checks, using private fixture data and local fake providers:

- `cargo test -p oc-adapters --locked --lib storage::children::tests`:17 passed.
  Includes full reports, bounded cards/events, complete-page byte preflight,
  statement/commit faults, lost acknowledgement, reopen, exact recovery and dedup.
- `cargo test -p oc-adapters --locked --lib runtime::children::tests`:7 passed.
  Includes exact foreground result outside the latest16 jobs and retained fences.
- `cargo test -p oc-adapters --locked --test subagent`:44 passed,28.50s total.
  Actual BG user-message report over64KiB survives exactly, with no BG artifact;
  actual FG limiter creates one artifact and real read reaches its distant suffix
  without another child request. Concurrency, continuation, DCP and source tests pass.
- `cargo test -p oc-adapters --locked --lib runtime::turn::context_tests`:2 passed.
  Escaped overflow causes no second POST or rejected notice IDs/body in checkpoint;
  the completed read and complete stored notice/result remain available.
- `cargo test -p oc-adapters --locked --lib duplicate_binding_redactions`:1 passed.
  Real silent shell exits0 with reopenable zero-byte Complete capture after dedup.
- All11 capture scenarios pass, including unique65536/65537 bytes, split secrets,
  real errno, facts/rename faults, interruption, quota/cap and exit17/effect-once/reopen.
  Largest targeted capture case21.36s; no targeted test exceeded30s.
- `python3 scripts/check_docs.py` and `git diff --check`: passed.

Full gate: `bash scripts/test.sh` exits0 at qualification code above: fmt, strict
workspace/all-target clippy,2114 nextest passed/12 pre-existing skipped, doctests,
`cargo build -p oc2 --bin oc2 --locked`, executable help/version (`oc2 0.2.0`).
Cold total1220.05s includes10m14s test compilation and2m53s debug compilation;
nextest execution384.506s. This is not a warmed-gate timing claim.
RECON: sole >30s case is UI07 discovery deadline,32.471s. Its existing owner-approved
2026-10-17 <=40s exception for the real30000ms budget is recorded beside the test
at crates/oc/tests/pty_t39/provider_readiness.rs:661. Others <=28.275s. No new exception.
The first gate stopped on one needless test borrow; fixed without lint suppression,
weaker assertions, reduced workload or a production change.
Raw synthetic logs remain gitignored under `.local/evidence/T71/`.

## Risks

- Existing context/model/transport, quota/lease/durability and permission guards stay.
  Technical Completed or provider length termination is not semantic goal success.
- No schema migration, dependencies, publication subsystem, BG report artifact,
  paid/live campaign, config/env edit, interactive launch or owner restart.
- Old clipped terminal receipts/notices and RAW are not rewritten or replayed.
  Their complete original child history remains accessible through existing links.
- The historical12 shell Io failures cannot be attributed to a specific cause from
  persisted generic diagnostics. Dedup fixes a proven code defect, not lost history.
- Default debug destination is this worktree's `target/debug/oc2`. The documented
  `~/ai/oc` alias checkout is absent in this environment; no alias was changed.
- runtime/turn.rs already exceeded5k lines; this bounded change stays with its request
  owner. Captured request preparation/admission is its next natural extraction seam.

## Next

T71 complete. No T44/T45/T61 resume, extra follow-up task or whole-product READY claim.


Следующий шаг: проверить зависимости и начать первую ready-задачу.

Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
