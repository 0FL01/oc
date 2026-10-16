# NOW — актуальный handoff

State updated: 2026-10-16T09:03:27+00:00
Active: нет

Сверить Git status/diff до выполнения команд.

Последний срез: T66 [done]; сверить незакоммиченный diff.

# T66 — OC2 key allowlist replaces the OpenCode 1 detector

## Result

D28 delivered: `config/legacy.rs` and the OpenCode 1 vocabulary are removed; the
schema-derived OC2 allowlist (`config/admission.rs`) refuses any other key with one
`unknown_config_key` report before provider/MCP effects. Model compatibility
overrides equal to the native default are no longer lost (CFG14 case, TS oracle
82 pass). Upstream models.dev all-zero image rows no longer reject the public
catalog. Repo `opencode.json` is OC2. The owner converted `~/.config/opencode`.
Details: evidence/T66/report.md.

## Checks

Fmt, strict workspace Clippy, check_docs pass. Final runbook workspace gate:
2012 passed, 0 failed, 12 unchanged opt-in ignored. Rebuilt release `oc2` via the
`~/.zshrc` alias: real `oc2 models` exit 0 without diagnostics; bounded PTY start
shows no startup error and exits 0.

## Risks

Allowlist is stricter than TS for unknown future keys (declared). Integration
tests reach the real models.dev (pre-existing); mcp08 asserts the retirement
contract instead of an exact pre-retirement spawn count.

## Next

T44 stays PAUSED until an explicit owner resume; E2E06 strace qualification
remains with its owner.


Следующий шаг: проверить зависимости и начать первую ready-задачу.

Ready (до 5): нет
Blocked: T27, T44, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
