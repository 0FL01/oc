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
