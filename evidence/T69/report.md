# T69 — subagent invocation compatibility

Status: PASS — bounded T69 correction, R1–R4 verified
Date: 2026-10-18
Source base: `02ffba50c25549f6653f2756a68cffd8a71383bb`
Plan commit: `5914d9458f39dedd58da3a8c14d3dc5bf1a5e30b` (pushed)
Reviewed implementation commit: `df2358176b30ed63925262cbcd98f1e625e0b132` (pushed)
Contract: `docs/goals/2026-10-18-subagent-call-compatibility.md`, R1–R4.

## Result

The shared typed subagent parser reuses existing structural validation and treats
exactly empty `model` and `sessionID` strings as omission. It does not trim other
strings or mutate the original tool arguments. Fresh children retain profile→parent
model selection; continuation preserves its own saved model unless explicitly
changed. Unknown/foreign IDs remain errors, never a fallback to fresh creation.

Foreground batching derives the same-child key from this canonical request only
after successful admission. New empty-ID calls can overlap; admitted continuations
retain predecessor ordering and actual execution joins after FG→BG conversion.
Refused leaves do not join, cancel or replace another child's predecessor.

Both ordinary Responses function-lowering maps now explicitly emit `strict: false`
beside the unchanged parameters. Subagent still requires only `agent`, `description`
and `prompt`; the shared WS framing retains this policy. Chat/Messages, ToolDef,
product configuration, storage and model resolution are unchanged.

## Required outcomes and direct evidence

| Outcome | Direct evidence | Result |
| --- | --- | --- |
| R1 | Parser table; complete reported argument shape through fake Responses; one nonempty child ID, parent-model fallback, independent history and exact durable/replayed argument string | PASS targeted |
| R2 | Existing fresh overlap/reverse completion/one durable effect, saved-model same-child continuation and BG fixtures; held foreign child survives two refused continuations and cancellation of the foreign parent; existing actual FG→BG join/fences unit | PASS targeted |
| R3 | Actual HTTP PROV01 and runtime subagent definitions, unchanged three-field required list, nonempty Key/OAuth WS initial/follow-up function captures | PASS targeted |
| R4 | Warmed full gate and timing budgets pass; debug artifact built; reviewed implementation pushed to verified origin/master, factual closeout recorded with this report | PASS |

## Significant experiments

1. Original parser failed both unit regressions: empty strings became `Some("")`,
   and direct parsing accepted a null optional value. The full runtime reproduction
   failed with the exact `error: Subagent session not found: ` output (0.88s).
2. Parser-only correction made both units and the full runtime reproduction pass,
   including exact durable/replay assertions. Fresh overlap still failed (5.97s)
   because the scheduler keyed independent calls by raw empty IDs. The held foreign
   regression also failed (4.38s): the second refused continuation joined foreign
   work instead of returning while it was held.
3. Existing HTTP and WS captures failed because emitted `strict` was absent. Adding
   it to both Responses maps fixed both captures without modifying parameters.
4. With admitted normalized keys and both wire edits, all targeted checks below
   pass. No timeout, assertion, workload, durability or security guard was weakened.

These failures were observed and then corrected; they are not current blockers.
Local diagnostic logs are under gitignored `.local/evidence/T69/`: `parser-red.log`,
`fresh-red.log`, `*-stage-one.log`, `*-green.log` and `subagent-green.log`.

## Checks

All product fixtures use isolated roots and synthetic loopback providers. One Cargo
owner uses the normal `target` path and approved disk TMPDIR; no live campaign,
user configuration/store mutation or interactive `oc2-dev` launch was performed.

| Command / evidence | Actual result |
| --- | --- |
| `cargo test -p oc-adapters --lib --locked tools::tests::subagent -- --nocapture` | 2/2 PASS, 0.00s |
| Same command with filter `provider::tests::prov01_exact_url_auth_store_cachekey` | 1/1 PASS, 0.00s |
| Same command with filter `provider::websocket::tests::auth04_runtime_channels_settle_read_followup_fork_and_partition_opaque_authority` | 1/1 PASS, 2.91s |
| Same command with filter `foreground_conversion_releases_only_waiter_and_retains_actual_join_and_fences` | 1/1 PASS, 0.65s |
| `.local/tools/cargo-nextest nextest run -p oc-adapters --test subagent --locked --no-fail-fast` | 43/43 PASS, 0 skipped, 25.734s; run `e9b7ae3f-cfe0-4f69-9697-a55a94ce70f8` |
| `bash scripts/test.sh` | Exit 0: fmt, strict all-target Clippy, nextest 2089/2089 PASS with 12 existing skips, doctests and debug build/help/version PASS; nextest 372.146s, whole cold/relinking gate 772.08s |
| Same full gate after the diagnosed exact scheduling change | Exit 0: all gates PASS; 2089/2089 passed, 12 existing skips across 43 binaries; nextest 389.131s, complete warmed gate 399.34s; run `e8441b7d-7c1f-4f04-8e0d-bfa1684d530e` |
| `python3 scripts/progress.py check`; `python3 scripts/check_docs.py`; `git diff --check` | PASS; 69 tasks / 202 acceptance entries; journal/doc checks repeated for factual closeout |

