# T45/R7 frozen atomic — truthful Linux execution context

## Coordinator qualification — 2026-10-05

The parent independently reviewed the runtime collector, shared request assembly,
shell selector and fixture normalization. The reproduced absolute-SHELL/oversized-
PATH defect is repaired without reporting uncertain PATH-dependent fallback or
cloning an oversized environment value. The original RED and historical 1540-test
phase remain below; they are not relabelled as current qualification.

Fresh parent checks exited 0:
- Environment owner: five tests passed.
- Workspace fmt and strict locked all-target Clippy passed.
- Normal locked build restored the retained debug artifact.
- Both normal ELFs passed the synthetic host-context fixture: 40 actual requests
  combined, covering default/custom roots, own-model children, title, reopen and
  same-process source-pinned A→B→A Location changes.
- Python47, documentation, progress structure and diff checks passed.

No Cargo command intervened between the last normal build and direct proofs.
Before/after fingerprints remained:
```text
debug   bc068497ed4cbbb5b262944ea7f380bdfa196a2522043a1be7d1b572bd146f35
release a2cddd726633f1b466ad730feb659ffad47b9225757f5d879ab956445e53b307
```
The complete current source gate is 1541 passed / 0 failed / 10 unchanged opt-in
ignores, recorded in the correction receipts below. Compiled source is baseline
`56533577959621639373812cda7a1badb0118937` plus the reviewed implementation,
not an artifact built from the unchanged clean baseline. No production source
changed during parent verification. Owned native processes and PTY/HTTP workers
joined before exact temporary cleanup; inherited `.opencode/` was untouched.

This closes only the frozen R7 Linux atomic. Structural Git probing is bounded;
cross-architecture/full Git-layout parity and a dedicated normal-ELF compaction
scenario are not claimed. Packs, donor3.2 and remaining T45 contracts stay open.
T44 remains PAUSED; the exhausted T27 allowance was not used or replenished.

## Result
Frozen before RED at source base `56533577959621639373812cda7a1badb0118937`,
branch `agent/oc-rust-port`. H1–H4 PASS for this atomic Linux slice after independent
offline qualification of normal debug/release binaries. This is not whole-T45 or
Goal READY. All changes remain unstaged for parent review/delivery.
Parent-review selector-budget correction also passes fresh final gates: current
workspace **1541/0/10**, five environment-owner tests, and refreshed normal ELF
proofs/fingerprints. Pre-correction 1540/0/10 results are retained as historical.

Outcomes: (H1) bounded native hostname/uname/distro/process architecture,
pointer bits, available parallelism and effective IDs, with unavailable values
unknown; (H2) actual runtime execution root/workdir, native Git metadata probe,
the existing command-shell selector and allowlisted TMPDIR/date; (H3) deterministic
control/delimiter-escaped separate developer layer and shared empty-system base
fallback, including custom and own-model child; (H4) one fresh snapshot per
prepared request, continuation/restart/Location refresh without history accumulation
or weakening captured config/generation/tool ownership.

Change envelope: existing runtime assembler and shell selector; cohesive private
runtime environment implementation/tests; nearest existing runtime tests if needed;
this report and synthetic native fixture. No dependency, crate, store, packs,
donor upgrade, general prompt audit, Go, terminals or visual qualification.
Concrete RED discovery: the existing acceptance path also launches auxiliary title
requests. Include only environment input in the two existing title constructors
and native compaction constructor; their separate task instructions stay owned there.
Concrete full-workspace RED discovery: legacy protocol/resource fixtures assume no
base/environment lane and two-item titles. Extend their existing independently
validated control-lane normalization (`support/context_ids.rs`) and its missing
reader hooks only, preserving exact conversational comparisons and all thresholds.
Dedicated native receipts keep the unmodified request. Equal-active-context
fixtures normalize request-local workspace paths, not archive-dependent payloads.
Final review corrected the bounded Git ancestor walk: an exhausted limit or metadata
access error is unknown/omitted, not a fabricated negative; its regression lives in
the existing host-collection owner test. Qualification was repeated after this fix.

