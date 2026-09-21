# Архитектура: небольшой модульный монолит

Статус: технический дизайн v3. Не утверждение о существующей реализации. Принципы владельца: Rust 2024, KISS/YAGNI/Парето. Принятые defaults и deferred — `DECISIONS.md`.

## Packages и разрешённые зависимости

```text
crates/oc-core/
  domain/{ids,session,message,tool,model,context,error}.rs
  application/{app,commands,queries,permissions}.rs
  runtime/{session_worker,turn,supervisor,context_projection}.rs
  ports/{provider,store,tools}.rs
crates/oc-adapters/
  config/                  # JSONC/AGENTS/definitions + native TOML + provenance
  storage/                 # rusqlite worker + blobs
  providers/openproxy/     # Responses wire + bounded discovery
  tools/                   # read/search/patch/bash/webfetch/skill
  mcp/                     # rmcp client, remote и stdio
  dcp/                     # range/protections/nudges/pruning
crates/oc-tui/
  app.rs, events.rs, views/, widgets/
crates/oc/
  main.rs, cli.rs, bootstrap.rs
```

`oc` связывает конкретные реализации и может зависеть от всех остальных. `oc-tui` зависит от публичного application API `oc-core`, не от storage/providers. `oc-adapters` реализует ports `oc-core`; обратной зависимости нет. `oc-core` может использовать Tokio/Serde и простые utility-типы, но не Ratatui, Axum, rusqlite, reqwest или rmcp.

DCP placement: детерминированные общие `ContextPlan`/`CompressionBlock` и application commit rules принадлежат core; upstream-specific selection/nudge/strategy policy находится в `oc-adapters::dcp`, возвращает проверяемые изменения через узкий `ContextPolicy` interface. Не создавать crate/trait на каждый тип. Подключение второго контекстного движка не входит в задачу.

Один binary target `oc`; библиотеки statically linked в приложение. Нет runtime DCP/npm loader. Без Axum в production до появления реального remote requirement; fake HTTP test servers могут быть dev-dependency.

## Composition и тестовые швы

Tokio runtime один на процесс. Application handle — cloneable typed sender/query façade. Узкие ports: Provider (request → stream), Store (короткие durable операции), ToolExecutor (descriptor/execute), ContextPolicy (projection + compression plan). Внутренняя диспетчеризация built-ins может быть enum/обычным match; DI framework и plugin service locator не нужны.

Использовать async traits только на реально подменяемых границах. Config adapter строит immutable `LocationGeneration`; ordinary enum/match по `(capability kind, exact identity, revision)` связывает DCP/OpenProxy marker с compiled module. Не создавать generic plugin trait/registry/service locator. MockProvider, fake clock и scripted tool driver нужны для unit/integration; production не содержит «успешных заглушек». Typed errors дают category/retryable/operation_id и redacted user message; произвольный `anyhow` chain не выходит в UI.

## Владение сессией

Один `SessionWorker` владеет mutable session/active turn/context generation. Session навсегда связана с одним `LocationId`; turn держит immutable `LocationGeneration` и selected-agent digest. Команды идут по bounded channel; одновременно один turn, tool mutations последовательно. Headless, TUI и MCP никогда не получают `Arc<Mutex<Everything>>` или raw DB handle. Чтение immutable session snapshots отделено от команд.

Долгий provider stream не блокирует обработку Cancel: worker select-ит stream, inbox и shutdown, а выполняемые child activities имеют owner и cancellation. Очередь новых пользовательских сообщений ограничена; сообщение принято только после durable acknowledgement. Ошибка storage не маскируется успешным UI ack.

Все tool calls, включая `compress`, сначала проходят registry → input validation → permission check → durable intent → executor → durable outcome. Порядок provider tool calls сохраняется; concurrent tools пока не нужны. Новый provider request посылается после завершения текущего tool batch. DCP не запускает скрытый второй agent loop.

## Storage без event-sourcing платформы

Одна SQLite DB, один выделенный worker thread с короткими транзакциями, WAL и `synchronous=FULL`. Версия bundled SQLite фиксируется после compile/security check. Worker никогда не ждёт сети/LLM внутри транзакции. Его inbox тоже bounded; synchronous calls не выполняются на async runtime worker.

Минимальные таблицы: schema_migrations, locations, sessions, messages/parts, turns, tool_operations, events, compression_blocks/members, prune_marks и provider_catalog metadata. Структуру уточнять миграциями текущего slice; не строить все таблицы пустыми заранее.

