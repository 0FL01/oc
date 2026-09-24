# NOW — актуальный handoff

State updated: 2026-09-24T12:09:39+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

## Result

Pinned v2.0.12 Rename session (Ctrl+R and Commands) now works through a dedicated, focused 60-cell dialog and application-owned durable root-title update, rather than an inert command entry. `/rename <title>` submits directly after validation and owner acknowledgement. The owner checks Location/root, persists title and event in one SQLite transaction, and keeps explicit titles ahead of late generated titles. A refused write leaves modal edit or slash draft intact; Home, read-only child, foreign Location and busy routes are refused. The isolated modal preserves the composer and grapheme-safe editing; generated titles longer than the 256-byte manual limit cannot be silently truncated on unchanged Enter. Owner validation rejects invisible/bidi/control titles and accepts valid visible joined emoji. Code/runner commit `5a37756`. Four independent paired attempts, fixture/executable/profile hashes, measured outcomes and limitations: `evidence/tui/recovery-v08-rename-report.md`.

## Checks

Both pinned original and Rust real PTYs pass prefilled/edited Ctrl+R, explicit rename, natural quit and restored title with no added provider requests. Final prefilled and edited dialog whole-frame grids, cursor and PNG compare EQUAL (0/4800 styled cells, 0/647040 pixels). Other frames still DIFFERENT in live elapsed digits and real version: never claim VIS PASS. Full serialized locked workspace tests passed (235 TUI, 29 pty_t42, 35 bin, existing opt-in live ignores), fmt, all-target Clippy -D warnings, locked build, Node syntax, docs/progress/diff checks PASS. First full serial run concurrently with Clippy/build had one intermittent signal-exit 0 vs130; isolated, full PTY and final serial workspace reruns all PASS without changes to test or expected exit.

## Risks

T44 remains active and full VIS01–24, V08–V09/S07 pixel acceptance is open. Pinned `/rename` with no argument requests title regeneration; native explicitly marks that path unavailable pending a real application/provider operation, not a fabricated title. Other Commands/Models actions, VIS05 80×24 PNG edge, error/replay and model effects remain unqualified. Genuine elapsed time, native 0.1.0 versus pinned original 2.0.12 and Home random examples were not forged. Existing `.opencode/` was not accessed.

## Next

RECON a safe owner-mediated title regeneration for bare `/rename` with real model/provider request, cancellation and durable title precedence, or choose the next source-backed Commands/Models capability with actual effect. Continue paired, dimension, error/replay and resource gates without disguising DIFFERENT as PASS.


Ready (до 5): T45, T46, T47
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