The targeted suite's longest case was 21.403s; compilation is excluded. Fresh bug,
held foreign-child, overlap, saved-model continuation and BG cases passed in
2.167s, 2.689s, 2.892s, 2.532s and 2.912s respectively.

### Required timing RECON

The first full run (`df11ca00-e56d-4c7e-9cef-153267ae479f`) includes 4m43s of
workspace-test rebuilding and 1m25s of dev rebuilding; 772.08s is not a warmed-gate
measurement. Its log is `.local/evidence/T69/offline-gate.log`.

Two cases exceeded 30s. UI07 discovery took 32.501s, within its existing explicit
owner-approved <=40s budget for the real 30000ms discovery deadline (approval in
`docs/goals/2026-10-17-slow-test-acceleration.md`, R5; unchanged comment beside the
test). This is not a new exception or shortened measurement window.

`storage::children::tests::typed_effect_uncertainty_and_unanswered_calls_never_claim`
took 33.059s with concurrent storage neighbors. The unchanged exact case alone
passed in 13.01s (13.63s command wall time; `storage-budget-recon.log`). It retains
all 19 independent disk-backed fixtures, six effect names times three states plus
one unanswered-call case, with WAL/FULL durability and every assertion. Static
inspection found repeated fresh-schema setup/durable writes, no sleep/retry.
Owned-process `strace -f -c -w` confirmed 1361 `fsync` calls taking 13.032038s,
96.38% of traced syscall wall time; the unchanged instrumented case passed in
15.04s (`storage-io-recon.log`). The minimal correction adds this exact case to the
existing full-pool nextest scheduling override, avoiding neighboring durable-write
contention without removing any operation. The unchanged matrix passes in 13.032s
in the warmed gate, confirming the scheduling correction against the same checks.
The whole warmed gate passes in 399.34s (<10min), with nextest taking 389.131s;
the log is `offline-gate-warm.log`. JUnit inspection confirms 2089 executed cases
without failures. Only UI07 exceeds 30s (32.669s, within its previously approved
40s); the longest ordinary case is 27.920s. The 12 ignored opt-in cases are reported
by nextest, not emitted as JUnit testcases. No new exception, assertion reduction,
durability change or production storage edit was introduced.

## Changed files and boundaries

Production edits are limited to `crates/oc-adapters/src/tools.rs`,
`runtime/turn/foreground.rs` and `provider.rs`. Regressions use the new compact
`tools/tests/subagent.rs` plus existing `tests/subagent.rs`, foreground/background
fixtures and provider HTTP/WS captures. Documentation contains the accepted goal,
its task/journal registration, narrow TEST_PLAN/roadmap pointers and this report.
The evidence-driven R4 envelope extension changes only the test scheduler's
existing full-pool filter for the diagnosed 19-fixture durable storage matrix.

No dependency, schema migration, second Db, new Jobs/store/serializer framework,
parallel wrapper, session-prefix restriction, nullable repair or new validation
rule was introduced. Nonempty whitespace is preserved, not treated as omission.

The original incident's server-side schema transformation remains unproven without
its original wire. Explicit non-strict policy follows the newer donor and Responses
documentation; it is independent of the confirmed empty-string execution defect.
Native batching is tested, not an external `multi_tool_use.parallel` implementation.
T45 detailed/live qualification and T44/T61 paused status remain unchanged; this is
not whole-product READY.

## Delivery

Debug `cargo build -p oc2 --bin oc2 --locked`, `--help` and `--version` passed in both
full gates (`oc2 0.2.0`). The binary is the normal worktree `target/debug/oc2`, ready
for `oc2-dev` adoption. Implementation `df2358176` was normally pushed to the
verified `github.com/0FL01/oc` origin on `master`; the observed upstream divergence
after that push was `0 0`. This report, completed contract and generated T69 finish
leaf form the following factual closeout commit; final delivery is verified by its
ordinary push and clean/upstream Git check, not a self-referential report hash.
No working owner was restarted or signalled. A new debug ELF is adopted at the next
ordinary launch after existing native owner leases finish normally.