| Frozen outcome | Observed result |
| --- | --- |
| H1 native bounded facts | PASS: hostname, kernel OS/release, machine architecture, distro, ELF-derived process architecture/pointer bits, available execution parallelism, effective UID/GID agree with independent Linux observations. Native processes are non-root and their safe `/proc/<pid>/status` shows zero effective capabilities. Collector uses existing libc/standard library, no subprocess or new dependency. |
| H2 truthful tool environment | PASS: actual execution/workspace roots; synthetic Git A versus non-Git B; selected `/usr/bin/bash` despite unusable fish `$SHELL`; approved inherited TMPDIR and UTC date. Actual admitted shell output independently confirms `$0`, `$TMPDIR`, and cwd. Permanent owner comparison also preserves selected absolute `/bin/bash` despite oversized unused PATH, while PATH-dependent uncertainty is omitted. |
| H3 shared assembly | PASS: distinct escaped developer metadata survives custom systems; shared compiled base appears for empty root/child systems only. Own-model default/custom children receive their own task/workspace without copied parent transcript. Literal os-release substitutions are data and never executed. |
| H4 request refresh/pinning | PASS: exactly one metadata layer on each captured main/title request; continuation, custom restart, default reopen, and same-process live A→B→A refresh. Pending move follow-ups and shell effects retain source root/AGENTS/system; later turns adopt target generation. No metadata messages persist in raw SQLite history. |

Exact changed paths (19; inherited untracked `.opencode/` is not part of the slice):

```text
crates/oc-adapters/src/application.rs
crates/oc-adapters/src/runtime.rs
crates/oc-adapters/src/runtime/turn.rs
crates/oc-adapters/src/runtime/environment.rs
crates/oc-adapters/src/runtime/environment/tests.rs
crates/oc-adapters/src/runtime_compaction.rs
crates/oc-adapters/src/shell.rs
crates/oc-adapters/tests/context_bounds.rs
crates/oc/tests/configured_workspace.rs
crates/oc/tests/dcp_runtime.rs
crates/oc/tests/golden_binary.rs
crates/oc/tests/mcp_application.rs
crates/oc/tests/pty_t42.rs
crates/oc/tests/recovery_v02.rs
crates/oc/tests/support/context_ids.rs
crates/oc/tests/support/title.rs
docs/CODE_MAP.md
evidence/T45/host-context.md
evidence/T45/native_host_context.py
```

## Checks
### Parent-review correction — frozen before RED

Returned sole ownership for one introduced selector-budget counterexample: a
valid absolute `SHELL=/bin/bash` remains the actual selector result even when PATH
exceeds `ARG_BYTES_CAP`, but the initial bounded map omitted shell metadata. Add one
permanent owner risk test comparing actual `command_argv` to collection; also check
PATH-dependent uncertainty and ignored ambient keys. Keep bounded cloning, the
shared selector, safe unknown omission and no subprocess. Narrow correction paths:
`runtime/environment.rs`, its existing `tests.rs`, and this report only. Fresh full
gates and normal ELF fingerprints were pending at this freeze; the 1540/0/10 and reviewed fingerprints
below are historical pre-correction results, not current qualification.

### Current correction results

RED reproduced the exact introduced omission: actual `command_argv` returned
`/bin/bash`, while collected `shellExecutable` was absent. The minimal production
change excludes oversized PATH from the bounded selector map, then accepts a
result only if PATH was bounded or the **same shared selector actually selected
the unchanged absolute SHELL**. It neither clones oversized PATH nor reports a
substituted-default-PATH fallback. Oversized SHELL remains safely unknown. One
new permanent risk test also checks ignored fish with oversized PATH is omitted,
and an oversized synthetic HOME does not affect the relevant selector budget.
No shell subprocess is involved in these owner comparisons.

Only three existing paths changed during returned ownership:
`crates/oc-adapters/src/runtime/environment.rs` (+10 lines),
`crates/oc-adapters/src/runtime/environment/tests.rs` (+38 lines, one risk test),
and this report. The atomic's total 19-path inventory above is unchanged.

Current full commands, exits and receipts (absolute receipt root below):

