# Архитектура: модульный монолит

Rust 2024, KISS/YAGNI. Это границы и поведенческий контракт, не qualification report.
Маршруты к реализации/tests — [CODE_MAP](CODE_MAP.md), неподтверждённые outcomes — [GOAL](../GOAL.md), design references — [DECISIONS](DECISIONS.md).

## Packages и зависимости

```text
oc2 (crates/oc)          binary, bootstrap, event loop, headless
 |-- oc-tui             view-model, rendering, typed intents
 |-- oc-adapters        native authority, runtime, storage, tools/config/provider
 `-- oc-core            typed façade, DTOs, domain, ports
```

- `oc-adapters` реализует ports `oc-core`; обратной зависимости нет. Core может использовать Tokio/Serde, но не Ratatui, rusqlite, reqwest, rmcp или UI/storage/network adapters.
- `oc-tui` использует core application API и существующий adapter read-side. Это не разрешает raw Db/runtime/network/process I/O в view-model и не расширяется ради файлового рефакторинга.
- Binary связывает реализации; один native `oc2`, statically linked workspace libraries. Нет production Node/Bun/npm loader, JS-plugin host, Axum/API server «на будущее». Native Code Mode — confined Rust interpreter; fake servers и capture tooling остаются dev-only.
- DCP deterministic `ContextPlan`/`CompressionBlock` и commit rules — core; upstream selection/nudges/strategies — adapter policy. Узкие Provider/Store/ToolExecutor/ContextPolicy ports; enum/match достаточен для built-ins. Не создавать DI/plugin locator или trait/crate на каждый тип.

## Файлы и тесты

- Единица навигации — владелец поведения и укрупнённые implementation/test slices. Мягкий ориентир **5000 физических строк** на рукописный файл, обычно 1–4k для крупных slices; это не CI-запрет, минимум или размер всего модуля.
- Выше ориентира объясни связанность, риск переноса и следующий естественный шов; отдельный waiver/размерный отчёт не нужен. Не минифицировать строки, не прятать монолит в `include!`, не дробить согласованный 3k owner ради счётчика.
- Состояние и инварианты сохраняют одного владельца. Дочерние файлы могут содержать `impl TuiState`/`impl Runtime`; не создавать копии состояния, `Arc<Mutex<Everything>>`, framework/traits/crates ради переноса методов.
- `app.rs` использует обычный `app/`, не одновременно `app/mod.rs`. Private по умолчанию; scoped visibility только для реального шва. Сохранять публичные пути явными re-exports, без `pub use *` и массового `pub(crate)`.
- Новые существенные unit-сценарии — `src/<owner>/tests.rs`, при росте несколько тематических test modules. Маленькие существующие inline tests не переносить массово; `#[cfg(test)]` probes/branches не удалять как «тестовый блок».
- Integration target сохраняет `tests/<suite>.rs` с явными `#[path = "<suite>/<topic>.rs"] mod topic;`. Fixtures внутри suite/существующего `tests/support/`, не новые Cargo targets/common crate; public API ради tests не расширять.
- При переносе сверять test discovery/ignored, живые filters и `include_str!`/`include_bytes!`/`#[path]`. Обновлять карту/consumers; старые sealed proofs и frozen source fixtures не переписывать, новых SHA256-манифестов ради рефакторинга не создавать.

## Authority, сессии и requests

- Один native owner на **UID/data root** владеет Db/recovery, resources и canonical Location sources/MCP. Same-UID TUI/run/auth/sessions подключаются через bounded private UDS. Второй write/root-lock owner отказывает; public HTTP/TCP, privileged cross-user broker и `oc service` отсутствуют.
- CoreApp — конкретная typed façade на local channels/native wire, не новый transport trait. Mutable execution/conversation ownership — на session; разные root-сессии могут работать одновременно даже в одной Location. Channels/queues bounded, принятие сообщения подтверждается durable ack.
- Долгий stream не блокирует Cancel: owner обслуживает stream/inbox/shutdown; activities имеют owner/cancellation. UI не выдаёт storage failure за успешный ack и не получает raw Db handle.
- Prepared request/stream/approval/full tool batch/outcomes сохраняют captured model/config/profile/authority. Picker draft отличается от committed selection; следующий fresh request той же задачи заново разрешает committed model/tools/budgets/compatible history. Старые calls не переименовываются/не replay.
- Same-Location config adoption — только atomically validated publication на fresh preparation; issued leases неизменны. Fatal candidate сохраняет complete last-good. Оставшийся composed hot-reload/settings scope не объявляется qualified этим документом.
- Все tool calls: registry → strict input validation → effective permissions → durable intent → executor → durable outcome. Admission/causality и mutation ordering/CAS/path guards общие; независимые admitted children могут работать concurrently, same-child continuation упорядочен.
- Новый provider request ждёт terminal foreground batch. Background call возвращает running, а terminal result доставляется durable notice через parent owner без polling и потери при busy parent. Compress не запускает скрытую вторую модель.
- Terminal background subagent/model-shell report может продолжить live idle canonical root через существующий session owner: bounded batch получает один typed empty tail anchor и атомарную assignment receipt, не новый composer prompt. Источник и cancellation lineage остаются original Origin, новая preparation использует committed parent selection. Accepted/failed/unknown batch не requeue; Revert, parent/origin cancellation и shutdown подавляют handoff. Direct-user Shell не будит модель. Нет startup/history wake scan или promotion nested child в primary.

