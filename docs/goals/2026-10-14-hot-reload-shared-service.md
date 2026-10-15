# Goal: hot reload и native shared service

Status: active — frozen contract; T61 active, implementation IN_PROGRESS.
Source: требование владельца о reload без перезапуска, «паритет с донором»;
явный выбор «Также shared service»; согласованный после параллельного read-only
аудита минимальный план; 2026-10-14: «Вноси правки в план работ и коммит пуш на текущей ветке».
Last updated: 2026-10-15
Task: T61 / M8; detailed acceptance HOT01–HOT04 только T61.

## Objective

Нативная фоновая служба `oc` обслуживает TUI/headless и внешние control commands.
Admitted config/definitions/native-module settings автоматически обновляются;
`oc reload` и `/reload` явно пересобирают сервисы загруженных Location. Выполняющийся
request и полный его tool batch остаются captured, следующая fresh preparation
той же автономной задачи использует coherent новую generation без нового user turn,
потери сессии/истории или повторения effects. CLI settings перечитывает сам TUI.
Новый бинарник/server env требует `oc service restart`, client env — restart клиента.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

T61 started 2026-10-14 after verified T59 closeout `4120504d7`; Git was clean before
activation. T44 scheduling/remote handoff and unrelated source/evidence are preserved.
Dependencies — completed T24/T46/T51; qualified T45/T50/T57/T59/T60 seams
потребляются без новых whole-task/live/visual done-dependencies. T30 требует T61.

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
  - Evidence: partial owner slices `8fc15275e`/`fb39d88a6`/`82ce7796b`/`581b2b186`/`85cbd3afc`/`78b3bf95a`/`322455183` below; composed HOT01 pending.
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
  - Evidence: partial mandatory reload admission `a9be5d0d4`, physical incarnation fences `823273e04`, same-supervisor desired reconciliation `e64e825a2`, finite admitted observations `bdafd9d19`, cancellation-safe commit `322455183`, complete idle application publication `7ce43773e`, superseded-build rejection `ac96ef997`, joined source observation `e5de05bd7`, original-input discovery reuse `98458b124` and same-task notification adoption `81d8fea50`; composed HOT02 pending.
- R3: shared native service и безопасные client/lifecycle boundaries.
  - Source: явное «Также shared service», TUI/headless, `oc reload`, `oc mcp list`,
    `oc service restart`; согласованный local IPC и existing owner reuse.
  - Acceptance: один host/Db write/root-lock owner, bounded independent Location;
    one foreground root per Location, busy refusal before acceptance. Detach не
    shutdown; consumer/--auto/SIGINT/PTY control не затрагивают чужое execution.
    Lost ACK reconciles через durable admission без retransmit effects; reconnect
    resnapshots active work/interactions. Restart joins старого owner до replacement,
    adopts invoking process env/binary и не replays unknown effects.
  - Primary evidence: HOT03 actual subprocess/PTY clients, cold-start election,
    cross-Location/lost-ACK/detach/restart/cleanup/recovery и interaction isolation.
  - Status: pending.
  - Evidence: NOT_RUN; `evidence/T61/report.md` отделит fake, actual binary и live.
- R4: client-only hot settings без server rebuild или UI reset.
  - Source: valid `cli.json` theme/keybind/прочие настройки подхватываются TUI;
    утверждённый native supported subset и сохранение native precedence.
  - Acceptance: ordered native sources + captured client inline overlay дают last-good
    UI snapshot; поддерживаемые theme/bindings/settings меняются во всех consumers,
    без потери draft/tabs/live transcript/panels/selection. Shared-file edits видит
    каждый использующий их клиент; overlays/permission consumer остаются scoped.
    Presentation-only edits дают ноль provider/MCP rebuilds.
  - Primary evidence: HOT04 real PTY edit/repaint/key events, snapshot/cache/overlay
    checks и service-side zero-effect counters.
  - Status: pending.
  - Evidence: NOT_RUN; full donor theme/action inventory не обещан.
- R5: composed executable qualification и правдивые living docs/handoff.
  - Source: требование паритета и существующие A01–A13/BUILD02/BUILD03 invariants.
  - Acceptance: HOT01–HOT04 и affected regressions зелёные на итоговом code commit;
    rebuilt debug/release binary, clean HOME/XDG/no Node/Bun и workspace final gates
    подтверждены. CONFIG/ARCHITECTURE/help/startup guidance различают managed/standalone,
    publication/adoption и reload/restart; report не подменяет live/visual owners.
  - Primary evidence: те же четыре composed fixtures, final commands и reviewed diff;
    не отдельный новый quality gate или paid campaign.
  - Status: pending.
  - Evidence: NOT_RUN; регистрация не implementation/qualification/overall READY.

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

