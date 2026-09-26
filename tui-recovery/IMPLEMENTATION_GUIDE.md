# T44 — пошаговый план доведения TUI до проверяемого pixel parity

Снимок аудита: `d232baa481ed6e4fc844359855d5f419f88b8a8e`.
UI reference: `anomalyco/opencode@2670273ff17da96f85c5826ced57aa1b368754fa`
(`v2.0.12`). Ссылки Pxx/Uxx раскрыты в `SOURCES.json`.
Это задания исполнителю, а не описание уже работающих исправлений.

## Правило выполнения

Сохранить `crates/oc-{core,adapters,tui}` и `crates/oc`. Не переписывать backend,
не вводить новый UI framework, event bus, plugin host или второй журнал. Работа идёт
в текущем T44; V00–V09 ниже — внутренние срезы с checkpoint через существующий
`progress.py`. T43/T45 остаются самостоятельными обязательствами по subagents.

Сначала прочитать `progress/NOW.md`, текущие Git status/diff и amendment этого пакета.
Сверить каждый finding с новым HEAD: уже исправленное не откатывать. Обновить активный
T44 goal: `verified` должен означать проверенное внешним эталоном, а не собственным expected.

Owner amendment 2026-09-26: перед Message Actions/Revert применять утверждённый
conversation-only Undo/Redo plan из `T44_CONTRACT_AMENDMENT.md`. Не строить Git/native
файловые snapshots или preimage-журнал: `/undo`/`/redo` меняют историю и версию
LLM/DCP-контекста, workspace/Git остаются неизменными. Припаркованный экспериментальный
snapshot diff разобрать после отдельного возобновления; эта правка плана не выполняет код.

Каждый slice: воспроизводящий пример → наблюдаемый failure → минимальное исправление →
targeted test → парный визуальный артефакт, когда он применим → checkpoint/commit.
Все результаты связывать с code SHA и tree/diff hash. Не менять код после qualification
и продолжать ссылаться на предыдущую зелёную проверку.

## V00. Получить исполнимый эталон и заморозить условия

**Вход:** три original PNG из `references/`, source inventory T44, pinned UI commit.

Пользовательские PNG задают требуемые экраны, но пока не являются парными goldens:
разные размеры, неизвестны terminal profile/font/grid/version и состояние приложения.
Четвёртый PNG показывает отказ до принятия turn, а первые три — завершённый ответ.
Сначала привести original и Rust к ОДИНАКОВОМУ состоянию, а не сравнивать пустую сессию
с готовой таблицей.

1. Использовать установленный original executable нужной версии либо собрать pinned
   upstream в отдельной reference-среде. Node/Bun допустимы в этой тестовой среде;
   зависимости reference не переносятся в production Rust.
2. Зафиксировать executable SHA-256, version, source commit, theme, debug/devtools,
   sidebar mode, размеры в cells, renderer/font/version/size/DPI, locale, цветовой режим.
   Шаблон: `templates/capture-environment.json`. Не вписывать неизвестные значения как факты.
3. Подготовить isolated HOME/XDG/project/data для КАЖДОЙ реализации. Не открывать их
   одновременно на одной DB. Reference не должен читать реальный HOME/ключи.
4. Зафиксировать input, transcript, модельные display metadata, usage и время/анимации.
   Использовать локальный fake provider, который выдаёт сценарий через обычный protocol.
   Формы requests двух implementations могут различаться: fake проверяет нужный контракт
   каждой реализации, но отдаёт одинаковое содержимое. Нельзя включать production ветку
   «нарисовать эталон», подменяющую normal application path.
5. Снять Session, Commands и Models через настоящий terminal/PTY обоих executable.
   Захват `.cast`/raw VT и PNG + decoded styled cells описан в VERIFICATION.md.

Пример безопасной организации (пути подставляются ПОСЛЕ проверки binary):

```sh
# В отдельной reference-среде; не production deployment.
export OPENCODE_REFERENCE_BIN=/absolute/path/to/pinned/opencode
"$OPENCODE_REFERENCE_BIN" --version
sha256sum "$OPENCODE_REFERENCE_BIN"
git rev-parse HEAD
cargo build --locked
sha256sum target/debug/oc
# Затем capture runner запускает оба executable с env_clear и отдельными HOME/XDG.
# TERM/COLORTERM, window size и terminal frontend должны совпадать.
```

