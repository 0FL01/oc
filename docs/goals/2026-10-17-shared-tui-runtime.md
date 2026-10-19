# Goal: общий portless runtime и зеркальные TUI проекта

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete
Source: RECON и уточнения владельца 2026-10-17; аудит, поручение реализации/commit/push и утверждение границ 2026-10-18.
Last updated: 2026-10-18
Base: `fed671c6960c1c50b9b05471f646177bfcbad35d`, исходный worktree clean.

## Суть

**Несколько TUI одного проекта — экраны одного рабочего пространства, не независимые наборы вкладок.**
**Есть подключённый TUI — агенты работают; последний TUI закрыт — отмена как по Esc и Interrupted.**

```text
Терминал A: oc2 в project       Терминал B: oc2 в project
        [D] [S]                         [D] [S]
           \                              /
            +---- Unix domain socket ----+
                            |
                Общий runtime пользователя
                + один Db / flock / recovery
                + общий workspace project: вкладки D и S
                + отдельные исполнители чатов D и S
```

Runtime запускается автоматически, не требует свободного TCP-порта и не повышает права.
Домен владельца — текущий UID + data root; workspace — канонический Location проекта.
`root` и `stfu` не подключаются к одному исполнителю и не разделяют writable data root.
Будущее удалённое управление не должно требовать переделки application/runtime ownership,
но remote listener/API/auth сейчас не реализуются.

## Авторитетные уточнения

- Исходный запрос: запускаемый сервер без конфликтов портов, разделение `root`/`stfu`,
  возможность будущего remote control, KISS/YAGNI/Pareto.
- Ответ в RECON: «Параллельно и в одном каталоге» — нужны разные одновременно работающие
  root-сессии одного проекта, не только несколько наблюдателей одного serial worker.
- Зеркало: два терминала одного проекта показывают те же вкладки D и S и актуальную
  работу; новое подключение получает существующее состояние, не создаёт его копию.
- Lifetime: «агент жив пока запущен хотя бы один TUI с опенкодом»;
  «агент становиться мёртвым когда все TUI закрыты».
- Последний disconnect: «да там статус Interrupted тупо будет, как будто на `Esc` остановили».
- Межпроектный lifetime: три разных пути A/B/C одного UID/data root; после закрытия
  TUI A/B их агенты продолжают работу, пока подключён TUI C.
- 2026-10-18: «Вноси правки в план работ и коммит пуш, далее начинай итеративную
  реализацию плана и потом коммит пуш».
- Ответ владельца «Общий фокус, локальный draft (Recommended)»: active tab общий,
  неотправленный текст/картинки локальны. Утверждена общая часть вопроса: конечные
  run/auth/sessions leases отдельно от TUI-счётчика; последний TUI отменяет interactive
  задачи, но не отдельный headless run; env owner фиксировано при старте.

**Отменены владельцем относительно первой версии плана:** независимые persisted `ViewId`,
отдельные decks/Home на каждый TUI, `--view` и работа интерактивных агентов после закрытия
всех TUI. Старые R3/HOT03 из T61 автоматически не восстанавливаются: у них иной lifetime
и оставалось ограничение одного foreground root на Location.

## Подтверждённый контракт

### Required Outcomes

- R1 — обычные запуски подключаются к одному portless owner.
  - Source: исходный запрос про сервер/порты; утверждённая схема общего сервера.
  - Acceptance: два одновременных TUI одного UID/data root подключаются к одному
    runtime через UDS; только owner открывает authoritative Db и выполняет recovery.
    Второй обычный TUI не получает `DataRootBusy` от попытки стать ещё одним Db owner.
  - Primary evidence: изолированный actual-binary concurrent-start/attach fixture,
    identity одного owner, once-only recovery и отсутствие control TCP listener.
  - Status: verified
- R2 — общий workspace и бесшовная смена подключений.
  - Source: зеркало вкладок D/S и «открыть новый опенкод, потом закрыть старый».
  - Acceptance: подключённые TUI одного Location видят общий набор, порядок и
     состояние вкладок/чатов и активную вкладку; изменения доходят в реальном времени.
    B получает актуальный snapshot и поток событий; закрытие A при подключённом B
     не отменяет, не перезапускает и не дублирует исполнение. Неотправленный draft
     (текст/PNG) остаётся у своего клиента; общий editor не создаётся.
  - Primary evidence: paired PTY A/B/C с управляемым provider stream: общий deck,
    live update, новый attach до старого detach, один execution/effect counter.
  - Status: verified
