# Goal: hot reload (per-process); shared service descoped

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: frozen contract amended 2026-10-15 (R3/HOT03 descoped); T61 paused by owner, implementation IN_PROGRESS.
Source: требование владельца о reload без перезапуска, «паритет с донором»;
явный выбор «Также shared service»; согласованный после параллельного read-only
аудита минимальный план; 2026-10-14: «Вноси правки в план работ и коммит пуш на текущей ветке».
Last updated: 2026-10-15
Task: T61 / M8; detailed acceptance HOT01, HOT02, HOT04 только T61.

## Owner amendment (2026-10-15 — independent processes, no shared service)

Владелец исключил shared service полностью: каждый `oc2` — независимый процесс без
общего host на 127.0.0.1, Unix IPC, `oc service`, client attach/detach, durable
admission tokens или cross-client recovery. Это осознанная дивергенция с OC2
(managed `127.0.0.1` service + `service.json`). R3 и HOT03 удалены; R1/R2/R4
выполняются внутри одного процесса («один coordinator/supervisor per Location»
значит — в этом процессе). Сохраняются per-process data-root `oc.lock`
(`DataRootBusy` для второго процесса на тот же data dir), SQLite/WAL safety и
OAuth loopback callback. `oc reload`, `oc service`, `oc mcp list` и lost-ACK
reconciliation не делаются; manual reload — `/reload` внутри TUI. R5 квалифицирует
независимые инстансы. Разделы ниже про shared host сохранены как история и помечены
DESCOPED; они не являются требованиями.

## Objective

Каждый независимый процесс `oc2` (TUI/headless) сам наблюдает admitted
config/definitions/native-module settings и автоматически их обновляет;
`/reload` явно пересобирает сервисы загруженных Location этого процесса. Выполняющийся
request и полный его tool batch остаются captured, следующая fresh preparation
той же автономной задачи использует coherent новую generation без нового user turn,
потери сессии/истории или повторения effects. CLI settings перечитывает сам TUI.
Новый бинарник или env требуют перезапуска процесса.

## Frozen Contract

### Required Outcomes

- R1: coherent request-boundary adoption без потери authority/history.
  - Source: reload на следующей границе шага, не посреди tool call; утверждённый audit.
  - Acceptance: G1 stream/tools/approval/output policy/outcomes завершаются на G1;
    следующая fresh primary/child preparation получает G2 и exact saved selection.
    Повтор prepared attempt остаётся captured; fresh retry/continuation после durable
    settlement заново admitted. Summary operation pinned до конца, затем primary G2.
    Latest HOT, compatible closed groups, child launch ceiling и Location сохраняются.
  - Primary evidence: HOT01 held provider/tool/approval и child/retry/summary barriers,
    request/journal identity и effect counters; legacy/restart reads без replay.
  - Status: in_progress.
- R2: automatic и explicit reload, last-good и lease-aware MCP reconcile.
  - Source: наблюдаемые config/agents/commands/skills/native plugins; MCP config/tools;
    команды reload и runtime connect/disconnect из требования владельца.
  - Acceptance: create/edit/delete/atomic replacement admitted sources наблюдаются;
    invalid mandatory candidate сохраняет всю last-good generation. Auto semantic
    no-op не rediscover/reconnect; unchanged MCP reused, changed/removed недоступны
    новым requests, старые calls завершаются до cleanup/replacement. Disabled zero-spawn.
    Explicit reload forces discovery/recreation всех loaded Location; truthful receipts.
  - Primary evidence: HOT02 real filesystem + fake HTTP/stdio MCP/discovery barriers,
    last-good/effect/resource receipts, stale completion/control/late-unknown checks.
  - Status: in_progress.