**Выход:** capture lock + настоящие original baseline frames с независимым происхождением.
**Stop condition:** reference не запускается → `BLOCKED_REFERENCE`, не «ближайший рисунок
достаточен». Независимые code fixes можно продолжать, но visual verification остаётся открытой.

## V01. Сначала исправить зависающий submit и диагностировать MCP

**Основание:** P08 `handle_enter` ждёт `app.submit`, P09 event loop ждёт `handle_key`;
P11 выполняет MCP attach до acceptance. Backend cancellation без поступления UI-команды
не даёт пользователю работающую отмену.

Сделать submission асинхронным относительно UI loop. Минимальная модель:

```text
Idle(draft)
  → SubmitRequested(request_id, immutable draft)
  → PendingSubmission(request_id) — frame/key/resize продолжают обрабатываться
  → Accepted(request_id, session_id, turn_id) — только теперь очистить соответствующий draft
  → Streaming(turn_id)
  → Completed | Failed | Cancelled
```

Отказ до acceptance возвращает исходный draft. Поздний Accepted/Delta для старой session
или поколения не должен попадать в новый экран. Пока PendingSubmission нельзя послать
дубликат Enter; Esc отменяет именно ожидающую операцию. Принятие input и запись истории
по-прежнему принадлежат application, не presentation.

Для `crw` сначала получить точную безопасную причину: transport, config source, стадия
`config / spawn / DNS / connect / initialize / tools-list`, status/protocol mismatch.
Не выводить URL с credentials, headers, raw body/exception, provider prompts. Typed
`McpAttachFailure { server_id, stage, safe_code, retryable }` либо существующий эквивалент
должен доходить до TUI. Сейчас `{server}` недостаточно (P11/P12).

Не угадывать причину `crw`: конфигурация этого сервера аудитору не предоставлена. Проверить,
не применяются ли codex_web-only требования bearer/exact-version ко всем remote MCP.
Реальный transport читать из конфигурации, а не выводить из имени. `oauth:false` означает
«не выполнять OAuth», а не универсальное «каждый remote обязан иметь bearer».

Обязательный MCP нельзя молча выключить ради красивого кадра. До отдельного owner
решения failure сохраняется, но UI остаётся интерактивным, показывает полезную причину
и позволяет повторить после исправления/явной настройки. Если будет введён optional MCP
mode — отдельный видимый контракт, не скрытый fallback.

**Тест:** fake зависает на initialize до принятия prompt; реальный PTY принимает resize,
редактирование/отмену, возвращает draft; после отмены нет лишнего turn, child или request.
Здесь приложить raw input sequence и временные границы, не только прямой `cancel()` из теста.

## V02. Передать в TUI данные, а не дорисовывать placeholders

P10 передаёт model ID вместо display name; history только role/text. Это объясняет
`Untitled session`, голый router ID и исчезновение cards после replay, но не является
основанием оставить их навсегда.

Расширять существующие application DTO только необходимыми полями:

```text
SessionSummary: id, title, location_label, active/unread/error
ModelView: provider_id, model_id, display_name, provider_name, variants,
           optional price/cost metadata с признаком known/unknown
TurnView: turn_id, agent label/color, model label, known usage/timing
TranscriptPart: stable part_id, turn_id, kind, status, bounded text/preview,
                tool operation/blob references, sequence/revision
```

Это набросок нужных данных, не требование ввести ещё одну иерархию моделей.
Переиспользовать уже существующие session metadata, journal, tool_operations и blobs.
Не сохранять второй дублирующий transcript. Presentation получает безопасные проекции,
никогда provider credentials или opaque encrypted reasoning.

`Free` показывать только при известных нулевых тарифах по согласованным metadata,
не при отсутствии цены. `Context` — реальное измерение/оценка с честным статусом;
не hardcode 6 763/3%. Тестовые значения из screenshots разрешены в fixtures, не в production.
Названия моделей с несколькими `/` не ломают display. Agent Build/цвет/auto marker —
из состояния, не глобальные константы.

**Тест:** изменить display name/agent/title/usage в fixture без изменения кода; видимый
кадр меняется. После restart тот же session transcript сохраняет cards/reasoning/footer
в согласованном объёме, а не только текстовый ответ.

## V03. Вернуть всю геометрию, начиная с широкого экрана