### Shared host, IPC, admission and frontends

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
  независимо видят использующие его clients; env overlays/resolved prefs/consumers
  не становятся server defaults другого клиента. Client owns preference-file persistence;
  permission-mode change — scoped owner command, не косметический global autoaccept.
- Incoming server snapshot merge сохраняет client facets без изменения selection,
  readiness/capabilities/diagnostics. Не использовать broad `apply_catalog` или
  `refresh_configuration` для cosmetics. Все render/hit-test paths используют current
  theme; palette revision invalidates transcript/Markdown caches, не только mode.
  Drafts/parked tabs/live parts/transcript/panels сохраняются, zero provider/MCP rebuilds.

## Change Envelope and five minimum slices

1. **Request view/fences/history.** `oc-core/src/queries.rs` (`RequestIdentity`),
   `oc-adapters/src/runtime.rs` (`PublishedGeneration`/`TurnParams`/`TurnLane`),
   `runtime/{turn,context,instructions,children}.rs`, `runtime_compaction.rs`,
   `tools.rs` (`TurnLog`), existing history/storage-instruction owners. HOT01.
   R1 envelope clarification (2026-10-14): optional profile provenance also belongs in
   existing `oc-core/src/compaction.rs` operation JSON and `storage_compaction.rs` reads.
   Native checkpoints persist only model/wire today, so unchanged wire cannot prove
   the producing profile. This is the minimal fence for the required explicit
   unsupported boundary, not a new table, schema, store or checkpoint retry path.
2. **Reload + MCP.** `composition.rs`/config/defs admission verdict and finite topology,
   `application.rs` with a coarse `application/reload.rs` owner slice,
   `runtime/mcp{,/lifecycle}.rs`; workspace/adapter manifests + Cargo.lock for `notify`.
   Один pipeline/supervisor, no new registry. HOT02 consumes slice1 lease release.
3. **Host/IPC/frontends.** Existing application + coarse `application/host.rs`, owned
   `Db::shared_handle` seam; closed pure DTOs `oc-core/src/service.rs`, adapter
   `service.rs` with necessary coarse lifecycle/client/server slices; binary CLI/
   bootstrap/headless/tui/auth/service controls. Existing turns/turn_acceptances for
   idempotent admission; no second journal. Consumer authority/recovery/cleanup qualified
   BEFORE default shared-client enablement. HOT03 consumes stable source publication.
4. **Client settings.** Extract existing composition/config parsing into adapter-owned
   `cli_config.rs`; binary client wiring; TUI app/input/live/theme/shell/transcript/
   dialogs/approval/question/terminal consumers and caches. HOT04 parsing/rendering may
   develop in standalone independently of daemon internals; scoped consumer seam must
   precede shared defaults. No full inventory or settings framework.
5. **Compose and deliver.** Existing Cargo targets/owner test modules; real debug/release
   processes and PTYs. Update live CONFIG/ARCHITECTURE/CLI help/startup diagnostics and
   narrow supported-differences docs on actual implementation, keep historical leaves.
   HOT01–HOT04 + affected regressions/final gates, factual report and ordinary own-branch push.

No crate/file-per-operation or production test-only API. Paths name expected owners,
not permission for unrelated refactor. До правок подтвердить symbols по current HEAD:
RECON/audit baseline `51aa7a6be7f7b7d9461a26a7e7758cda12c3294d`, registration baseline
`68d3ca653f5c7e534dac5f2156c1fd2408c7f8cd`; чужие T58/source/evidence changes не входят.

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

## Current checkpoint and completion

