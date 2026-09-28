# Архитектура: небольшой модульный монолит

Статус: технический дизайн v3. Не утверждение о существующей реализации. Принципы владельца: Rust 2024, KISS/YAGNI/Парето. Принятые defaults и deferred — `DECISIONS.md`.

## Packages и разрешённые зависимости

```text
crates/oc-core/src/
  application.rs, core_app.rs, queries.rs, ports.rs, domain.rs, ...
crates/oc-adapters/src/
  application.rs, application_*.rs  # application lifecycle, workers, queries
  runtime.rs, runtime/             # native turn/tool/MCP/context execution
  storage.rs, storage_*.rs         # Db owner, transactions, history, DCP
  config.rs, composition.rs, defs.rs, discovery.rs, provider.rs, ...
crates/oc-tui/src/
  app.rs, app/                     # TuiState owner + coarse implementation slices
  messages.rs, shell.rs            # transcript rendering / frame composition
  history.rs, styled.rs, editor.rs, tools.rs, ...
crates/oc/src/
  main.rs, cli.rs, bootstrap.rs, headless.rs, tui_cmd.rs
# <owner>/tests.rs и tests/<suite>/ — тесты, не production-зависимости.
```

Это карта укрупнённых владельцев, не предписание создать все возможные подпапки. `app/` и дополнительные slices `runtime/` уже содержат implementation/test parts с прежними state owners; реальные маршруты и ближайшие tests — в `CODE_MAP.md`. Не раскладывать небольшие core/config/provider-модули по старому концептуальному дереву ради единообразия.

`oc` связывает конкретные реализации и может зависеть от всех остальных. `oc-tui` использует публичный application API `oc-core` и уже разрешённый D12 read-side `oc-adapters`; эта зависимость есть в Cargo.toml и не устраняется файловым рефакторингом. В текущем TuiState snapshots/intents проходят через CoreApp, а lifecycle Db/runtime/network/process остаётся у существующих application/binary owners. T52 не добавляет UI новый I/O и не расширяет исключение D12. `oc-adapters` реализует ports `oc-core`; обратной зависимости нет. `oc-core` может использовать Tokio/Serde и простые utility-типы, но не Ratatui, Axum, rusqlite, reqwest или rmcp.

DCP placement: детерминированные общие `ContextPlan`/`CompressionBlock` и application commit rules принадлежат core; upstream-specific selection/nudge/strategy policy находится в `oc-adapters::dcp`, возвращает проверяемые изменения через узкий `ContextPolicy` interface. Не создавать crate/trait на каждый тип. Подключение второго контекстного движка не входит в задачу.

Один binary target `oc`; библиотеки statically linked в приложение. Нет runtime DCP/npm loader. Без Axum в production до появления реального remote requirement; fake HTTP test servers могут быть dev-dependency.

## Файлы и тесты

Единица навигации — владелец поведения плюс несколько укрупнённых implementation/test slices. Лимит 5 000 физических строк мягкий и действует на отдельные рукописные production/test/dev-tool файлы; строки комментариев/пустые строки учитываются. Размер модуля со всеми дочерними файлами не ограничен этим числом. Ориентир 1–4k полезен для новых крупных slices, но не является минимумом: фасады, ports и существующие самостоятельные небольшие модули не укрупнять искусственно.

Сначала отделять большие тестовые блоки, затем повторно измерять production. Согласованный файл на 3k строк не дробить только потому, что до выноса тестов он занимал 8k. Выше 5k — warning и объяснение: какие обязанности связаны, почему выбранный перенос опаснее сохранения, следующий шов/условие пересмотра. Запись исключения держать рядом с владельцем в `CODE_MAP.md`, не в отдельной системе waiver/хешей. Уплотнение строк и монолит за `include!` не являются исправлением.

