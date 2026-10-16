# RECON — что в порте действительно не так

## Снимок и ограничения

Порт: `d232baa481ed6e4fc844359855d5f419f88b8a8e`.
Текущий UI reference: upstream `v2.0.12` → **commit**
`2670273ff17da96f85c5826ced57aa1b368754fa`, не «tree SHA».
Все локаторы в этом документе относятся к этим снимкам; каталог ссылок — `SOURCES.json`.

Проверены текущий T44, selected source paths, progress и четыре PNG пользователя.
Сборки/PTY/live здесь не запускались. Упоминания 413 passed и flaky PTY — утверждения
`progress/NOW.md`, а не независимый rerun аудитора. GitHub Actions API при проверке
ветки вернул `total_count: 0`; это отсутствие GitHub CI evidence, не доказательство
того, что локальные тесты не запускались или что другого CI нет.

Скриншоты 1–3 обозначены пользователем как оригинал, 4 — как порт. Версию бинарника,
шрифт, терминал и grid нельзя достоверно установить из PNG. Первые три имеют размеры
1919×1011/1012, четвёртый — 1918×981. Их нельзя масштабировать до совпадения и выдавать
полученный процент за pixel parity. На четвёртом отличается также состояние:
сообщение осталось в editor после ошибки `mcp attach failed for crw`, ответ не получен.
Отсутствие transcript в этом кадре само по себе НЕ доказывает отсутствие renderer.

## Вердикт

В коде уже есть настоящий Ratatui TUI, native application, тема, message/tool renderers,
DCP и MCP integration. Bare `oc` теперь действительно вызывает TUI на TTY — старый
finding про usage error исправлен [P16]. Переписывать четыре crates заново не нужно.

Однако **пиксельная эквивалентность не доказана, и значимые несовпадения подтверждены
исходниками**. T44 сам переопределил pixel-perfect как правильность собственных
source-derived snapshots. Такая проверка фиксирует выбранную реализацию, но не её
равенство upstream. Неизвестны намерения исполнителя; доказано преждевременное
закрытие части acceptance, а не сознательный обман.

R3 `verified` не согласуется с отсутствием sidebar и диалоговой геометрии. R4 `verified`
не согласуется с text-only tables и пропаданием tool cards после reload. R5, напротив,
честно остаётся pending: агент не заявляет, что command palette уже закончена [P01].

## Главные визуальные расхождения

### UI01. Самопроверка вместо эталона

`shell.rs::golden_screen_80x24/120x40` собирают `expected` вручную в том же модуле.
`views.rs::render_test` возвращает только `cell.symbol()`, а `screen` делает trim_end.
Проверка палитры отдельно полезна, но не подтверждает правильный цвет конкретной клетки,
style bold/underline/cursor либо backdrop. Это unit goldens, не paired pixel proof [P03/P05].

### UI02. 20 строк вместо высоты viewport; нет sidebar

`render_transcript` получает `area.height`, но выбирает только `VIEWPORT_LINES`.
Высокое окно оставляет лишнюю пустоту. Short content ещё и смещается вниз через `pad`.
Upstream `stickyScroll` описывает удержание scroll position при росте content, а не
разрешение всегда выравнивать короткий transcript по нижнему краю; short-state нужно
зафиксировать исполняемым reference. На предоставленном populated эталоне контент
начинается сверху [P05/U04].

`layout::sidebar_auto(121)` в тесте возвращает true, однако renderer не выделяет sidebar
вообще. Upstream default auto учитывает ширину >120, vertical tabs и child session;
панель занимает своё место и меняет ширину transcript [P04/U03].

### UI03/UI04. Панель меню и клавиши не соответствуют показанным

`panel_lines` выдаёт строки `model | …`, `agents | …`; shell вставляет inline block
с рамкой `panel`. На эталоне — overlay, поиск, оранжевая строка selection, right-aligned
hotkeys и затемнение underlying screen [P03/P05/U01/U05].

