# Goal: встроенный customize-opencode — конфигурационный ранбук native oc

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete — T58/R1–R3 verified; implementation `650fa99ac`.
Source: запрос владельца 2026-10-08 на встроенный skill-мануал, параллельный аудит
плана и разрешение «вноси это в план правок работ и коммит пуш на текущей ветке».
Last updated: 2026-10-14
Task: T58 / M8; единственный новый detailed acceptance — SKILL01.

## Objective

Native `oc` поставляет `customize-opencode` внутри бинарника: агент обнаруживает
его metadata и явно загружает самостоятельный мануал поддерживаемой конфигурации
через существующий `skill`. В `AGENTS.md` остаётся одна краткая ссылка.

## Frozen Contract

### Required Outcomes

- R1: встроенная поставка через обычный native skill pipeline.
  - Source: «добавить скилл с документацией как в оригинале → Skill customize-opencode»;
    утверждённый исправленный план, часть 3.
  - Acceptance: бинарник без runtime `SKILL.md`, checkout и обязательного network fetch
    объявляет metadata; `skill({"id":"customize-opencode"})` возвращает body под обычной
    policy и captured generation. Global/project overrides сохраняют precedence,
    provenance, diagnostics и caps; automatic context не содержит body.
  - Primary evidence: реальный `defs::register_builtins` unit suite и existing-target
    `configured_workspace` fake-provider binary roundtrip по SKILL01.
  - Status: verified.
- R2: компактный самостоятельный ранбук именно native-конфигурации.
  - Source: «этот скилл используется как ранбук (мануал) для конфигурации опенкода
    с помощью агента»; утверждённый исправленный план, часть 2.
  - Acceptance: один канонический Markdown описывает работающие native inputs,
    безопасный workflow и границы поддержки; рекомендации сверены с consumers,
    а approved/pending и upstream-only возможности не рекламируются как работающие.
    При стандартных tool-output limits body приходит полностью.
  - Primary evidence: reviewed asset/примеры против native consumers и полный matching
    `function_call_output` в бинарном сценарии R1, не оценка LLM-текста.
  - Status: verified.
- R3: краткая связанная документация без дублирования мануала.
  - Source: «а потом задокументировать в AGENTS.md кратко со ссылкой»;
    утверждённый исправленный план, части 2 и 5.
  - Acceptance: одна строка `AGENTS.md` честно различает pending и delivered, после
    поставки ссылается на asset. `docs/CONFIG.md` связан с ним; непосредственно
    используемые противоречащие рекомендации исправлены точечно.
  - Primary evidence: reviewed docs diff, существующие локальные ссылки и docs/journal gates.
  - Status: verified.
### Constraints and non-goals

- Existing catalog/snapshot/policy/TUI owner, immutable raw history и captured requests
  сохраняются. Skill-текст не даёт прав на global/outside-Location mutations.
- Существующие count/byte/output limits не повышать и не обходить; более строгие
  пользовательские tool-output limits остаются действующими.
- Не переносить весь upstream config reference: no JS/npm host, executable skills,
  config-authoring UI, новый loader/registry/schema/validator/installer или paid campaign.
- Старые tasks, detailed-ID owners, checkpoints и PASS-история не переписываются.

## Source and maintenance

Pinned donor v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa` регистрирует `opencode`
в `opencode/packages/core/src/plugin/skill.ts`; его `plugin/skill/opencode.md` — широкий
сетевой индекс docs, не точный source named `customize-opencode` из authoring environment.
Результат — native-адаптация назначения ранбука, не claim буквального upstream parity.
Attribution сохранять при заимствовании текста, без нового manifest ради учёта.
При изменении описанной поддержанной конфигурации обновлять соответствующий раздел
bundled manual в том же изменении; основные инструкции не зависят от внешних ссылок.
