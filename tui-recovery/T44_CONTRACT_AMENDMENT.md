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
   round trip, no model/draft change or request on shortcut, busy/read-only guards,
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

## File-mutation parity — VIS35

1. Сохранить единый `apply_patch(patchText)` для всех моделей; write/edit не
   добавлять в model registry и не вводить native model-name routing. Визуальный
   эталон — оригинальный OC2 `patch`, компонент `ApplyPatch`; Write/Edit — отдельные
   представления OC2, не fallback для «глупых» моделей. Это ordinary function tool,
   не provider-hosted Responses apply_patch schema.
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
   реальные tool calls OC2 patch/native apply_patch, проверка объявленных
   schemas, файловых effects и model-visible results. Для OC2 использовать
   штатный hook, допускающий patch на fixture-model; не менять донор.
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

Pinned источники: U18–U20 в `SOURCES.json`. VIS35 — спецификация, не executed PASS.

## Audited Approve permission parity — VIS36

Утверждённый контракт будущей реализации, не claim существующего approval backend.
Все необходимые dependencies поддерживаемых действий обязательны; отсутствие backend
не заменяется Unsupported waiver или инертным UI. Pinned sources: U20–U23.

1. **Contract/mappings.** Зафиксировать native tool → permission action → presentation,
   actual resources и отдельные owner-generated save patterns. apply_patch использует
   donor edit preview. Реализовать donor project identity для grants: Git root/origin,
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
   Интегрировать все supported tools, включая patch move targets, shell cwd/command/save,
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
   cursor/focus. Session shortcuts (U02/U35) include interrupt, Down picker, child
   left/right and Up parent where applicable; never hijack ordinary editor arrows.
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
   stage/safe-code/retryability and allowed next action, not raw exceptions or generic
   Configuration load failed alone. Details/copy/investigate share safe owner payload;
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
