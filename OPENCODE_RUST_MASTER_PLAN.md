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

### Приоритетный follow-up: idle Esc не закрывает TUI (RECON 2026-09-30)

Запрос владельца: устранить дивергенцию, при которой `Esc` во время idle агента
закрывает Rust OpenCode; сначала внести RECON и план правок. **Статус: план внесён;
реализация и runtime qualification pending/NOT_RUN.** Это конкретный behavioral
bugfix этапа 5, а не разрешение расширить backend scope или объявить полный TUI parity.

#### Проверенная база и место в плане

Основная ветка `master` на момент RECON содержит глобальный план; Rust workspace и
детальные task/spec registries находятся в `agent/oc-rust-port`. Исследован code
commit [`19b224ef065496eb7de9e4c8ef8384bf46a84e3f`](https://github.com/0FL01/oc/commit/19b224ef065496eb7de9e4c8ef8384bf46a84e3f).
Незакоммиченный provider/reasoning/storage/harness diff T55 не относится к idle-Esc
и не входит в эту доставку. Не сливать всю development-ветку ради изменения плана.

В детальном плане уже есть общий keymap parity: **T44 → R5 / V05 / VIS11**, но нет
явной idle-Esc регрессии. Дополнить этот existing slice, не создавать второго owner
или нового acceptance ID. T44 остаётся PAUSED; внесение плана не снимает паузу,
не переключает активную T55 и не меняет execution statuses/исторические PASS.
Исполнение — после явного resume и безопасного single-writer scheduling handoff.

#### RECON: причина и эталон

- Native `crates/oc-tui/src/events.rs:99,146` описывает idle quit и переводит `Esc`
  в `KeyAction::Cancel`. `app/input.rs::TuiState::handle_key:2198–2266` при busy
  обрабатывает отмену; idle-ветка пытается вызвать `app.cancel(session)` и при
  `TurnNotActive` либо отсутствии session выставляет `TuiStatus::Quit`.
  `crates/oc/src/tui_cmd.rs:1109–1110,1150–1164` завершает TUI-loop по этому статусу.
  Это явный fallback обработчика, не установленный сбой terminal decoding.
- Для этого follow-up эталон — pinned **OC2 v2.0.12**, commit
  `2670273ff17da96f85c5826ced57aa1b368754fa`, уже используемый T44. Это уточнение
  именно данного bugfix, не подмена исторического v2.0.10 recon выше.
- [Keybindings](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/config/keybind.ts#L48):
  `app.exit` defaults — `ctrl+c,ctrl+d,<leader>q`; `session.interrupt` — `escape`
  (line 126), `prompt.clear` — `ctrl+c` (line 202).
- [Prompt interrupt](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/component/prompt/index.tsx#L500-L528)
  включён только при `status() === "running"`, требует focused prompt и скрытого
  autocomplete; второе нажатие в пятисекундном окне вызывает interrupt.
  Поэтому обычный idle Home/session prompt не выходит и не очищает draft по Esc.
- [App exit layer](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/app.tsx#L1233-L1240)
  отделён от interrupt и выключен для focused непустого prompt. Явный пользовательский
  remap `app.exit` на Esc отличать от стандартной привязки. Dialog/autocomplete/leader,
  формы и shell mode имеют собственные handlers; mini footer не является oracle
  обычного Home/session prompt.
- Ближайшие existing tests: `oc-tui/src/app/tests/lifecycle.rs::cancel_releases_turn`
  проверяет busy double-Esc/expiry; `app/tests/input.rs` — leader/draft и modal dismissal;
  `oc/tests/pty_t39/interaction.rs::v04_raw_dialogs_preserve_draft_and_select_normal_provider_model_variant`
  проверяет настоящий Esc в modal. Они не доказывают idle-root поведение.

#### Контракт исправления

При стандартных bindings один или несколько Esc в обычном idle Home/session prompt
не завершают приложение, не очищают draft и не создают submission/provider request.
Сохраняются cursor, paste chips, focus, session и agent/model/variant selection.
После Esc можно продолжить редактирование и отправить ровно один обычный запрос.
Это действует и после завершения/прерывания turn, а не только до первого prompt.

Сохранить ownership события: modal, autocomplete, pending leader и permission/question
UI сначала выполняют собственную state-specific обработку; событие не проходит затем
в root quit. Busy interrupt сохраняет double-Esc и пятисекундное окно; pending input,
compaction и shutdown/cancel/cleanup guarantees не ослабляются. Явный выход, включая
Ctrl+C clear-nonempty/exit-empty, Ctrl+D и leader quit, остаётся отдельным действием.
Существующую native отмену background shell из T50 не удалять заодно: её обработанная
отмена или отсутствие активной операции не должны превращаться в выход из TUI.

#### Порядок правок и done

1. На implementation HEAD повторно сверить Git/diff и routing. Добавить focused
   failing regression к `crates/oc-tui/src/app/tests/input.rs` для Home/session,
   empty/nonempty draft и повторного idle Esc. Проверять состояние редактора и отсутствие
   submit, не только enum mapping. Пользоваться existing fixtures, без нового keymap engine.
2. В `app/input.rs::handle_key` убрать idle `Cancel → Quit` fallback; в `events.rs`
   исправить комментарий. Сохранить существующие focus consumers, background-shell
   cancellation и busy/pending/compaction guards. Не менять provider routing или runtime.
3. В existing `oc` target `pty_t39`, pack `interaction.rs`, добавить raw `0x1b` сценарий:
   idle Home и открытая session, empty/nonempty prompt, repeated Esc и idle после turn;
   процесс жив, следующий edit/Enter даёт fake provider ровно один запрос с точным
   draft. Закончить явным exit и проверить terminal restoration. Escape parser timeout
   учитывать bounded ожиданием реального следующего действия, не мгновенным `try_wait`.
4. Переиспользовать nearest modal/autocomplete/leader, busy double-Esc/expiry и T50
   background-cancel tests. Выполнить targeted `cargo test -p oc-tui --lib app::`,
   `cargo test -p oc --bin oc tui_cmd::` и
   `cargo test -p oc --test pty_t39 --locked -- --test-threads=1`, затем applicable
   fmt/clippy/workspace tests/build на итоговом code commit. Никаких paid API calls.
5. Выполнить paired pinned-original/native PTY idle before/after Esc под одинаковыми
   fixtures/profile; подтвердить process/draft/selection/next-request effects и full
   styled-cell/PNG/cursor frames. Native-only green не закрывает visual VIS11; bugfix
   qualification и полный T44 parity отчёт имеют отдельные результаты.
6. При синхронизации детального development-плана внести это дополнение в
   `tui-recovery/T44_CONTRACT_AMENDMENT.md`, existing VIS11 в `ACCEPTANCE.json`, V05 в
   `IMPLEMENTATION_GUIDE.md`, R5 в `docs/goals/2026-09-21-tui-pixel-parity.md`,
   `roadmap/M9.md`, T44 `work` в `planning/tasks.json` и методику `docs/TEST_PLAN.md`.
   Эти пути принадлежат development-ветке и не являются отсутствующими local links
   в `master`. Existing statuses/evidence не переписывать ради plan delivery.

**Done для исправления:** default idle Esc не закрывает TUI, сценарий следующего ввода
проходит, затронутые lifecycle/focus/exit regressions зелёные, actual-binary evidence
приложено. RECON здесь source-derived; новых runtime/PTY PASS этим документом не заявлено.

### Приоритетный follow-up: мигание курсора ввода (RECON 2026-09-30)

Запрос владельца: в Rust-порте курсор в окне запроса — вертикальная «палка», похожая
на I/L — не мигает, хотя в оригинале мигает; явно запланировать исправление этой
дивергенции. **Статус: план внесён; реализация и runtime/visual qualification
pending/NOT_RUN.** Это caret терминального редактора, не полоска профиля, scanner
работающего агента, spinner сообщения или вкладки.

#### Проверенная база и место в плане

Основная ветка `master` содержит глобальный план; Rust workspace и детальные T44
registries находятся в `agent/oc-rust-port`. RECON сверён с code commit
[`9cbb5f2080e71b580682af142b27ade9154198f9`](https://github.com/0FL01/oc/commit/9cbb5f2080e71b580682af142b27ade9154198f9)
и актуальным незакоммиченным T50 search diff: он не меняет terminal/prompt cursor
owners и не входит в эту доставку. Не сливать development-ветку целиком и не менять
её активную задачу ради plan delivery.

В T44 уже требуется общий cursor parity, но явного контракта мигания нет. Закрепить
**T44 → R5 / V05 / VIS44 — Prompt cursor shape and blinking parity**; VIS44 свободен
в проверенном registry. При дальнейшей синхронизации повторно проверить ID, не
перезаписывать чужой case. Один existing T44 owner, без новой backend task/store
или animation framework. T44 остаётся PAUSED до явного resume и безопасного
single-writer scheduling handoff; approval не меняет execution statuses/evidence/PASS.

Для данного follow-up эталон — **OC2 v2.0.12**, commit
`2670273ff17da96f85c5826ced57aa1b368754fa`, уже используемый T44, не исторический
v2.0.10 кандидат глобального recon выше.

#### RECON: реализация и слепая зона проверки

- [Donor Cursor config](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/config/index.tsx#L57-L64)
  допускает `block`, `underline`, `line`, `default` и boolean `blinking`.
  В `:297–302` объект cursor нормализуется с defaults `block`/`true`; без объекта
  передаётся undefined. При `style:default` blinking не переопределяет terminal setting.
- [Обычный prompt](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/component/prompt/index.tsx#L1736-L1745)
  передаёт `config.cursor` в textarea. Donor `bun.lock:2249` закрепляет OpenTUI 0.5.10;
  [EditBufferRenderable](https://github.com/anomalyco/opentui/blob/v0.5.10/packages/core/src/renderables/EditBufferRenderable.ts)
  задаёт дефолт `cursorStyle: {style:"block", blinking:true}` и использует его при
  отсутствии override. Поэтому вертикальная форма пользовательского курсора не
  является универсальным дефолтом upstream. Cursor policy отдельна от `animations`.
- Native `crates/oc-tui/src/shell.rs:1654–1658` задаёт только caret position;
  `terminal.rs:50–62` и `oc/src/tui_cmd.rs:1074–1079` управляют цветом. Форма/мигание
  и соответствующая config/DTO-проекция отсутствуют. Locked Ratatui backend
  `show_cursor`/`hide_cursor` управляет видимостью, не выбирает blinking/steady style.
  Это подтверждённый implementation gap; точная причинная цепочка в пользовательском
  terminal ещё не измерена настоящим PTY-запуском, не приписывать её decoder или FPS.
- `scripts/tui_capture/bridge.py:460–464` явно задаёт оригиналу `blinking:false`;
  `frontend.js:3–6` стартует с `cursorBlink:false`, а `:87–89` сохраняет только
  `x/y/visible/shape`. Видимость здесь означает show/hide режима, не текущую фазу
  терминального мигания. Existing steady-cursor captures и schema/comparator не
  доказывают blink parity; cell `slow_blink` — атрибут текста, не caret.

#### Контракт исправления

1. В Home и session prompt воспроизвести upstream cursor shape/blinking: без настройки
   — editor default block/blinking; явный `line` — вертикальный caret с выбранным
   миганием; `blinking:false` — steady; `style:default` — terminal default без отдельного
   принудительного blink override. Сохранить также admitted underline/block варианты.
   Не захардкодить blinking bar для всех пользователей и не читать/переписывать их
   config ради green. Cursor настройка проходит через existing native config owner
   с typed validation, precedence/provenance и актуальной generation при reload/switch.
2. Использовать терминальный cursor control у existing `terminal.rs`/TUI lifecycle
   owner, а не рисовать символ I/L в styled-cell buffer. Уже используемый Crossterm
   0.29.0 имеет [SetCursorStyle](https://docs.rs/crossterm/0.29.0/crossterm/cursor/enum.SetCursorStyle.html)
   для blinking/steady block/underline/bar и default (DECSCUSR). Не добавлять UI blink
   timer, постоянные redraw/wakeup или SGR text-blink. Terminal/frontend воспроизводит
   мигание; не обещать одинаковый физический период на разных эмуляторах.
3. `animations:false` не отключает отдельно выбранное caret blinking. Сохраняются
   позиция по Unicode/display cells, цвет, draft/paste chips, selection и editor focus.
   Prompt → dialog/search → prompt меняет видимость/позицию по реальному focus owner,
   без курсора в неактивном поле или второго caret. Busy/cancel и modal routing не
   меняются. Normal exit, error и panic корректно восстанавливают terminal cursor
   state вместе с existing raw/alternate-screen cleanup; не оставляют навязанный style.

#### Порядок правок и done

1. После explicit resume повторно сверить implementation HEAD/diff, source defaults,
   focus и config route. Добавить nearest failing tests для cursor policy/ANSI output
   в `oc-tui/src/terminal/tests.rs` и для admitted config в existing owner suite;
   существенные новые tests отдельно от production, без нового crate/renderer.
2. Доставить минимальный typed config/projection + terminal-control slice; apply style
   только при реальном изменении effective policy/lifecycle, не на каждом frame.
   Проверить Home/session, default/line/steady/default-terminal, animations on/off,
   focus restore и terminal cleanup targeted tests у затронутых owners.
3. В existing `oc` target `pty_t39` проверить rebuilt actual binary с isolated config
   и fake provider: управляющие cursor sequences, idle без дополнительного ввода,
   typing/Unicode/resize, dialog round trip и explicit exit/restoration. По возможности
   использовать тот же SSH/tmux-like профиль; unsupported terminal control фиксировать
   явно, не объявлять working parity. Дополнительная provider generation для мигания
   не требуется. Переиспользовать VIS31 counters: нет новых периодических UI paints,
   writes/wakeups при idle; terminal blink не считается application redraw.
4. Расширить existing capture profile/producer/validator согласованно: записывать
   effective shape и blink policy отдельно от show/hide и наблюдаемой painted phase.
   Старые four-field dumps сохраняют своё значение, не получают fabricated blink
   metadata задним числом. Running pinned-original/native full styled-cell/PNG/cursor
   sequences на identical fixture/profile должны доказать on → off → on при неподвижном
   caret без дополнительного input; отдельно steady и focus restoration. Сопоставлять
   одинаковые наблюдаемые фазы, не произвольные wall clocks. Нельзя выключить blinking
   обеим сторонам, замаскировать cursor или заменить временное доказательство одним PNG.
5. Выполнить nearest config/terminal/TUI/PTY tests и applicable affected-crate checks,
   затем обязательные fmt/clippy/workspace tests/locked build на итоговом code commit.
   Никаких paid calls, browser/MCP campaign или обновления baseline ради этой правки.
6. При синхронизации development-плана добавить VIS44 mandatory/NOT_RUN/evidence empty
   в `tui-recovery/ACCEPTANCE.json`, pinned sources в `SOURCES.json`, контракт в
   `T44_CONTRACT_AMENDMENT.md`, шаг V05 в `IMPLEMENTATION_GUIDE.md`, методику в
   `VERIFICATION.md`/`docs/TEST_PLAN.md`, ссылки в R5 spec, `roadmap/M9.md` и T44 `work`
   в `planning/tasks.json`. Это development paths, не отсутствующие local links master;
   сохранить PAUSED, existing case statuses и исторические evidence.

**Done для исправления:** actual native caret мигает и сохраняет форму/config/focus
как pinned original в допущенном terminal profile; steady/default modes, cleanup и
затронутые regressions зелёные, paired temporal evidence приложено, нового idle work
нет. Plan commit не является runtime PASS, не завершает VIS44/T44 или весь TUI parity.

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

