# T45/R9/DCP12 — frozen percentage-default atomic

## Coordinator qualification — 2026-10-04

Independently reviewed the shared percentage/budget owner, captured runtime
preparation, typed snapshot and panel consumers, both new test packs, recovered
full-gate receipts and bounded native fixture. Recovery made no extra production
changes. The scout's apparent `Option<Box>` compilation finding was a display
truncation: the actual field is `Option<Box<DcpReminderFacts>>`, and independent
compilation and strict Clippy passed. No workaround or validation relaxation was
introduced for that finding.

Fresh independent commands exited 0: adapter `dcp12_` tests **3 PASS**, native-crate
`dcp12_` owner test **1 PASS**, workspace fmt, strict locked all-target Clippy,
and normal locked build. After that last build, both normal ELFs passed all eight
native default/override/fallback/restart/panel scenarios, **16 cases total**.
Each ELF made **28 physical POSTs: 21 main, 7 title, 0 hidden summary**; invalid
effective thresholds made zero POSTs, turns or effects. Owned groups and HTTP
workers joined before fixture cleanup.

Normal fingerprints were identical before and after the independent native runs:

```text
debug   166e4f774dc3d9e99b5057aac2489b353fcd5d1304276ffdb820d6e3be8a2864
release 202e790f46a8184b9df6b6002d9318c5343bf9a3f2c4bde3f0158e9e76c294e1
```

Python **47 PASS**, docs, progress and diff checks also exited 0. The recovered
full workspace receipt remains **1471 passed / 0 failed / 10 unchanged opt-in
ignored**, with unchanged production source after that gate. Source association
is the recorded base plus reviewed implementation, not a clean-base binary claim.
This closes only percentage defaults and effective runtime/panel agreement;
controls/manual/child, donor3.2, task packs and whole T45 remain open. T44 remains
PAUSED; no live request or exhausted-ledger replenishment occurred.

## Result

Percentage-default atomic PASS; resumed and closed 2026-10-04. Obligations were
FROZEN before RED on 2026-10-03. Execution base/source association:
`dc4bc3a7ae62eee0330bc0fc8e8e4eacaa05bb85` plus the reviewed DIRTY implementation
listed below, including both new Rust owner test packs. No implementation source
changed during recovery. The completed final workspace/Clippy logs were recovered,
not blindly repeated. Current locked normal builds reused that same source and
produced the same retained debug/release ELF hashes.

Contract: `docs/DCP.md:294–365` and T45 spec R9:101–125. Native policy diverges
from retained DCP3.1.15 source `11f6517780a502512a3467645074be447cb0369e`;
AGPL provenance/license and exact alias admission remain pinned. This closes only
the percentage-default atomic, not all R9/DCP12/T45 or T44/VIS38.

## Checks

| Frozen obligation | Outcome | Evidence |
| --- | --- | --- |
| No-file defaults40%/55%/false, retained5/15/soft | PASS, initial RED preserved | `DcpConfig::default`, empty loader fixture, real no-file application query, normal native requests/panel |
| Integer/percent/partial/exact provider/model overrides and precision validation | PASS | three `dcp12` owner unit scenarios, real layered loader, native numeric/partial/nested-model cases |
| Shared canonical key/positive context/fallback warnings; floor>=1, effective min<=max before effects | PASS | shared `reminder_facts`, budget fixtures, typed application/panel, native zero-POST invalid case |
| Min includes summaries; false max total, true max active summaries only | PASS | owner boundary/buffer fixture plus existing runtime active-summary wire regression |
| Atmin eligible, atmax no max escalation, above max strong when due; iteration/reset only success | PASS | owner exact boundaries/cadence/cooldown/iteration15; runtime restart/success and no-gain regressions; success-only reset branch reviewed |
| Threshold reminder separate from model admission; no hidden summarizer | PASS | current debug/release fake-provider request identities, actual request and dispatch-lane counters |
| Known/missing/zero/partial metadata, precedence/restart/safe Location generation | PASS | real owner query/restart/A→B→A generations and source precedence; actual normal binary known/fallback/partial requests and reopened current panel |
| Disabled/manual/Deny/captured generation/protections/RAW-HOT/noReplay preserved | PASS regression | 15 nearest runtime context tests and final full workspace; existing actual-binary DCP/manual/resource suites included |
| Full fmt/locked strict Clippy/ws tests/debug+release/help/Python47/docs/progress/diff/advisory | PASS | serial commands and lossless logs below |

### Source owners and assertions