Persist messages/operations и небольшое durable событие в одной транзакции. События имеют monotonic seq на сессию; UI notifications после commit. Это transactional event journal, не система восстановления всей DB проигрыванием event stream. UI делtas могут coalesce; единственная durable копия terminal outcome не теряется из-за медленного потребителя.

SQLite data root отделён от upstream; advisory OS lock на время владения. Lockfile inode не удалять по PID. Второй local instance — `DataRootBusy`, не опасный «repair». DB на локальном ext4; network filesystem не квалифицирован.

Full outputs и attachments — content-addressed files в own blob dir, atomic temp/write/fsync/rename. Сначала durable blob, затем DB reference. Unreferenced files после crash безопасно собираются только собственным GC с grace period; referenced blobs не удаляются. DB errors/disc full прекращают mutations. Логи не дублируют blobs.

## История, projection и continuation

Raw history — append-only source of truth. `ContextPlan` содержит bounded references к retained ranges/summaries/tool results и protected data. Сборщик обходит план, не делает `clone()` всей истории. Активные projection entries и rendered bytes имеют предел; старые inactive compression blocks доступны из SQLite, не из process-wide cache.

DCP создаёт `CompressionBlock` с anchors/member IDs, summary, refs на вложенные blocks, protected refs, input generation и tool call ID. Commit атомарно меняет активную projection generation, не messages. Summary source — текст из аргумента tool, написанный моделью; semantic preservation не доказывается одной меньшей длиной строки. Структурные invariants и regression fixtures обязательны.

Provider continuation items (включая opaque reasoning) хранить отдельно от UI text и в пределах quotas. Они привязаны к provider/config generation/model/agent digest и causality group. Не переносить их между разными провайдерами или config/agent generations по совпавшему имени модели. В первой реализации используется явная локальная history projection с `store:false`, без зависимости от remote `previous_response_id`; не посылать старую server-side conversation цепочку после локального compress.

Один runtime prompt assembler владеет semantic lanes: compiled policy, primary-agent body, ordered AGENTS, DCP additions, history projection, user input и tool results. Config adapter/TUI не собирают финальный prompt. Command expansion остаётся durable user input; skill body — tool result. Fixed config lanes не входят в DCP compression и не дублируются между turns.

При reproject сохранять законченные call/result/reasoning группы целиком либо заменять закрытую группу summary. Активный незавершённый batch не сжимается. Корректность replay подтверждается fake wire suite и live OpenProxy; несовместимость конкретного opaque item — visible capability blocker, не strip-and-retry.

## Lifecycle

Process владеет DB worker/config/logger; `LocationGeneration` — canonical Location root/identity, permission ceiling, prompt fragments, exact native capabilities, agent/command/skill catalogs со snapshotted skill bodies, MCP clients и redacted diagnostics; Session — worker и DCP state; Turn — pinned generation, provider stream и tool activities. MCP не создаётся заново на каждый provider chunk/HTTP request. Candidate generation строится полностью и публикуется атомарно только между turns; failure сохраняет old generation. Location switch выбирает другую Location-scoped session, не перепривязывает существующую. Hot reload native machine code отсутствует.

Cancel до admission side effect = не запускать. Cancel после старта = запросить остановку, записать outcome/partial/unknown. Drop future не обещает остановить blocking work. Shell: отдельная process group, drain stdout/stderr, TERM → deadline → KILL → wait/reap. Отделившийся malicious daemon может выйти из простой group; это не kernel sandbox, containment нужен на runner/OS уровне.

Shutdown: запрет новых turns → cancel streams/tools → завершить known outcomes → drain DB → restore terminal → release lock. Panic/abrupt kill оставляет interrupted turn; startup repair не повторяет неизвестные команды. Не предлагать автоматический `git reset --hard`.

## TUI без второй модели мира

Ratatui+Crossterm; bounded viewport и lazy paging session list/history. Input buffer ограничен. Rendering не парсит весь transcript на каждый token. UI получает snapshots + live hints; при отставании перечитывает snapshot с cursor, а не держит бесконечный backlog.

Основные экраны: chat, sessions/Locations, model/variant/primary-agent picker, slash command completion, skill catalog/tool card, approvals, DCP context/stats. UI получает только redacted projection текущей generation и dispatches typed operations с expected generation; второго parser/catalog state нет. Минимальный markdown/diff/tool cards, paste/resize/Unicode и terminal restoration. Не строить собственный renderer/editor framework. При non-TTY без подкоманды — usage error; headless `run --json` выдаёт NDJSON, diagnostics отдельно stderr.

