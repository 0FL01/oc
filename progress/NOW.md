# NOW — актуальный handoff

State updated: 2026-09-25T19:00:15+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R4/V06 и R5/V04 обеспечить visual + feature parity наведения и клика по user message (Message Actions с реальными Jump to/Revert/Copy/Fork), а также hover и повторного раскрытия/сворачивания поддерживаемых upstream групп инструментов и длинного вывода Shell; не обещать раскрытие содержимого обычного Read или восстановление обрезанного результата. В R5/V04 исправить Sessions: удалить встроенный slash-alias /session из dispatch/autocomplete, сохранить /sessions, /resume, /continue и внутренний session.list; показывать реальные названия root-сессий, группы по дате обновления и контекст проекта вместо ID. Проверить поиск, выбор, rename/delete/scope switch и reopen по pinned OC2 без фиктивных данных/shortcuts и без изменения ID-only list_sessions consumers. В R5/V05 реализовать Ctrl+T variant.cycle для вариантов активной модели, включая default и только объявленные моделью варианты; не переключать модель и не отправлять prompt при смене. Проверить видимый/сохранённый выбор и значение в следующем реальном provider request, без hard-coded reasoning levels. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

T44 remains ACTIVE, not TUI_PARITY_VERIFIED. Implementation slice ae17373 fixes narrow Home visibility/centering and one-row completed assistant footer geometry without falsifying version or time. Original/native 63x24 Home in home-visibility-20260925-06 matches the entire styled grid and PNG; the 44x24 final paired Home and completed-session frames in -10 remain DIFFERENT (random example and real elapsed digits). Prior checkpoint 0055's claim that all remaining T44 work is externally blocked was too broad and is historical, superseded by this active checkpoint.

## Checks

Serial full gate PASS: workspace fmt, locked tests (zero failures), all-target Clippy -D warnings, locked build, docs/progress checks, capture JS/Python syntax and diff check. Full output /home/opencode/.local/share/opencode/tool-output/tool_0d9ed5ef0001ICIYpexUnLdJBu. Full-grid/PNG comparator and actual provider receipts recorded in evidence/tui/home-visibility-report.md and immutable paired attempts -01 through -10. These local checks do not satisfy final V09.

## Risks

Original wide Home reports truthful 2.0.12 while Rust reports truthful 0.1.0; independent elapsed time, randomized Home examples and T45-owned non-file @ inventory prevent blanket pixel PASS. Integrations/Settings, next-prompt durable model-switch event and OS clipboard destination still require qualification. Inherited untracked .opencode/ is not inspected or staged. No contract, screenshot or identity was modified to make a comparison pass.

## Next

Continue an independent source-backed T44 outcome: investigate identical durable replay of actual timestamp metadata without synthesizing timing, or implement a missing real action without fake menu entries; verify against the pinned original with paired full-frame grids and PNG. Then complete all mandatory VIS01-VIS28/V09 on final SHA. Any irreconcilable exact-wide-Home identity and independent-clock qualification needs an owner contract decision, not agent masking. Keep T44 active while independent work exists.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
