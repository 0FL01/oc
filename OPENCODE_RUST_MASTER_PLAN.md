# OpenCode на Rust — глобальный план для агента

## Миссия

Создать независимую нативную реализацию рабочего процесса OpenCode V2 для Linux: Rust CLI, Rust TUI, Rust agent runtime, нативные provider/MCP adapters и вручную перенесённые на Rust расширения. Поставлять один исполняемый файл на целевую архитектуру, без обязательного Bun, Node, JS/TS extension host и исходной реализации OpenCode в production.

Приоритеты: корректность и сохранность данных → предсказуемое потребление памяти → совместимость конфигурации и рабочего поведения → расширение API/UI-паритета.

Это глобальный вектор, не готовый технический дизайн. Детализировать только ближайший этап и его проверяемый результат.

## Границы продукта

Основной фронтенд — терминальный TUI. Web и графический desktop не входят в первый релиз; будущие клиенты должны переиспользовать application API, а не создавать второй backend.

Один бинарник не означает один процесс во всех режимах. Целевые режимы: локальный TUI со встроенным ядром, headless run, headless serve, TUI attach к серверу. Названия команд и default mode фиксировать отдельным решением; отличие от upstream отражать в compatibility ledger.

Внешние shell, git, MCP-серверы и инструменты проекта допускаются как явные интеграции. Если MCP-конфиг запускает npx или python, соответствующий runtime остаётся зависимостью этого MCP, но не Rust-приложения. Не переименовывать и не заменять такие команды автоматически.

Rust относится к коду приложения. Не ставить дополнительную цель «все транзитивные зависимости тоже написаны только на Rust»: в частности, допускается встроенная SQLite. Полностью статическая линковка — отдельная проверяемая задача поставки, не следствие одного ELF.

## Подтверждённая точка отсчёта

Recon выполнен 2026-09-20. Подтверждён существующий tag upstream:

- repository: anomalyco/opencode;
- tag: v2.0.10;
- commit: b8cedc1a7a5e2916bbb65dc1d4b620729c261638.

Это воспроизводимый исходный кандидат, не утверждение о самом новом или самом стабильном релизе. В фазе 0 закрепить окончательный baseline. Не смешивать исходники dev, v2, старых beta и выбранного тега. Живая документация может отличаться от тега: расхождения проверять исполняемым эталоном и тестами.

## Архитектурное направление

Модульный монолит в Cargo workspace. Разделить domain/application, runtime и adapters, TUI, HTTP API, extensions. Начать с малого числа crates; provider, tool и storage не обязаны сразу становиться отдельными crates.

Ядро не зависит от TUI и HTTP. Локальный TUI использует типизированный application interface без обязательного loopback HTTP. Remote adapter реализует те же пользовательские операции через OpenCode-compatible HTTP/event protocol. Проверки permissions, location resolution, ограничения, hooks и session transitions принадлежат application/runtime и не обходятся локальным транспортом.

Не переводить Effect/Layer/Fiber и Solid/OpenTUI механически. Переносить поведение, состояния, ошибки и lifecycle. Для TUI исходная реализация — спецификация действий и сценариев, а не дерево компонентов для буквального перевода.

Начальный технологический ориентир: Tokio; Ratatui с Crossterm; Serde и отдельный JSONC parser; SQLite; HTTP adapter на Axum; нативные provider adapters; официальный Rust MCP SDK rmcp. Фиксировать версии после небольших проверок совместимости. Не разрабатывать собственный терминальный renderer, executor или MCP protocol stack без выявленного блокера.

## Политика совместимости

Совместимость измеряется независимо по конфигам, CLI/TUI workflow, HTTP/events, tool/provider semantics и данным. Не заявлять общий процент без определённого набора сценариев.

Состояния capability: supported, supported-with-documented-difference, deferred, unsupported. Принятый парсером ключ ещё не означает работающую возможность. Неподдержанные опасные настройки, provider и необходимые плагины не должны незаметно заменяться defaults. Ошибки должны называть исходный файл, поле и причину без раскрытия секретов.

Config pipeline: discovery → JSON/JSONC → substitutions → V1/V2 normalization → domain-specific precedence/merge → canonical Rust config → capability validation. Хранить происхождение значений. Не использовать универсальный deep merge для всех доменов: серверные настройки, permissions, MCP entries, plugins и CLI settings имеют разные правила.