При сохранении публичного `app.rs` Rust ищет его обычные дочерние модули в `app/`; одновременно `app.rs` и `app/mod.rs` не создавать. Корневой owner сохраняет тип состояния и инварианты; тематические файлы могут содержать `impl TuiState`/`impl Runtime`. Не создавать новые копии состояния, `Arc<Mutex<Everything>>`, traits, crates или dispatcher framework ради переноса методов. Private поля предка доступны его потомкам; moved helpers, вызываемые родителем/соседями, получают только необходимую scoped visibility. Старые публичные пути сохранять явными re-exports, без общего `pub use *`.

Unit tests остаются дочерними модулями владельца, но их код читается отдельно:

```rust
// src/<owner>.rs
#[cfg(test)]
mod tests; // src/<owner>/tests.rs; при необходимости содержит mod input; и т.п.
```

Fixtures общего unit-suite можно оставить в его `tests.rs`, сценарии — в нескольких крупных файлах `tests/<topic>.rs`. Мелкие существующие inline-тесты допустимы; новую крупную сценарную серию не наращивать в production-файле. `#[cfg(test)]` поля, probes и test/non-test branches не путать с целым тестовым блоком и не удалять механически. Уже публичный ScriptDriver не превращать в cfg(test)-only API только из-за переноса: чистая раскладка сохраняет доступность, даже если найденные call sites — unit tests.

Для существующего integration target сохранять `tests/<suite>.rs`; подключать части явными `#[path = "<suite>/<topic>.rs"] mod topic;`. Не добавлять одновременно `<suite>/main.rs`, второй target того же имени или top-level `tests/common.rs`. Shared fixtures остаются внутри suite либо в существующем `tests/support/`, без нового dev-support crate. Отдельный test-файл не означает отдельный Cargo target. За переносом следуют сверка test discovery/ignored и коррекция живых filters, `include_str!`/`include_bytes!`/`#[path]`; публичный API ради доступа тестов не расширять.

Правила относятся к раскладке, а не к переутверждению поведения. Исторические evidence и frozen upstream сохраняются. Новый размерный отчёт — advisory; compile/test failures, нарушение ownership и потеря тестов остаются настоящими failures. Подробный первый проход: `goals/2026-09-28-code-slices.md`.

## Composition и тестовые швы

Tokio runtime один на процесс. Application handle — cloneable typed sender/query façade. Узкие ports: Provider (request → stream), Store (короткие durable операции), ToolExecutor (descriptor/execute), ContextPolicy (projection + compression plan). Внутренняя диспетчеризация built-ins может быть enum/обычным match; DI framework и plugin service locator не нужны.

Использовать async traits только на реально подменяемых границах. Config adapter строит immutable `LocationGeneration`; ordinary enum/match по `(capability kind, exact identity, revision)` связывает DCP/OpenProxy marker с compiled module. Не создавать generic plugin trait/registry/service locator. MockProvider, fake clock и scripted tool driver нужны для unit/integration; production не содержит «успешных заглушек». Typed errors дают category/retryable/operation_id и redacted user message; произвольный `anyhow` chain не выходит в UI.

## Владение сессией

Один execution owner на session владеет mutable session/active turn/context generation. Session имеет текущий `LocationId`; T50 допускает только явный durable admitted move той же session на safe boundary, superseding прежнюю пожизненную привязку. Turn держит immutable `LocationGeneration` и selected-agent digest: исходный turn terminal под pinned source generation; destination request принадлежит отдельному последующему turn, не in-flight retarget. Команды идут по bounded channel; одновременно один turn на session, не один на всю семью/Location. T45/R3 требует параллельных независимых foreground-детей и background progress до конца parent turn; existing supervisor owns bounded jobs/cancellation/delivery, без нового scheduler service. Tool mutations сохраняют ordering/CAS/path protections, а parent-held lease не сериализует все child sessions. Headless, TUI и MCP никогда не получают `Arc<Mutex<Everything>>` или raw DB handle. Чтение immutable session snapshots отделено от команд.

