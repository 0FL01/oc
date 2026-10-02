# DCP native port — обязательное контекстное ядро

Основа владельца: `compress` и подсказки агенту о сжатии. Source baseline: DCP 3.1.15, commit из `planning/baseline.lock.json`, AGPL-3.0-or-later. [D1–D4] в SOURCES. Это перенос функционального ядра, не npm package/auto-updater и не заявление о полном parity всех экспериментальных возможностей.

## Входит в goal

Range compression с несколькими spans, stable message/block references, вложенные summaries, protected tool/file/user/tag content; system/tool instructions и context/turn/iteration nudges; deduplication, purgeErrors; эффективный DCP config; durable state и replay; `/dcp` context/stats/manual controls и `/dcp-compress [focus]`.

Experimental message compression и custom prompt overrides остаются отложенными.
Subagent integration включена owner-approved T45/R9 (2026-09-27), implementation
pending: прежнее исключение отменено, не превращено в PASS. Explicit enable прочих
неподдержанных режимов — UnsupportedCapability, не fallback на range. AutoUpdate и
auto-generated upstream config не выполняются; native code обновляется сборкой,
не зависит от npm registry.

## Архитектурная граница

Raw session messages/parts не удаляются и не перезаписываются. DCP policy принимает bounded conversation view и выдаёт `ProjectionDelta`. Core валидирует anchors/generation/protections/call-result graph и транзакционно сохраняет план. Provider serializer работает над committed hot projection, не над архивом; TUI history показывает исходные данные с отметками compression. Fixed runtime/agent/AGENTS lanes собираются отдельно и не сжимаются/дублируются DCP. Loaded skill — обычная call/result group, не пожизненная hot lane. По owner amendment 2026-09-30 закрытые группы могут уходить из hot path целиком вместе с заменённым диапазоном. Не хранить вторую history в DCP JSON file/глобальном cache и не превращать native history в обязательную модельную память.

Summary — model-authored payload инструмента, не автоматически вызванная вторая модель. Никаких скрытых платных summarizer requests. Процесс DCP не управляет compaction authoring-agent; его продолжение обеспечивает `progress/`.

## Модельный инструмент

Range contract из upstream types [D3]:

```json
{
  "topic": "Закрытая часть задачи",
  "content": [
    {"startId": "m0001", "endId": "m0012", "summary": "Проверенные результаты и важные ограничения"}
  ]
}
```

`topic`/`summary` — непустые bounded strings; `content` — непустой bounded массив. Public shape не заменять произвольным `{text:...}`. Syntax/format реальных raw/block references и placeholder expansion взять из pinned source+tests, а не только примера выше. В первом task DCP сохранить schema и fixture IDs с provenance. Shape message mode не подмешивать в range.

Нельзя сжимать active незавершённый provider/tool batch; нельзя рвать связи внутри retained call/result/reasoning группы. Закрытая группа может быть забыта целиком, без повторного исполнения tool. Invalid/stale/cross-session IDs, backwards range, конфликтующие spans и неразрешимые явно authored references дают no-mutation error. Полностью покрытый старый блок не требует обязательного placeholder/append, если replacement намеренно забывает его незащищённое содержимое. Upstream overlap/embedding fixtures и owner-approved native forgetting difference квалифицировать раздельно, не запрещать все случаи ради удобства.

Compression планируется для всего batch до commit. Summary может явно включать ссылку на старый block; выбранное nested content раскрывается faithfully при bounded подготовке. Новый hot replacement самостоятелен и не требует persistent обхода всех предков. Пределы cycle/bytes срабатывают до allocation blow-up; не recursively materialize неограниченную историю. Ошибку явно запрошенного раскрытия не скрывать заглушкой. Намеренно не включённое в replacement старое содержимое — разрешённое забывание, а не failed expansion.