**Нельзя закрыть R3 только на 80×24 и 120×40.** На upstream auto-sidebar включается
выше 120 cells, а пользовательские скриншоты широкие. Именно неохваченная ширина
скрыла пропуск целого региона (P04/U03).

1. Исправить transcript viewport: число видимых строк зависит от реального Rect.height.
   Удалить применение константы 20 как screen viewport, но сохранить bounded backing
   storage. «В памяти ограниченное окно» не означает «показывать только 20 строк».
2. Короткий transcript размещать как в снятом upstream. `stickyStart=bottom` задаёт
   поведение scroll; не выводить из этого автоматически bottom-align короткого ответа.
3. Добавить SessionFrame: main + sidebar/right pane + separator, соблюдая upstream
   config/child-session/width conditions. При auto учитывать vertical tabs width.
4. Sidebar показывает настоящий title, Context, location/footer; пустые/unknown данные
   имеют явное представление. Ни padding, ни выбор шрифта не подгоняются под отсутствие DTO.
5. Debug/devtools bar — условный. U06 использует `debug.devtools ?? channel == local`.
   Для screenshots нужен тот же mode; нельзя резервировать строку безусловно.
6. Tabs используют реальные session titles/selection. Home, empty session, populated
   session, attach error — разные состояния, не один пустой экран на всё.
7. Prompt высота растёт по строкам редактора в пределах видимой области; footer/status
   остаются на правильных местах. Narrow mode не должен терять input или controls.

Пример исправления viewport арифметики — только основа; точное выравнивание берётся из reference:

```rust
let viewport_rows = area.height as usize;
let max_scroll = rendered_rows.len().saturating_sub(viewport_rows);
let scroll = requested_scroll.min(max_scroll);
let end = rendered_rows.len().saturating_sub(scroll);
let start = end.saturating_sub(viewport_rows);
let visible = &rendered_rows[start..end];
// Не строить rendered_rows по всему архиву: только bounded active page/window.
// Позицию начала при коротком содержимом сверить с upstream capture.
```

Снять 80×24, 120×40, 160×48 и grid, установленный для пользовательского wide screenshot.
Добавить edge widths 43/44,119/120/121, shrink→grow, hidden sidebar и child-session mode.
**Тест:** при transcript >35 строк и высокой области видны >20 строк; sidebar есть при
нужном wide mode. Сам `assert!(sidebar_auto(121))` не является проверкой отрисованного sidebar.

## V04. Один реальный Dialog/Select вместо inline CLI-панелей

Реализовать в `oc-tui` небольшой общий `DialogFrame` и `SelectList`, а не уникальный
layout каждой команды. Существующие application actions сохранить.

По U01: dialog medium/large/xlarge = 60/88/116 cells, maxWidth=terminalWidth−2,
обычный top≈terminalHeight/4, горизонтальный центр; вертикальный центр только для
явного `centered`. Backdrop — black alpha150/255; применить к уже нарисованному
содержимому, включая fg/background, не просто залить всё чёрным. Округление проверить
реальным reference; одинаковые формулы flex и integer layout не гарантируют одинаковые cells.

Иллюстрация geometry, не замена upstream baseline:

```rust
fn dialog_rect(area: ratatui::layout::Rect, wanted: u16, height: u16)
    -> ratatui::layout::Rect
{
    let width = wanted.min(area.width.saturating_sub(2));
    let top = area.y.saturating_add(area.height / 4);
    ratatui::layout::Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        top,
        width,
        height.min(area.bottom().saturating_sub(top)),
    )
}
```

Command palette: title `Commands`, Search, Suggested/Session и прочие реальные группы,
left label/right shortcut, выбранная строка в theme selection, Esc label. Команды
строить из ОДНОГО registry: id/title/group/keybind/enabled reason/action. Footer подсказки
генерируются из тех же bindings. Нет control без обработчика.

Model dialog: `Select model`, query/search, current-dot отдельно от focus/cursor,
человекочитаемые model/provider names, допустимые status/cost columns, scroll дальше
первых восьми элементов. Выбор меняет runtime model/variant, а не только цвет строки.
После закрытия restore focus и исходный draft. Agents/Sessions/Skills/MCP/error details
используют тот же scaffold.

Не вводить ложные working `Share session`, `Connect an integration`/OAuth ради скриншота.
Для отсутствующего backend нужен owner scope decision или явная недоступность; этот
профиль тогда не является полным behavioral parity. Сопоставление capabilities фиксировать
до capture, не удалять неудобные строки из эталона задним числом.