Долгий provider stream не блокирует обработку Cancel: worker select-ит stream, inbox и shutdown, а выполняемые child activities имеют owner и cancellation. Очередь новых пользовательских сообщений ограничена; сообщение принято только после durable acknowledgement. Ошибка storage не маскируется успешным UI ack.

Все tool calls, включая `compress`, сначала проходят registry → input validation → permission check → durable intent → executor → durable outcome. Admission/causality IDs и объявленный mutation order сохраняются; независимые subagent calls могут исполняться одновременно. Новый provider request следует после terminal foreground batch; background call возвращает running, не ждёт terminal ребёнка. Завершение доставляется durable notice через parent owner, без polling, не теряется при занятом parent. Явно запущенные child loops не являются скрытым DCP summarizer: compress не вызывает вторую модель.

T50 selected native tool target is defined in [its frozen contract](goals/2026-09-27-native-tool-parity.md).
Canonical shell(command/workdir/timeout/background) and legacy bash(argv) reuse one
policy/supervisor, with no duplicate advertised shell or alias-based authority gain.
Background shell jobs retain bounded owned capture/cancel/teardown/durable notices;
unset/zero execution timeout does not make retention or shutdown unbounded. Question
uses typed application-owned pending/answer/cancel identities, not permission replies
or a held database transaction. Direct opencode_* tools reuse catalog/session owners,
without Code Mode/execute, websearch integration, built-in browser or a second registry.
Read images use the existing provider media boundary, not base64 text pretending to
be an image. Search/conversion engines stay bounded and pinned; PDF remains excluded.

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

Один runtime prompt assembler владеет semantic lanes: compiled policy, selected session-agent body/base prompt (primary или child), environment/date, ordered AGENTS/skill metadata/permitted MCP, DCP additions, history projection, user input и tool results. Config adapter/TUI не собирают финальный prompt. Command expansion и delegation/context pack остаются durable user input; skill body — tool result. Fixed config lanes не входят в DCP compression и не дублируются между turns.

