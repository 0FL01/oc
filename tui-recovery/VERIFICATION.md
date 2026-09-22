# Как доказывать TUI parity, а не собственную непротиворечивость

## Три разных вида доказательств

**Component regression:** наш renderer повторяет сохранённый expected. Полезно для
защиты от изменений, но expected мог быть ошибочным с самого начала. Текущие goldens
из `shell.rs` относятся сюда; text dump без стилей тем более не проверяет цвета.

**Reference parity:** одна fixture/state/terminal profile отрисована pinned original
и Rust. Ожидание происходит от external reference, а не Rust. Сравниваются geometry,
символы, стили, cursor и PNG. Это необходимое визуальное доказательство.

**Behavioral qualification:** реальные PTY keystrokes вызывают нужные application
операции; fake provider/MCP и SQLite независимо подтверждают model selection, submit,
cancel, tools, restart. Красиво нарисованная кнопка не доказывает работу обработчика.

Нужны все три. Ratatui TestBackend не проверяет terminal setup/event loop/teardown;
это прямо отмечено в официальной testing recipe (D01 в SOURCES.json).

## Capture profile

Заполнить `templates/capture-environment.json` до прогона. Не выводить font/cols/rows
из pixel width арифметикой без проверки. Записать:

- original binary version + SHA256 + source commit; Rust code SHA/tree/binary SHA256;
- terminal/frontend и capture-adapter версии, font/fallbacks, размер/DPI/scale,
  ligatures, padding, opacity, locale, TERM/COLORTERM, Unicode/width policy;
- cell columns/rows, pixel dimensions, theme, light/dark, sidebar, devtools/channel;
- fixture SHA256, scenario/state ID, fake service version, key sequence и clock policy.

Environment ID — hash canonical profile без разных executable hashes, а не случайная
строка, которую можно менять для сокрытия разницы. Fixture hash покрывает весь bundle
transcript/model data/config/scenario. Секреты только фиктивные локальные; настоящие
ключи, URLs с credentials, исходные prompt/private files не попадают в evidence.
Шрифты установить в reference-среде законным способом; **не включать font files в ZIP**.

Оба приложения должны рендериться одним terminal frontend при одинаковых настройках.
Растеризовать ANSI в браузере другим шрифтом и сравнивать с host screenshot некорректно.
PTY stream не равен screenshot: если есть только PTY, нужен mature VT parser/frontend,
причём одинаковый для обоих. Не писать свой terminal emulator ради этой задачи.
Парсер должен сохранять truecolor, attributes, wide-cell continuation и cursor; plain
ANSI stripping не подходит. Терминальные escape output можно сохранить отдельно для audit.

## Как получить одинаковый state

Три supplied original PNG:
1. завершённая беседа с таблицей + правый Context sidebar;
2. та же беседа + command palette;
3. та же беседа + model picker.

В четвёртом PNG turn не принят из-за MCP attach failure. Это самостоятельный error
scenario, не baseline для сравнения успешной беседы. Обязательны оба класса сценариев.

Original нужно воспроизводить через normal executable с isolated config и fake provider,
а не вручную рисовать его экран. Seed session допустим через штатный API/fixtures
upstream с проверенным преобразованием; не писать незнакомую DB schema догадками.
Визуальный fixture может содержать текст модели «доступен execute/write/edit» как в
скриншоте. Это не capability manifest и не основание добавлять запрещённый JS host.
Реальные tool assertions берутся из actual registered tool catalog.

В model fixture display-name, Free label и selected marker фиксированы. В production
они динамические. Не захардкодить имена из screenshots в Rust renderer.
Состояние idle/streaming/complete/failed и sidebar/config должно совпадать у пары.
Prompt draft, caret, scroll offset и active dialog тоже входят в state.

## Минимальный формат styled cell dump

`templates/cells.schema-example.json` показывает формат, но НЕ является capture upstream.
Для реальных dump использовать:

