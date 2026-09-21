# T33 — executed checks

Base `18b0792`; offline Linux non-root worktree. No production config/credentials
used. Initial failures: [regression.md](regression.md).

## Final targeted run — exit 0

```text
cargo fmt --all
cargo test --locked -p oc-adapters --test runtime
  16 passed; 0 failed; 2.79s
cargo test --locked -p oc-adapters --lib tools::tests
  10 passed; 0 failed; 0.11s
cargo test --locked -p oc-adapters --test blob_audit
  11 passed; 0 failed; 0.28s
cargo test --locked -p oc --test durability
  1 passed; 0 failed; 0.22s
cargo clippy --locked --workspace --all-targets -- -D warnings
  Finished dev profile in 2.63s; exit 0
```

AUD06 actual-binary test stops oc after a shell append, SIGKILLs it before
outcome, inspects durable started state, then restarts the same session through
normal application startup. Unknown state, unchanged sentinel and stable input
message identity are asserted; test-local subreaper verifies shell exit/reap.

AUD07 runtime fake Responses + real local stdio MCP: observed file append order
`B1/M/B2`. An intent trigger failure leaves only `B1`; an outcome trigger failure
leaves `B1/M`, returns Storage and does not run B2. Input-message and terminal-turn
SQLite trigger failures roll back associated state/events/history, including
history checks after storage reopen. Existing compress test now verifies that
failed intent prevents compression-block writes.

AUD08 covers valid orphan adoption; row-trigger failure after file publication,
retry and reopen; quota includes orphan/temp files; referenced GC preservation;
corrupt content refusal, metadata repair, concurrent publication/GC/quota, and
partial temp write failure in an isolated RLIMIT_FSIZE subprocess.

## Final mandatory workspace run — exit 0

```text
cargo test --workspace --locked
  compilation: 10.40s
  oc unit: 1 passed
  actual binary AUD01: 1 passed
  actual binary AUD06 kill/reopen: 1 passed (0.23s)
  actual binary PTY/restart: 13 passed (8.04s)
  adapters unit: 100 passed (2.40s)
  blob_audit: 11 passed (0.24s)
  offline E2E: 3 passed
  remote MCP: 15 passed, 1 existing external-only ignored
  stdio MCP: 6 passed, 1 existing external-only ignored
  patch_audit: 10 passed
  runtime: 16 passed (2.74s)
  soak: 4 passed (11.15s)
  core: 16 passed
  TUI: 31 passed
  doc tests: 0 tests, exit 0
cargo fmt --all -- --check
  exit 0
cargo build --locked
  Finished dev profile in 2.71s; exit 0
target/debug/oc --help
  Usage: oc [OPTIONS] [COMMAND]
  Commands: run, sessions, tui, help
  exit 0
git diff --check
  exit 0
```

Exactly three pre-existing external harnesses remain NOT RUN, not PASS:
`live_workflow_harness`, `live_search_harness`, `real_server_smoke`.
No new ignored test. Workspace run is applicable because storage transactions,
recovery and tool dispatch are shared by both application frontends.

Intermediate integration runs caught a changed denial-state label and changed
direct-executor diagnostics. Implementation was corrected to retain existing
contracts; tests were not weakened. Final suite above ran after corrections.

## Review boundary

Reviewed production diff and new regression fixtures. Four packages unchanged;
no dependency, schema or Cargo.lock change. New storage methods are crate-local;
existing begin/finish/read/write APIs retained. Runtime has one ordered dispatch,
not a builtins-first partition. SQLite failures propagate; actual application
startup invokes recovery. No claim of exactly-once external execution, physical
power-loss simulation, malicious same-UID isolation, or output blob integration.
Later corrective tasks remain unresolved despite these passing existing suites.
