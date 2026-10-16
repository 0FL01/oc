# T60 — Code Mode `execute` (native Rust interpreter)

Status: DONE locally; R1–R7/CM01–CM08 verified on code `366039687`, current required
offline gates PASS. Historical live execute smoke and remote delivery are separate below.
Spec: `docs/goals/2026-10-13-code-mode.md`. Owner amendment 2026-10-13 in `GOAL.md`.

## Commits

- `59cab66b0` feat(codemode): native Rust interpreter for the execute tool
- `495070f5a` feat(codemode): expose execute and route MCP through Code Mode
- `5eded43c7` test(codemode): pin binary MCP fixtures to direct exposure
- `51aa7a6be` docs(codemode): owner amendment, T60 registration and live smoke
- `2437020f0` fix(codemode): enforce unavailable APIs and exact log caps
- `366039687` fix(codemode): preserve exact MCP path segments

## What exists

- `crates/oc-adapters/src/codemode*` — own lexer/parser/AST, async tree-walking evaluator on
  a dedicated thread (`LocalSet`), promises/generators, stdlib, host bridge (`tools.*`,
  `search`, `fetch`), donor data boundary/result format, catalog signatures/search. Only new
  dependency: `regress =0.12.0` (pure-Rust ECMAScript regex, `utf16`), plus explicit
  `indexmap` already in the lockfile. No JS engine, Node/Bun or plugin host.
- `crates/oc-adapters/src/runtime/code_mode.rs` — MCP split (default Code Mode,
  `codemode:false` direct), `execute` definition, nested `admit_tool` + MCP lease,
  `fetch` through `webfetch::fetch_raw` (new guarded raw exchange in the webfetch owner).
- `codemode: true` is no longer rejected by config/stdio/remote admission; `execute`
  gets a central module grant (Allow), explicit user Ask/Deny still applies.

## Initial implementation checks (actual, before current qualification)

| Check | Command | Result |
| --- | --- | --- |
| Unit CM01–CM06 | `cargo test -p oc-adapters --lib codemode` | 19 passed |
| Integration CM07–CM08 | `cargo test -p oc-adapters --test runtime code_mode` | 6 passed |
| Adapter suite | `cargo test -p oc-adapters --no-fail-fast` | all targets pass; one lib timing test (`discovery::tests::reqwest_http_error_status_precedes_oversized_or_slow_body`) failed once under load and passed alone |
| Lints | `cargo clippy -p oc-adapters --all-targets`, `cargo fmt --all -- --check` | clean |
| Build | `cargo build --locked`, `target/debug/oc --help` | PASS |
| Workspace | `cargo test --workspace --no-fail-fast` | first run: MCP-default fallout in `oc` binary targets (fixtures assumed direct MCP) → fixed in `5eded43c7`; reruns `mcp_application` 41/0, `golden_binary` 1/0, `pty_t42` 35/0, `pty_t39 vis31` 1/0, `live_bounded` 15/0 (needs a private `TMPDIR`; under shared `/tmp` its envelope `init` refuses `untrusted_ancestor` by design, independent of this change) |

Existing direct-MCP runtime fixtures were pinned to `codemode: Some(false)` (they test
direct exposure); the stdio mapping test and config capability test now assert that
`codemode: true` is admitted.

## Current-source qualification — 2026-10-14

T60 was still journal-todo despite the committed implementation/report. It was
adopted at `ba48c74e3` after T58 completion. The shared source baseline at `650fa99ac`
already passed locked workspace tests (1906/0/12 existing opt-in ignored), strict
all-target clippy, fmt, locked build and help. The original-contract review then
found and reproduced the following gaps, rather than treating a broad green run as closure:

- R3: `return process.env` succeeded with null; `new Function('return 1')` produced
  TypeError instead of the required explanatory ReferenceError. Runtime names are
  now absent host globals with an explanatory missing-name diagnostic (ordinary
  local shadowing still works). `Function` keeps its normal prototype but both call
  and construction fail with ReferenceError, including bound construction.
- R7: console admission omitted the newline which accounting then retained, allowing
  an extra empty line one byte beyond the cap. Admission now compares the complete
  UTF-8 line plus separator with the remaining byte budget, without addition overflow.
- R2 follow-up: the MCP registry admits literal dots in server/tool names, but catalog
  and bridge flattened them into one dotted string. A two-server runtime reproduction
  returned 105/105 instead of 105/5 for `fixture`/`v1.add` and `fixture.v1`/`add`, and
  accepted the unadvertised normalized alias. Its regression was red, then passed
  with `366039687`: structured literal segments drive render/search/keys and exact
  lookup; dispatch uses the collision-safe actual registry key. Normal dotted-path
  `Catalog::new` behavior, ordering, permissions and captured MCP leases are preserved.

Three owner regressions were red against the original implementation (19 PASS /
3 FAIL), then green with the narrow fixes committed in `2437020f0` (base `ba48c74e3`).
No expectations, limits or ignored tests were weakened. No new dependency, public
API, host, runtime owner or paid campaign.