| Command | Exit / result | Current receipt filename |
| --- | --- | --- |
| `cargo test -p oc-adapters --lib --locked prm01_shell_metadata_matches_selector_with_oversized_unused_path` (RED) | 101; 0 passed / 1 failed / 0 ignored, expected missing metadata | `t45-host-correction-red.log.gz` |
| Same targeted command (GREEN) | 0; 1/0/0 | `t45-host-correction-green.log.gz` |
| `cargo test -p oc-adapters --lib --locked runtime::environment` | 0; **5/0/0** | `t45-host-correction-owner.log.gz` |
| `cargo fmt --all -- --check` (initial correction check) | 1; new test assertion wrapping; fixed with apply_patch | `t45-host-correction-fmt.log.gz` |
| `cargo fmt --all -- --check` (final) | 0 | `t45-host-correction-fmt2.log.gz` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `t45-host-correction-clippy.log.gz` |
| `cargo test --workspace --locked --no-run` | 0; separate precompilation | `t45-host-correction-no-run.log.gz` |
| `cargo test --workspace --locked --no-fail-fast` | 0; **1541 passed / 0 failed / 10 existing ignores**, 1067.19s; one added risk test above prior 1540 | `t45-host-correction-workspace.log.gz` |
| `python3 -m unittest discover -s scripts -p 'test_*.py'`; `python3 scripts/check_docs.py`; `python3 scripts/progress.py check`; `python3 scripts/code_size.py --base 56533577959621639373812cda7a1badb0118937 --changed`; `git diff --check` | wrapper 0, all five nested exits 0; **47 Python tests** | `t45-host-correction-checks.log.gz` |
| `cargo build --locked` | 0; normal debug, 4.80s | `t45-host-correction-debug.log.gz` |
| `cargo build --release --locked` | 0; separate normal release, 148.48s | `t45-host-correction-release.log.gz` |
| `target/debug/oc --help`; `target/release/oc --help`; `python3 evidence/T45/native_host_context.py --binary target/debug/oc`; same fixture with `target/release/oc` | wrapper 0; nested **[0,0,0,0]**, synthetic HOME/XDG; 9.47s | `t45-host-correction-elf.log.gz` |
| Sequential docs/progress/changed-size/diff, workspace aggregate, hash recheck, owned PID/TempDir cleanup audit | 0, all nested exits 0 | `t45-host-correction-handoff.log.gz` |

All 13 current logs are at
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`
with the exact filenames above. Each invocation uses the existing bounded atomic
GZ runner; wrappers repeat the losslessly recorded argv, with full expanded command
and nested exit receipts. Current 13 receipts total **83131 compressed bytes**;
combined with historical 49: **62 complete receipts / 390466 compressed bytes**,
below 1MiB. Both correction failures remain lossless. The earlier zero-byte partial
and historical logging limitation remain explicitly retained below.

Current ELF proof after the LAST normal debug/release builds, no later Cargo:

```text
source: 56533577959621639373812cda7a1badb0118937 + DIRTY 19-path atomic above
target/debug/oc   SHA256 bc068497ed4cbbb5b262944ea7f380bdfa196a2522043a1be7d1b572bd146f35
target/release/oc SHA256 a2cddd726633f1b466ad730feb659ffad47b9225757f5d879ab956445e53b307
ELF magic 7f454c46; class 2 (64 bit); machine 62 (x86_64), both binaries
before -> help/native proof -> after -> handoff rehash: identical
help debug/release and native debug/release exits: [0, 0, 0, 0]
```

This is BASE+DIRTY association, not full-source parity certification. The unchanged
native fixture again captures 13 headless plus 7 live placement requests per ELF:
six root/default/custom/own-model-child/restart/reopen scenarios and one live
same-process A→B→A scenario, 14 scenario executions / 40 requests across both ELFs.
Independent facts and exact layers/AGENTS/base/pending-source pinning are asserted.
All 10 owned native PIDs are absent at handoff; eight headless PGIDs exited 0 and
were reaped, both PTY owners joined HTTP/PTY/stdio and removed only their exact
TempDirs. The runner has exited; no owned Cargo/native/fixture processes remain.
Changed-size advisory: 16 Rust files / 28056 lines / 1123647 UTF-8 bytes, all below
5000 lines; environment owner 295 lines / tests164. Ownership is released to parent.

### Historical pre-correction qualification

Required nearest RED→GREEN, owner fixtures and normal debug/release ELF fake-wire
root default/custom, own-model child, continuation, restart and workspace switch.
Facts independently checked with Python OS APIs, safe process /proc status and
synthetic Git/config/paths, never product collector expectations.
Required fmt, strict workspace Clippy, separate no-run/full workspace tests,
separate normal debug/release builds/help, Python47, docs/progress/diff and advisory
changed-file code size. After final builds hash both ELFs, run native proof with
no intervening Cargo, rehash. Lossless atomic GZ receipts in approved cache,
combined new receipts ≤1MiB; command runner deadline1798s, tool1800000ms;
Cargo jobs3, tests1, offline, approved disk TMPDIR.

Historical final checks, all exit 0 (superseded by current correction gates above):

| Command | Result | Receipt suffix |
| --- | --- | --- |
| `cargo test -p oc-adapters --lib --locked runtime::environment` | 4 passed, 0 failed/ignored | `owner-reviewed` |
| `cargo fmt --all -- --check` | PASS | `fmt-reviewed` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS | `clippy-reviewed` |
| `cargo test --workspace --locked --no-run` | separate compilation PASS | `no-run-reviewed` |
| `cargo test --workspace --locked --no-fail-fast` | **1540 passed, 0 failed, 10 existing ignores**, 1071.50s; baseline 1536 plus four owner tests | `workspace-reviewed` |
| `cargo build --locked` | normal debug PASS | `debug-reviewed` |
| `cargo build --release --locked` | normal release PASS | `release-reviewed` |
| `target/debug/oc --help`; `target/release/oc --help` | synthetic HOME/XDG; both 0 | `elf-reviewed` |
| `python3 evidence/T45/native_host_context.py --binary target/debug/oc`; same with `target/release/oc` | both 0; six headless scenarios + live placement scenario per ELF | `elf-reviewed` |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | **47 passed**, including progress/docs/code-size/bounded-live suites | `checks-qualified` |
| `python3 scripts/check_docs.py`; `python3 scripts/progress.py check`; `git diff --check` | PASS sequentially; structure checks only | `checks-qualified`, final `handoff` |
| `python3 scripts/code_size.py --base 56533577959621639373812cda7a1badb0118937 --changed` | advisory: 16 Rust files, all below 5000 lines; environment owner 285/test126 | final `handoff` |

Receipt root: `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Each suffix above resolves to `t45-host-<suffix>.log.gz`; full argv and exit are in
each lossless receipt. The external cache runner `t45-host-run.py` pins jobs3,
tests1, offline and approved TMPDIR; its 1798s operational timeout does not alter
product/test deadlines. It creates one atomic GZ per invocation and refuses overwrite.
Historical 49 complete receipts total 307335 compressed bytes, below 310KiB and within the
1MiB envelope. The separate zero-byte interrupted partial is retained below.

