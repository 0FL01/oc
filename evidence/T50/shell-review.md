# T50/R10 shell capture — scoped parent review correction

## Coordinator qualification

The complete shell-producer atomic and both reachable review findings are verified.
Independent review followed the original Jobs/supervisor, bounded actual drains,
resource writer and shared quota, published extents, redaction carry, terminal/live
snapshot, notices and outcome/history consumers. No second resource owner, capture
at conversion, producer replay or cold-autoload path was introduced.

Fresh parent commands exited0: `tool21_` **24 tests**, workspace fmt, strict locked
all-target Clippy and normal debug build. After the last build both normal ELFs
passed **13 directed cases each**, **26 native cases total**: four common cases,
short Interrupted capture, artifact cap, quota, five actual shell-control cases,
and same-task model-switch/compaction/restart projection. Each profile observed
**77 physical fake POSTs**; no selection/conversion-induced idle generation.

The real shell chain retained3740084 admitted bytes as Complete, exposed distant
read/grep and useful continuation without reinflation. Conversion retained the same
PID/capture with1100025 bytes; cap16777216, quota and registration failure kept
known effects separate from capture loss. Short22-byte Interrupted capture remained
completed/exit0/effect1 and explicitly incomplete after restart. Projection retained
one2616049-byte resource through A→B→A, compact and reopen without replay.

Current full workspace receipt below remains1450/0/10,42 targets, exit0; no compiled
source changed after it. Python47, documentation/progress structure and diff checks
also exited0. Normal fingerprints matched before and after every parent direct run:
debug `cafa1ba844eb01282986603de44c951c3964979a40aa991861b052c69bc7d2c0`;
release `d2f0b426ca35b7254f410583f46e275ef1b3ee903c966c84840e7df6f197c20b`.
Base9d8b1b818 plus this reviewed implementation is the actual compiled association;
intervening owner commits are documentation-only. Fixture-owned groups, drain/PTY
readers and HTTP workers joined before exact cleanup. No actual HOME/config/env,
credential or paid campaign access occurred. Full T50/GOAL and separate T44 visual
qualification are not claimed by this atomic delivery.

## Result

Frozen BEFORE corrective source edits at HEAD
`718f56e36569db9c1d78d682b3035000844a1a30` + inherited owned shell DIRTY.
Original source BASE remains `9d8b1b8186ec2a140fb28db1681fc58b89086120`.
**PASS — both reachable review findings corrected and current source qualified.**
Interpretation was frozen before the edits. Original shell delivery receipts remain
historical; this report supplies current-source qualification. Return HEAD
`44af1f8a7161759628f7d2099bfbb8838805338d` adds the owner's documentation-only Long
Horizon commit after entry HEAD; no committed Rust changed, and those docs are preserved.

| Obligation / frozen interpretation | Proof | Status |
|---|---|---|
| Short interrupted capture must be explicit despite known exit0 | Actual Jobs leader emits short prefix/effect1 and leaves a group descendant holding pipes; supervisor disposes group; resource Interrupted, bounded native/provider/hot-history text advertises it; execution stays completed/exit0/no logging-failure invention/replay | PASS |
| Ordering facts must name the actual ordering domain | stdout partial secret at raw read1, stderr complete at2, stdout suffix at3. Safe carry necessarily permits stderr publication first. Artifact labels describe serialized normalized/redacted **publication**, not raw reads or OS emission. Fixed first/last raw-read sequence counters per stream plus carry-release counts expose original bounded provenance; no event list, unsafe flush or delayed stderr queue | PASS |
| Resource/legacy seams unchanged | Same Jobs/Db/Writer/descriptor JSON; optional serde-default bounded counters, no migration/dependency/new owner or historical rewrite | PASS |
| Current qualification | targeted RED/GREEN, strict fmt/clippy, full workspace, normal debug/release/help and direct chain/negative/FG-BG/live/final/projection/short-incomplete proofs; Python47 and read-only metadata/diff/size; exact owned cleanup | PASS |