- R3 — lifetime по TUI и штатный Interrupted на последнем disconnect.
  - Source: последние два уточнения владельца про все TUI и Esc/Interrupted.
   - Acceptance: счётчик TUI общий для UID/data root. В проектах A/B/C идут задачи;
     закрытие TUI A/B не прерывает их работу при подключённом C. Последний глобальный
     disconnect вызывает Esc cancellation всех interactive задач и durable Interrupted.
     Конечный headless run не отменяется уходом последнего TUI; auth/sessions удерживают
     только свою конечную операцию. При отсутствии TUI/startup/CLI leases owner joins
     ресурсы и выходит. Следующий запуск восстанавливает вкладки/историю без auto-resume.
   - Primary evidence: actual-binary A/B/C fixture наблюдает новый progress после
     закрытия A/B, затем global last-close/reopen; CLI без TUI/overlap/client-loss,
     durable Interrupted, ноль root auto-resume requests и owned orphans после cleanup.
  - Status: verified
- R4 — параллельные разные сессии одного каталога.
  - Source: явный выбор «Параллельно и в одном каталоге».
  - Acceptance: D и S одного Location начинают независимые root turns одновременно;
    одна сессия сохраняет single execution/conversation mutation ownership.
    Navigation/model/cancel/move одного чата не меняют captured execution другого.
  - Primary evidence: application owner barrier test: оба provider requests начаты
    до освобождения любого; same-session refusal происходит до input/effects.
  - Status: verified
- R5 — пользовательская граница исключает root-executor для stfu.
  - Source: исходная проблема файлов в home `stfu`, созданных общим root-сервером.
  - Acceptance: endpoint/data-root/peer UID проверены; клиент чужого UID не attach-ится
    к executor. Runtime и его tools работают под своим UID без privilege broker.
  - Primary evidence: реальные UDS peer credentials и foreign-owner/UID admission
    fixture; реальная пара root/stfu — только штатные отдельные пользовательские
    запуски, без sudo или host-admin действий authoring-agent.
  - Status: verified
### Затронутые ограничения

- Один storage/resource owner, `oc.lock`, WAL + `synchronous=FULL`, restrictive
  permissions, no-follow admission, immutable RAW/latest HOT и no unknown replay.
  Снятие flock или несколько `application::spawn` на один root не являются решением.
- Один primary execution owner сессии; child launch ceilings, permissions/Deny,
  captured request/full tool batch, session/branch/move fences сохраняются.
- Потеря ACK не разрешает повторный effect. Reconnect восстанавливает authoritative
  projection, не запускает второй turn и не притворяется полным event replay.
  Неоднозначно отправленные mutations, включая Submit, автоматически не пересылаются;
  неизвестная acceptance не обозначается ни отказом, ни подтверждённым успехом.
- `Interrupted` — штатная отмена как Esc, не замена честных tool outcomes. Уже
  начатый неизвестный эффект остаётся unknown; настоящий crash самого owner идёт
  через существующий crash recovery, не подделывается под записанный Esc.
- Общий workspace не даёт транзакционной изоляции файлов. Preimage/CAS и проверки
  конфликтных правок сохраняются; worktree остаётся способом разделить файлы.
- Существующие TUI/headless/auth/sessions/models contracts, stdout/exit semantics,
  prompt/image limits и resource/security/cleanup guards не ослаблять.
- Четыре Rust crates, без production Node/Bun/JS-host, нового broker/store/framework
  или dependency без конкретного blocker. Никаких user-config/secrets/data edits.
- Mandatory gates из GOAL/TEST_PLAN сохраняются. Case >30s или прогретый offline
  gate >10min требует RECON; thresholds/timeouts/ignores не повышать ради PASS.

### Вне scope

- Вечный daemon с работающими интерактивными агентами при нуле TUI; runtime,
  исполняющийся внутри одного выбранного TUI; перенос процесса между TUI.
- Независимые пользовательские наборы вкладок/`--view`, root-to-user broker,
  общая writable SQLite для разных UID и автоматический chown старых файлов.