T45/R9 supplements this guard: valid repeated replacement/forgetting must not
inevitably exhaust depth merely because previous summaries form an aging chain.
Flatten alone is insufficient if it copies all old content. Preserve explicitly
chosen placeholder expansion/current protected bytes faithfully; keep stable IDs,
provenance and replay in durable metadata, not obligatory live dependencies.
Historical depth/byte bounds remain safety guards for legacy/malformed expansion;
increasing them is not the long-horizon solution. The detailed 2026-09-30 contract
is [infinite hot / optional cold](#infinite-hot-context--optional-cold-path--t45r9dcp11-pending).

Token saving фиксируется как measured/estimated; сохранность смысла не следует из меньшего размера. При summary, не уменьшающем проекцию, не создавать бесконечный compression loop: visible no-gain outcome, сохранить исходную projection и разрешить другой selection. Этот anti-loop guard — наш safety difference.

## Protections и унификация patch

Upstream protects tool/file content и умеет preserve user messages/tags [D1]. Перенести required config и exact behavior из fixtures. `apply_patch` имеет защиту, эквивалентную protected write/edit; owner-approved T50/R9/TOOL20 распространяет её на реальные `edit`/`write`. У patch вычислять `affected_paths` из распарсенного patch до исполнения; protected glob проверяет все source/destination пути, а не отсутствующий параметр `filePath`. Для edit/write брать parsed `path`, не трактовать `content`/`oldString`/`newString` как ресурсы. Common mutation permissions/data-root/no-follow/approved-preimage и configured tool/content/dedup/purge protections сохраняются при смене модели; Ask не bypass защиты.

T50/R1/TOOL12 pending: DCP/restart/native compact reconstruct the committed selection's
compatible file schemas/catalog/managed guidance, never archived obsolete definitions.
Live commit while busy is adopted at the next request preparation/rebuild of the same
task, including retry/compaction; prepared streams/tools retain captured identity/view.
Recompute model budget/thresholds and compatible hot projection together. Removing
tools does not erase/rename retained historical call/result groups or drop completed
outcomes merely because the model changed; exclude alien opaque/checkpoint state.
It never reloads forgotten raw groups. Current causal/protected groups and bounded
hot state remain; no mutation replay or second context owner. TOOL20 verifies new tool
protection consumers while existing DCP04/other DCP IDs retain owners and historical evidence.

Summary хранит выбранные useful mutation outcomes/outputs; references на большие blobs только когда они нужны продолжению. Нельзя подменить currently required verbatim content ссылкой, когда effective policy требует verbatim. Runtime task/pack protection снимается по завершении своего scope; historical suffix не получает защиту только из-за происхождения. Явные пользовательские protections остаются пользовательским выбором; их изменение/release учитывается на admitted generation, не обходится молча. Не append всей истории/огромного patch/старой защиты к каждой summary «для сохранности».

Permissions `compress:allow|ask|deny` независимы от file mutations. DCP summaries/теги не могут расширить privileges. Labels/IDs выдаёт runtime, не доверенный user text — экранировать collisions и prompt-injection содержимое.

## Nudges и automatic strategies

Owner-approved native defaults (2026-09-27): minContextLimit `"40%"`, maxContextLimit
`"55%"`, summaryBuffer `false`; остальные defaults сохраняются, в том числе
nudgeFrequency 5, iterationNudgeThreshold 15 и nudgeForce `soft`. Это pending
T45/R9/DCP12, не claim изменения compiled code. Исторический upstream/native
profile 50000/100000/summaryBuffer=true остаётся source baseline [D4] и может
сохраняться как явный user override, но больше не является целевым default.

Runtime hard admission учитывает requested output/safety reserve и сильнее DCP
reminder; процентная база — positive effective context capacity, не оставшийся input
budget. Панель различает reminder thresholds и реальные model/input constraints,
объясняя fallback и budget clamps, а не расширяя input до reminder threshold.
Для models без limits не выдавать estimate за точный token counter.

Nudge timing/placement и counter resets перенести из pinned hooks/prompts/tests. Не добавлять новую persistent reminder message на каждый token/chunk; emitted prompt projection не должна со временем накапливать дубликаты nudges. Context-limit reminder повторяется по частоте, пока превышение реально сохраняется; успешная compression пересчитывает counters. Manual mode отключает autonomous tool invocation/instructions в объёме baseline; manual automaticStrategies control не игнорируется.

Dedup: одинаковые tool name+canonical arguments, оставить последний нужный output, protections сохраняются. PurgeErrors: после configured turns (default 4) убрать из projection крупный input errored call, но сохранить error outcome. Эти стратегии пересчитываются при compress, как описано upstream; не запускать pruning на каждом request из инерции старых версий. Случаи manual mode уточнить по executable fixture; README не заменяет source test.

## Config surface

Поддержать enabled; pruneNotification/type; commands.enabled/protectedTools; manualMode; turnProtection; protectedFilePatterns; compress enabled/range/permission/showCompression/summaryBuffer/min/max/model overrides/nudgeFrequency/iterationNudgeThreshold/nudgeForce/protectedTools/protectTags/protectUserMessages; strategies.deduplication/purgeErrors. `compress.enabled` — owner-approved native extension от 2026-10-02, pending, не поле pinned upstream. `debug` включает только безопасные metadata logs. `pruneNotification` сохраняет off/minimal/detailed, не boolean; type — chat/toast. Display defaults detailed/chat, showCompression=false. Notification `toast` может отображаться как bounded transient TUI status notice — documented UI difference.

Config source order фиксируется отдельной source-derived fixture вместе с general config roots; `cli.json` сюда не входит. Sources проходят explicit trust boundary и остаются read-only. Native enabling не добавляет npm package. Supported aliases — exact `@tarquinen/opencode-dcp`, `@tarquinen/opencode-dcp@3.1.15` и пользовательский `@tarquinen/opencode-dcp@latest`; все три дают один instance repository-pinned compiled revision. Последний не резолвится через npm/registry. Semver ranges и другие versions — `UnsupportedPlugin` до package/network side effects.

## Compression switch and effective config controls — T45/R9/DCP12 (approved 2026-10-02; pending)

**Source/owner:** после RECON владелец утвердил правки плана, commit и push текущей
ветки. Новый `compress.enabled` и найденные manual/commands/debug gaps принадлежат
T45/R9/DCP12; DCP05–DCP07 сохраняют прежних владельцев и служат regression/source
fixtures, DCP10 — child isolation, T44/VIS38 — отдельная presentation qualification.
Новых tasks/gates, runtime config writes, npm host или paid campaign нет.

### Source-derived snapshot, not runtime qualification

Read-only RECON на HEAD `32fc02ba4031ba0a630e62873e7015caead2077f` с dirty T50:

- `dcp_auto.rs::DcpConfig/load_config`: `compress.enabled` отсутствует и неизвестные
  nested compress keys молча игнорируются; `commands_enabled`/`debug` принимаются,
  но их runtime effects не подключены. Compiled defaults пока 50000/100000/true.
- `runtime/turn.rs` и `runtime.rs::preflight_compression` скрывают/запрещают tool в
  manual mode. `application.rs::compress_prompt`/`InboxMsg::Compress` всё равно
  превращают `/dcp-compress` в ordinary Submit. Прямой `run_compress` проверяет
  permission, но не global enabled/manual; один UI-only gate этого не исправляет.
- Native nudge counter продвигается при подготовке requests и сбрасывается после
  compression. Original context/turn/iteration anchors и last-user timing нельзя
  считать эквивалентными только по числам 5/15 и старым synthetic tests.
- Настраиваемые file/tool/tag/user/turn protections и strategies имеют consumers.
  Notification/channel/showCompression уже подключены к typed TUI renderer, но
  это не полный VIS38 PASS. `allowSubAgents:true` пока warning/ignore.
- Original 3.1.15/3.2.0 не имеют `compress.enabled`; их compress surface/defaults
  совпадают. `compress.permission:deny` снимает tool и system guidance; внутренние
  `injectCompressNudges`/`injectMessageIds` также проверяют deny, несмотря на отсутствие
  early return у внешнего chat-transform hook. Manual trigger не обходит permissions.

Original references: [3.1.15 config](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/config.ts),
[registration](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/index.ts),
[manual command/hooks](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/hooks.ts),
[leaf nudge/ID gates](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/messages/inject/inject.ts),
[3.2.0 config comparison](https://github.com/Opencode-DCP/opencode-dynamic-context-pruning/blob/d637981555a18c3992472268a0657a948925d5fa/lib/config.ts).
Это source comparison, не смена compiled baseline/AGPL provenance и не полный3.2.0 PASS.

### Frozen switch/manual/command semantics

Proposed native fragment — **не поддерживается текущим бинарником**:

```json
{
  "enabled": true,
  "compress": {"enabled": false, "permission": "ask"}
}
```

1. `compress.enabled` — строго boolean, missing means true. False запрещает **все
   новые compression operations**, включая explicit manual/API entry points; true
   не меняет сохранённый permission и не расширяет central/profile/parent-child
   authority. Global `enabled:false`, effective Deny и child opt-out сильнее switch.
   Schema/catalog, managed guidance, compression anchors/nudges, preview, preflight
   и dispatch используют одну effective availability. Не удалять слово compress из raw user/
   profile/history text. Malformed switch — safe pre-effect config error; unknown
   nested compress keys дают source/field-qualified warning, не silent acceptance.
   Независимые stable text-message IDs CTX02 остаются видимыми при off/manual/deny;
   removal касается compression-specific annotations, не capability context selection.
2. Disabled `/dcp-compress`/panel trigger отказывает с безопасной причиной **до**
   Submit/provider/нового user turn или compression intent. Stale/unsolicited model
   call получает paired refusal без block/projection mutation. Read-only context/
   stats остаются доступны при `commands.enabled:true`, с честной причиной off/deny.
   Low-level `run_compress` не служит обходом global/tool switches или Ask/Deny.
3. `manualMode.enabled:true` означает отсутствие автономных compression nudges/
   invocation, но разрешает явно admitted `/dcp-compress` при enabled switches и
   effective permission. Application owner передаёт typed session/generation-bound
   manual trigger и только для его bounded turn/pass выставляет tool/closed anchors
   и manual guidance. Не узнавать trigger по ключевым словам/focus/LLM-тексту;
   произвольный Submit его не создаёт. Trigger не становится постоянным enable и
   снимается по completion/cancel/denial/failure. Ask требует настоящего consumer;
   headless без consumer отказывает до provider/effects, не auto-approves.
   Superseded только blanket manual tool exclusion для explicit admitted trigger;
   обычные autonomous/manual restrictions и automaticStrategies policy сохраняются.
4. `commands.enabled:false` реально выключает `/dcp` и `/dcp-compress` в native
   registration/completion/palette/dispatch; stale panel action проверяется у owner.
   Это не switch самого model tool, strategies, native `/compact` или read-only
   query API. Отдельное compression off не выключает остальные product commands.
5. `debug:true` подключает metadata-only diagnostics у существующего trace owner;
   false оставляет обычные mandatory error diagnostics. Не писать transcript,
   arguments/summary bodies, env/keys/headers/raw responses или sensitive paths.
   UI получает минимальные typed availability/reason facts через существующий query,
   не отдельный policy calculator/store; VIS38 остаётся отдельным visual gate.
6. Config принимается read-only по существующим source roots и публикуется на
   безопасной config/Location generation boundary. Prepared request/tools/approval
   сохраняют captured view; модельный live switch не является reload DCP-файла.
   Off/on не декомпрессирует committed summaries, не сбрасывает durable history/
   counters/marks и не воскрешает забытое. Настройки strategies сохраняются, но новый
   pruning по-прежнему commit-time: отсутствие compress trigger не создаёт фонового
   pruning worker. `/compact` остаётся отдельным механизмом с прежними guards.

### Compatibility boundaries and qualification

- Сохранить уже утверждённые native roots: replacement OPENCODE_CONFIG_DIR,
  global→Location root→.opencode, inline→JSON→JSONC. Original добавляет config_dir
  поверх global, ищет nearest ancestor .opencode и выбирает JSONC вместо JSON.
  Native deep merge заменяет arrays, original объединяет protection arrays; это
  объявленные отличия, не разрешение менять source precedence в switch-срезе.
- Original command/strategy protections отличаются от compress output preservation;
  compress defaults содержат task/skill/todowrite/todoread, command defaults — десять
  donor tools. Native общий список сейчас пуст по умолчанию и смешивает command/
  compress scopes. Freeze executable protection/default fixtures с реальным native
  каталогом; не заявлять parity по accepted keys, не копировать несуществующие tool
  names и не ослаблять explicit protections. Не менять defaults/array merge молча;
  native forgetting/current protection lifecycle DCP11 остаётся authoritative.
- Cadence fixtures сверяют original context/turn/iteration/last-user semantics с
  native requests, сохраняя 5/15/soft, successful-compress cooldown и approved
  40%/55%/false accounting. Исправлять установленный timing gap, не выдавать одинаковые
  defaults за algorithm parity и не переносить archived reminders в hot state.
- DCP12: no-file default/explicit true/false/wrong type/unknown nested key, source
  override, Deny/Ask/global off и explicit manual trigger; fake-provider captured
  requests доказывают отсутствие schema/guidance/compression anchors/nudge при off;
  rejected manual command не создаёт provider dispatch/нового turn/block. Direct entry point
  obeys switches; re-enable сохраняет Ask/Deny и committed projection.
- Rebuilt actual binary: `/dcp`/`/dcp-compress` и palette после reload/restart/Location
  switch, commands off при still-enabled model tool, metadata-only debug on/off;
  DCP10/CTX02 reuse root/eligible-child isolation и truthful capability preview.
  Нужны реальные successful manual compression и следующий request в manual mode,
  не только API `run_compress`/успешный model prose. Regression targets: dcp_auto,
  composition/application/runtime, dcp_atomic/context_bounds, oc/tests/dcp_runtime;
  relevant crate/workspace gates по существующему TEST_PLAN, без новой paid нагрузки.

Done этого среза — qualified flags, manual route и truthful consumers; не all-T45,
DCP11, VIS38 или READY. Исполнение только после safe handoff active T50; T44 PAUSED,
task IDs/dependencies/statuses, historical evidence и baseline остаются неизменными.

## Approved percentage defaults — T45/R9/DCP12 (pending)

Canonical target фрагмент `dcp.jsonc` (иллюстрация утверждённых defaults, не
изменение пользовательского файла или доказательство реализации):

```json
{
  "compress": {
    "minContextLimit": "40%",
    "maxContextLimit": "55%",
    "summaryBuffer": false,
    "nudgeFrequency": 5,
    "iterationNudgeThreshold": 15,
    "nudgeForce": "soft"
  }
}
```

1. **Native defaults, overrides and provenance.** Change omitted-field defaults in
   `DcpConfig::default`, not only a sample/local override. Positive integer token
   limits and existing valid percent strings remain configurable, including partial
   and exact per-model overrides. Explicit numeric overrides clear the percentage
   form; explicit summaryBuffer=true keeps its opt-in semantics. Preserve source
   precedence, validation/precision guards and one immutable config generation.
   Update public help/samples with implementation; existing examples/dcp.jsonc is
   an explicit older profile, not a no-config default test. Other defaults and
   pinned revision/license stay unchanged. Config files are optional for DCP defaults.
2. **One effective model basis.** Runtime and DCP panel use the same canonical
   provider/model key, selected model budget/context capacity, native positive
   fallback and exact model overrides. Resolve percent limits as floor(context ×
   percent/100), minimum one token, then validate effective min≤max before effects.
   Missing/zero/partial metadata uses existing AUD41 fallback/warnings; discovery
   metadata remains unknown, DISC05/admission caps unchanged. Never use context=0
   or DCP's max threshold as model capacity. Keep input/output/safety admission
   stronger than reminders; do not hardcode production model IDs or vendor rules.
3. **Total-active accounting and cadence.** The minimum check includes active
   summaries in both buffer modes. With default false, the upper check also uses
   total active estimated context, including summaries sent now. Opt-in true
   subtracts only those active summary estimates for the upper check, not archived
   summaries or actual request/model budgets. Below min: no context nudge; at min
   it is eligible subject to existing cadence. At max: no max-only escalation;
   strictly above max: strong/required reminder when cadence is due. Preserve
   iteration-threshold escalation, frequency and successful-compress reset/cooldown;
   invalid/no-gain attempts do not falsely reset them. Estimates are not exact billed
   usage. Both modes send summaries and preserve immutable raw history/protections.
4. **Reminder, not capacity.** 55% is an upper reminder criterion, not an enforced
   occupancy ceiling, automatic summary call or extension of model admission.
   `nudgeForce` is `soft|strong`; internal Hard means the stronger transient developer
   reminder, not a separate input limit. Preserve disabled/manual/effective Deny,
   child gates/isolation and active-task/context-pack protection. Native compaction
   remains separate. No hidden paid summarizer, config writes or automatic Git work.
5. **Truthful typed consumer.** Existing application/query/panel projection reports
   effective min/max reminders separately from model capacity, buffer mode and
   fallback provenance/warning. Correct the current panel's raw-model-ID/context=0
   path to share runtime resolution, not a second policy calculator. Threshold facts
   agree across selection/overrides/reopen/restart/Location generations; token estimates
   retain their declared method rather than pretending every UI estimate equals wire
   usage. Reuse bounded active queries and existing VIS38/resource evidence, no new
   store, archive-loading consumer, framework or whole-T45 completion dependency.
6. **Qualification.** DCP12 first tests no-DCP-file defaults, integer/percent/partial/
   model overrides, true/false buffer distinction, boundary/cadence/reset and invalid
   effective combinations. Then rebuilt actual binary with fake-provider requests and
   owner snapshots proves reminder strength and matching effective thresholds for
   known and fallback models, config precedence, restart and safe-boundary Location
   changes. Reuse DCP05/DCP07/DCP10/DCP11/AUD41 and A03/A07/A08/A10/A13; no duplicate
   long-horizon/negative matrix or new paid campaign. T45 owns DCP12 backend/facts;
   T44 independently qualifies presentation, including pending VIS38. Historical
   T19/T36/T39 reports, task statuses, baseline and existing evidence remain intact.

Standalone paths, parameter groups and partial-implementation distinctions are in
[CONFIG.md](CONFIG.md#dcp-configuration--approved-target). DCP12 and R9 remain
pending until actual qualification; this plan does not qualify other display flags.

## Approved transcript presentation — T44/VIS38 (pending)

Контракт: [T44 DCP amendment](../tui-recovery/T44_CONTRACT_AMENDMENT.md#dcp-compression-display--vis38).
Pinned display/accounting references D05–D10 и OC2 wrapper U34 — в
[`tui-recovery/SOURCES.json`](../tui-recovery/SOURCES.json). Это DCP compression,
не `/compact`/VIS34. Default detailed/chat после successful commit выглядит так
(иллюстративные числа, не результаты выполненного теста):

```text
┃ ▣ DCP | -4.2M removed, +84.8K summary
┃
┃ │████████████████████████████████░░░░░░░░██░░░░░░⣿█│
┃ ▣ Compression #8 -61.5K removed, +11.9K summary
┃ → Topic: Confirmed apply patch delivery
┃ → Items: 33 messages and 39 tools compressed
```

Блок находится между шагами агента в transcript. Pending показывает краткий
Compressing…/topic, commit заменяет его подтверждённым результатом. No-gain/error/
cancel/denial/unknown не рисуются успешным report. showCompression=true добавляет
сохранённый summary; minimal показывает header/run, off отключает notification.

Верхний removed — cumulative gross для session/current DCP revision, summary —
active-summary total; нижние метрики — run. Это token estimates с disclosed method,
не bytes, net saved или billed provider usage. Unique newly-covered message/tool
occurrences не пересчитываются при consumption старых summaries. Один multi-range
вызов получает один stable run/card; legacy metadata не заменяется выдуманными цифрами.
Карта 50 message positions: █ ordinary, ░ previously active-compressed, ⣿ newly
compressed; она не сообщает execution progress или долю tokens.

Shared native formatter использует integer/K/M, decimal base 1000, максимум один
decimal, half-up, trimmed .0 и rounded 1000K → 1M. Полные счётчики сохраняются;
4218800 → 4.2M, 1000000 → 1M, 999950 → 1M. M/promotion — owner-approved display
extension к pinned DCP K formatter, явно отмеченная в full-frame comparisons.

Historical card header/bar фиксируются при commit; reopen/restart и Undo/Redo
используют existing durable operation/history projections без нового model-visible
user message, duplicate report или compression replay. Current panel stats читают
текущую branch/revision. Session/child accounting изолирован; late events не меняют
чужой экран/итоги. Bounded queries/paging и компактный snapshot исключают full-archive
loads и lifetime UI caches ради счётчика/шкалы. UI04 no-duplicate-history сохраняется.
T44 владеет display/accounting qualification A07/A08/A10; T45 сохраняет R9/DCP10/11
алгоритмы. Исторический T39 panel/status-notice PASS не квалифицирует VIS38.

## Approved child/long-horizon contract — T45/R9 (pending)

This is a native addition, separate from OC2 automatic/manual compaction. OC2 core
has no allowSubAgents DCP key. Pinned DCP 3.1.15/current native RECON did not identify
a total successful-compression-call quota; the owner's historical OC1 error cause
remains unverified. Do not invent a cap removal or blame nudge counters for it.

- dcp.experimental.allowSubAgents missing means true; explicit false disables child
  model compress/anchors/nudges/automatic strategies, not root DCP. Global enabled,
  manualMode and effective compress permissions retain their semantics; Deny wins.
  Remove the obsolete unsupported warning only when actual behavior is implemented.
- Built-in Explore gets a narrow own-history compress grant while patch/bash/nested
  delegation stay denied. Custom explicit/wildcard Deny is not overridden. The
  orchestrator's owner-backed preview reports effective availability/ask conditions.
- Scope all block/anchor/projection/nudge/protection state to session/generation.
  A child cannot compress parent/sibling IDs; parallel sessions cannot share cadence
  resets or protection changes. Fixed profile/environment/AGENTS lanes stay separate.
- Current admitted delegation task/context pack is protected while pending/running,
  without turning quoted user context into system authority. Terminal completion
  releases the extra active-task protection; configured user/tag protections still
  apply. Safe recovery reinstates it before requests. Continuation cannot accumulate
  every old pack in permanent fixed lanes. Native compaction separately preserves/
  reconstructs active requirements; irreducible budgets are explicit failures.
- No default lifetime successful-call/block-count quota; stats are not admission
  counters. Invalid/no-gain calls do not spend a lifetime attempt. Per-call ranges/
  payload limits, cycle/ID/protection/tool-graph checks and active-memory/model/turn
  budgets remain; unlimited operation count is not unlimited RAM/disk/provider use.
- Planning/projection reads bounded active/addressed descriptors and selected payload,
  not every superseded block or every historically covered member ID. Retain raw
  history for native durability/history, but do not require it as live context ancestry.
  Recovery/compress must not demand an over-budget full-before/archive read.
- Summary guidance preserves selected useful facts/paths/current decisions/open
  questions/next step, not all previous summaries. User `.md`/Git cold memory and its
  references are optional; no automatic journal writes/Git commits, hidden recall,
  paid DCP summarizer or regex extraction of free model text. Compression is optional
  context management, not a mandatory call after each read. The following amendment
  defines intentional forgetting and manual `/compact` compatibility.

DCP10 covers defaults/gates/parallel-session isolation/active-pack lifecycle.
DCP11 repeats compress/continue/recompress/restart and compares equal active context
over growing inactive block archives with row/depth/peak/retained-state measurements.
Keep A07/A10 thresholds and actual-binary cleanup. A finite frozen workload is
evidence, not a product session-lifespan quota or proof of infinite resources.

## Infinite hot context / optional cold path — T45/R9/DCP11 (pending)

**Source/finish line (owner-approved 2026-09-30):** hot path is the current model
context window; long-horizon work continues indefinitely by repeatedly throwing
away useless context. Cold path in user `.md` files/Git history is entirely optional.
The objective is renewable working memory, not a lossless summary archive. This
amends R9/DCP11 hot-retention semantics; implementation and qualification are pending.

### Replacement and forgetting

An admitted `compress` replaces the selected closed hot span with a standalone
summary of chosen working facts. The resulting hot projection contains the current
task/state, chosen summaries and unchanged fresh tail; content outside the selected
span and fixed lanes is neither discarded nor copied into every new summary.
Compression does not add a summary while retaining the same old range as a hidden
model-input obligation. Prior summaries may themselves be
rewritten, reduced or forgotten. A useless span need not leave useful semantic
content behind; the existing bounded nonempty summary shape can use a concise
closed-work marker rather than introduce a second forget tool or alternate schema.

- Consume all fully covered old blocks in the selected span, not only blocks whose
  placeholders happen to appear in the new summary. Omitted unprotected old content
  is intentionally absent, not automatically appended as a missing-block fallback.
- Explicit authored placeholders retain their meaning while preparing the new
  payload. Resolve them once into chosen hot content; the committed replacement
  does not depend on an aging parent graph. Literal placeholders in protected text
  must not be reinterpreted after insertion, and protection headings must not absorb
  unrelated later text. Use structured existing block/offset metadata, not semantic
  keyword extraction from LLM prose.
- Keep stable IDs/provenance/raw records in existing durable history. They do not
  force loading old content or serializing every covered message ID into a candidate.
  Use bounded endpoints/span/consumption descriptors and SQL validation/transfer;
  candidate placement, wire measurement and commit must agree on the same coverage.
- Legacy valid stored chains need a bounded, addressed transition to the new live
  representation when used/reopened, not eager normalization of the whole archive.
  Any minimal persisted representation change joins existing context versioning,
  Undo/Redo/fork and revision checks. Do not delete raw records/reachable history or
  silently accept a failed expansion to obtain green tests.

### Real wire and resident-memory removal

**Current-task seam (2026-10-02 audit; pending):** renewal also applies inside one
long-running root/child task, not only between user turns. Current `TurnLog` retains
all request/span/part/input/opaque history; `checkpoint_turn` overwrites the complete
`turns.result`. Evicting that prefix and checkpointing it would lose canonical raw
history; tool-operation rows and conversation versions are not its full archive.
Current compaction uses message boundaries and protects the started turn, while
partial continuation reloads the whole journal. Existing DCP wire filtering alone
does not establish bounded resident retention.

Resolve a safe closed intra-task boundary, independent durable raw preservation
and bounded committed hot restoration at the existing runtime/storage/checkpoint
owner **before eviction**. Reuse mechanisms where sufficient; schema-free feasibility
is not established. No second history store/service is chosen in advance, and raw
records, current in-flight groups, effects and effective protections cannot be
discarded to obtain boundedness. Qualify retry/model switch/DCP/compact/recovery
against the same hot view without full historical reload. Measure checkpoint
processing/I/O and peak/retained RAM at equal hot state with increasing past steps.
The independent removal of 8/16 round stops in T45/R6 is not DCP11/A10 PASS.

Forgetting applies to the actual provider continuation, not only the ordinary
message-text projection. Closed tool-call/result/reasoning groups, obsolete media,
opaque items and associated hot marks leave as complete groups when their span is
replaced and they are not selected/currently required. Preserve the needed outcome
in the new working summary, not every historical argument/output. The currently
executing batch remains intact; durable known/unknown operation state is not replayed
or erased by hot forgetting.

Runtime-added task/pack protection has an owner and an active lifecycle. Preserve
the admitted pending/running task, reinstate it before safe recovery requests, and
release extra protection on terminal completion. Replaced tasks/requirements and
completed packs do not survive through inherited fixed lanes/protected suffixes.
Re-evaluate applicable effective user protections on the admitted scope/generation;
do not carry obsolete protection solely because its bytes occur in an old summary,
and do not silently override a still-explicit user policy. Default hot renewal does
not enable blanket lifetime verbatim protection of all past user/tool data.

Release superseded live objects, expansion buffers and inactive tool decisions;
select only marks needed for the chosen wire. Occurrence identity includes reused
`call_id` occurrences, not a prefix-local renumbering that changes their decisions.
SQL-side storage is not permission for unbounded Rust materialization or repeated
full-archive copying. Inspect DB/WAL/I/O amplification as well as process RAM.

### Optional cold memory and restoration

Compression/continuation/restart must work with no user journal files, checkpoint
Markdown or Git repository/history. DCP neither writes such artifacts automatically
nor creates a cold exporter/index/RAG service. If the user chooses cold notes, normal
authorized file/shell operations remain available independently of compression.
Missing cold references are not an admission dependency for current hot state.

Existing native persistence and UI/Undo history are separate from model memory;
this amendment does not introduce raw-DB retention/deletion policy or disable
durability. Restore exactly the latest committed hot representation after restart.
Ordinary continuation, DCP and compaction do not fetch forgotten text from raw
history, an old block or a cold file. Explicit admitted read/context-message
selection or conversation Undo may deliberately reintroduce selected information;
that is not automatic resurrection. R8 quotation/trust checks and conversation-only
Undo/Redo remain authoritative, with no file/Git rollback.

### Manual `/compact` and admission/recovery

Manual `/compact` is a wider rebuild of the same working memory. It consolidates
the previous checkpoint, chosen current facts and eligible work into one standalone
replacement checkpoint plus fresh tail. Obsolete checkpoint contents can be forgotten;
there is no mandatory chain of old checkpoints or archival DCP re-expansion. Existing
native-capability/auxiliary compaction routes consume admitted hot input, never raw
archive to recover omitted facts. Model-authored DCP still has no hidden second call.

- Advertise only DCP-resolvable anchors, not the synthetic `session-checkpoint` ID.
  Archived addressability for explicit user selection is distinct from active anchors.
- Before/after measurement includes the same unchanged checkpoint, fixed and current
  lanes and counts real retained call/result/media wire. Do not credit an unchanged
  checkpoint as removed or claim savings from dropped accounting only.
- Form reminders from the final projection after any compaction. Stateful cadence
  advances/evaluates once per logical iteration; no stale hard nudge, double emission
  or indiscriminate reset of IDs/archive/session state.
- Load only relevant tool marks and align selection/application/checkpoint retirement
  by occurrence identity at one context revision. Qualify reused IDs and a block
  straddling cutoff, then a later advancing compaction, before changing the boundary
  algorithm. Straddling alone is not an established dead end.
- Acquire floor/revision/closed-group metadata before content. Planning and measurement
  do not require full successful admission/materialization of overflowing `before`.
  Forgetting an entire eligible historical group need not load its huge payload.
  Count actual wire without a full oversized serialization copy; raw text size alone
  is not wire/model cost. Keep currently required task/input/protocol groups intact.
- Qualify the user's existing `/compact` control after host-memory overflow as well
  as model-budget overflow. `/dcp-compress` is an ordinary admitted model turn and
  direct `run_compress` alone cannot prove that escape path. Show measured recovery,
  honest partial progress or current-state/policy failure, not an unavailable
  "compress to proceed" instruction. No hidden multi-request chunk-summary loop.
- Invalid/no-gain replacement leaves the prior hot state intact and does not consume
  lifetime capacity/reset cadence. A wider eligible span or smaller chosen replacement
  remains available; no persistent retry trap or successful-block counter as a gate.
  Operation safety checks are not an obligation to retain intentionally forgotten
  closed history. Unadmitted clipping/error truncation is not legitimate forgetting.

### Work slices, minimum evidence and done

Use the existing T45/R9 owner and DCP10/DCP11/DCP12 IDs. The ordered, independently
checkable slices are in [M8](../roadmap/M8.md#infinite-hot-context--optional-cold-path--t45r9dcp11-approved-2026-09-30-pending):
standalone replacement; full-wire/protection release; compact/recovery seam; one mixed
qualification. Reproduce a kept fact and an intentionally forgotten sentinel before
each affected change. No new memory framework/store/worker/cold tool/paid campaign;
T44 remains PAUSED and has its own presentation qualification. A plan-only delivery
does not interrupt active T50 or mark implementation evidence PASS.

Freeze one actual-binary fake-provider workload before qualification:

```text
compress → continue → recompress → compact → continue
→ compress → restart → compact → continue
```

Each cycle adds bounded closed work, so there is real scope for renewed shrinking;
do not demand infinitely shrinking an unchanged finite string. Keep control facts
for the current objective, changed requirement, selected path/result and next action.
Forget sentinels in a prior summary, superseded requirement, pointless investigation,
large closed tool/media group and released runtime task/pack. A concise disproof reason
is kept only if still useful; no obligation to remember every failed experiment.

Capture outbound requests and committed projections, not only a UI estimate or
successful tool report. Assert kept facts and deliberate absence from all following
hot requests through recompression/compact/restart; explicit Undo/read is a separate
case. Exercise root/parallel-child isolation by reusing DCP10 lifecycle fixtures.
Run with cold memory absent; an explicitly read cold note is opt-in, not a second
mandatory workload or automated archive fallback.

Cross the former depth pattern and more than 4096 historically covered IDs while
keeping the same small live state; include unrelated inactive tool marks. Compare
small/large inactive archives with equal hot wire. Measure loaded rows/bytes/live
dependency depth, peak/retained RAM, task/process/queue cleanup and DB/WAL/I/O growth.
Reuse `dcp_atomic`, `context_bounds`, runtime/context/compaction tests and actual-binary
`oc/tests/dcp_runtime.rs`; a minimal PTY smoke proves real `/compact` plus continuation,
not T44 visual parity. Host/model overflow, reused IDs, stale/cancel/no-gain/cycle and
current protections use the nearest existing targets without a duplicate full matrix.

**Done:** the replacement really removes selected useless data from hot wire and
resident state; selected current facts remain usable; no ordinary compact/restart
resurrection or age/depth/member/mark exhaustion; the same session demonstrably
continues after repeated mixed cycles and reachable recovery with no cold files.
Relevant targeted, actual-binary and existing required workspace/resource gates pass
without raising thresholds. This closes the qualified DCP11 outcome, not all T45,
DCP10/DCP12, T44 or product READY. Historical evidence and donor baseline stay factual;
a finite cycle count measures the invariant, never defines the product lifespan.

## Acceptance и source trace

Нужны upstream-derived fixture groups: range boundaries/nested IDs/protections, strategy timing, nudges/counters, config merge, cancelled/failed compress, restart projection и compression-before-tools continuation. Test method фиксируется: source-derived / differential executed / synthetic, никогда один под видом другого.

Fake-model scenario заставляет вызвать compress на большом закрытом span, затем потребовать выбранный fact из summary и выполнить patch/test. Assert immutable raw history checksum, stable IDs после restart, уменьшенную real wire projection, выбранные facts/current protected bytes, целостность retained tool groups и deliberate absence забытых закрытых групп, отсутствие config/provider credentials и restart consistency. Дополнительно квалифицировать no-resurrection mixed workload DCP11; один прежний E2E его не заменяет. Live regression проверяет реальный endpoint, но не обещает универсальную semantic losslessness.

## Квалифицированный native path (T36)

Следующий раздел описывает historical T36 implementation/evidence, не новый hot/cold
target. В частности, прежнее сохранение tool payload/protected ancestry и nested
consumed blocks не квалифицирует approved 2026-09-30 replacement/forgetting.

Normal `oc run`/TUI runtime публикует `compress` как ordinary function tool рядом
с `glob`/`grep`; все три проходят общий validation → permission → durable intent →
dispatch → durable outcome. DCP developer lane содержит bounded ordered anchors
`{id,role,closed}` и transient nudge, но не записывается в raw history. Disabled,
manual или неразрешённый compress не публикует schema/anchors/nudge.

Модельный compress сначала строит весь plan. Одна SQLite transaction сохраняет
blocks/members, consumption старых memberships, durable dedup/purge projection,
tool outcome, turn wire journal и reset nudge state. No-gain не создаёт block и
не сбрасывает cadence. После commit следующий round заново строит projection.
SIGKILL regression останавливает actual binary на durability sync внутри этой
transaction и после restart допускает только zero-or-complete state без replay.

Nudge state durable и изолирован ключом session/provider/model. Percent limits и
exact provider/model overrides вычисляются от context limit выбранной модели;
summaryBuffer=true вычитает активные summary только из верхнего nudge estimate,
но не из первого minimum check или model admission. Это историческое buffer
поведение, не новый default и не расширение hard input boundary.
`nudgeForce` принимает `strong|soft`. Успешная compression сбрасывает cadence
только своего ключа. Manual mode отключает autonomous strategies/reminders.

Dedup/purge решения появляются только вместе с успешной compression и переживают
restart. Identity включает `call_id` и occurrence, поэтому reuse ID не смешивает
вызовы. Dedup использует canonical JSON, purge удаляет только большой старый input
ошибочного call и сохраняет exact outcome. `*`/`?` tool protections, typed file
paths (для patch — parsed affected paths), recent-turn protection и user/tag/file
content сохраняются. Complete call/output pairs никогда не рвутся. Block anchors
могут быть вложенными; consumed block rows остаются для bounded expansion, а их
active memberships заменяются транзакционно.

Historical T36 qualification: `dcp.json/jsonc` и inline `dcp` читались в общем порядке admitted config roots,
read-only, bounded 1 MiB, regular UTF-8, no-follow. Deep object merge поддерживает
required surface выше. `autoUpdate:true` and unsupported modes/custom prompts
remain explicit failures. Subagents were not qualified by T36 (later native config tolerated allowSubAgents
with a warning). This does not implement R9 or override its new target default.
`showCompression`, notification и commands display сохранялись в snapshot, UI
controls квалифицировались T39 в прежнем объёме. Новый detailed transcript,
typed display modes, session-scoped durable metrics/bar и K/M требуют отдельной
T44/VIS38 qualification; исторический отчёт не является их PASS.

Offline evidence: `evidence/T36/report.md`. Полная archive materialization и
process-wide lifetime/RSS gates не считаются закрытыми этим срезом. T40 measured
active-history construction; T45/R9 additionally owns repeated-summary/archive-block
qualification. Historical one-compression evidence is not sustainable-DCP PASS.

До переноса кода/prompts/tests сохранить license/notices/provenance. Rust перевод не удаляет лицензирование источника. Не копировать unrelated OpenProxy code с неустановленной лицензией.
