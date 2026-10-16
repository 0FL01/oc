# T53/R3 GO03 — native Chat Completions wire (atomic)

Not a GO03 PASS: Messages, Go metadata lanes, protocol journal provenance
(R4) and live qualification remain. Pinned donor:
`packages/ai/src/protocols/openai-chat.ts` (OC2 `2670273…`).

## Behavior

- Binding: `config.rs::package_protocol` maps `@ai-sdk/openai` /
  `@opencode/ai/providers/openai` (or omitted) → Responses and
  `@ai-sdk/openai-compatible` / `@opencode/ai/providers/openai-compatible` →
  Chat; Anthropic and unknown packages stay unsupported before effects (the
  Messages adapter is a later slice). `provider_wire` validates explicit
  per-model `compatibility` (`reasoningField` string/`{field}`,
  `maxTokensField`, `supportsPromptCacheKey`) plus legacy `interleaved`
  string/`{field}` → `reasoningField`; boolean `interleaved` invents nothing;
  other compatibility keys (e.g. `requireReasoning`) are refused as having no
  native semantics. `ResponsesConfig.wire` carries the immutable binding;
  `same_request_binding` now also compares the protocol.
- Dispatch: `stream_input_overlaid` routes every lane (main, tool follow-up,
  title, summary compaction, child, retry) to `stream_chat_overlaid` for a Chat
  binding, reusing the existing HTTP client, private-host guard, cancel,
  idle/connect timeouts, byte/event caps, status classification and the
  one-attempt contract (`stream_body`/`stream_attempt` only choose URL
  `/chat/completions` and the event decoder).
- Request (`provider/chat.rs::request_body`): leading developer items form one
  `system` message; chronological system/developer updates use the escaped
  `<system-update>` user text in place; user text/images, assistant text, Chat
  `tool_calls` (consecutive output items merge into one assistant message),
  `tool` results; MCP/read results lower to text and other modalities refuse
  (`ProviderError::UnsupportedModality`); alien Responses opaque state never
  crosses wires; Chat-origin reasoning replays only through the configured
  field. `stream:true`, `stream_options.include_usage`, `max_tokens` or
  explicit `max_completion_tokens`, exact selected `reasoning_effort`; no
  Responses `store`/`include`, no host/vendor/model dialect guessing; empty
  `tools:[]` kept when history has tool calls; profile body overlays cannot
  replace runtime-owned fields.
- Stream (`chat::dispatch` inside the shared `SseParser` framing): text,
  configured/known reasoning fields, indexed fragmented parallel tool calls
  with identity-conflict refusal, usage (top-level or per choice), finish
  reasons (`stop`/`tool_calls` → stop, `length` explicit, `content_filter`
  and unknown/error → typed failure), content after the finish reason refused,
  `[DONE]` or EOF after a finish reason as the only terminals; tool ids/names
  and object JSON arguments validated before synthesizing the common
  `message`/`function_call`/Chat-origin `reasoning` output the runtime already
  journals and executes.

## Checks

- `provider::chat::tests::go03_chat_request_lowers_common_history_in_order`
- `provider::chat::tests::go03_chat_stream_assembles_fragmented_parallel_tools_reasoning_and_usage`
- `provider::chat::tests::go03_chat_stream_refuses_untrustworthy_terminals`
- `provider::tests::go03_chat_wire_one_attempt_url_auth_body_and_common_output`
  (fake HTTP: one POST to `/chat/completions`, Bearer, Chat body, common output).
- `config::tests::go03_package_protocol_and_explicit_chat_compatibility`
- `application::tests::chat_wire_tests::go03_openai_compatible_package_runs_a_complete_chat_tool_roundtrip`
  (real `opencode.json` with `@ai-sdk/openai-compatible`: Chat request,
  `read` tool executed, follow-up carries the assistant `tool_calls` with
  replayed `reasoning_content` and the `tool` result, final text persisted).
- Full workspace **1576 / 0 / 10**; fmt and strict workspace Clippy PASS.

## Follow-up slice — refusal and finish boundary (2026-10-05)

Base: `d66db2eaa4161ceadf7cf60c37316537242526f5`. Read-only donor comparison
(`openai-chat.ts:984–1021`) found two concrete decoder defects; this slice
does not claim full GO03 or T53 completion.

