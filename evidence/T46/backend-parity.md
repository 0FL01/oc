# T46 / backend R3 — MCP result and instruction projection

Date: 2026-09-22. Base HEAD: `15719e976030a8fdaadd843ac13556de237f559d`.
Scope: `docs/goals/2026-09-22-backend-blockers.md` R3, alongside the existing
T46 spec/report and D13. This is an uncommitted backend slice, not a T46 finish
or a replacement for the historical report/live qualification.

Reference checkout HEAD verified with `git rev-parse HEAD`:
`2670273ff17da96f85c5826ced57aa1b368754fa` (upstream v2.0.12).
Relevant source: `packages/core/src/mcp/client.ts:319–337` preserves structured
content, text, media and resource content; `mcp/index.ts:640–647` retrieves
initialize instructions; `mcp/instructions.ts:81–105` gates guidance on reachable
server tools. Implementation is a small native adaptation to the existing Rust
Responses tool-output surface.

## Delivered behavior and interfaces

- Both `CodexWebClient::call_tool` and `StdioClient::call_tool` retain their
  `Result<String, ...>` interface. Shared `mcp_result::project` preserves text,
  structured-only JSON and mixed text + structured JSON. Structured data is
  serialized as a `{"structuredContent": ...}` JSON object, retaining nesting.
- Embedded textual resources retain their URI/MIME/text JSON; resource links
  retain the advertised link metadata. These reach the next provider request as
  normal function-call output. No implicit resource fetch occurs.
- `isError` remains failed. `ToolFailedDetail(FailureDetail)` adds useful fixed
  categories/hints for invalid arguments, not found, rate limiting, denied access,
  and timeout. Standard JSON-RPC invalid-params/method-not-found errors also map
  to these categories. Arbitrary error messages, data, paths, URLs and server
  stack text never become diagnostics; unknown errors remain opaque.
- Both clients expose `instructions() -> Option<&str>`. Internal
  `connect_redacted` / `launch_redacted` accept generation redactions before
  projecting initialize guidance. Guidance is capped at 8 KiB/server, UTF-8-safe,
  with an explicit truncation marker. Literal secret replacement precedes the
  bound, preventing a secret crossing that boundary from leaving its prefix.
- Runtime `mcp_instruction_input` uses the exact turn lane's `RuntimePolicy`,
  including the concurrently implemented ordered resource rules. Only an
  attached server owning at least one allowed tool contributes guidance; the
  header enumerates its permitted tools. `Ask`, deny, absent permission, failed
  attach, disabled servers and empty catalogs do not contribute instructions.
- Guidance is rebuilt into `lane.fixed_input` for provider projection and included
  in admission/nudge estimates. It is never added to `TurnLog`, persisted turn
  input or immutable history. Reload reevaluates visibility; previous raw history
  rows are unchanged. The fixed lane also serves child turns instead of using
  primary workspace instructions accidentally.
- Generation redactions cover provider API keys/base URLs/header values, MCP
  URL/header values and auth token portions, and credential-named parent env
  values. Adapter redactions additionally cover its bearer/custom headers or
  configured stdio secrets. Stdio guidance and tool-result projection share the
  complete retained redaction set, including launch argv/working env values.
  Structured sensitive fields such as token/password/headers/env are redacted,
  including unknown-value canaries. Known literal values are removed from text
  and JSON string values before persistence/provider delivery.
- The complete serialized result and final output retain a 1 MiB ceiling.
  Existing catalog/page/server caps, cancellation and owned transport/process
  cleanup are retained. D13 attach degradation still supplies safe warnings;
  `report.warnings.extend(mcp_warnings)` is preserved alongside model warnings.

## Native distinctions / remaining parity

The current `InputItem::FunctionCallOutput` has a string output, not media parts.
Image/audio/blob content therefore fails explicitly, including mixed results:
it is not dropped, converted to purported visible base64 data, or reported as a
successful partial result. Ordinary Responses message image input is a different
surface and is not repurposed here. This remains a native media-output parity gap.

Error diagnostics deliberately expose fixed categories rather than upstream's
arbitrary raw message. Unknown error detail remains hidden. Literal redaction of
successful content/instructions is not a sandbox or a guarantee against an
adversarial server encoding secrets in data. SDK-owned initialize metadata may
retain its original payload internally; the 8 KiB bound applies to the projected
guidance, not an SDK allocation claim.

Upstream treats `ask` as reachable because it has an approval path. This native
runtime has no approval channel, so only `Allow` contributes server guidance.
Guidance is filtered at server granularity using its allowed tools, matching the
upstream reachability principle; its prose is not parsed as per-tool policy.

