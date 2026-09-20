# OpenCode Rust — пакет исполнения v3

**Цель:** собрать из исходников полезный локальный `oc`: Rust 2024, модульный монолит, OpenProxy (`@ai-sdk/openai` как config alias), discovery без списка моделей в коде, DCP range/compress, configured workspace, базовые tools, MCP и TUI. Это документация и утилиты ведения прогресса, НЕ готовая Rust-реализация.

Редакция: 2026-09-20. Основание: ответы владельца Q01–Q07, присланный discovery-plugin и обязательный configured-workspace scope. Порядок источников: новые требования владельца → `GOAL.md` и принятые контракты этого пакета → pinned upstream. Новые технические defaults помечены в `docs/DECISIONS.md`.

## Перенос в рабочую папку

Распаковать содержимое этой папки в корень отдельного checkout/worktree `0FL01/oc`. Не заменять `.git`; архив его не содержит. До копирования сохранить текущий diff. Пакет заменяет одноимённые документы предыдущей редакции, но не содержит Rust-кода и не разрешает затирать существующую реализацию.

Старые `PRO_REVIEW_PACKET.md`, `OPENCODE_GPT_PRO_REVIEW_PACKET.md`, `OPENCODE_RUST_ARCHITECTURE.md`, `prompts/AGENT_START.md`, `prompts/GPT_PRO_REVIEW.md`, `planning/execution-profile.example.json`, `planning/decisions.json`, `planning/capabilities.json`, `planning/test-catalog.json`, `review/`, `scripts/validate_plan.py` и `scripts/build_packet.py` из v1 больше НЕ являются активными инструкциями. При наличии переместить их в `archive/plan-v1/`, только если это действительно файлы старого пакета; не удалять неизвестные пользовательские данные. В v3 единственный актуальный task registry — `planning/tasks.json`. Активные документы перечислены ниже; не делать глобальный concat всех markdown.

При повторном развёртывании v3 не перезаписывать уже ведущиеся `progress/STATE.json`, checkpoint leaves и `evidence/`: стартовое состояние архива предназначено только для первого запуска. После обновления документов выполнить check/reindex и явную reconciliation графа.

Добавить правила из `examples/gitignore.fragment` в существующий `.gitignore`, не заменяя его целиком. До первого push проверить, что ключи, конфиги с секретами и `.local/` не staged.

Проверка самого пакета (ещё НЕ приёмка Rust-продукта):

```sh
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/progress.py show
```

Опционально, при наличии Node в dev-среде: `node --test scripts/test_discovery_reference.mjs`. Это offline oracle tests присланного JavaScript, а не runtime-зависимость Rust-продукта. Результаты проверки пакета: `evidence/PACKAGE_VALIDATION.md`. `SHA256SUMS` проверяет исходную поставку; после изменений агентом исходные хеши закономерно перестанут совпадать.

## Что передать Codex

Открыть Codex в этом worktree, выбрать уже настроенную владельцем GPT 5.6 Luna; параметры подключения к модели-исполнителю не менять. Вставить строку из `prompts/CODEX_GOAL.txt` как `/goal`. Полные условия находятся в `GOAL.md`, поэтому сама команда короткая.

Первым действием агент читает `AGENTS.md`, `GOAL.md`, `progress/NOW.md`, `progress/INDEX.md`. На первом старте — мастер-план и архитектуру. Далее — только текущий этап и его профильные контракты. Не загружать всё дерево журнала.

Полномочия: non-root Linux account, работа в этом repo, dev tools, проверенный rootless Docker, commits и обычный push в свою рабочую ветку `origin`. Нельзя force-push, публиковать release, менять OpenProxy/хост или искать чужие credentials. Подробнее `docs/AGENT_RUNBOOK.md`.

## Настройка тестируемого продукта

`examples/opencode.jsonc` — валидный пример с `ludka`, `ludka2`, `codex_web` и выключенным браузерным MCP. `examples/oc-rs.toml` — отдельные Rust-specific настройки и limits. `examples/dcp.jsonc` — поддерживаемый range-профиль.

Секреты приходят из уже настроенной среды: `LUDKA_API_URL`, `LUDKA_API_KEY`, `LUDKA2_API_URL`, `LUDKA2_API_KEY`. `LUDKA2_API_URL` — base API URL, обычно с `/v1`; код не должен добавлять `/v1` самовольно. Не записывать ключи в markdown, Git или командные аргументы. Не требовать все четыре переменные для выключенного/неиспользуемого provider.

При отсутствии credentials агент продолжает offline задачи. Заключительный live gate остаётся `BLOCKED_EXTERNAL`, а не превращается в PASS. `OC_TEST_MODEL` позволяет явно выбрать `provider/model-id` для live теста; без него тестовый runner может выбрать подходящую опубликованную модель по детерминированному правилу из `docs/TEST_PLAN.md`. Продукт сам незаметно модель не выбирает.

## Карта активных документов

- `GOAL.md` — конечный результат, критерии и статусы завершения.
- `OPENCODE_RUST_MASTER_PLAN.md` — scope, этапы и неизменяемые границы.
- `docs/ARCHITECTURE.md`, `docs/CONTRACTS.md` — устройство runtime и состояния.
- `docs/PROVIDER_OPENPROXY.md`, `docs/DCP.md`, `docs/TOOLS_MCP.md`, `docs/CONFIG.md` — рабочие контракты портов.
- `docs/ROADMAP.md` и `roadmap/M0.md` … `M6.md` — план и ближайшая задача.
- `docs/TEST_PLAN.md` — проверяемые сценарии и evidence.
- `docs/AGENT_RUNBOOK.md`, `docs/PROGRESS.md`, `docs/SECURITY.md` — автономная работа и восстановление.
- `docs/DECISIONS.md`, `docs/RECON.md`, `docs/SOURCES.md` — решения, границы проверки и provenance.
- `docs/CHANGES_V3.md` — минимальный diff scope/acceptance относительно v2; `docs/CHANGES_V2.md` остаётся историей.
- `progress/` — текущее состояние и иерархический журнал; `evidence/` — маленькие отчёты, не сырые логи.

Не читать список как обязательную загрузку всех файлов после каждого compaction. Активный resume-набор ограничен; большие источники открываются по необходимости.
