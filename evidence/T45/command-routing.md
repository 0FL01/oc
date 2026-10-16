# T45 R3 command-routing atomic — frozen outcomes

## Coordinator qualification

The routing atomic is independently reviewed and qualified for delivery. The
original frozen outcomes and implementation history below are preserved. This is
not whole T45, full prompt/host/child-DCP parity, T44 visual PASS or product READY.

Reviewed the application command owner, session selection restore/commit, runtime
budget probe/admission/launch, durable acceptance and command-bound recovery.
The separate scoped static review found no concrete authority counterexample;
that review is not a runtime gate.

Fresh coordinator commands all exited 0:

- `cargo test --locked -p oc-adapters command_routing -- --test-threads=1`:
  one application owner and three real integration scenarios passed.
- Workspace fmt and strict locked all-target Clippy passed.
- Normal locked build restored the exact qualified debug binary.
- After that last build, both normal ELFs passed all 19 routing scenarios each:
  38 cases, 50 physical POSTs (28 main/child + 22 title), 16 single notices;
  16 refusals had zero requests, accepted turns, child jobs, intents or selection
  writes. Safe recovery retained original job/turn/admission IDs; changed
  command generation quarantined without redispatch. Idle reopens did not replay.
- Python 47, docs/progress structural checks and diff check passed.

The unchanged production source also has the complete current locked workspace
gate below: 1527 passed, zero failed, ten existing opt-in ignores. Native process
groups, shell leaves, HTTP handlers and PTY readers were joined/reaped before
exact owned fixture cleanup. No paid/live inputs or exhausted T27 ledger were used.

Both fingerprints matched before and after coordinator native qualification,
without intervening Cargo:
`36864e4fdbfc5c49fffb619878577759bbf8ab03eba0c6dad6fa2a4093e3eeb7`
(debug),
`dea2c385f6d9021b481258461d60e1e0fd4b09f15562e4894d098b3b1355c40f`
(release). Compilation associates BASE `788069c7772f7592273f456957e78c8517440f7c`
with the reviewed dirty implementation, not with clean-BASE binaries.

Base: `788069c7772f7592273f456957e78c8517440f7c`, donor
`2670273ff17da96f85c5826ced57aa1b368754fa`. Frozen before RED/source changes.
Owner: temporary sole mutation/Cargo/offline-fixture owner; parent delivers.

| ID | Measurable required outcome |
|---|---|
| C01 | JSON explicit true, Markdown deprecated subtask true, and inferred subagent mode launch one real background Jobs child; false overrides alias true and inferred mode. |
| C02 | Inline agent switch precedes explicit model; command model wins profile pin, profile pin wins captured parent fallback. Exact provider/model/variant and private profile prompt appear on actual wire. |
| C03 | Background parent selection remains byte-equivalent; child gets resolved command profile/model/variant, fresh context, bound Location/generation and narrowed parent/central/profile authority. |
| C04 | Literal slash invocation and expanded prompt remain durable raw input; reopen/recovery never rewrites them. Ordinary primary selection still excludes subagent-mode profiles; only source-authorized command inline selection admits them. |
| C05 | Unknown/disabled profile, invalid model/variant, Deny/Ask-without-consumer, depth and irreducible budget refusal cause zero accepted turns/children/tool intents/provider requests/selection writes. |
| C06 | Actual normal debug/release ELF fake-server proof observes running child progress before release, unchanged parent, real cancellation, same-job safe restart and one durable notice without duplicated execution. Typed states/counters determine success, never free LLM text. |
| C07 | Nearest RED/GREEN; fmt; strict locked workspace all-target clippy; separate no-run; full locked workspace no-fail-fast tests; normal debug/release builds/help; final ELF hashes stable across direct proofs; Python 47 and read-only docs/progress/size/diff gates. |

Implementation reuses definitions, application selection, runtime admission,
existing Jobs and existing SQLite history/recovery. No new generic scheduler,
store/schema/dependencies. Owner-authored closed subagent occurrence is a native
command launch receipt, not an invented provider response. Child budget probing
must reuse exact runtime request admission before accepting parent input.

Scope: this command-routing atomic only. Whole T45, executable/custom shell
command expansion, child DCP and T44 visual acceptance remain separate.
T44 PAUSED; exhausted T27 allowance untouched. No live/paid/credentials or
inherited root `.opencode` access. No staging/commit/push or planning/status writes.

## Qualification

The frozen C01–C07 outcomes are qualified on this uncommitted worktree. This
does not finish T45 or change its checkpoint, plan, acceptance or delivery state.

### Implementation and authority

- `crates/oc-adapters/src/defs.rs` exposes the existing native structured model
  parser to its crate; `composition.rs` retains complete `CommandDef` metadata
  alongside the existing command body/description maps.