| Command | Current result |
| --- | --- |
| `cargo test --locked -p oc-adapters --lib codemode` | 22 passed / 0 failed. |
| `cargo test --locked -p oc-adapters --test runtime code_mode` | 7 passed / 0 failed, including actual collision routing and alias refusal. |
| `cargo test --workspace --locked` | 1910 passed / 0 failed / 12 existing opt-in ignored;46 result records, including all four new regressions. |
| `cargo fmt --all -- --check` | PASS. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS. |
| `cargo build --locked`; `target/debug/oc --help` | PASS. |
| Python progress/docs unit suites | 15 +14 passed /0 failed. |
| `scripts/progress.py check`; `scripts/check_docs.py`; `git diff --check` | PASS; structure checks do not establish product acceptance. |

Run environment: approved private TMPDIR, `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=2`, Rust/Cargo 1.93.0, dedicated UID1003, normal stacks.
Gitignored logs: `.local/evidence/t60-owner-red.log`, `t60-paths-red.log`,
`t60-owner-paths-final.log`, `t60-runtime-paths-final.log`,
`t60-{fmt,clippy,workspace,build,help}.log`, `t60-{progress,docs}-tests.log`.
The first green broad run before the R2 correction is retained as `t60-stage1-*.log`,
not used as final-source proof. All final code gates were rerun after the last source
edit at `366039687`; all four new test names are confirmed in that final workspace log.

### Frozen outcome evidence map

| Outcome | Status | Direct evidence / owner |
| --- | --- | --- |
| R1 tool | verified | `codemode::{DESCRIPTION,input_schema,tool_description}` matches the pinned donor invariant text; `composition` grants the module default without replacing explicit user rules; runtime absence/Deny cases pass. |
| R2 catalog | verified | `Catalog`/`schema_to_typescript`, signature/search/bracket tests, real literal-dot server/tool collision regression; runtime filters actual denied names before request assembly. |
| R3 language | verified | Existing language/control-flow/class/closure/generator/diagnostic cases plus the process API and dynamic constructor regressions. |
| R4 stdlib | verified | Existing number/string/collections/date/URL/encoding/Promise/RegExp cases; Function prototype invocation remains functional with dynamic compilation forbidden. |
| R5 boundary | verified | `data_boundary_matches_the_donor_table` and `logs_and_warnings_are_formatted`: Promise/Set/Error/URLSearchParams, JSON omissions, Uint8Array/circular refusals, prototype-key filtering and output layout. |
| R6 host | verified | Seven `tests/runtime/code_mode.rs` cases and the unit concurrency barrier: real stdio effects, exact dispatch, single outer op, own nested Ask/Deny, direct opt-out, guarded fetch; unique `<call>#<n>` admission and existing MCP lease. |
| R7 guards | verified | Existing steps/cancel/stack/string/array/promise cases plus exact UTF-8 console boundary; resource limits remain enabled, no successful-step/round quota added. |

Closure checked every frozen R1–R7 outcome, affected authority/resource/raw-history
invariants and the reviewed diff envelope. No outcome remains pending. Initial
timing/full-workspace caveats are historical, not current failures; no test/baseline
or cap was disabled or relaxed to achieve the final green result.

## Live

`crates/oc-adapters/tests/e2e_live.rs::live_code_mode_execute_reaches_mcp` (ignored,
env-gated), run once with `.local/live.env` (values not printed), MCP URL shape
`{LUDKA2_API_URL}/mcp` as in the documented config, `codex_web` in default Code Mode:
`LIVE_CODE_MODE status=Completed calls=[("execute", "completed")]`, 8.07 s, PASS.
One turn (generation requests within the runbook envelope), one Code Mode program instructed
to reach `codex_web`. Model text is not asserted; the nested search result itself is not separately
asserted beyond a completed, non-empty `execute` output. An earlier attempt failed before
any model request (wrong MCP URL derivation in the test; fixed).

This is historical evidence recorded with `51aa7a6be`, not a new live run during
2026-10-14 qualification. It proves the live `execute` completion smoke as described,
not an independently observed nested search result or full current-binary live campaign.
CM07's deterministic runtime fixture separately proves actual nested MCP effects.
No live credential read, paid request or unknown external-effect replay is repeated
merely to close the journal; overall mandatory live/product qualification stays separate.

## Declared differences / limits

As listed in the spec: guards always on (50M steps, 16 MiB strings, 10M arrays, 10k
pending promises, 1 MiB logs, native-stack RangeError), catalog in the tool description,
no `?codemode=false` URL query, UTC local time, `normalize` is identity, fetch body above
1 MiB refused, nested calls are part of the `execute` operation (no separate TUI cards).
Cyclic garbage created during one program is reclaimed when the program ends.

Remote delivery is separate from local task completion. The existing dedicated-account
write-auth blocker from T44 handoff147 is unchanged; no push retry, foreign credential
lookup, forced update or product READY/full live/paired-visual PASS is implied here.
