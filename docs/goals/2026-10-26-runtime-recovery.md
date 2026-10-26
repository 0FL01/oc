# Goal: восстановление после provider failure и надёжная компакция

Status: active
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
  - Status: in_progress
  - Evidence: —
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

- Closes: R4.
- Smallest next action: один `bash scripts/test.sh`; при ошибке только failing/affected slice, не весь suite повторно.
- Expected evidence: strict Clippy/nextest/doctests/debug smoke, затем diff review/commit/push/locked release build.
- Stop or replan if: чужой concurrent diff; необходим выход за envelope или representative compaction не проходит узкий путь.

## Current State

- Resolved: R1/R2/R3; R4 in_progress, R5 blocked.
- Last relevant evidence: native-read, 24-case provider/retry и 6-case compaction slice PASS. Используется существующий `.local/tools/cargo-nextest` без установки.
- Next: final gate/доставка R4. Никаких live requests.

## Checkpoint History

- 2026-10-26: исправленный план и ограничения зафиксированы до runtime edits.
- 2026-10-26: R2 verified; source facts/base64 исключены из token estimate, byte retention и image validation неизменны.
- 2026-10-26: R1 verified; общий pre-send boundary используется HTTP DNS и WS establishment; shared resolver сохраняет network/security distinction у непосредственных consumers. Existing retry owner/allowance не изменены, backoff и контракт обновлены.
- 2026-10-26: R3 verified; complete protected-capable source допускается под существующим 32MiB cap, model admission выполняется по canonical replay/final frame. Captured DCP передаётся через selectors/checkpoint projection; combined selection bounded. Ложная irreducibility/нулевые измерения удалены, неизвестный prefix по-прежнему fail-closed. Новый planner не понадобился.

## Completion

- Не завершено. R5 несовместим с текущими применимыми ограничениями; конечный retry fix не является infinite-recovery PASS.
