# NOW — актуальный handoff

State updated: 2026-09-28T11:45:05+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12, включая Approve (VIS36), question (VIS37), DCP (VIS38), subagent (VIS39), MCP modal/startup (VIS19/VIS40), tab hover-marquee (VIS41), plugin/service errors (VIS42)
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R3/VIS06 отображать имя профиля в prompt metadata через существующий Locale.titlecase (build → Build, build-yolo → Build-Yolo), рассчитывая ширину по display label; не менять ID, выбор, цвет профиля и model/provider/variant. В R4/V06/VIS16 исправить успешную Shell-карточку: убрать синтетический Command exited with code 0, обеспечить одну пустую строку с границей между командой и непустым выводом как в OC2; при пустом выводе не добавлять фиктивный разделитель. Сохранить фактический вывод, exit metadata и диагностику ошибок. Обновить существующий golden и проверить парные кадры, replay/reopen. В R4/VIS17 исправить потерю agent/color metadata user message после завершения turn и attach_page: сохранять корректную связь user и assistant с turn у владельца history projection. Без явной смены профиля полоска не меняет цвет при completion/reopen; двумя последовательными provider requests подтвердить сохранность выбранного agent ID/digest и его инструкций, не заявляя PASS KV cache по цвету UI. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R4/V06 и R5/V04 обеспечить visual + feature parity наведения и клика по user message (Message Actions с реальными Jump to/Revert/Copy/Fork), а также hover и повторного раскрытия/сворачивания поддерживаемых upstream групп инструментов и длинного вывода Shell; не обещать раскрытие содержимого обычного Read или восстановление обрезанного результата. В R5/VIS33 проверить user click → Message Actions → Revert → карточка N messages reverted: клик, ctrl+x r, /redo и palette Redo снимают всю staged-границу как OC2, заменяя прежний one-turn Redo. Счётчик user messages из owner, hover/hint, selection guard и reopen проверять по pinned U11–U14; без provider/tool replay и изменений workspace/Git. В R5/V04 убрать лишний agent: … toast после успешной смены профиля: обновить фактический выбор и metadata, закрыть диалог, вернуть draft/focus как в pinned OC2; сохранить ошибки и несвязанные предупреждения, не менять глобальный lifetime уведомлений. В R5/V04 исправить Sessions: удалить встроенный slash-alias /session из dispatch/autocomplete, сохранить /sessions, /resume, /continue и внутренний session.list; показывать реальные названия root-сессий, группы по дате обновления и контекст проекта вместо ID. Проверить поиск, выбор, rename/delete/scope switch и reopen по pinned OC2 без фиктивных данных/shortcuts и без изменения ID-only list_sessions consumers. В R5/V05 реализовать Ctrl+T variant.cycle для вариантов активной модели, включая default и только объявленные моделью варианты; не переключать модель и не отправлять prompt при смене. Проверить видимый/сохранённый выбор и значение в следующем реальном provider request, без hard-coded reasoning levels. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза. VIS39 R4/R5/V06: полный delegation transcript/controls/colors/animations по T44 amendment и pinned U35–U49/U02, не OCR. T45/R3/SUB01/SUB02 minimal real lifecycle/typed projections/Ctrl+B first, затем inline Delegating/running/continuation/Background, durable notices/parent footers, lower Subagents composer и family/tab indicators. Full paired styled-cell/PNG/cursor плюс animation-on phase/state sequences и source-specific off fallback; replay/restart/current child status без duplicate delivery/reexecution. Preserve response-close-before-tool-admission, typed outcomes, policy/Undo/Redo/resource invariants; reuse existing gates, no all-T45 dependency/new task/store/framework/sleep300 campaign. VIS40 R3/R5/V04: MCP servers modal after T46/R5/MCP08 minimal typed status/control slice; /mcps/registry, Search/empty/no-match, source statuses/actions/colors/focus/details and full paired cells/PNG/cursor. U50–U55/U02/U49, no component spinner; D13/disabled/quarantine/OAuth boundaries per amendment. Backend/visual results separate; no all-T46 dependency or historical PASS. VIS41 R3/R5/V03: follow approved tab slice in T44 amendment (U56–U59); own-tab spinner remains VIS39, independent of T45 completion.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T44
## Result
T52 done; summary=true3-size exact. dcp-report20260928-05.md.
## Checks
Workspace1191 PASS; parsed-screen2s gate intact.
## Risks
Full VIS38/V09 open.
## Next
Remaining controls/negative/running/lifecycle.


Ready (до 5): T45, T46, T47, T50, T51
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
