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

## Обязательные результаты нового прохода

V00–V09 из IMPLEMENTATION_GUIDE.md и сценарии VIS01–VIS36 из ACCEPTANCE.json:
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