MCP prompts/resource catalog operations are not implemented or qualified by this
slice. They are separate outstanding parity work; embedded resource/link tool
results here do not establish catalog/read/prompt support. No scope exclusion or
completion claim is made for those operations. Existing task/input-required MCP
responses remain unsupported.

## Checks actually run

Final successful commands, all exit 0:

```text
cargo test --locked -p oc-adapters --test mcp_remote --test mcp_stdio --test runtime
  mcp_remote: 24 passed, 0 failed, 1 existing live ignore
  mcp_stdio:  12 passed, 0 failed, 1 existing real-server ignore
  runtime:   38 passed, 0 failed

cargo test --locked -p oc-adapters --lib mcp_result
  4 passed, 0 failed

cargo clippy --locked -p oc-adapters --lib --test mcp_remote --test mcp_stdio --test runtime -- -D warnings
  exit 0

rustfmt --edition 2024 --check --config skip_children=true crates/oc-adapters/src/mcp_result.rs crates/oc-adapters/src/mcp_remote.rs crates/oc-adapters/src/mcp_stdio.rs crates/oc-adapters/src/runtime.rs crates/oc-adapters/tests/mcp_remote.rs crates/oc-adapters/tests/mcp_stdio.rs crates/oc-adapters/tests/runtime.rs
  exit 0
git diff --check
  exit 0
```

New regression coverage:

- Remote and stdio structured-only/text+structured output, embedded text resource
  content; remote resource-link metadata and JSON-RPC errors.
- Bounded Unicode instructions, header/bearer/env/config canaries, unknown secrets
  in sensitive JSON fields, useful fixed errors without arbitrary canary leakage.
- Actual runtime fake-provider continuation contains all supported data and two
  linked tool outcomes (completed structured call, failed categorized call).
- Runtime guidance appears once per request only for the allowed attached server;
  denied/ask/empty/broken/disabled/unlisted servers are excluded. Reload to a
  resource-specific ruleset excludes guidance despite the scalar compatibility
  `Allow`. History prefix and persisted turn results prove projection isolation.
- Mixed image/audio/blob results fail instead of silently losing content;
  aggregate/structured result limits fail; secrets matching a protocol discriminator
  do not invalidate ordinary text before projection.
- Existing tests in these suites cover fatal cancellation, catalog limits, server
  cap before spawning, partial attach degradation, refresh, shutdown and reaping.

Earlier attempts retained as factual failures: initial `cargo check` found an
owned `peer_info` borrow error (fixed before tests); the first locked test command
was blocked by concurrent `preserve_order`/Cargo.lock work; later compilation
found a new fixture argv argument type mismatch (fixed), and temporarily missing
`permission_rules` fields in concurrent runtime test updates (resolved by the
permission work). The final commands above supersede those attempts. No failed
test was suppressed or newly ignored.

No live credentials/network qualification or whole-workspace acceptance claim is
made by this slice. Parent integration owns final affected-workspace gates and
delivery. No commit/push or T44 task/state/spec/evidence/TUI edits were performed.

## Owned files

- `crates/oc-adapters/src/mcp_result.rs` (new shared projection/error boundary)
- `crates/oc-adapters/src/mcp_remote.rs`, `src/mcp_stdio.rs`
- `crates/oc-adapters/src/lib.rs` (one module export)
- `crates/oc-adapters/src/runtime.rs` (MCP attach redactions, fixed instruction
  projection, typed MCP execution/attach error mapping; shared file)
- `crates/oc-adapters/tests/mcp_remote.rs`, `tests/mcp_stdio.rs`
- `crates/oc-adapters/tests/runtime.rs` (one new MCP integration; shared file)
- This evidence file.

No core DTO/provider wire shape migration was required. Parent/permission/model
changes in shared files remain present and are not claimed as MCP-owned changes.

## Review follow-up — successful stdio argv echo redaction

The reviewer identified an actual gap: initialize guidance included argv/working
env redactions, but `call_tool` passed only `config.secrets` to result projection.
Before the fix, both extended regressions failed (exit 101):

```text
cargo test --locked -p oc-adapters --test mcp_stdio backend_parity_stdio_data_instructions_and_redacted_errors
  failed: structured argvEcho retained CONFIG-CANARY instead of [redacted]
cargo test --locked -p oc-adapters --test runtime backend_parity_mcp_projection_permissions_history_and_error_canaries
  failed: CONFIG-CANARY reached the captured provider request
```