```json
{
  "schema_version": 1,
  "origin": "upstream",
  "scenario": "session-wide-completed",
  "fixture_sha256": "<64 lowercase hex>",
  "environment_id": "<same canonical profile id for both>",
  "producer_commit": "<40 lowercase hex>",
  "columns": 160,
  "rows": 48,
  "cursor": {"visible": true, "x": 5, "y": 43, "shape": "block"},
  "cells": [[{"symbol": " ", "fg": "#eeeeee", "bg": "#0a0a0a", "modifiers": [], "width": 1}]]
}
```

В примере массив укорочен для чтения; реальный dump обязан иметь ровно rows×columns
cells. `origin` у actual = `oc`. `width=2` занимает следующую cell с width=0 и empty
symbol. RGB — resolved colors, не роль `text.base`. `modifiers`: отсортированные
bold/dim/italic/underlined/slow_blink/rapid_blink/reversed/hidden/crossed_out.
Нельзя trim строки, пропускать blank cells или терять background у пробелов.

Сравнивать одинаковые scenario/fixture/environment/geometry. `producer_commit` различается
между original и портом; для каждой стороны должен совпасть с lock. Comparator проверяет
форму SHA, но не достоверность remote commit или происхождение capture: это отдельный
обязанность runner/reviewer. Рукописный dump с `origin:upstream` не становится доказательством.

## Строгие критерии

- Grid: symbols, cell width/continuation, fg, bg, modifiers и cursor совпадают.
- PNG: одинаковая геометрия, тот же rasterizer/settings, нет differing pixels.
- Выполнены behavioral assertions сценария: ни одной пропущенной mandatory операции.
- Full rerun после code changes имеет собственный report, failed attempts видны.

Общий процент похожих пикселей — диагностическая цифра, НЕ pass: чёрный фон может дать
99% даже с пропавшим dialog. Смотреть отдельные регионы transcript/sidebar/modal/editor.
Comparator выдаёт bounding box различий; дополнительно reviewer сверяет парные PNG.

Анимации: предпочтительнее остановленные завершённые состояния для исходных трёх кадров.
Для отдельного running fixture захватывать одинаковую заданную фазу средствами test
clock reference либо поддержанным animation-off mode обеих сторон. Не требовать чудесного
совпадения разных wall clocks. Сначала стабилизировать условие; нельзя превращать diff
в PASS допуском или замазыванием всего status area.
В этом компактном checker нет masks, auto-resize, tolerance или auto-update-goldens.
Добавление любого исключения в будущем требует отдельного documented approved reason.

## Использование comparator

```sh
python3 tui-recovery/scripts/compare_frames.py grid reference.json actual.json --report grid-diff.json
python3 tui-recovery/scripts/compare_frames.py png reference.png actual.png --report pixel-diff.json
```

Exit codes: 0 = equality, 1 = comparable but different, 2 = invalid/incomparable/input error.
PNG mode требует Pillow. Grid mode stdlib-only. Missing styles/metadata, размер mismatch,
несовпадающий profile, один и тот же input-файл и попытка перезаписать input отчётом
возвращают non-success. Утилита не исполняет binary/cargo/network и не служит test runner.

Хранить `reference`, `actual`, `diff`, `capture.lock.json`, `commands.json` и raw outputs
рядом по scenario. Capture runner самостоятельно перечисляет executed/failed/blocked/
skipped и итог не преобразует exit2 в PASS. Не править upstream golden при падении Rust.

## Артефакты результата

На каждый обязательный VIS01–VIS24: case ID, status, code SHA/tree hash, test command,
exit code, timestamp, environment/fixture hash, paths всех evidence; failed attempt list.
Для VIS без визуального результата допустимы structured protocol/DB/PTY assertions,
но они не подменяют тройку Session/Commands/Models.

Изоляция single test может помочь диагностике флейка. Она не стирает failed full run.
Исчерпан budget или отсутствует original binary — honest blocked; не сокращать scope
ради завершённого roadmap. T44 verified не означает, что T43/T45 и весь A01–A13 READY.