Historical pre-correction ELF proof after that phase's last normal builds:

```text
source: 56533577959621639373812cda7a1badb0118937 + DIRTY paths above
target/debug/oc   SHA256 e45856ee3d47f5f50a444579e1996f7005b319ec805b3483f1cb7fdb0f62e61b
target/release/oc SHA256 cee961265ed37837413681306c1c291f360724eb80104f87beccc20f0f30bc1a
ELF magic 7f454c46; class 2 (64 bit); machine 62 (x86_64), both binaries
hash-before -> help/native proofs -> hash-after: identical; no intervening Cargo
help debug/release and native debug/release exits: [0, 0, 0, 0]
```

This is honest BASE+DIRTY build association, not full-source parity certification.
The receipt records actual Git HEAD/status at proof time. Each ELF captures 13
headless requests (including two title requests and two requests per own-model
child) plus seven exact live placement requests. Four headless native PGIDs per
ELF exit 0 and are reaped. The shared PTY owner confirms same PID across A→B→A,
joins PTY/HTTP/stdio before exact TempDir removal; shell watchdog/effect cleanup
remains owned by the existing runtime. No owned Cargo/native/fixture processes remain.

Historical experiment index (same receipt naming; commands abbreviated only here,
full argv in receipts):

| Receipt suffix(es) | Command / exit / counts |
| --- | --- |
| `red-native`, `red-native2` | native debug: 1, 1; baseline has missing metadata. First HTTP500 assertion caused retry and bounded fixture timeout; corrected peer records assertion with terminal HTTP200, then explicit missing/duplicate-layer RED. Baseline debug SHA256 `073a84304c3d2ef86127da6c275264d47599ccc06286a7217447c0833ac64558`. |
| `format`, `format2`, `format3`, `format-final` | `cargo fmt --all`: 0 each |
| `owner`, `owner2`, `nearest` | adapter lib filters `prm01_host`, `runtime::environment`, `prm01_`: 0; 2/4/12 passed respectively |
| `debug-dev`, `debug-dev2` | normal debug build: 0 each |
| `native-dev`, `native-dev2` | native debug: 0 each; intermediate smaller proofs, not final source association |
| `clippy`, `clippy2` | strict Clippy: 101 missing getegid SAFETY comment, then 0 |
| `fmt-check`, `fmt-final` | fmt check: 0 each |
| `no-run`, `no-run-final` | workspace no-run: 0 each |
| `workspace2` | workspace: runner timeout1798, exit -9; completed output counts 780 passed/121 failed/10 ignored, incomplete suite. Old title/input-shape assumptions caused cascading fake-peer failures/timeouts; not evidence of a runtime hang. |
| `nearest-binary` | `cargo test -p oc --locked --test application --test configured_workspace --test durability --test dcp_runtime --test responses --test golden_binary --no-fail-fast`: 101; 27 passed/1 failed. Golden fake reader also handles MCP without `input`; added guarded normalization. |
| `nearest-golden`, `nearest-golden2` | golden target: 101 E0282 missing Value annotation after guard, then 0/1 passed |
| `nearest-context` | adapter context_bounds target: 0/3 passed |
| `clippy-final`, `clippy-final2` | strict Clippy: 101 duplicate shared module in title helper, then 0 with one crate-root owner |
| `workspace3` | full workspace: 101; 1539 passed/1 failed/10 ignored. Bare-rename local input index still expected two-item title; validated normalized clone fixes it. |
| `nearest-title` | bare rename targeted PTY test: 0/1 passed |
| `python47`, `progress`, `docs` | Python47 0/47 passed; progress check 0; docs check 1 because my parallel read-only checks contend on existing progress writer lock. Repeated sequentially PASS. |
| `fmt-qualified`, `fmt-qualified2` | fmt check: 1 import order; formatting applied, then 0 |
| `clippy-qualified`, `no-run-qualified` | strict Clippy/workspace compilation: 0 each |
| `workspace-qualified` | full workspace before final Git-bound fix: 0; 1540/0/10 |
| `checks-qualified` | Python47/docs/progress/changed-code-size/diff wrapper: 0, nested exits all 0 |
| `debug-qualified`, `release-qualified`, `elf-qualified` | intermediate normal builds/proofs: 0 each; superseded fingerprints, retained losslessly |
| `owner-reviewed`, `fmt-reviewed`, `clippy-reviewed`, `no-run-reviewed`, `workspace-reviewed`, `debug-reviewed`, `release-reviewed`, `elf-reviewed` | final post-review validation as above, all 0 |
| `handoff` | final sequential docs/progress/code-size/diff + receipt/count/hash/owned-process audit |

