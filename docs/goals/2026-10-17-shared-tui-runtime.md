# Goal: общий portless runtime и зеркальные TUI проекта

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

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied. Stop
substantive work at a proven external blocker, an approved budget boundary, or when
no remaining in-scope action has a falsifiable expected result; record the exact
evidence and smallest unlock.

Контракт frozen ответом владельца 2026-10-18. Исполнение принадлежит T68; T44 временно
приостановлена без completion, T61 остаётся paused. Не выдавать structural checks,
промежуточный slice или сохранение плана за implementation PASS всей цели.

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
  - Evidence: debug full gate `9179f01a-6711-4888-bb25-e35c0453a865` и rebuilt release
    `4b096bed-d4c2-4858-a2bb-eb64869db277` PASS, включая actual concurrent-start/owner/startup.
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
  - Evidence: debug paired PTY, native gap/receipt и unchanged PNG/editor regressions
    PASS в `9179f01a-6711-4888-bb25-e35c0453a865`; rebuilt release paired PTY PASS
    в `4b096bed-d4c2-4858-a2bb-eb64869db277` (5.656s).
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
  - Evidence: A/B/C/last-close, headless/EOF/crash и auth/PTY handoff PASS в debug
    `9179f01a-6711-4888-bb25-e35c0453a865` и rebuilt release
    `4b096bed-d4c2-4858-a2bb-eb64869db277`.
- R4 — параллельные разные сессии одного каталога.
  - Source: явный выбор «Параллельно и в одном каталоге».
  - Acceptance: D и S одного Location начинают независимые root turns одновременно;
    одна сессия сохраняет single execution/conversation mutation ownership.
    Navigation/model/cancel/move одного чата не меняют captured execution другого.
  - Primary evidence: application owner barrier test: оба provider requests начаты
    до освобождения любого; same-session refusal происходит до input/effects.
  - Status: verified
  - Evidence: same-directory barrier/pre-effect reservation, scoped controls/move и
    captured publication/MCP regressions PASS в debug full gate
    `9179f01a-6711-4888-bb25-e35c0453a865`; rebuilt release scoped controls/move PASS
    в `4b096bed-d4c2-4858-a2bb-eb64869db277`.
- R5 — пользовательская граница исключает root-executor для stfu.
  - Source: исходная проблема файлов в home `stfu`, созданных общим root-сервером.
  - Acceptance: endpoint/data-root/peer UID проверены; клиент чужого UID не attach-ится
    к executor. Runtime и его tools работают под своим UID без privilege broker.
  - Primary evidence: реальные UDS peer credentials и foreign-owner/UID admission
    fixture; реальная пара root/stfu — только штатные отдельные пользовательские
    запуски, без sudo или host-admin действий authoring-agent.
  - Status: verified
  - Evidence: real same-UID UDS peer credentials и unsafe/foreign metadata/FIFO
    admission fixtures PASS в debug full gate `9179f01a-6711-4888-bb25-e35c0453a865`.
    Unprivileged rebuilt release fixtures также PASS. Это не real root/stfu launch
    pair: такой запуск только owner-operated, не выдан за authoring-agent PASS.

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

## Change Envelope

- Runtime: существующие application/runtime/publication/selection/storage owners в
  `crates/oc-adapters`, новый private service seam без нового crate/framework/store.
- Consumers: CoreApp/queries/DTO в `oc-core`, TUI state в `oc-tui`, bootstrap/TUI/
  headless/auth/sessions/trace в `oc`; сохранить public contracts и dependency direction.
- Evidence: ближайшие unit modules и существующие Cargo targets/binary fixtures,
  affected docs/registry/progress. Никаких user config/data/secrets edits, privileged
  authoring actions, arbitrary dependencies/paid calls/remote API/release scope.

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

## Минимальные доказательства и завершение

- R1/R5: actual UDS discovery/election/identity и unsafe/stale/incompatible admission;
  отдельный startup не может провести recovery живых ресурсов первого owner.
- R2/R3: paired PTY shared-deck/live-work/handoff/last-close/reopen; статус Interrupted,
  одна provider execution, после last-close ноль owned orphans и ноль auto-resume requests.
- Обязателен normal UI quit на fresh-acceptance границе, не только client kill. Отдельный
  A/B/C cross-project fixture наблюдает новый progress всех трёх после ухода A/B;
  reattach A подхватывает текущую работу; последний глобальный close останавливает все.
- CLI без TUI/overlap/client-loss и auth без model/project/MCP; transport oversized
  replies/original PNG/gap/stale control по ближайшим owner tests, не новый test matrix.
- R4: owner concurrency barrier, same-session busy-before-effects, scoped cancel/auto,
  coherent profile/reload и move A при продолжающем B.
- Сохранить affected STORE/crash/quota/leases/tab CAS/child launch/captured-batch/
  shell/PTY/auth/headless/DCP regressions, без копии каждого сценария на всех слоях.
