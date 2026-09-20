# OpenCode Rust

**Цель:** собрать из исходников полезный локальный `oc`: Rust 2024, модульный монолит, OpenProxy (`@ai-sdk/openai` как config alias), discovery без списка моделей в коде, DCP range/compress, configured workspace, базовые tools, MCP и TUI. Сейчас repository содержит execution contract и утилиты прогресса, но ещё не Rust-реализацию.

Основание: ответы владельца Q01–Q07, присланный discovery-plugin и обязательный configured-workspace scope. Порядок источников: новые требования владельца → `GOAL.md` и принятые контракты → pinned upstream. Технические defaults помечены в `docs/DECISIONS.md`; старые change/recon документы являются историей и provenance, не активными инструкциями.

Проверка самого пакета (ещё НЕ приёмка Rust-продукта):

```sh
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Опционально, при наличии Node в dev-среде: `node --test scripts/test_discovery_reference.mjs`. Это offline oracle tests присланного JavaScript, а не runtime-зависимость Rust-продукта.

## Что передать compatible coding agent

Передать `prompts/AGENT_GOAL.txt` любому compatible coding agent через его native prompt/task/job механизм. Не требуется конкретная модель, provider, CLI, slash command или runner tool API. Полные условия находятся в `GOAL.md` и `docs/AGENT_RUNBOOK.md`.

Первым действием агент читает `AGENTS.md`, `GOAL.md`, `progress/NOW.md` и сверяет реальный Git. Далее — только текущую задачу и нужные ей контракты. Не загружать всё дерево журнала.

Полномочия: non-root Linux account, работа в этом repo, dev tools, проверенный rootless Docker, commits и обычный push в свою рабочую ветку `origin`. Нельзя force-push, публиковать release, менять OpenProxy/хост или искать чужие credentials. Подробнее `docs/AGENT_RUNBOOK.md`.

## Настройка тестируемого продукта

`examples/opencode.jsonc` — валидный пример с `ludka`, `ludka2`, `codex_web` и выключенным браузерным MCP. `examples/oc-rs.toml` — отдельные Rust-specific настройки и limits. `examples/dcp.jsonc` — поддерживаемый range-профиль.

Секреты приходят из уже настроенной среды: `LUDKA_API_URL`, `LUDKA_API_KEY`, `LUDKA2_API_URL`, `LUDKA2_API_KEY`. `LUDKA2_API_URL` — base API URL, обычно с `/v1`; код не должен добавлять `/v1` самовольно. Не записывать ключи в markdown, Git или командные аргументы. Не требовать все четыре переменные для выключенного/неиспользуемого provider.

При отсутствии credentials compatible agent продолжает offline задачи. Заключительный live gate остаётся `BLOCKED_EXTERNAL`, а не превращается в PASS. `OC_TEST_MODEL` позволяет явно выбрать `provider/model-id` только для live-теста продукта; без него тестовый harness может выбрать подходящую опубликованную модель по детерминированному правилу из `docs/TEST_PLAN.md`. Это не выбирает модель authoring-agent и не является product default.

## Карта активных документов

- `GOAL.md` — конечный результат, критерии и статусы завершения.
- `OPENCODE_RUST_MASTER_PLAN.md` — краткий product scope и неизменяемые границы.
- `docs/ARCHITECTURE.md`, `docs/CONTRACTS.md` — устройство runtime и состояния.
- `docs/PROVIDER_OPENPROXY.md`, `docs/DCP.md`, `docs/TOOLS_MCP.md`, `docs/CONFIG.md` — рабочие контракты портов.
- `planning/tasks.json` — единственный task/dependency registry; `roadmap/M0.md` … `M6.md` содержат только phase-specific constraints.
- `planning/acceptance.json` — сценарии; `docs/TEST_PLAN.md` — общая методика.
- `docs/AGENT_RUNBOOK.md`, `docs/PROGRESS.md`, `docs/SECURITY.md` — автономная работа и восстановление.
- `docs/DECISIONS.md`, `docs/SOURCES.md` — активные решения и provenance. `docs/RECON.md`, `docs/CHANGES_V2.md`, `docs/CHANGES_V3.md` — датированная история, не execution instructions.
- `progress/` — текущее состояние и иерархический журнал; `evidence/` — маленькие отчёты, не сырые логи.

Не читать список как обязательную загрузку всех файлов после каждого compaction. Активный resume-набор ограничен; большие источники открываются по необходимости.
