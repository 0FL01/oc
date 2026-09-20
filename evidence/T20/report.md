# T20 — Remote MCP codex_web

Status: PASS (fake + bounds). Live search: BUILD_READY_LIVE_BLOCKED (no credentials in this environment).

## MCP01 Remote JSON — PASS

- Exact configured URL: handshake asserts POST path equals the configured path byte-for-byte (`/v1/mcp`, no probing, no rewrite); bearer sent as `Authorization: Bearer …` (rmcp adds the prefix from the raw token); `Accept: application/json, text/event-stream`, `Content-Type: application/json`.
- `2025-11-25` enforced: `server/discover` first (fake 404 → legacy fallback), `initialize` negotiated version must equal `2025-11-25` or connect fails; `mcp-protocol-version: 2025-11-25` observed on tool calls; mismatch (`2025-06-18`) rejected.
- No session/GET dependency: stateless fake sends no `Mcp-Session-Id`, client completes handshake, list, and call; zero GET requests observed; standalone GET answered 405.
- `McpEntry → CodexWebConfig` mapping keeps the exact URL, strips one `Bearer ` prefix, applies timeout; `oauth: true`, non-remote, disabled, or bearer-less entries refused before any network use; `Debug` redacts the bearer.

## MCP02 Remote lifecycle — PASS

- JSON and SSE response paths both return tool text (`text/event-stream` `event: message` frame covered).
- 401 → `Unauthorized`, terminal with ≤2 requests (no reconnect storm); garbage handshake body → `Transport`; 60 s client timeout fires `Deadline` on a stalled call; pre-dial SSRF guard refuses loopback without the test flag and sends nothing.
- Explicit cancellation wins over a stalled `search`; nothing is retried inside the client.
- Re-list after a catalog change surfaces the new tool (default zero-TTL cache stays fresh without server TTL).

## MCP03 Registry — PASS

- Paginated `tools/list` across cursors (`search`, `fetch` + `summarize`); namespaced `{server}__{tool}` mapping; cross-server collision merges to `__2` suffix; duplicates and >32 KB schemas skipped with ids recorded; 64-tool cap; `isError: true` surfaces as `ToolFailed`, never success text.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test -p oc-adapters --test mcp_remote` 15/15 (+1 ignored live harness); workspace 127 total (oc 4 + adapters 85 + mcp_remote 15 + core 16 + tui 7); `cargo build --locked` exit 0.
- Live harness `live_search_harness` (ignored, `LUDKA2_MCP_URL`/`LUDKA2_API_KEY`) asserts non-empty catalog and search text when credentials exist.

## Scope and limitations

- Live codex_web shape (search args beyond `{query, limit?}`, catalog contents) verified only by the harness; run `cargo test -p oc-adapters --test mcp_remote live_search_harness -- --ignored --nocapture` with credentials.
- `search_args` shape `{query, limit?}` follows the fake contract; adjust to the observed server if live differs.
- Turn-loop/executor wiring of the registry arrives with the runtime consumer (T21/T22); T20 proves client, mapping, and error semantics.
