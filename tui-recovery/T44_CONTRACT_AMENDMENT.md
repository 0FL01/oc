# Уточнение контракта T44 по текущему запросу владельца

## Приоритет

Этот документ уточняет `docs/goals/2026-09-21-tui-pixel-parity.md`.
Сохранить рабочую ветку, историю checkpoint, четыре crates и существующую реализацию.
Внести небольшой reviewed diff в активный goal; не переписывать старые reports задним
числом. Новая пользовательская инструкция имеет приоритет над self-authored запретом
«не добавлять requirements from reviews». При этом случайные советы не становятся scope.

## Owner amendment 2026-09-26: conversation-only Undo/Redo

**Утверждено владельцем:** `/undo` и `/redo` возвращают разговор и прошлые точки
LLM-контекста, **не изменяя workspace, файлы, права, Git index/HEAD или внешние эффекты**.
Файловую историю владелец контролирует обычным системным Git через shell. Это сознательное
отличие от оригинала с включёнными snapshots; отменяет прежнее требование R5/VIS10
«Revert including file changes». Остальные требования T44 не отменяются.

Источник решения: уточнение владельца «НЕ ТРОГАТЬ содержимое воркспейса, файлы и т.п»,
привычный режим `"snapshot": false`, затем «План утверждаю вноси правки в план работ
и коммит пуш». Этот delivery — документы и план, **не разрешение автоматически
возобновить припаркованную реализацию** и не claim рабочего undo.

### Поведение

- `/undo` выбирает последнее предшествующее user message с непустым текстом, как OC2: запрос и
  весь последующий ответ агента с tool steps перестают входить в активный разговор
  и следующий provider context. Исходный запрос возвращается в prompt для редактирования.
- Утверждённое уточнение паритета: `/redo`, palette `session.redo`, настроенный shortcut
  (по умолчанию `ctrl+x r`) и клик по карточке reverted снимают **всю staged-границу**
  и возвращают весь сохранённый хвост, как pinned OC2. Это заменяет прежний пошаговый
  Redo. Ответы и контекст возвращаются **без генерации, provider-вызова и повторного
  исполнения tools**; запрет восстановления workspace/файлов/Git сохраняется.
- Revert из Message Actions устанавливает ту же conversation-only границу перед
  выбранным user message и восстанавливает его prompt; никогда не восстанавливает файлы.
- VIS33 проверяет путь user-message click → Message Actions → Revert → reverted block
  → Redo. Карточка показывает количество отменённых user messages из durable owner,
  hover и keymap-derived hint; выделение текста блокирует восстановление по клику.
  Успешный Redo убирает карточку целиком; ошибка сохраняет committed boundary и feedback.
  Состояние/счётчик восстанавливаются при reopen, не выводятся из одной страницы истории.
- Новая принятая отправка после undo создаёт новую активную ветку и прекращает обычный
  redo старого хвоста. Raw messages/turns/events сохраняются; не удалять архив ради UI.
- История, tool-call/result пары и DCP-проекция (summary blocks, pruning/exclusions)
  используют одну причинную границу. Более поздняя summary не может вернуть отменённый
  ход в старый контекст. Состояние undo/redo и контекстная точка переживают restart.
- Это восстановление сохранённого клиентского контекста, не удалённой памяти/KV-cache
  LLM-сервера. Workspace может оставаться новым при старом разговоре — намеренно.
- При активном выполнении сначала interrupt и дождаться остановки/cleanup, затем
  переключать границу; не допускать поздних событий в неверную активную проекцию.
- Snapshots выключены по умолчанию, файловый capture/restore не реализуется.
  Принимать `snapshot:false` и `snapshots:false`; `true` даёт явную диагностику
  неподдерживаемых файловых snapshots, не скрытое включение. Legacy snapshot-ссылки
  не являются разрешением на файловое восстановление.

### RECON и план исполнения после отдельного возобновления

Pinned `opencode/packages/core/src/config/normalize.ts:70–91` нормализует `snapshot`
в `snapshots`; `config/plugin/snapshot.ts:15–18` применяет настройку;
`snapshot.ts:115–118` прекращает capture при выключении. TUI `routes/session/index.tsx:898–945`
сохраняет Undo/Redo, вызывая `revert.stage`/`revert.clear`. В `session/revert.ts:23–75`
файловое восстановление отделено от изменения границы; наш путь вообще не вызывает restore.

1. Разобрать припаркованный diff по reviewed hunks. Убрать экспериментальные whole-tree
   snapshot hooks/module/storage/workers; **не заменять их preimage-журналом**. Сохранить
   независимые identity/fork изменения и полезные shell fixes для отдельной qualification.
2. В application/storage owner добавить долговечную активную границу/ветку и ссылки
   на уже сохранённые сообщения и версии контекстной проекции. Не копировать весь
   диалог на каждый ход, не сканировать/хешировать workspace.
3. Согласовать history paging, следующий provider input и DCP versions с этой границей.
   Tool results возвращаются как история, не как задания для повторного исполнения.
4. Добавить typed owner операции и `/undo`/`/redo`/Message Actions Revert в TUI;
   показывать доступность и результат настоящих операций, не visual-only placeholders.
5. Проверить последовательные undo и whole-tail redo, новую ветку, restart, DCP crossing boundary, отсутствие
   provider/tool вызовов при redo, causal tool pairs и остановку активного выполнения.
   Проверить неизменность workspace bytes/modes и пользовательского Git index/HEAD.
   Сравнивать применимые UI frames с оригиналом при отключённых snapshots; минимум три
   завершённых user turns и отмена нескольких сообщений различают whole-tail и one-turn
   Redo. Отдельно проверить клик карточки, shortcut, slash и palette без масок/ложного PASS.

### Pinned references для VIS33

Commit: `2670273ff17da96f85c5826ced57aa1b368754fa` (OC2 v2.0.12).

