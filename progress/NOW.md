# NOW — актуальный handoff

State updated: 2026-09-22T07:32:43+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Полностью воспроизвести интерфейс upstream opencode v2.0.12 в crates/oc-tui: тема/палитра, геометрия layout, рендер сообщений (markdown/reasoning/tool cards/diff), keymap и диалоги; golden-снапшоты PTY на фиксированных размерах. Recon-артефакты evidence/tui/*, коммит+push каждого среза.

Последний checkpoint этой задачи (проверить актуальность по Git):

# T44 recovery V00 checkpoint

## Result

Audited HEAD `d232baa481ed6e4fc844359855d5f419f88b8a8e`, initially no tracked diff.
Reviewed owner amendment applied only to active T44 goal; historical reports unchanged.
R3/R4 explicitly implemented-partial/unverified, R2 rendered colors unverified.
No reset, new progress engine, production JS dependency or subagent-scope rollback.

Static findings confirmed at this SHA: `events.rs::map_key` modifier mask and missing
event-kind filter; `TuiState::handle_enter` awaits submit from the UI input loop;
MCP attach precedes acceptance; shared remote client imposes bearer/exact protocol;
DTOs drop model display metadata/history parts. P13 is narrower than the review's
initial claim: composition rejects the outside `.opencode` root eventually, but only
after reading/parsing discovered configuration. Existing test does not prove read ordering.

Runnable pinned original acquired from npm `@opencode/cli-linux-x64@2.0.12`, registry
SHA512 integrity independently matched downloaded archive. Isolated `--version` exit 0.
See README and attempt-05 lock for original/Rust binary hashes and source association.
Paired actual PNG/styled-cell/VT diagnostics exist for three screens. All six comparator
commands exit 1. Rust dialog states fail their predicates; these are diagnostic comparisons,
not equivalent-state acceptance. Parent independently inspected original Session/Commands
and Rust Session PNG: table/sidebar/modal geometry visibly differs as documented.

## Checks

- `python3 scripts/check_docs.py`: exit 1, `Unknown test referenced by T43`.
  Existing tasks T43/T44/T45 use product gate IDs as acceptance test IDs. Not caused by
  the goal amendment; not suppressed. Repair registry links without weakening scope.
- `python3 scripts/progress.py check`: exit 0.
- `git diff --check`: exit 0.
- Actual `cargo build --locked`: exit 0 (capture attempt-05).
- Capture/frontend commands, exits, failed attempts 01–05: README and commands.json.

## Risks

V00 is diagnostic groundwork, not a completed freeze: application wall clocks, absent
reasoning fixture and font fallback remain unresolved. No BLOCKED_REFERENCE claim:
reference does run. VIS01–VIS24 remain unverified; final full qualification NOT_RUN.

## Next

Next slice V01: raw PTY stalled initialize/resize/Esc reproduction, responsive pending
submission with retained draft and duplicate prevention, safe staged MCP diagnostics.
Then V02–V09, with config boundary negative tests and fresh qualification. T43/T45 and
live product gates remain open. Capture archive ZIP is user input and is not staged.


Ready (до 5): T30
Blocked: T27, T43

Done в журнале не означает READY всего продукта; см. GOAL.md.