- Closes next: R1; `8fc15275e` consolidates the immutable execution view; `fb39d88a6`
  carries it through issued batches/admission/Code Mode/output/instruction execution.
  `82ce7796b` adopts fresh primary publications and pins prepared retries/summary.
  `581b2b186` shares child source publications within immutable launch/ancestor ceilings.
   `85cbd3afc` qualifies profile history, exact unavailable choices, native checkpoint
   refusal and child restart across a publication-counter reset. R2 groundwork
   `a9be5d0d4` rejects malformed mandatory replacements before optional effects;
    `823273e04` fences MCP worker results/cleanup by physical incarnation;
    `e64e825a2` reconciles desired clients on that supervisor with leased retirement.
     `bdafd9d19` retains finite loader observations; `78b3bf95a` fixes initial capture
      across MCP awaits and releases physical clients at the issued batch boundary.
      `322455183` makes the supervisor own an accepted source/MCP commit even when its
      caller loses the ACK; expected-view CAS precedes retirement, and fresh captures
       share the short commit gate. `7ce43773e` supplies the complete admitted application
       candidate on that same runtime/supervisor for idle manual reload. `ac96ef997`
        re-admits original local inputs after discovery before replacement effects.
         `e5de05bd7` observes admitted Linux source events with a joined task and fences
         explicit candidates by event epoch, including changed-and-restored inputs.
         `98458b124` retains original local provider inputs for semantic no-op/catalog
         reuse, independently of selected provider and remote-merged metadata.
       Next retain that observer in the single persistent watch/manual coordinator
       before enabling busy publication and composing public HOT01 evidence.
- Expected evidence: HOT01 issuing/captured G1 versus next fresh G2, history/authority
  unchanged; если потребуется material expansion вне envelope — зафиксировать blocker,
  не наращивать общий framework.
- Current: R1/R2 in_progress; R3–R5 pending, HOT01–HOT04 not yet qualified. T59 complete;
  T44/T45/T57/live/visual statuses and historical PASS remain independent.
- Current owner evidence: held G1 provider response after G2 publication keeps G1
  approval/question bindings, read output budget, model lookup and after-read roots;
  rename and durable settlement finish without latest-counter rejection. Exact live
  waiter, active turn/child operation, Location/move, path/preimage and instruction CAS
  remain authority. Wrong/duplicate replies and foreign/settled/moved work refuse.
- Fresh-primary evidence at `82ce7796b`: after the held G1 batch, G2 fixed input,
  permissions, reconciled instruction roots and exact provider/catalog/effort are
  admitted without another prompt. Removed exact variant refuses before dispatch;
  settled calls/results survive route change, incompatible opaque output does not.
  Optional `RequestIdentity.config_generation` records attribution without authorizing
  execution or changing legacy receipt decoding. No second provider registry/store.
- Existing Chat/Messages retry case now publishes during held attempts: pre-output
  retry is byte-identical G1; settled batch admits G2; durable partial continuation
  admits G3, preserving partial text and the single settled tool, never partial args.
  Summary barrier proves transport retry and template correction stay on G1's route,
  transcript and protected-group installation, then the next primary uses G2.
- Child evidence at `581b2b186`: actual delegated G1 stream/read/approval completes
  after G2 publication; the same child's next request uses G2 prompt but retains G1
  resource Deny, Ask and closed native-tool authority. Exact G1/G2 approval replies route
  through the retained source even from another parent view; duplicate replies refuse.
  Source handle is shared, active/nudge/compaction state remains independent. Nested
  delegation cannot discard ancestor provider/MCP/credential/profile/depth ceilings;
  persisted ancestor fingerprints prevent restart reconstruction from broader sources.
  Changed static credential material or profile headers require new child authority;
  timeout/Code Mode routing changes alone do not authorize a new MCP domain.
- Profile/recovery evidence at `85cbd3afc`: optional `RequestIdentity.profile` preserves
  explicit default versus legacy absence; each retained request owns opaque compatibility.
  Profile changes retain closed pairs and portable facts, including selected HOT groups,
  without rewriting RAW or dropping a whole turn. Approval/question/output attribution
  uses the latest issuing receipt, not the initial turn digest. Exact model/variant/agent
  removals refuse before a next dispatch, preserving settled work and saved identity.
- Native checkpoints record optional profile in existing operation JSON. Same wire with
  an incompatible profile explicitly refuses without clearing the checkpoint or retrying.
  Legacy/default operation decoding remains supported; unknown custom origin fails closed.
  Summary input stays conversation-only and profile-compatible, never reconstructed AGENTS.
- Restart evidence reopens the actual SQLite root: original child generation77 resumes
  under current generation1 with matching launch/source/route/profile proof and one
  provider dispatch. Configuration/key mismatch, absent fence and unknown effect remain
  Unknown with zero replay. Exact selected-job/turn launch generation fences are retained.
