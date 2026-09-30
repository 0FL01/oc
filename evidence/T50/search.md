# T50 R3 / TOOL14 — frozen search fixture and qualification

## Coordinator review and delivery qualification

The coordinator independently reviewed the search owner, canonical schema and
argument admission, descriptor-relative reopening, own-data and explicit read
Deny guards, dependency pins, cancellation dispatch, native fixture and tests.
The review reproduced the active-scan cancellation gap; the correction and its
RED/GREEN proof below are part of the current source, not a waived risk.

Current full-workspace log `t50-search-cancel-workspace-final.log` contains 42
successful summaries: **1,355 passed, zero failed, ten unchanged opt-in ignored**.
Independent coordinator commands subsequently passed: `cargo test -p oc-adapters
--lib --locked files::` (20, including the actual traversal barrier), workspace
fmt, strict locked all-target Clippy, normal locked debug build, 47 Python tests,
docs/progress checks and diff check. After that build, direct `native_search.py`
checks passed all 14 cases on each normal debug/release ELF, with the exact
hashes in the current artifact table unchanged before and after both runs.
No Cargo build intervened between those direct native checks.

The declared donor differences remain explicit: admitted Location only, no-follow,
own-data exclusion, bounded resources and deterministic pagination; ambient Git
ignore configuration is not silently loaded. Active traversal cancellation is
proved by the same Runtime path in cfg(test) ELFs, not falsely attributed to the
normal CLI's held-provider pre-admission cancellation case. New dependency
licenses and registry/checksum provenance remain in the table below and Cargo.lock.
No live campaign or user configuration was read or changed during this slice.
T50 R4–R8, full inventory, T27's blocked post-compression live process and T44's
owner pause are unaffected; this is not a full-goal readiness claim.

Base: `9cbb5f2080e71b580682af142b27ade9154198f9`; donor:
`2670273ff17da96f85c5826ced57aa1b368754fa` (local HEAD verified).
This slice covers R3 and the search portion of R1 only. R4–R8 remain pending.

## Frozen obligations (before RED)

| Obligation | Source-derived fixture / observable result |
| --- | --- |
| Regex default, literal opt-in, caseSensitive default true | Anchors, alternation, repetition, character classes, Unicode case folds; escaped metacharacters versus literal text; exact path/line/text in native continuation |
| Vetted syntax and validation | Rust regex engine used by default ripgrep; malformed expression and unsupported look-around/backreference produce redacted typed errors before scan; no PCRE claim |
| grep path and include | File versus directory scope, nested paths, `*.rs` and `*.{rs,txt}` include; returned paths remain Location-relative; positive include overrides ignore rules |
| glob path/hidden/pattern | Scoped directory, slash-aware glob/brace/class matching; default hidden false even with positive pattern; hidden true includes dot entries; `.git` excluded in either mode |
| Ignore semantics | Root/nested `.gitignore` (only with repository), `.ignore`, `.rgignore`, negation and precedence; grep always hidden; glob positive pattern overrides ignores, final dot/.git exclusions win |
| Ordering/pagination/budgets | Lexicographic path then line ordering; one-call lookahead, exact truncated/next_offset; malformed option before scan; entry/aggregate/file byte/deadline outcomes explicit |
| Native trust/data guards | Own data root inside project, direct/recursive paths, symlink escape and concurrent directory swap, FIFO; no content from outside/data root; canonical admitted absolute paths only |
| Policy/catalog | Actual root/Explore child provider schemas and search calls; Allow/Ask/Deny, denied file/absolute scope, no scan/effects on refused calls; effective capabilities stay read-only |
| Regression gates | Nearest files/tools/policy, TOOL01/A05/Deny/data/path, fg/bg/T54/T55; workspace fmt/strict clippy/tests, locked debug/release build/help; final direct native ELF hash before/after |

Donor owners: `packages/core/src/tool/plugin/{grep,glob}.ts`,
`packages/core/src/filesystem.ts` (default 100 / deadline 30 seconds),
`packages/core/src/ripgrep.ts:164–270` (`--no-config`, positive glob override,
grep `--hidden`, final `.git` exclusion and glob final hidden exclusion).

Native declared differences: no external-directory implicit grant, no symlink
following, no own-data-root access, bounded scan/compile/output resources,
lexicographic offset pagination rather than donor discovery order. Ambient HOME
Git excludes and ignore parents outside admitted Location are not read.

## Execution results

