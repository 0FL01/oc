# Goal: strict OpenCode 2 key allowlist (no OpenCode 1 vocabulary)

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Historical implementation: 2026-10-16. Current source checks follow TEST_PLAN.
Source: owner request 2026-10-16: «выкинуть из код базы поддержку легаси пути и оставить
только oc2 и исправить всё остальное»; owner chose «V1-детектор → V2-allowlist».
The owner converts `~/.config/opencode/*` himself; the agent does not edit it.
Last updated: 2026-10-16
Task: T66 / M8 (D28; supersedes the report form of D27/T62 R1)

## Objective

`oc2` knows only the canonical OpenCode 2 schema. The OpenCode 1 vocabulary — key
tables, replacement hints, `LegacyConfig`/`ReplaceLegacyConfig`, the specialised
startup screen and V1-only guards — is removed. Admission stays fail-closed: every
admitted `opencode.json(c)`, `cli.json(c)` and agent/command Markdown frontmatter is
checked against a schema-derived OC2 allowlist before assembly; any key outside it
refuses startup, `/reload` (last-good kept) and `oc2 models` before provider/MCP/tool
effects with one `unknown_config_key` report listing every `scope: path` (map IDs as
`<id>`, no values, no replacements).

## Frozen Contract

### Required Outcomes

- R1: allowlist admission replaces the V1 detector.
  - Levels: root `Info` (`packages/schema/src/config.ts`) + native extensions `dcp`,
    `animations`, `terminal`; `experimental`; `compaction` and `compaction.keep`;
    `providers.<id>` (+ T65-oracle `compatibility`), `models.<id>`, `variants[]`,
    model `capabilities`/`limit`; `mcp`, `mcp.timeout`, `mcp.servers.<id>` and
    `oauth`; JSON `agents.<id>`/`commands.<id>` and Markdown frontmatter (+ `variant`
    for agents); `cli.json` sections of `packages/tui/src/config/index.tsx` and donor
    keybind names. Open records (`settings`, `body`, `headers`, `env`, `environment`,
    plugin `options`, rule objects, `compatibility` fields) are validated by their
    existing owners, not by the allowlist.
  - Primary evidence: CFG11, CFG13.
- R2: V1-only code is removed and canonical inputs still load.
  - `config/legacy.rs` and its tests, V1 `permission`/`tools` guard, the old
    `/provider/openai` discovery probe and V1 `provider` redaction alias are deleted;
    the tool→action runtime mapping (`config::legacy_key`) stays.
  - Primary evidence: CFG12 and existing canonical config/defs/MCP/permission tests.
- R3: model compatibility override equal to the Rust default is not lost.
  - Provider `maxTokensField: "max_completion_tokens"` + model `"max_tokens"` sends
    `max_tokens`.
  - Primary evidence: CFG14 regression.
- R5: the owner's converted real config starts without a catalog error.
  - Upstream models.dev now lists image-generation rows with all-zero limits, which
    rejected the whole public document (both `opencode-go` and `openai`). Such rows
    are dropped; a partial zero limit stays a malformed, atomically rejected document.
  - Primary evidence: `go02_malformed_public_rows…` and `oc2 models` on the real config.
- R4: repo `opencode.json`, examples, docs and the bundled `customize-opencode`
  runbook use and describe only OC2 forms; historical `evidence/**`/`progress/**`
  are not rewritten.

### Constraints and non-goals

No V1 migration, warn-and-ignore mode, replacement hints, config rewrite or edits to
the owner's `~/.config/opencode`. Values and secrets are never printed. The allowlist
is stricter than TS (`onExcessProperty: "ignore"`): an unknown future key fails
explicitly — a declared fail-closed divergence.