This accepts normalized/redacted admitted-text publication serialization as the
observable combined capture order. It deliberately does not claim earlier raw
stdout was published first when safe normalization/redaction withheld it. First/
last sequence counters are one-based nonempty read admissions to the existing
Capture lock; carry-release counters count publications using prior per-stream
carry. They are bounded summary provenance, not a complete raw-arrival event log.

## Checks

### Current changes / proof meaning

- `shell/jobs.rs`: prepare every non-Complete native resource, including small
  Interrupted prefixes. `capture_failure`/logging_failed remain separate: known
  completed exit0 is not changed to failed merely for producer interruption.
- `shell/jobs/output.rs`, `storage_tool_output.rs`: fixed serde-default first/last
  nonempty Capture-ingress sequences and carry-release counters per stream. Format
  explicitly names normalized/redacted publication serialization. Reader/Writer,
  cap/quota/TTL/provenance/policy ownership stays the existing owner.
- `tools/output.rs`: private owner seam `StreamRedactor::has_pending` measures carry
  attribution, not new output flushing or redaction semantics.
- Permanent `shell/jobs/output_tests.rs::tool21_review_*`: actual short job/restart/
  stored hot history and exact artifact order + typed secret/UTF-8 provenance. Raw
  stdout1 remains withheld while stderr2 publishes; stdout3 safely releases carry,
  so exact cold text is stderr then stdout while typed stdout first/last are1/3,
  stderr2/2 and stdout carry releases1. No invented original byte-event log.
- `native_tool_output_common.py --case shell_interrupted`: actual normal provider
  graph/restart/read of the22-byte Interrupted prefix, completed exit0/effect1,
  no logging-failure event, reap and no replay. Existing chain also validates new
  descriptor ordering-domain/first-last fields in the actual native resource.

Permanent `shell-review-red.log`: two behavior failures, short native Jobs outcome
unprepared and false/underspecified ordering format. `shell-review-native-red2.log`
on the prior normal debug ELF narrows finding1: provider common preparation and
replayed tool history already advertised Interrupted; the native stored short Jobs
outcome did not. No claim that the provider defect was reachable; no extra provider
fix. Initial native probe used an incorrect `exit: 0` oracle (native text is `exit 0`),
preserved in `shell-review-native-red.log` and corrected before the genuine RED.

First current full workspace `shell-review-workspace.log` completed983.972s/exit101
with one unrelated VIS38 resource-PTY teardown failure: status SIGINT after terminal
restoration (`lifecycle.rs:1380`). Nearest exact test passed14.379s. Its two control
bytes raced terminal restoration; this resource probe now uses neighboring probes'
explicit `/quit` teardown, retaining success/restoration/resource/deadline assertions.
No TUI production behavior or shell deadline was changed.

Second full run `shell-review-workspace2.log` completed975.957s/exit101 with a
different unrelated VIS31 input-paint100ms bound failure (no shell tool in this
workload). Exact nearest rerun passed7.519s: max30.532ms at165Hz /20.424ms at250Hz.
Read-only diagnostics `shell-review-cpu-sample.log` and CPU PSI showed contention
(avg10=24.05, avg60=22.41), not proof of an R10 causal regression. The assertion and
TUI production source are untouched; one bounded full-gate retry follows the
nearest/host diagnosis, not repeated blind retries or a weakened timing gate.

The retry `shell-review-workspace3.log.gz` is current exit0; both earlier full
failure logs remain exact lossless archives. No100ms timing bound, ignored test,
baseline, product deadline or resource cap was weakened. VIS38 resource-only
`/quit` teardown is the sole unrelated fixture change; no TUI production change.

### Current gates / exact logs (all exit0)

Commands/exits/seconds/bytes are recorded in existing `shell-gates.jsonl` under
`review-*`. Original `.log` receipts map to exact gzip-n `.log.gz` archives.