Retained logging gap: the first buffered workspace logger was interrupted at tool
timeout1800000 and left `t45-host-workspace.log.gz.partial` (zero bytes, not a
recoverable log). Its output cannot be certified lossless; this failure is not
hidden or used as PASS. The streaming atomic logger then preserved every complete
receipt listed above, including failures. One earlier misspelled runner invocation
referenced nonexistent `t45-host-no-run`, exited 2 before starting Cargo and produced
no log/side effect. No failed tests were disabled and no cap/deadline/assert threshold
or ignore count was raised. Only the exact-owned fixtures were removed after joins.

## Risks
Host context is metadata, not filesystem/tool permission or sandbox authority.
Optional failed collection must not prevent startup. Date is UTC. Git metadata
probing must not execute Git or expose remotes/config. T45 remains open, T44
PAUSED; T27's exhausted ledger is untouched. Inherited root `.opencode` is excluded
from all manual inspection/mutation. Fixtures have synthetic HOME/XDG only.
Qualification is Linux x86_64 on this host, not cross-architecture or donor prompt
parity. Git is a bounded native structural worktree/submodule/linked-worktree probe;
unreadable/truncated optional metadata is omitted. Bare-repository Locations and
nonstandard repository layouts are not qualified. Dates are UTC, not a timezone
policy. Native compact/title consumers use the same metadata method; title wire is
directly captured, while compaction retains existing workspace/PRM01 tests rather
than a separate normal-ELF R7 compaction scenario. No inventory of environment,
real HOME/config, remotes, auth or paid/live API was read. No new crate/dependency,
store, framework or manually reviewed source manifest was introduced.

## Next
Parent independently reviews the exact unstaged paths and receipts, then decides
delivery. Mutation/Cargo/offline-fixture ownership is released after the final
handoff check. No task/spec/GOAL/progress updates, staging, commits or pushes by
this owner; whole T45 remains active, T44 PAUSED, exhausted T27 ledger unchanged.