- R3: DESCOPED 2026-10-15 (owner amendment above) — shared native service и client/lifecycle boundaries.
  - Source: явное «Также shared service», TUI/headless, `oc reload`, `oc mcp list`,
    `oc service restart`; согласованный local IPC и existing owner reuse.
  - Acceptance: один host/Db write/root-lock owner, bounded independent Location;
    one foreground root per Location, busy refusal before acceptance. Detach не
    shutdown; consumer/--auto/SIGINT/PTY control не затрагивают чужое execution.
    Lost ACK reconciles через durable admission без retransmit effects; reconnect
    resnapshots active work/interactions. Restart joins старого owner до replacement,
    adopts invoking process env/binary и не replays unknown effects.
  - Primary evidence: HOT03 removed with this requirement.
  - Status: descoped.
- R4: client-only hot settings без server rebuild или UI reset.
  - Source: valid `cli.json` theme/keybind/прочие настройки подхватываются TUI;
    утверждённый native supported subset и сохранение native precedence.
  - Acceptance: ordered native sources + captured client inline overlay дают last-good
    UI snapshot; поддерживаемые theme/bindings/settings меняются во всех consumers,
    без потери draft/tabs/live transcript/panels/selection. Shared-file edits видит
    каждый использующий их клиент; overlays/permission consumer остаются scoped.
    Presentation-only edits дают ноль provider/MCP rebuilds.
  - Primary evidence: HOT04 real PTY edit/repaint/key events, snapshot/cache/overlay
    checks и runtime zero-effect counters.
  - Status: pending.
- R5: composed executable qualification и правдивые living docs/handoff.
  - Source: требование паритета и существующие A01–A13/BUILD02/BUILD03 invariants.
  - Acceptance: HOT01, HOT02, HOT04 и affected regressions зелёные на итоговом code commit;
    rebuilt debug/release binary, clean HOME/XDG/no Node/Bun и workspace final gates
    подтверждены, включая два независимых процесса на разных data roots. CONFIG/
    ARCHITECTURE/help/startup guidance различают publication/adoption и reload/restart;
    report не подменяет live/visual owners.
  - Primary evidence: те же три composed fixtures, final commands и reviewed diff;
    не отдельный новый quality gate или paid campaign.
  - Status: pending.
### Constraints and non-goals

Четыре existing crates; один native runtime/store, no Node/Bun/JS/plugin host.
Сохраняются trust-before-substitution/effects, no-follow/source admission, central/
profile/parent/resource ceilings, Deny, response-close-before-tool admission, RAW
immutability, latest HOT/DCP/checkpoint bounds, finite retries и unknown-effect quarantine.
Caps/security/cleanup остаются fatal/non-success, optional connection failures isolated.

Нет general HTTP/REST/OpenAPI/SDK parity, web/remote attach/pairing/clustering,
same-Location parallel-root scheduler, durable work queue, второго journal/store,
generic RPC/config/plugin framework, новой dotenv discovery или polling service.
Не добавлять external theme documents, полный donor theme/action/setting inventory,
произвольный JS/TS/npm hot-load, machine-code hot swap, root auto-resume/replay,
provider-name heuristics, periodic OpenProxy `/models` polling или paid campaign.

## Boundary and ownership decisions

### Request publication, fencing and history

- Один source-Location publication handle; atomic immutable execution snapshot включает
  provider/catalog/profile/policy/tools/instructions/skills/commands/DCP/budgets и MCP
  authority. Все execution-affecting reads берутся из issuing request view, включая
  output limits/redactions, provider/child admission, approvals/questions, rename/move
  и after-read instruction attribution. Не оставлять ambient latest-generation reads.
- Lease валиден через issuing request/operation ownership, active turn/session/Location/
  move identity и применимые branch/HOT/instruction CAS. Arc сам не authority; latest
  publication inequality не отменяет корректный старый batch. Path/preimage checks остаются.
- Child разделяет только source publication handle; собственные execution/cancel/
  compaction/nudge state и launch identity не объединяются. Latest configuration
  пересекается с immutable launch ceiling, включая resource rules и service/credential
  domains; новый endpoint под прежним MCP name не даёт дополнительных полномочий.
  Parent Location switch не перенаправляет child. `session_move` по-прежнему завершает
  source turn; destination execution — отдельный последующий turn.