- `crates/oc-adapters/src/dcp_auto.rs`: native percentage defaults in basis points,
  `summary_buffer=false`; one `reminder_facts(provider, selection, budget)` owner.
  Canonical key is provider plus complete selected model ID, including nested IDs.
  Existing percentage flooring uses u128 and a minimum of one. Effective invalid
  min>max/zero budget is rejected before effects. Integer loaders clear percentages.
- `crates/oc-adapters/src/runtime/turn.rs`: initial and follow-up preparations use
  that owner with the captured selection and existing model budget. Request
  identities retain separate context/input/output capacity and reminder thresholds.
  Reminder cadence, accounting, disabled/manual/Deny gates and successful-compress
  reset implementation are preserved. The success-only reset excludes no-gain/error.
- `crates/oc-adapters/src/application.rs`: bounded active DCP query resolves the
  selected model/variant and configured native fallback through the same model
  budget owner. It no longer uses raw IDs, context=0, or DCP max as capacity.
- `crates/oc-core/src/queries.rs`: typed `DcpReminderFacts` carries exact key,
  context, fallback provenance/warning, effective min/max and buffer mode. Its boxed
  optional snapshot field avoids ballooning by-value worker/event stack frames.
- `crates/oc-tui/src/dcp_panel.rs`, `crates/oc-tui/src/views.rs`: existing panel shows
  reminders separately from model context/fallback and warning. No second policy
  calculator. Context estimates retain the existing declared estimate method.
- `crates/oc-adapters/src/dcp_auto/defaults_tests.rs`: three new owner scenarios
  cover omitted/default equality, partial integer override, floor/precision/exact
  provider precedence, tiny context, missing/zero/context-only/output-only metadata,
  invalid effective combinations and exact below/at/above boundary accounting.
  Total-active tests use 39,999/40,000/55,000/55,001 with 20,000 active summary tokens;
  true mode subtracts those summaries only for upper reminders. Frequency5 cooldown,
  successful reset and iteration15 escalation are asserted.
- `crates/oc/tests/dcp_runtime/defaults.rs`, registered in
  `crates/oc/tests/dcp_runtime.rs`: one real owner scenario uses no DCP file, checks
  exact 40,001/55,001 at context100,003, panel rows and restart; layered inline →
  standalone global → project → admitted fixture `.opencode` overrides resolve to
  exact model25%/65%/true. Safe A→B→A Location changes increment generation and
  restore each source's facts. Missing/zero/partial metadata uses positive fallback
  and warnings. Owner queries/restarts/Location switches cause zero provider POSTs.
  This synthetic `.opencode` is inside its owned TempDir, not the inherited worktree
  `.opencode`.
- `crates/oc-adapters/tests/runtime/context.rs`: four existing explicit numeric
  profiles clear inherited percentages. Expectations/caps/deadlines are preserved.
- `docs/CONFIG.md`: implemented defaults, optional files, numeric/partial overrides,
  explicit older sample profile and typed capacity/reminder distinction documented.

### Commands, exits and lossless logs

Every command below was run through
`python3 evidence/T45/check_dcp_defaults.py LABEL COMMAND...`. The helper reuses the
existing serial lossless-gzip owner. Settings: `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`, `CARGO_NET_OFFLINE=true`, `PYTHONDONTWRITEBYTECODE=1`,
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Outer watchdog1798s/tool1800000ms is operational, not a product turn limit.
All paths in this table are under `evidence/T45/dcp-defaults-logs/`.