## Native lifecycle и Location

- TUI зеркалят Location deck/order/active route и bounded live projection; unsent text/PNG drafts локальны. Persistent tab changes используют существующий Location/CAS owner.
- Accepted running turn не блокирует root tabs/New/Add/Sessions и просмотр running child; navigation не отменяет и не переводит работу в background. Unresolved submission/Shell/model receipts и finite route-rebuild transactions нельзя парковать. Linked child остаётся read-only, но может вернуться к своему writable root; standalone child не получает root authority. Late attach/gap repair объединяет bounded canonical page и live snapshot с session-owned revision floor, не поглощая чужие stream events и не сбрасывая healthy retained window/draft.
- Last global TUI disconnect отменяет interactive lineage как durable Interrupted, не auto-resume. Finite run/auth/sessions leases независимы; headless не отменяется уходом TUI. Startup env фиксирован первым owner, новый клиент его не подменяет.
- One debounced watch/manual coordinator, latest desired MCP config и per-instance retirement/quarantine; request держит immutable authority/catalog lease, не создаёт clients на каждый chunk. Changed/removed instances закрываются после issued leases, late completions fenced; sticky unknown-effect quarantine сохраняется.
- Client settings принадлежат adapter orchestration: narrow UI overlay, сохранение live/runtime facets, palette cache invalidation без provider/MCP rebuild или draft/selection reset. Полная qualification этого surface остаётся в GOAL.
- Ordinary Location switch выбирает/создаёт Location-scoped session. Complete target generation строится до publication; failure оставляет текущую Location. Switch во время turn (OC2) не отменяет running root исходной Location; unresolved receipts его откладывают, view-model сбрасывает старые generation caches.
- Явный same-ID session move отдельно проверяет caller/access/destination type/trust и строит target generation; placement admitted/applied durable. Source turn/operations/jobs/children сохраняют provenance, move применяется на safe turn boundary, не мигрирует family и не расширяет permissions.
- После move следующий execution обновляет environment/instructions/catalogs/provider causality. Нет старого opaque continuation или unbounded retired-generation cache. Crash/restart deduplicates committed delivery, не replay неизвестные effects и не делает ambiguous resend.

## Storage и effects

- Одна SQLite DB/worker с короткими transactions, WAL и `synchronous=FULL`; synchronous work не выполняется на async runtime worker. Никаких network/LLM waits внутри transaction; inbox bounded.
- Messages/operations и небольшое monotonic session event сохраняются одной transaction; notifications после commit. Это transactional journal, не event-sourcing платформа; coalescing UI hints не теряет единственный terminal outcome.
- Own data root отделён от upstream, no-follow/restrictive ownership; advisory lock удерживается до joined shutdown. Lock inode не удалять по PID. Guarded read-only catalog/cache connection допустим без writes/recovery/второго owner. Network filesystem не квалифицирован.
- Outputs/attachments — content-addressed blobs: atomic write/fsync/rename **до** DB reference. Только own GC с grace period удаляет unreferenced blobs; referenced не удаляются. Disk-full/DB errors прекращают mutations, logs не копируют blobs.
- Cancel до admission — не запускать; после старта — остановка с partial/unknown outcome, не rollback. Drop future не обещает остановить blocking work. Shell process group: drain → TERM → deadline → KILL → wait/reap; escaped malicious daemon не делает shell kernel sandbox.
- Shutdown: запрет новых turns → cancel → settle known outcomes → drain DB/resources → restore terminal → release lock. Panic/startup recovery не replay неизвестные shell/MCP/mutations и не делает `git reset --hard`.
- Background recovery проверяет durable parent/child/job identity и deduplicates notices; committed results доставляются без исполнения. Donor at-least-once не означает exactly-once native effects; started/unknown effects quarantined. Parent completion не равно child cancellation, все own resources joined.

## История, context и DCP

