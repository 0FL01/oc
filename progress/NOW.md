# NOW — актуальный handoff

State updated: 2026-09-26T15:49:35+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R3/VIS06 отображать имя профиля в prompt metadata через существующий Locale.titlecase (build → Build, build-yolo → Build-Yolo), рассчитывая ширину по display label; не менять ID, выбор, цвет профиля и model/provider/variant. В R4/V06/VIS16 исправить успешную Shell-карточку: убрать синтетический Command exited with code 0, обеспечить одну пустую строку с границей между командой и непустым выводом как в OC2; при пустом выводе не добавлять фиктивный разделитель. Сохранить фактический вывод, exit metadata и диагностику ошибок. Обновить существующий golden и проверить парные кадры, replay/reopen. В R4/VIS17 исправить потерю agent/color metadata user message после завершения turn и attach_page: сохранять корректную связь user и assistant с turn у владельца history projection. Без явной смены профиля полоска не меняет цвет при completion/reopen; двумя последовательными provider requests подтвердить сохранность выбранного agent ID/digest и его инструкций, не заявляя PASS KV cache по цвету UI. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R4/V06 и R5/V04 обеспечить visual + feature parity наведения и клика по user message (Message Actions с реальными Jump to/Revert/Copy/Fork), а также hover и повторного раскрытия/сворачивания поддерживаемых upstream групп инструментов и длинного вывода Shell; не обещать раскрытие содержимого обычного Read или восстановление обрезанного результата. В R5/VIS33 проверить user click → Message Actions → Revert → карточка N messages reverted: клик, ctrl+x r, /redo и palette Redo снимают всю staged-границу как OC2, заменяя прежний one-turn Redo. Счётчик user messages из owner, hover/hint, selection guard и reopen проверять по pinned U11–U14; без provider/tool replay и изменений workspace/Git. В R5/V04 убрать лишний agent: … toast после успешной смены профиля: обновить фактический выбор и metadata, закрыть диалог, вернуть draft/focus как в pinned OC2; сохранить ошибки и несвязанные предупреждения, не менять глобальный lifetime уведомлений. В R5/V04 исправить Sessions: удалить встроенный slash-alias /session из dispatch/autocomplete, сохранить /sessions, /resume, /continue и внутренний session.list; показывать реальные названия root-сессий, группы по дате обновления и контекст проекта вместо ID. Проверить поиск, выбор, rename/delete/scope switch и reopen по pinned OC2 без фиктивных данных/shortcuts и без изменения ID-only list_sessions consumers. В R5/V05 реализовать Ctrl+T variant.cycle для вариантов активной модели, включая default и только объявленные моделью варианты; не переключать модель и не отправлять prompt при смене. Проверить видимый/сохранённый выбор и значение в следующем реальном provider request, без hard-coded reasoning levels. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Implemented bounded owner metadata for Sessions while preserving ID-only legacy
query. Canonical /sessions with /resume and /continue, no extra /session. Truthful
titles, real update order and Today/date categories, current/running/worktree
metadata, scope/search and selected rename/two-key confirmed delete have real
owner effects. Listed foreign root Open validates owning Location/config/deck
before publication; ordinary foreign history/rename protections remain. Delete
is root-family atomic, preserves independent forks/shared metadata, and drains
title work; stale tab scope cannot overwrite the owner preference.

## Checks

Final serial workspace fmt/locked tests/strict all-target Clippy/locked build,
capture syntax/frontend/docs/progress/diff PASS: tool_0de60cc67001Tsl2vZu7GUCjgE.
Source-built paired sessions-live-11 verifies full current-day workflow, six real
provider requests each and none during actions. All 62 unmasked comparisons remain
DIFFERENT. Contrast regression fixes dark-on-red delete confirmation/plain labels.
See evidence/tui/recovery-v00/sessions-correction-report.md.

## Risks

Multi-day paired qualification remains unavailable in this current-day capture;
legacy unknown update times are honestly unknown. No dates derived from IDs or
relabelled creation time. No VIS30/V09 PASS. Earlier failing captures retained;
inherited .opencode/ untouched. Owner c452180 amendment changes Redo to whole-tail.

## Next

Deliver verified Sessions slice, then implement c452180 original-compatible Undo,
whole-tail Redo and clickable reverted card (VIS33), retaining conversation-only
workspace immutability. High-refresh scheduling/wheel qualification remains open.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