| Command | Exit / counts | Full log |
| --- | --- | --- |
| `cargo test --locked -p oc-adapters --lib dcp_auto -- --nocapture` | 0; 7 passed | `targeted-defaults.log.gz` |
| `cargo test --locked -p oc --test dcp_runtime dcp12 -- --nocapture` | 0; 1 passed | `owner-panel-2.log.gz` |
| `cargo test --locked -p oc-adapters --test runtime context:: -- --nocapture` | 0; 15 passed | `runtime-context-2.log.gz` |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0; 24.982s | `clippy-final.log.gz` |
| `cargo test --locked --workspace --no-run` | 0; 75.949s | `test-precompile-final.log.gz` |
| `cargo test --locked --workspace --no-fail-fast` | 0; **1471 passed / 0 failed / 10 ignored**, 42 result groups; 996.694s | `workspace-tests-final.log.gz` |
| `cargo fmt --all -- --check` | 0; current source, 2.953s | `fmt-current.log.gz` |
| `cargo build --locked` | 0; initial13.954s/current0.819s | `debug-build.log.gz`, `debug-build-current.log.gz` |
| `cargo build --locked --release` | 0; initial152.885s/current0.521s | `release-build.log.gz`, `release-build-current.log.gz` |
| `target/debug/oc --help` | 0 | `debug-help.log.gz` |
| `target/release/oc --help` | 0 | `release-help.log.gz` |
| `python3 evidence/T45/native_dcp_defaults.py target/debug/oc` | 0; 8 cases, 12.017s | `native-debug-current.log.gz` |
| `python3 evidence/T45/native_dcp_defaults.py target/release/oc` | 0; 8 cases, 7.193s | `native-release-current.log.gz` |
| `sha256sum target/debug/oc target/release/oc` | 0; hashes below | `elfs-before.log.gz`, `elfs-after.log.gz`, `elfs-current-after.log.gz`, `elfs-final.log.gz` |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 0; **47 passed**, 11.626s | `python47.log.gz` |
| `python3 scripts/check_docs.py` | 0 | `docs.log.gz` |
| `python3 scripts/progress.py check` | 0; structure only | `progress.log.gz` |
| `git diff --check` | 0 | `diff-final.log.gz` |
| `python3 scripts/code_size.py --base dc4bc3a7ae62eee0330bc0fc8e8e4eacaa05bb85 --changed` | 0; 10 Rust files, 16,230 physical lines, no >5000 warning | `size.log.gz` |

Workspace ignores are the unchanged opt-in live/real-server/catalog probes;
no new ignore or assertion/deadline/resource weakening. Final workspace covers
DCP05/07/10/11 regressions, actual-binary DCP/restart/manual tests, RAW/HOT/no-gain,
model-admission/fallback, protection/tool graph and VIS38 equal-active archive bounds.
The fallback model fixtures reuse the existing AUD41 model-budget owner/semantics;
discovery metadata remains unknown rather than fabricated as fallback capacity.

### Normal native receipts and retained ELFs

The eight current cases for **each** normal binary are no-file known, no-file missing,
no-file zero, context-only warning, explicit numeric/buffer=true, partial numeric,
exact provider/nested model and invalid effective min>max. Successful cases perform
prime → continue → restart → continue, compare exact saved owner request facts with
current/reopened panel and verify cadence wire `[0,1,0]`.

- Known context40,000: default reminders16,000/22,000, input36,928; context-only
  metadata gives input34,880 and output warning, retaining context40,000.
- Missing/zero context: native context32,768, reminders13,107/18,022,
  input27,648, visible fallback provenance and existing budget warning.
- Explicit100/50,000/true is advisory; partial100/default55% is required above max.
  Exact `fixture/nested/m`25%/60% gives10,000/24,000 and a required reminder.
- Each successful case: physical4 POSTs = main3 + title1. SQLite dispatched lanes
  exactly `main:3,title:1`; zero tool operations/compression blocks. The existing
  title lane is accounted explicitly, not misrepresented as a hidden summarizer.
- Invalid effective thresholds: zero physical POSTs, zero turns, zero tool effects,
  real painted `invalid_config` before Submit/admission effects.
- Totals per binary: **8 cases; 28 physical / 21 main / 7 title / 0 hidden-summary
  POSTs; 0 compress operations**. Across the two current runs:16 cases,56/42/14/0.
  Earlier passing debug/release receipts are also preserved; these totals describe
  the current proof pair, not a claim that failed development runs made no requests.

After the LAST normal locked debug/release build, both native proofs ran against the
actual retained `target/{debug,release}/oc`; no intervening Cargo command. Per-run
before/after hashes and final post-proof hashes all match the recovered initial pair:

```text
debug   166e4f774dc3d9e99b5057aac2489b353fcd5d1304276ffdb820d6e3be8a2864
release 202e790f46a8184b9df6b6002d9318c5343bf9a3f2c4bde3f0158e9e76c294e1
```

### Failed attempts and diagnosis (preserved, not external blockers)

- `red-defaults.log.gz`, exit101: intended initial RED, two failures against old
  summaryBuffer=true/integer thresholds. Actual omitted-default implementation fixes
  those assertions; no sample-only workaround.
- `owner-panel.log.gz`, exit101: fixture referenced nonexistent catalog `.generation`;
  corrected to existing `.chrome.selection_generation`, then owner-panel-2 passed.