Сохранить имена и поддержанную семантику opencode.json/jsonc, .opencode, AGENTS.md, skills, agent/command frontmatter; отдельно реализовать глобальный cli.json и его inline overrides. Совместимость V1 покрывать нормализатором в границах выбранного V2 baseline, а не бесконечным набором догадок.

Исходные пользовательские конфиги по умолчанию только читать. Не форматировать и не мигрировать их без явного действия. Команда записи должна сохранять несвязанные поля и JSONC comments. Собственные storage, logs, credentials и service registration отделить от upstream. Не писать двум реализациям в одну SQLite DB; миграция — отдельный импорт из согласованной копии или экспортного формата.

Собственные дополнительные лимиты и режимы хранить в отдельном namespace/файле, не меняя значение upstream-полей. Изменения defaults и unsupported behavior документировать.

## Rust-расширения

Вначале расширения — доверенные Rust modules/crates, включённые в сборку, с runtime enable/disable. Новый код расширения требует пересборки; config reload не равен hot reload машинного кода.

Сделать небольшой типизированный extension API на основе нужных перенесённых плагинов: регистрация tools/commands, transformations, hooks с определённым порядком, storage namespace, initialization и shutdown. Не строить универсальный SDK до двух реальных переносов.

Для каждого портированного плагина зафиксировать исходное имя, поддержанные версии/исходный revision, options schema и behavioral fixtures. Известные старые package references могут разрешаться через явный registry в встроенный Rust-порт. Не искать совпадение только по basename и не объявлять совместимым любой npm semver. Для local TS paths нужна явная привязка либо диагностика.

Аналогично разрешать известные provider package identifiers в нативные adapters. Произвольный npm/file provider не исполнять: требовать native port или явно подтверждённую настройку поддержанного протокола.

Нативное расширение обладает полномочиями процесса. Extension API не является sandbox, а Rust сам по себе не ограничивает память плагина. В первой версии исключить динамические .so, WASM plugin platform и JS compatibility host.

## Отдельное решение: Code Mode

Upstream execute — не JS-плагин, а интерпретатор JavaScript-подобного языка для кода, написанного моделью. У MCP codemode по документации включён по умолчанию. Удаление TS-плагинов не устраняет этот компонент.

Целевое направление для полного tool parity: ограниченный Code Mode interpreter, реализованный на Rust, с проверяемым подмножеством синтаксиса и upstream fixtures. Это не требует Bun/Node, но означает поддержку интерпретируемого языка внутри Rust-приложения.

Проверить этот риск отдельным ранним spike. Сохранить permission checks вложенных tools, cancellation, async completion semantics, budgets на вычисления, память, вызовы и вывод. Не выполнять модельный код через системный node, произвольный eval или компиляцию rustc.

На ранних этапах допустим direct-tool профиль. Он должен явно заявлять отсутствие Code Mode и не интерпретировать codemode:true как false. Полный соответствующий паритет не заявлять до реализации и тестов интерпретатора.

## Этап 0 — исполняемый контракт и ограниченный recon

Зафиксировать baseline, его схемы, тесты и документацию. Составить карту модулей по поведению, перечень внешних зависимостей и каталог capabilities. Отдельно проверить config merging, default model/tool exposure, permissions, Code Mode и provider packages.

