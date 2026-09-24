# NOW — актуальный handoff

State updated: 2026-09-24T04:59:06+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Commit `f8e5540` persists up to 16 ordered Location-bound root tabs and selected route through the single application owner without creating a session for Home or on restart. Explicit `--session` retains authority; child inspection is standalone/read-only. Byte-bounded versioned preference, Location-bound CAS and validated root IDs guard against stale writes/foreign or partial projection; a pending-adoption marker atomically journaled with the first turn recovers accepted roots after a crash. Admission fails closed on a malformed or full saved deck; tab close is durably saved before view mutation. Native real-PTY+SQLite restart/negative evidence and immutable paired current-frame captures are in `evidence/tui/recovery-v08-persisted-tabs-report.md` and its three named attempts.

## Checks

Final `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet` PASS 0 failures (204 TUI, existing opt-in live ignores); workspace fmt, all-target Clippy `-D warnings`, locked build, docs/progress and diff checks PASS. Qualified actual original/native v2.0.12 120x40 Reader/read+tab add/close pair passes both provider and click predicates; after-close tab row x0–69,y0 has 0/70 styled-cell differences. Whole after-close frame remains DIFFERENT: 200/4800 styled cells, 4540/647040 PNG pixels. This runner did not compare seeded paired restarts; native PTY+SQLite restart is the durability evidence. Earlier full-workspace failures were stale MCP root-count and V03 Home assertions after semantics changed; targeted and full reruns passed after asserting actual no-root and restored routes.

## Risks

Full VIS01–VIS24 and T44 parity remain open; paired reference/native restart with equivalent persisted history, V08–V09, remaining S07, other terminal sizes/states, dynamic timer/Home random example, native real version and Location text still differ. Unreadable/projected/stale saved preferences do not delete roots or silently rewrite hidden IDs; owner returns a bounded safe error. The old untracked `.opencode/` was not accessed.

## Next

Develop a paired original/native restart fixture with equivalent opened tabs, selected route and state, validate VIS at 80x24/120x40/160x48 and narrower widths, and continue frozen remaining V08–V09/VIS01–24 outcomes without marking T44 done until complete pixel equality and safety gates are verified.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