- Earlier profile/recovery gates on `85cbd3afc`: `cargo test --workspace --locked` 1947 passed / 0 failed /
  12 existing opt-in ignored (46 result records); strict workspace all-target clippy and
  fmt pass. Nine `hot01_` owner cases, native-checkpoint profile, instruction lifecycle
  and actual TERM01 binary case pass. Logs `.local/evidence/t61-profile-*.log`.
  Red cases proved whole-turn profile history loss and current-counter restart refusal.
  Existing PRM01 caught summary instruction contamination during implementation; fixed
  with a data-only, profile-compatible projection, keeping its original assertions.
  TERM01 explicitly returns input ownership from the retired PTY before model submission;
  its original effect/resource/terminal assertions remain. Public idle reload guards
   remain unchanged; no watcher/daemon/settings or full HOT01–HOT04 delivery claim.
- Current R2 owner evidence at `a9be5d0d4`/`823273e04`: a malformed nonselected
  definition rejects the whole replacement before MCP launch, preserving the catalog
  and skills. Corrected atomic config replacement recovers. Invalid skills/commands/
  profiles/instructions/native settings reject reload, while existing startup fault
  isolation and disabled/unsupported optional inventory remain unchanged.
- Every physical MCP attempt has a retained internal incarnation token. Late relist,
  close and ordinary failure cannot consume a replacement's cancellation or state;
  late cleanup failure remains fatal. A real late stdio client is hidden from the new
  catalog, retained by an old request lease, then explicitly closed/reaped without
   mutating the replacement. This prerequisite was extended by `e64e825a2` below.
- Earlier R2 prerequisite gates: adapter tests 1043 passed / 0 failed / 5 existing opt-in
  ignored (19 result records); strict adapter all-target clippy and fmt pass. Four
  `hot02_` owner cases and all four actual `mcp_application` lifecycle cases pass.
  Logs `.local/evidence/t61-reload-verdict-*.log` and `t61-mcp-incarnation-*.log`.
  Red cases proved malformed replacement publication and stale completion consuming
  new ownership. No caps, deadlines, tests or assertions were relaxed. Whole candidate
  publication, source watches, shared service and client settings remained incomplete.
- Current R2 owner evidence at `e64e825a2`: desired reconciliation preserves the owner
  instance and unchanged client Arcs; view-only Code Mode changes also survive a held
  initialize. Changed/removed clients disappear from new catalogs but an old issued
  tool/lookup completes on its own client/binding. Replacement waits for confirmed reap;
  repeated edits launch only the latest desired client. An eight-client leased workload
  plus twelve edits retains at most eight physical instances, not queued generations.
  Automatic no-op preserves manual disconnect; old controls refuse. Aggregate catalog
  overflow remains fatal before exposure, and failed desired-count admission changes no
  live state. Late old-lease quarantine survives replacement and an empty publication.
- Exact admitted MCP source inputs guard an interrupted execution/MCP publication;
  a different counter with equal inputs is not itself a refusal. Existing runtime remote
  quarantine also applies to low-level reload. The public low-level reload remains idle-only;
   application reload still replaced its runtime at this earlier slice. Poisoned-owner
   retirement and source watches remained incomplete; application integration is below.
- Earlier `e64e825a2` gates: strict workspace all-target clippy; workspace tests 1957
  passed / 0 failed / 12 existing opt-in ignored, 46 result records; ten `hot02_` owner
  cases; fmt/docs/diff checks pass. Actual debug MCP/PTY/title/memory regressions are in
  that workspace run. Logs `.local/evidence/t61-mcp-reconcile-*.log`.
  AUD32 initially caught excess worker-stack RSS after service scheduling changed.
  Heap-pinning the existing actor execution operation and avoiding an empty MCP request
  roundtrip restored the unchanged64MiB bound: three measured large-archive peaks
  63492/63208/63240KiB and the final workspace gate pass. No test/bound/stack increase;
  temporary memory diagnostics were removed. Empty supervisor shutdown no longer emits
  a spurious post-terminal service event; the original title watchdog test passes.
- Current `bdafd9d19` loader evidence: known missing native config/DCP/instruction inputs,
  supported definition shapes, lexical/canonical aliases and actual admitted substitution
  dependencies are retained without bodies or environment values. Unselected provider
  admission contributes used and valid missing references; disabled templates, refused
  parent traversal, dotenv discovery and client-only CLI files do not extend this scope.
  Canonical definition targets come from pinned descriptors, not a raced lexical reopen.
  Topology is independently bounded at4096 targets/1MiB path bytes with typed failure.