Начальные источники: packages/schema и packages/protocol; packages/core и core/src/config/*; packages/ai; packages/codemode; packages/tui; packages/cli. Source code и тесты выбранного тега важнее устаревших design notes. При заимствованиях сохранять происхождение и лицензионные уведомления.

Подготовить воспроизводимый upstream runner с изолированными HOME/XDG/DB, mock provider и тестовым репозиторием. Сохранить fixtures конфигов, сообщений, provider requests, tool results, ошибок и событий с удалёнными секретами. Для недетерминированных IDs/timestamps использовать устойчивое отображение; сравнивать причинный порядок событий, а не порядок независимых операций.

Выход: RECON.md, COMPATIBILITY.md, ARCHITECTURE.md, baseline manifest и минимальный executable test harness. Recon завершён, когда можно выбрать первый end-to-end slice; полный аудит репозитория не является условием старта.

## Этап 1 — один бинарник и первый вертикальный сценарий

Создать workspace, application interface, базовые diagnostics и изолированное storage. Одновременно поднять минимальный Rust TUI и headless run на mock provider. Цикл: ввод → поток ответа → сохранение → открытие истории → interrupt → корректный выход.

Ранний TUI обязан иметь ограниченный viewport и bounded buffers. Локальная реализация без обязательного HTTP и без незаметно запускаемого upstream daemon. Сразу добавить lifecycle counters и базовый memory measurement.

Выход: один бинарник выполняет одинаковый сценарий интерактивно и headless, восстанавливает терминал и не оставляет собственных задач/процессов.

## Этап 2 — config parity и безопасные операции

Расширить config pipeline до серверной конфигурации, CLI settings и file-based definitions. Ввести объяснение effective config с redaction. Избирательно перенести нормализацию и merge tests upstream.

Добавить Location/project semantics, permissions, read/write/edit/patch/search и shell supervisor. Запись разрешается только после проверки; ошибки parsing/permissions не превращаются в allow. Для файлов учитывать concurrent changes и не затирать пользовательские изменения молча.

Выход: поддержанный пользовательский конфиг воспроизводит ожидаемые model/agent/tool/permission selections; unsupported dependencies и поля видны до опасной операции. Базовые операции работают одинаково через TUI и headless API приложения.

## Этап 3 — рабочий agent runtime и первый provider

Добавить нативный adapter основного используемого provider protocol. Расширять по protocol families, не по количеству vendor names. Эталон — streaming text, tool calls/results, reasoning/continuation metadata, usage, errors и cancellation, а не только один успешный ответ.

Развить session state machine: pending input, последовательное владение сессией, prompts, tools, retries, compaction checkpoints, subagents и restart recovery. Нужные prompts и tool descriptions переносить осознанно: они влияют на поведение модели.

Не обещать exactly-once внешних side effects после аварии. Неизвестный результат shell/tool operation должен быть отмечен как неопределённый; не повторять команду автоматически только ради завершения transcript.

Выход: реальная coding task от начала до конца, включая редактирование, проверку, продолжение сохранённой сессии, отмену и корректную ошибку provider.

## Этап 4 — MCP, Rust-расширения и Code Mode

Подключить rmcp как клиент с transport/protocol negotiation и capability набором выбранного baseline. Перенести stdio/HTTP, catalog updates, timeouts, OAuth и credential lifecycle в нужном объёме. Discovery SDK не означает автоматического совпадения OpenCode semantics.

Добавить первые два реальных Rust-порта плагинов и проверить нужные hooks/порядок/options. Подключить native package alias registry. Реализовать Code Mode на основании решения этапа 0 и дифференциальных tests; этап допускает внутреннее разбиение, но критерии parity остаются явными.

Ресурсы MCP и расширений привязать к эффективной конфигурации и владению Location, а не к HTTP request. Не разделять stateful MCP между различными credentials/cwd ради экономии RAM. Ленивое подключение, если отличается от baseline, оформлять как объявленный режим.

Выход: MCP workflows, портированные расширения и заявленный профиль tool exposure работают без Bun/Node и без TS/JS extension host. Смена проекта, reload и shutdown освобождают ненужные ресурсы.

## Этап 5 — повседневный Rust TUI и удалённый API

Развить TUI: prompt editor, sessions/projects/models/agents, approvals/questions, compact tool cards, diff, markdown, keybindings и themes. Переносить пользовательские действия, а не Solid widgets. Обрабатывать paste, resize, Unicode, SSH/tmux и восстановление terminal state.

Реализовать HTTP/events adapter совместимости, serve и attach того же бинарника. Проверять routes, schemas, errors, pagination, auth, location и последовательность событий через upstream client. Отсоединение клиента не должно автоматически означать отмену daemon session.

Выход: пользователь может работать весь день в Rust TUI; API поддерживает опубликованный набор операций; local и remote transports не расходятся в domain invariants. Полный API-паритет оценивается отдельно, не по одному работающему экрану.

## Этап 6 — долгие сессии, миграция и поставка

Прогнать одинаковую воспроизводимую нагрузку на upstream и Rust: большие outputs, tool bursts, compaction, отмены, много завершённых сессий, смена Location, reconnect, медленный клиент, ошибки и завершение MCP. Добавить отдельно TUI scrolling/resize и headless soak.

Мерить baseline/peak/post-idle RSS и PSS, cgroup usage всего дерева, живые задачи, дочерние процессы, watchers, queued bytes, объёмы caches и DB/WAL. Нагрузка должна фиксировать размер активного контекста; рост сохранённой истории не должен автоматически вызывать линейный рост резидентного состояния. Аллокатор и RSS alone не доказывают наличие либо отсутствие leak.

Назначить численные memory/latency budgets после baseline замеров и закрепить regression gates. Не обещать произвольный объём RAM до измерений. Safety limit cgroup — последняя защита, не доказательство исправления памяти.

Сделать независимое versioned storage и безопасный importer. Проверить release в чистой Linux-среде без Bun/Node и без исходников upstream. Поставлять один ELF на целевую архитектуру, отдельно сообщая prerequisites включённых внешних инструментов.

Выход: воспроизводимая сборка, release artifact, compatibility report, memory report и пригодная для повседневной работы миграция.

## Инварианты на всех этапах

История хранится на диске; в RAM только ограниченный активный контекст и UI window. Полные tool outputs/attachments имеют отдельное хранилище и квоты. Отдельные API read/export могут материализовать большие ответы, но не должны превращать их в постоянный process-wide cache.

У очередей есть предел по байтам и максимальному размеру элемента, у caches — eviction, у tasks — owner/cancellation/shutdown, у дочерних процессов — supervision и reap. CancellationToken — сигнал, не принудительное уничтожение произвольной работы. Политика медленного потребителя не должна терять единственную копию durable результата.

UI рендерит видимое окно с ограниченным запасом; не парсит всю историю на каждый токен и не удерживает render trees всех посещённых сессий. Снижение числа DOM/terminal rows само по себе не ограничивает память backing store.

Ни одно расширение не создаёт скрытую вторую историю сообщений. Config reload заменяет нужную generation с управляемым завершением старой. Secrets не попадают в traces и fixtures.

Без подтверждённых тестов нельзя маркировать capability supported. Любое осознанное отличие от upstream фиксируется до того, как становится пользовательским поведением.

## Не начинать с этого

Не строить сразу web/desktop, свой JS runtime общего назначения, Node compatibility, динамический plugin loader, десятки crates или все provider families. Не переносить LSP и snapshot/undo из инерции: в исходном пользовательском сценарии они отключены. Не заменять полноценные tools фиктивными успешными ответами ради видимости API parity. Не откладывать первый TUI до полного backend-паритета.

## Первое задание агенту

Выполнить этап 0 в ограниченном объёме; подтвердить исходный commit и три риска: config semantics, native plugin/provider mapping и Code Mode. Создать четыре документа и минимальный baseline harness. Затем реализовать вертикальный сценарий этапа 1. Не расписывать весь проект до функций и не реализовывать оставшиеся этапы одновременно.

## Источники recon

Живые страницы документации проверены 2026-09-20; для контрактных тестов сохранить snapshot выбранной версии. Источники описывают upstream. Архитектура Rust и этапы выше — рекомендации, а не уже реализованная или измеренная система.

[S1] Upstream baseline: `https://github.com/anomalyco/opencode/tree/v2.0.10`

[S2] Config pipeline выбранного тега: `https://github.com/anomalyco/opencode/blob/v2.0.10/packages/core/src/config.ts`

[S3] API groups: `https://github.com/anomalyco/opencode/blob/v2.0.10/packages/protocol/src/api.ts`

[S4] TUI dependencies/exports: `https://github.com/anomalyco/opencode/blob/v2.0.10/packages/tui/package.json`

[S5] Code Mode interpreter: `https://github.com/anomalyco/opencode/blob/v2.0.10/packages/codemode/README.md`

[S6] Config и migration: `https://opencode.ai/v2/docs/config/` ; `https://opencode.ai/v2/docs/migrate-v1/`

[S7] CLI settings: `https://opencode.ai/v2/docs/cli/config/`

[S8] MCP semantics, merge и codemode: `https://opencode.ai/v2/docs/mcp-servers/`

[S9] Providers/packages: `https://opencode.ai/v2/docs/providers/`

[S10] Tools и permissions: `https://opencode.ai/v2/docs/tools/` ; `https://opencode.ai/v2/docs/permissions/`

[S11] Plugins lifecycle: `https://opencode.ai/v2/docs/build/plugins/`

[S12] Ratatui backends/testing: `https://ratatui.rs/concepts/backends/`

[S13] RMCP: `https://github.com/modelcontextprotocol/rust-sdk`

[S14] Tokio shutdown: `https://tokio.rs/tokio/topics/shutdown`

[S15] SQLite Rust adapter: `https://docs.rs/rusqlite/latest/rusqlite/`