- Сначала ближайшие owner tests и affected Cargo targets; после связной интеграции
  обязательный `bash scripts/test.sh`, обычный/locked build и изолированные rebuilt
  debug/release binary fixtures. Нового paid generation campaign для сервера нет.
- Завершение: каждый R verified текущими evidence, нерешённых material decisions нет,
  affected constraints сохранены, итоговый diff проверен; затем complete и остановка.
  Historical PASS или doc validator не доказывают runtime/whole T44/overall READY.

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

## Current Checkpoint

- Closed: R1–R5, только T68; [factual qualification report](../../evidence/T68/report.md).
- Source: `4af8c3ad646d283c5d7c79f269594931b4bb685c`, ordinary push в verified origin/master ACK.
- Primary evidence: full offline/debug и rebuilt release actual-ELF PASS, numeric
  resource receipts и RECON в report. Дальнейших implementation/audit действий нет.

## Current State

- Implemented и runtime-qualified: цельный native TUI/run/auth/sessions cutover;
  одна retained Db/recovery, private UID-checked UDS, Source/primary/resource ownership,
  bounded materialized live snapshot/gap repair, общий deck/active и локальные drafts,
  global TUI/finite CLI lifetime, scoped controls/auto/model/move и idle Source retirement.
- Последние прямые доказательства:
  - `76eb44ac-2051-49c7-b953-08f3f5d4d94f`: paired live/draft A/B/C/handoff/last-close
    PASS 4.198s; CLI overlap PASS 2.666s; stale-T1/picker PASS 2.492s; scoped move
    PASS 3.245s; обычные два одновременно стартующих TUI PASS 2.468s; startup/discovery
    PASS 25.701s/17.128s. Его единственный fresh-label отказ исправлен и подтверждён
    `3b9806b0-b827-4282-a548-d3717545224a` PASS 4.156s с управляемым title response.
  - `659b9ce0-6dda-4e17-a255-4c9ee7765abc`: scoped controls/move и два IPC checks 4/4 PASS;
    `fd6f83cc-f4e2-49c7-9bf7-c6dd6b8672bf`: MCP metadata/operation cancellation,
    per-incarnation retirement, truthful cleanup failure и no-effect-replay 9/9 PASS.
  - `849c5d2c-6bf9-4395-a5cc-0f7a69f07707`: 16-tab capacity, CAS/no automatic mutation
    retry, corrupt-deck isolation и независимые Home/session selection repairs PASS.
  - `7e784786-f310-48a2-87c3-c113f3dcf3e1`: неизменный 120-turn soak PASS 28.125s;
    RECON нашёл ожидание полного TERM grace при уже zombie-only группе. Теперь leader
    остаётся unreaped/PID-reserved, final group KILL сохранён; отдельные tests подтверждают
    пропуск только zombie wait и полный grace/SIGKILL для реально работающей группы.
    WAL FULL, workload, resource limits и assertions не ослаблены.
  - `defa9f8b-c8af-42a9-80cd-bcaf0714736e`: strict all-target Clippy PASS; 5/5 tests PASS:
    lost startup intent 1.388s, actual client crash/EOF handoff and Interrupted 2.064s,
    real owner/auth/terminal handoff 2.191s, TERM01 19.210s, local quit after owner loss
    0.008s. Owner crash остаётся unknown/recovery, не ложный записанный Interrupted.
- Первый полный gate был RED (2024/54/12); targeted fixes не выданы за полный PASS.
  Повторный gate `277ac197-cd6f-4125-a090-44c5f9793626` прерван рестартом authoring
  server: 641 PASS, 10 SIGTERM, 1432 not run; assertion failures до сигнала нет.
  Следующий полный `847b8495-3420-44b1-9bc5-b090dce948fe`: 2082 PASS/1 FAIL/12 opt-in
  skips, 378.306s test phase/383s wall. V04 отказ был false match terminal delta:
  diagnostic reconstruction показал точный ответ на painted screen. Проверка того же
  сценария через screen grid сохраняет wire/durable assertions: `6a69d8ee-ede2-441c-937a-c039f3b54c6a`
  1/1 PASS 6.570s. Discovery 32.122s имеет существующее owner-approved исключение ≤40s
  для настоящего 30000ms deadline, не новый timeout или ослабление бюджета.
- Review выявил два дефекта новых R3/R5 paths; `29c8f5e4-033c-409a-b1e1-3394facdc6f1`
  воспроизвёл потерянный overtaking CancelRequest и blocking FIFO registration open.
  Добавлены bounded pre-admission cancellation tracking и O_NONBLOCK перед прежними
  descriptor/file/UID/mode checks. `7eab9d6a-c34f-440a-b924-175950bf8701` подтвердил
  native/authority 9/9 PASS 2.540s: FIFO 0.019s, overtaking cancel 1.484s; wire limits,
  full-window priority, gap resnapshot, stale socket, peer boundary и same-directory
   reservation также PASS.