Target architecture under owner-approved [T45/R7](goals/2026-09-21-config-compat-and-subagents.md#environmentcontext-references--r7), not a current implementation claim: host/workspace environment is a compiled harness instruction layer, separate from the selected agent system/body and ordered AGENTS.md. A custom agent system replaces the base harness prompt but never suppresses environment/date. The missing base harness fallback is an explicit implementation requirement.

Request context order: agent system/base harness prompt → host/workspace environment + date → ordered workspace instructions and skill metadata → permitted MCP guidance and DCP additions → history projection → current user input and tool results. Tool descriptions/schemas remain a separate provider request field. Root and child requests share the assembler; guidance describes actual native tools.

T45/R8–R10 target: initial instruction baseline is durable; safe-boundary updates/removals and successful-read nested AGENTS are chronological messages with origin/revision, not rewrites of raw history. Reconcile after compaction/Revert/reopen within admitted source boundaries. A new child gets its own profile/context, not parent transcript/system; optional context_message_ids adds exact selected parent text/roles as escaped quoted user context alongside the task. Resolve branch/revision/cutoff/unsupported content/budgets before admission and persist one snapshot/digest; continuation admits its next task/pack once. Candidate IDs are bounded and independent of DCP. Guidance and effective capability preview distinguish automatic AGENTS/tools/skill metadata from supplied task/history, never claim denied tools or automatic skill bodies.

Linux host facts are collected natively with ordinary user permissions, without shell subprocesses or privilege escalation. Runtime metadata reflects the actual tool executor, not the inherited $SHELL. Render selected bounded fields deterministically, escaping control characters/block delimiters; unavailable optional facts are unknown/omitted, not startup failures. Preserve one immutable snapshot per request and refresh the relevant context on restart/Location change without accumulating duplicates. The environment block describes execution facts, not an access grant or sandbox assertion. Do not inject raw environment/proc dumps or changing resource/toolchain inventories.

При reproject сохранять законченные call/result/reasoning группы целиком либо заменять закрытую группу summary. Активный незавершённый batch не сжимается. Корректность replay подтверждается fake wire suite и live OpenProxy; несовместимость конкретного opaque item — visible capability blocker, не strip-and-retry.

## Lifecycle

Process владеет DB worker/config/logger; `LocationGeneration` — canonical Location root/identity, permission ceiling, prompt fragments, exact native capabilities, agent/command/skill catalogs со snapshotted skill bodies, MCP clients и redacted diagnostics; Session — worker и DCP state; Turn — pinned generation, provider stream и tool activities. MCP не создаётся заново на каждый provider chunk/HTTP request. Candidate generation строится полностью и публикуется атомарно только между turns; failure сохраняет old generation. Обычный Location switch выбирает другую Location-scoped session; T50 explicit move меняет placement той же session только после source-turn boundary. Hot reload native machine code отсутствует.

T50 move admission validates caller/session access and destination existence/type/
trust before building a complete target generation. Persist admitted versus applied
placement and retain ID/history/DCP; next execution refreshes environment/instructions/
catalogs and provider causality without stale source-local rules or opaque continuation.
Original operations/background jobs/children keep execution Location/generation and
provenance; the move is not family migration or a permission grant. Do not close a
generation still owned by admitted work or accumulate an unbounded retired-generation
cache. Sticky unknown-effect MCP quarantine/cleanup survives transitions. Crash/restart
deduplicates committed move/result delivery, never replays unknown shell/MCP/mutations.

Cancel до admission side effect = не запускать. Cancel после старта = запросить остановку, записать outcome/partial/unknown. Drop future не обещает остановить blocking work. Shell: отдельная process group, drain stdout/stderr, TERM → deadline → KILL → wait/reap. Отделившийся malicious daemon может выйти из простой group; это не kernel sandbox, containment нужен на runner/OS уровне.

Shutdown: запрет новых turns → cancel streams/tools → завершить known outcomes → drain DB → restore terminal → release lock. Panic/abrupt kill оставляет interrupted turn; startup repair не повторяет неизвестные команды. Не предлагать автоматический `git reset --hard`.

T45 background recovery validates durable parent/child/job identity and deduplicates
terminal notice delivery. Already committed results are delivered without execution;
eligible work may continue within pinned donor recovery bounds. Donor execution is
at-least-once, not exactly-once; native started/unknown mutation/shell/MCP effects
remain blocked from automatic replay. Successful parent completion is not child
cancellation. Shutdown still joins all owned work; no orphan background tasks.

## TUI без второй модели мира

Ratatui+Crossterm; bounded viewport и lazy paging session list/history. Input buffer ограничен. Rendering не парсит весь transcript на каждый token. UI получает snapshots + live hints; при отставании перечитывает snapshot с cursor, а не держит бесконечный backlog.

Основные экраны: chat, sessions/Locations, model/variant/primary-agent picker, slash command completion, skill catalog/tool card, approvals, DCP context/stats. UI получает только redacted projection текущей generation и dispatches typed operations с expected generation; второго parser/catalog state нет. Минимальный markdown/diff/tool cards, paste/resize/Unicode и terminal restoration. Не строить собственный renderer/editor framework. При non-TTY без подкоманды — usage error; headless `run --json` выдаёт NDJSON, diagnostics отдельно stderr.

T39 wiring: `oc-tui` — чистый view-model без storage handle. Панели открываются командами, а данные приходят bounded-запросами application owner (`catalog`, `skills`, `history_page`, `tool_ops_page`, `dcp_snapshot`); выборы уходят как typed intents (`ChooseModel`/`SelectAgent`/`SwitchSession`/`Compress`) и применяются только после acceptance — effective model/variant/agent меняются для следующих turns, смена session во время turn явно отклоняется. История живёт в bounded окне (`WINDOW_ROWS`/`WINDOW_BYTES`): старые страницы выгружаются, новые запрашиваются курсором; tool cards читаются newest-first страницами, а не первыми 200 старейшими записями. Bracketed paste приходит одним событием, resize/stream не блокируют loop, а ошибка output handle завершает процесс с восстановлением терминала. `oc` бинарник владеет event loop, terminal guard и opt-in `OC_TUI_TEST_METRICS` (только для PTY-квалификации).

T40 wiring: активный контекст собирается bounded projection, а не полным чтением архива. `Db::active_history` читает только строки выше prune mark, пропуская покрытые compression-блоками (placeholders не материализуются: `project_active_rows` ставит summary по позиции блока из `Db::block_positions`/`message_seqs`), а `Db::wire_logs_for_window` переигрывает turn logs newest-first до anchor floor — pruned turns не читаются. Итоговая проекция побайтово совпадает с `project_rows` над полной историей (это проверяется в тестах и debug-assertions). Независимый byte safety budget `ACTIVE_CONTEXT_BYTES_CAP` (16 MiB) — memory guard, а не model admission: token admission по `limit.context` остаётся отдельным и выполняется по эвристике bytes/4, поэтому переполнение любого из них даёт явную ошибку (`ContextOverflow`/`OverContext`) с диагностикой, а не silent truncation. Manual `/dcp-compress` остаётся explicit owner-действием над видимым транскриптом: ranges могут адресовать pruned rows, поэтому этот путь материализует адресованную историю и не входит в per-turn hot path. Tool operation rows отдают bounded preview с явным маркером `…[+N]`, полный result остаётся единственной durable копией в `tool_operations.output` и доступен продолжением `Db::read_tool_op_output(op, offset, limit)`; UI никогда не становится единственной копией результата. TUI input budget равен core-лимиту (`MAX_INPUT_BYTES`), а вставка/набор сверх него видны в note, а не отбрасываются молча.

## Производительность и простота

T45/R9 extends T40 with repeated-compression qualification: no lifetime compress-call
or block-count quota. Keep durable archive/stable IDs but load only active/addressed
blocks and required dependencies; normalize repeated summaries transactionally so
age does not become depth exhaustion. Bounds apply to active traversal/payload/model
budget, not cumulative operations. Per-session active delegation task/pack protection
is released on terminal completion and reinstated for safe recovery, never permanent
fixed-lane growth. T36/T40 historical evidence does not prove this pending outcome.

Первоначальные safety caps заданы в `examples/oc-rs.toml`; это новые product defaults, не upstream defaults и не benchmark-обещание. Memory budgets квалифицируются A10. У метрик не должно быть high-cardinality labels на каждый token/message. Лог по умолчанию — metadata; payload tracing требует отдельного opt-in и никогда не включает credentials.

Не оптимизировать custom allocator, static musl или speculative workers/cache до измерений; owner-required child concurrency не является speculative optimisation. Не заводить API server «на будущее». Первые архитектурные сигналы успеха — работающий вертикальный slice и тестируемые ownership boundaries.

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

The paragraph above records T42 SelectLocationSession wiring, not a prohibition of
T50's pending explicit same-session move. Ordinary UI switch semantics remain;
permanent session binding alone is superseded by the admitted safe-boundary operation.
Keep old operation provenance visible after move/reopen; do not rewrite history to
make old tools appear to have executed in the destination workspace.

`apply_patch` tool cards render a bounded diff summary (per-file op marker, path, +/- counts,
hunk count, rename target; totals; file cap) parsed from the patch text, never a second copy
of the patch bytes. Config sources are admitted only inside the canonical root that declared
them: a symlinked `opencode.json`/`.opencode` root/`AGENTS.md` resolving outside fails closed,
while in-root symlinks remain admitted and `{file:}` stays relative to the admitted source.