- Public TCP/HTTP/WebSocket listener, TLS/RBAC, remote attach parity, web/desktop UI.
- Worker-процесс на каждый чат, распределённый scheduler, новая durable queue,
  generic transport/Application trait или весь соседний T61/hot reload.
- Автоматическое продолжение Interrupted или повтор неизвестных tools.
- Новая paid campaign, release/version bump/publishing, whole T44/T61/READY completion.
- Новые persistent retry receipts/fingerprints/index/tombstones, автоматический retry
  неоднозначной mutation, shared editor/CRDT, inactivity eviction активных Locations.

## RECON: подтверждённые владельцы поведения

- `storage.rs::Db::open_with_quota` владеет root lock, схемами и connection domain.
  Обычный второй open отклонён до SQLite. При обходе lock повторная initialization
  прервала бы active outputs; root-wide recovery может quarantine живые shell/PTY/child
  resources. WAL сам ownership не исправляет.
- `oc-core/src/core_app.rs::{CoreApp, InboxMsg, CoreEvent, WorkerGuard}` — существующая
  frontend/application граница. Там локальные channels, receipts и cancellation
  capabilities; это не готовый сериализуемый протокол.
- `application.rs::worker/query` и `runtime.rs::Runtime::active` — два отдельных
  single-flight barriers. `run_turn*` уже принимает `&self`, turn state в основном
  локален future; нужны session-scoped runners, не дополнительный Db owner.
- `Runtime::owned_child_snapshot` — образец owned shared handles, но копии MCP slot/
  safety flags и глобально выбранный `ExecutionGeneration.workspace` нельзя слепо
  использовать для независимых primary sessions.
- `application_selection.rs` и `application_tab_deck.rs` уже хранят Location-scoped
  deck/Home/session preferences и CAS. Для зеркала сохранять общий scope, не вводить
  ранее предложенные ключи `[view, location]`.
- `tui_cmd.rs`/`headless.rs` пока делают owner-wide shutdown; `auth_cmd.rs` и
  `headless::list_to_writers` открывают Db напрямую. Эти consumers затронуты seam.
- Screenshot соответствует `DataRootBusy` и строке в `oc-tui/src/shell.rs`
  «Native runtime is in-process; service attach is unsupported», не TCP-port error.

## Исправленный план: три зависимых шага

### Шаг 1. Общая application authority и исполнители сессий

- Узко supersede D26: второй storage owner всё ещё запрещён, второй TUI становится
  клиентом. Не восстанавливать T61 R3/HOT03 и не менять D27/D28/config/security gates.
- Один task T68 / SRV01–SRV05, существующий scheduling handoff; T44 не done, T61 не resume.
- Обновлять только затронутые live contracts/maps, отличая target от implementation.

- Владельцы: `application.rs::{spawn_stages, start_worker_inner, worker, query}`;
  предлагаемые модули `application/host.rs`, `application/root_runner.rs`.
- Host держит Db/recovery, общие shell/child/terminal/credential owners, automatic
  titles, `Location → LocationOwner` и `SessionId → RootRunner`. LocationOwner хранит
  rooted execution/config/publication/reload/provider/MCP state, не один global cwd.
- Owned root constructor в `runtime.rs`/`runtime/children.rs` клонирует shared handles
  без schema/recovery/MCP startup. Session-local active lease, compactions, moves,
  interaction queues и cancellation не блокируют другие roots того же Location.
- Разделить общий source/profile catalog и выбранный primary profile: изменения в
  `runtime/publication.rs`, `runtime/turn.rs`, `application/reload.rs`. Fresh request
  берёт coherent generation + выбор своей сессии; issued batch остаётся captured.
- Резервировать сессию до async preparation/selection mutation/spawn и до settlement;
  отдельный bounded completion channel возвращает settlement. Cancel доступен даже
  при full inbox. Compaction/move/title/cleanup не отменяют соседний root.
- Удалять неиспользуемые idle runners/Locations после освобождения activity/resource
  references, не по отсутствию TUI проекта. Active root/child/resource work удерживает
  Location; нет inactivity TTL. Сохранённый deck не удерживает runtime/MCP навечно.

- Host держит authoritative Location workspace и текущую revision. Клиенты
  отправляют действия над ним; owner сохраняет и публикует принятое состояние всем
  подключённым TUI этого проекта. Старый полный snapshot не затирает новую revision.
