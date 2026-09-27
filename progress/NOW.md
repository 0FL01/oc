# NOW — актуальный handoff

State updated: 2026-09-27T11:29:12+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12, включая полный Approve permission lifecycle/UI (VIS36), question UI (VIS37) и DCP UI (VIS38)
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R3/VIS06 отображать имя профиля в prompt metadata через существующий Locale.titlecase (build → Build, build-yolo → Build-Yolo), рассчитывая ширину по display label; не менять ID, выбор, цвет профиля и model/provider/variant. В R4/V06/VIS16 исправить успешную Shell-карточку: убрать синтетический Command exited with code 0, обеспечить одну пустую строку с границей между командой и непустым выводом как в OC2; при пустом выводе не добавлять фиктивный разделитель. Сохранить фактический вывод, exit metadata и диагностику ошибок. Обновить существующий golden и проверить парные кадры, replay/reopen. В R4/VIS17 исправить потерю agent/color metadata user message после завершения turn и attach_page: сохранять корректную связь user и assistant с turn у владельца history projection. Без явной смены профиля полоска не меняет цвет при completion/reopen; двумя последовательными provider requests подтвердить сохранность выбранного agent ID/digest и его инструкций, не заявляя PASS KV cache по цвету UI. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R4/V06 и R5/V04 обеспечить visual + feature parity наведения и клика по user message (Message Actions с реальными Jump to/Revert/Copy/Fork), а также hover и повторного раскрытия/сворачивания поддерживаемых upstream групп инструментов и длинного вывода Shell; не обещать раскрытие содержимого обычного Read или восстановление обрезанного результата. В R5/VIS33 проверить user click → Message Actions → Revert → карточка N messages reverted: клик, ctrl+x r, /redo и palette Redo снимают всю staged-границу как OC2, заменяя прежний one-turn Redo. Счётчик user messages из owner, hover/hint, selection guard и reopen проверять по pinned U11–U14; без provider/tool replay и изменений workspace/Git. В R5/V04 убрать лишний agent: … toast после успешной смены профиля: обновить фактический выбор и metadata, закрыть диалог, вернуть draft/focus как в pinned OC2; сохранить ошибки и несвязанные предупреждения, не менять глобальный lifetime уведомлений. В R5/V04 исправить Sessions: удалить встроенный slash-alias /session из dispatch/autocomplete, сохранить /sessions, /resume, /continue и внутренний session.list; показывать реальные названия root-сессий, группы по дате обновления и контекст проекта вместо ID. Проверить поиск, выбор, rename/delete/scope switch и reopen по pinned OC2 без фиктивных данных/shortcuts и без изменения ID-only list_sessions consumers. В R5/V05 реализовать Ctrl+T variant.cycle для вариантов активной модели, включая default и только объявленные моделью варианты; не переключать модель и не отправлять prompt при смене. Проверить видимый/сохранённый выбор и значение в следующем реальном provider request, без hard-coded reasoning levels. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

VIS35 owner/core/TUI slice implements bounded committed effect metadata, actual
minimal diff/context/old-new gutters, configured unified/split/auto/wrap and move
destination-only header. Real argument streaming is transient, bounded and linked
to durable intent without inventing effects. Replay uses persisted results, no file
reads or mutation re-execution. Partial known effects remain truthful and modes,
path policy, symlink/CAS safety and conversation-only Undo/Redo remain intact.

## Checks

Final serial workspace fmt/locked tests/strict all-target Clippy/debug+release build,
capture syntax/frontend/docs/progress/diff PASS: tool_0e1997ece001NKZRyIv1yVq87s.
Source-built paired19–22 actual lifecycle/file bytes/modes/pending/replay PASS.
132 cursors equal; six full grids and six PNGs equal, remaining comparisons
DIFFERENT/BLOCKED. Reports and all historical01–22 artifacts preserved.

## Risks

No VIS35/V09 exact PASS. Denial hover lacks a counterpart; PTY partial/cancel/unknown,
new A09 and approval gates not qualified. Donor move mode0664 differs from native
preserved0751. Elapsed digits retained. Inherited.opencode never inspected/staged.

## Next

Deliver this checked slice; implement the owner-approved real permission request/
reply lifecycle and its actual TUI (VIS36), then leader pending presentation and
remaining exact matrix. Do not invent approval or waive security for parity.


Ready (до 5): T45, T46, T47, T50
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