**Тест:** raw Ctrl+P открывает modal поверх неизменного transcript без reflow; Search
фильтрует длинный список; Enter выбирает; Esc закрывает; один key не попадает одновременно
в modal и editor. Цвет фона выбранной строки и координаты проверяются не строкой текста,
а styled grid.

## V05. Keymap, focus и настоящий multiline editor

Не копировать только подписи. U02 — источник defaults, плюс component-specific layers.
Действия одинаковы для hotkey/palette/slash entry. Focus priority: активный modal →
completion/permission prompt → editor → session/global. У каждого события один consumer.

Конкретный bug P07: `contains(CONTROL | ALT)` проверяет наличие ОБОИХ флагов, а не любого.
Для исключения modifier text нужен `intersects`, после обработки явных shortcuts:

```rust
if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
    // Обычный printable input; Shift допустим, press/repeat проверены отдельно.
}
```

Это исправление не заменяет обработчики Ctrl+P/leader/Shift+Tab. `KeyEventKind::Release`
не должен повторно вводить текст/отправлять turn. Поведение Repeat зависит от действия:
text/navigation повторяются, submission/chords не вызывают дубликат side effect.

По U02:
- Ctrl+P — command palette; внутри select может быть Previous item: важен focus.
- leader Ctrl+X + L/N/M/B — соответствующие upstream actions, точный список из source.
- Enter — submit; Shift/Ctrl/Alt+Enter и Ctrl+J — newline в поддержанном terminal encoding.
- Left/Right/Home/End/word movement/Backspace/Delete/selection/undo работают с editor,
  не со scroll transcript. History Up/Down включается при нужной позиции, как в reference.
- Esc и Ctrl+C зависят от режима; modal Esc не закрывает приложение. Interrupt policy
  проверять из upstream handler, не угадывать по одной keybind строке.

Использовать небольшой готовый Unicode/editor building block после compile spike, либо
компактную модель text+cursor+selection на mature unicode-segmentation/width. Не писать
свою Unicode database. Графемы `е́`, emoji modifier/ZWJ, CJK занимают не `String::len()` cells.
Paste — одно логическое действие, не миллион Char events; input budget/rejection visible.
Не возвращать искусственный 4KiB лимит для больших owner commands; опираться на актуальный
ограниченный application contract.

**Тест:** двинуть cursor в середину русской строки, вставить/удалить emoji grapheme,
ввести две строки через raw Ctrl+J, открыть palette и закрыть, убедиться что draft не потерян,
затем один Enter → одна durable submission.

## V06. Markdown, reasoning и tools: одинаково live и после restart

P06 — ручной parser subset, который прямо оставляет tables plain text. Это не соответствует
центральному элементу screenshot1. Не расширять `if line.starts_with(...)` до своего Markdown engine.

После небольшого compile spike выбрать native mature Markdown parser, например
`pulldown-cmark` с Tables, и написать renderer AST/events в bounded rows. Pin совместимую
версию; dependency не нужно обновлять до latest вслепую. Syntax highlighting — отдельно:
точная tokenization требует совместимого highlighter/grammar, четыре эвристики не считаются
паритетом со всеми upstream языками.

Обязательные примеры из `fixtures/transcript.md`: таблица 2 columns/11 tool rows,
кириллица и inline code, списки, headings, fences. Добавить escaped pipes, CJK/emoji,
длинную ячейку, wide/narrow wrap, незакрытый fence при streaming. Ширины столбцов и borders
брать из capture, а не скриншота, растянутого под свой layout.

Streaming: завершённые blocks кешируются по part revision+width+theme, invalidation
ограничена новым/грязным block. Не разбирать весь transcript на каждый token и каждый
frame. Смена terminal width инвалидирует geometry, не историю. Не рендерить raw ANSI
из model/tool text; преобразование terminal control content — отдельный тест безопасности.

Reasoning: collapsed/expanded state, correct title/duration, partial/running/failed. Не
показывать encrypted opaque continuation. Tools: реальные started/finished operation IDs,
status/exit/error/truncated; apply_patch имеет readable hunks и partial result, не success
плашку при ошибке. Пагинация result/blob доступна, UI preview не единственная копия.