- Переиспользовать общий Location deck, CAS, fresh-root/fork pending adoptions и
  transactional capacity checks в `application_tab_deck.rs`/`storage.rs`/`storage_fork.rs`.
  Независимые ViewId, scoped prefs migration и `--view` из первой версии удалить из плана.
- Attachment identity нужна соединению/receipts/events, не новому persisted workspace.
  Новый клиент получает authoritative snapshot, затем live deltas; gap ведёт к
  bounded resnapshot, не дублированию чатов и не whole-history read. Snapshot/subscription
  ordering закрывает окно потери событий. Live projection позволяет observer root,
  не получавшему submission receipt, принять accepted turn/status/text/tools/retry.
- Active tab общий, draft остаётся локальным. Committed model обновляется у всех,
  но picker draft подтверждает только точный attachment/caller/revision ACK; issued
  request сохраняет captured attribution. Busy-close проверяет owner по session ID,
  stale painted index не переинтерпретируется по новому deck.

- Root approval/question queues разделяются с его children; pending requests доступны
  через общий workspace. Уход исходного клиента не лишает оставшийся TUI возможности
  ответить. Первый валидный ответ по полной binding решает конкретный pending request.
  При нуле viewers проекта и TUI C в другом Location новый Ask/question остаётся pending,
  не NoConsumer. Headless no-consumer refusal и explicit saved permission actions сохранены.
- `--auto` — captured policy своей операции/разрешённых descendants, не service-wide
  флаг, который разрешит чужую очередь. Явные saved permission changes отдельны.
- Consumer registrations/PTY control leases снимаются при disconnect. Resize/input/
  scroll одного PTY имеют одного ephemeral controller attachment; другой viewer может
  занять освободившийся slot. Passive viewers читают owner screen без auto-resize.
- Cancel проверяет ожидаемую operation identity атомарно у владельца: старый T1 control
  не отменяет T2. Conversation/move/family guards адресованы только затронутому owner.
- Выход шага: same-directory provider barrier и same-session refusal-before-effects;
  ближайшие deck/control/selection tests, без дублирования каждого контракта по слоям.

### Шаг 2. Bounded UDS bridge и lifecycle

- Сохранить concrete CoreApp frontend API; typed bridge конвертирует production
  commands/receipts/events. Не сериализовать InboxMsg/Composition/oneshot/atomics/
  WorkerGuard/KeyInput через общий Debug/log; не вводить Application/Transport trait.
- Wire и OS admission принадлежат adapters/binary, shared pure DTO — core при реальном
  consumer. Предлагаемые seams: `oc-adapters/src/service.rs` и крупные private
  protocol/client/server/lifecycle модули по фактическому совместному изменению.
- UDS с length-prefixed JSON: protocol/instance/attachment/request, Location/projection
  revision и session/operation identity. Binary fields явно base64, не numeric arrays.
- Frame cap не равен logical reply cap: prompt history до 50×1MiB и original PNG из
  Revert/Fork сохраняются; oversized supported replies передаются bounded parts.
  History paged; новая upload/blob система не нужна. Byte budget до allocation,
  bounded aggregate ingress/pending/egress и decoding copies; slow peer не блокирует owner.
- Существующие acceptance transactions и ACK-after-commit сохраняются. Никакого
  automatic resend неоднозначного Submit/shell/revert/fork/delete/preferences.
  Reattach читает текущую authoritative projection; missing result не доказательство
  nonacceptance, пока исходная admission pending. Новая retry schema не требуется.
- Private UDS directory/socket и owner-only registration: UID/admitted root/instance/
  protocol identity, no-follow/type/mode и mutual peer UID. PID/build — metadata,
  не auth и не разрешение убивать процесс. Короткий pathname fallback при необходимости.
- Election — один реальный Db open, winner сохраняет тот же Db в host; не предварительный
  flock + второй opener, не open/drop/reopen, не unlink oc.lock. Только winner запускает
  recovery и меняет stale endpoint; cleanup удаляет только собственную instance запись.
- Native internal launch отдельно от terminal/stdin/stdout/HUP; public service manager
  не нужен. Incompatible owner не kill/restart-ить молча; старый in-process owner даёт
  actionable migration diagnostic.
- Startup lease существует до ready attach; уход инициатора до первого attach без
  других startup/TUI/CLI owners вызывает joined cleanup, не вечный initial-zero daemon.
