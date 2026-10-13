# T60 — Code Mode `execute` (native Rust interpreter)

Status: implementation landed; offline checks PASS; bounded live smoke PASS (see «Live»).
Spec: `docs/goals/2026-10-13-code-mode.md`. Owner amendment 2026-10-13 in `GOAL.md`.

## Commits

- `59cab66b0` feat(codemode): native Rust interpreter for the execute tool
- `495070f5a` feat(codemode): expose execute and route MCP through Code Mode
- `5eded43c7` test(codemode): pin binary MCP fixtures to direct exposure
- closeout commit: live smoke test, this report, GOAL amendment, D24, CM01–CM08, T60

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

## Checks (actual)

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

## Live

`crates/oc-adapters/tests/e2e_live.rs::live_code_mode_execute_reaches_mcp` (ignored,
env-gated), run once with `.local/live.env` (values not printed), MCP URL shape
`{LUDKA2_API_URL}/mcp` as in the documented config, `codex_web` in default Code Mode:
`LIVE_CODE_MODE status=Completed calls=[("execute", "completed")]`, 8.07 s, PASS.
One turn (generation requests within the runbook envelope), one Code Mode program reaching
`codex_web`. Model text is not asserted; the nested search result itself is not separately
asserted beyond a completed, non-empty `execute` output. An earlier attempt failed before
any model request (wrong MCP URL derivation in the test; fixed).

## Declared differences / limits

As listed in the spec: guards always on (50M steps, 16 MiB strings, 10M arrays, 10k
pending promises, 1 MiB logs, native-stack RangeError), catalog in the tool description,
no `?codemode=false` URL query, UTC local time, `normalize` is identity, fetch body above
1 MiB refused, nested calls are part of the `execute` operation (no separate TUI cards).
Cyclic garbage created during one program is reclaimed when the program ends.
