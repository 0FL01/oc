# Test plan — общая методика

Канонические общепродуктовые scenario IDs и expected behavior хранятся в `planning/acceptance.json`; T44 task-local VIS specifications — в `tui-recovery/ACCEPTANCE.json`, без дублирования в общем registry. Authoritative task owner — только `planning/tasks.json`. Executable tests/reports используют соответствующие IDs; повторный regression run не создаёт второго owner.

`A01`–`A13` — отдельный namespace high-level gates из точных headings `GOAL.md`.
Task `tests` может ссылаться на такой общий gate (как T44/T45), но это не detailed
scenario и не второй owner. Unknown IDs, duplicate references, shadowing gate IDs
и multiple owners detailed acceptance по-прежнему отвергаются validator.

## Команды качества

После появления workspace полный offline gate выполняется в T29:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --locked
target/debug/oc --help
```

Во время реализации запускать ближайший targeted test и только применимые crate/workspace checks. Live tests исключены из default `cargo test` и запускаются явным opt-in harness. Missing tool/Cargo manifest = fail/not-ready, не skip-success.

## Эталоны и недетерминизм

Clock/IDs/RNG/remote streams заменяются fake fixtures; normalized timestamps/IDs сохраняют identity, порядок независимых операций не навязывается. Данные с секретами в fixtures запрещены. Производный fixture хранит source path/commit/license и normalization recipe.

Mock-provider tests проверяют протокол/state machine, а не интеллект модели. Live coding acceptance проверяет behavior/files/tests, не точную формулировку ответа. Tool success нельзя выводить из финального текста без tool events и результатов проверки.

## Live model и campaign

При наличии `OC_TEST_MODEL` использовать exact provider/model-id и требовать его presence в effective catalog. Иначе использовать explicit configured default; только test harness может детерминированно выбрать первый подходящий discovery ID. Selection записывается до request и не становится product default. Variant — explicit `OC_TEST_VARIANT` либо provider default; disabled/unknown capability даёт blocker, без угадывания по имени.

T13/T20/T25 готовят offline-tested live harnesses; T16/T27 исполняют их. T27 также допускает минимальные offline-tested исправления своего harness после backend audit; обязательные gates и live envelope не меняются. Один durable campaign хранит opt-in, request/search counters, input/output totals, elapsed time и external watchdog/cancel result. Смена модели после failure не автоматическая. Catalog metadata не доказывает wire/tool/reasoning support.

## E2E fixture

Маленький Rust crate содержит намеренный bug, tests и protected public API. Offline и live workflows читают код, исправляют его через `apply_patch`, выполняют `cargo test` через `bash` и проверяют diff только expected paths. Live campaign повторно открывает ту же session, выполняет следующую команду и не сопоставляет exact natural-language text.

После закрытого tool span вызывается compress; после restart проверяются retained fact, reduced outbound context и следующее локальное изменение. Webfetch и codex_web сохраняют source info. Browser default disabled обязателен; actual browser smoke только explicit opt-in.

## Soak и память

T28 сначала фиксирует короткий baseline и measurement windows, затем до qualification замораживает достаточно длинный synthetic workload для проверки history/output/queue/cache/process behavior. Нагрузка не использует live paid prompts. Проверяются cold/warm baseline, peak, post-idle, PSS/RSS, task/process/queue/blob/cache/DB/WAL counters, crash injection, scroll/paging и cleanup.

Численные thresholds выводятся из baseline до optimisation candidate и не повышаются для сокрытия регрессии. Требуется отсутствие linear retained-history growth; произвольное заранее заданное число turns/sessions не является product contract.

## T45 prompt/delegation/DCP qualification (approved 2026-09-27; pending)

Detailed specs SUB01/SUB02/CTX01/CTX02/PRM01/DCP10/DCP11 in planning/acceptance.json
have only T45 as owner; supplements A02/A03/A04/A05/A07/A10/A13. No duplicated
owner assignment to earlier T24/T36/T40 or to T44 visual scenarios.

- SUB01 uses provider barriers to demonstrate simultaneous independent foreground
  children and background progress before parent completion, including command route.
  SUB02 injects crash/cancel at admission/completion/delivery; verify lineage, bounded
  cleanup, deduplicated notices and no unknown-effect replay, not exactly-once claims.
- CTX01 captures exact selected parent text/roles/provenance and immutable pack on
  background/restart/continuation. Invalid/foreign/reverted/future/unsupported/over-
  budget selection creates no child/input; compressed raw text is not a reverted branch.
  CTX02 captures independent DCP-off IDs, automatic-context guidance and policy-backed
  capability preview; no unselected history or quote-to-system promotion.
- PRM01 proves the actual shared root/child base/custom/environment/tool/skill/AGENTS
  lanes and initial/nested/changed/removed/restart/compaction/Revert lifecycle within
  immutable-generation/trust boundaries. Existing R6/R7 profile/host fixtures remain.
- DCP10 checks child default true/false/off/manual/deny/Explore, parallel-session state
  isolation and active task/pack protection/release/safe recovery. DCP11 repeatedly
  recompresses and restarts using fixed equal active data over small/large inactive
  archives, measuring loaded rows/depth and peak/retained process state. No lifetime
  call/block quota; keep payload/cycle/no-gain/model/memory/turn guards. Freeze workload
  before qualification, never raise A10 regression caps or make a finite cycle count
  a product lifespan. Do not infer sustainability from the single-compression E2E.

Use source-derived fixtures/captured fake-provider requests/actual binary and existing
bounded live envelope, not days of paid prompts. Plan-only checks prove structure,
not these runtime outcomes; executed historical reports remain unchanged.

## Evidence cadence

- Slice: targeted test; checkpoint только при handoff/block/non-idempotent external action.
- Task finish: все назначенные IDs и один sanitized `evidence/Txx/report.md`.
- T25/T26/T28/T29: по одному соответствующему offline/integration campaign, без дублирующих suites.
- T16/T27: bounded explicit live campaigns.
- T30: roll-up evidence и final-code-commit AUD38/AUD39 requalification после backend changes; прежний T42 PASS не заменяет проверки нового кода.
