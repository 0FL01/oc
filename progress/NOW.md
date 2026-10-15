# NOW — актуальный handoff

State updated: 2026-10-15T10:07:57+00:00
Active: T62

Сверить Git status/diff до выполнения команд.
Task: T62 — Canonical OpenCode 2 config only (no OpenCode 1 compatibility)
Spec: docs/goals/2026-10-15-canonical-v2-config.md
Evidence target: evidence/T62/report.md

Owner decision 2026-10-15: accept only the canonical OpenCode 2 v2.0.12 config schema (packages/schema config Info + cli.json schema). Everything upstream accepts only through packages/core/src/v1/** (ConfigNormalize/ConfigMigrateV1, including markdown agent frontmatter migrated by migrateAgent) and tui.json/kv.json migration is removed. Any v1 key fails closed with one typed diagnostic listing every offending key, its source and canonical replacement, without values, on startup, catalog-only `oc2 models` and hot reload (last-good kept). One detector in config/legacy.rs; delete v1 branches in providers/permissions/agents/MCP/compaction/keybinds; add missing canonical commands/plugins/experimental.policies. Convert test fixtures, harness scripts, examples and docs to canonical v2; historical evidence unchanged. Owner converts their own config.

Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
