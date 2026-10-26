# Goal: восстановление после provider failure и надёжная компакция

Status: unmet
Source: RECON двух отказов сессии передачи звука; параллельный аудит плана; «Делай копию плана в цель и итеративно реализовать и коммит пуш билд».
Last updated: 2026-10-26

## Objective

Предзапросный сетевой сбой использует существующие retries, native-read изображения не тарифицируются как base64-текст, а допустимая компакция не отвергается по размеру архивного журнала. Проверенные изменения доставлены обычными commits/push и release build. Буквальное бесконечное восстановление остаётся явно незакрытым из-за finite-контракта, не маскируется circuit breaker.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker, an approved budget boundary, or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: предзапросные transport/DNS failures и deadlines классифицированы для существующего retry owner; jittered backoff растёт до 60 секунд.
  - Source: исходный bug report, исправленный аудит и уточнение fair usage.
  - Acceptance: доказанный NotSent получает typed retryable failure; security/config/cancel и delivered-WS replay guards сохранены. Итоговый собственный backoff ≤60s, применимый более длинный Retry-After имеет приоритет; completed tools не повторяются.
  - Primary evidence: provider/endpoint timeout regressions, retry policy tests, существующие no-replay/cancellation tests.
  - Status: verified
  - Evidence: local nextest provider timeout/retry/endpoint/WS slice 24/24 PASS (4.685s). Typed NotSent Network/Deadline, authority/cancel, finite allowance, 60s jitter cap/900s server minimum, delivered WS no-replay сохранены. Общий gate ещё впереди.
- R2: native-read image accounting учитывает image heuristic, display и wrappers один раз, не считает base64 или attachment facts текстовыми токенами.
  - Source: подтверждённый RECON-дефект и исправленный аудит.
  - Acceptance: typed image правильно оценивается в fallback/неизмеренном suffix; measured prefix не тарифицируется заново, serialized bytes и validation сохранены.
  - Primary evidence: один focused native-read accounting regression и существующий usage-anchor срез.
  - Status: verified
  - Evidence: `cargo test -p oc-adapters --lib --locked tool16_stored_read_facts_require_bounded_complete_pixel_validation` PASS (0.10s); exact fallback formula, unmeasured suffix и measured-prefix control, native payload/validation assertions сохранены.
- R3: компакция допустимой активной истории не отказывает из-за архивного bloat при наличии protections.
  - Source: второй наблюдаемый отказ и исправленный аудит.
  - Acceptance: bounded полная eligible active projection рассматривается до усечения; captured protections/целые обязательные группы сохраняются. Финальный summary input и объединённый selected state допускаются до dispatch/commit; следующий primary request продолжает работу. Настоящий overflow отказывает без context mutation и без ложного утверждения irreducible.
  - Primary evidence: архивный bloat + protected fact → summary/продолжение; genuine refusal; ближайшие capture/current-segment/reopen/no-replay tests.
  - Status: verified
  - Evidence: 6-case compaction slice по совокупности initial run и failed/interrupted rerun PASS: admitted replay/next primary/RAW, final-frame refusal без dispatch/mutation, captured protection report, summary retry/correction capture, current-segment effect-once, host-overflow escape. Initial run был прерван shell deadline после компиляции; RAW baseline в новом fixture исправлен на фактически committed journal, не pre-commit representation. Общий gate ещё впереди.
- R4: проверка, commits, push и release binary.
  - Source: последний запрос пользователя.
  - Acceptance: targeted regressions и final offline gate, обычный push в проверенный origin, locked release build `target/release/oc2`; ограничения и непроверенное названы явно.
  - Primary evidence: test commands/results, Git local/remote SHA, locked release build.
  - Status: blocked
  - Evidence: локальные commits `c99bec99b`, `2e2ad0c52`, `66d6af4ae`, `5710212c5`. Один `bash scripts/test.sh`: fmt и strict workspace Clippy PASS; nextest 2418 passed / 6 failed / 18 skipped, exit100. Unchanged failed-only rerun: 2 PASS (memory bound, Home selection), 4 воспроизводятся (2 media/DCP, pending-submit draft, manual clipboard). Workspace doctests и debug build/help/version PASS (`oc2 0.2.0`). `cargo build -p oc2 --bin oc2 --locked --release` PASS (6m43s), `target/release/oc2 --version` → `oc2 0.2.0`, runtime source HEAD `5710212c5`. SSH-preflight `git ls-remote --exit-code origin refs/heads/master` отказал `Permission denied (publickey)`; push не выполнялся.
- R5: буквальное постоянное восстановление при outage/rate limit.
  - Source: «он должен постоянно долбиться в ретраи».
  - Acceptance: неограниченное восстановление до успеха/отмены.
  - Primary evidence: пересмотр запрещающего finite failure retries контракта; такой пересмотр сейчас не разрешён.
  - Status: blocked
  - Evidence: применимые AGENTS/developer constraints требуют finite failure retries. Нельзя обходить ограничение infinite open/probe циклами или обнулением allowance; R1 не закрывает R5.