- RAW — append-only source of truth; HOT/provider projection хранится отдельно и восстанавливается как latest committed view. Сборщик идёт по bounded references/active or addressed dependencies, не clone/read всего архива в RAM.
- Compression atomically меняет projection generation, не messages. Summary написан моделью; меньший размер сам по себе не доказывает semantic preservation. Активный batch не разрывается, закрытые call/result/reasoning группы сохраняются целиком или заменяются summary.
- Нет lifetime compress/block-count quota или depth exhaustion из-за возраста. Число прошлых операций не становится грузом hot path; active traversal/payload/model bounds и explicit user protections сохраняются.
- Exact child task/context pack неизменен в RAW и доставляется первым request; HOT может обновляться compress на закрытой границе до завершения той же задачи. Restart/recompression не восстанавливает забытое автоматически. LLM выбирает полезность/semantic completion, не harness classifier.
- Opaque reasoning привязан к admitted provider/config/profile/model/agent compatibility и causality group, не только numeric generation или имени модели. Harmless metadata publication не требует strip compatible groups; foreign opaque не переносится. Локальный `store:false` history projection не продолжает старую remote chain после compress.
- Token admission по model `limit.context` и независимый active byte safety cap дают явный OverContext/ContextOverflow, не silent truncation. При replay несовместимый opaque item — visible blocker, не strip-and-retry. Полный контракт — [DCP](DCP.md).

## Prompt и native tools

- Один runtime assembler: agent system/base → compiled bounded host/workspace/date → ordered AGENTS/skill metadata → permitted MCP/DCP guidance → history → user/tool results. Custom system не убирает environment; schemas — отдельное request field, config/TUI финальный prompt не собирают.
- Linux facts собираются native без shell/privilege escalation, отражают реальный executor, не `$SHELL`. Deterministic escaped fields, unknown optional facts без startup failure; immutable request snapshot, без env/proc dumps и накопления duplicates.
- Initial instruction baseline durable; successful nested reads/updates/removals — chronological origin/revision messages, не RAW rewrite. Reconcile после compact/Revert/reopen внутри admitted source bounds.
- Child получает свой profile/context, не parent transcript/system. Optional selected parent IDs — bounded exact escaped quotes после branch/revision/cutoff/budget checks; один durable snapshot/digest. Capability preview не обещает denied tools или automatic skill bodies.
- Canonical shell и legacy bash используют один policy/supervisor, не два advertised tools. Unset/zero timeout не снимает retention/shutdown bounds. Question имеет application-owned pending/answer/cancel identities, не permission reply/DB wait.
- File family (`apply_patch` либо `edit`/`write`) привязана к issued request. Exact OC2 model predicate — узкое исключение, не routing allowlist/permission grant. Direct model/session tools используют существующих owners; Code Mode вложенные MCP/fetch проходят общий admission.
- Read images идут через provider media boundary, не base64 pretending text; search/rendering bounded/pinned. PDF, built-in browser/websearch и arbitrary JS-host не добавляются под видом parity.

## TUI и bounded presentation

- Ratatui/Crossterm view-model получает redacted snapshots/live hints и dispatches typed operations; raw storage/config parser/catalog authority не дублируются. При отставании перечитывает cursor snapshot, не unbounded event backlog.
- Transcript paging использует целые durable parts с исходной sequence, не общий расходуемый preview budget. Continuation идёт через History/launch-fenced ReadChild; новые message pages начинаются с первой части, без tail gaps/reordering.
- Page target **8 MiB** допускает один более крупный producer-admitted part; TUI retention **240 rows /64 MiB** вмещает part до **32 MiB** generation guard и footer. Это не рост producer limits и не автоматическое чтение cold artifacts.
- Tool cards newest-first; diff card — bounded parsed summary, не копия patch. Tool operation preview явно clipped, full durable result читается continuation. Oversized input видим в note, не отбрасывается молча; prompt budget совпадает с core.
- Active DCP context читает только retained rows/needed wire logs; byte safety cap отдельно от heuristic bytes/4 token admission. Ручное обращение к pruned ranges может читать адресованный архив, но это не обычный per-turn hot path.
- Bracketed paste — одно событие; resize/stream не блокируют loop, output failure завершает с terminal restoration. `oc2` владеет event loop/terminal guard, `OC_TUI_TEST_METRICS` только opt-in PTY.
- Bare `oc2` при terminal stdin+stdout открывает TUI; иначе exit2 с headless hint, без hidden daemon. Explicit `oc2 tui` — stdin gate, непригодный stdout даёт видимый draw failure. `run --json` — NDJSON stdout, diagnostics stderr.

## Простота и безопасность

- Config sources admitted только внутри declaring canonical root; escaping symlinked config/.opencode/AGENTS fails closed, in-root symlink и source-relative `{file:}` сохраняются. Lower-trust text/summary не становится system authority; [SECURITY](SECURITY.md).
- Product caps — [examples/oc-rs.toml](../examples/oc-rs.toml), не benchmark guarantee. Логи metadata-only по умолчанию; payload trace отдельно opt-in, без credentials/high-cardinality labels на каждый token/message.
- Не оптимизировать allocator/musl/speculative cache/workers до измерений; owner-required child concurrency не speculative optimisation. Cancellation, memory/queue/process cleanup и bounded recovery — обязательные свойства, не необязательная документация.
