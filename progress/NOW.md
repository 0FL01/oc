# NOW — актуальный handoff

State updated: 2026-09-24T13:57:57+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. В R4/V06 проверить и исправить вертикальный ритм двух последовательных реплик: последний текст ассистента → подпись agent/model → следующее сообщение пользователя, по парным кадрам pinned original/native; не подгонять общий отступ множителем. В R5/V05 исправить Ctrl+C: непустой prompt очищается без выхода/отправки, пустой prompt выходит; modal сохраняет свой контекст. Заменить противоположное ожидание в существующем PTY-тесте и проверить реальные эффекты. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Pinned original `session.sidebar.toggle` shows **Show sidebar** when absent and **Hide sidebar** when visible; native previously registered a static **Toggle sidebar**. Native Commands option now reflects actual painted sidebar visibility including the 120/121 breakpoint, vertical rail width, Home/child state and last observed frame. ID/shortcut and actual action remain unchanged (`93b7840`). New test-only real Ctrl+P `sidebar` search/Return paired runner and immutable attempts: `evidence/tui/sidebar-palette-report.md`, hidden and visible `-current` attempts plus earlier probe attempts.

## Checks

Both pinned v2.0.12 and native real PTYs use valid Reader/tools fake Responses and report `SIDEBAR_PALETTE_ACTION_EFFECT_CONFIRMED` for both initial states: label matches, Return toggles the painted sidebar. The selected action row matches. Whole search frames remain DIFFERENT 167/7680 styled cells: original has additional actual Settings/Sidebar and Settings/Layout rows not represented natively. After effect, one differing elapsed digit remains. Full serialized locked workspace tests PASS 0 failed (TUI 237, opt-in live ignored), fmt, workspace all-target Clippy -D warnings, locked build, Node syntax, docs/progress and diff checks PASS.

## Risks

T44 remains ACTIVE with full VIS01–24/V08–V09/S07 open. Settings results are not fabricated without owner-backed interactive behavior; no pixel parity is claimed for search/Home/session frames. Existing 80x24 screenshot edge and real elapsed/version/random examples are not masked. `.opencode/` untouched.

## Next

RECON actual pinned Settings Sidebar/Layout routes and native safe owner/config persistence, or choose next small source-backed error/replay/Models case with a functioning action. Continue mandatory paired VIS, scroll/resize, negative behavior and resource gates without replacing full-frame DIFFERENT by regional success.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