`ctrl+p commands` уже нарисован, но в `events.rs` нет соответствующего action.
Особенно важно исправить guard:

```rust
// Текущий код: contains требует ОДНОВРЕМЕННО оба флага.
!m.contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
// Для запрета каждого из этих модификаторов нужен intersects,
// после маршрутизации известных shortcuts и правил AltGr.
!m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
```

При CONTROL без ALT первый guard true: Ctrl+P может превратиться в символ `p`.
Enter независимо от modifiers означает submit; Shift+Enter не даёт отдельный newline.
Нет фильтра `KeyEventKind::Release`. Это функциональные, а не цветовые дефекты [P07].

### UI05. Основной блок эталонного ответа — таблица — не реализован

`messages.rs::markdown` прямо документирует `tables render as paragraph rows`.
Нужны real table layout, borders, header и inline spans. Исправление оттенка рамки
не решит отсутствие самого блока. Ручной subset parser также не доказывает вложенность,
escaping, multi-backtick inline/fences и full syntax parity [P06].

### UI06/UI07. Live/replay и данные chrome различаются

`HistoryMessage` содержит только seq/role/text. UI теряет tool cards, reasoning и footer
metadata при reload/page load; это признано в T44. Live-only polish не соответствует
повседневному session browser [P01/P06/P10].

Tab постоянно печатает `1 Untitled session`; `ModelEntry` не передаёт display name;
agent color/path/context footer не доходят до renderer. Devtools рисуются безусловно,
хотя в upstream это config/channel gate. Нельзя оправдать пустые slots тем, что «DTO
не хватает»: presentation contract нужно минимально расширить [P05/P10/U05/U06].

## Поведение и безопасность

### UI08. Slow MCP способен остановить обработку клавиш

Actual event loop await-ит `handle_event`; тот await-ит `handle_key`/`handle_enter`;
последний await-ит `app.submit`. MCP initialize находится до принятия input runtime.
На этом пути UI не сможет обработать следующий Esc/resize до завершения submit [P08/P09/P11].
Нужен raw-key PTY test с зависшим initialize и локальным watchdog; здесь он не выполнен.

### UI09. `crw` ещё не диагностирован

Видно имя server, но нет failure stage/cause. Возможные классы: config, spawn, DNS/TLS,
auth, protocol negotiation, tools/list, catalog limits. Конкретная причина неизвестна.
Shared remote client требует Bearer и exact 2025-11-25 — это подтверждённое ограничение,
но не доказанная причина ошибки `crw` [P12]. Нельзя «исправлять» скриншот скрытым
отключением MCP или ослаблением egress/permissions. Draft после отказа должен сохраняться.

### UI10. Проверка source symlink неполна

Для discovered `project/.opencode` код сначала получает canonical_root из root,
а затем проверяет config относительно этого root. Если `.opencode` сам является
symlink наружу, новая проверка source не проверяет принадлежность project. Кроме того,
файл читается до проверки canonical containment [P13]. Нужен отдельный negative fixture,
а не повторный тест только `project/opencode.json -> outside`.

### UI11. Новые изменения требуют новой qualification

Исторический T42 done не квалифицирует T44 и добавленные core events. Flaky suite нельзя
объявить green только на основании последующего удачного запуска отдельного test.
Отчёт должен сохранять все попытки и ограничивать claim конкретным code SHA [P02/P15].

## Scope, который нельзя незаметно откатить

В текущем GOAL уже есть owner amendment на subagent system. Старый совет «subagents
вне scope» больше не является актуальным ограничением: T43 blocked, T45 todo [P14/P15].
Не возвращать старый запрет, не считать `<subagent>` карточку доказательством полного
background/reap workflow. T44 не должен потерять результат T43, а FINAL обязан учитывать
T43/T45 и новую визуальную приёмку. OAuth/Code Mode/произвольный JS host остаются вне
согласованного product scope, пока владелец явно не изменит его.
