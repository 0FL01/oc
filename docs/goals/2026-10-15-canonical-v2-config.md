# Goal: canonical OpenCode 2 config only

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Historical implementation: 2026-10-15. Current source checks follow TEST_PLAN.
Source: решение владельца 2026-10-15: «совместимость с opencode 1 конфигами не нужна,
это всё под нож, по апи совместимость только с opencode 2»; выбран fail-closed отчёт
с подсказкой; владелец конвертирует свой конфиг сам.
Last updated: 2026-10-15
Task: T62 / M8; detailed acceptance CFG11–CFG13 только T62.

## Objective

`oc2` принимает только канонический schema актуального OpenCode 2: `Info` из
`packages/schema/src/config.ts` с `packages/schema/src/config/*.ts` и отдельный
`cli.json` schema (`packages/cli/src/config/schema.ts`). Всё, что upstream принимает
только через `packages/core/src/v1/**` (`ConfigNormalize` в `core/src/config/normalize.ts`,
`ConfigMigrateV1` в `core/src/v1/config/migrate.ts`, включая Markdown frontmatter агентов,
который `core/src/config/plugin/agent.ts` отправляет в `migrateAgent`) и миграция
`tui.json`/`kv.json` → `cli.json`, удаляется. Встретив такой ключ, `oc2` не загружает
конфиг и показывает один отчёт со всеми ключами, источником и канонической заменой.

## Frozen Contract

### Required Outcomes

- R1: единый fail-closed детектор OpenCode 1 ключей.
  - Acceptance: все admitted documents, definitions и `cli.json` сканируются до
    assemble; любой v1 ключ отказывает startup/reload/`oc2 models` до provider/MCP/tool
    effects одним typed `LegacyConfig` диагнозом: полный детерминированный список
    `source: path → replacement`, без значений. Reload сохраняет last-good (T61 R2).
  - Primary evidence: CFG11, CFG13.
  - Status: done (`084ee744d`, `e4d788c42`, `272ec41b4`).
- R2: v1-ветки удалены, канонические замены поддержаны.
  - Acceptance: providers/permissions/agents/MCP/compaction/keybinds парсят только
    канонические формы; добавлены `commands`, `plugins`, `experimental.policies`
    (provider.use, donor last-match). Внутренние built-in профили используют
    `permissions`. Runtime-маппинг tool → permission action (`config::legacy_key`)
    не является config compatibility и сохраняется.
  - Primary evidence: CFG12 и существующие config/defs/MCP/permission регрессии на
    канонических фикстурах.
  - Status: done (`272ec41b4`).
- R3: фикстуры, harness, examples и docs только в канонической форме.
  - Acceptance: test fixtures, `scripts/**` генераторы конфигов, `examples/*.jsonc`,
    корневой `opencode.json`, CONFIG/SKILL/README/PROVIDER/DCP docs используют v2 формы;
    CONFIG содержит таблицу v1 → v2, совпадающую с детектором. `evidence/**` и
    `progress/**` — история, не переписываются.
  - Status: done (`b75a8d86e`).

### Decisions from the pinned donor

- Канон: каталоги `agent/agents` (рекурсивно), `mode/modes` (primary, один уровень),
  `command/commands` (`plugin/command.ts`); они вне `v1/**`. Frontmatter агента —
  поля `ConfigAgent.Info` + `variant`, тело → `system`. Команда — `template`,
  `description`, `agent`, `model`, `subagent`, `subtask` (канонический deprecated alias).
- v1 (отказ): top-level `agent`, `mode`, `provider`, `tools`, `permission`, `command`,
  `plugin`, `autoupdate`, `autoshare`, `snapshot`, `attachment`, `reference`,
  `small_model`, `enabled_providers`, `disabled_providers`, прямые `mcp.<name>`,
  `experimental.mcp_timeout`; compaction `preserve_recent_tokens`/`reserved`/`prune`/
  `tail_turns`; provider `npm`/`options`/`api`/`baseURL`/`id`, вложенный `provider`,
  `tool_call`/`modalities`/`interleaved`/`status`, variants-map; MCP `enabled`, numeric
  timeout, camelCase oauth и `oauth: true`; agent `tools`/`permission`/`prompt`/
  `temperature`/`top_p`/`options`/`maxSteps`/`disable`/`name`/`body`, theme-name color;
  `permissions` как объект; действия `write`/`patch`/`bash`/`task`/`apply_patch` в
  правилах (→ `edit`/`shell`/`subagent`); `skills: {paths, urls}`; underscore keybinds и
  `leader_timeout` в `cli.json`.
- Native extensions (`dcp`, `animations`, `terminal.copy`, native profile) — не v1.
- Отчёт: ID и пути только в stderr/startup screen; DB/trace — redacted projection.

### Constraints and non-goals

Нет `oc2 config migrate`, автоматической перезаписи пользовательских файлов, warn-and-
ignore режима или скрытого fallback. Trust-before-substitution, secret redaction,
CFG04/CFG10 safety и T61 last-good сохраняются. Четыре crates, без нового framework.
