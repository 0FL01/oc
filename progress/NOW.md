# NOW — актуальный handoff

State updated: 2026-09-24T10:02:04+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Current pinned original/native 160×48 Reader Commands/Models captures verified a one-cell Markdown table divider mismatch and a source-space style mismatch. Production fixed both: table spare width is distributed consistently across full/indexed rendering; the actual Markdown separator before inline code remains colored on wrap. Final paired completed frame has only real elapsed/token-rate cell differences, 17/7680; Commands 290 and Models 102 cells still DIFFERENT because actual available actions/rows differ. Test-only per-frame DOM probe proved 80×24 shrink PNG's 32-pixel border difference comes from an upstream xterm span painting beyond the resized 674px screen, despite identical visible cells; identical force-refresh did not remove it. Six immutable attempts and report: `evidence/tui/recovery-v04/dialogs-current-20260924-report.md`. Code/tooling commit `1ef0123`.

## Checks

Serialized full locked workspace suite passed (220 TUI tests, no failures; existing opt-in live ignores), workspace fmt, all-target Clippy -D warnings, locked build, Node syntax, geometry/frontend checks, docs/progress and diff checks PASS. Both paired PTYs satisfy provider contract; captures have pinned executable/source/fixture/profile, styled grids, PNGs, VT, input/protocol and comparator results. Dialog and resize full-frame comparator exit 1 DIFFERENT. Independent code review found no actionable regression.

## Risks

T44 remains active. VIS01–24, V08–V09/S07 not closed; false version/timer/context and inert unavailable actions were not introduced. The off-grid PNG difference is not fixed or masked. Full Commands/Models content/action parity and effective model selection remain open. Pre-existing `.opencode/` untracked and untouched.

## Next

Confirm pinned real Rename session dialog and owner-backed title semantics; implement only with actual mutation and paired interaction. Continue mandatory dialogs/error/replay and VIS qualification; do not treat a matching partial frame as full T44 PASS.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
