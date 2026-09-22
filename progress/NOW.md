# NOW — актуальный handoff

State updated: 2026-09-22T12:38:49+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T44 recovery V03 checkpoint

## Result

Base `a4bb361`. Full measured-height rendered transcript replaces fixed 20-row
screen slicing; short history top placement, wrapped-row scroll and resize clamp
work. Sidebar uses actual title/usage/location and durable parent/config conditions.
Prompt height is dynamic. Devtools uses optional override plus explicit native
Local/Packaged mapping. Startup/preflight and query failure have safe distinct TUI
routes, not usable empty catalog or raw configuration interpolation.

Independent review found missing error/combined-scroll/default-channel/paired-child
coverage. Corrections include first Down after resize clamp. Parent inspected DTO,
composition, TUI state and startup route diffs and reviewed final evidence report.

## Checks

- Parent `cargo test --locked -p oc --test recovery_v03 -- --nocapture`: exit 0,
  one actual binary integration, six resize/draft/sidebar/debug modes, wrapped Up,
  Home/empty, query error, startup error and non-TTY paths; 50.10 s.
- Parent `cargo fmt --all -- --check`, `git diff --check`: exit 0.
- Implementation final workspace: 443 passed, 4 existing live ignores; clippy,
  locked build and formatting passed. Failed/interrupted attempts retained.
- Paired original/native actual styled/PNG captures 16–20 verify geometry,
  scroll/draft/cursor and child/vertical conditions. Whole frames remain DIFFERENT.

## Risks

No VIS or R3/R4 closure. Native error routes are not an invented upstream attach
service. Native devtools informational only. Original child composer/root tabs
differ. Near-limit paste and chip behavior belong to V05; Markdown/footer to V06.
Context denominator follows effective model, not pinned historical context/cost.
V07 config admission/in-flight MCP cancellation and docs registry gate remain open.

## Next

V04 genuine modal Commands/Select model with capability mapping and raw PTY
application effects, then V05–V09. Preserve user ZIP unstaged. No historical report
rewritten, no T44 finish/READY claim.


Ready (до 5): T30
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