Minimal correction: `StdioClient` now retains the already-assembled complete
`redactions` vector and supplies it to both initialize guidance and tool-result
projection. Typed errors remain fixed-category/payload-free. The launch config
and restart/lifecycle behavior are unchanged.

The stdio fixture returns its configured argv canary in successful structured
and text content. The runtime fixture actually echoes `sys.argv[2]` into both
surfaces; captured provider continuation, immutable history and persisted turn
result assertions all exclude that canary after the fix. The existing argv/env
echo test now expects withheld values while still asserting two intact argv
entries (including the unsplit multiword argument), not raw configured values.

Focused post-fix checks, all exit 0:

```text
cargo test --locked -p oc-adapters --test mcp_stdio
  12 passed, 0 failed, 1 existing real-server ignore
cargo test --locked -p oc-adapters --test runtime backend_parity_mcp_projection_permissions_history_and_error_canaries
  1 passed, 0 failed
cargo clippy --locked -p oc-adapters --lib --test mcp_stdio --test runtime -- -D warnings
  exit 0
rustfmt --edition 2024 --check --config skip_children=true crates/oc-adapters/src/mcp_stdio.rs crates/oc-adapters/tests/mcp_stdio.rs crates/oc-adapters/tests/runtime.rs
  exit 0
git diff --check
  exit 0
```

Follow-up files: `src/mcp_stdio.rs`, `tests/mcp_stdio.rs`, `tests/runtime.rs`
under `crates/oc-adapters`, and this evidence. No T44 edits or commits; existing
concurrent changes preserved. HEAD remains `15719e976030a8fdaadd843ac13556de237f559d`.

## Final focused review — exact error codes, no keyword inference

Removed whole-payload/prose substring classification from `mcp_result.rs`.
It could misclassify an unrelated `timeout:false` field as a timeout, or let
"invalid parameter" prose override a structured rate-limit code. Error categories
now use only the following optional native convention, **not a universal MCP
error schema**:

- For `isError:true`, read `/structuredContent/error/code`; only when that path
  is absent, try `/structuredContent/code`. A present unknown/non-string nested
  code stays opaque rather than falling through to another code.
- Exact, case-sensitive supported strings: `INVALID_ARGUMENTS`, `NOT_FOUND`,
  `RATE_LIMITED`, `ACCESS_DENIED`, `TIMEOUT`. No substring matching, normalization,
  arbitrary code exposure, or prose parsing. Unknown/absent codes remain failures
  with opaque detail. Content text and unrelated JSON fields cannot set a category.
- JSON-RPC keeps only standard numeric `-32602` (invalid arguments) and `-32601`
  (not found) mappings. Other numeric codes remain opaque regardless of message
  or `data` contents.

Remote, stdio and runtime fixtures now supply actual structured codes when
asserting categories. The stdio fixture deliberately pairs `RATE_LIMITED` with
conflicting "invalid parameter" prose. Unit regressions cover both supported
paths, exact casing, `EIO` plus `timeout:false`, misplaced/null/unknown codes,
conflicting nested/direct codes and unknown JSON-RPC messages with tempting
keywords. `isError` always remains a failure; raw arbitrary error text never
enters provider or durable output.

Before the correction, `cargo test --locked -p oc-adapters --lib mcp_result`
reproduced the defect: 3 passed / 3 failed, exit 101 (opaque-prose, exact-code
precedence and unknown-JSON-RPC regressions). Final checks:

```text
cargo test --locked -p oc-adapters --lib mcp_result
  6 passed, 0 failed; exit 0
cargo test --locked -p oc-adapters --test mcp_remote --test mcp_stdio --test runtime
  remote 24 passed / 1 existing live ignore
  stdio 12 passed / 1 existing real-server ignore
  runtime 41 passed; all 0 failed; exit 0
cargo clippy --locked -p oc-adapters --lib --test mcp_remote --test mcp_stdio --test runtime -- -D warnings
  exit 0
rustfmt --edition 2024 --check --config skip_children=true crates/oc-adapters/src/mcp_result.rs crates/oc-adapters/tests/mcp_remote.rs crates/oc-adapters/tests/mcp_stdio.rs crates/oc-adapters/tests/runtime.rs
  exit 0
git diff --check
  exit 0
```

This follow-up changed only `src/mcp_result.rs`, the three adapter/runtime test
fixtures, and this additive evidence. No production runtime/TUI refactor, T44
edits, commits or live calls. Parent owns broader contract/document integration.