- `runtime-context.log.gz`, exit101: three older numeric Rust fixtures inherited new
  default percentage fields. Explicit numeric profiles now clear percent fields,
  matching loader semantics; runtime-context-2 passed15 without weaker assertions.
- `fmt-red.log.gz`, exit1: normal formatting drift, fixed; fmt/current PASS.
- `workspace-tests.log.gz`, exit101: binary VIS11 lifecycle test stack overflow;
  `stack-isolate.log.gz`, exit101 reproduced it. Newly enlarged by-value snapshot
  inflated event/async frames. Boxing only new reminder facts corrected layout;
  `stack-boxed.log.gz` passed1, strict final Clippy and final whole workspace passed.
  No increased test stack or ignored test. All implementation mtimes precede final
  Clippy/workspace and normal builds; warm current builds confirm current association.
- `native-debug.log.gz`, exit1: fixture expected a string model identity instead of
  existing typed `{provider,id,variant}`. Corrected to assert provider and ID.
- `native-debug-2.log.gz`, exit1 and `native-debug-4.log.gz`, exit1: fixture expected
  zero auxiliary POSTs; actual SQLite lane counters proved one existing title request.
  Now asserts exact physical/main/title counts and no hidden summary request.
- `native-debug-3.log.gz`, exit1: DB settlement did not acknowledge PTY panel-close
  input. Added existing-screen close acknowledgement before continuation; no timeout
  increase or product change.
- `native-debug-5.log.gz`, exit1: compared background/frame-border prefixes beside
  modal rows across restart. Extracts exact reminder/model/budget substrings while
  retaining all field/value assertions; typed exact owner values remain checked.
- `native-debug-6.log.gz`, exit1: invalid fixture expected a failed persisted turn,
  but current initialization correctly rejects before any turn exists.
- `native-debug-final.log.gz`, exit1 and `native-invalid.log.gz`, exit1: expected
  private error detail `minContextLimit`, while actual safe painted category is
  `submit: runtime native initialize: invalid_config`. Now checks that category and
  stronger zero-POST/zero-turn/zero-tool assertions. `native-debug-pass.log.gz` and
  `native-release-pass.log.gz` first passed8 each; both current proof runs passed8.

All 30 interrupted-run gzip logs, including failures, are retained losslessly.
New closeout logs also retain full command/output/exit trailers; none are fabricated
summaries, truncated captures or replacements for a missing gate.

## Risks

UI estimates and displayed K-label rounding remain distinct from exact saved owner
facts and full request wire cost; this is intentional. Native root request coverage
and real owner Location/source coverage are separate layers, not full child or VIS38
visual qualification. The typed query reuses bounded current projection, never a new
archive-loading store. The largest changed Rust file is application.rs4615 lines;
new substantial tests are in separate owner packs, with no new crate/dependency.

Evidence corpus for this atomic stays below1MiB total, including scripts/report and
lossless gzip logs. The recovered30 logs were142,491 compressed bytes. Recovery's
45-log closeout inventory was149,146 compressed bytes with45 complete exit trailers;
scripts/report/logs together175,328 bytes before this final factual inventory update.
Largest uncompressed log135,881 bytes, well below16MiB. Any subsequent final check
logs remain additional lossless records, included in the final directory totals.

Native cleanup receipts assert owned TempDir removal after native process wait,
PTY reader join, HTTP shutdown/server-close (joining non-daemon request workers)
and server-thread join. All55 recorded native cleanup receipts in that45-log inventory
were checked: every exact named TempDir was absent. Current proof commands created
no residual TMP paths.
No shell/child process was launched by these eight native cases; no compression/tool
effects. Recovery found no Cargo/rustc/native-default worker active. Existing unrelated
TUI processes and historical TMP fixtures were not stopped or removed. No broad prune.

No live/paid/API credentials/user HOME/env/config/runner-auth operations; T27 exhausted
24G/15C/1MCP is untouched. Inherited worktree `.opencode` remains untouched. Source-owner
concurrent docs and GOAL/spec/planning/acceptance/progress/statuses are preserved;
no staging/commit/push. T44 remains PAUSED; T45 remains active.

## Next

Percentage-default atomic is complete, with mutation/Cargo/offline-fixture ownership
RELEASED to parent after this report/checks. Compression switch/manual/commands/debug,
allowSubAgents/full child qualification, selected task packs and donor3.2 upgrade
remain NEXT separate atomics. This report cannot claim all DCP12/R9/T45 or VIS38
done. No GOAL/spec/planning/acceptance/progress edits, staging/commits/push.
