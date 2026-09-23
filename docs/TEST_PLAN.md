# Test plan — общая методика

Канонические scenario IDs и expected behavior хранятся только в `planning/acceptance.json`; authoritative task owner — только в `planning/tasks.json`. Task-specific executable tests и reports используют те же IDs. Повторный regression run допустим, но не создаёт второго owner.

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

## T48 Zen Free (не T27)

`ZEN01`: parser/filter and bounded public two-feed GETs (`cargo test --locked -p oc-adapters zen_catalog --lib`); opt-in read-only live preflight: `cargo test --locked -p oc-adapters read_only_official_catalog_preflight --lib -- --ignored`. Каждый fresh publish заменяет каталог; Chat POST требует повторной проверки ID непосредственно перед send. `ZEN02`: `cargo test --locked -p oc-adapters zen_chat --lib` и `cargo test --locked -p oc --test zen_free` (реальный бинарь с искусственными каталогами/Chat/SSE, continuation, title, child, DCP, restart, changed price и durable campaign slots). Response tests остаются отдельными.

`ZEN03`: единственный разрешённый путь прямой live-проверки — opt-in `live_free_chat_text_smoke` с явно выбранным `OC_TEST_ZEN_MODEL`, `OC_TEST_ZEN_LIVE=1` и существующим canonical private 0700 `OC_TEST_ZEN_CAMPAIGN_DIR` под изолированным HOME; неизвестный/платный ID не достигает Chat POST. Test-only adapter резервирует атомарно/долговечно один из 24 слотов **перед каждым** outbound Chat HTTP, включая title/child и failure, а max output для этого smoke ≤2048; без retry и MCP/search (0 из 4). Product без test-env не заявляет billing limiter. Реальный Zen key передаётся только через явно заданный `OC_TEST_ZEN_API_KEY`; без ключа запрос keyless, не `Bearer public`. Остановиться при 401/403/429: текущая документация Zen требует sign-in/API key, а Go-контракт не является Zen entitlement. Не запускать T27 campaign вместо T48 и не маркировать Zen smoke как A04/PROV08.

## Soak и память

T28 сначала фиксирует короткий baseline и measurement windows, затем до qualification замораживает достаточно длинный synthetic workload для проверки history/output/queue/cache/process behavior. Нагрузка не использует live paid prompts. Проверяются cold/warm baseline, peak, post-idle, PSS/RSS, task/process/queue/blob/cache/DB/WAL counters, crash injection, scroll/paging и cleanup.

Численные thresholds выводятся из baseline до optimisation candidate и не повышаются для сокрытия регрессии. Требуется отсутствие linear retained-history growth; произвольное заранее заданное число turns/sessions не является product contract.

## Evidence cadence

- Slice: targeted test; checkpoint только при handoff/block/non-idempotent external action.
- Task finish: все назначенные IDs и один sanitized `evidence/Txx/report.md`.
- T25/T26/T28/T29: по одному соответствующему offline/integration campaign, без дублирующих suites.
- T16/T27: bounded explicit live campaigns.
- T30: roll-up evidence и final-code-commit AUD38/AUD39 requalification после backend changes; прежний T42 PASS не заменяет проверки нового кода.
