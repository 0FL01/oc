# oc2 — OpenCode Rust

Нативный coding agent на Rust 2024: локальный TUI и headless sessions в одном
бинарнике `oc2`. Встроены tools, MCP, DCP range/compress, skills, темы и syntax
grammars. Для запуска не нужны исходники проекта, Node/Bun или JS-плагины.
Явно настроенные внешние MCP и вызываемые агентом dev tools устанавливаются отдельно.

## Установка и запуск

```sh
cargo install oc2 --locked
oc2 --version
oc2
```

Если установлена bootstrap-версия 0.0.1, та же команда обновит её до актуальной.
`oc2` без аргументов открывает TUI в терминале; `oc2 --help` показывает команды,
`oc2 run "задача"` запускает headless session с настроенной моделью.

Cargo собирает приложение из исходников: нужны Rust **1.93+**, C/C++ toolchain,
make и CMake. Квалифицированная платформа — GNU/Linux x86_64 (Debian 13);
отдельной qualification Fedora/macOS/Windows пока нет. После установки запускается
один native-бинарник, без отдельного сервиса и загрузки грамматик.

Конфигурация — только каноническая OpenCode 2; OpenCode 1 config отвергается с
диагностикой, автоматической миграции нет. Пример: [examples/opencode.jsonc](examples/opencode.jsonc),
подробности: [docs/CONFIG.md](docs/CONFIG.md). Provider credentials нужны для
запросов к модели, но не для `--help`/`--version`.

Версия 0.1.0 содержит приложение, а не прежнюю bootstrap-заглушку. Проект остаётся
в разработке: публикация не означает full-product READY или полный паритет с
OpenCode. Текущие результаты и ограничения — [progress/NOW.md](progress/NOW.md),
[GOAL.md](GOAL.md); owner-config/E2E06 requalification и известный VIS38 PTY flake
не исправляются этой публикацией.

Сборка из checkout:

```sh
cargo install --path crates/oc --locked --bin oc2
```

Основание: ответы владельца Q01–Q07, присланный discovery-plugin и обязательный configured-workspace scope. Порядок источников: новые требования владельца → `GOAL.md` и принятые контракты → pinned upstream. Технические defaults помечены в `docs/DECISIONS.md`; старые change/recon документы являются историей и provenance, не активными инструкциями.

Проверка execution-документации (не замена Rust/runtime-приёмки):

```sh
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Опционально, при наличии Node в dev-среде: `node --test scripts/test_discovery_reference.mjs`. Это offline oracle tests присланного JavaScript, а не runtime-зависимость Rust-продукта.

## Лицензия

Собственный код проекта распространяется по [MIT](LICENSE). Сторонние и производные
компоненты сохраняют свои лицензии, уведомления и атрибуцию; MIT их не заменяет.
Provenance DCP: [ядро](docs/DCP.md) и [представление](docs/DCP_VIS38_PROVENANCE.md).
Лицензии встроенных upstream assets и syntax grammars сохранены рядом с ними.
Пакеты с DCP и связанный executable имеют составную metadata
`MIT AND AGPL-3.0-or-later`; исходный MIT-код не перелицензирован.

## Что передать compatible coding agent

Передать `prompts/AGENT_GOAL.txt` любому compatible coding agent через его native prompt/task/job механизм. Не требуется конкретная модель, provider, CLI, slash command или runner tool API. Полные условия находятся в `GOAL.md` и `docs/AGENT_RUNBOOK.md`.

Первым действием агент читает `AGENTS.md`, `GOAL.md`, `progress/NOW.md` и сверяет реальный Git. Далее — только текущую задачу и нужные ей контракты. Не загружать всё дерево журнала.

Полномочия: non-root Linux account, работа в этом repo, dev tools, проверенный rootless Docker, commits и обычный push в свою рабочую ветку `origin`. Нельзя force-push, публиковать release, менять OpenProxy/хост или искать чужие credentials. Подробнее `docs/AGENT_RUNBOOK.md`.

## Настройка тестируемого продукта

`examples/opencode.jsonc` — валидный пример с `ludka`, `ludka2`, `codex_web` и выключенным браузерным MCP. `examples/oc-rs.toml` — отдельные Rust-specific настройки и limits. `examples/dcp.jsonc` — поддерживаемый range-профиль.

Секреты приходят из уже настроенной среды: `LUDKA_API_URL`, `LUDKA_API_KEY`, `LUDKA2_API_URL`, `LUDKA2_API_KEY`. `LUDKA2_API_URL` — base API URL, обычно с `/v1`; код не должен добавлять `/v1` самовольно. Не записывать ключи в markdown, Git или командные аргументы. Не требовать все четыре переменные для выключенного/неиспользуемого provider.

При отсутствии credentials compatible agent продолжает offline задачи. Заключительный live gate остаётся `BLOCKED_EXTERNAL`, а не превращается в PASS. `OC_TEST_MODEL` позволяет явно выбрать `provider/model-id` только для live-теста продукта; без него тестовый harness может выбрать подходящую опубликованную модель по детерминированному правилу из `docs/TEST_PLAN.md`. Это не выбирает модель authoring-agent и не является product default.

## Карта активных документов

Список точных включённых native model references: `target/debug/oc2 models` или
`target/release/oc2 models`. Команда читает admitted config/catalog без выбора
default model, открытия database, session/recovery или MCP/browser startup.
IDs идут в stdout; diagnostics в stderr, обязательный discovery failure — nonzero.
Static metadata доступна без generation key; это не проверка generation readiness.
Подробности: `docs/CONFIG.md`, receipt `evidence/T50/cli-models.md`.

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
