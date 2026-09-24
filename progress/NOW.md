# NOW — актуальный handoff

State updated: 2026-09-24T06:05:13+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Commit `b18442c` adds a test-only paired original/native two-root restart probe and corrects native bare startup to sessionless Home with ordered returnable real tabs, matching pinned v2.0.12 when capacity permits. Explicit `--session` remains authoritative, a full 16-root deck never loses a tab, and a partially unreadable 16-root deck restores Home with 15 safe tabs without overwriting hidden saved IDs. Home version-row padding now retains canvas foreground around the actual unsuppressed `0.1.0` glyphs. Details, complete attempts and final capture lock: `evidence/tui/recovery-v08-paired-restart-report.md` and `recovery-v08-paired-restart-{01,02,03,qualified,final,qualified-02,qualified-03}/`. T44 remains active; no whole-frame or VIS PASS.

## Checks

Final `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace --no-fail-fast --quiet` PASS zero failures (205 TUI; existing opt-in live ignores); workspace fmt, all-target Clippy `-D warnings`, locked build, Node syntax, docs/progress structure and diff check PASS. Final actual v2.0.12/native 120x40 Reader/read pair `qualified-03` reports both sides `TAB_RESTART_CHECKS_PASS`, natural PTY quit, two durable tabs, original Home selected on bare restart, both histories accessible by clicks, exactly 4 transcript + 2 title request/completions each side before exit and none after. Restored entry tab header x0–69 and Home version-row padding x0–111 each have zero styled-cell differences. Whole Home frame still DIFFERENT: 27/4800 styled cells and 1247/647040 PNG pixels; first and second restored Sessions 201 and 204/4800 cells. First full workspace attempt hit stale V03 bare-route expectation; the next geometry assertion clicked the wrong root and was corrected; targeted and final full reruns passed. Runner `qualified-02` exposed a real PTY exit/EIO classification race; bounded natural-exit grace was added, `qualified-03` green on interaction. Earlier attempts preserved unmodified.

## Risks

Full VIS01–VIS24 and V08/V09/S07 remain open: only one paired restart geometry/slice was exercised, complete frames still differ. Random Home examples, actual app version, Location text and elapsed duration cannot be faked to make frames equal. A separate truthful-context issue remains: initial tool round omits provider usage, final text round reports it; native refuses incomplete **full-turn** usage while upstream context footer shows the latest reported value. Do not label the missing billed tool usage as zero or force a model/context string. Full 16-root startup conservatively selects a real tab because Home would be a 17th native slot. `.opencode/` pre-existed and was never touched.

## Next

Distinguish latest provider generation's reported context measurement from complete turn usage in runtime/durable replay, validate actual context footer on a fresh paired two-root restart without inventing missing tokens, then expand paired fixtures to VIS05 80x24/120x40/160x48 and edge widths, dialogs, errors and remaining mandatory outcomes on the final code SHA. Keep T44 active until every frozen VIS passes.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
