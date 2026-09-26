# NOW — актуальный handoff

State updated: 2026-09-26T11:56:36+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R3/VIS06 отображать имя профиля в prompt metadata через существующий Locale.titlecase (build → Build, build-yolo → Build-Yolo), рассчитывая ширину по display label; не менять ID, выбор, цвет профиля и model/provider/variant. В R4/V06/VIS16 исправить успешную Shell-карточку: убрать синтетический Command exited with code 0, обеспечить одну пустую строку с границей между командой и непустым выводом как в OC2; при пустом выводе не добавлять фиктивный разделитель. Сохранить фактический вывод, exit metadata и диагностику ошибок. Обновить существующий golden и проверить парные кадры, replay/reopen. В R4/VIS17 исправить потерю agent/color metadata user message после завершения turn и attach_page: сохранять корректную связь user и assistant с turn у владельца history projection. Без явной смены профиля полоска не меняет цвет при completion/reopen; двумя последовательными provider requests подтвердить сохранность выбранного agent ID/digest и его инструкций, не заявляя PASS KV cache по цвету UI. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R4/V06 и R5/V04 обеспечить visual + feature parity наведения и клика по user message (Message Actions с реальными Jump to/Revert/Copy/Fork), а также hover и повторного раскрытия/сворачивания поддерживаемых upstream групп инструментов и длинного вывода Shell; не обещать раскрытие содержимого обычного Read или восстановление обрезанного результата. В R5/V04 убрать лишний agent: … toast после успешной смены профиля: обновить фактический выбор и metadata, закрыть диалог, вернуть draft/focus как в pinned OC2; сохранить ошибки и несвязанные предупреждения, не менять глобальный lifetime уведомлений. В R5/V04 исправить Sessions: удалить встроенный slash-alias /session из dispatch/autocomplete, сохранить /sessions, /resume, /continue и внутренний session.list; показывать реальные названия root-сессий, группы по дате обновления и контекст проекта вместо ID. Проверить поиск, выбор, rename/delete/scope switch и reopen по pinned OC2 без фиктивных данных/shortcuts и без изменения ID-only list_sessions consumers. В R5/V05 реализовать Ctrl+T variant.cycle для вариантов активной модели, включая default и только объявленные моделью варианты; не переключать модель и не отправлять prompt при смене. Проверить видимый/сохранённый выбор и значение в следующем реальном provider request, без hard-coded reasoning levels. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Implemented prompt profile titlecase using the existing Locale helper and painted
width; successful profile changes are silent while errors remain visible. Ctrl+T
cycles freshly queried, owner-persisted declared variants and the next request uses
the selected overlay. Shell/Explored hover and repeated supported expansion use
painted targets; normal Read is not advertised as expandable. Successful Shell has
one bordered blank separator only with actual output, no generated exit-zero text
or empty-output placeholder. Recorded output and error diagnostics remain intact.

## Checks

Serial workspace fmt/locked tests/strict all-target Clippy/locked build, capture
syntax/xterm frontend, docs/progress/diff PASS: tool_0dd8e54820010JHYPv5cQJpKAw.
Final source-built paired bounded-shell-09 and bounded-variants-06 pass behavior
with real tools/profile/effort requests; all unmasked comparisons DIFFERENT.
See evidence/tui/recovery-v00/bounded-evidence-report.md for exact commands.

## Risks

Observed original success suffix differs from explicitly required native behavior;
the experiment adding it was removed and historical captures retained. No masking,
no invented missing output. OS clipboard readback and final exact matrix remain
open. Inherited .opencode/ untouched; T44 ACTIVE, not TUI_PARITY_VERIFIED.

## Next

Deliver this verified source slice, then implement the smallest remaining T44
requirement: Sessions dialog or high-refresh event-driven redraw, with real effects
and bounded paired qualification. Do not weaken unrelated exact comparisons.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