Initial 1,354-test qualification below is historical, not final acceptance.
Parent source review found a mandatory active-scan cancellation gap: synchronous
search blocked the async actor and Budget only checked time/entries. The held-
provider SIGINT fixture established pre-admission cancellation, not running-scan
cancellation. Current repair qualification is recorded separately below.
R1's search schemas/guidance are qualified, not the entire future tool inventory.
Deterministic fake-provider evidence is not live model proficiency, paid-campaign
completion, or T44 visual/pixel qualification. T50 remains active; R4–R8 and their
question/read/fetch/session-control obligations are not completed. T44 stays paused.

### Implementation / dependency provenance

- Existing `Files` owns `src/files/search.rs`; `files/tests.rs` owns the new risk
  scenarios. `tools.rs` validates canonical arguments before runtime intent and
  dispatches through that owner; `tools/tests/search.rs` exercises real tool lowering.
  `runtime.rs` publishes the actual schema; `approval.rs` previews the admitted
  scope; `permissions.rs` keeps explicit read Deny as an independent search ceiling.
  Missing read authority does not fabricate a read grant or suppress separately
  authorized search. Existing patch `glob_match` is unchanged and is not used by search.
- Direct crates.io pins: `regex =1.12.2` (Rust Project/Andrew Gallant,
  <https://github.com/rust-lang/regex>, MIT OR Apache-2.0), `ignore =0.4.24`
  (Andrew Gallant, <https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore>,
  Unlicense OR MIT). Their real package manifests/API examples were inspected.
  The latter's Gitignore/Override builders use `globset 0.4.18` (Unlicense OR MIT).
  Lock records the registry sources/checksums and regex-automata 0.4.14 /
  regex-syntax 0.8.11; ten new lock packages, no existing version upgrades.
- Local source cache initially had older regex-automata/syntax, but no ignore/
  globset/regex source directories. An isolated approved-cache manifest's offline
  fetch failed on `ignore 0.4.24`; the permitted standard locked fetch downloaded
  that crate. All production updates/builds/checks subsequently succeeded offline,
  without a toolchain upgrade or external `rg` production dependency. Donor's
  `ripgrep/binary.ts` pins 15.1.0; this is Rust engine compatibility, not a claim
  that an independently installed rg 14.1.1 is the donor oracle.
- 30-second scan deadline, 10,000 entries, 64 directory levels, 1 MiB/file,
  16 MiB aggregate scanned bytes (including bounded ignore rules), 4,096 pattern
  bytes, 10 MiB regex compile/DFA budget, 2,048-byte line previews; no raised gates.
  Over-file/aggregate/entry/deadline budgets fail explicitly. Text preview cuts
  are counted in `diagnostics.truncated_line_previews`; pagination uses one scan
  and limit+1 lookahead even at the 1,000-result public ceiling.

### Per-obligation evidence

| Obligation | Qualified observation |
| --- | --- |
| Regex/literal/case/file/directory/include | Native `canonical-root` and `canonical-explore-child`: `^foo[0-9]+$` → `src/a.rs:1 foo12`; literal `f.o` → line 2; Unicode insensitive `école` → line 3 `ÉCOLE`; default sensitive → no matches; admitted absolute file scope and brace includes are genuine calls |
| Hidden/ignore matching | Native `ignore-hidden-data`: glob default excludes dot paths, hidden:true adds exactly two; positive pattern/include overrides `.ignore`/`.rgignore`; `.git` and the inside-project own store stay absent. Owner test also proves .gitignore only with repository, explicit-file ignore bypass, nested negation and cross-class ancestor precedence |
| Validation/budgets | Native `invalid-before-scan` targets nonexistent scope with invalid regex/glob/option types/limit: precise validation failure wins over I/O; `scan-budget` reads a bounded 17×1 MiB fixture and returns budget failure. Owner AUD19 wide-directory/scan/text-preview/FIFO and injected expired deadline tests pass |
| Trust/path/policy | Native `boundaries`: own store, absolute outside Location, directory symlink and relative/absolute read-denied file scopes all fail. Allowed directory/include scans omit denied file contents. Owner concurrent directory swaps into external/data roots never expose trap contents |
| Catalog/root/child/Ask/Deny | Identical captured root/Explore search schemas, false literal/hidden defaults, true case default, limit 100 and closed object; Explore's actual catalog has no shell/patch/subagent. Both child tools genuinely run under Allow and --auto Ask; central Deny removes both child definitions and malicious calls become durable failed pairs. Root headless Ask exits 1 with zero tool-operation/grant rows; root/child --auto executes Once and still stores no grant |
| Graph/cancel/cleanup | Held completed-item frames produce zero operation rows until genuine response completion. SQLite terminal outputs equal captured function_call_outputs by exact call ID; validation, path and budget failures also have paired durable results. Held-response SIGINT exits 130 with zero operation rows; fixture processes/servers join and owned temporary trees clean on exit |

Native schema SHA256, identical on root/Explore and both ELFs:
grep `afb515a28153d7dac8699d912825b1c4bbaf9e1dbb599b8b787488695cdf4bb8`;
glob `705ea4aefb069b395b76ae0f1ce3be2b786c275575181907870d5e2664b3423f`.

### Initial commands / exits / artifacts (historical)

All Cargo commands use `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1
CARGO_NET_OFFLINE=true TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Resource preflight: uid 1003, MemAvailable above 2 GiB and free disk above 10 GiB
(final build preflight 6,696 MiB / 174 GiB). One Cargo owner, timeout 900000 ms.

| Command | Final exit / result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib files::` | 0; 19 PASS |
| `cargo test --locked -p oc-adapters --lib tools::tests::` | 0; 18 PASS |
| `cargo test --locked -p oc-adapters --lib approval` | 0; 3 PASS |
| `cargo test --locked -p oc-adapters --test permissions` | 0; 8 PASS |
| `cargo test --locked -p oc --test dcp_runtime aud19_aud20_aud21_binary_model_compress_nudges_and_restart` | 0; regression PASS after missing-read-authority correction |
| `cargo fmt --all -- --check` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo test --workspace --locked --no-run` then `cargo test --workspace --locked --no-fail-fast` | 0 each; full run 1,354 PASS / 0 fail / 10 ignored, 42 reported targets |
| `cargo build --locked`; `cargo build --release --locked`; both `oc --help` | 0 each; normal debug/release artifacts |
| `python3 evidence/T50/native_search.py target/{debug,release}/oc` | 0 each; 14 actual-binary cases each |
| `python3 evidence/T50/native_foreground.py target/{debug,release}/oc --background-supported` | 0 each; 21 cases each |
| `python3 evidence/T50/native_background.py target/{debug,release}/oc` | 0 each; 26 cases each, including join/cancel/crash/Revert/Fork/capacity |
| `python3 evidence/T54/native_runtime.py target/{debug,release}/oc` | 0 each; 9 retry/effect/child/compaction/title/PTY/restart cases each |
| Existing T55 `Fixture` via `runpy.run_path`, modes headless/pty/conflict/unclosed/eof/unknown, each ELF | 0; 6 cases each; direct existing fixture methods + finally close, no relay or campaign access |
| `git diff --check`; `python3 scripts/code_size.py --base <base> --changed` | 0; largest changed owner 2,247 lines; natural search/test seam mapped in CODE_MAP |

No Cargo followed those initial builds. Initial ELF hashes were identical before and after
the direct search/foreground/background/T54/T55 proofs:

- Debug: `99f3ad4c64918f9051cf80b4ceb16da62a59e581063f3afe764d889ab6a73a63`.
- Release: `8e7e8ce2fcf359532fb147a19375c93fde8efcc22b479a40ec1ea96684f66eb9`.

Small sanitized logs are in `/home/opencode/.cache/opencode-tmp/opencode/`:
`t50-search-workspace-final.log`, `t50-search-{debug,release}-final-3.log`,
`t50-search-{fg,bg}-{debug,release}.log`, `t50-search-t54-{debug,release}.log`,
`t50-search-t55-final.log`. They retain only bounded test output/facts, not fixture
trees or repeated derived Cargo fixture targets. Failed attempts: expected RED
on absent canonical options; legacy assertions updated to frozen donor semantics;
Clippy collapsible-if fixed; missing-read-authority regression fixed; first full
suite hit the 900-second limit, warmed repeat passed. Native harness corrections
isolated auxiliary title requests and respected headless Ask's pre-intent refusal;
SIGINT uses the established foreground fixture's 150 ms consumer-readiness window
while completion remains held. No failing test was disabled or baseline relaxed.
Owned new search logs, including failed development attempts, total 372 KiB on
disk; largest is 128 KiB, well below the per-log ceiling.

Dirty association: workspace manifests/lock; adapter `files.rs`, `files/search.rs`,
`files/tests.rs`, `tools.rs`, `tools/tests/search.rs`, `runtime.rs`, `approval.rs`,
`permissions.rs`; `docs/CODE_MAP.md`; this report and `native_search.py`.
HEAD unchanged; no staging/commit/push/progress/spec/Goal edits. Supported
trust/ordering/resource differences above remain;
ambient/outside-location ignore sources and linked external Git metadata are not
implicitly admitted. Existing live/post-compress/T44 obligations are not claimed PASS.

## Parent-review cancellation repair — current qualification

RED: `cargo test --locked -p oc-adapters --lib
tool14_active_scan_cancel_refuses_partial_and_joins_before_next_query` ran one
test and failed: **"grep: traversal held after partial result but async
cancel/controller could not run"**. This is a tiny real Runtime/provider/SQLite
fixture on Tokio's current-thread executor, not a sleep or large workload. The
private cfg(test) barrier stops after an actual first grep hit (or glob candidate).
The fake peer is stopped/joined even on that RED path.

Repair: `FileToolError::Cancelled`; the private search path borrows its supplied
token in every budget check. The public Files::glob/grep signatures retain an
uncancelled default. Tool execution supplies ctx.cancel; runtime search uses the
existing foreground-shell spawn_blocking strategy with a five-millisecond bridge
from **this invocation's ctx.cancel**, immutable copied permission rules/permit/
root, Drop cancellation, and unconditional worker join before durable finish.
The original ctx.cancel is checked again after join to refuse a final-result race.
This preserves the borrowed public cancellation API without unsafe lifetimes or
silently rebinding to another child/session's token. Readdir checks before each
entry; ignore loading/rules and path reopening, each 8 KiB file-read chunk, every
regex line/hit, sorted result boundaries all check cancellation/time. No scan or
regex budget/cap/deadline was weakened. Typed cancellation becomes the actual
cancelled ToolFinished/outcome and turn status, not a successful partial JSON page.

The strengthened test only releases traversal after its running guard observes
cancellation. Both grep/glob discard already accumulated partial results, emit
cancelled finish and durable paired error, cancel the queued shell before effects,
make no provider continuation/replay, join the scan, then successfully execute a
fresh query against the same runtime/session. No public test API or production
barrier exists. Current required gates/artifacts are verified below.

### Current checks / command exits

All Cargo commands used `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1
CARGO_NET_OFFLINE=true TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
one Cargo at a time and 900,000 ms command ceilings. Preflight: uid 1003,
MemAvailable 6,514 MiB and 172 GiB disk free before final builds.

| Current command / obligation | Actual result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib files::` | exit 0; 20 tests, including TOOL01/AUD19/own-data/FIFO/path-swap and both active-cancel scenarios |
| `cargo test --locked -p oc-adapters --lib tools::tests::` | exit 0; 18 tests, canonical options, Deny-before-dispatch and foreground alias/cancel ceilings |
| `cargo test --locked -p oc-adapters --test permissions` | exit 0; 8 tests, central authority/resource/child constraints |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --workspace --locked --no-run` then `cargo test --workspace --locked --no-fail-fast` | exits 0; **1,355 PASS / 0 failed / 10 ignored**, 42 reported targets; current final workspace qualification |
| `cargo test --release --locked -p oc-adapters --lib tool14_active_scan_cancel_refuses_partial_and_joins_before_next_query` | exit 0; exactly one test, two real Runtime/provider active-traversal scenarios |
| `python3 -B -m unittest discover -s scripts -p 'test_*.py'` | exit 0; **47/47** |
| `python3 -B scripts/check_docs.py`; `python3 -B scripts/progress.py check`; `git diff --check` | exits 0; documentation/journal checks read-only |
| `python3 -B scripts/code_size.py --base 9cbb5f2080e71b580682af142b27ade9154198f9 --changed` | exit 0; advisory; largest changed Rust owner application.rs 4,214 lines (+1 exhaustive error arm), search.rs 546 and owner tests 410 lines |
| `cargo build --locked`; `cargo build --release --locked`; both normal ELFs `--help` | exits 0 |
| Direct debug/release test ELFs with `--exact files::search::tests::tool14_active_scan_cancel_refuses_partial_and_joins_before_next_query` | exits 0; exactly one test each; 4 constituent grep/glob scenarios, no zero-test PASS |
| `python3 -B evidence/T50/native_search.py target/{debug,release}/oc` | exits 0; **14/14 per normal ELF**; root/Explore canonical schemas/calls, explicit Deny/Ask no effects, once permits, scope/ignore/data/symlink/budgets, pre-admission held-provider cancellation |
| `python3 -B evidence/T50/native_foreground.py target/{debug,release}/oc --background-supported` | exits 0; **21/21 per normal ELF** |
| Existing `native_background.py` functions `terminal_cases`, `restart_cases`, `normal_group_completion`, `review_capacity`, via runpy with **resolved absolute** normal ELF paths | exits 0; **6/6 per ELF**: timeout, idle cancel/output pressure, unknown restart/no replay, terminal once, group cleanup, joined capacity reuse |
| `python3 -B evidence/T54/native_runtime.py target/{debug,release}/oc` | exits 0; **9/9 per normal ELF** |
| Existing `native_completed.py` Fixture via runpy, modes `headless`→positive, `unclosed`→negative, `unknown`→unknown, close in finally | exits 0; **3/3 per normal ELF**; actual patch/read/reopen, unclosed no operations, unknown one effect/owned leaf joined/no replay; no Relay/live campaign |

Normal CLI proofs total **106 PASS**; the four active-traversal cases above are
separately identified **cfg(test) ELF** proofs of the same real Runtime execution
path, not a claim that normal CLI ELFs contain a test barrier. The existing native
held-provider case continues to prove only pre-admission cancellation. Both
normal profiles recaptured the unchanged root/Explore schema hashes above and
exact durable/continuation path/line/pagination metadata.

### Current artifact/source association

HEAD remains `9cbb5f2080e71b580682af142b27ade9154198f9` plus the reviewed owned
dirty changes. The repair additionally touches runtime/turn.rs (blocking typed
dispatch) and application.rs (+1 exhaustive Cancelled mapping); no dependency,
DB-schema, tool catalog or public legacy Files API change in this repair.
**No Cargo command followed the final normal builds.** Hashes before and after
all direct proofs match for the two normal CLI ELFs. The separate cfg(test) ELF
hashes below were recorded after their direct executions:

| Artifact | SHA256 |
| --- | --- |
| `target/debug/oc` | `6757df310a03ef6501e1e4dbef20875c3661f056d810171f2541bfd975572f17` |
| `target/release/oc` | `912e82691eebf76068d7658761deb5dce1ed0dc75d6327cba4f0e586ebf6c598` |
| Direct debug cfg(test) `target/debug/deps/oc_adapters-a5f3eb2acab51ce2` | `d63496a25f189cf6768ab6cb7afbe8004b105079dc7696dc135fb0d8defa1d23` |
| Direct release cfg(test) `target/release/deps/oc_adapters-7c2c3840a1cc6f0c` | `b959c72965616622d9092882b800c3e36b588ca543fcbd118ee56038f37f845a` |

Small logs are in the approved cache, prefix `t50-search-cancel-`: substantive
RED `red-2.log`; current `workspace-final.log`, `release-barrier.log`,
`direct-{debug,release}-barrier.log`, `native-{debug,release}.log`,
`fg-{debug,release}.log`, `bg-subset-final.log`, `t54-{debug,release}.log`,
`t55-subset.log`, `python.log`. No fixture trees or derived Cargo fixture targets
are retained. New repair logs total **232 KiB** on disk, largest **128 KiB**;
each is below the 16 MiB cap. Short failed-attempt facts: the first test compile used the wrong
DcpConfig import, and the new Cancelled variant needed the existing exhaustive
suggestion mapper arm; both corrected before qualification. One background
runpy invocation supplied a relative ELF path under a fresh cwd and failed before
process/tool start; rerun with `.resolve()` passed. No unknown effect was retried.

### Current risks / handoff

The active-scan cancellation counterexample is repaired and the current gates
above are green. The borrowed-token API intentionally uses the existing bounded
five-millisecond worker bridge; normal cancellation joins before outcome, and
Drop signals its read-only worker. Search trust/ignore/order/resource differences
from donor remain as already declared, with all original limits intact. No
external blocker is introduced. Parent independent diff/source review precedes
commit; no staging/commit/push/progress/Goal/spec/status edits were made.
R1 is search-only; **R4–R8 are still pending**, T50 remains ACTIVE, T44 PAUSED.
Fake-provider qualification does not claim live/model proficiency, post-compress
campaign acceptance or pixel parity.
