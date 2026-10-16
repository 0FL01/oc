# Goal: встроенный customize-opencode — конфигурационный ранбук native oc

Status: complete — T58/R1–R3 verified; implementation `650fa99ac`.
Source: запрос владельца 2026-10-08 на встроенный skill-мануал, параллельный аудит
плана и разрешение «вноси это в план правок работ и коммит пуш на текущей ветке».
Last updated: 2026-10-14
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

Исполнение активировано 2026-10-14 текущим явным запросом владельца продолжить и
реализовать все утверждённые планы GOAL. Безопасный journal handoff после локального
T44 commit68d3ca653 и зелёных текущих offline/release gates: обязательный push T44
заблокирован отсутствием write authentication у dedicated non-root account (SSH
и HTTPS проверены). T44 не завершена, её полный контракт сохранён; завершение всей
T44/T45 не является dependency T58. На момент активации runtime/acceptance PASS T58
не заявлялся; результаты реализации ниже не закрывают T44 или весь GOAL.

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
  - Evidence: `650fa99ac`; три owner и три actual-binary SKILL01 cases, все green
    в current-source workspace1906/0. [Factual report](../../evidence/T58/report.md).
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
  - Evidence: reviewed standalone asset15,695bytes/293lines against native consumers;
    real-binary matching complete output and stricter-user-limit proof. См. report.
- R3: краткая связанная документация без дублирования мануала.
  - Source: «а потом задокументировать в AGENTS.md кратко со ссылкой»;
    утверждённый исправленный план, части 2 и 5.
  - Acceptance: одна строка `AGENTS.md` честно различает pending и delivered, после
    поставки ссылается на asset. `docs/CONFIG.md` связан с ним; непосредственно
    используемые противоречащие рекомендации исправлены точечно.
  - Primary evidence: reviewed docs diff, существующие локальные ссылки и docs/journal gates.
  - Status: verified.
  - Evidence: reviewed AGENTS/CONFIG links and narrow corrections in `650fa99ac`;
    docs/journal checks and existing Python29/0 regressions green. См. report.

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

- R1–R3 verified: builtin asset, ordinary registration/override/policy/output pipeline
  и краткие docs delivered в reviewed implementation `650fa99ac`.
- Dependency только T13; T30 дополнительно зависит от T58. TOOL11/CFG07/UI06/E2E05/
  AUD17 и root/child preview scenarios остаются у существующих owners/regressions.
- Current-source workspace1906/0/12existingopt-inignored, affected adapters1006/0,
  targeted builtin9/0 и configured_workspace18/0; fmt/strictClippy/locked build/help
  green. Docs/journal/Python checks green; commands и scope — в factual report.
- T44 остаётся незавершённой, её paired-visual/live/delivery gates не waived.
  BLOCKED_REMOTE сохраняется: dedicated-account SSH/HTTPS write auth недоступен;
  unchanged push retry и credential fallback не выполнялись. Локальная поставка сохранена.

## Completion

Closure check 2026-10-14: все frozen R1–R3 verified, current-source required gates
green, diff остаётся в envelope; caps/authority/history/request capture не ослаблены.
`evidence/T58/report.md` связывает reviewed code commit и реально выполненные проверки.
Задача complete; общий GOAL, T44/T45/T57 и remote delivery имеют отдельные статусы.
