# NOW — актуальный handoff

State updated: 2026-09-21T20:35:34+00:00
Active: T27

Сверить Git status/diff до выполнения команд.
Task: T27 — Live пользовательский workflow
Spec: roadmap/M5.md
Evidence target: evidence/T27/report.md

Явно opt-in выполнить готовый bounded live campaign: coding без edits вне fixture, reopen/next command, compress/resume, webfetch и codex_web search с counters/watchdog; optional browser smoke only explicit opt-in. Product/harness code здесь не добавлять; report remaining models unqualified.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Owner startup blockers: two fixed, one remains (config-side).

FIXED
- `dcp.experimental.allowSubAgents: true` no longer blocks; it is accepted with a visible
  warning until subagent support lands (`customPrompts: true` stays deferred/rejected).
- `model required` is actionable: the error lists the configured models (bounded) and the
  repo Location now has `opencode.json` with `model: ludka2/ocg/muse-spark-1.3-contributor`.
- Unknown provider option keys (`authToken`) are warnings, and a foreign `npm` only fails for
  the *selected* provider, so another frontend's provider entry cannot block startup.

REMAINING (owner config): `mcp.crw` cannot attach from this client. Direct probe:
`POST https://crw.bash8.de/mcp` with the configured `Bearer {env:CRW_API_KEY}` returns
Cloudflare `403 Error 1010` (browser-signature block) for a non-browser HTTP client, while
`mcp.codex_web` on ludka2 answers `initialize` in 0.03 s. Attach failure is fatal by the
existing contract (`mcp_attach_failure_is_loud`, `aud23_partial_attach_failure_reaps_...`),
so the run stops with `error: application: mcp attach failed for crw`. Disable `crw`
(`"enabled": false`) or move it to a non-Cloudflare endpoint; a browser-compatible client is
a separate, larger change.

Live path verified end to end on the production binary with the real credentials when the
crw server is disabled: `oc run` completes and answers (`session s-...` + `pong`) against
`ludka2` / `ocg/muse-spark-1.3-contributor`. `POST {LUDKA2_API_URL}/responses` returns 200
for both documented models; `/models` returns 40 ids.

REVERTED: an attempted "unreachable MCP degrades to a warning" change contradicted the
existing T37 contract tests (AUD23 reaping + loud failure) and was rolled back; the
workspace is green again.

## Checks

`cargo test --locked --workspace --no-fail-fast` -> 333 passed / 0 failed / 4 ignored (exit
0). `cargo clippy --locked --workspace --all-targets -- -D warnings` -> exit 0. `cargo fmt`
clean. `cargo build --locked --release` rebuilt. Live probes as above (no secrets printed).

## Risks

- The crw MCP server is unusable for this client until the config or the endpoint changes;
  with it enabled, every `oc` command that loads the Location fails fast.
- T27 (mandatory live campaign) still has no PASS: the harness must accept the discovered
  `ludka2` model and the campaign must run with `codex_web` (mandatory) and `crw` disabled.

## Next

1. Owner: set `"enabled": false` for `mcp.crw` (or repoint it) in
   `~/.config/opencode/opencode.jsonc`.
2. Then T27: harness model acceptance for discovery providers, live campaign with codex_web,
   summary in `evidence/T27/`; then T30 FINAL over A01-A13.


Ready (до 5): T30
Blocked: нет

Done в журнале не означает READY всего продукта; см. GOAL.md.