### Constraints / Non-goals

- Один non-root mutation/Cargo owner, текущий worktree. Без новых dependency, service, persistence/schema, public API, универсального retry/planning framework.
- Captured model/config/authority, endpoint all-answer/peer checks, RAW/HOT, целые protocol groups, FULL durability, cancellation, finite allowance и no unknown-effect replay сохранены.
- Не читать authoring-agent config/auth, не менять исходную пользовательскую БД и не replay её tools. Не restart/kill native owner, не удалять locks.
- Не повышать model/resource caps и не менять usage scope/threshold ради green. Без provider-shared circuit breaker без подтверждённого конкурентного напора.
- Не развивать auth/discovery, protection defaults, TUI и live/visual qualification попутно. Shared consumer edits — только необходимая зависимость diff.

## Change Envelope

- `runtime.rs`, `tools/read.rs` и ближайшие native-image/usage tests.
- `provider.rs`, `endpoint.rs`, `provider/websocket.rs`, `runtime/retry.rs`, непосредственные error mappings и ближайшие tests.
- `runtime_compaction.rs`, `runtime/context.rs`, необходимый storage projection/admission путь и ближайшие compaction tests.
- Эта цель; короткая ссылка в `GOAL.md`; `docs/CONTRACTS.md` только для изменения backoff-контракта.
- Копия БД, live payloads/ciphertext, auth/config и дампы не коммитятся. Origin: `git@github.com:0FL01/oc.git`, ветка `master`, без force/tag/release.

## Исправленный план

1. **Accounting и provider recovery.** Локально исправить native-image estimator с regression. Нормализовать доказанные pre-send failures у owner; сохранить существующий retry owner, добавить capped jittered backoff до 60s с приоритетом Retry-After. Проверить точечно, коммитить проверяемые части независимо.
2. **Компакция.** Сначала bounded полная активная история под существующим host/source cap, канонический replay вместо archival model cost. Селекторы видят полную допущенную область и captured spec; итоговый frame/combined selection проходят admission. Если этот путь не исправляет воспроизведение, сначала измерить blocker; только тогда ограниченный full-scope protection discovery до omission. Непросмотренное не считается незащищённым.
3. **Доставка.** Targeted tests, один final full offline gate, review diff; commits/push и locked release build. Зафиксировать результаты и R5 как unmet, не объявлять весь исходный запрос complete.

## RECON evidence / границы доказательств

- Baseline `678f3a86e36c0c8585a279a10c4896d1ee628c25`, worktree clean. Приватный online backup в approved cache, quick_check ok; пути/raw содержимое в репозиторий не переносятся.
- Timeout: два prepared spans, один physical dispatch, второй span failed через 10035ms, retry не назначен; ранее выполненные tools completed. Bare DNS Deadline обходит typed retry policy; конкретная историческая DNS причина не доказана.
- Compaction: protection_admission_refused до summary dispatch; raw older journals 2158133 bytes, одна replay-копия с изображениями ≈1.50MB. Старый floor считает архив против model-derived source budget.
- Prefix SHA совпал с usage anchor; последний shell выполнял git init. Dynamic gitRepository входит в scope, поэтому full fallback после Git transition сильно подтверждён, но исторические scope/buffer не сохранены. Native image base64 двойного учёта 425116 bytes даёт ≈212558 лишних tokens. Не менять scope validation или thresholds.
- Read-only реконструкция старого summary: canonical older history 1497521 bytes / 267 items, quoted frame без исторического environment 1521935 bytes, запас 273969 bytes против сохранённого input budget 448976 tokens. Это не provider run и не реконструкция отсутствующего исторического environment.

## Current Checkpoint

- Closes: разрешённая runtime/build часть завершена; R4 blocked, R5 unmet внутри текущих ограничений.
- Smallest next action: отсутствует; runtime/build и локальная запись результата завершены. Push не повторять без исправленного доступа.
- Expected evidence: чистый worktree после docs commit; successful release artifact и исходный FAIL общего gate сохраняются в отчёте.
- Stop or replan if: дальнейшая работа требует SSH-авторизации, согласованного отдельного scope для gate blockers либо пересмотра применимого finite-контракта. Не расширять TUI/media/auth и не начинать новый backlog автоматически.

## Current State

- Resolved: R1/R2/R3 verified. R4 частично выполнен (commits/checks/build), blocked по push и whole-suite GREEN. R5 blocked применимым finite-контрактом; цель в целом unmet, не complete.
- Last relevant evidence: targeted slices PASS; full gate exit100 и failed-only rerun сохранены без изменения guards/assertions/deadlines/load. Full gate elapsed ≈12m49: Clippy 28.21s, affected workspace compile 5m46, tests 395.349s. Test stage <10min; UI07 32.948s входит в согласованное исключение. Долгий общий run складывается из compile/link и suite, не одного зависшего test.
- Blocker: push — текущий пользователь не авторизован SSH-сервером GitHub для настроенного origin; минимальный unlock — предоставить этому окружению разрешённый SSH-доступ. Не читать/перенастраивать authoring credentials и не подменять origin.
- Next: остановиться на blockers R4/R5. `target/release/oc2` создан, owner не перезапускался. Никаких live requests.