- На `1→0` TUI использовать существующую Esc-отмену всех активных interactive tasks,
  во всех Locations, settlement/Interrupted и cleanup. Конечные CLI leases отдельно:
  headless run не отменяется последним TUI, auth/sessions не держат host после операции.
  Client EOF завершает его lease/owned operation cleanup; другой TUI не теряет работу.
- Attach/detach сериализуются владельцем: готовый B удерживает lifetime до ухода A.
  Уже начатый shutdown не обозначать успешным attach; новый запуск ждёт cleanup,
  затем создаёт новый owner, не revive старого Interrupted исполнения.
- Счётчик относится к подключённым TUI общего runtime UID/data root, не к процессу,
  который первым его запустил. Последний disconnect запускает cancellation/cleanup;
  EOF/crash клиента также не оставляет вечную consumer/lifetime registration.
- Ранее предложенные public `serve`/`service start|status|stop|restart` не считать
  обязательными требованиями. Они не должны вводить бессрочный keepalive без TUI;
  достаточен минимальный internal launch/control path, нужный этому контракту.
- Выход шага: actual-binary election/identity/unsafe admission и потерянный startup;
  сохранён низкоуровневый STORE second-owner refusal, не обычный second-TUI failure.

### Шаг 3. Цельное переключение consumers и qualification

- `oc/src/{bootstrap,tui_cmd,headless,auth_cmd,cli}.rs`: TUI attach/detach вместо
  собственного Db/shutdown, CLI storage operations через authority. Auth не требует
  model/project/MCP; masked key input, browser и clipboard остаются client-local.
- До default activation готовы TUI/headless/auth/sessions и утверждённые CLI leases;
  никаких случайных alternate Db owners/временного public standalone режима.
- Сохранить catalog-only/read-only `models`, help/version/smoke без daemon effects.
- Server env фиксируется при старте; следующий TUI не подменяет его своим env.
  `trace.rs` не позволяет каждому attach/contender truncate trace живого owner.
- Перенести весь normal quit path, включая `reconcile_fresh_quit`/user-shell/title/
  conversation/acceptance reconciliation, не только `app.shutdown`. Clipboard helper
  остаётся локальным; viewer exit не Esc при другом TUI. Normal TUI quit остаётся 0,
  interrupted task сам по себе не делает exit130; headless SIGINT contract сохранён.
- `oc-tui/src/shell.rs`: typed startup/compatibility/migration diagnostics вместо
  универсального «service attach unsupported». Raw secrets/config/provider dumps
  в wire diagnostics и evidence не появляются.
- В binary support fixtures обеспечить stop/join owner даже после test failure;
  storage crash cases убивают owner, а отдельный client-crash case доказывает
  продолжение при оставшемся TUI. Низкоуровневый second_owner_is_refused сохранить.
- `memory_bounds.rs` измеряет owner + client + owned processes, не только тонкий TUI.
  Существующие archive workload/thresholds сохранены; произвольный fixture 128/8 удалён.

## Material decisions — frozen 2026-10-18

- Реальное препятствие wire: shared DTO содержат original `Arc<[u8]>` PNG и PTY
  bytes, без serde; numeric JSON arrays нарушили бы binary/byte budgets. Переиспользовать
  уже установленный workspace `base64` и узкий pure DTO codec в `oc-core` (не новый
  пакет/transport trait), сохраняя `KeyInput` несерилизуемым и Debug-redacted.

- Общие deck/order/active tab/live state, локальные unsent text/PNG drafts. Shared
  active route не теряет локальный draft другого чата; existing editor atomicity сохранена.
- Finite CLI leases отдельно от TUI count: последний TUI отменяет только interactive
  work, не headless run. При завершении/потере CLI клиента освобождаются его операция/
  ресурсы/lease, не чужие агенты. Auth/session-only startup не требует project/model/MCP.
- Host env immutable от запуска; последующие TUI/CLI не заменяют его окружением.
  Это явный shared-owner compatibility choice, не per-invocation env injection API.

Подтверждённый lifetime в обсуждении относится к interactive agents. Для предлагаемого
общего owner буквальное «все TUI» считается по UID/data root, не отдельно по чатам.
Browser OAuth callback 1455/1457 остаётся отдельным provider contract: portless control
socket не обещает отсутствия иных явно нужных listeners, существующий device flow сохранён.