- [User click / selection guard](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/index.tsx#L2307-L2342).
- [Message Actions / Revert](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/dialog-message.tsx#L23-L49).
- [Undo / Redo commands](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/index.tsx#L898-L945).
- [Reverted block click / hover / hint](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/index.tsx#L2172-L2250).
- [Keybindings](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/config/keybind.ts#L189-L190); default leader is `ctrl+x` at line 41.
- [Owner stage / clear](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/revert.ts#L23-L75).

Expected paths: `oc-core` typed operations/queries, `oc-adapters` application/storage/runtime/DCP
and config, `oc-tui` commands/app/history; remove only reviewed experimental snapshot code.
Targeted owner/provider/PTY tests, затем affected-crate checks и обязательные workspace gates.
Текущий статус: **план утверждён, реализация pending/paused**. Файловый undo, full-tree
checkpoints, preimage storage, staged filesystem Revert/Clear и автоматический Git вне scope.

## Canonical effort ordering — VIS09/VIS29

**Owner-approved 2026-09-27 after RECON; plan pending/NOT_RUN.** Existing T47 R6/VAR01
owns the shared behavior, T44 owns presentation under VIS09/VIS29; no new task or
whole-T47 completion dependency. This supersedes only the old pinned-declared-order
acceptance for variants, not the remaining UI parity requirements or historical reports.

1. Consume [Canonical effort ordering](../docs/CONTRACTS.md#canonical-effort-ordering--t47var01-approved-2026-09-27-pending)
   after effective discovery/local/static merge: Default → none → minimal → low →
   medium → high → xhigh → max → custom → Default, skipping absent/disabled/reserved
   exact `default`. These are supported-level ranks, not hardcoded capabilities.
   Explicit known effort wins over name; absent effort may rank by standard name;
   explicitly unknown effort remains custom. Exact case/whitespace unknown spellings,
   aliases, equal-rank ties and custom source order follow the shared contract.
2. Existing application snapshot, model/variant picker, Ctrl+T and applicable enabled
   diagnostics use one policy, no independent lexical sort. Preserve exact IDs/wire
   values and selected identity, not an index; sorting cannot synthesize reasoning.effort.
   T45 primary/subagent selection and T50 opencode_models consume the same facts, not
   a second ordering owner; model/provider grouping order itself is unchanged.
3. Actual binary PTY proves identical picker/cycle sequence in Home/session, full
   round trip, no model-ID/text-draft change or request on shortcut, read-only/authority
   guards. The 2026-10-01 live-selection clarification supersedes only model/variant
   busy refusal: local draft, captured send/blank Enter commit and next-request adoption
   inside the same task; prepared requests/tools keep actual variant. Preserve
   Default no-overlay distinct from named none, no-named/all-disabled no-op and
   explicit stale-cycle → Default. Fake Responses next prompt carries exact selected
   declared effort; ranking by name without effort does not invent wire data.
   Reorder/refresh/reopen/restart preserves chosen ID; retirement/disable stays visibly
   actionable until explicit recovery, with no automatic substitution.
4. First deliver minimal shared ordering/effective-snapshot VAR01 slice; then pair
   running pinned original/native full styled-cell/PNG/cursor frames with identical
   canonically supplied variants to verify geometry/colors/focus/draft/selection.
   Separately capture identical **unsorted** supplied variants: OC2 preserves declared
   order, native normalizes known efforts. Label that approved difference without
   hiding/reordering capture rows, masks or universal donor pixel PASS. Reuse existing
   VAR01 protocol/durability and VIS09/11/17/21/22/23/29 fixtures, not duplicate matrices.

Pinned OC2 `2670273ff17da96f85c5826ced57aa1b368754fa` sources U66–U68:
`packages/tui/src/component/dialog-variant.tsx:10–30` (Default plus declared names),
`context/local.tsx:472–498` (catalog names/cycle/persist),
`model-preference.ts:67–75` (default/named/stale/empty cycle).
Native RECON: `application.rs::EffectiveSelection::snapshot` preserves variants iteration;
`oc-tui/src/picker.rs::variant_options` preserves it, `ModelPicker::variants` separately
sorts names; `oc/src/tui_cmd.rs::CycleVariant` traverses the owner's Vec. JSON already
uses preserve_order. Remove the competing sort when implementing the shared view;
do not sort all config objects, mutate discovery oracle or deduplicate aliases by effort.
Targeted checks and A03/A04/A08/A13/integration workspace gates remain required;
no extra paid campaign, current execution-status/baseline change or PASS from this plan.

## Owner amendment 2026-09-24: inline autocomplete

Новая инструкция владельца (2026-09-24) добавляет в R5 два обязательных элемента.

1. **Slash autocomplete overlay:** при вводе `/` в prompt появляется inline-список
   подходящих команд (built-in registry и workspace-команды текущей generation) в session
   и на Home; Up/Ctrl+P и Down/Ctrl+N перемещают выбор, Tab дополняет, Enter исполняет
   команду без аргументов либо вставляет `/alias ` для команды с аргументами, Esc
   закрывает список, сохраняя draft. Пробел между триггером и курсором скрывает список,
   пустой фильтр показывает no-match состояние. Источник: pinned `autocomplete.tsx`
   (modes, trigger wiring, выбор) и `prompt/display.ts` (`slashTriggerIndex`), keybind
   `prompt.autocomplete.*`.
2. **`@` file mention overlay:** при вводе `@` (в начале строки или после пробела, без
   пробелов в query до курсора) появляется список файлов относительно текущей Location;
   выбор вставляет относительный `@path`. Модель читает файл штатным инструментом `read`:
   mention остаётся текстовым, а не структурированной prompt part — A13 требует skill
   body только как bounded result `skill`, а parts без расширения storage нарушили бы
   live/replay консистентность (C5/A09).

Сценарии: **VIS25** и **VIS26** в ACCEPTANCE.json (step V05, mandatory, NOT_RUN).
Срезы реализации, по порядку: (1) slash overlay в `oc-tui`; (2) bounded
location-implicit file query (Files-level: skip VCS/build, truncation вместо
`BudgetExhausted`, только относительные пути); (3) `@` overlay; (4) paired capture probe
(`--autocomplete`, Tab-only, record-only предикаты). До реализации полные визуальные
gates VIS25/26 остаются открытыми.

Зафиксированные supported differences (не маскируются):

- ranking: существующий fuzzy-движок без frecency/fff упорядочивания эталона;
- `@` без секций skills/agents/subagents: non-primary agent exposure (`!hidden &&
  mode !== "primary"`) принадлежит scope T45, VIS26 проверяет файлы;
- описания workspace-команд в списке — только после расширения CatalogSnapshot
  (не обязательны для gate);
- mentions передаются текстом (`@path`), structured prompt parts не планируются.

## Уточнение R5: выделение и копирование текста мышью

Повторить pinned upstream v2.0.12: `terminal.copy` принимает `select`/`manual`;
по умолчанию `select` на Linux, `manual` на Windows (`packages/tui/src/app.tsx:561–562`).
В `select` непустое выделение копируется на `mouse-up`, только если событие имеет
`isDragging`; обработчик не проверяет кнопку. Это включает выделение перетаскиванием
и повторными щелчками для слова/строки без перемещения указателя
(`packages/tui/test/util/selection-copy-on-select.test.tsx:50–60`).
В `manual` существующее выделение копируется по `mouse-down` ПКМ
(`packages/tui/src/app.tsx:1315–1325`). Пустое выделение не копируется;
подсветка остаётся. После успешного результата записи показывается info-toast
`Copied to clipboard`, при ошибке — ошибка, не success-toast
(`packages/tui/src/util/selection.ts:34–62`). Не подменять запись показом toast.
Обязательный сценарий — VIS27; системный clipboard конкретного терминала/SSH
нельзя считать проверенным только по PTY-кадру.

### Дефект VIS27, обнаружен 2026-09-25 10:03:13 UTC: toast поверх названия чата

При копировании выделения `Copied to clipboard` перекрывает заголовок чата
в видимом sidebar, но буквы заголовка остаются в отступах toast и сливаются
с уведомлением. На момент RECON дефект не исправлен: native
`shell.rs::render_toast` задаёт внутренней области `background.raised.high`
через `Block::style`, который меняет стиль ячеек, не очищая их символы.
Исправить у владельца поверхности toast: очистить внутреннюю область перед
заливкой и текстом, сохранив фон, боковые границы и расположение. Не менять
для этого обработчик выделения и копирования.

До закрытия VIS27 проверить на ширине с видимым sidebar (например, 121×40)
длинный заголовок под toast: в отступах и промежутках уведомления не должно
оставаться букв чата; фон покрывает перекрытую область. Добавить ближайший
render regression и повторить парный upstream/native PTY-захват с реальным
копированием. Прежний захват 120×40 со скрытым sidebar этот дефект
не проверял; VIS27 не переводить в PASS по нему.

## Уточнение R5: индикатор работы агента

В обычном session prompt pinned upstream v2.0.12 под строкой метаданных агента/модели
нижний левый footer показывает индикатор перед `esc interrupt` только при
`session.status === "running"` (`packages/tui/src/component/prompt/index.tsx:1825–1837,1884–1900`).
Это не анимация текста метаданных и не spinner сообщения или вкладки. Индикатор —
восьмиячеечный blocks/trail от цвета текущего агента (fallback `theme.border.base`),
с кадрами раз в 40 мс: движение слева направо, пауза, обратно, пауза
(`packages/tui/src/component/prompt/index.tsx:1624–1641`,
`packages/tui/src/ui/spinner.ts:25–81,272–328`). При отключённой анимации
upstream показывает `[⋯]` вместо движущегося индикатора, сохраняя interrupt hint;
по выходу из running индикатор исчезает. Не хардкодить OCR-строку с именами
агента/модели. VIS28 проверяет живую последовательность кадров, состояние без
анимации и завершение/прерывание; один кадр или только animation-off — не PASS.

## Заменить ослабленное Material Decision

Pixel-perfect — не «наши тесты совпадают с нашими expected».
Это соответствие запущенному pinned upstream на одинаковых fixtures, state и terminal
profile: layout, content, exact cell symbols/styles/colors, cursor, dialogs и interaction.
Парные PNG служат внешним визуальным подтверждением; парные styled-cell dumps —
воспроизводимым low-noise gate. Собственный TestBackend golden остаётся regression test,
но не источником истины о внешнем приложении.

Original Bun/Node/build dependencies допускаются только в отдельной reference/test
среде. Запрет Node/Bun в Rust production не запрещает запуск эталона в тестах.
Можно использовать уже установленный original executable с проверенной версией/hash.
Если он недоступен — capture gate BLOCKED_REFERENCE; остальные независимые исправления
можно делать, но R3/R4/R5 не становятся verified автоматически.

## Скорректировать статусы

R1: оставить source inventory, исправить commit-vs-tree термин, добавить capture lock.
R2: palette unit tests сохранить; final rendered colors — пока unverified до paired frames.
R3: implemented-partial/unverified до sidebar, dynamic viewport, real dialogs и paired captures.
R4: implemented-partial/unverified до tables и идентичного replay/live projection.
R5: pending до real keyboard/editor/dialog flows.
R6: pending до full rerun на финальном code SHA; сохранить failed/flaky попытки.

`progress.py` остаётся единственным task-state owner; R-status — детализация T44, не
второй task tracker. `ACCEPTANCE.json` здесь содержит спецификации, а не PASS результаты.

## Primary profile selection parity — VIS06/VIS10/VIS17

Owner-approved 2026-09-27 after RECON. Backend/CLI contract: R6 in
`docs/goals/2026-09-21-config-compat-and-subagents.md` (T45).
Pinned sources U25–U30 in `SOURCES.json`; UI source U29. No new task tracker.

1. Use T45's real Build/Plan/custom catalog and application-owned session selection.
   Match visible primary/all picker/cycle eligibility; exclude hidden, disabled
   and subagent-only profiles from automatic selection surfaces as in OC2.
2. Shift+Tab cycles eligible profiles rather than opening the picker; `/agents`,
   palette and agent.list open the real picker. Match configured bindings, cycle
   order/wraparound and empty eligibility without inventing a fallback profile.
3. Preserve complete draft/focus and actual model/variant/session selection.
   Prompt color comes from admitted profile metadata, not an invented color index.
   Existing display titlecase/width fitting and no-success-toast contracts remain.
4. Qualify Build → Plan → custom → Build using captured outgoing requests and actual
   permissions/effects, including Plan enter/leave context. Reopen/reload and
   compaction/Revert retain or reconcile the correct profile/reminders; labels alone
   do not prove execution parity. Do not permit autoaccept to bypass effective Deny.
5. Extend VIS06/VIS10/VIS17 with paired original/native full styled-cell/PNG picker
   and prompt states plus owner/runtime evidence. T45 backend and T44 visual results
   remain separate; no circular task-completion dependency or PASS by approval.

**Built-ins/MCP clarification, approved 2026-10-01; pending:** T45's
[R3/R6/R8/R10 amendment](../docs/goals/2026-09-21-config-compat-and-subagents.md#built-in-profiles--default-mcp-access--approved-2026-10-01-pending)
supplies actual Build/Plan primary and General/Explore subagent registration, Plan
reminders/permissions and truthful default-all-MCP capabilities. VIS06/VIS10/VIS17
consume real no-custom-profile Plan selection/replay; VIS26/VIS39 consume the distinct
subagent catalog. Keep explicit Deny/Ask and user-profile eligibility; do not claim
native Explore MCP access is donor permission parity or implement another UI policy
store. Reuse CTX02/PRM01 behavioral receipts, preserve paired visual requirements.
No new VIS gate, all-T45 dependency, status/PASS rewrite or implicit T44 resume.

### Agent-cycle keybindings — уточнение 2026-09-30

**Owner-approved; T44/R5 + T45/R6; implementation pending.** Владелец сообщил,
что Shift+Tab в Rust открывает выбор профиля, тогда как OC2 переключает его сразу,
и попросил выяснить назначение Tab относительно OC1. После RECON утверждено:
«Вноси в план работ развернуто и коммит пуш в текущую ветку». Это детализация
существующих VIS06/VIS10/VIS17, не новый VIS45, задача или keyboard framework.

#### RECON: неправильный action, отсутствующий cycle и назначение Tab

Native на `26ef8ab92e46ddb8f41d97d670e374c98d9433bc`:

- `oc-tui/src/commands.rs:159–181,289–295` назначает `shift+tab` команде
  `agent.list`/`OpenAgents`; `events.rs:120–151` маппит BackTab+Shift в
  `KeyAction::Agents`, а plain Tab — в `KeyAction::Tab`.
- `app/input.rs::handle_key` вызывает `run_command(OpenAgents)`, который открывает
  `TuiPanel::Agents`. Plain Tab завершает slash/mention suggestion; без неё no-op.
  Forward/reverse agent-cycle action отсутствует: заменить текст hint недостаточно.
- `oc-adapters/src/config.rs::ConversationKeybinds::merge` не читает agent list/
  cycle/reverse ни с dotted, ни с legacy names. Поэтому добавление `agent_cycle`
  или `agent.cycle` в нынешний пользовательский config само по себе не исправляет UI.
- `application.rs::Selection::catalog` фильтрует `primary_capable()`, но
  `defs.rs::AgentDef::primary_capable` проверяет только mode != subagent, не hidden.
  `AgentEntry` уже отфильтрован у владельца; TUI не имеет hidden/disabled facts.
  Нужен минимальный R6 catalog slice, не угадывание eligibility по ID/description.
- Existing Enter в picker возвращает `PanelIntent::SelectAgent`; бинарный
  `tui_cmd.rs::apply_intent` использует общий `selection(..., SelectionAction::Agent)`
  для session и Home. Owner validation, persistence, effective model/variant,
  no-success-toast и error handling уже доступны; переиспользовать этот путь.

Pinned OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`:

- **U29/U76:** `agent.list` = `<leader>a`, `agent.cycle` = `shift+tab`,
  `agent.cycle.reverse` = `none`. `app.tsx` разделяет picker и `local.agent.move(1|-1)`;
  `context/local.tsx:56–123` исключает hidden/subagent-only, сохраняет supplied order
  и оборачивает цикл. `test/keybind.test.ts:4–6` проверяет эти defaults.
- **U76:** legacy names `agent_list`, `agent_cycle`, `agent_cycle_reverse` маппятся
  в dotted names. `config/v1/keybind.ts` внутри OC2 уже имеет новые defaults:
  это compatibility importer, не самостоятельное доказательство defaults OC1.
- Историческая привычка владельца подтверждается [официальной OC1 keybind docs](https://opencode.ai/docs/keybinds/)
  (просмотрено 2026-09-30): Tab = next, Shift+Tab = previous, `<leader>a` = picker.
  Это контекст explicit override, не frozen native default или pinned OC1 runtime.
- **U77:** normal prompt объявляет capture `tab`, autocomplete имеет собственный
  mode/layer. Таблица config и этот source-derived capture не доказывают весь
  OpenTUI dispatch. Как работает configured Tab при idle/autocomplete, проверить
  на запущенном pinned OC2; не обещать глобальный fallthrough по одному имени bind.

#### Frozen behavior и узкая граница

1. **Defaults OC2, picker отдельно.** Обычный Shift+Tab в idle root composer
   выбирает следующий eligible primary/all profile на один шаг с wraparound,
   без открытия модального окна. Reverse default unbound; /agents, palette и
   `<leader>a` продолжают открывать picker. Plain Tab default остаётся completion
   для slash/mention, без suggestions — no-op; не менять defaults на OC1 молча.
2. **Explicit overrides.** Existing admitted config composition поддерживает
   `agent.list`/`agent.cycle`/`agent.cycle.reverse` и соответствующие legacy aliases.
   Reuse string shortcuts, comma alternatives, leader resolution и none/false
   disabling из текущего механизма; preserve source precedence/Location generation
   и actionable invalid-binding diagnostics. Не вводить новый config file/store,
   generic keymap engine или весь расширенный OpenTUI binding-object API.
   Пользователь может явно получить OC1-style navigation:

   ```json
   {"keybinds":{"agent.cycle":"tab","agent.cycle.reverse":"shift+tab"}}
   ```

   Для native override plain Tab в composer без активной completion переключает
   агента; активные approval/question, modal и slash/@ autocomplete владеют клавишами
   первыми. Ни одно нажатие не выполняет и completion, и profile change. Actual
   pinned-reference capture квалифицировать отдельно; подтверждённое отличие
   этого native override раскрывается без masking или universal keymap parity claim.
3. **Catalog и selection — один owner.** Цикл использует ordered eligible IDs
   текущего catalog, не hard-coded Build/Plan pair, alphabetical re-sort в TUI,
   picker search/cursor или synthetic unavailable row. Empty/single eligibility
   не создаёт новый выбор. Удалённый/недоступный saved profile не подменяется default
   при загрузке или failed cycle; сохранить явную диагностику и ручной picker repair
   по T51. Owner revalidates target/current generation; отказ сохраняет прежний
   выбор и draft. Минимальный hidden/disabled/subagent-only catalog fix — T45/R6;
   explicit addressing вне automatic surfaces сохраняет отдельные правила.
4. **State и guards.** Direct cycle возвращает existing SelectAgent intent, не
   локально меняет active_agent. Полные draft/chips/cursor/focus/scroll сохраняются;
   effective model/variant/profile instructions и persistence обновляет existing
   owner. Успех не добавляет agent-selection toast; отказ остаётся видимой ошибкой.
   Busy/read-only/child/Location guards не обходятся, in-flight request не меняется.
   Само переключение не submit, generation или tool execution; последующий request
   действительно получает выбранный профиль. Reopen/restart сохраняют committed
   выбор; Plan reminders/permissions проверяются существующим R6, не новым UI owner.
5. **Hints соответствуют action.** Footer и palette используют effective list/cycle
   shortcuts из той же admitted projection, не нынешний фиксированный AGENTS_HINT.
   Проверять Press/Release и реальные BackTab/Shift+Tab encodings на terminal path;
   Repeat semantics согласовать с pinned reference, не изобретать auto-repeat policy.

#### Ordered slice и достаточная квалификация

1. После safe scheduling handoff и **explicit resume T44** сверить Git/runtime;
   расширить ближайший `oc-tui/src/app/tests/input.rs` regression: terminal Shift+Tab
   даёт следующий SelectAgent, panel None и unchanged draft, тогда как старый код
   открывает Agents. Не обходить raw mapping синтетическим новым action в RED.
2. Минимально расширить existing `config.rs::ConversationKeybinds`/composition и
   `oc-core::queries::TuiChrome` projection для трёх agent shortcuts; переиспользовать
   существующий resolver/Location reload. Supply R6 catalog eligibility/order,
   не требовать all-T45 или полного Plan runtime для самой input маршрутизации.
3. Разделить OpenAgents и forward/reverse в existing commands/events/input; выбирать
   adjacent catalog ID и выдавать SelectAgent. Проверить прямых consumers shared
   enum/DTO и `tui_cmd.rs::apply_intent`; никаких новых selection API/schema/store.
4. Сохранить focus priority до global cycle, особенно configured Tab при slash/@,
   pending mention, dialogs и approval/question. Подключить effective footer/palette
   hints. Extend nearest keymap/config/focus tests только для отличающихся contracts:
   default cycle, explicit forward/reverse override, wrap/empty и focused completion.
   Reuse existing busy/read-only/unavailable/owner-failure assertions без новой matrix.
5. Расширить `tui_cmd/tests/routing.rs::selecting_agent_updates_owner_and_draft_without_selection_toast`
   либо ближайший owner test, затем existing `oc/tests/pty_t39/{interaction,lifecycle}.rs`:
   реальный `ESC[Z` для Shift+Tab и `HT` для explicit Tab, Home/session, cycle/wrap,
   picker всё ещё доступен, completion priority и полный draft/focus, persisted choice
   после restart. Fake-provider counters до submit остаются нулевыми; последующий
   captured request подтверждает profile instructions/model/variant. Reuse R6 Plan/
   policy/reminder/replay evidence, не новую paid campaign или дублирующий E2E harness.
6. Для VIS06/VIS10/VIS17 сравнить full styled cells/PNG/cursor running pinned-original/
   native до/после default cycle и явного picker, с representative existing profiles,
   затем replay/reopen. Independently check actual selected ID/effects. Configured Tab
   и autocomplete capture отдельно зафиксировать на reference; native-only goldens,
   crop/mask и таблица binds не квалифицируют parity. Missing reference —
   BLOCKED_REFERENCE, не runtime PASS. Затем affected-crate и обязательные T44/R6
   gates по runbook, reviewed implementation/evidence commit и delivery.

План не запускает T45, не снимает **PAUSED T44** и не переключает active T50.
VIS06/VIS10/VIS17 остаются mandatory **NOT_RUN/evidence empty**; R6 pending,
historical evidence/statuses сохранены. Doc commit/push не является implementation PASS.

## Compaction parity — VIS34

1. Проверить и переиспользовать session compaction runtime. Проследить `/compact`
   и palette до владельца операции; устранять только подтверждённые расхождения
   admission, safe-boundary execution, summary request, checkpoint и следующего
   provider context. Это не DCP range compression и не `/dcp-compress`.
2. Передавать в TUI действительные queued/running/completed/failed/cancelled
   состояния, streamed summary и usage запроса, с корректным replay после reopen.
3. Сверить divider rules, заголовок и Markdown body с pinned OC2. Running:
   Braille spinner каждые 80 мс, либо `⋯` при `animations=false`; completed:
   без spinner, с фактическим formatted in/out. Не подменять это text shimmer.
4. Проверить ручной `/compact`, автоматический context-threshold trigger и
   overflow recovery детерминированными runtime-сценариями; сохранить raw history,
   causal tool pairs, DCP и conversation-only Revert invariants. Provider-native
   completion, если поддержан активным route, отображается как `Provider compaction`
   без придуманного summary body. `Instructions updated` — отдельное instruction
   событие, а не статус compaction.
5. Снять paired original/native styled-cell и PNG для queued/completed и running
   frame sequence; проверить summary, следующий provider context, failure/cancel,
   reopen и отсутствие animation wakeups после завершения.

Pinned источники VIS34: U15–U17 в `SOURCES.json`.

### Compaction configuration/runtime parity — дополнение к VIS34

Утверждено владельцем 2026-09-27 после RECON. Эталон: OC2 v2.0.12, commit
`2670273ff17da96f85c5826ced57aa1b368754fa`.
Владелец: T44; это дополнение VIS34, не отдельный task tracker.
Утверждение не является claim реализации или PASS qualification.

1. Config normalization.
   - `auto` принимает boolean; default `true`.
   - `prune` и `tail_turns` пропускаются с диагностикой `unsupported`,
     без отказа запуска и без изменения runtime policy.
   - Некорректные recognized values пропускаются с диагностикой `invalid`;
     корректные соседние поля и предыдущие config layers сохраняются.
   - Неизвестные compaction keys обрабатываются как в TS normalizer.
   - `keep.tokens` имеет приоритет над `preserve_recent_tokens`;
     `buffer` имеет приоритет над `reserved`. Различающиеся корректные
     native/legacy значения сопровождаются диагностикой `conflict`.
   - Сохраняются pinned defaults, порядок layers и provenance.
   - Diagnostics содержат source, field path, kind и action;
     не включают raw config, credentials или значения секретов.
   - Это узкая нормализация compaction, не blanket-ignore ошибок config:
     остальные validation, permissions и trust boundaries не ослабляются.

2. `auto` runtime parity.
   - `auto=true` включает threshold-triggered compaction и допускает
     установленный OC2 overflow recovery.
   - `auto=false` отключает оба автоматических пути, сохраняя ручной
     `/compact` и palette action.
   - Сверить `estimateTokens`, input/context/output limits и threshold
     boundary с TS; provider usage и новые tool results учитываются
     согласно эталону.
   - Воспроизвести guards после completed checkpoint, включая restart:
     не запускать повторную автоматическую compaction до появления
     требуемой primary-response usage anchor.
   - Сверить failure/cancel и ограничение overflow rebuild с владельцем
     TS runner; не продолжать ошибочный путь как успешную compaction.

3. Реализация по срезам.
   - Переиспользовать текущий незакоммиченный compaction runtime,
     предварительно сверив Git status/diff; не стирать чужую работу.
   - Сначала исправить normalization и diagnostic delivery.
   - Затем закрыть подтверждённые runtime расхождения.
   - После этого выполнить существующие VIS34 UI/replay gates.
   - Не смешивать session compaction с DCP compression/purge и не
     добавлять отдельный legacy pruning-алгоритм, отсутствующий в OC2.

4. Qualification.
   - Differential fixtures из pinned TS normalizer: normalized config
     и ordered diagnostics сравниваются с Rust.
   - Покрыть `prune=true/false/отсутствует`, `auto=true/false/default`,
     invalid leaves, aliases/conflicts и layered configuration.
   - Проверить настоящие TUI/headless entry points с
     `{"compaction":{"auto":true,"prune":true}}` на fake provider.
   - Проверить threshold below/at boundary, usage anchor, tool results,
     `auto=false/manual`, overflow, failure/cancel и checkpoint/restart.
   - Сохранить raw-history, causal tool pairs, DCP и conversation-only
     Revert/Redo invariants.
   - Пересобрать и проверить `target/release/oc`, а не только debug binary.
   - VIS34 остаётся открытым до прохождения backend и paired UI gates;
     existing evidence/status не заменять утверждением этого плана.

Источники дополнения в pinned donor `opencode/`:
`packages/core/src/config/normalize.ts:318–374,765–777`;
`packages/core/src/config.ts:104–133`;
`packages/core/src/config/plugin/compaction.ts:15–22`;
`packages/core/test/config/normalization.test.ts:369–387`;
`packages/core/src/session/compaction.ts:174–207,742–791`;
`packages/core/src/session/runner/llm.ts:220–225,265–270`.

## Live model-selection parity — VIS09/VIS29/VIS17/VIS35/VIS36 (2026-10-01)

Owner approved original OC2 busy-time switching after follow-up RECON and requested
the detailed plan commit/push. Pinned U95–U102 distinguish draft, commit and prepared
request identity. This corrects whole-turn model pin and model/variant busy refusal
in the first file-tools plan; config/Location/agent generations and child/read-only/
profile/permission authority remain. Backend slice/TOOL12 is T50, prompt/draft consumer
PRM01 is T45, effort ordering VAR01 is T47; T44 owns paired presentation, stays PAUSED.

1. **Picker/composer — VIS09/VIS29.** Real `/models`/effective shortcut is usable while
   root work is busy. Select B changes local composer draft scoped to session/agent
   (Location/agent before a session), not current request or committed model. Preserve
   exact provider/model/variant identity, enabled choices/canonical effort order,
   current/focus markers, ordinary text draft/chips/cursor and modal restoration.
   Captured normal send/command or blank Enter in an existing ordinary composer commits
   choice; busy alone is not refusal. Do not force a composer through a permission/form
   overlay or bypass child/read-only authority. Matching ack/events reconcile draft;
   error/stale echo retains truthful choice and does not clear a newer draft.
2. **Request attribution/replay — VIS17.** With A's stream/tool held, choose B and
   prove draft-only means no committed switch/request/cancel. Commit B while busy:
   old A parts/footer/tool/approval continue bound to actual A; next natural LLM request
   of the same task is B without a new user message. Footer model/variant comes from
   actual request/assistant-span metadata, not composer preference or a synthetic
   single model for all steps in the turn. Replay/reopen/restart preserves real A/B
   attribution and committed selection, not unconfirmed draft or repeated execution.
   Selection after final answer does not itself create a request.
3. **Old/new file families and approval — VIS35/VIS36.** Use eligible GPT A/non-GPT B
   and reverse switch inside one task. A's issued apply_patch remains admissible by
   A's captured tool view after B/edit-write is committed, subject to normal permission/
   preimage rules. Keep pending approval on its original call/resources/preview/identity;
   choice does not authorize, cancel, relabel or bypass it. B's next request receives
   compatible file definitions/managed guidance and retained ordinary outcomes, no
   incompatible opaque replay. Old actual cards retain their names/effects; new calls
   show the correct family. Excluded-by-issuing-request call gets truthful failed state.
4. **Evidence.** Reuse TOOL12 provider/tool/approval barriers and source U100 same-run
   switch, U101 pending-admission/captured commit order. T44 adds full running-original/
   native paired styled-cell/PNG/cursor captures for busy picker/draft-only/commit/
   next-step/old-new footer-card/approval and replay at established representative
   profiles, not a Cartesian matrix. Retry/compact/bytes/storage negatives are existing
   backend receipts, not repeated visual gates. No crop/mask/static label/renderer-only
   golden qualifies effects or busy adoption. New scenarios remain NOT_RUN, evidence
   unchanged; plan delivery does not resume T44 or rewrite historical PASS.

## File-mutation parity — VIS35

Owner-approved 2026-10-01 file-tools amendment supersedes только прежнее universal
patch/no-write-edit/model-name-selector исключение. Backend owns T50/R1/R9,
TOOL12/TOOL20; VIS35/VIS36 — отдельная T44 presentation qualification после explicit
resume, не второй mutation owner или зависимость от completion всего T50/T45.

1. Использовать выбранное семейство T50: exact case-sensitive OC2 model.id содержит
   `gpt-` и не содержит `oss`/`gpt-4` → только `apply_patch(patchText)`; иначе только
   `edit`/`write`, затем effective policy сужает набор. Следующий request после user
   commit обязан заменить несовместимые tools/управляемую guidance в следующем request
   той же задачи, не ждать нового user turn. Prepared request с его tools сохраняет
   captured model/view; picker draft не commit. Raw calls/results не переименовываются/
   не удаляются, retained compatible outcomes не теряются при model mismatch.
   Визуальные эталоны — отдельные OC2 ApplyPatch, Write и Edit, не generic row или
   fake apply_patch. Native apply_patch остаётся ordinary function patchText,
   не provider-hosted Responses apply_patch schema и не model/provider routing.
2. Проверить настоящий executor: create, empty create, multi-hunk update,
   full replacement, delete, move и multi-file. Независимо проверять bytes,
   modes, отсутствие удалённого/source файла и содержимое destination.
   Повторно выполнить применимые TOOL02–TOOL04 и integration/replay checks.
3. Получать bounded result-derived diff metadata у mutation owner:
   operation type, итоговый путь, before/after hunks с корректными номерами,
   additions/deletions и подтверждённые effects. Сохранять для live/replay;
   reopen не читает текущие workspace-файлы и не повторяет mutation.
   Это metadata transcript, не файловый snapshot/restore subsystem.
4. Воспроизвести `# Created` / `← Patched` / `# Deleted` и `-N line/lines`,
   fallback `Patching` со spinner и `# Patch failed`, отдельные per-file blocks,
   geometry/theme/syntax/gutters, unified/split/auto (>120 columns) и wrap.
   Preview до исполнения не показывать как подтверждённый success.
5. Добавить bounded paired PTY scenario к существующему capture harness:
   реальные calls OC2 patch/native apply_patch и OC2/native edit/write, проверка
   объявленных schemas, файловых effects и model-visible results. Использовать
   fixture model IDs, штатно проходящие exact donor selector обеих веток, не
   override production selection, не менять донор или подменять tool labels.
   В одной работающей задаче переключить eligible GPT → non-GPT → GPT через actual
   busy commit и stream/tool barriers, без нового prompt. Проверить completion старых
   calls, следующий request и actual attribution новых/исторических cards отдельно;
   TOOL12 уже владеет protocol/own-model-child/restart/DCP/compact assertions.
6. Снять полные styled-cell/PNG frames на narrow/wide и границе 120/121,
   при default и explicit diff settings, для completed/error и permission
   accept/reject; проверить observable streaming/running frames.
   Проверить session switch/reopen/restart без повторного вызова tools.
7. Сохранить проверки conflict, denied, partial, cancelled/unknown:
   отображать только подтверждённые effects, не обещать repo-wide atomicity.
   Поведение и visual comparator имеют отдельные результаты; несовпадение
   полного кадра не становится PASS через crop/mask или mock-карточку.
8. Отдельно переиспользовать coding E2E из A09: реальная модель сама составляет
   patch и выполняет цикл read → apply_patch → shell test → ответ, с проверкой
   expected paths и reopen. Успех scripted provider fixture доказывает backend/TUI,
   не способность модели работать с форматом; наличие строкового patchText в schema
   тоже не гарантирует корректные hunks. Не добавлять новый model matrix/framework.

9. Добавить реальные Write/Edit consumers после минимальных T50 executors/admission:
   - Write: `{path,content}`, create/overwrite/empty/missing parents; `# Wrote` path
     и numbered syntax content из original input, не сегодняшнего файла; pending
     `Preparing write…`/Write fallback по pinned source. Empty content не скрывает
     реальную zero-byte mutation, длинный input сохраняет bounded presentation.
   - Edit: `{path,oldString,newString,replaceAll?}`, actual unique/all/count effects,
     CRLF/BOM/Unicode; `← Edit` path, result-derived first-file PatchDiff и
     `Preparing edit…` fallback. Donor exact/typography/line matching и negatives —
     TOOL20 behavior, не regex по свободному output для UI status.
   - Shared grammar-based syntax, gutters/line numbers, semantic fg/bg/attributes,
     unified/split/auto (>120), diff wrapping и geometry остаются обязательными.
     Completed effects не заменять approval preview; rejected/error/cancelled/unknown
     честны. Для partial показывать только confirmed changes, не rollback обещание.
   - Live attach/history/reopen/restart используют durable input/result/effects с
     правильным session/operation binding; нет mutation replay или reread текущих
     workspace files. Кадры сохраняют старое настоящее tool name после model switch.
     Approval branches shared с VIS36, bytes/storage — TOOL20, no duplicate matrix.

Pinned источники: U18–U20/U86–U87 и appended U90–U102 в `SOURCES.json`.
VIS35 mandatory pending/NOT_RUN, evidence empty: plan-only delivery не квалифицирует
новые инструменты, не переписывает historical captures и не снимает PAUSED T44.

## Audited Approve permission parity — VIS36

Утверждённый контракт полной реализации/qualification, не claim VIS36 parity PASS.
Все необходимые dependencies поддерживаемых действий обязательны; отсутствие backend
не заменяется Unsupported waiver или инертным UI. Pinned sources: U20–U23,
owner-approved T50 edit/write preview supplement (2026-10-01): U91/U92.

1. **Contract/mappings.** Зафиксировать native tool → permission action → presentation,
   actual resources и отдельные owner-generated save patterns. apply_patch и T50
   edit/write используют shared canonical mutation permission и donor edit preview;
   actual path/resource/home/save-pattern normalization, prepared before/after и
   approved identity/bytes/absence recheck не зависят от выбранной моделью схемы.
   Реализовать donor project identity для grants: Git root/origin,
   subdirectories/worktrees/clones/non-Git; не подменять session/Location directory.
2. **Owner lifecycle.** Existing application façade владеет authoritative pending
   query, asked/resolved events и typed once/always/reject reply с optional feedback.
   Request связан с session/turn/call/operation, pinned Location/config generation и
   agent context. Event loss/attach восстанавливается query. Wait cancellable; inbox
   принимает reply/cancel; DB transaction не держать через wait. Stale/duplicate/wrong
   replies не выпускают другую операцию и не создают grants. Cancel/shutdown/drop
   очищают waiters. Restart не авторизует replay старого ожидания.
3. **Leaf admission/durability.** Policy evaluation чистая; один подготовленный leaf
   request и invocation-local Once покрывают exact resources и rechecks, не весь lane.
   Интегрировать все supported tools, включая patch move targets, edit/write path
   и overwrite/missing-target preimage, shell cwd/command/save,
   read/search/webfetch/skill/MCP/subagent/compress. После wait revalidate prerequisites.
   Порядок: prepare/validate → approval → revalidate → durable execution intent → effect
   → durable outcome. Если нужен durable waiting-state, он pre-execution, не started
   с ложным unknown effect. Plain Reject прерывает runner, отклоняет остальные pending
   той же session и не исполняет remaining batch; feedback rejection имеет corrected
   continuation. Typed outcomes сохраняют call/result graph, не превращаются в error string.
4. **Always.** Existing SQLite, deduplicated project/action/save-pattern rows, commit
   до persistent acknowledgement, save failure не success. Сначала effective configured
   Deny, затем saved allowances для Ask; grants не restrictive authority layer. Ask
   approvable, Deny/structural ceilings нет. Переоценить eligible pending с их agent
   context. Once/autoaccept не создают permanent grants. Проверить restart/isolation.
5. **Original UI/previews.** Один request-keyed lower surface/fullscreen state machine,
   не Select modal/per-tool dialogs. Root own/descendant queue в donor order, reply
   адресован request session, direct child route соблюдает original composer flow.
   Request change reset selection/fullscreen/reject stage. Geometry/theme, inline
   maxHeight 15, terminal breakpoint <80, Once/Always iff save/Reject, Always patterns,
   keyboard wraparound/mouse/Enter, configurable fullscreen default Ctrl+F, app.exit
   bindings и Esc minimize-before-reject. Child feedback separate editor с confirm,
   cancel и Ctrl+C clear-before-cancel. Полный draft/chips/mentions/cursor сохранён;
   reply error не удаляет pending UI. Matching tool warning/denied и family tab attention.
   Real owner previews до effect: command/path/pattern/URL/edit diff, metadata precedence,
   first files diff/raw-patch/no-diff fallback, permission word-wrap и auto split >120.
   Completed patch metadata — confirmed effects, не preview. Не добавлять LSP/JS hooks
   или обход trusted roots ради presenter branches.
6. **Mode controls.** Default prompt, admitted cli.json/jsonc session.permissions,
   working Settings → Permissions, /settings/Open settings/filtered palette entry,
   persistence/error/reload и CLI --auto precedence с donor compatibility aliases.
   Autoaccept once-consumer для already-pending/new root/child asks, не Deny/Always;
   truthful capability/auto marker. Headless без consumer ApprovalRequired/nonzero;
   donor run --auto entry требует real explicit once-consumer, не unsupported stub.
7. **Shared qualification.** Owner lifecycle/grants/recovery integration + real paired
   PTY full styled-cell/PNG + independently verified effects. Once/reask, Always/project
   identity/restart/isolation, Deny/mixed resources, no-save, root reject/child feedback,
   family queue/reply errors/cancel/shutdown/stale replies/storage failure/headless modes,
   Settings/config/CLI precedence, representative supported preview branches,
   narrow/fullscreen и 79/80,120/121. Reuse VIS35 patch matrix и A09 live evidence,
   без cross-product каждого tool/viewport/state. Behavior PASS отдельно от visual;
   native-only goldens/crop/mask не закрывают parity.

Sequence: contract → owner → leaf metadata/grants → UI/modes → qualification.
Текущий VIS31/32 slice удобно завершить для ownership, но это не hard dependency.
UI scaffold после DTO, patch preview qualification после real preflight. VIS36 нужен
для accept/reject VIS35, не independent executor/completed cards; VIS34 независим.
Не добавлять T44 depends_on completion T43/T45. KISS: existing owner/channels/storage/
editor/theme/diff, один pending map/state machine; без нового task/framework/DB/policy
engine/grant dashboard. VIS36 NOT_RUN до actual qualification, не full T44 closure.

### Autoaccept mode completion — VIS36 (2026-10-01)

**Owner-approved after read-only RECON; plan pending/NOT_RUN.** Источник: вопрос
владельца о `--auto` в OC2 TS и включении в Rust-план, затем «вноси правки в план
работ и коммит пуш». Это доведение существующего T44/R5/VIS36 mode slice, не новый
task и не разрешение снимать PAUSED T44 или переключать active T50.

**RECON baseline, не новая qualification.** Pinned OC2 v2.0.12
`2670273ff17da96f85c5826ced57aa1b368754fa`, U21/U23: `--auto` по умолчанию false;
скрытые `--yolo`/`--dangerously-skip-permissions` передаются тем же auto bool.
TUI выбирает args.auto → autoaccept либо `session.permissions`, агрегирует root
и descendants и отвечает Once. Headless `run` отвечает Once на `permission.asked`,
но фильтрует exact launched session ID. Это клиентский responder после admission,
не AI safety orchestration и не global Allow. Configured Deny проверяется до Ask,
Once не сохраняет grant; App `permissions.autoApprove` — отдельный web/desktop путь
вне scope. Pinned paths: `packages/cli/src/commands/commands.ts:26–36`,
`commands/handlers/default.ts:82–87`, `commands/handlers/run.ts:14–28`,
`run/noninteractive.ts:135–154,180–183`; `packages/tui/src/context/permission.tsx:5–15`,
`routes/session/index.tsx:185–195,243–259`; `packages/core/src/permission.ts:173–188,231–259,295–304`.

Native `cli.rs`/`bootstrap.rs`/`headless.rs`, `oc-core/src/approval.rs` и application/
runtime/TUI уже реализуют `--auto`, explicit consumer, Once и mode persistence.
Поведенческий milestone `0a86cf67c` описан в `progress/M9/T44/0066.md`: full VIS36
остаётся OPEN, не PASS. Later actual-binary evidence:
`evidence/tui/recovery-v00/permission-report20260927-01.md:138–145` и
`evidence/T50/foreground-shell.md:111–118,127–145` подтверждают Ask+CLI-auto effects,
zero auto grants, Deny/no-effect и headless config-alone refusal на тех срезах.
Это historical evidence, не запуск проверок на текущем HEAD. Старое
`evidence/tui/recovery-v02/auto-capability.md` Unsupported сохраняется как история.
RECON не нашёл hidden aliases в native CLI; mode transitions и full visuals ещё
требуют qualification. Не переписывать reports и не переоткрывать готовый backend с нуля.

**Frozen semantics.** Default Prompt; admitted `cli.json/jsonc`
`session.permissions=prompt|autoaccept` и Settings сохраняют preference. CLI auto
process-only, не пишет эту preference, имеет приоритет даже после Settings → Prompt
и reload. Без CLI auto следующий restart использует сохранённую preference.
Headless Ask без explicit CLI consumer даёт actionable ApprovalRequired/nonzero;
config autoaccept alone не включает его. Auto отвечает Once только eligible Ask,
включая already-pending/new owned root/child, не Deny/Always и не ответы `question`.
Plan/parent-child authority, parsed resource/path/preimage rechecks, Location/generation,
cancel/shutdown и no unknown-effect replay не меняются. Same-run owned foreground/
background child auto coverage сохраняется как **native headless extension** относительно
donor exact-session filter; own child profile/policy и foreign/stale isolation обязательны.

**Ordered remaining work after explicit T44 resume:**

1. **Mode contract/current evidence.** Сверить HEAD и перечисленные owners/tests;
   отделить implemented behavior от ещё открытого full VIS36. Использовать одну
   effective-mode модель `CLI auto || persisted autoaccept`, отдельно saved preference
   и explicit headless consumer. Никакого второго policy engine/LLM classifier.
2. **CLI compatibility.** Добавить hidden `--yolo` и
   `--dangerously-skip-permissions` как aliases существующего `Args.auto`, не отдельный
   bypass. Проверить bare TUI, `tui` и `run`, admitted global-flag positions и help:
   visible `--auto`, aliases hidden. Real Ask effect/zero grants и Deny/no-effect
   доказывают путь после parsing; одной unit-проверки bool недостаточно.
3. **Pending/new transitions.** Barrier-controlled owner/actual-binary тестами
   проверить Prompt→auto для уже ожидающего и следующего Ask, auto→Prompt для
   следующего Ask, CLI-auto priority при Settings Prompt/reload, failure before ack
   и restart. RECON видел application pending drain и runtime registration drain,
   а TUI SetPermissionMode сначала сохраняет mode, затем повторно включает CLI auto.
   Это гипотеза transition risk, **не доказанный баг**. Проследить registration/
   drain/effective state; только если эксперимент покажет расхождение, свести применение
   effective mode/pending resolution к существующему owner без transient prompt,
   duplicate resolution или lost waiter. Save/reply error сохраняет committed mode/
   pending/draft и actionable feedback, cancel/stale reply не выдаёт success.
4. **Owned child routing.** Расширить ближайший root/foreground/background child
   fixture, не копировать donor headless exact-session limitation. Approve binding
   остаётся request session/turn/call/operation/resource/agent/Location/generation;
   navigation/late events не отвечают current-view или соседней/foreign сессии.
   Effective child Deny/Plan/narrowing сохраняются даже при parent/child auto.
5. **TUI controls and parity.** Довести Settings → Permissions, /settings/Open
   settings/filtered palette entry, pending dismissal, truthful effective auto marker
   и root/child tool/tab attention. Сохранить точный draft/chips/cursor/focus при
   toggles, reply/save errors, tab/navigation и reload; CLI override не показывать
   как успешно выключенный auto. Running original/native full styled-cell/PNG/cursor
   sequences Prompt pending→auto→resolved→Prompt/reask и CLI override квалифицировать
   на существующих profiles, не static marker/native goldens. `question` остаётся
   отдельным VIS37/TOOL15 form/answer consumer.
6. **Closure.** Расширить ближайшие `crates/oc/src/approval_tests.rs`, actual ELF
   `crates/oc/tests/approval_binary.rs`, core approval/runtime и существующие PTY
   fixtures только для неподтверждённого поведения/самостоятельного риска. Сначала
   targeted checks и independently verified effects/grants/config, затем affected
   crate/integration workspace fmt/clippy/tests/build и rebuilt binary qualification.
   Reuse existing denial/preimage/grants/child/question/recovery regressions;
   метод — `docs/TEST_PLAN.md`. Behavior PASS и paired VIS36 parity отдельны;
   Deny tool result может завершить headless с exit 0: обязательны zero effects/grants,
   а nonzero требуется именно для unconsumed Ask. Нет новой paid campaign/model matrix.

Change envelope: existing CLI/bootstrap/headless, approval/application/runtime/config
owners и TUI mode/Settings/routing/tests; новых stores/traits/framework/DB/config key
или gate IDs нет. Existing VIS36 mandatory NOT_RUN/evidence empty и все historical
status/baseline/PASS остаются неизменными. T50 TOOL15/TOOL12/TOOL20 и T45 child facts
можно reuse без all-task dependencies; независимые T44 slices не блокируются.

## Question UI parity — VIS37

Owner-approved 2026-09-27: сначала настоящий backend `question`, затем visual parity
в T44. Это обязательное дополнение R4/R5, отдельное от permission approval/VIS36.
T50/R4/TOOL15 владеет tool schema, application pending/wait/replies/cancel и реальным
frontend answer consumer; T44/VIS37 — pinned OC2 presentation/interactions и paired
visual qualification. Pinned sources: U31–U33 в `SOURCES.json`.

1. **Backend prerequisite, не task completion.** Доставить минимальный question slice:
   реальный model call → application-owned pending question → typed PTY answer/cancel
   → durable outcome; ordered answers видны в следующем provider request. Bounded
   operation/session/generation-scoped pending/replies, stale/duplicate/foreign refusal,
   no DB transaction через wait, explicit headless failure и restart graph — TOOL15.
   Effective Deny и General/Explore restrictions сохраняются; --auto/autoaccept не
   отвечает за пользователя. Не добавлять T44 depends_on all T50 или обратную связь;
   остальные T44 slices независимы, один active task через progress owner.
2. **Original question surface.** После готовности backend воспроизвести lower
   FormPrompt, не Select modal и не Once/Always/Reject форму. Match raised background,
   left split border/action role, padding/spacing, Questions title, question/header и
   option descriptions; ordinal/hover/focus/selected markers используют donor
   formfield roles. При нескольких вопросах — tabs/Submit либо narrow Field n of m
   fallback по реальной tab-fit геометрии, completion counts и review когда у original.
   Сохранять Unicode/wrapping/long descriptions и реальные размеры textarea/review.
   Scope — формы, создаваемые question, не arbitrary Form/OAuth/external-URL framework.
3. **Answers and interactions.** Single выбор, multiple toggles и автоматически
   добавленный Type your own answer; несколько вопросов, сохранение/редактирование
   ответов, review/Submit и footer hints соответствуют U32. Проверить arrows/j/k,
   numeric shortcuts, Enter/Space, Tab/Shift+Tab и мышь в применимых состояниях.
   Esc сначала закрывает option custom-answer edit, если так делает original, а не
   всегда dismiss. Default Ctrl+C в непустом answer edit очищает текст; в пустом
   non-textual custom edit закрывает editing, в пустом textual edit dismisses форму.
   Вне editing/textual режима app.exit dismisses форму; configured bindings/focus
   остаются authoritative. Text selection не превращается в click/submit. Reply адресован
   исходному request/session, не текущему экрану или соседней форме.
4. **Lifecycle and draft.** Pending → answer edit/review → submitting → answered
   либо reply error/dismissed/cancelled основан на настоящем owner state. Ошибка reply
   не придумывает answer/success и не теряет незавершённые ответы. Match composer/form
   priority и request-keyed reset; pending drafts формы переживают tab remount в рамках
   процесса, не обещая их persistence после crash. Обычный composer draft/chips/cursor
   и focus восстанавливаются; поздний ответ не закрывает чужую форму и не выпускает
   чужую операцию. Dismissal прерывает execution, не становится успешным tool result.
5. **Transcript and replay.** Match Asking questions…/Asked n question(s) и completed
   # Questions с фактическими metadata.answers в исходном порядке; question muted,
   answer base, spacing по U33. Не заполнять пропущенные ответы догадками. Снять live
   и reopened/restarted cards из durable результатов без reask/tool reexecution;
   cancelled/error presentation остаётся честной, не completed-answer block.
6. **Shared qualification.** После TOOL15 backend prerequisite снять running pinned
   original/native при одинаковых questions/answers/theme/profile/state. Full paired
   styled-cell/PNG pending, focused/selected, custom-answer edit, applicable review,
   reply-error/dismissal и answered/reopen frames; cursor и routing проверяются real
   PTY input. Использовать existing 80x24/120x40/160x48 profiles и representative
   tab-fit/long-text cases, не каждый type×viewport×state cross-product. Независимо
   проверить фактический answer/result и next request; переиспользовать TOOL15 evidence,
   не повторять всю backend negative matrix ради visual gate. Behavior PASS отдельно
   от visual PASS; native-only goldens/static widget/crop/mask parity не закрывают.

Sequence: T50 question owner/schema → real answer consumer + TOOL15 → T44 FormPrompt
and cards → VIS37 paired/replay qualification. VIS36 не prerequisite существования
question backend; при одновременных requests соблюдать оригинальную composer priority,
не подменять вопросы permissions и не отменять независимый approval scope. Existing
owner/channels/editor/theme/history, без нового tracker/framework/DB или fake outcome.
VIS37 mandatory, NOT_RUN/evidence empty до actual qualification; plan approval не PASS.

## DCP compression display — VIS38

Owner-approved 2026-09-27: подробный DCP-блок появляется непосредственно в ленте
разговора между шагами агента; большие token-метрики используют K/M. Это обязательный
R4/R5 slice T44, не отдельная modal и не OC2 native `/compact`/VIS34. Источник текста,
шкалы, defaults и accounting — DCP 3.1.15, commit
`11f6517780a502512a3467645074be447cb0369e`, AGPL-3.0-or-later; оригинальная transcript
геометрия — OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`.
Pinned links/locators: D05–D10 и U34 в `SOURCES.json`.

1. **Backend before display.** Model `compress` и existing manual compression route
   публикуют typed presentation из настоящей operation/commit, не разбора свободного
   LLM-текста или canned screenshot. Сохранять scoped operation/run identity, topic,
   связанные block IDs, отдельные removed/summary token estimates и их метод, unique
   newly-covered message/tool-call counts, cumulative accounting и компактный снимок
   карты сообщений. Один successful вызов с несколькими ranges — один run/card, не
   отдельный номер на каждый block. Run identity стабильна через restart; не выводить
   её из process-global stats, количества загруженных rows или global block sequence.
   Legacy результаты без необходимых данных получают честное unavailable/legacy
   представление, не fabricated counts или повторное исполнение ради backfill.
2. **Accounting semantics.** Верхний removed — cumulative gross removed из projection
   этой session/DCP revision, включая действительно учтённый pruning; верхний summary —
   текущая сумма active summaries, не lifetime сумма всех blocks. Нижние removed/summary
   принадлежат run. Items дедуплицируются по стабильным message/call occurrence IDs,
   а не именам tools; inherited covered IDs при recompression не считаются вновь.
   Pure summary recompression может уменьшать projection при zero newly-covered Items:
   это не автоматически no-gain. Native no-gain определяется фактической projection.
   Net saved estimate отдельно от removed и summary; `saved = byte difference / 4`
   недостаточно для обеих величин. Token estimates/метод доступны в DCP metadata/panel;
   не называть их provider billing, точным usage или размером raw history. Accounting
   коммитится атомарно с projection/outcome, failed/no-gain/cancel не прибавляет успех.
3. **Detailed transcript block.** Default detailed/chat после commit показывает
   `▣ DCP | -… removed, +… summary`, blank row, `│…│`,
   `▣ Compression #N -… removed, +… summary`, `→ Topic: …` и
   `→ Items: N messages and M tools compressed`. При zero tools donor опускает tools
   clause; zero summary metric и multi-range summary handling следуют D05. Match
   raised background, left split `┃`, padding/spacing/theme roles и Unicode wrapping
   настоящего OC2 transcript wrapper, используя existing semantic theme roles.
   Карта — 50 ячеек канонических message positions: `█` ordinary/not compressed,
   `░` ранее active-compressed, `⣿` newly compressed этим run, recent имеет приоритет.
   Mapping, fixed width и empty fallback следуют D06; это не token ratio или execution
   percentage. Running имеет короткое DCP/Compressing… состояние с topic и existing
   animation policy; confirmed metrics/bar появляются только после commit. No-gain,
   denied/error/cancelled/unknown состояния честные; successful report не подменяет их.
4. **Shared number/K/M formatter.** Только presentation token-метрик: below 1000 —
   целое число; от 1000 — K; от 1 000 000 — M. Decimal radix 1000, максимум один знак
   после точки, rounding half-up, `.0` убирается. Если округлённое K становится 1000K,
   promote до 1M. Примеры: 842 → 842, 11 900 → 11.9K, 1 000 000 → 1M,
   4 218 800 → 4.2M, 999 950 → 1M. Raw counters сохраняют точность; formatting не
   используется для arithmetic/admission. Один formatter для header/run/panel и
   notification token labels. Pinned DCP имеет только K: M и boundary promotion —
   явно согласованное native display difference, не изменение token estimator.
5. **Controls and lifecycle.** `pruneNotification` off/minimal/detailed не сводится
   к bool; chat/toast и `compress.showCompression` независимы. Defaults detailed/chat,
   showCompression=false; true добавляет реальный сохранённый summary, bounded/paged,
   с source-derived multi-range headings. Minimal — header + Compression #N; off
   отключает notification, не runtime failure diagnostics. Native toast mapping в
   bounded transient status notice остаётся documented difference, без persistent
   duplicate. В chat run имеет одну card identity, pending сменяется committed view.
   Карточка — projection существующей операции, не новый user prompt/LLM message.
   Поздний event адресован operation/session/generation; child не меняет parent totals.
6. **Replay and resource bounds.** Freeze header/bar/run metrics на момент commit;
   следующий run добавляет свою карточку, не переписывает предыдущие. Session switch,
   reopen/restart восстанавливает metadata без повторного compress/provider request.
   Current `/dcp` stats отражают current committed branch/revision; historical cards
   остаются snapshots. Undo/Redo hides/restores соответствующие operations/cards и
   projection versions без replay, новый branch не получает abandoned-tail totals.
   Получать cards через bounded history paging/queries; компактный bar/accounting не
   требует полной материализации raw history/superseded archive или unbounded UI cache.
   Использовать existing owner/storage/channels, без второй history/DB/event framework.
7. **Qualification and references.** Сначала actual binary compression/continuation:
   independently checked counters, меньший next provider request, immutable raw
   history, multi-range/recompression, failure/cancel/no-gain и durable restart.
   Затем одинаковые accounting fixtures/theme/terminal/state у native и display
   reference: запускать original pinned DCP formatter и отрисовывать его payload
   настоящим OC2 TUI. Возможен reference-only fixture driver; явно назвать метод
   source-derived display comparison, не claim совместимости legacy plugin runtime
   с OC2 и не замена actual native tool effects. Без исполнимого formatter/renderer
   reference — BLOCKED_REFERENCE. Full styled-cell/PNG/cursor frames на existing
   80x24/120x40/160x48 profiles и representative long-topic/summary cases; source
   fixtures проверяют bar categories и K/M rounding/boundaries. M/promotion и existing
   native toast mapping фиксируются как named differences в unmasked full comparisons;
   прочие cells/styles/geometry не waive-ить, не переписывать reference по Rust.
   Shared A07/A08/A10, VIS21/22/23/33 evidence покрывает controls, late routing,
   secret/control safety, replay и equal-active small/large-archive measurements;
   не повторять всю backend negative или viewport×state matrix. Производный код,
   formatter fixtures и snippets сохраняют pinned provenance/license/notices.

Sequence: existing DCP operation/accounting → typed durable view → transcript/controls
→ paired display/replay/resource qualification. T44 owns VIS38 display slice; T45
retains DCP10/DCP11 child/long-horizon algorithms, без all-T45 done-dependency или
новой задачи. UI04 notification-not-duplicate-history остаётся invariant: card
читает существующую operation, transient notices не создают дополнительные prompts.
VIS38 mandatory, NOT_RUN/evidence empty до actual qualification; исторический T39
panel/status notice PASS не подтверждает этот новый контракт.

## Subagent delegation parity — VIS39

Owner-approved 2026-09-27: полный visual/interactive parity делегирования, работающих
children и completion notices, включая цвета и анимации. OCR-примеры — иллюстрации,
не oracle символов или production strings. Reference — исполнимый OC2 v2.0.12
`2670273ff17da96f85c5826ced57aa1b368754fa`, pinned U35–U49 и existing U02 в SOURCES.json.
T44 owns R4/R5/V06/VIS39; T45 owns R3/SUB01/SUB02 lifecycle и необходимые typed
history/query/event projections. Сначала lifecycle slice, затем UI qualification,
не all-T45 completion dependency и не второй task/store/job/event framework.
Это порядок family/subagent qualification, не запрет независимой работы над own-tab
spinner и VIS41 hover-marquee; их утверждённый срез описан ниже. Full VIS39 всё ещё
требует реальных SUB01/SUB02 family facts, не локальный фиктивный busy flag.

1. **Truthful phases before paint.** Distinguish arguments streaming, admitted call,
   permission wait, foreground child running, completed background launch/child running,
   completed/error/cancelled/unknown child и continuation. Presentation uses bounded
   call/part IDs, typed complete input, actual child/session/parent linkage, operation
   outcome, launch metadata and current child job generation/status. Incomplete string
   input is empty presentation input как в U35/U38: `Delegating…`, без extracting
   agent/description/sessionID из partial JSON или свободного LLM-текста. Preserve
   native response-close/schema-validation-before-tool-admission (CONTRACTS.md): это
   declared streaming-timing difference, не разрешение early execution и не основание
   сериализовать детей. No child/effect до admission. T45 proves real overlapping
   foreground children, batch join before next parent step и immediate background
   progress while parent active; unsupported background не подменять фиктивным success.
2. **Inline delegation rows.** Follow Subagent + InlineToolRow (U35/U36), paddingLeft=3,
   source wrapping/spacing, no extra bordered result/session-ID block. Pending literal
   `Delegating…`; populated label `<Titlecased agent> Subagent — <description>` with
   General/Subagent fallbacks. `sessionID` input means `Continue subagent`; append
   ` · <model label>` only for explicit input.model, resolving actual catalog name or
   raw reference and `#variant` → ` (variant)`, not inherited parent-model decoration.
   Running = tool running OR linked child's current status running. Non-continuation
   running uses spinner, completed non-running `✓`; continuation uses `↳` without the
   ordinary subagent spinner once populated. Missing complete description still uses
   pending Spinner through InlineToolRow fallback, even if explicit spinner=false.
   Running suppresses stale tool-error presentation. `Background` badge is completed
   tool + launch metadata.status=running, NOT current child status: may remain after
   child completes. Historical launch rows can animate again when that SAME child is
   continued; unlike VIS38 DCP snapshots, do not freeze their live child status.
3. **Colors, attributes and interaction.** Inline color precedence: explicit override,
   permission-pending warning, ordinary failed error, clickable hover text.base,
   otherwise text.muted. Denial is typed rejection/strikethrough, not ordinary failure;
   preserve native typed error model rather than donor error-message keyword classifiers.
   Successful `✓` is muted, not invented green. Badge text muted, background
   theme.decrease(background.base), spaces/padding and label flex-wrap as source.
   Click a normal linked row → actual child; failed click toggles actual error detail
   before navigation, expanded detail is indented as source. Text selection blocks
   clicks; hover alone does not expand. VIS36 supplies real root/child permission
   routing and attention, not an auto-grant; VIS37 question priority remains distinct.
4. **Thought and parent footer.** Reuse VIS15/17 spacing and metadata qualification:
   actual nonempty public reasoning groups, completed collapsed `+` / expanded `-`,
   unfinished spinner. Thought step count is reasoning parts, NOT children; duration
   sums nonnegative completed intervals. Collapsed non-hover completed warning has
   alpha .6, hover/expanded full warning; unfinished uses text.base. Click toggle is
   selection-safe. AssistantFooter uses that response's actual agent label/color,
   model, duration and optional tok/s, source width/session.tps conditions. Duration/
   rate muted, error profile label muted. Notice rows do not receive fake assistant
   footers. OCR prose, IDs, Muse Spark/Free, 14.4s and 47.4 tok/s are fixture data only.
5. **Durable completion notices and ordering.** T45 generates real synthetic parent
   messages with source=subagent, childID, agent, state, description and actual child
   result; persist/deduplicate job-generation/delivery identity before display. U35/U41:
   completed `↳ <Actor> finished`, error `! <Actor> failed`, cancelled actual state;
   suffix ` · <description>` muted, display-width truncated to remaining width,
   marginLeft=3, no wrap. Heading info/error/warning by state; normal child-linked hover
   text.base and selection-safe navigation. This is conversation history, not transient
   toast or generic Notice. Parent receives result context and can continue automatically
   without UI polling. Capture child 1 alone and children 2/3 together before the next
   parent response, preserving real admission/order/batching, no assumed global ordering
   of independent jobs. Ordinary child prose saying it cannot run sleep is completed,
   not runtime failed; no free-text status inference or canned parent follow-up.
6. **Child controls and composer.** Footer counts actual running family descendants
   excluding current session, correct `N subagent(s)`, separate shell count, muted/base
   hover, real keymap hint and click → child picker. Use real lower Subagents composer
   (U44/U45), not fullscreen modal: raised surface, split border, source padding/title/
   hints, maxHeight=5 scrollbox. Active/inactive filter, No active/inactive subagents,
   truthful titles/agent fallback and Running labels; current vs focused semantic
   action fg/bg, focused bold, mouse selection/open. Ctrl+A toggles filter, Ctrl+D
   interrupts only running child, Enter opens, first Up closes, Down wraps; Esc/Ctrl+C
   and left/right tabs follow composer/focus layer. Child route opens Subagents by
   default when no form; close returns parent, preserve actual parent draft/chips/
   cursor/focus. Session shortcuts (U02/U35) include interrupt, Down picker and Up
   parent where applicable; never hijack ordinary editor arrows. The declared child
   next/previous bindings have no registered route handlers in this pinned source;
   do not invent functional sibling Left/Right navigation. Composer Left/Right
   switches its tabs.
   Ctrl+B invokes actual owner Session.background conversion of owned foreground work,
   admits genuine control context, returns running launch state and keeps observing
   terminal delivery; not a local badge toggle. Other shell lifecycle stays T50/TOOL13.
7. **Animation contract.** Regular Delegating/running row Spinner (U37): frames
   `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`, 80 ms, actual row color; animations off → `⋯`.
   No subagent text shimmer: Subagent does not pass Spinner's optional shimmer.
   Reconcile component mount/branch/phase rules, not globally synchronize all rows.
   Parent own-running scanner (VIS28, 40 ms eight-cell profile scanner/off `[⋯]`) is
   separate from child-row running and root-tab family busy: parent may be idle while
   children run. U46 root-family permission attention wins question; unread belongs
   to root and follows actual parent continuation/idle, not every child completion.
   U47 status indicators use default dots/80 ms, attention `!`/`?`, numbers mode keeps
   number; off status-mode running keeps first frame, NOT universal `⋯`. Source-level
   arcs/quadrants/line are 120 ms only if actually selected; do not invent config options.
   Match U47/U48 running/attention/unread/completion tints and transitions: unread fade
   180 ms, glow-level 200 ms, number ignition 700 ms, title/number glow tween 400 ms;
   running sweep 2800 ms/attack450/head4/tail18/release500, completion1200/attack.12/
   opacity.18, edgeflash800, glow ignition600/release900/drain200/opacity.16, with source
   easing/blending, branches and animations-off behavior. Automatic-rename title
   shimmer is not ordinary delegation animation. Reuse VIS31 demand-driven deadlines:
   after finite transitions no periodic work, static glow needs no timer. No new clock
   framework, permanent polling or animation-off-only waiver.
8. **Resolved palette and resource/replay invariants.** Use v2 palette U49 and source
   semantic roles, not legacy palette or guessed OCR colors. Default dark checkpoints:
   base #eeeeee, muted #808080, background #0a0a0a, raised #141414, error #e06c75,
   warning #f5a742, info #56b6c2; tab attention uses source accent[200] (#9d7cd8),
   not tool warning. Verify alpha/tints, bold/strikethrough and blank-cell background;
   profile/footer colors data-driven. Reuse R2 representative light/custom checks,
   no full theme×viewport×state cross-product. Replay/reopen/restart restores actual
   notices/tool metadata and current child status, no duplicate result delivery or
   UI-triggered provider/tool re-execution of committed work. T45 may safely resume
   unfinished jobs under R3 recovery; this is distinct from view restoration and
   never authorizes unknown-effect replay. Unknown stays unknown, no infinite spinner.
   Late events target actual session/job generation; bounded family queries/history
   paging/cache/queues do not materialize all archived child transcripts. Conversation-
   only Undo/Redo, immutable history, no unknown-effect replay, parent-child narrowing,
   cleanup and A10 bounds remain authoritative. Do not widen Explore shell permission
   for a sleep fixture or add excluded built-in websearch/CodeMode/browser.
9. **Qualification sequence.** First SUB01/SUB02 actual rebuilt binary, protocol and
   SQLite evidence for runtime states, linkage/results/notices/continuation/recovery.
   Then running original/native paired full styled-cell/PNG/cursor captures with
   identical fixtures/theme/profile at 80x24/120x40/160x48 and representative Unicode/
   long description. Main bounded fake-provider scenario: three call argument streams
   → three overlapping background children → parent answer while children run → first
   notice/parent continuation → two notices/next parent response. Use barriers, no
   sleep300 or new paid campaign. Addressable foreground/continuation/explicit model-
   variant/Ctrl+B/permission/deny/error/cancel/unknown/restart paths share existing
   runtime evidence; reuse VIS15/17/21/22/23/28/31/33/36/37 and A02/A08/A10, not a
   duplicated backend negative matrix. Require normal/hover/error-detail/selected-text
   and composer effects, animation-on phase sequences INCLUDING state transitions
   and animation-off fallbacks. One static frame/native golden/off-only capture is
   insufficient. No masks/crops/tolerance/reference updates to hide differences;
   missing executable reference remains BLOCKED_REFERENCE. Preserve pinned provenance,
   licenses and failed/blocked attempts. Runtime and visual results reported separately.

VIS39 mandatory, NOT_RUN/evidence empty until actual qualification. Approval freezes
the plan, not implementation PASS; existing task statuses/evidence/baseline stay intact.

### Child TUI, Shell/Terminals — уточнение VIS39 (2026-10-01)

Owner asked for full parity of this segment and explicitly chose **«Включить
Terminals»**, then approved the detailed work-plan update/commit/push. Pinned OC2
remains `2670273ff17da96f85c5826ced57aa1b368754fa`; OCR is scenario intent, not a
literal strings/theme/status oracle. This subsection supplements the full VIS39
contract above. Its old no-new-task language excludes duplicate subagent/UI owners;
the approved exception is **T56/TERM01 for native interactive PTYs**, not another
VIS gate. T44 remains PAUSED until explicit resume; this plan is not execution PASS.

**RECON baseline at native `cd2a95926e3c3c5f3eeda451cf8a1a8b814914fd`:** linked
inline child navigation/family composer is absent; explicit child history is
read-only and blanket-refuses SwitchSession; child turn text/reasoning/tool callbacks
are no-ops; background subagents are rejected and tool batch awaits sequentially.
Ctrl+B currently means input Left. Shell has initially background jobs/notices/cancel
and terminal-only retained output, not authoritative live inventory/output or
foreground conversion. No persistent PTY owner/pane exists. Syntax is hand-written
Rust/Python/Shell/JSON heuristics with plain fallback. These are source observations,
**not** executed RED/PASS or evidence that the dirty T50 code implements this scope.

| Required surface | Behavioral owner/prerequisite | T44 qualification |
|---|---|---|
| Linked child, ordinary live transcript, family/current state, independent interrupt/background, notices | T45/R3/SUB01/SUB02 | VIS39 + reused VIS15/17/21/22/23/27/28/31/33/36/37 |
| Actual running shell list, live bounded output/final flush, exact-job kill, same-process Ctrl+B | T50/R2/TOOL13 | VIS39 Shell composer/output dialog, existing VIS16 plain shell cards |
| Session-local interactive PTY inventory/create/select/input/resize/snapshot/reap | T56/TERM01 | VIS39 Terminals composer/picker/right pane/focus/colors/cursor |
| Shared complete code grammar/alias/query/style rendering in parent and child | T44/R4 native renderer | VIS14 and VIS35, reused VIS27/31/32/33 |

1. **Actual child route, not a result preview.** Clicking the parent's delegation
   row or durable child notice, or selecting the family row, opens that same child's
   ordinary session transcript while it is still running. Reuse the parent/child
   renderer for user task, Thought/reasoning groups, Markdown/fenced code, Shell/
   read/grep/patch cards, selection/copy and truthful agent/model/duration/TPS/
   interrupted footer. No extra bordered result excerpt, synthetic status footer,
   cropped child view or unconditional native read-only banner in place of donor
   chrome. Open history plus live events without duplicate parts or a restart of work.
   When no form has priority the child auto-opens the lower Subagents composer;
   its close and `session.parent` (default Up) return to the real parent. Normal
   child routes suppress the root sidebar; root-family tabs remain root-owned.
   Narrowly supersede only blanket navigation/control refusal: do **not** turn a
   child into an arbitrary new root-turn editor or relax profile/model/Undo guards.
   Round trips preserve root deck/draft/chips/cursor/focus and pinned execution state.
   Keep descendant permission/question parent routing and actual request bindings.

2. **One lower composer, three distinct inventories.** U45/U44 plus U78/U80/U88
   define raised background/split border/padding, at-most-five visible list rows,
   current/focused/hover states and tab-specific footer hints. Subagents and Shell
   are unconditional; Terminals follows resolved `session.terminal` (enabled on
   Linux, derived from platform in U88, not a donor user-config option or presence
   of a process). Left/right wrap tabs; Esc/Ctrl+C or clicking
   the `esc` hint closes the composer without stopping work. Tab labels themselves
   are text, not invented mouse buttons. Rows select on hover/move and activate on
   mouse-up. Existing Subagents Ctrl+A active/inactive, selected-running Ctrl+D
   interrupt, Enter child open, first-Up-close/Down-wrap and empty states remain.
   Current child running derives from its owner, not a completed launch badge.

3. **Shell tab and output viewer are operational.** U78 lists only currently
   running jobs of the source session, including a child session's own shell jobs;
   never reconstruct the list from immutable tool results. Empty text is `No shell
   commands`. Enter or row mouse-up opens the selected job's output; Ctrl+D kills
   precisely that job through its source session/Location identity, not the parent
   turn. Up from first closes; Down wraps; hints are conditional `output`/`kill`
   with actual configured keys. U79 dialog is centered/xlarge with source height
   `max(3, floor(H * .6) - 6)`, title/command and Running/Timed out/Killed/Exited
   code states, recent bounded 64-KiB cursor reads, live output and final flush.
   It retains its original job after the running inventory removes it. Follow-tail,
   user scroll/page/home/end, ANSI stripping/CR normalization and Esc close match
   source; content is **plain text**, not Bash grammar coloring. Cancel scoped reads/
   subscriptions on close; no permanent idle poller or unbounded retained output.

4. **Ctrl+B is a real control, not hide/filter/kill.** The delayed hint `Press
   ctrl+b to move running work to the background` uses effective key formatting and
   actual blocking Shell/Subagent state. Application dispatch freezes the target
   work identity and asks T45/T50 owners to convert that same admitted execution;
   it does not launch a duplicate process/child, cancel it or mark a TUI boolean.
   Verify parent/child continuation and durable notice ordering, no repeated result/
   delivery, conversion-versus-terminal races and late/stale controls after routing.
   Composer active/inactive filtering, session navigation and pane hiding are never
   conversion or interruption. Key priority follows the focused source surface;
   terminal Ctrl+C/D bytes must not leak into these global/composer actions.

5. **Terminals is a real interactive PTY, not Shell relabelled.** U80/U81 list
   existing session terminals plus `+ New terminal`, at most five visible rows,
   actual foregroundProcess/title fallback and selected/focused action styles/bold.
   Up/k and Down/j wrap through the New row; unlike Subagents/Shell, Up on first
   does not close. Enter or row mouse-up selects/creates; no invented kill hint.
   Selection is session-local/persisted through remount; removed selection clears.
   Without a visible terminal initial selection is undefined: immediate Enter is
   a no-op until Up/Down/hover selects a row. Activation calls composer.close before
   select/create; on a child this shared close navigates to parent. Qualify actual
   reference route/target identity, not an assumed child-local modal or pane.
   U82 `/terminal` creates new; toggle-on refreshes and chooses the **last terminal
   in inventory**, creating only if empty. Select opens the lower Terminals composer
   including `+ New terminal`, not a generic modal. Hide/toggle-off/close clears
   persisted selected ID and focuses session **without terminating the PTY**.
   Default leader-left/right focuses session/right pane, leader-down selects,
   leader-t toggles, leader-up closes. Exact layout integrates the right pane with
   sidebar/panel and tab rail, default half width, source width clamps/persistence
   and drag resize; child suppresses sidebar, not its own PTY capability.

6. **Terminal rendering, focus and resource ownership.** U83 is a VT screen, not
   transcript text or raw escape forwarding to the host. A vetted pinned native
   emulator renders cells/cursor; snapshot then replay cursor/ready ordering avoids
   lost/duplicated output/input. Actual PTY size follows pane resize. Theme semantic
   ANSI16/default fg/bg and attributes update with theme, distinct from code syntax.
   Focused terminal has raw-key priority: only the configured leader key or active
   leader sequence bypasses interception, not arbitrary non-leader pane remappings.
   Ctrl+C/D reach the PTY. First click into session only focuses it and consumes the matching
   release before permission/transcript actions; transcript wheel does not steal
   terminal focus. Hide/show/session change never kills a PTY; exit/disconnect
   restores session focus. T56 owns caps on screen/scrollback/escape/input/output/
   process counts and actual shutdown/reap. No host OSC/clipboard escape, provider/
   runner credentials or view-driven process replay. Donor daemon handoff survives
   server/client lifetimes; native ownership is limited to live `oc`, with safe
   interrupted/unknown crash recovery and no auto-recreated shell. Declare this
   runtime difference; do not waive pane/list geometry/colors/keys or fake persistence.

7. **Full shared grammar/style parity — VIS14/VIS35.** U86/U87/U89 are the
   grammar/alias/highlight-query and semantic style oracle, not OCR or a language
   keyword list. Inventory OpenTUI built-ins Markdown/JavaScript/TypeScript and all
   configured donor parsers/filetypes/aliases (including Rust, Python, Bash, JSON,
   Go, C/C++, Java, HTML/CSS, YAML/TOML, diff and the rest of U86). Freeze actual
   grammar/query asset revisions/provenance/licenses for the reference fixture:
   a pinned parsers-config URL can still point at floating assets. Native uses
   vetted pinned compiled grammars/highlight queries, no Node/Bun/WASM plugin host
   or production runtime downloads. Qualify token boundaries **and** fg/bg, italic,
   bold, underline and precedence for comments/keywords/keyword.type/functions/
   variables/types/operators/punctuation, Markdown headings/emphasis/links/inline
   code background, diff and feedback. Reasoning keeps source styles but muted fg.
   Unknown-language fallback is allowed only for genuinely unknown source languages;
   missing required grammar/reference asset is pending/blocked, not silent plain
   fallback or an all-languages waiver. Plain Shell commands/output, Read/Grep/Glob
   and Subagent labels stay plain; do not add decorative grammar coloring to them.
   One renderer/cache serves parent/child/live/replay and result-derived patch hunks;
   preserve streaming partial fences, Unicode/wrapping, theme/width invalidation,
   correct old/new diff lines/unified-split and bounded completed-block parsing.

8. **Ordered evidence and closure.** First actual-binary/protocol/SQLite/process
   SUB01/SUB02, TOOL13 extension and TERM01 establish real work/control/input/output
   facts; T44 can consume each minimal qualified slice without waiting for all
   T45/T50/T56 to finish. Keep existing three-child fixture and add a bounded
   canonical path: parent delegation → child open while blocked → live child shell
   → Ctrl+B → Shell output/selected-job kill or selected-child interrupt → notices/
   parent continuation; independently create/select/type/hide/reopen/resize a real
   terminal in the same fixture. Freeze a small shared mixed-code/patch transcript
   for parent/child with grammar token spans and source styles; finite representative
   language fixtures cover the full admitted grammar inventory without a duplicated
   viewport/state cross-product. Reopen/restart checks history/current-state/notice/
   selection reconciliation without execution; no literal sleep300/paid campaign.
   Then paired running-original/native full styled-cell/PNG/cursor evidence at
   80x24/120x40/160x48 covers controls, child transcript, all composer tabs, Shell
   dialog and right pane, representative Unicode/theme/focus/selection/resize and
   animation-on phase/state sequences plus source-specific off branches. Reuse
   VIS14/15/16/17/21/22/23/27/28/31/32/33/35/36/37 and A02/A08/A10 rather than new
   visual gates or duplicate safety suites. Own-tab/VIS41, native goldens, static
   widgets or off-only captures cannot close this segment. No masks/crops/tolerance/
   reference substitution; missing runnable reference stays BLOCKED_REFERENCE.
   Behavioral and visual results are separately reported with factual provenance.

## MCP modal parity — VIS40

Owner-approved 2026-09-27: MCP-модалка должна воспроизводить pinned OC2 v2.0.12
`2670273ff17da96f85c5826ced57aa1b368754fa`, не OCR. T44/R3/R5/V04 owns VIS40;
T46/R5/MCP08 owns its minimal typed status/control prerequisite. No all-T46 done
dependency, reverse gate, new task/client store/event framework or OAuth expansion.
Sources U50–U55 plus existing U02/U49; preserve pinned provenance/license notices.

1. **Exact source rather than OCR.** `/mcps` and palette `MCP servers` (`mcp.list`,
   Agent category, default direct binding none) open the real DialogMcp; slash
   autocomplete and dispatch share the existing registry. Current native
   `commands.rs` explicitly inventories `mcps` as unsupported: replace that expectation
   only when the owner-backed action actually works, keeping other unsupported entries.
   Title is `MCP servers`, right label `esc`, input placeholder `Search`. With no
   configured rows/query show `No items available`; a nonempty query with no matches
   shows `No results found`. No extra Enabled/Disabled column or checkbox from OCR.
   Server names come from current Location and are name-sorted, not production constants.
   A server with status disabled is selectable/connectable, not a disabled Select option.
2. **Geometry, filter and selection.** Use existing medium Dialog: width60 capped by
   terminal width−2, horizontally centered/top at height/4, no added border, dialog
   surface/base, paddingTop1 and backdrop black alpha150/255. No transcript reflow.
   DialogSelect gap1/paddingBottom1, title/search paddingLeft/Right4 and input top gap1;
   source row/title/footer paddings and alignment, hidden-scrollbar list with height
   min(rows, floor(terminal height/2)−6), actual width/truncation/Unicode behavior.
   Case-insensitive fuzzy filtering follows U51, not an ad-hoc substring/status search;
   query/caret and selected server identity survive status updates. Filter/removed-row
   reconciliation and scroll reveal follow the shared Select source. Do not add a
   current-dot marker: DialogMcp supplies no current/gutter. Compare narrow/resize
   and long names without altering caps or shrinking the overall comparison viewport.
3. **Truthful statuses and action labels.** Right footer: pending/local loading
   `Connecting …`; connected bold `Connected ✓`; failed `Failed !`; needs_auth
   `Sign in required →`; disabled `Disabled ○`. Local loading also covers an ongoing
   disconnect, not a new invented Disconnecting label. Toggle title derives from
   actual focused status: connected disconnect, failed retry, needs_auth sign in,
   otherwise connect; do not overwrite it merely from a local loading flag. Hint
   comes from actual `dialog.mcp.toggle` binding (default space). Empty configured
   list still renders disabled/no-op `connect space`; filtered no-match disables
   the action but may retain the last focused server's title, not force connect.
   While pending or already loading, repeat Space creates no duplicate operation. Never infer
   connected from enabled config, auth from arbitrary prose or success from a UI flag.
4. **Colors and attributes.** Resolve v2 `surface("dialog")` (U49), not legacy tokens
   or raw hard-coded RGB. Unselected status colors: success connected, error failed,
   warning needs_auth, muted pending/disabled; Connected retains source bold. Default
   dark: text.base#eeeeee, muted#808080, success#7fd88f, error#e06c75, warning#f5a742,
   dialog background#141414. Focused row uses action.primary focused bg/fg (default
   dark #fab283/#0a0a0a), title bold; this overrides ordinary status color. When Tab
   focuses the footer action, selected row becomes muted/raised.high and action gets
   source focus/bold; disabled action uses action.primary.disabled. Input/background/
   cursor, blank-cell backgrounds and alpha compositing also match. Reuse R2 for
   representative light/custom fallback, not a whole state×width×theme matrix.
5. **Keyboard, mouse and details.** Up/Down/Ctrl+P/Ctrl+N wrap, PageUp/PageDown use
   source ±10 selection policy, Home/End select endpoints; effective remaps apply.
   Mouse hover/down moves selection, row click follows Enter/select, footer click
   follows its action; Tab/Shift+Tab cycle available footer focus. Space is the
   real toggle action, not search insertion or editor input. Enter follows actual
   server status, not local loading: connected/disabled/pending does not connect/close;
   failed opens details with
   `enter to view error`. Details match `MCP server: <name>`, safe error/context,
   scroll, c/Copy and i/Investigate controls, copied feedback and Esc back to medium
   list. Use sanitized owner diagnostics, not donor raw exceptions. Investigate
   explicitly prepares a bounded safe diagnostic draft without auto-submit/tool
   effects; distinguish this requested draft replacement from ordinary close/back.
   Common modal Esc/selection/backdrop and Ctrl+C search-clear-before-dismiss rules
   restore original prompt/chips/cursor/focus and do not exit the app; no key leaks.
6. **Runtime owner and native boundaries.** MCP08 first supplies current
   Location/config-generation snapshots/events and actual async connect/disconnect/
   retry using existing clients/registry. Connected follows successful initialize
   and tools-list; disconnect performs cleanup/catalog removal. Preserve in-flight
   request immutability and publish controls/catalog at safe boundaries; bounded
   snapshot reads/resize/cancel must not wait behind the entire turn's MCP mutex.
   Revalidate server/Location/generation before effects; coalesce pending actions
   and ignore stale display completion, without abandoning owned cleanup. Reopen
   reads current facts; restart rebuilds from effective config, not saved connected
   labels or replayed operations. Runtime toggles do not silently edit config.
   D13: per-server failed attach is visible and turn continues, never silently disable
   required MCP; fatal cancel/cleanup/caps still fail. Preserve permissions/authority,
   strict codex_web versus generic negotiation, redaction and unknown-effect quarantine:
   retry cannot clear safety state or reexecute uncertain tool work. Disabled browser
   has zero automatic spawn/probe, including modal open/search. Explicit connect tests
   use configured fake stdio, not real browser/npx. OAuth/integration flows remain
   unsupported under GOAL; actual typed auth-required display and honest unavailable
   sign-in are a predeclared capability mapping, not working OAuth/full behavioral parity.
7. **State transitions, not invented animation.** Status itself has no Spinner,
   shimmer or periodic animation; `Connecting …` is literal text. Compare actual
   pending→connected/failed, connected→local-loading→disabled, failed→retry plus
   row/footer focus and disabled branches, with animations on/off where relevant to
   surrounding UI/cursor. Do not borrow the VIS39 Braille spinner or invent a fade.
   Update from owner events; idle modal has no polling/timer per server. Reuse VIS31
   demand-driven deadlines/resource evidence and preserve bounded snapshots/queues.
8. **Qualification.** First rebuilt actual binary/fake HTTP+stdio proves MCP08
   handshake/catalog/control counters, safe errors, next-request registry and owned
   cleanup. Then run actual pinned DialogMcp/native with identical fixture/state/theme
   and capture full styled-cell/PNG/cursor at80x24/120x40/160x48. Cover empty, mixed
   connected/disabled, search match/no-match, pending/success/failure/retry/disconnect,
   error details/back/copy/investigate, footer/mouse/keyboard focus and representative
   Unicode/long-name/scroll/resize. Check real action effects independently; a static
   renderer or OCR-derived golden is not qualification. Reuse VIS10/11/19/21/22/23/31,
   MCP07/MCP05/AUD23 and applicable A02/A06/A08/A10 evidence, no duplicated backend
   negative matrix or paid/browser campaign. Reference missing is BLOCKED_REFERENCE;
   no crops/masks/tolerances or discarded capability rows to hide gaps. Report native
   unsupported-OAuth capability and sanitized-diagnostic differences before capture, preserve failed attempts
   and distinguish backend from visual results; this slice does not close T46 R4 live.

VIS40 mandatory, NOT_RUN/evidence empty until actual qualification. Plan approval
does not change task statuses, previous evidence or baseline. Independent T44 work
continues while the minimum MCP08 prerequisite is delivered by the existing owner.

## Session tab spinner and hover-marquee parity — VIS39/VIS41

Owner-approved 2026-09-27 после RECON: вращающийся индикатор работающей вкладки
остаётся под VIS39; **VIS41 / R3/R5/V03** добавляет бегущее название при hover.
Reference — pinned OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`,
U46–U49 и новые U56–U59. Один T44 owner, без второго spinner gate/task/store.
Own-session spinner и marquee независимы от T45; family busy/attention/unread
по-прежнему потребляют его минимальный R3/SUB01/SUB02 projection/lifecycle slice.

RECON native: `shell.rs::deck_tab_line` рисует постоянный `⠋` и начало title с
trailing fade, а `app.rs::next_ui_deadline/tick_ui` не имеют вкладочной фазы/offset.
`render_deck_tabs` определяет hover через `tab_close_cell`, который запрещён при busy:
визуальное наведение ошибочно связано с возможностью закрытия. Существующий тест
`close_glyph_only_on_hovered_eligible_tab_and_title_fade_moves_left` проверяет резерв
для крестика/fade, не прокрутку текста во времени. Это source-level finding, не PASS.

1. **Working-tab indicator — existing VIS39.** Follow U46/U47 actual typed state:
   runs = busy without attention; permission wins question. Status mode dots frames
   `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`, 80 ms; attention `!`/`?`, numbers mode keeps the
   ordinal. Animations off keeps first frame `⠋`, not row `⋯` or scanner `[⋯]`.
   Preserve source mount/phase/transition/color rules and U48 pulse/glow evidence,
   not global frame synchronization. Existing source-only spinner variants do not
   authorize new config options. Parent own-running, root-family busy, permission
   wait and actual terminal/unknown outcomes remain distinct; no prose inference.
2. **Hover and width.** Use real pointer hit testing and stable session ID from the
   existing bounded owner projection, not title text or deck index as identity.
   Separate presentation hover/marquee from close/switch eligibility: busy tabs can
   receive visual hover without permitting a forbidden action. Do not change busy/
   permission/Location action guards or invent switch/close/interruption semantics.
   Resolve U56 actual resting/hovered widths, ordinal prefix and close reservation:
   horizontal hover reserves two more title cells; vertical noncompact one more.
   Short/exact-hover-fit titles do not move; a resting fit may overflow on hover.
   Compact vertical rail passes Infinity and has no title marquee. Reconcile actual
   title updates, resize/layout, hidden/removed tabs and modal routing by source rules
   and painted geometry; do not restart an unchanged hover on status-only updates.
3. **One cycle, not an infinite ticker.** U56 enter starts at offset0, waits600 ms,
   sets offset1, then advances by one display cell every80 ms. U57 cycle width is
   title plus ` · `; after one cycle return offset0, stop the step deadline and fade
   the leading edge out. Active hovered identity remains until leave/reset: repeated
   motion within the same tab must not start a second cycle. Match U56 deferred
   hover-leave cleanup when moving into nested tab controls, ordinary leave/reset
   and component disposal; no stale continuation on another session/Location.
4. **Text and styled cells.** Use source grapheme/display-width slicing without
   splitting wide glyphs; title comes from current metadata, never screenshot text.
   Four-grapheme trailing/leading fade uses U56 alpha/tints and source250-ms leading
   opacity tween/easing. Generated separator dot blends toward background at0.55;
   a dot already in the title is not that separator. Match actual selected/inactive/
   hovered source fg/bg, attributes and blank backgrounds through U49 and R2.
   Hover marquee is distinct from automatic-rename `title_shimmer` and from VIS28.
   **Animations off is component-specific:** tab spinner is static, but U56/U58
   still advance marquee after600/every80 ms with one cycle. Only its leading fade
   tween jumps instead of interpolating. Do not freeze the title or force a universal
   `⋯` fallback because `animations:false` was set.
5. **Implementation/scheduling slice.** Extend existing `app.rs` tab state and
   `tui_cmd.rs` projection only as needed for stable IDs, phases and actual status;
   render in `shell.rs`. Reuse monotonic `next_ui_deadline`/`tick_ui`, not a separate
   animation engine, per-widget timer or polling loop. Render/input/status updates
   do not reset animation clocks. A motionless pointer still progresses the delayed
   marquee; off-mode marquee still has active deadlines. Once the one cycle/fade and
   other visible animations settle, no periodic wakeup/redraw remains. Spinner work
   remains only for actual visible running status indicators. Bound state/queries/
   caches to the retained/visible deck, not all archived sessions/transcripts.
6. **Checks and qualification.** Extend nearest Rust clock/state/render regressions
   for spinner branch/cadence and marquee delay/one-cycle/leave/Unicode; source U58/U59
   fixtures are an oracle, not executed proof. Actual rebuilt binary with bounded
   fake-provider barriers proves running→completion/cancel and hover→delay→motion→
   settled/leave without extra input, submission, selection or tool effects. Then
   paired running-original/native full styled-cell/PNG/cursor sequences at existing
   80x24/120x40/160x48 profiles compare matched phases, on/off behavior, short/long/
   exact-hover-fit and representative Unicode titles, active/inactive/busy hover,
   close-cell movement, resize and applicable vertical/compact branches. Reuse VIS39
   family/attention and SUB01/SUB02 evidence when available, VIS05/06/11/21/22/23/28/31
   and S04/S06/S07 invariants; no duplicated backend/theme×width×state matrix or paid
   campaign. Static/native-only goldens or animation-off-only captures are not PASS.
   Record actual effects, idle deadlines/counters and capture provenance separately;
   preserve failed attempts. Missing reference is BLOCKED_REFERENCE, not a masked
   region, crop, relaxed tolerance or rewritten baseline.

VIS41 mandatory, NOT_RUN/evidence empty until qualification; VIS39 remains open.
Plan approval changes neither task execution statuses, historical reports/evidence
nor baseline. This plan-only delivery does not implement either animation.

## Service config/startup error parity — VIS19/VIS40/VIS42

Owner-approved 2026-09-27 after RECON. [T46 R6/R7](../docs/goals/2026-09-22-mcp-attach-parity.md)
adds real donor config/cwd/environment and nonblocking initial MCP connections;
[T51 R1–R3](../docs/goals/2026-09-27-startup-fault-isolation.md) adds plugin/provider
isolation and safe startup diagnostics. T44 owns the UI consumer/qualification,
not another service owner or a whole-task completion barrier. Independent T44 slices
stay ready. Pinned OC2 commit unchanged; U60–U65 extend U50–U55/U02/U49.

1. **Existing VIS19/VIS40 startup extension.** Config-admission failure, failed/slow
   initial connection and unexpected close must be visible before any turn warning.
   Status/details consume actual current Location/config-generation inventory, including
   invalid recognized entries and disabled records. Valid disabled chrome with environment/
   timeout opens usable TUI without npx/browser spawn/probe; opening/searching modal cannot
   launch it. Healthy/slow/failed fixture shows real independent state and available tools.
   Connected requires init/catalog success; close/disconnect retires unavailable tools,
   failed relist keeps last healthy catalog per MCP07. Explicit admitted retry/config
   repair restores true state, never clears quarantine or reexecutes uncertain effects.
   Existing VIS40 geometry/actions/colors/focus/details/idle requirements still apply.
2. **New VIS42 / R5 / V04 — plugin failure inventory.** /plugins and palette entry
   use real bounded native module/admission facts. Reproduce pinned PluginsDialog list,
   failed footer/gutter, semantic error/focus roles and safe error detail access, search/
   empty/long Unicode/scroll/resize/close routing through shared Dialog/Select. Rejected
   unknown marker is failed/UnsupportedPlugin, not hidden by donor unsupported-row filter,
   and not active; healthy compiled aliases stay usable. Requested/current retained
   activation differs truthfully when relevant. Do not invent registry install/update/
   hot-load controls; unavailable donor actions have predeclared native capability mapping.
   No raw target URL/absolute path/error/env secret in labels/details/copy/investigate.
3. **VIS42 service/readiness diagnostics.** Status/sidebar/dialog/error feedback follows
   real owner facts and pinned U63/U64/U65 components. A provider discovery/connect/
   credential failure leaves TUI/history/model picker usable, shows actionable safe cause,
   does not silently select a different model or show mock response. Unavailable prompt
   fails before generation/tool effects; admitted recovery or explicit model choice
   becomes ready and next request uses it. Preserve ordinary prompt/chips/cursor/focus
   on error/detail/back/reopen; late events cannot corrupt another Location/session.
4. **Structured diagnostic/fatal native surface.** Show redacted source/field/service/
   stage/safe-code/retryability and allowed next action in safe details and fatal frames,
   not background conversation rows (2026-10-02 clarification below), raw exceptions or
   generic Configuration load failed alone. Details/copy/investigate share safe owner payload;
   no regex/keyword inference or auto-submit. Fatal trust/policy/storage/data-root/
   recovery/cleanup/caps remains non-success. Native fatal startup frame is a declared
   functional difference, not a nonexistent donor pixel reference; test exit/focus/
   terminal restoration and concrete safe source/cause/action without disguising it as
   paired parity. Optional malformed config cannot bypass mandatory effective policy.
5. **Qualification.** Rebuilt actual binary proves MCP09/MCP10/MCP08 and CFG09/CFG10/
   UI07 startup/failure/recovery/request/effect/cleanup assertions first. Then paired
   running pinned-original/native production components with identical safe fixture/
   theme/state/profile prove full styled-cell/PNG/cursor list/details/error/recovery,
   real keyboard/mouse, focus/draft restoration and reopen at existing 80x24/120x40/
   160x48 profiles and representative long labels. Use fake services/controlled plugin
   failures, not npm installs/real browser or a paid campaign. Safe-diagnostic wording,
   compiled-only plugins and unsupported OAuth/CodeMode/protocol/actions are named
   unmasked differences before capture, never hidden rows/fake buttons/native-golden
   PASS. Reuse VIS10/11/19/20/21/22/23/31/40 plus backend evidence rather than repeat the
   negative/theme×viewport×state matrix. Bound owner snapshots/queries/queues; no idle
   polling/new framework. Preserve reference provenance/failed attempts; missing
   executable reference is BLOCKED_REFERENCE, not relaxed tolerance or baseline edit.

VIS42 mandatory NOT_RUN/evidence empty; VIS19/VIS40 remain open until current backend
and paired qualification. No execution status/history/PASS rewrite. T46/T51 own backend
scenarios, T44 owns VIS only; minimal slice dependencies have no circular task completion.

## Clean dialogue and service diagnostics

Owner-approved 2026-10-02 after read-only RECON of the reported OCR: technical
config/plugin/provider warnings before the assistant response obstruct daily TUI use.
Approval requests a plan patch and current-branch commit/push, not runtime changes or
automatic resume. Existing T44/R5/V04/VIS42 owns diagnostic delivery/status/details;
T44/R4/V06/VIS17 owns the compact live-preview indicator. VIS19/VIS40 retain MCP
visibility and VIS43 retains actual provider retry; no new task or acceptance ID.

### Source-grounded defect and reference

- At recon HEAD `9d8b1b818`, `oc/src/tui_cmd.rs::service_warnings` converts current
  config/plugin/provider diagnostics to full strings and calls `TuiState::push_warning`.
  initial_state, refresh_provider_views, reload and Location adoption invoke this path;
  refresh republishes the full set without comparison. `oc-tui/src/app.rs::push_warning`
  creates synthetic `(warning: …)` transcript rows, not committed raw history records.
- `app/live.rs::apply_catalog` also places full unavailable provider/selection diagnostics
  in the replaceable note. `app/mcp.rs::apply_mcp_snapshot` has change comparison but
  still appends diagnostics, potentially overlapping startup service delivery. Matching
  turn-completion `TurnReport.warnings` is another path for background degradation.
  Removing service_warnings alone does not close all of the reported behavior.
- Pinned OC2 U106 (`plugin/context.tsx:459–487,539–562`) notifies newly failing plugin
  states only and routes details to `/plugins`; U107 (`app.tsx:531–559`) alerts MCP
  failed/needs-auth once per state and directs to `/mcps`. U62–U65 provide the existing
  inventory/status/details reference. This is source RECON, not executed visual PASS.
- `[Live preview truncated; durable parts remain available through history and /cards]`
  is a separate projection-bound indicator from `app/transcript.rs`, not a config
  warning. Local live-part eviction or a truncated owner projection can set it; OCR
  alone does not establish why a particular short visible answer triggered it.

### Required presentation

1. **Clean dialogue.** Zero nonfatal background config/plugin/provider/MCP diagnostic
   rows before the first answer or between later messages. Ignored DCP/legacy compaction
   fields, unsupported plugins and discovery/catalog pending do not become assistant/
   system-looking conversation text. Do not rewrite existing messages, stored outcomes
   or model context; service warnings are presentation state, not durable conversation.
2. **Visible, usable diagnostics outside dialogue.** Use existing compact status/counts
   and safe Settings, `/plugins`, `/mcps` details. Show the real failed inventory and
   omitted counts, not hidden rejected entries or fake healthy labels. Brief actionable
   summaries can say that settings need review, a plugin is unavailable or the catalog
   is loading; these examples define meaning, not a new localization requirement.
   Source/field/stage/code/retryability, requested/current identities and full opaque
   hashes stay in details/copy/investigate, not an expanded technical toast. Preserve
   safe provenance and unsent investigation/focus/draft/cursor; no raw identity/config/
   URL/env/ANSI disclosure, source reads/launches or automatic submission on detail open.
3. **Change-sensitive delivery.** Identical snapshots, provider refresh, unchanged reload
   and parked-tab updates cannot re-notify the same problem. Notify briefly once for
   a new failure, changed cause, or a new failure after recovery; initial issues may
   use one aggregate summary instead of one toast per entry. Pending is connection
   status, not a failure alert. Recovery clears only that service's stale status, not
   unrelated operation feedback. Use bounded current owner state scoped to Location/
   service/source/cause; generation guards reject stale events, but an unchanged issue
   does not re-alert merely because a generation was republished. No lifetime ledger,
   second registry or permanent polling. The same background facts delivered through
   startup, MCP snapshot or completed-turn warnings must not produce duplicate alerts;
   route by existing structured owner facts/caller purpose, never warning-text regex.
4. **Genuine failures remain loud.** A submitted unavailable request gives one concise
   actionable error before generation/tool effects, with safe details accessible.
   Keep explicit selection/no fallback, truthful tool failed/denied/cancelled/unknown
   cards, actual generation errors and VIS43 retry-footer. Nonfatal turn-specific
   feedback remains visible in its appropriate status/card surface; do not globally
   delete push_warning callers or drop warnings without a replacement route. Fatal
   trust/policy/storage/data-root/recovery/cleanup/caps and headless stderr/nonzero/
   NDJSON remain unchanged. This supersedes only verbose/repeated background TUI rows,
   not T51/T46 failure visibility/admission or D13 degradation.
5. **VIS17 live-preview indication.** Replace the long synthetic line inside the answer
   stream with one unobtrusive viewing-status indicator outside message text, present
   only when the actual projection is truncated. Preserve working history/cards access
   and bounded live/durable projections; distinguish saved parts from producer-discarded
   bytes, never promise recovery of tool-truncated data. An untruncated short answer
   has no indicator. No raising memory/part/model caps, history deletion, reexecution
   or persistent assistant warning. Compact native indication is a predeclared display
   difference, not a fabricated OC2 pixel reference.

### Ordered execution and qualification

First user-facing UI slice **after explicit T44 resume**, preserving active T50/dirty
shell-output work and T55 safe-handoff priority. Use completed T51/T46 minimal owner
facts without reopening their historical completion or waiting for whole backend tasks.

1. Extend nearest binary `pty_t39/{plugin_admission,provider_readiness}.rs` and existing
   startup/config/MCP fixtures to reproduce the mixed pre-answer spam and refresh path;
   extend nearest TUI snapshot/delivery tests for repeated and changed states.
2. Change only existing TUI/binary consumers: service_warnings/initial_state/provider
   refresh/reload/Location, apply_catalog/app/mcp and matching completed-turn delivery;
   reuse status/toast/details owners. Keep ownership/admission/error/redaction contracts.
3. Move the VIS17 indicator at its transcript/status owner and extend the existing
   `app/tests/lifecycle.rs` live-part eviction case plus a short untruncated answer.
4. One bounded actual rebuilt-binary fake-service scenario proves zero service warning
   rows in the dialogue, change-sensitive notification counts, accessible safe details,
   pending/recovery status, unavailable pre-effect refusal and stale routing. Include
   reopen/restart/resize and ordinary draft/focus/cursor restoration. Assert dialogue
   region separately from detail screens/PTY logs; technical details must remain there.
   Reuse CFG09/CFG10/UI07/MCP09/MCP10, genuine failures/VIS43 and A10 bound regressions,
   not a new backend negative matrix. Preview indication shares existing eviction proof.
5. Then existing running pinned-original/native full styled-cell/PNG/cursor captures
   at established profiles prove status/brief alert/details/back/recovery; disclose
   sanitized wording/compact native indicator before comparison, no masks/crops/fake
   components/native-golden PASS. Method: docs/TEST_PLAN.md clean-dialogue subsection.

Plan-only change envelope: GOAL, T44 spec/amendment/ACCEPTANCE/SOURCES, CONFIG/TEST_PLAN,
planning/tasks, live M8/M9 scheduling references and the two derived M9/T44 indexes
(title synchronization only; canonical progress state and checkpoint leaves unchanged).
No production code, user config,
new schema/dependency/framework/store/paid campaign, progress execution-state change,
historical report/checkpoint/baseline rewrite or relaxed VIS gates. VIS42/VIS17 remain
mandatory NOT_RUN/evidence empty, T44 PAUSED; plan validation is not A08/A13/runtime PASS.

## Provider-error retry footer — VIS43

Owner-approved 2026-09-29 after OC2 retry RECON and independent audit:
[T54/RET01](../docs/goals/2026-09-29-provider-retry-parity.md) owns classified
generation errors, finite continuation, durable assistant-span facts and actual
headless/PTY effects; future T53/GO03 wires consume its policy. T44 owns only
visible/interactive VIS43 after **explicit resume** from PAUSED, without
whole-T54/T53 completion dependency or duplicate retry owner. Pinned sources
U69–U73; startup/MCP retry VIS19/VIS40/VIS42 is a separate feature.

1. **Assistant footer.** On an assistant bearing a retry descriptor, draw
   `⚠ Retrying in Ns · attempt N · <safe error>` in warning color, left padding 3;
   `N = max(0, ceil((at - now)/1000))`. At zero draw `⚠ Retry due · attempt N · …`,
   never `in 0s`. Update immediately, then at 1s intervals until due/clear,
   even with animations off; stop the timer at due and use existing UI deadline
   scheduler, not a new spinner or idle poll. Ordinary text wrapping; no
   truncation/detail modal. Suppress a simultaneous `Error:` line, place one
   blank row before agent/model footer and mute the agent only if the assistant
   actually has an error. Preserve normal model/width/timing/usage footer rules,
   disabled animation and surrounding busy indicators without adding a retry badge.
2. **Lifecycle and causality.** Publish retry only from T54 owner state; neither
   provider failure before scheduled wait nor intermediate step failure is a
   terminal TurnFailed/headless done. Semantic `step.started`, not physical POST
   or `response.created`, clears a matching span's retry or the last *unfinished*
   assistant before a new span. A failed completed partial span can retain its
   own historical retry even after the continuation's new assistant succeeds;
   that descriptor never means active wait, cancel authority or restart work.
   Active retry/terminal/cancel/shutdown changes only currently unfinished
   owner state. Keep safe error message controls/redaction and session/turn/span
   identity; ignore stale foreign events.
3. **Navigation.** Retained same-Location tab park/return/reopen and actual
   history read preserve the notice, draft, selection, scroll and cursor;
   address events to focused or parked owning view through existing receiver.
   Do not let an older query overwrite a later retry-clear/delta (local ordered
   event-loop boundary suffices). Keep accepted worker running, Location/model/
   permission guards and one-request-at-a-time admission; do not add parallel
   session execution just for a warning.
4. **Qualification.** First prove T54/RET01 fake-provider classification,
   physical request count, cancellation and durable effect safety plus actual
   rebuilt binary PTY/headless routing. Source-derived clock/render assertions
   at donor 44/100 widths cover 3→2→1→due, reschedule/attempt, step start,
   expired due, terminal and cancel. Paired running pinned-original/native
   full styled-cell/PNG/cursor captures at matched clock/phase, fixture and
   environment cover retry, success/exhaustion/quota, partial→new span→success
   with old historical footer, park/reopen and representative long Unicode
   error. Reuse existing 80x24/120x40/160x48 profiles and relevant narrow
   wrapping boundaries, not every state×width×theme combination. VIS31/normal
   footer checks cover idle deadlines/resource behavior. No masks, relaxed
   tolerance, native-only golden parity claim, paid fault campaign or rewrite
   of prior evidence; reference unavailable means BLOCKED_REFERENCE.

VIS43 mandatory NOT_RUN/evidence empty until actual paired qualification.
Neither a plan commit nor RET01 PASS resumes/finishes T44 or proves full
provider-retry visual parity; no new task/progress engine or historical PASS.

## Middle Click tab close — VIS44

**Owner-approved 2026-09-30; T44 / R5 / V04; implementation pending.** Источник —
сообщение владельца: «нельзя закрыть вкладку в TUI наведясь курсором на вкладку
и нажав Middle Click (при этом в оригинале TS 2 — вкладка закрывается)», затем
«Вноси в план работ развернуто и коммит пуш в текущую ветку». Это отдельный
проверяемый bug-fix slice существующей T44, не новая задача или tab framework.
VIS39 spinner и VIS41 hover-marquee не доказывают обработку средней кнопки.

### RECON и pinned источники

Reference — OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`:

- **U74**, `packages/tui/src/component/session-tabs.tsx:81–83,869–877,1630–1639`:
  `MIDDLE_MOUSE_BUTTON = 1`; vertical и horizontal tab box на `onMouseDown`
  сбрасывают drag и вызывают `tabs.close(sessionID)`, затем `preventDefault`/
  `stopPropagation`. Horizontal Middle Click также освобождает close hold;
  для `NEW_SESSION_TAB` передаёт `undefined`. Отпускания кнопки или крестика
  обработчик не ждёт; compact vertical rail проходит тот же tab-box путь.
- **U75**, `packages/tui/src/context/session-tabs.tsx:315–344,398–423`:
  close убирает вкладку из persisted deck и сохраняет её для reopen, но не
  удаляет durable session. При закрытии неактивной вкладки route не меняется;
  при активной выбирается survivor, а без него Home. Busy-запрета здесь нет.
  Это source-derived semantics, не выполненная проверка оригинала/native.

Native RECON на `4ac332f109dc06e21e930b9bf5c07340268b23c4`:
`crates/oc-tui/src/app/input.rs::handle_mouse` возвращает tab intents только
через Left down/up; `MouseButton::Middle` не обрабатывается. `app/tabs.rs::tab_hit`
различает тело/крестик/Add, а `tab_close_cell` отвечает одновременно за hover
крестика и close eligibility. В compact rail крестика нет: это не основание
запретить Middle Click. `crates/oc/src/tui_cmd.rs::apply_mouse_outcome` и
`PanelIntent::CloseTab` уже ведут к `LoopState::close_tab`, checked saved deck
и replacement view; новый путь должен переиспользовать именно этого владельца.
Ближайшие tests: `app/tests/tabs.rs` и `tui_cmd/tests/{routing,lifecycle}.rs`;
actual-binary close/reopen case уже есть в `oc/tests/pty_t42.rs`.

### Обязательное поведение и границы

1. **Whole-tab target.** Обычный Middle Click без modifiers внутри видимой
   нарисованной вкладки возвращает CloseTab для этой вкладки на mouse-down.
   Предварительный Left Click, активность вкладки, отдельный mouse-move и
   попадание в крестик не требуются. Title/ordinal/blank fill принадлежат той
   же вкладке. Использовать actual painted/clipped hit test для horizontal,
   vertical и compact rail; скрытые вкладки, overflow markers, Add и transcript
   не становятся close targets. Synthetic Home/New session следует существующей
   native CloseTab доступности: bare Home без real tabs не закрывает приложение.
2. **One gesture, one action.** Consume среднее нажатие в tab routing, сбросив
   старое tab press/drag намерение. Последующие Middle Up/Drag не закрывают
   следующую вкладку и не запускают ActivateTab/NewSession/left-release action.
   Middle Close не включает пятисекундный cross-close hold, даже если нажатие
   пришлось на крестик; U74 освобождает его. Не менять left-cross down/up,
   keyboard/palette/slash close, wheel и modal/composer routing ради нового пути.
3. **Existing close owner.** Неактивная вкладка закрывается без её предварительной
   активации и без потери draft/cursor/scroll текущей. Активная использует
   существующий native выбор replacement view; последняя real tab ведёт в Home,
   не завершает процесс. Reuse checked deck saves и существующую диагностику:
   отказ не должен локально убрать вкладку или потерять draft. Сессия, raw
   messages/turns и DCP history остаются; реальный history reopen доступен,
   restart восстанавливает committed deck без самовольного возврата closed tab.
   Закрытие не отправляет prompt, не генерирует ответ и не переисполняет tools.
4. **Busy boundary — явное отличие, не скрытый parity claim.** Original U74/U75
   допускает busy-close; native `LoopState::close_tab`/tab guards запрещают его.
   Запрос добавляет отсутствующий mouse gesture, **не разрешает снять existing
   busy/permission/Location/read-only/lifecycle guards или автоматически отменять
   работающий turn**. Проверять доступность отдельно от видимости hover крестика;
   не bypass через Middle Click. Полный busy-close parity остаётся вне этого
   узкого среза до отдельного утверждения owner-backed execution/cancel/navigation
   семантики. Не заявлять full donor behavior по успешному закрытию idle tab.

### Ordered implementation slice после explicit resume

1. Сверить актуальный Git/runtime путь и минимально расширить существующий
   `app/tests/tabs.rs` regression: Middle Down в теле неактивной eligible tab
   должен дать её CloseTab, Middle Up после обновления deck — никакого intent.
   RED на прежнем коде фиксирует именно reported failure; не создавать новый
   test target/helper или копию tab renderer.
2. В `app/input.rs` добавить отдельную Middle Down ветку и переиспользовать
   `shell::tab_strip`/`layout` painted hit test. В `app/tabs.rs` отделить close
   eligibility от glyph/hover/compact visibility только насколько нужно этому
   пути. Сохранить existing stable session/deck mapping и owner revalidation;
   не выбирать вкладку по title и не удалять state из input handler.
3. Проверить `tui_cmd.rs::apply_mouse_outcome` и `mouse_close_snapshot`:
   Middle Down на glyph не должен ошибочно включить Left-cross hold. При
   необходимости различить origin минимально в существующей маршрутизации;
   обычный `PanelIntent::CloseTab` и `LoopState::close_tab` остаются общим путём.
   Не добавлять второй API/store, MRU navigation, migration, middle-button
   setting или новую worker/cancel/background-tab систему.
4. Проверить ближайшие regressions и actual rebuilt binary под bounded PTY,
   затем парные pinned-original/native frames и effects. Срез независим от
   завершения T45/T50, не требует paid provider вызовов. После успешных
   targeted checks — affected-crate и обязательные gates T44 по runbook;
   reviewed implementation/evidence commit и delivery отдельно от plan commit.

### Минимальная квалификация VIS44

- **Input regression:** один nearest scenario показывает прежний сбой, правильный
  inactive target и отсутствие второго действия на release. Расширить ближайшие
  existing geometry/guard assertions лишь для materially distinct vertical/compact,
  outside/Add/overflow/modal и busy ветвей; не делать Cartesian test matrix.
- **Actual binary:** existing `pty_t42` close/reopen fixture либо nearest retained
  PTY scenario получает реальную SGR Middle Down/Up последовательность. Закрыть
  inactive и active tab, затем last real tab; проверить focus/draft survivors,
  Home/живой процесс, committed saved deck после restart и actual history reopen.
  Fake-provider counters подтверждают отсутствие generation/tool effects. Existing
  left-cross/keyboard/palette, save-failure и busy tests переиспользуются, не копируются.
- **Paired reference:** одинаковые idle sessions/state/profile и real Middle Click
  на запущенных pinned original/native. Full before/hover/after styled cells, PNG
  и cursor в existing 80x24/120x40/160x48 profiles, с representative horizontal,
  vertical/compact и clipped geometry, не полный width×state product. Подтвердить
  фактический closed session identity/reopen независимо от картинки. Native-only
  goldens и crop/mask не закрывают gate; missing reference — BLOCKED_REFERENCE.
  Busy refusal и существующие native replacement-view отличия фиксировать отдельно,
  не подменять ими matched idle fixture и не маркировать universal donor parity.

VIS44 **mandatory, NOT_RUN, evidence empty** до фактической квалификации.
Этот план не меняет historical PASS/baselines, execution statuses или active T50
и не снимает **PAUSED T44**. После explicit resume выполнять slice по обычному
one-active-task workflow; plan commit/push не является implementation/evidence PASS.

## Tool-output phantom caret and prompt blink — VIS16/VIS31 (2026-10-01)

Владелец сообщил: при наведении на большую подсвечиваемую карточку tool call
иногда появляется фантомный `Caret`, мерцающий и прыгающий по строкам при быстром
движении мыши. Отдельно потребовал сохранить корректный caret blink в поле промпта,
затем утвердил запись плана и commit/push в текущую ветку. Оба результата обязательны
в **T44/R4–R5/V06/VIS16**, с reuse **VIS31** для frame lifecycle/settled idle;
не новый VIS ID/task или permission на resume. «На весь экран» здесь означает
существующий доступный просмотр/раскрытие recorded tool output, не требование нового
fullscreen UI, Code Mode/execute или восстановления tool-truncated данных.

### RECON: наблюдение, source facts и ещё не подтверждённая причина

Read-only native RECON: HEAD `71fcd0f06bf618f06d4b5972fb312123f24378d2`;
dirty T50 file-mutation paths не участвовали в выводе о cursor lifecycle.
Новые source entries P17–P22/U103–U105/D11–D12 в `SOURCES.json` не заменяют
исходный audit snapshot и не являются выполненной runtime qualification.

- **U103**, pinned OC2 `routes/session/index.tsx:2784–2810,2984–3041`:
  BlockTool меняет hover background, ShellDisplay раскрывает/сворачивает preview
  по mouse-up; selected text блокирует click. Hover handlers не перемещают caret
  в вывод. Обычный Read не становится полнофайловым viewer.
- **U104/U105**, `config/index.tsx:57–64,296–302` и
  `component/prompt/index.tsx:1736–1753`: cursor shape/blinking отделены от mouse;
  textarea получает `cursorStyle={config.cursor}`. `style:default` сохраняет
  terminal setting, а blinking при нём не переопределяется. Это источник semantics,
  не новый config slice. Установленных исходников `@opentui/core` при RECON нет:
  hardware cursor lifecycle/blink оригинала нужно проверить executable captures,
  не приписывать renderer отсутствие/наличие hide или reset по TSX handlers.
- **P17–P20**, native `tui_cmd.rs::drive_ui`,
  `app/transcript.rs::paint_transcript_at`, `shell.rs::render_prompt`, `terminal.rs`:
  hover меняет стили, composer задаёт Frame cursor position; actual draw не защищён
  предварительным hide/synchronized-output transaction. TerminalOutput пишет в
  Stdout, не собирает весь кадр в атомарную операцию. Cursor color — отдельный OSC,
  не причина для удаления подсветки/цвета и не гарантия невидимости курсора.
- **D11/D12**, закреплённые Cargo.lock `ratatui 0.30.2`, core/crossterm `0.1.2`:
  backend.draw выдаёт MoveTo/Print для changed cells; apply_buffer_with_cursor
  сначала применяет diff, затем **show_cursor → set_cursor_position**. Show имеет
  отдельный flush. Видимый terminal cursor может оказаться на строках repaint и
  кратко показываться перед окончательным MoveTo. Поэтому одиночный hide перед
  обычным draw недостаточен как доказательство исправления.
- **P21/P22**: read-only `dialog.rs::render_card_detail` не назначает своего caret
  после underlying prompt; `oc/tests/support/screen.rs::render_screen` хранит только
  конечную позицию, не visibility/shape/blink. `pty_t39.rs::settled_cursor` специально
  ждёт окончания output. Эти final-state assertions не обнаруживают transient caret.

Вероятная причина — видимый hardware cursor во время diff/Show-before-MoveTo,
но конкретный пользовательский flicker **ещё не воспроизведён** actual binary/PTY.
Форма/версия terminal frontend и его blink behavior не предоставлены. Первый RED
должен отделить hardware cursor от нарисованного glyph, mouse pointer и ошибочного
focus/hit-test; если наблюдение опровергнет гипотезу, расследовать тот же контракт,
а не закрывать баг по одному исправленному конечному кадру.

### Frozen behavior и границы

1. **Нет фантома во время вывода.** Entry/быстрые mouse moves по строкам/leave,
   повторный entry, click-expand/recollapse, recorded-output viewer scroll/close
   и resize не показывают caret в tool content, на repaint boundary или padding.
   Hover только подсвечивает: input focus, draft/chips/selection/editor caret и
   tool identity не меняются; существующие click/selection/scroll semantics остаются.
2. **Правильный input owner.** В обычном composer caret сохраняет исходную позицию,
   форму, цвет и эффективное blink поведение. Search/редактируемая форма владеет
   своим caret, read-only result overlay скрывает underlying composer caret;
   закрытие восстанавливает прежний draft/focus/caret. Не прятать cursor навсегда,
   не отключать mouse/hover/expand и не удалять символы, похожие на caret, из
   настоящего tool output ради сокрытия бага.
3. **Blink при нагрузке обязателен.** При включённом blink и неизменном editor caret
   есть нормальные visible/hidden cycles и в покое, и при непрерывном hover repaint.
   Повторные Hide/Show, MoveTo в ту же позицию или shape/color updates не должны
   подавлять blink, постоянно перезапускать его до первой hidden phase, превращать
   caret в steady-on/steady-off или вызывать нерегулярное мерцание. Сохранить
   effective nonblinking/terminal-default setting; не заставлять всех мигать.
   Не обещать совпадение абсолютной фазы двух независимых wall clocks: сравнивать
   matched phases и наблюдаемые cycles/cadence с recorded idle baseline одного
   frontend/profile. Статичный кадр или blink только после остановки мыши не PASS.
4. **Безопасная output boundary.** В existing terminal/frame owner защитить весь
   paint, включая first background/resize/clear: transient cursor moves не должны
   стать видимы. Для unsynchronized path скрытие предшествует этим writes, final
   position устанавливается при скрытом cursor, а Show следует после неё лишь
   для active input owner. Учитывать внутренний Show-before-MoveTo закреплённого
   backend, не добавлять внешний hide поверх него и считать задачу закрытой.
   Synchronized output допустим после проверки поддержки/profile, но обязателен
   корректный fallback без него. Если выбранная стратегия сбрасывает blink timer,
   менять стратегию output/cursor lifecycle и повторять qualification обоих
   результатов, не ослаблять blink acceptance. Не форкать dependency/вводить новый
   UI framework, per-widget clock, постоянный repaint timer или второй input owner.
5. **Restoration и неизменные boundaries.** Normal/error/panic exit восстанавливают
   cursor visibility/terminal state вместе с existing raw/mouse/alternate-screen
   cleanup; ошибка draw остаётся non-success. Terminal control injection protections,
   bounded history/output, permissions, execution identity и no tool replay сохраняются.
   T56 PTY pane/его VT cursor — отдельный owner; этот срез не заменяет его contracts.

### Ordered slice после explicit resume и достаточная квалификация

1. Сверить новые HEAD/diff и captured terminal profile; nearest regression должен
   сначала показать reported failure на старом пути. Использовать actual большую
   multiline Shell/eligible expandable card через bounded fake provider и existing
   native tool; input содержит непустой Unicode draft с caret внутри. Capture
   entry → быстрые moves/leave/re-entry → expand → scroll/resize → close/recollapse,
   без paid generation и без повторного исполнения tool от просмотра.
2. В existing terminal/frame seam (`oc-tui/src/terminal.rs`, `oc/src/tui_cmd.rs`)
   минимально исправить output ordering/presentation; `shell.rs`/`dialog.rs` меняются
   только для доказанного ownership conflict. Не переписывать hover renderer ради
   hardware cursor. Сохранить Ratatui bookkeeping, full-frame diff, error model и
   demand-driven scheduling. Проверка blink обязана сопровождать выбор стратегии,
   а не быть необязательной завершающей smoke-проверкой.
3. **Ordering regression:** nearest tests в `terminal/tests.rs` и existing
   `tui_cmd/tests`/TUI focus tests фиксируют реальные backend commands, fragmentation
   и output-error cleanup. Не добавлять production API ради теста. Для fallback
   после каждого complete VT command cursor hidden до final placement, а Show не
   предшествует ему; synchronized path проверяется по presentation semantics.
   Final grid equality и TestBackend cursor position одни этот риск не доказывают.
4. **Actual binary + frontend temporal evidence:** extend existing `pty_t39`
   scenario/support либо existing capture producer bounded cursor-state trace через
   mature VT parser/frontend; text-only parser не расширять до собственного emulator.
   Raw PTY/command trace доказывает ordering, но не реальную raster/blink timing.
   Одним и тем же frontend/profile снять хотя бы **три полных blink cycles** в
   stable composer idle, затем непрерывном hover repaint при том же caret, затем
   restored composer. Записать cycle visibility/timestamps и cadence относительно
   idle baseline; infinite hover/выбор одного удачного кадра не нужны. При read-only
   viewer курсор скрыт, Search имеет свой caret; exit/error/panic cleanup проверяется
   существующими PTY restoration cases, расширенными только на cursor state.
5. **Paired qualification:** running pinned-original/native full styled-cell/PNG/
   cursor sequences при одной fixture/profile на representative existing
   80x24/120x40/160x48, Unicode/long output, blink-enabled и effective nonblinking
   controls. Проверить supported synchronized и non-support fallback paths без
   полного state×width×terminal product. Unmasked transient frames, matched blink
   phases, real expand/scroll effects и input-owner restoration обязательны;
   no final-only/native-golden/mouse-disabled/crop/mask waiver. Недоступный runnable
   reference — BLOCKED_REFERENCE, не PASS. VIS31 reuse подтверждает queue/latency/
   settled idle; host cursor blink не должен требовать periodic product repaint.

VIS16/VIS31 остаются **mandatory, NOT_RUN, evidence empty** до новой квалификации.
Done этого bug slice = **нет фантомного caret И сохранён корректный prompt blink**,
с intact focus/hover/expand/restoration. Plan-only commit/push не переключает active
T50, не снимает PAUSED T44 и не переписывает historical evidence/PASS/baselines.

## Обязательные результаты нового прохода

V00–V09 из IMPLEMENTATION_GUIDE.md и все mandatory сценарии из актуального ACCEPTANCE.json:
1. Изолированный upstream reference + identical fixture/state для трёх пользовательских экранов.
2. Исправленные UI event loop/keymap и диагностируемый MCP error без потери draft.
3. Shell/sidebar/tabs/prompt/footer из реальных данных с геометрией эталона.
4. Dialog/Select/command registry без фиктивных работающих actions.
5. Markdown table/code/reasoning/tools/diff и durable replay того же вида.
6. Unicode/multiline editor и terminal restoration.
7. Config-source boundary, negative failures, memory и stale-generation checks.
8. Парные styled frames + PNG + behavioral assertions + актуальная qualification.

Не уменьшать таблицу, шрифт, screenshot scale или весь viewport ради green; не скрывать
MCP ошибку; не хардкодить Model/Free/Context; не подделывать provider results. Не править
reference goldens автоматически по результату Rust. Не удалять старую safety assertion
ради нового внешнего вида. Изменение устаревшей UI assertion допустимо только с paired
reference/provenance и сохранением первоначального behavioral invariant.

## Definition of done

`TUI_PARITY_VERIFIED`: все mandatory сценарии исполнены, одинаковый environment/fixtures,
необъяснённых cell/PNG diffs нет, ошибки и restart проверены. Это ещё не общий READY.

`TUI_IMPLEMENTED_UNVERIFIED`: код есть, но upstream capture/paired comparison отсутствует.
`BLOCKED_REFERENCE`: нет исполнимого reference/profile для объективной проверки.
`BLOCKED_PRODUCT`: реальная ошибка protocol/storage/MCP/UI мешает сценарию.

Эти labels — отчётные статусы, не требование добавить runtime enum.
Окончательный READY дополнительно зависит от обязательных live gates, T43/T45 scope,
полного FINAL и актуальной security/resource qualification. Если новая работа идёт после
исторического T42 — записать superseding qualification, не выдавать старый отчёт за новую проверку.

## Видимые upstream-функции вне native scope

Полный визуальный контракт не даёт права тайком добавить OAuth, JS host или remote
sharing service. Для любой отсутствующей backend-функции из palette составить явный
capability mapping и потребовать owner decision о backend scope. Не рисовать работающую
кнопку без обработчика. В рабочем UI допускается только честное unavailable-состояние
с объяснением; такой профиль не объявлять полным upstream behavioral parity.
Для трёх paired visual fixtures доступности у original/port должны совпадать. Если этого
нельзя добиться без неподтверждённого scope — соответствующий full-parity gate blocked,
а не «совпало после удаления неудобных строк».