- Already-prepared replay pinned. Fresh retry/partial-output continuation после
  durable settlement получает current admitted view, сохраняет partial text/results
  и allowance, не исполняет incomplete arguments. Summary transcript/binding pinned
  на всю auxiliary compaction с correction/retries; затем fresh primary adopts current.
- Valid removal model/variant/agent публикует exact unavailable choice и отказ перед
  следующим dispatch, не fallback/reset. Mandatory malformed policy/config/definition
  отвергает целый candidate; permitted optional inventory failures не становятся fatal.
- Optional/default provenance в existing `RequestIdentity`/`TurnLog.requests` совместима
  со старыми logs; numeric generation — attribution, не restart authority. Совместимость
  по admitted config/profile/wire/auth, не имени модели и не любому новому counter.
  Compatible closed call/result groups сохраняются; foreign opaque не переносится,
  harmless metadata edit не уничтожает всю историю. Только latest HOT, без resurrection
  forgotten RAW; unsupported native-checkpoint boundary явный, не strip-and-retry.

### Один watch/manual coordinator и persistent MCP owner

- Loader возвращает finite watch metadata: known/missing sources, lexical/canonical
  targets, recursive supported definition roots, admitted `{file:...}` dependencies
  и instructions. Existing temporary `AdmittedSources` не готовый watch graph.
  Disabled templates inert; `{env:...}` использует captured server env, не новую `.env`.
- `notify` только в adapter; callback marks dirty. Debounce100ms, один active build,
  один accumulated pending trigger с OR force и bounded manual waiters. Watcher event
  не отменяет explicit force. Watch readiness/overflow/error/root recreation требуют
  reconcile/re-arm И admitted rescan. Missing parent anchors наблюдают known path,
  не расширяют config discovery/trust, не создают directories.
  Implementation decision (2026-10-15): notify8.2's Linux and polling backends do not
  retain/join their worker thread. The Debian/Linux adapter therefore uses notify's
  event/handler vocabulary on an owned nonblocking inotify FD and one joined Tokio
  task. No polling fallback, detached worker, extra runtime or generic watch framework;
  the callback still only marks dirty and the admitted loader remains the read authority.
- Фильтровать access/unrelated events, не все self-writes: atomic preference writes
  и concurrent external edits остаются наблюдаемы. Semantic no-op предотвращает loop;
  topology reconciles даже без новой generation. Global edits fan out loaded Locations.
- Stage = re-admit/read/normalize/explicit mandatory-versus-optional verdict, then
  required existing bounded discovery; до validation/freshness никаких MCP replacements.
  Discard superseded candidates. Provider identity сравнивает admitted discovery inputs,
  включая endpoint/auth/headers/authority/filter/local overrides, не raw text/selected ID;
  merged old remote catalog не превращать в original local models.
- Auto no-op не rediscover/reconnect и сохраняет runtime `/mcps` control state. Explicit
  `oc reload`/`/reload` forces discovery/recreation всех already-loaded Location, не
  boot caller cwd. Atomicity per Location: successful peers остаются, aggregate failure
  правдив. Config publication может предшествовать async MCP handshake; receipt различает
  published/pending/failure, не утверждает all-connected.
- Один Location-lifetime MCP supervisor, не новые supervisors с shared client Arcs на
  каждую generation. Desired config reconciles; request/catalog views immutable.
  Unchanged clients reused; changed/removed сразу исключены из новых request catalogs.
  Старые physical request/full-batch leases завершаются; виден pending retirement.
  После quiescence bounded stop/reap; replacement только после confirmed cleanup,
  проверки latest desired identity и sticky guards. Не блокировать unrelated MCP/coordinator.
