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