- `application/commands.rs` owns captured route resolution: nullish
  `subagent.or(subtask)` then resolved profile mode; agent first, explicit command
  model second, with exact structured variant precedence. Background returns the
  captured parent selection and a typed child route. Inline false eligibility is
  source-scoped, persists through existing session prefs, revalidates on reopen,
  and retains parent profile permission ceilings. Ordinary primary selectors
  still reject that subagent-mode profile. Absent command agent resolves the
  admitted parent, including a previously source-authorized inline profile.
- `application.rs` and `application_selection.rs` prepare command selection
  read-only, then commit inline prefs with durable input acceptance. Existing
  session/fresh acceptance transactions in `storage.rs` own that atomic write.
- `runtime.rs`, `runtime/turn.rs` and `runtime/turn/commands.rs` reuse exact
  schema-inclusive request-budget admission and the shared child lane before
  accepting parent input. An owner-authored closed call/result occurrence feeds
  existing Jobs, tool intent and launch-source recovery. Only the private command
  launch catalog admits primary profiles; model-tool eligibility stays native.
- `runtime/children.rs` retains the existing fence/route/profile/Location/source
  validation and adds command metadata fingerprinting plus a typed launch receipt
  for primary-profile recovery. A changed command generation quarantines the same
  job as unknown, makes no additional request and emits one terminal notice.
- `runtime_compaction_tests.rs` updates the internal call signature;
  `tests/subagent.rs` registers `tests/fixtures/command_routing.rs` in its existing
  target. Separate owner tests are `application/command_tests.rs`.

No new production dependencies, schema, store, scheduler or generic framework.
The shared turn owner distinguishes a read-only admission proof from an executed
report; probe mode does not allocate persistent per-child DCP nudge state.

### Exact checks and retained logs

All commands below ran serially through
`python3 evidence/T45/run_background_check.py command-routing-<name> <command>`.
Environment: `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`,
`CARGO_NET_OFFLINE=true`, `PYTHONDONTWRITEBYTECODE=1`, and
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Tool timeout 1,800,000 ms; operational subprocess watchdog 1,798 s plus 2 s
kill-after. Product/test caps, budgets and ignores were not changed.

| Name | Command | Exit/result |
|---|---|---|
| red | `cargo test --locked -p oc-adapters --lib command_routing_invalid_metadata -- --test-threads=1` | 101, behavior RED: missing command agent accepted |
| nearest-final | `cargo test --locked -p oc-adapters command_routing -- --test-threads=1` | 0; owner 1, integration 3 |
| fmt-qualified | `cargo fmt --all -- --check` | 0 |
| clippy-qualified | `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| no-run-qualified | `cargo test --workspace --locked --no-run` | 0, separate compile gate |
| workspace-qualified | `cargo test --workspace --locked --no-fail-fast -- --test-threads=1` | 0; **1527 passed / 0 failed / 10 existing ignored**, 42 actual result lines |
| debug-qualified | `cargo build --locked` | 0, normal debug build |
| release-qualified | `cargo build --release --locked` | 0, normal release build; last Cargo invocation |
| debug-help-qualified | `env -i HOME=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 PATH=/usr/bin:/bin target/debug/oc --help` | 0 |
| release-help-qualified | same isolated help command with `target/release/oc` | 0 |
| native-qualified | `python3 evidence/T45/native_command_routing.py target/debug/oc target/release/oc` | 0; 38 actual direct ELF cases after last build |
| python47-qualified | `python3 -m unittest discover -s scripts -p 'test_*.py'` | 0; 47 tests |
| docs-qualified | `python3 scripts/check_docs.py` | 0, read-only |
| progress-qualified | `python3 scripts/progress.py check` | 0, read-only |
| size-qualified | `python3 scripts/code_size.py --base 788069c7772f7592273f456957e78c8517440f7c --changed` | 0, advisory |
| audit-final | read-only gzip receipt/hash/owned-resource audit; exact Python argv is in its receipt | 0; validates 37 completed development/qualification logs, 70,579 gzip bytes / 234,452 raw bytes |

Full lossless logs (including failed trials) are retained read-only outside the
repo at
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t45-background-command-routing-<name>.log.gz`.
Workspace log: 141,671 raw bytes, 36,860 retained gzip bytes, raw SHA256
`f6607676797dede1418d00654cdf710a4e257db4f3f0c4419b3eeeed9babf8e6`.
Native final log: 31,608 raw bytes, 3,081 retained gzip bytes, raw SHA256
`a5382d1d188b7adcfc995c8f8f266470e24427c019b6f02d79b4462fdb03b12d`.
The receipt repeats Cargo result strings as JSON metadata: count only raw result
lines before `RECEIPT`, not twice.

### Normal ELF physical counterfacts

Both are ordinary x86-64 Linux dynamically linked PIE ELF executables:

