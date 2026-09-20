# Fixtures — source-derived contracts for T02

Method legend (per `docs/TEST_PLAN.md`, `docs/SOURCES.md`):

- `source-derived`: small normalized excerpt from a pinned upstream file.
  Header records `source_repo`, `commit`, `path`, `license`, `sha256` of the
  fetched upstream bytes at T02 time, and `normalization` recipe.
  This is NOT a claim that the upstream suite was executed.
- `synthetic`: hand-written contract for Rust behavior, marked as such.
- `user-supplied`: exact user-provided reference (`references/`), authority
  over repository plugin.

No fixture contains secrets, live URLs with credentials, or raw payloads.
Full upstream audit is out of scope (see `roadmap/M0.md`); T02 pins only the
minimal wire/protocol/order contracts needed before porting material.

Files:

- `config-roots.order.json` — global/Location JSON/JSONC + `.opencode` order.
- `instructions.order.json` — ordered `AGENTS.md` inputs.
- `definitions.order.json` — skill/agent/command roots + frontmatter.
- `native-plugins.json` — exact native plugin identities.
- `discovery-oracle.json` — user discovery constants + oracle expectations.
- `responses-wire.json` — OpenProxy Responses wire minimum.
- `mcp-wire.json` — MCP strict version + `codex_web`/stdio minimum.
- `dcp-range-schema.json` — DCP range tool schema minimum.