- Earlier `bdafd9d19` gates: two `hot02_watch_` owner cases pass; strict adapter all-target clippy,
  adapter tests 1051 passed / 0 failed / 5 existing opt-in ignored (19 result records),
  fmt/docs/diff checks pass. Logs `.local/evidence/t61-watch-inputs-*.log`. No watcher,
  kernel watch allocation, completed candidate coordinator or new active publication is
  claimed. The full workspace above belongs to the earlier source, not this slice.
- Current `78b3bf95a` evidence: one actual MCP call issued on G1 finishes after desired
  G2 retirement; its settled result reaches the next same-task request. While that
  follow-up is held, the old stdio PID is reaped and its latest replacement connects
  on the same supervisor. The following request advertises only the new catalog,
  retaining the settled local result; two effects execute exactly once, receipts [1,2,2].
  Initial lane/profile and finite provider catalog are captured coherently after MCP
  awaits. Retiring the exact model during that await refuses before user acceptance
  or provider dispatch. Child admission/previews use current services intersected with
  all launch ceilings; initial degradation reporting retains no physical clients.
- Earlier `78b3bf95a` gates: eleven `hot01_` owner cases; strict workspace all-target clippy;
  workspace tests 1961 passed / 0 failed / 12 existing opt-in ignored (46 result records);
  fmt/docs/diff checks pass. Logs `.local/evidence/t61-mcp-batch-lease-*.log`. The red
  case proved a whole-task initial lease prevented same-task physical replacement.
  No public busy reload, actual watcher, shared host or client settings is claimed.
- Current `322455183` evidence: an accepted low-level reload commits execution and MCP
  even if its caller never polls the ACK and is dropped. A same-id superseding source
  refuses before any replacement process starts. Root, child and quoted-child fresh
  preparation capture source/service leases together before further awaits; protected
  source reads, discovery and network work are outside the short commit gate. MCP
  activation descriptors now belong to the same immutable execution source.
  The real held G1 MCP batch uses this commit path, finishes once, and the next requests
  retain its outcomes while adopting G2 prompt/profile and latest-only replacement tools.
- Earlier `322455183` gates: fourteen `hot02_` and eleven `hot01_` owner cases; strict workspace
  all-target clippy; workspace tests1963 passed/0 failed/12 existing opt-in ignored,
  46 result records. Fmt/docs/diff checks pass. Logs `.local/evidence/t61-atomic-source-*.log`.
  The red lost-ACK case left execution at1 while MCP was2; no test/cap was weakened.
- Current `7ce43773e` evidence: idle application reload stages complete workspace/profile,
  skill metadata, instruction roots, DCP/protections, executable provider views and MCP
  activation before the supervisor-owned commit. No temporary runtime or second owner;
  actual stdio supervisor instance stays identical through replacement and both PIDs reap.
  A real following request contains the new profile/instructions/skill metadata but no
  skill body, with read denied and DCP disabled. Reload itself creates zero sessions.
  Valid exact profile removal publishes unavailable without fallback; explicit repair
  is required before dispatch. Old issued lookup survives reload on its original binding;
  its connection is reaped before replacement connects. Explicit reload retires old
  title tickets only on success; rejected reload keeps accepted title work alive.
- Earlier `7ce43773e` gates: strict workspace all-target clippy; workspace tests1964 passed/0 failed/
  12 existing opt-in ignored,46 result records;22 reload-filter owner cases, fmt/docs/diff
  checks pass. Logs `.local/evidence/t61-application-source-*.log` and reload-owner logs.
  Red cases proved whole-runtime replacement and missing title-ticket retirement;
  source behavior was repaired without weakening resource, cleanup or title assertions.
- Current `ac96ef997` evidence: a real held `/models` request outlives a valid file edit.
  Local-only re-admission refuses that superseded candidate before MCP activation/source
  publication, preserving exact last-good catalog/generation and zero sessions; the next
  explicit reload admits the latest source. No extra discovery or credential resolution
  is used for the freshness check. Original local overrides remain separate from merged
  remote models, including resolved connection captures, definitions and pinned roots.
  Comments/formatting and inert disabled-template files do not change these semantic inputs;
  changed used credentials, instructions, skills or local metadata do. No payload logging.
- Earlier `ac96ef997` gates: strict workspace all-target clippy; workspace tests1966 passed/0 failed/
  12 existing opt-in ignored,46 result records;24 reload-filter owner cases, fmt/docs/diff
  checks pass. Logs `.local/evidence/t61-source-freshness-*.log`; red case proved stale
  publication during discovery. No existing assertion, cap or deadline was relaxed.
