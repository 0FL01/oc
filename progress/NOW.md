# NOW — актуальный handoff

State updated: 2026-09-24T08:16:03+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

The current pinned v2.0.12 original/native VIS05 matrix in `evidence/tui/recovery-v03/vis05-fresh-20260924-final/` confirms actual scroll/resize, draft/cursor and 80x24/120x40/160x48/43/44/119/120/121x48 geometry state predicates on both PTYs. TUI now preserves the first displayed transcript row when resizing a scrolled viewport; test harness uses SGR wheel instead of Up/Down history recall, keeps paired states equal before comparison, and verifies draft/cursor in each matrix frame. Pinned metadata fitting and sidebar blank foreground were corrected with test-first regressions. Full comparator still DIFFERENT, so VIS05 and T44 stay ACTIVE. Factual report `evidence/tui/recovery-v03/vis05-current-report.md`; immutable attempts `vis05-fresh-20260924-01` through `-10`, plus `-final` preserve failure/diagnosis and final result.

## Checks

Final full serialized locked workspace tests PASS (212 TUI tests; existing opt-in live ignored), workspace fmt/clippy all targets `-D warnings`, locked build, Node syntax, docs/progress/diff PASS. Final both sides `SCROLL_RESIZE_CHECKS_PASS`, `provider_contract=true`, 0/1920 styled-cell differences on 80x24 scroll-shrink but 32 PNG edge pixels differ. Matrix visible marker counts, draft row and cursor agree; full-frame runner exits 1 honestly.

## Risks

This fixture does not test semantic anchoring if preceding lines reflow under width change; current top anchor is a wrapped-row index. Pinned original/native duration, tok-s, Home random example, true app version and a remaining sidebar blank style differ. Some existing T44 VIS states and S07/V08/V09 qualification remain unmeasured; no mask/threshold relaxation. Old untracked `.opencode/` untouched.

## Next

Test width-sensitive multiline/Markdown content above a scrolled anchor against the pinned reference; resolve any actual mis-anchoring. Continue paired Commands/Models, error/replay and remaining VIS geometries and safety gates until mandatory zero-unexplained-diff acceptance; do not mark T44 done on this slice.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