- Connecting/live/retiring instances учитываются в bounds. Repeated edits coalesce
  latest desired, не очередь generation owners. Incarnation fences late connect/relist/
  close; binding/status/catalog coherent, old config controls rejected и revalidated
  при исполнении. Quarantine shared, не разовая copy: старый leased call может стать
  unknown после G2 publication. Смена URL/reconnect/eviction не очищает запрет.
- Prospective aggregate tools/schema/metadata catalog проверяется до exposure;
  existing cap/security/cleanup failures остаются non-success. Не превращать cap в
  необязательную ошибку entry и не повышать лимиты. Ordinary optional connect failure
  остаётся failed entry, не восстанавливает прежнее подключение под новым config.
- Automatic reload сохраняет unresolved interactions действующего lease. Explicit
  reload завершает только affected unresolved interactions distinct typed `Reloaded`,
  не generic question dismissal/whole-turn user cancel, не host-wide cancel(None).
  Settlement один раз, executing effects не прерываются; следующая preparation G2.

### Shared host, IPC, admission and frontends — DESCOPED 2026-10-15

История; не требование (см. owner amendment 2026-10-15).


- Один process/Tokio/Db write/root-lock/recovery owner; existing children/shell/approval/
  question/auth/PTY/title owners общие, не `application::spawn` на клиента. Bounded
  canonical-Location map; eviction только без clients/executions/retained resources
  и без сброса sticky safety state. Connection-local Location/view, existing
  Location tab persistence/CAS, не новые persistent decks на каждого клиента.
- One foreground root per Location; второй root там `TurnBusy` до durable acceptance.
  Разные Location независимы под host-wide cap; child concurrency не меняется.
  Малый session execution reservation защищает admission/move/routing, не scheduler.
- Root unknown/shell/terminal recovery один раз. Safe child candidates маршрутизируются
  по durable Location, config-dependent readmission deferred при auth-only boot.
  Первая загруженная Location не маркирует foreign candidates unknown. No root auto-replay.
- Consumer lifecycle scoped к connection/session и accepted execution lineage;
  `--auto` не глобальный queue default и не grant всем pending asks. Detach unregisters,
  first valid bound reply wins, отсутствующий consumer fail-closed/current headless
  semantics. Нет cross-Location cancellation. PTY один controller lease, остальные observe.
- Unix socket + closed versioned operation DTOs через existing `CoreApp` facade;
  покрыть shipped history/output paging, selections/sessions/tabs/Revert/fork, DCP,
  approvals/questions, MCP/children/shell/terminals/auth, не экспортировать все legacy APIs.
  `InboxMsg` с oneshot/cancel atomics и sensitive Composition не wire format.
- Private admitted directory/socket0700/0600, peer UID, product/protocol/instance/
  canonical data-root handshake. Election/stale cleanup lock-protected, не PID-only
  unlink/kill. Stable lifecycle exchange позволяет explicit restart при version mismatch;
  несовместимый protocol не вызывает silent replace. Bounded decoded frame/inflight/
  bytes/slow clients + host accounting; separate dispatch/replies/events сохраняют cancel
  и approval responsiveness. Service detached от terminal group; env captured at start.
- Admission token известен клиенту до send и связан с existing durable acceptance
  transaction. ACK = committed acceptance, не completion. Acceptance lookup reconciles
  lost ACK; повтор token не dispatch второй раз. Lost response = unknown admission,
  не rejection. Mutations не retransmit автоматически. Headless SIGINT адресует своё
  admission/execution, не чужой active turn по одному SessionID.
- Gap/reconnect выполняет explicit reset/resnapshot active execution, pending interactions,
  history/catalog/MCP; event rebroadcast недостаточен. Client guard detach отличается
  от host WorkerGuard shutdown/join. Detach не завершает service-owned work.
