# NOW — актуальный handoff

State updated: 2026-09-26T08:01:14+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R3/VIS06 отображать имя профиля в prompt metadata через существующий Locale.titlecase (build → Build, build-yolo → Build-Yolo), рассчитывая ширину по display label; не менять ID, выбор, цвет профиля и model/provider/variant. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R4/V06 и R5/V04 обеспечить visual + feature parity наведения и клика по user message (Message Actions с реальными Jump to/Revert/Copy/Fork), а также hover и повторного раскрытия/сворачивания поддерживаемых upstream групп инструментов и длинного вывода Shell; не обещать раскрытие содержимого обычного Read или восстановление обрезанного результата. В R5/V04 исправить Sessions: удалить встроенный slash-alias /session из dispatch/autocomplete, сохранить /sessions, /resume, /continue и внутренний session.list; показывать реальные названия root-сессий, группы по дате обновления и контекст проекта вместо ID. Проверить поиск, выбор, rename/delete/scope switch и reopen по pinned OC2 без фиктивных данных/shortcuts и без изменения ID-only list_sessions consumers. В R5/V05 реализовать Ctrl+T variant.cycle для вариантов активной модели, включая default и только объявленные моделью варианты; не переключать модель и не отправлять prompt при смене. Проверить видимый/сохранённый выбор и значение в следующем реальном provider request, без hard-coded reasoning levels. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Owner clarified and approved conversation/context-only /undo, /redo and Message Actions Revert, then explicitly requested plan edits, commit and push. Updated GOAL, T44 goal/amendment/work guide and VIS10 acceptance; no product code implemented or resumed. Authoritative plan: tui-recovery/T44_CONTRACT_AMENDMENT.md, 2026-09-26 conversation-only section. Undo moves back one user/agent turn; redo forward one saved turn without provider/tool replay; new accepted input cuts normal redo of the old tail. Restore causal LLM/DCP projection, retain raw history and persist boundary across restart. Workspace/files/modes/Git remain unchanged. Snapshots default off; false aliases accepted; true unsupported. Prior file-preimage plan is superseded, not a pending requirement.

## Checks

Documentation/acceptance validation and staged diff review are required before this documentation-only commit. Prior 577ce84 full code gate remains historical evidence only; current dirty Rust/snapshot correction is unqualified and no Cargo gate is claimed. Delivery status is determined by Git, not this pre-commit note.

## Risks

Independent owner docs commits through 2429a85 are preserved. Parked identity/fork/migration/snapshot/shell changes and untracked Rust modules remain unstaged; no reset or automatic cleanup. Inherited .opencode/ not inspected/staged. No implementation PASS, no TUI_PARITY_VERIFIED. Approved difference supersedes filesystem Revert and original clear-all redo only; unrelated parity/quality gates unchanged.

## Next

Deliver only reviewed planning/docs/journal changes. Product implementation stays parked until separately resumed. On resumption remove snapshot-specific hunks, not identity/fork or independent shell fixes; implement owner conversation boundary/context versions and commands under the approved plan. Verify actual provider context/DCP boundary, stepwise redo without requests/tools, restart/branch behavior and workspace/Git immutability. No filesystem checkpoints, preimage journal or file rollback.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