### Незакрытые gate failures (не новые задачи этой цели)

- `mcp12_dcp_sql_projection_reindexes_native_facts_and_reopen_keeps_raw_history` и `tool16_local_graph_dcp_fork_restart_preserves_bytes_after_source_change`: tests ожидают безусловное сохранение media после замены span без явных protections. Незатронутый DCP path выбирает только protected/live группы; это соответствует `docs/DCP.md:600–606,633–638`. Не возвращать забытые RAW/media ради green.
- `s06_pending_submit_allows_picker_but_refuses_departure_and_keeps_draft`: ожидание painted pending payload расходится с pre-baseline commit `52b6ebcb5` / guard `!pending.outstanding()` и существующим admission regression. Это не доказательство потери retained draft текущим diff.
- `vis27_real_pty_osc52_select_and_manual_clipboard_modes`: неизменённый Home mode path имеет окно до async `sync_home/apply_catalog`, когда новый Home ещё использует Linux Select вместо Manual. Достижимое объяснение extra OSC52; причина именно этого run не доказана.
- Attribution выполнен source/diff review двумя read-only reviewers и проверен parent. Эмпирический запуск baseline не выполнялся; его результаты не заявляются. Test bodies/связанные DCP/TUI production paths не менялись этой целью. Исправление текущего recovery diff этими failures не обосновано; нужен отдельный согласованный scope для contract/fixture reconciliation и clipboard investigation.

## Checkpoint History

- 2026-10-26: исправленный план и ограничения зафиксированы до runtime edits.
- 2026-10-26: R2 verified; source facts/base64 исключены из token estimate, byte retention и image validation неизменны.
- 2026-10-26: R1 verified; общий pre-send boundary используется HTTP DNS и WS establishment; shared resolver сохраняет network/security distinction у непосредственных consumers. Existing retry owner/allowance не изменены, backoff и контракт обновлены.
- 2026-10-26: R3 verified; complete protected-capable source допускается под существующим 32MiB cap, model admission выполняется по canonical replay/final frame. Captured DCP передаётся через selectors/checkpoint projection; combined selection bounded. Ложная irreducibility/нулевые измерения удалены, неизвестный prefix по-прежнему fail-closed. Новый planner не понадобился.
- 2026-10-26: R1–R3 закоммичены; push preflight blocked (GitHub SSH publickey). Разрешённые offline gate/build продолжаются, auth repair в scope не добавлен.
- 2026-10-26: единственный full gate exit100 (2418/6/18); unchanged failed-only rerun 2/6 PASS. Два независимых read-only reviewers проверяют attribution media/DCP и PTY failures; Cargo/build остаётся у одного parent owner. Whole-suite GREEN не заявляется.
- 2026-10-26: attribution закрыт без runtime edits/ослабления tests: три ожидания противоречат неизменённой retention/pending presentation policy; clipboard path также неизменён, конкретный run trigger остаётся неизвестен. Doctests/debug smoke PASS, release запущен; общий gate по-прежнему FAIL.
- 2026-10-26: locked release build/version PASS (6m43s, source `5710212c5`); разрешённая реализация остановлена на доказанных blockers R4/R5. Runtime diff остался в envelope; SQL schema/migrations, исходная БД, live owner, test guards и credentials не менялись.

## Completion

- Resolved outcomes: R1/R2/R3 verified; R4 blocked (неотправленные commits и FAIL общего gate), R5 blocked (finite failure retries). **Final status: unmet**, не полное закрытие исходного запроса.
- Commands/artifacts: targeted native-read / 24-case provider-retry / 6-case compaction evidence PASS; один `bash scripts/test.sh` exit100 (fmt/strict workspace Clippy PASS, nextest 2418/6/18); failed-only rerun 2 PASS / 4 FAIL; workspace doctests и debug build/help/version PASS; locked release build PASS 6m43s, binary `target/release/oc2`, version `0.2.0`.
- Delivery: runtime commits локальны; origin остаётся `git@github.com:0FL01/oc.git`, ветка `master`. SSH publickey refusal подтверждён до push; remote/credentials не менялись. Минимальный unlock R4 — разрешённый SSH-доступ и отдельный согласованный scope для оставшихся gate blockers; infinite recovery требует допустимого пересмотра finite-контракта, не скрытого reset/probe loop.
- Closure/scope check: чужого diff не обнаружено; все runtime/test edits внутри envelope и проверены после последнего изменения. Дополнительный planner, dependency, persistence/schema, new service/public API, replay завершённых/unknown effects или ослабление protections/limits не добавлены. Raw live payloads/DB/auth в Git отсутствуют. Полный suite повторно не запускался, whole-product/live/visual READY не заявляется.