T39 wiring: `oc-tui` — чистый view-model без storage handle. Панели открываются командами, а данные приходят bounded-запросами application owner (`catalog`, `skills`, `history_page`, `tool_ops_page`, `dcp_snapshot`); выборы уходят как typed intents (`ChooseModel`/`SelectAgent`/`SwitchSession`/`Compress`) и применяются только после acceptance — effective model/variant/agent меняются для следующих turns, смена session во время turn явно отклоняется. История живёт в bounded окне (`WINDOW_ROWS`/`WINDOW_BYTES`): старые страницы выгружаются, новые запрашиваются курсором; tool cards читаются newest-first страницами, а не первыми 200 старейшими записями. Bracketed paste приходит одним событием, resize/stream не блокируют loop, а ошибка output handle завершает процесс с восстановлением терминала. `oc` бинарник владеет event loop, terminal guard и opt-in `OC_TUI_TEST_METRICS` (только для PTY-квалификации).

T40 wiring: активный контекст собирается bounded projection, а не полным чтением архива. `Db::active_history` читает только строки выше prune mark, пропуская покрытые compression-блоками (placeholders не материализуются: `project_active_rows` ставит summary по позиции блока из `Db::block_positions`/`message_seqs`), а `Db::wire_logs_for_window` переигрывает turn logs newest-first до anchor floor — pruned turns не читаются. Итоговая проекция побайтово совпадает с `project_rows` над полной историей (это проверяется в тестах и debug-assertions). Независимый byte safety budget `ACTIVE_CONTEXT_BYTES_CAP` (16 MiB) — memory guard, а не model admission: token admission по `limit.context` остаётся отдельным и выполняется по эвристике bytes/4, поэтому переполнение любого из них даёт явную ошибку (`ContextOverflow`/`OverContext`) с диагностикой, а не silent truncation. Manual `/dcp-compress` остаётся explicit owner-действием над видимым транскриптом: ranges могут адресовать pruned rows, поэтому этот путь материализует адресованную историю и не входит в per-turn hot path. Tool operation rows отдают bounded preview с явным маркером `…[+N]`, полный result остаётся единственной durable копией в `tool_operations.output` и доступен продолжением `Db::read_tool_op_output(op, offset, limit)`; UI никогда не становится единственной копией результата. TUI input budget равен core-лимиту (`MAX_INPUT_BYTES`), а вставка/набор сверх него видны в note, а не отбрасываются молча.

## Производительность и простота

Первоначальные safety caps заданы в `examples/oc-rs.toml`; это новые product defaults, не upstream defaults и не benchmark-обещание. Memory budgets квалифицируются A10. У метрик не должно быть high-cardinality labels на каждый token/message. Лог по умолчанию — metadata; payload tracing требует отдельного opt-in и никогда не включает credentials.

Не оптимизировать custom allocator, static musl, parallel workers или cache до измерений. Не заводить API server «на будущее». Первые архитектурные сигналы успеха — работающий вертикальный slice и тестируемые ownership boundaries.

## T42 compatibility wiring

Bare `oc` is the local TUI: `bootstrap` launches the same `run_tui` path as `oc tui` when
stdin *and* stdout are terminals, and otherwise exits 2 with an actionable `oc run "<prompt>"`
hint instead of any hidden headless/daemon mode. `oc tui` stays the explicit equivalent
(stdin-only gate, so an unusable stdout still fails visibly at draw time).

A Location switch happens inside one running application lifecycle: `/location <path>` sends
`InboxMsg::SwitchLocation`; the supervisor builds the complete target generation (config
load, catalog, agents/skills/commands, MCP registry, runtime, session binding) *before*
publication, then closes the old runtime's MCP resources, swaps the state and only then
answers the frontend with `LocationSnapshot`. A build failure leaves the current Location
untouched, and a switch requested while a turn streams is refused explicitly. Sessions stay
Location-bound: a first visit mints a session, a return reopens the recorded one. The
view-model drops every generation-bound cache (`TuiState::reset_workspace`) so no panel can
show the previous Location's catalog, skills, cards, sessions or DCP snapshot.

`apply_patch` tool cards render a bounded diff summary (per-file op marker, path, +/- counts,
hunk count, rename target; totals; file cap) parsed from the patch text, never a second copy
of the patch bytes. Config sources are admitted only inside the canonical root that declared
them: a symlinked `opencode.json`/`.opencode` root/`AGENTS.md` resolving outside fails closed,
while in-root symlinks remain admitted and `{file:}` stays relative to the admitted source.
