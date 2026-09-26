# NOW — актуальный handoff

State updated: 2026-09-26T20:51:22+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12, включая полный Approve permission lifecycle/UI (VIS36)
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R3/VIS06 отображать имя профиля в prompt metadata через существующий Locale.titlecase (build → Build, build-yolo → Build-Yolo), рассчитывая ширину по display label; не менять ID, выбор, цвет профиля и model/provider/variant. В R4/V06/VIS16 исправить успешную Shell-карточку: убрать синтетический Command exited with code 0, обеспечить одну пустую строку с границей между командой и непустым выводом как в OC2; при пустом выводе не добавлять фиктивный разделитель. Сохранить фактический вывод, exit metadata и диагностику ошибок. Обновить существующий golden и проверить парные кадры, replay/reopen. В R4/VIS17 исправить потерю agent/color metadata user message после завершения turn и attach_page: сохранять корректную связь user и assistant с turn у владельца history projection. Без явной смены профиля полоска не меняет цвет при completion/reopen; двумя последовательными provider requests подтвердить сохранность выбранного agent ID/digest и его инструкций, не заявляя PASS KV cache по цвету UI. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R4/V06 и R5/V04 обеспечить visual + feature parity наведения и клика по user message (Message Actions с реальными Jump to/Revert/Copy/Fork), а также hover и повторного раскрытия/сворачивания поддерживаемых upstream групп инструментов и длинного вывода Shell; не обещать раскрытие содержимого обычного Read или восстановление обрезанного результата. В R5/VIS33 проверить user click → Message Actions → Revert → карточка N messages reverted: клик, ctrl+x r, /redo и palette Redo снимают всю staged-границу как OC2, заменяя прежний one-turn Redo. Счётчик user messages из owner, hover/hint, selection guard и reopen проверять по pinned U11–U14; без provider/tool replay и изменений workspace/Git. В R5/V04 убрать лишний agent: … toast после успешной смены профиля: обновить фактический выбор и metadata, закрыть диалог, вернуть draft/focus как в pinned OC2; сохранить ошибки и несвязанные предупреждения, не менять глобальный lifetime уведомлений. В R5/V04 исправить Sessions: удалить встроенный slash-alias /session из dispatch/autocomplete, сохранить /sessions, /resume, /continue и внутренний session.list; показывать реальные названия root-сессий, группы по дате обновления и контекст проекта вместо ID. Проверить поиск, выбор, rename/delete/scope switch и reopen по pinned OC2 без фиктивных данных/shortcuts и без изменения ID-only list_sessions consumers. В R5/V05 реализовать Ctrl+T variant.cycle для вариантов активной модели, включая default и только объявленные моделью варианты; не переключать модель и не отправлять prompt при смене. Проверить видимый/сохранённый выбор и значение в следующем реальном provider request, без hard-coded reasoning levels. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Implemented demand-driven scheduling with bounded input/worker bursts and active
animation deadlines; no idle redraw loop. Default wheel preserves three rows/tick
and bounded temporal presentation; optional MacOS acceleration not claimed. Fixed
real detached-completion jump by semantic durable message/part/row anchoring,
retained older pages and expansion, preserving sticky bottom and explicit reset.

## Checks

Final serial workspace fmt/locked tests/strict all-target Clippy/locked build,
capture syntax/frontend/docs/progress/diff PASS: tool_0df7a262c0017rL2kcrdyCQPZi.
Actual source-built high-refresh-20260926-05 confirms detached/sticky completion,
Shell/list routing, 32/32 glyph paints per input rate and zero lagged worker events.
Four idle windows: zero bytes/CPU ticks/main-thread context switches. Native input
p95 38.307/35.761ms at requested165/250Hz; draw p95 24.441ms. These are PTY timing,
not measured FPS. Failed earlier attempts remain immutable.

## Risks

44 full comparisons DIFFERENT, three active PNGs unstable. Exact external idle
window/scheduler alignment, all-thread wakeups and large-history paired paging
remain unqualified. No VIS31/VIS32/V09 complete claim. .opencode/ untouched.

## Next

Deliver verified scheduling/anchor slice, then session compaction 4bee144 without
removing DCP; preserve newer owner patch/permission/leader-pending requirements.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
