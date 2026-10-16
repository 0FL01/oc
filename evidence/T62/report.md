# T62 — Canonical OpenCode 2 config only

Status: DONE locally (owner decision D27, 2026-10-15). Spec:
`docs/goals/2026-10-15-canonical-v2-config.md`. CFG11–CFG13 verified on code `272ec41b4`
plus this report commit; delivery pushed to `origin/agent/oc-rust-port`.

## Commits

- `9920ac96f` docs(plan): descope shared service, register T62 canonical OC2 config
- `084ee744d` wip(config): T62 legacy detector, fail-closed gate, canonical plugins/commands/policies
- `fb0a5db35` wip(test): convert v1 config fixtures to canonical OpenCode 2 (automated pass)
- `259095b18` wip(config): canonical settings passthrough, frontmatter mapping sequences, credential paths, defs/config tests
- `e4d788c42` test(config): canonical OpenCode 2 fixtures for oc-adapters unit tests; detector precision
- `b75a8d86e` test(config): canonical OpenCode 2 integration fixtures, harnesses and docs (T62)
- `272ec41b4` refactor(config): remove OpenCode 1 parsing branches behind the T62 gate

## What exists

- `crates/oc-adapters/src/config/legacy.rs` — one table-driven detector for documents,
  Markdown agent/command frontmatter and `cli.json`. Paths are schema-only (`<id>`, no
  values). `Report` aggregates per static scope (`global`/`location`/`location .opencode`
  + file) with repeat counts, bounded to 64 lines.
- Gate: `composition::admit_sources` → `legacy_gate` (documents + cli.json; on failure also
  scans Markdown definitions) covers startup, reload and `oc2 models`; Markdown findings
  from the ordinary loader fail in `load_stages`; `config::assemble` refuses OpenCode 1
  shapes for direct callers. One `ServiceCode::LegacyConfig` diagnostic with
  `ServiceAction::ReplaceLegacyConfig`; detail lines travel in
  `ServiceDiagnostic.details` (`Option<Box<Vec<String>>>`, pointer-sized) and render on
  stderr and the TUI startup screen.
- Canonical additions: `commands`, `plugins` (string or `{package, options}`),
  `experimental.policies` `provider.use` (`ProviderPolicy`: reverse document precedence,
  last matching wildcard wins, `permission` policies refused as unsupported), YAML
  sequences of mappings in frontmatter (`permissions:` rule lists), canonical provider
  `settings` pass extra keys with a warning (donor `StructWithRest`).
- Removed OpenCode 1 branches: provider root/`npm`/`options`/model `id`/variant maps;
  `permission`/`tools`/object `permissions` (now refused, never dropped); JSON `agent`
  domain and v1 agent fields, theme-color migration; command `body`; direct `mcp.<name>`,
  `enabled`, numeric timeouts, camelCase OAuth; compaction `reserved`/
  `preserve_recent_tokens`/`prune`/`tail_turns`; underscore keybinds, `leader_timeout`,
  `keybinds`/`leader` in opencode.json; `snapshot`; `enabled_providers`/
  `disabled_providers`. `fixtures/mcp09-normalization.json` deleted.
- Security fix found on the way: `mcp::source_credential_values` read only OpenCode 1
  `provider.<id>.options`; it now protects canonical `providers.<id>.settings.apiKey`,
  `settings.headers` and provider `headers`.
- Docs: CONFIG.md T62 section with the OpenCode 1 → 2 table, SKILL.md (customize-opencode),
  PROVIDER_OPENPROXY.md, examples/opencode.jsonc (checked by `check_docs.py`), CODE_MAP.

## Checks (actual)

- Workspace `TMPDIR=$HOME/.cache/oc2-test-tmp CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2
  cargo test --locked --workspace --no-fail-fast`: 1983 passed, 1 failed, 12 existing
  ignored. The failure is `pty_t39 lifecycle::vis38_pty_dcp_equal_active_archive_resource_samples`
  (`status.success() && pty.restored()`), intermittent: it failed in the first full run of
  this session before any T62 change (`8c416102c`) and fails 1/2 even in isolation.
  Not a T62 regression; not fixed here.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`,
  `python3 scripts/check_docs.py`, `python3 -m unittest discover -s scripts -p 'test_*.py'`
  (71 OK), `cargo build --locked --release -p oc`, `target/release/oc2 --version`: pass.
- CFG11: `config::legacy::tests` (document with ~20 v1 keys, Markdown agent/command,
  cli.json, bounded report); actual debug binary `oc2 models` with a copy of the owner's
  `~/.config/opencode` (opencode.jsonc, cli.json, agents/, commands/) in a temp
  XDG root: exit 1, 15-line report, no values, refused before discovery.
- CFG12: canonical round-trips in config/defs/MCP/permission tests,
  `cfg12_provider_policies_follow_donor_last_match_with_global_last`; actual binary
  `oc2 models` on a canonical temp config (providers/settings, variants array, ordered
  permissions, mcp.servers, agents, commands, policies): exit 0, lists both models.
- CFG13: `cfg13_opencode1_key_on_reload_keeps_last_good_and_reports_every_key` (reload
  refused with the full report, catalog unchanged, canonical correction adopted);
  catalog-only refusal via `admit_sources` (same binary run above).

## Owner action

The owner's current `~/.config/opencode` is OpenCode 1-shaped and will be refused by
`oc2` until converted (keys listed above; mapping table in docs/CONFIG.md). TS
opencode 2 also reads the canonical form, so one converted config serves both.

## Not done / limits

- `Generation.permissions`/`AgentDef.permissions` scalar maps remain as internal
  summaries (now empty from config); removing them touches runtime narrowing and is
  not required for T62.
- `McpEntry.timeout` serde field kept for stored-state compatibility (always `None`).
- E2E06 real-user qualification is re-run by its owner after the owner converts the config.
