# NOW — актуальный handoff

State updated: 2026-09-23T21:33:40+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T44 — V08 Home normal-mode examples

## Result

Code `cb5eadb` adds source-backed randomized Home examples as muted, bounded,
empty-input prompt hints. Session and nonempty prompt remain unaffected; shell
mode is not falsely presented. Independent original/native captures in
`evidence/tui/recovery-v08-home-placeholder-{01,02,03,04,05}/` and report
`evidence/tui/recovery-v08-home-placeholder-report.md` document a matched
13-cell prefix and uncorrelated random example selection. Prompt blank-cell
style was corrected after attempt 01. No VIS PASS claimed.

## Checks

Full serial workspace tests PASS (TUI 183; live ignores), fmt, all-target
clippy -D warnings, build, Node syntax, docs/progress and diff checks PASS.
Both real paired PTYs execute the provider contract and explore group click
in every capture; whole-frame comparisons are DIFFERENT (runner exit 1).

## Risks

Uncorrelated original/native random choices prevent asserting exact full
placeholder equality in attempts 02–05. Native fixture has no implicit Build
profile; tab add/unread/attention, original prompt/footer, dynamic durations,
S07 residuals and VIS01–VIS24 remain open. T44 is active, not finished.

## Next

RECON real add-tab action and cross-session navigation against the pinned
original before painting the `+`; continue paired VIS qualification and
honest profile-state alignment, no hardcoded default agent.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
