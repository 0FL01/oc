# oc2 — OpenCode Rust

Нативный coding agent на Rust 2024: TUI и headless sessions в одном бинарнике `oc2`.
Встроены tools, MCP, DCP range/compress, skills, темы и syntax grammars.
Для запуска не нужны checkout, Node/Bun или JS-плагины; внешние MCP и dev tools устанавливаются явно.

## Установка и запуск

```sh
cargo install oc2 --locked
oc2 --version
oc2
oc2 run "задача"
```

Нужны Rust **1.93+**, C/C++ toolchain, make и CMake. Квалифицированная платформа —
GNU/Linux x86_64 (Debian 13); Fedora/macOS/Windows отдельно не квалифицированы.
Версия 0.2.0 — приложение, не bootstrap 0.0.1; та же install-команда обновляет bootstrap.
Из checkout: `cargo install --path crates/oc --locked --bin oc2`.

`oc2` без аргументов открывает TUI; `oc2 --help` показывает команды.
TUI/run/auth/sessions автоматически используют один private same-UID native owner на data root;
отдельно устанавливаемого/управляемого `oc service` нет. Пересборка не обновляет уже работающий owner.
Проект остаётся в разработке: публикация и offline PASS не означают full-product READY или полный паритет с OC2.
Незавершённые возможности и qualification — [GOAL](GOAL.md), не автоматическая очередь задач.

## Конфигурация

Только канонический OpenCode 2 + объявленные native extensions. Ключи вне allowlist
отвергаются одним `unknown_config_key` отчётом; миграции OC1 нет.
Примеры: [opencode.jsonc](examples/opencode.jsonc), [Rust limits](examples/oc-rs.toml),
[DCP](examples/dcp.jsonc). Полный native reference — [CONFIG](docs/CONFIG.md).

- Пример содержит `ludka`, `ludka2`, `codex_web` и выключенный browser MCP.
- Keys приходят из настроенной среды: `LUDKA_API_URL`, `LUDKA_API_KEY`, `LUDKA2_API_URL`, `LUDKA2_API_KEY`.
- `LUDKA2_API_URL` — literal API base URL, обычно с `/v1`; не дописывать `/v1` автоматически.
- Не печатать/коммитить ключи и не требовать credentials для выключенного provider или `--help`/`--version`.
- `oc2 models` выводит admitted exact model references без default selection, Db/recovery и MCP/browser startup; diagnostics — stderr. Static metadata без key не доказывает generation readiness.

## Разработка

Работа начинается с конкретного запроса, а не регистрации task. Единственная обязательная
инструкция — [AGENTS](AGENTS.md): Git/diff → owner/tests → исправление → релевантные проверки → ответ.
Документация обновляется при изменении контракта; reports, журналы и closeout-коммиты не требуются.

```sh
cargo test -p oc-adapters --lib --locked <filter>
bash scripts/test.sh
```

Full gate — для integration/final checks, не каждой редакционной правки.
Prerequisites, nextest scheduling, resource budgets, bounded live и PTY safety — [TEST_PLAN](docs/TEST_PLAN.md).
Examples/source baseline checker: `python3 scripts/check_docs.py`.
Python tooling tests: `python3 -m unittest discover -s scripts -p 'test_*.py'`.
При наличии Node: `node --test scripts/test_discovery_reference.mjs` — offline oracle, не runtime dependency.

## Справочники по необходимости

- [CODE_MAP](docs/CODE_MAP.md) — поведение → owner → tests.
- [ARCHITECTURE](docs/ARCHITECTURE.md), [CONTRACTS](docs/CONTRACTS.md), [SECURITY](docs/SECURITY.md) — ownership/state/trust.
- [OpenProxy](docs/PROVIDER_OPENPROXY.md), [DCP](docs/DCP.md), [tools/MCP](docs/TOOLS_MCP.md), [CONFIG](docs/CONFIG.md) — продуктовые контракты.
- [GOAL](GOAL.md) — ограничения и открытые требования; `docs/goals/` — предметные specs, не execution commands.
- [SOURCES](docs/SOURCES.md), [DECISIONS](docs/DECISIONS.md) — provenance и design references. Старые execution-планы/отчёты доступны в Git history.
- `evidence/` и `tui-recovery/` содержат действующие fixtures/proofs/licenses/live ledgers: не удалять каталог по названию и не читать его целиком для обычного bugfix.

## Лицензия

Собственный код — [MIT](LICENSE); сторонние и производные компоненты сохраняют свои лицензии/уведомления.
DCP provenance: [ядро](docs/DCP.md), [представление](docs/DCP_VIS38_PROVENANCE.md).
Пакеты с DCP и executable имеют metadata `MIT AND AGPL-3.0-or-later`; MIT-код не перелицензирован.
Полные licenses/notices и upstream syntax assets сохраняются рядом с packages/assets, не только в Git history.