- Gate `4e226a42-9ea1-418e-928e-1d61d8ea1d5f` не является qualification: во время
  запуска владелец очистил часть `target`; 1128 failures — отсутствующие test ELF
  (exit70/ENOENT). Итог 953 PASS/1132 FAIL/12 skips, 233.749s test phase, 651s wall
  включая 379s rebuild. Один из четырёх assertion/timeout failures снова показал
  точный ответ в painted screen при false raw-delta match; тот же modal scenario
  переведён на existing screen-grid checks без изменения wire/durable assertions.
  После rebuild `50b4470d-344e-4e96-a6b8-04939a8fcd12` проверил все три PTY cases:
  3/3 PASS, 17.203s; исправленный modal scenario 7.952s, два прежних startup timeout
  1.996s/7.249s. Serial RECON `b4ef8666-37b9-4021-8779-fdc5d4a63299`: 5/5 PASS,
  60.195s test phase; три неизменённых storage workloads 13.658s/13.451s/19.798s,
  approval commit/restart 11.890s и reconnect/reply 1.390s. Все снова в пределах
  исходных budgets; конкретная причинность очистки/IO contention не заявляется.
  Deadlines, workload и durability assertions не изменены.
  Логи: `.local/t68-offline-gate-final.log`, `.local/t68-target-rebuild-{pty,recon}.log`.
- Blocker: отсутствует; material product boundaries утверждены 2026-10-18.
- Gate `7fafe172-6823-4537-8fd3-60fb7197625d`: 2084 PASS/1 FAIL/12 existing skips,
  371.976s test phase, 916s wall включая cold build. Единственный отказ — потерянный
  `closed` receipt в Python MCP fixture, после успешных sensitive-identity assertions.
  Deterministic worker-directed TERM reproduction показал 0 receipts: маска main
  не мешала unmasked worker запланировать Python handler между `closed=True` и записью.
  Workers теперь наследуют blocked TERM mask; main остаётся получателем сигнала.
  Тот же reproduction дал ровно 1 receipt; `f028f997-4caf-4159-b319-5f71812841fe`
  подтвердил весь affected `mcp_stdio`: 21/21 PASS, 1 прежний opt-in skip, 1.953s.
  Production teardown/security, assertions и deadlines не менялись. Следующий full
  gate использует новый log path; предыдущий RED не объявлен qualification.
- Next: нет новой implementation работы. T44 temporarily paused/incomplete, T61 paused;
  historical evidence неизменны. Closeout фиксирует только T68.
- Full gate `9179f01a-6711-4888-bb25-e35c0453a865`: 2085/2085 PASS, 12 прежних opt-in
  skips, 381.170s test phase, 430s wall; fmt, strict all-target Clippy, doctests и
  locked debug build/help/version PASS. Numeric owner/client/descendant receipts captured.
  Soak 32.662s вызвал RECON, не молчаливое исключение: дорогая первая эпоха 17.720s;
  scheduling уже exclusive. Узкий unchanged workload `2fdad410-0897-4290-9766-142272352443`
  PASS 28.172s (эпохи 13.328s/14.133s). Wall-time syscall profile показал 2323 fsync,
  23.304s в instrumented run: FULL durability — доминирующий I/O cost; он сохранён.
  Это не новый timeout/waiver и не утверждение точной причины разницы latency.
  Дальнейшая full-suite перестройка не требуется без нового implementation change.
- Rebuilt release `4b096bed-d4c2-4858-a2bb-eb64869db277`: 12/12 PASS, 29.246s test
  phase, 426s wall включая 6m36s cold fat-LTO build с 2 jobs. Девять shared-runtime
  cases, два startup suites и clean-HOME configured workspace; longest case 27.003s.
  Обычный `cargo build`, `cargo build --locked`, explicit locked debug build и
  debug/release help/version PASS. `target/debug/oc2` готов (697854200 bytes, mode755).
  Изменений storage schema/migrations нет; working owner/данные/конфигурация не тронуты.

## Completion

- R1–R5 verified; debug full offline и rebuilt release qualification PASS.
- Source implementation `4af8c3ad646d283c5d7c79f269594931b4bb685c` committed/pushed;
  factual report: [evidence/T68/report.md](../../evidence/T68/report.md), T68-only journal closeout.
- Commands: full `scripts/test.sh` 2085 PASS/12 existing opt-in skips; rebuilt release
  12/12 PASS; ordinary/locked debug builds и debug/release help/version PASS.
- Constraint/diff check: один retained Db/flock, FULL durability, UID/private UDS,
  bounded resources/captured requests/no unknown replay сохранены; нет schema/migration,
  user data/config/env edits, paid campaign, нового retry ledger или remote API.
- Final status: complete. T44/T61 не завершены и не resumed; whole READY не заявлен.
