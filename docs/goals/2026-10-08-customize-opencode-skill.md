# Goal: встроенный customize-opencode — конфигурационный ранбук native oc

Status: active — утверждённый контракт; T58 todo, implementation NOT_STARTED.
Source: запрос владельца 2026-10-08 на встроенный skill-мануал, параллельный аудит
плана и разрешение «вноси это в план правок работ и коммит пуш на текущей ветке».
Last updated: 2026-10-08
Task: T58 / M8; единственный новый detailed acceptance — SKILL01.

## Objective

Native `oc` поставляет `customize-opencode` внутри бинарника: агент обнаруживает
его metadata и явно загружает самостоятельный мануал поддерживаемой конфигурации
через существующий `skill`. В `AGENTS.md` остаётся одна краткая ссылка.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

Текущая доставка — только план. Не запускать T58, не переключать active T44 и не
создавать runtime/acceptance PASS. Исполнение — на следующем согласованном безопасном
handoff через обычный journal workflow; завершение всей T44/T45 не является dependency.

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
  - Status: pending.
  - Evidence: NOT_RUN; будущий factual report — `evidence/T58/report.md`.
- R2: компактный самостоятельный ранбук именно native-конфигурации.
  - Source: «этот скилл используется как ранбук (мануал) для конфигурации опенкода
    с помощью агента»; утверждённый исправленный план, часть 2.
  - Acceptance: один канонический Markdown описывает работающие native inputs,
    безопасный workflow и границы поддержки; рекомендации сверены с consumers,
    а approved/pending и upstream-only возможности не рекламируются как работающие.
    При стандартных tool-output limits body приходит полностью.
  - Primary evidence: reviewed asset/примеры против native consumers и полный matching
    `function_call_output` в бинарном сценарии R1, не оценка LLM-текста.
  - Status: pending.
  - Evidence: NOT_RUN; asset пока не создан.
- R3: краткая связанная документация без дублирования мануала.
  - Source: «а потом задокументировать в AGENTS.md кратко со ссылкой»;
    утверждённый исправленный план, части 2 и 5.
  - Acceptance: одна строка `AGENTS.md` честно различает pending и delivered, после
    поставки ссылается на asset. `docs/CONFIG.md` связан с ним; непосредственно
    используемые противоречащие рекомендации исправлены точечно.
  - Primary evidence: reviewed docs diff, существующие локальные ссылки и docs/journal gates.
  - Status: pending.
  - Evidence: регистрация плана не доказывает поставку мануала или завершение R3.

### Constraints and non-goals

- Existing catalog/snapshot/policy/TUI owner, immutable raw history и captured requests
  сохраняются. Skill-текст не даёт прав на global/outside-Location mutations.
- Существующие count/byte/output limits не повышать и не обходить; более строгие
  пользовательские tool-output limits остаются действующими.
- Не переносить весь upstream config reference: no JS/npm host, executable skills,
  config-authoring UI, новый loader/registry/schema/validator/installer или paid campaign.
- Старые tasks, detailed-ID owners, checkpoints и PASS-история не переписываются.

## Change Envelope and minimum slices

1. **Один asset:** `crates/oc-adapters/assets/skills/customize-opencode/SKILL.md` с
   `name: customize-opencode` и узким description для настройки самого native `oc`,
   не произвольного приложения. Минимальные разделы:
   - назначение/authority; реальные files, trust boundaries и precedence;
   - runtime JSON/JSONC, `.opencode` definitions, CLI preferences, instructions и DCP;
   - короткие native-примеры providers/models, MCP, permissions, agents/skills/commands;
   - прочитать → минимальный разрешённый diff → `/reload` на idle boundary либо
     restart → доступная диагностика; без `oc config explain/validate`;
   - различия с upstream, включая native package aliases и явный внешний MCP.
   `opencode.ai/config.json` не является полной native-схемой. `examples/oc-rs.toml`
   помечен Proposed: parser acceptance не доказывает работу настройки.
2. **Existing seam:** `defs::register_builtins` включает asset через `include_str!`,
   использует `parse_skill`, вставляет обычный `SkillDef` с `origin: builtin` и одну
   order entry. `composition` уже регистрирует builtins перед admitted roots;
   `SkillSnapshot`, `skills_input`, `tool_skill` и `skill_cards` не требуют нового пути.
   Узкая сопутствующая правка `defs::insert_skill_text`: count guard запрещает новый
   ID сверх 256, но не replacement; существующий byte accounting сохраняется.
   Malformed override диагностируется, последняя корректная entry остаётся callable;
   `autoinvoke:false` убирает preview, не явную загрузку.
3. **Existing tests:** substantive unit cases — `defs/builtin_tests.rs`; новый
   тематический `configured_workspace/customize_opencode.rs` подключить к прежнему
   Cargo target/Fixture. Достаточное покрытие и commands —
   [TEST_PLAN / SKILL01](../TEST_PLAN.md#customize-opencode--t58skill01-approved-2026-10-08-pending).
   Default body помещается в 51 200 bytes / 2000 lines, а не только SKILL_BODY_CAP.
4. **Документация:** после поставки заменить pending-строку `AGENTS.md` ссылкой на
   asset и добавить связь в `docs/CONFIG.md`. Исправить только затронутые расхождения:
   отсутствующий `oc config explain`, реально admitted CLI preferences и flat skills,
   если соответствующие разделы используются. Не начинать общий docs audit.

## Source and maintenance

Pinned donor v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa` регистрирует `opencode`
в `opencode/packages/core/src/plugin/skill.ts`; его `plugin/skill/opencode.md` — широкий
сетевой индекс docs, не точный source named `customize-opencode` из authoring environment.
Результат — native-адаптация назначения ранбука, не claim буквального upstream parity.
Attribution сохранять при заимствовании текста, без нового manifest ради учёта.
При изменении описанной поддержанной конфигурации обновлять соответствующий раздел
bundled manual в том же изменении; основные инструкции не зависят от внешних ссылок.

## Current Checkpoint / State

- План утверждён и зарегистрирован; R1–R3 pending, SKILL01 NOT_RUN, T58 todo.
- Dependency только T13; T30 дополнительно зависит от T58. TOOL11/CFG07/UI06/E2E05/
  AUD17 и root/child preview scenarios остаются у существующих owners/regressions.
- Active T44 и все прежние state/leaf references сохраняются. Pending-ссылка в
  `AGENTS.md` — только навигация, не shipped skill и не task finish.
- Первый implementation checkpoint: компактный asset и обычная builtin registration,
  replacement-safe count guard, ближайшие owner tests. Никаких runtime-изменений сейчас.

## Completion

Не завершено: R1–R3 pending. После green relevant/final gates factual report должен
связать reviewed implementation commit, команды и SKILL01 evidence, затем обычный
`finish`. Docs/journal validation и commit/push плана не являются implementation PASS.