| Binary | SHA256 before = after the final direct proofs |
|---|---|
| `target/debug/oc` | `36864e4fdbfc5c49fffb619878577759bbf8ab03eba0c6dad6fa2a4093e3eeb7` |
| `target/release/oc` | `dea2c385f6d9021b481258461d60e1e0fd4b09f15562e4894d098b3b1355c40f` |

Final source association for the qualified fmt/clippy/no-run/full workspace,
build and native gates:
`e5e3c8fd19ffb75f5b760b3cf21f9c74feee565e14a72949d1ff04753f908521`
(existing helper's ordered crate-source fingerprint, not a new manifest).
No Cargo was interleaved with final ELF proofs.
Nearest targeted GREEN preceded the final internal boxed-report Clippy fix;
the final full-workspace gate re-executed all four new tests on the final source.

Each binary executed these 19 cases: inferred, explicit true, Markdown legacy
subtask true, false-over-true, alias false, primary inline, captured fallback,
subagent safe recovery, primary safe recovery, changed command generation,
real cancellation, invalid model, invalid variant, unknown profile, disabled
profile, central Deny, Ask without consumer, depth zero, budget one.

- **38 PASS cases**, including **16 refusal cases** with zero physical HTTP,
  accepted input, child jobs, tool intents, dispatch events or selection writes.
- **50 physical HTTP requests = 28 main + 22 title auxiliary**. Durable dispatch
  counts equal actual physical requests. Background command parents issue no
  provider request; inline requests use their selected command profile/model.
- **16 durable child notices**, exactly one per job; idle reopen adds zero
  requests or notices. Safe restart keeps operation/child/turn/delivery IDs,
  acceptance rows and original prompt bytes, with exactly one additional child
  request and one resume claim. Changed command generation makes zero additional
  requests and zero resume claims.
- Barrier-held child requests prove running progress while the command parent
  is completed and its session prefs byte-equivalent. Parent and child display
  Location/generation match. The native command receipt contains one linked
  closed function call/result pair with typed `running` output.
- Ctrl+C cancels an actual active parent and its existing background child,
  cancels the child's real shell leaf and reaps that owned process group.
- Exact raw slash invocation remains in durable user history; expanded text is
  the prompt and wire input. Structured command variant overrides the embedded
  variant, including its unavailable name. Error-prefixed completed fixture prose
  still produces typed completed jobs: text is never parsed for success/routing.
- All **38 exact temporary roots removed**, HTTP handlers and PTY readers joined;
  **50 started PTY native owners** have PID/PGID/startticks and reaped receipts.
  Sixteen refusal CLI owners also exit/reap. Watchdogs exit/reap serially.

Integration additionally covers existing and fresh command acceptance/refusal,
parent-inferred profile routing after an explicit-false inline command, restart
eligibility and preserved parent permission ceilings. Full tests grew by four
meaningful owner/integration tests from baseline 1523 to 1527.

### Failures, review and boundaries

Retained failed trials: `green1` (three compile errors), `green2` (recursive Send
future), `integration1` (test API compile errors), `integration2` (embedded model
variant incorrectly checked before structured override), `elf-dev`/`elf-dev2`
(launch tool state incorrectly completed while job running), `elf-dev3` (fixture
cleanup assumed a list before any HTTP handler), `nearest2` (fresh test queried
an uncreated session instead of Home), `elf-generation-dev` (fixture expected
failed rather than the existing unknown/quarantine state), and `clippy-final`
(large internal enum, fixed with boxed report). All were corrected; no failed
test disabled or budget widened. A same-path multi-section patch overwrote an
earlier edit; source review caught it and one file section fixed it before final
gates. Failed/current logs remain lossless in the approved cache.
The first optional receipt auditor also read its own unfinished gzip log and
failed with exit 1; `audit-final` excludes both auditor logs and verifies completed
receipts, raw hashes, final binary hashes, owned PID/startticks and cleanup paths.

Reviewed tracked diff and all four new Rust files; `git diff --check` passes.
14 changed/new Rust files: 25,871 current physical lines / 1,067,776 UTF-8 bytes.
`application.rs` is 4,952 lines, `runtime/turn.rs` 4,551; neither crosses 5,000.
Natural next application seam remains submit/selection preparation if later
growth requires it. New substantive owner/integration tests remain separate.

Only this routing atomic is qualified. Whole T45, child DCP qualification,
remaining prompt/context/capability scopes, executable/custom shell command
expansion beyond the approved template path, and T44 visual acceptance remain
separate. T44 stays PAUSED; T27 allowance was not replenished or consumed.
No live/paid/ledger/auth/configuration access, root `.opencode` inspection,
planning/status writes, staging, commit or push. Parent independently reviews
and delivers the uncommitted patch. Temporary mutation/Cargo/fixture ownership
is released when the final handoff is returned.
