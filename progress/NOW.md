# NOW — актуальный handoff

State updated: 2026-10-16T09:36:58+00:00
Active: нет

Сверить Git status/diff до выполнения команд.

Последний срез: T67 [done]; сверить незакоммиченный diff.

# T67 — owner live dev launcher oc2-dev

## Result

`oc2-dev` alias in the owner's ~/.zshrc runs ~/ai/oc/target/debug/oc2 with the same
secrets/TMPDIR as oc2, shared config and store, no build step. AGENTS.md records the
soft debug-build rule and the D28 config line. Details: evidence/T67/report.md.

## Checks

zsh syntax and aliases OK; debug build; `oc2-dev --version`, real `oc2-dev models`
exit 0 without diagnostics; bounded PTY start without startup error, exit 0.

## Risks

Shared store: oc2 and oc2-dev cannot run concurrently (DataRootBusy) and dev storage
changes reach the owner's live data.

## Next

T44 stays PAUSED until an explicit owner resume.


Следующий шаг: проверить зависимости и начать первую ready-задачу.

Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