- Current `e5de05bd7` evidence: notify event semantics run on one owned nonblocking Linux
  inotify descriptor and joined Tokio task, without detached backend threads or polling.
  Bounded admitted anchors observe writes/atomic replacement, supported definition shapes,
  missing/recreated roots and overflow/error rearm; access, unrelated and client-only files
  are filtered. Failed replacement retains live watches; aborted observation is non-success.
  During idle explicit reload, registration/rescan and event epochs reject a changed-and-
  restored candidate even when its final bytes match. Normal cleanup is always awaited.
- Earlier `e5de05bd7` gates: four actual source-observer owner cases; strict workspace all-target clippy;
  workspace tests1970 passed/0 failed/12 existing opt-in ignored,46 result records; fmt/docs/
  diff checks pass. Logs `.local/evidence/t61-source-watch-*.log`. The isolated red case
  proved final semantic comparison alone missed changed-and-restored inputs. No resource
  bound, baseline, assertion or deadline was weakened. This observer is currently owned
  only by explicit candidates: no continuous watch/coordinator or busy publication claim.
- Current `98458b124` evidence: one shared original local snapshot is retained before saved
  credentials, custom/public catalogs or discovery can mutate the admitted composition.
  Provider identity uses original finite leaf captures and actual resolved request binding,
  not selected ID, raw formatting or old remote models. Changed credentials/endpoint/local
  metadata refuse reuse; matching catalogs/readiness carry forward without old credentials
  or remote models becoming local overrides. The saved custom-provider revision cache is unchanged.
- The same reload owner entry supports nonforced semantic no-op and profile-only publication:
  real bounded HTTP counts remain one through both, then explicit force performs the second
  GET. No-op keeps generation/MCP/manual control state; topology alone can reconcile. No
  automatic dispatcher or persistent observer has been enabled by this private owner path.
- Earlier `98458b124` gates: strict workspace all-target clippy; workspace tests1972 passed/0 failed/
  12 existing opt-in ignored,46 result records; both new owner cases and unchanged AUD32 pass.
  Fmt/docs/progress/diff checks pass. Logs `.local/evidence/t61-discovery-reuse-*.log`.
  AUD32 first caught worker-state RSS above the unchanged64MiB bound. Heap-pinning the
  existing whole-worker future reduced anonymous residency without changing runtime/threads/
  stacks/test limits: three measured large peaks63872/63932/65484KiB and final workspace pass.
  No threshold, baseline, assertion, deadline or ignore was changed.
- Current `81d8fea50` evidence: the existing actor owns finite accepted main/title/summary/
  shell catalog and context inputs instead of borrowing Composition through execution I/O.
  The existing MCP owner drives dirty-only notification relists before fresh same-task
  preparation, not during a prepared retry or issued batch. A real two-call G1 batch
  finishes exactly once after a tools-changed notification; the next request advertises
  only the new dirty-server catalog while preserving the unrelated server and both results.
  Dirty initialize/list counts are1/2, unchanged server1/1; both processes are joined.
- Current gates: strict workspace all-target clippy; workspace tests1973 passed/0 failed/
  12 existing opt-in ignored,46 result records, including the actual same-task notification
  case and existing three-turn notification regression. Fmt/docs/progress/diff checks pass.
  Logs `.local/evidence/t61-active-notification-clippy.log`, `t61-active-notification-workspace.log`
  and `t61-fresh-notification-*.log`. The red case missed next-request catalog adoption.
  No assertion, cap, deadline, resource threshold or ignore was changed. Public busy reload
  and continuous watching remain unimplemented; finite captures are a prerequisite only.
- Next: persistent single watch/manual coordinator with100ms debounce, including observations
   from failed candidates and later admitted nested instructions/explicit activation.
    Use the qualified original-input discovery identity, finite operation captures and semantic
  no-op path. Workspace admission must retain and re-resolve the active primary identity,
  not a global fallback or stale profile. Automatic/manual interaction boundaries and
  poisoned-owner/late-quarantine lifetime remain part of that integration.
  R3–R5 remain pending.
- Completion: все R1–R5 verified по HOT01–HOT04 и affected final gates на code commit,
  constraints/reviewed diff в envelope, factual report, затем обычный progress finish.
  Пока не выполнено; doc checks не дают runtime/visual PASS или overall READY.