- TUI/run/reload/mcp и `auth`/`sessions list` используют shared owner вместо full Db opens.
  Auth base host не требует project config/model/MCP, browser opening client-side,
  input bounded/masked. `help`, catalog-only `models`, `service status` не auto-spawn.
  Existing guarded read-only SQLite catalog/cache path `models` разрешён: это не
  второй Db write/root-lock owner, no writes/recovery/credential reads.
- `--standalone` использует тот же native runtime/reload path с private owner;
  занятый тот же data root даёт `DataRootBusy`, не silent fork/store. Stop/restart ждёт
  всех shared-handle/root-lock holders; timeout/cleanup failure non-success. Replacement
  не стартует поверх старого owner. Restart использует env/binary вызывающего CLI,
  не меняет env живого TUI. Client/server trace destinations не truncates друг друга.

### Client settings

- Отдельные client parser/watch owner и narrow `apply_client_settings`. Сохранить
  native G/Location/.opencode `cli.json` затем `cli.jsonc`, runtime fallbacks, затем
  captured client `OPENCODE_CLI_CONFIG_CONTENT` overlay last. Добавить реальные CLI
  consumers `animations`/`terminal.copy`, не только перенос existing subset loop.
- Supported subset: compiled `opencode` theme, `theme: {name, mode}` и dark/light/system;
  system использует известный terminal mode, при unknown deterministic dark fallback.
  Donor binding shapes для implemented actions нормализуются с заявленной press/release/
  fallthrough semantics; unsupported options дают typed diagnostic, не silent flatten.
  Exact shapes/IDs брать из pinned donor, не придумывать. External themes вне scope.
- Валидный snapshot atomic; invalid edit сохраняет last-good. Shared-file edit
  независимо видят все процессы, использующие файл; env overlays/resolved prefs
  остаются внутри своего процесса. Client owns preference-file persistence;
  permission-mode change — scoped owner command, не косметический global autoaccept.
- Runtime catalog snapshot merge сохраняет client facets без изменения selection,
  readiness/capabilities/diagnostics. Не использовать broad `apply_catalog` или
  `refresh_configuration` для cosmetics. Все render/hit-test paths используют current
  theme; palette revision invalidates transcript/Markdown caches, не только mode.
  Drafts/parked tabs/live parts/transcript/panels сохраняются, zero provider/MCP rebuilds.

## Source oracle and current gaps

Pinned donor: `opencode` v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`.
Config watch/reload: `packages/core/src/config.ts`, `config/watch.ts`, domain plugins;
MCP diff: `config/plugin/mcp.ts`, `mcp/index.ts`; explicit all-loaded reload and borrowed
step graphs: `location-service-map.ts`, `location-services.ts`, `session/execution.ts`,
`session/runner/{llm,step}.ts`; CLI service/reload handlers and `packages/client/src/effect/service.ts`;
client settings: `packages/cli/src/config/config.ts`, `packages/tui/src/config/{index.tsx,keybind.ts}`,
`context/{theme,keymap}.tsx`. Источник истины — pinned source, не moving web docs.

Native registration baseline: explicit idle-only `reload_location`/`switch_target`,
no watcher/daemon, fixed-generation MCP supervisor и independent execution captures.
T61 groundwork now captures config/workspace/DCP/readiness and finite executable
provider views together. Fresh primary/child requests adopt the source publication;
prepared retries and one summary operation stay pinned. Children retain immutable
launch/ancestor ceilings while sharing only their source handle. Per-request profile
history and provenance-based restart are qualified at the owner boundary; composed
HOT01 with public staged publication remains. Request boundaries alone are not reload.
`CoreApp` — local bounded channels, не сериализуемый transport; queues имеют global
consumer flags, source-specific recovery привязан к initial runtime. CLI bypasses
`Db::open`, broad TUI refresh/static dark/mode-only cache keys требуют указанных швов.
OpenProxy-specific plugin отсутствует в donor core: native existing bounded discovery,
не выдуманное general periodic `/models` поведение; quota/sidebar refresh не reload trigger.
