# T36 — DCP in the real model tool loop

Status: **PASS for T36 mandatory offline scope**, not overall product READY.
Implementation: `4ae0f0a` on base `1aac158`; finding **F07**.
Contract: `audit/repairs/T36.md`. Initial red: `regression.md`; executed gates:
`checks.md`. No live/paid endpoint or real credential was used.

## Result

The actual `oc` application now advertises and executes `glob`, `grep` and
`compress` as ordinary model function tools. They share validation, central
permission, durable intent/outcome and typed continuation. The runtime emits a
bounded DCP developer lane with stable raw/block anchors and a due transient
nudge; it does not mutate raw history or persist reminder messages.

Model-generated compression plans all ranges before one SQLite transaction.
Blocks/members, consumed memberships, durable dedup/purge projection, tool
outcome, turn wire journal and nudge reset commit together. The next provider
round reloads the smaller projection immediately. No-gain leaves projection and
cadence unchanged and returns a visible structured result.

Config is loaded through admitted global/project sources and native DCP aliases,
without npm/JS execution. Numeric/percentage limits, exact provider/model
overrides, strong/soft nudges, manual mode, summary/turn protections, strategy
protections, compress permission and documented nested shape are effective.
Unsupported updater/capability modes fail explicitly.

## Acceptance evidence

### AUD19 — real model-visible tools

`aud19_aud20_aud21_binary_model_compress_nudges_and_restart` launches the real
Cargo `oc` binary with isolated HOME/XDG/project/data and captures Responses
requests. The first request must contain strict schemas for `glob`, `grep` and
`compress` plus ordered `{id,role,closed}` anchors. The fake model generates all
three calls. Their structured outputs return as correctly linked
`function_call_output`; SQLite contains durable operations in call order.

Search implementation additionally enforces: 4096-byte pattern, 64 glob
segments, 10,000-entry walk, memoized `**`, 1 MiB/file and 16 MiB aggregate grep
scan, 2 KiB UTF-8 hit text. No-follow/nonblocking regular-file reads prevent FIFO
hangs. Direct bounds, wide-directory and FIFO regressions execute in adapter unit.

### AUD20 — actual nudge and strategies

The binary fixture captures due advisory/required nudge text in the provider
input, checks cadence without duplicate accumulation, runs model compression,
then starts a new `oc` process and proves persisted cooldown. A separate session
does not inherit state. Manual/disabled/denied profiles expose no autonomous
compress schema/anchors/nudge.

Runtime tests use captured typed requests to prove session/provider/model-keyed
state, database/runtime restart, percentage/model thresholds and summaryBuffer.
Strategies run only with successful compression and commit with it. Duplicate
canonical calls hide older complete call/output pairs; purge replaces only old
large errored arguments and retains exact outcomes. Identity includes call-ID
occurrence. Wildcard/global/strategy tool protection, parsed patch affected paths,
configured file globs and recent turns remain exact. A second compression proves
durable strategy state and reused provider call IDs do not drift.

### AUD21 — durable accurate compression

`dcp_atomic` injects SQLite failure during the second range and proves zero
blocks/members/outcome/journal changes. It also proves no-gain/no-write, verbatim
user/tag/file protection, out-of-order range-summary association, nested block
anchors, consumed membership replacement, and unknown/cycle/depth/size rejection
before commit. Raw messages remain immutable.

Runtime model tests compress a real completed shell turn, retain its opaque/call/
output graph, restart, continue without repeating the side effect, and preserve
recent completed turns. The binary fixture requires the next request to be
smaller and retain a summary fact across process restart.

`aud21_binary_sigkill_never_publishes_a_partial_multi_range_compression` uses a
temporary test-only `LD_PRELOAD` shim. After observing durable compress intent it
pauses actual SQLite DB/WAL fsync/fdatasync inside the 32-range/1792-member commit,
then SIGKILLs the actual `oc`. Reopen observes only zero-or-complete compression,
consistent operation/journal state, unchanged raw history, recovery without
automatic replay, and successful next explicit prompt. No production failpoint.

## Storage/projection invariants

- Stable block rows remain for bounded nested expansion; active memberships move
  atomically to the replacement block and use raw start/end anchors.
- Complete function-call/output pairs are retained or removed together. Opaque
  reasoning is never rendered. Protected bytes are not replaced by a claim.
- DCP context protections do not grant file-mutation permission. `apply_patch`
  continues through its independent central policy and confined filesystem path.
- Durable nudge preference reset occurs in the same transaction as compression;
  memory updates only after commit. Manual host compression uses the same planner
  and atomic commit boundary, not compensating deletes.

## Verification

Final targeted: adapter unit 124, runtime 26, atomic DCP 4, actual-binary DCP 2,
soak 4 — all pass. Final `cargo test --workspace --locked`, workspace/all-target
clippy `-D warnings`, fmt check, locked build, `oc --help`, progress/docs checks
and `git diff --check` all exit 0. Exact suite table is in `checks.md`.

Three pre-existing external-only harnesses are ignored/NOT RUN, not PASS; no T36
test is ignored. No dependency, Cargo lock, package count or schema migration
version changed. Independent final read-only review reported no T36 blocker.

## Explicit remaining scope

T36 does not claim semantic quality of arbitrary model summaries, live provider
compatibility, T39 DCP panels/commands/notification rendering, or T40 bounded
whole-archive materialization/RSS lifetime. Parsed `showCompression`, notification
channel and commands display belong to T39. T27 remains product-blocked until T42.

Next: T37 — real MCP auth/config generation lifetime and cancellation through the
actual application, using offline fake transports first. Audit fragments remain
one-shot and must not be merged again. No push performed.