**Критическое правило:** history-page DTO должен позволять восстановить те же parts,
что live events. Показанный patch не исчезает после переключения session/страницы/restart.
Сохранить semantic view projection из существующей durable history, не сериализовать
весь Ratatui state и не заводить второй transcript.

Subagent card связан с реальным parent/child session и status; T43/T45 ещё имеют
незавершённые обязательства. Не представлять background/notice/reap как готовые только
потому, что появилась карточка с таким названием.

## V07. Интеграционные и негативные проверки до визуального finish

Выполнить SAFETY_REGRESSIONS.md. Особое внимание:
- discovered `project/.opencode` не может само легализовать external root после canonicalize;
- source admission — до содержимого/file references; controls остаются bounded без
  произвольного отказа от нормальных больших user configs;
- late events не переносят сообщения между session/Location generations;
- pending MCP/provider/file operations отменяются из настоящего UI;
- denied/ask/error/unknown показывают реальное состояние, не становятся allow/success;
- live/replay и parent/child не нарушают durable intent-before-effect;
- нет full archive или всех wrapped rows в постоянном UI cache;
- original shell/patch/permission/redaction regressions остаются обязательными.

Memory qualification повторить ПОСЛЕ новых styled rows/markdown caches. Старый T40
измерял предыдущую реализацию. Сравнить одинаковый активный viewport/context при разном
архиве; измерить RSS/PSS, peak allocation, queues, live parts, owned processes и shutdown.
Прямой подсчёт `retained_bytes()` полезен, но не измеряет все transient allocations.

## V08. Закрывать parity только парными кадрами

Для каждого VIS case — input sequence, state predicate, environment lock, original frame,
Rust frame, styled-cell diff, PNG diff, behavioral result. Не использовать sleep как
единственную гарантию готового экрана: ждать marker/status, не зависеть от скорости модели.

Проверка уже захваченных кадров:

```sh
python3 tui-recovery/scripts/compare_frames.py grid \
  evidence/tui/reference/session.cells.json \
  evidence/tui/actual/session.cells.json \
  --report evidence/tui/diffs/session.grid.json

python3 tui-recovery/scripts/compare_frames.py png \
  evidence/tui/reference/session.png \
  evidence/tui/actual/session.png \
  --report evidence/tui/diffs/session.pixels.json
```

Файлы выше исполнитель должен получить capture runner; пакет НЕ содержит выдуманных
upstream terminal dumps. Исходные screenshot-файлы references имеют другое назначение.
Comparator не является эмулятором/runner/CI/источником истины; он лишь сравнивает два файла.
PNG mode требует Pillow; grid mode — Python stdlib. Подробности в VERIFICATION.md.

Exact fixture требует 0 необъяснённых cell/pixel differences при одинаковом renderer.
Нет global-SSIM «почти 99%» как финального pass: огромный чёрный фон скроет отсутствующий
диалог. Animated/time-varying output заморозить в capture environment, не маскировать
половину экрана. В v1 comparator masks отсутствуют намеренно.

## V09. Повторная qualification и честная передача

На одном code SHA: fmt, all-target clippy, workspace tests, build --locked, bare-oc PTY,
paired scenarios, golden coding/MCP/DCP/restart, негативные проверки, clean HOME/XDG и
no-required-Node production. Точные команды согласовать с текущим workspace; основу сохранять:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --no-fail-fast
cargo build --locked
python3 scripts/check_docs.py
python3 scripts/progress.py check
git diff --check
```

Все попытки сохраняются: failed в параллельном suite не исчезает из отчёта потому, что
isolated rerun успешен. Либо воспроизвести и исправить/доказать environmental fault,
либо qualification остаётся incomplete. Изоляция fixtures/PID/timeouts не заменяется
бездоказательным увеличением timeout. Current Actions=0 не доказывает отсутствие local
run, но независимый обычный CI job с артефактами снимет часть этой неопределённости.

Обновить T44 evidence с VIS01–VIS26: pass/fail/blocked/not-run, source SHA, capture inputs,
все outstanding gaps. Новый визуальный milestone не отменяет T43/T45, live T27 и FINAL T30.
После offline qualification — только разрешённая bounded live campaign на настоящем `oc`,
не массовый перебор платных моделей. Полный READY запрещён при открытой обязательной
capability или неисполненном gate. Не переводить task done только потому, что закончился
контекст сессии: оставить checkpoint с точным следующим действием.
