# OpenCode Rust

Нативная Linux-реализация рабочего процесса OpenCode на Rust: один бинарник, TUI-first, без обязательных Bun/Node/JS/TS extension host.

## Map

- `OPENCODE_RUST_MASTER_PLAN.md` — обязательный контракт целей, границ, этапов и инвариантов проекта.

## Rules

- Перед изменениями сверяйся с планом; сначала выполняй ограниченный этап 0, затем один проверяемый вертикальный сценарий этапа 1. Не реализуй последующие этапы авансом.
- Подтверждённая исходная точка — upstream `anomalyco/opencode` tag `v2.0.10`, commit `b8cedc1a7a5e2916bbb65dc1d4b620729c261638`; baseline нужно закрепить отдельно, не смешивая версии.
- Переноси наблюдаемое поведение, состояния, ошибки и lifecycle, а не исходное дерево UI или runtime-модель JavaScript. Локальный TUI не должен требовать loopback HTTP.
- Сохраняй имена и семантику поддержанных `opencode.json/jsonc`, `.opencode`, `AGENTS.md`, skills и frontmatter. Пользовательские конфиги по умолчанию только читаются; не теряй comments и несвязанные поля при записи.
- Не смешивай собственные storage/logs/credentials с upstream и не пиши двум реализациям в одну SQLite DB. Секреты не попадают в traces и fixtures.
- Не называй capability `supported` без подтверждающих тестов. Любое отличие, deferred или unsupported-поле должно быть явно диагностировано, без незаметной подмены default.
- Все очереди, buffers и UI viewport ограничены; у tasks есть owner/cancellation/shutdown, у дочерних процессов — supervision и reap, у caches — eviction. Durable result нельзя терять из-за медленного потребителя.
- Rust extensions первой версии — доверенные встроенные модули с явным lifecycle и enable/disable; не добавляй dynamic `.so`, WASM platform или JS compatibility host.
- Code Mode — отдельный риск: direct-tool профиль обязан явно сообщать об отсутствии интерпретатора; не выполнять модельный код через Node, произвольный eval или `rustc`.
- Не подменяй полноценные tools фиктивным успехом и не заявляй API/TUI/provider parity без executable fixtures и проверок.

## Docs

- `OPENCODE_RUST_MASTER_PLAN.md` — перед началом любой реализации; источники recon и критерии выхода указаны внутри.