| Gate | Receipt |
|---|---|
| Targeted two review tests | `shell-review-green.log`,2 passed |
| Owner `tool21_` | `shell-review-owner.log`,24 passed |
| Existing shell controls | `shell-review-controls.log`,9 passed |
| Strict fmt / workspace all-targets locked clippy -D warnings | `shell-review-fmt3.log`, `shell-review-clippy3.log` |
| Separate precompile | `shell-review-precompile2.log` |
| Full locked workspace --no-fail-fast | `shell-review-workspace3.log.gz`, **1450 passed/0 failed/10 unchanged ignored**,42 summary targets, exit0/964.886s (962.0s target times),133503 decoded bytes |
| Actual-binary MCP media | `shell-review-media.log`,3 passed |
| Normal debug/release builds | `shell-review-build-{debug,release}.log`,10.618s/148.054s |
| Isolated normal help / ELF kind | `shell-review-help-{debug,release}.log`, `shell-review-elf-kind.log` |
| Python47 (13/5/15/14) | `shell-review-python47.log` |
| Docs / progress read-only | `shell-review-docs-return.log`, `shell-review-progress-return.log` |
| Changed source advisory / diff | `shell-review-advisory.log`, `shell-review-diff.log`; no oversized source introduced |
| Current normal hash/count/owned cleanup audit | `shell-review-audit-return.log` |

Last normal builds preceded all current direct proofs; no Cargo command between
those builds and direct proof/hash audit. BASE+DIRTY association above, normal SHA256:

- debug `cafa1ba844eb01282986603de44c951c3964979a40aa991861b052c69bc7d2c0`
- release `d2f0b426ca35b7254f410583f46e275ef1b3ee903c966c84840e7df6f197c20b`

### Current native rows / requests / effects

**28 cases/profile,56 total PASS.** Common4, new Interrupted1, shell cap1/quota1,
controls5, projection1, question14, image/DCP1; exact logs
`shell-review-{common,incomplete,cap,quota,controls,projection,question,image}-{debug,release}.log`.
Directed shell/common/control/projection subset: **77 actual physical POSTs/profile
=54 main +22 title auxiliary +1 compact summary**; question/image regressions are
additional cases, excluded from that POST sum.

New short proof:6 POSTs (4 main/2 auxiliary),2 native rows (shell completed399-byte
bounded output/read completed347 bytes), one22-byte Interrupted artifact, known
exit0/effect1, logging-failure facts0, actual producer reap; provider replay contains
the same Interrupted result and explicit read obtains its prefix without executing again.
Both ordering examples publish stderr first while metadata retains stdout raw1..3
versus stderr2..2; deferred secret is redacted and split UTF-8 remains二.

Chain still completes one >3.7 MiB resource with actual distant read/grep/continuation;
FG→BG Complete1,100,025 bytes with the same PID/identity/live/final notice; selected
cancel/crash remain Interrupted; negative quota/cap/registration retain independent
exit/effects; A→B→A/manual compact/restart retains one >2.6 MiB capture with effect1,
leader reap and no cold inflation. Exact bytes/native rows per profile are in logs.

All owned process groups, drain workers, PTY readers and non-daemon HTTP handlers
join before exact TempDir cleanup. Recorded gate/native TempDirs are absent at
final audit. Conservative cumulative own shell evidence (including prior phase,
all failed archives and touched fixtures) is below650 KiB, below1 MiB; every decoded
log <=16 MiB. No retained fixture Cargo trees/global cleanup/foreign signals/private
inputs/live calls/task-state writes/staging/commit/push. Inherited `.opencode` untouched.

Only owned offline fixtures. Prior shell logs/receipts/history remain immutable.
Jobs3/threads1/offline/approved TMPDIR; one serial Cargo. Whole workspace command
gets1798s watchdog/1800000ms tool room; product/test deadlines remain unchanged.

## Risks

Interrupted capture is not a storage failure and must not overwrite successful
leader exit/effects. Secret/UTF-8 carry cannot be flushed early to restore raw order.
Bounded provenance summaries cannot claim a complete per-byte raw-arrival trace.

## Next

Parent independent review/bookkeeping; remaining whole TOOL21/T50/GOAL acceptance
is not claimed. All mutation/Cargo/offline-fixture ownership is explicitly released
with final return; parent may resume then. T44 PAUSED and paid ledger untouched.
