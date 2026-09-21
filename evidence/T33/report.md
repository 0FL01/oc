# T33 — Durable tool execution and recovery

Status: **PASS for T33 / AUD06–AUD08**, not overall product readiness.
Implementation: `63ad057` on base `18b0792`. Findings F06 and F16 were rechecked
after T31/T32, not assumed from the audit SHA.

## Reproduction and repair

Initial executed failures: [regression.md](regression.md). SQLite intent failure
still created a patch file; failed input insertion left a started turn; failed
terminal update left a completed assistant message; existing orphan blob caused
successful write followed by BlobNotFound. All four assertions now pass.

- Runtime no longer partitions builtins ahead of MCP. One ordered loop checks
  assembled input/required built-in argument shape, patch protection, permission
  and cancellation, commits intent, dispatches, then commits outcome. Tool-owned
  filesystem/URL/command validations remain before their own effects. Failures
  also have records, but no dispatch. Any DB failure stops the batch visibly.
- Durable operation IDs include turn/round/index and original provider identifier;
  rows retain session, turn, exact name and JSON arguments. T34 still must separate
  Responses item_id/call_id correctly; this task does not claim protocol repair.
- MCP no longer discards intent/outcome storage errors. Transport dispatch remains
  the existing adapter, with registered-tool lookup and central policy gating.
- Manual compress uses durable intent/outcome and propagates storage errors.
  It is not yet evidence of model-invoked compress integration (T36).
- Crate-local storage transactions atomically accept input with started turn/events
  and commit terminal turn/result/event with the optional assistant message. UI
  acknowledgement follows acceptance commit; failed terminal commit cannot expose
  a completed assistant. Existing storage API remains compatible.
- Application startup recovers unfinished operation/turn rows to `unknown` in one
  transaction. It never invokes the tools again; errors abort startup.
- Blob file presence alone cannot return a successful digest. Existing bytes are
  validated, quota includes regular orphan/temp files, valid missing metadata is
  repaired, file and directory synchronization precede metadata publication.
  Read verifies metadata/content; GC and publication serialize under the existing
  mutex. Unique exclusive temps allow retry and are cleaned on failed writes.

## Acceptance mapping

| Finding / acceptance | Executed regression and observed result |
|---|---|
| F06 / AUD06 intent-before-mutation | `aud06_intent_failure_prevents_patch`: SQLite BEFORE INSERT failure returns Storage and sentinel does not exist. `aud07_mixed_order_and_mcp_storage_failures`: failed MCP intent does not invoke MCP or later builtin. Existing compress test also checks no block mutation on failed intent. |
| F06 / AUD06 process kill | `crates/oc/tests/durability.rs` launches actual configured oc, performs shell append, SIGSTOPs parent before outcome then SIGKILLs it. Started operation and turn survive with original metadata/input; actual binary restart changes them to unknown, does not append again, does not fabricate assistant/tool result or replay request. Adopted shell is explicitly reaped and normal exit checked. |
| F06 / AUD07 mixed order | Actual shell/stdin MCP/shell execution appends B1/M/B2 in exact order. Failed MCP intent leaves B1; failed MCP outcome leaves B1/M; both surface Storage and stop remaining execution. |
| F06 / AUD07 terminal transaction | Input insert fault rolls back turn and events and never acknowledges. Terminal update fault rolls back assistant and terminal event. Reopened storage retains only committed history. Actual-binary crash/restart and existing CLI/PTY resume pass. |
| F16 / AUD08 orphan blob | `blob_audit`: valid orphan write returns digest that read_blob resolves; injected row failure after file publication remains error, retry/reopen repairs it. Physical quota includes orphan/temp bytes; metadata/content validation and GC preserve referenced files. Isolated RLIMIT_FSIZE injection proves partial temp cleanup. |

## Executed qualification

Full bounded command/results: [checks.md](checks.md).

- Targeted: runtime 16/16, tools 10/10, blob_audit 11/11, actual binary kill 1/1.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: exit 0.
- `cargo test --workspace --locked`: all executed tests pass, including T31 actual
  binary config/restart and T32 patch safety. Three existing external-only ignored
  harnesses are **NOT RUN**, not PASS; no new ignored test.
- `cargo fmt --all -- --check`, `cargo build --locked`, `target/debug/oc --help`,
  `git diff --check`: exit 0.
- Progress/docs structure checks: exit 0 (43 tasks / 122 acceptance specs);
  those checks are not product evidence.

## Limits and next task

No distributed exactly-once or automatic rollback/replay. A side effect whose
outcome cannot be committed remains ambiguous until recovery marks it unknown;
user must inspect before explicitly retrying. Linux kill fixture does not claim
hardware power-loss simulation. Directory fsync errors were not injected.
Owned data root is not a malicious same-UID sandbox. Blob fixes do **not** prove
that all outputs use blobs; output/history bounding remains T40.

Four Rust 2024 packages preserved; no dependency, schema, lockfile or public
generic API added. No live/paid probes, no credentials, no valuable user-data
mutations, no push. T34–T42 remain product work; T27 remains blocked by offline
qualification. Next: start ready T34 and reproduce typed Responses/terminal/
streaming/cancellation/continuation findings, then rerun binary smoke against
the corrected protocol. **NOT READY; not BUILD_READY_LIVE_BLOCKED.**