- RED: `go03_chat_refusal_is_visible` initially emitted only `Sorry: `,
  losing the provider refusal `cannot help`.
- RED: `go03_chat_reasoning_and_refusal_obey_finish_boundary` accepted the
  configured `private_thought` delta after a tool-bearing finish reason.
- GREEN: content and refusal now append in wire order to both streamed text
  and the completed ordinary message. Refusal on the finishing frame is retained.
- The same reasoning-field resolution serves decoding and the late-content
  guard: configured fields and the three existing aliases, refusal and nonempty
  reasoning-details arrays cannot follow finish or publish a completed tool batch.
  Empty frames/usage remain allowed; a later empty frame cannot replace the
  original finish reason (including explicit `length`). Reasoning-details replay
  remains outside this slice; this guard does not claim its full implementation.

Executed offline (no live generation):

- `cargo test -p oc-adapters --lib --locked go03_`: **10 passed**.
- `cargo test -p oc-adapters --lib --locked provider::`: **45 passed**,
  including Responses reconciliation, errors, limits and cancellation regressions.
- `cargo clippy -p oc-adapters --all-targets --locked -- -D warnings`: PASS.
- `cargo fmt --all -- --check` and `git diff --check`: PASS.

Cargo used `CARGO_BUILD_JOBS=3`, tests `RUST_TEST_THREADS=2`, and the pre-approved
disk-backed `TMPDIR`. Next independent risk: scope Chat reasoning replay to its
actual assistant group rather than carrying it into later unrelated responses.

## Follow-up slice — assistant-group reasoning replay (2026-10-05)

Base: `9b9482a7d` (the preceding slice was committed and pushed).

- RED: `go03_chat_reasoning_replay_is_scoped_to_its_assistant_group` demonstrated
  that reasoning from a completed tool-bearing assistant group was assigned to
  a later plain assistant response after the tool result.
- GREEN: every existing assistant-group flush also clears pending reasoning.
  Consecutive text/parallel calls in the same group retain their own reasoning;
  tool results, user/operator messages and even reasoning-only groups cannot
  lend it to the next response. Configured-field gating remains unchanged.
- Extended the real application fake-HTTP roundtrip to three main requests:
  reasoning + read → plain text + second read → final. Captured follow-up wire
  keeps both linked call/result pairs, only the first assistant's reasoning,
  and the second response's actual text. Reopened native history retains the
  turn's partial text and final answer. No production storage/raw-row mutation.

Wide-gate diagnosis and correction:

- The initial workspace run failed two unrelated R4A PTY assertions under the
  pre-approved long disk TMPDIR. A bounded synthetic excerpt identified the
  normal current-project **Location footer**, not config/credential diagnostics.
  The assertion had forbidden the entire fixture ancestor, including Location.
  It now checks the private global config, launch trap, project config/AGENTS/
  definition source paths; all existing secret/URL canaries remain forbidden.
  Product footer behavior, privacy policy and test deadlines are unchanged.
  Temporary diagnostic instrumentation was removed.
- The next full run exceeded the authoring shell's 600-second timeout after
  earlier targets passed (runtime retries and child guard tests consume real
  backoff time). No test was disabled and no product/test timeout changed.
  Re-executed with a 1200-second command allowance: full suite completed.

Final executed checks on this slice, all exit 0:

- `cargo test --workspace --locked`: **1579 passed / 0 failed / 10 ignored**.
  Existing opt-in live/internal tests remain ignored, not qualified as live PASS.
  Includes the 11 GO03 owner tests, actual-binary PTY, runtime, subagent,
  DCP/fork/restart, soak and Responses compatibility regressions.
- `cargo test -p oc --test pty_t39 --locked inherited_selection::`: **2 passed**.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `cargo build --locked`,
  `target/debug/oc --help`, `git diff --check`: PASS.

Same Cargo concurrency/disk TMPDIR as the previous slice. No live requests,
new dependencies or DCP donor changes. T53 remains active, T44 remains paused;
these two corrective Chat slices do not close R1–R6/GO01–GO06. Next wire
implementation is the thin Messages adapter/auth binding, followed by the
remaining explicit options and protocol-safe durable provenance qualification.
