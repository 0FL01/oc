# T46 — MCP attach parity: per-server degradation + outbound User-Agent

Task: T46 (phase M8). Spec: `docs/goals/2026-09-22-mcp-attach-parity.md`.
Decision: `docs/DECISIONS.md` D13. Upstream reference: anomalyco/opencode `v2.0.12`
(commit `2670273ff17da96f85c5826ced57aa1b368754fa`).

## Result

**Slice A — MCP attach failure degrades one server instead of aborting the turn.**

- `crates/oc-adapters/src/runtime.rs`: `McpGeneration` carries `degraded: Vec<RuntimeError>`
  (`McpAttach` notices only) and exposes `warnings()`; `attach_mcp` records a per-server
  failure and continues, while `Cancelled`, cleanup/`McpShutdown`, `MAX_MCP_SERVERS` and
  generation catalog caps stay fatal; `refresh_if_changed` keeps the previous catalog on a
  failed relist and clears the notice when the server recovers; `TurnReport.warnings`
  carries the sanitized notices (`mcp <id> <stage>: <code> (retryable=<bool>)`).
- `crates/oc-adapters/src/application.rs`: terminal `CoreEvent::TurnFinished`/`TurnFailed`
  carry the turn's warnings.
- `crates/oc-core/src/core_app.rs`: both variants gained `warnings: Vec<String>`.
- `crates/oc/src/tui_cmd.rs`: degradation is rendered as a synthetic transcript row
  (`(warning: …)`) after the durable page attach, so it stays visible; `crates/oc-tui/src/app.rs`
  gained `push_warning` (same synthetic-row mechanism as `(error: …)`).
- `crates/oc/src/headless.rs`: prints `warning: …` on stderr for both terminal events.
- Contract rewrites (deliberate, per D13 — not gate weakening):
  `mcp_attach_failure_is_loud` → `mcp_attach_failure_degrades_the_server`;
  `aud23_partial_attach_failure_reaps_previously_connected_child` →
  `aud23_partial_attach_failure_degrades_and_still_reaps_child` (AUD23 reaping still asserted
  via `shutdown_mcp`); soak degraded-server block; binary scenarios
  `v01_anonymous_remote_and_codex_required_auth`, `v01_required_remote_diagnostic_…` →
  `v01_degraded_remote_diagnostic_is_staged_and_redacted_in_tui`, `aud22_…`,
  `aud24_…` now assert: run answers, warning visible, no MCP/partial catalog reaches the
  model, no secret leakage. Upstream parity: `packages/core/src/mcp/index.ts` keeps per-server
  `failed`/`needs_auth` status and continues; no upstream failure kind aborts a turn.

**Slice B — outbound `User-Agent` (root cause of the crw Cloudflare 403).**

- `crates/oc-adapters/src/lib.rs`: `USER_AGENT = oc/<version>` and `WEB_USER_AGENT = oc-user/1.0`
  (upstream pattern `OpenCode-User/1.0`; JS runtimes always send a UA, `reqwest` sends none).
- Applied to the remote MCP client, provider generation client, discovery headers
  (reserved like accept/auth) and webfetch.
- Tests assert UA presence on each client: provider, discovery, webfetch unit tests and
  `tests/mcp_remote.rs`.

**Live check (real crw endpoint, no secrets printed).** Isolated config with the real
`mcp.crw` entry (templates resolved from the process env) + a local logging provider;
harness `/home/opencode/.cache/opencode-tmp/crw-live-check.py`:
`provider UA: oc/0.1.0`, `crw tools in provider request: 9`
(`crw__crw_crawl`, `crw__crw_extract`, `crw__crw_check_crawl_status`, …),
`invalid_config present: False`. The prior `mcp crw config: invalid_config` abort and the
recorded T27 conclusion “Cloudflare 1010 = browser-signature block, non-browser clients
cannot attach” are both refuted: 1010 is answered to UA-less requests; with any honest UA
the endpoint returns a valid MCP `initialize` (probe: `undici`, `rmcp/0.1`, browser UA → 200;
default `Python-urllib` UA → 403). T27 evidence leaves were not rewritten.

## Checks

- `cargo test --locked --workspace --no-fail-fast` → all binaries ok, 0 failed
  (includes 155 `oc-adapters` lib tests, 35 runtime, 4 soak, 12 mcp_application, 106 oc-tui).
- `cargo clippy --locked --workspace --all-targets -- -D warnings` → exit 0.
- `cargo fmt --all` → clean; `git diff --check` clean.
- Targeted: `mcp_attach_failure_degrades_the_server`,
  `aud23_partial_attach_failure_degrades_and_still_reaps_child`, soak, `mcp_application`
  (12 passed), `oc-adapters --lib` (155 passed), `--test mcp_remote` (23 passed).
- Journal: `scripts/progress.py reindex`/`check` → OK. `scripts/check_docs.py` still reports
  the pre-existing stale acceptance ids in T43/T44/T45 (`A02`/`A03`/`A05` are not in
  `planning/acceptance.json`); unchanged by this task.

## Risks

- Degradation is reported once per generation state: a server that stays broken warns on
  every turn of that generation (no persistent status panel yet), and a repaired server's
  notice clears only on a successful relist.
- `codex_web` is now degradable like any server: A06's live requirement (“codex_web attaches
  and serves search”) is asserted by the live campaign, no longer by refusing the turn.
- No `oc mcp list` status surface (upstream CLI parity) and no `{file:}` trim parity
  (upstream `variable.ts:79` trims; native keeps bytes verbatim) — both remain open.
- Live provider answer with crw enabled was not re-run (T27 campaign scope); only MCP attach
  and tool publication were verified.

## Next

1. Optional slice C: trim `{file:}` content to upstream semantics (`config.rs`
   `read_trusted_file`) + test.
2. Optional: `oc mcp list` / persistent status surface for degraded servers.
3. T27 live campaign re-run with `crw` enabled (codex_web mandatory) to close A06 end to end.
