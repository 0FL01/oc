# Goal: LLM-provider retry и безопасное продолжение как в OC2

Status: active
Source: требование владельца о полном parity provider-error retry (лимиты и 5xx)
и его TUI-представления; утверждение исправленного после RECON/параллельного аудита
плана 2026-09-29. Reference: `opencode` v2.0.12,
`2670273ff17da96f85c5826ced57aa1b368754fa`.
Last updated: 2026-09-29
Task: T54 (todo; документ — план, не execution PASS).

## Objective

Для допущенных native LLM-provider вызовов `oc` воспроизводит конечные
классы ошибок, задержки и continuation retry pinned OC2 без повторения
подтверждённых или неизвестных эффектов. RET01 квалифицирует backend/Responses;
T53/GO03 подключает следующие Chat/Messages wires к тому же owner, а T44/VIS43
после явного resume квалифицирует внешний вид. Пока backend и visual gates не
пройдены, заявлять полный retry parity нельзя.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: один типизированный contract исхода физического LLM-запроса.
  - Source: pinned `packages/ai/src/provider-error.ts`, `route/executor.ts`,
    `route/client.ts`, `protocols/open-responses.ts`; A04/PROV06/AUD11/AUD13.
  - Acceptance: RET01 различает структурированные HTTP/SSE ошибки и
    transport/read/incomplete-stream, сохраняет только безопасную диагностику;
    parsing/status/auth/permission/trust/caps никогда не становятся generic
    неограниченным retry. Adapter исполняет ровно одну физическую попытку.
  - Primary evidence: fake HTTP/SSE fixtures для 429 throttle и quota, 402,
    408/409/5xx, 400/401/403/413, ошибок внутри HTTP 200 SSE, EOF после
    partial output и наблюдаемых retry headers.
  - Status: pending
  - Evidence: pending — evidence/T54/report.md.

- R2: конечное ожидание и истинное продолжение одного логического шага.
  - Source: pinned `packages/core/src/session/runner/{retry,step,llm}.ts`;
    текущие `docs/CONTRACTS.md` и no unknown-effect replay.
  - Acceptance: RET01 доказывает первоначальный запрос + до десяти retry,
    общий allowance прозрачной попытки и continuation, отменяемое ожидание и
    отсутствие второго HTTP retry-loop. До output сохраняется assistant ID; после
    output partial span завершается неуспехом, retry публикуется до ожидания,
    следующий span получает новый ID и историю, а завершённые tools/unknown
    effects не переисполняются. Попытки не тратят agent-step allowance.
  - Primary evidence: deterministic clock/RNG fake provider: mixed
    pre-output/partial failures, exhaustion, terminal auth/quota, retry override,
    interrupted read после output, cancel during backoff, persisted tool outcomes.
  - Status: pending
  - Evidence: pending — evidence/T54/report.md.

- R3: durable assistant-span факт и адресная presentation без второго владельца.
  - Source: pinned `session/message-updater.ts:213–255,402–409`,
    `session/projector.ts:298–313`; A02/A07/A08/A10 и T44/VIS43.
  - Acceptance: `RetryScheduled` сохраняет `{attempt, at, safe error}` на
    assistant-span в текущем TurnLog/storage/projection. Семантический старт
    существующего span снимает его retry; при новом ID очищается только
    последний незавершённый span. Исторический завершённый failed span с retry
    не становится активным ожиданием, не стирается общим success/cancel и не
    разрешает HTTP dispatch после crash. Focused/parked views, reopen и
    headless различают retry от terminal failure; stale events не меняют чужую
    session/turn. Новые retry diagnostics/UI/SQL-поля не содержат raw failure
    body, auth headers или credentials; canonical history не переписывается.
  - Primary evidence: owner journal/query и actual-binary fake-provider
    headless/PTY: reschedule, due, partial→continue→success→reopen, tab park,
    restart/cancel и ordering snapshot/events без ложного нового запроса.
  - Status: pending
  - Evidence: pending — evidence/T54/report.md.

- R4: сохранить лимиты, другие LLM lanes и существующие gates.
  - Source: pinned `session/compaction.ts`, `session/title.ts`; GOAL.md
    A01–A13, PROV07/AUD11/AUD13, T53/GO03, docs/TEST_PLAN.md.
  - Acceptance: один счётчик фактически выданных физических generation HTTP
    requests у owning operation охватывает retries, связанные child/compaction
    и title; logical step/round caps не считаются скрытыми физическими
    запросами и не подменяются тестовым бюджетом ≤24. Сохраняются binding,
    модель, auth/headers и config generation без fallback; cancel, safe
    checkpoint, input/output caps, DCP/history и response-close-before-tool
    admission. Root/child шага используют ту же policy со своим allowance,
    поддержанные native/generated compaction summary/correction — auxiliary
    allowance, title — один вызов без main-loop. Новые протоколы T53/GO03
    потребляют policy, T54 не ожидает завершения всего T53/T44/T51.
  - Primary evidence: owner regressions + actual rebuilt binary fake
    Responses/headless/PTY и affected crate/workspace gates по TEST_PLAN;
    отдельные T53/GO03 и T44/VIS43 evidence без подмены результатов.
  - Status: pending
  - Evidence: pending — evidence/T54/report.md.

### Source-derived decision table

- Приоритет из pinned classifier: verified event/HTTP status и structured
  error code/type/message → context overflow только для client-scoped
  status (нет или 4xx) → 413/payload too large → content policy → quota
  (402, known insufficient_quota/usage_not_included/billing/Go account-cap
  codes, 429 с quota evidence) → authentication (401/403) → rate limit
  (429/known throttle) → provider internal (408/409/5xx) → invalid request
  (остальные 4xx) → UnknownProvider. Context overflow идёт в существующий
  compaction path, не generic retry; quota/auth/policy/invalid terminal by
  default, UnknownProvider retryable только для настоящего provider failure.
  Local policy/storage/validation errors не маскировать им.
- `x-should-retry` именно из наблюдаемых HTTP headers: точные строки `true`
  или `false` переопределяют default typed policy. Post-output incomplete
  stream/transport read может продолжаться даже при `false`, если это
  source-approved interrupted-stream case; это не replay исходного запроса.
  Transport delivery=rejected и accepted non-read по умолчанию terminal;
  generic `LLMEvent.providerError` — terminal, но Responses `response.failed`
  и `error` дают typed provider failure. Применять override только к
  provider-owned observed headers, не к input/config/local trust failure.
- Для HTTP failure `retry-after-ms` (конечное неотрицательное число)
  приоритетнее `Retry-After` seconds/HTTP-date. Только RateLimit/
  ProviderInternal используют provider delay как minimum, clamp ≤15 минут;
  nominal delays 2/4/8/10s (10s ещё 7 раз) с uniform jitter [0.8,1.2),
  затем max и ceil milliseconds. `at` записать до cancellable wait;
  `Retry due` не отправляет запрос сам. Для SSE `response.failed` под HTTP
  200 учитывать отдельный event status/code; HTTP 200 retry-after не
  пересчитывать в `retryAfterMs` (source behavior), но observed headers
  доступны для `x-should-retry`.
- `response.incomplete` с `max_output_tokens` означает правдивый
  finish=`length` (не бесконечный retry/полный нескороченный ответ),
  `content_filter` — failure, неизвестная причина — typed incomplete stream
  с ограниченным retry. Без настоящего terminal завершения EOF/failure/
  exhaustion остаются non-success; incomplete tools никогда не dispatch.

### Constraints and non-goals

- A02/A04/A07/A08/A10/A13, PROV06/PROV07/AUD11/AUD13, GO03 и
  permissions/trust/redaction/effect quarantine сохраняют одного owner.
  Отдельный T44/VIS43 обязателен для **визуального** заявления, но не
  done-зависимость всего T44 для backend T54.
- Production не получает JS plugin retry hook, SDK/HTTP double retry,
  new protocol/provider, MCP/discovery/startup retries, daemon, global
  restart auto-resumer или новую платную campaign. T51 readiness refresh
  не является paid generation retry. Native закрывает полный response
  до локального tool admission, в отличие от donor overlapping tool fibers.

## Change Envelope и порядок срезов

1. Зафиксировать эту таблицу и адресно supersede старые pre-first-event/
   per-turn 2-retry и blanket incomplete rules в CONTRACTS/PROV06/AUD11/
   AUD13/T53 GO03. Исторический PASS не подтверждает новый RET01.
2. `crates/oc-adapters/src/provider.rs` — bounded typed HTTP/SSE classification,
   safe diagnostic и один physical attempt; existing runtime turn owner —
   finite retry/continuation и счётчик dispatch. Добавлять узкий модуль
   только если его реально разделят primary/child/compaction; без нового crate/
   trait/общего store. Не re-enter user admission/agent-step при retry.
3. Через текущий TurnLog/SQLite/core query/event/TUI/headless DTO провести
   irreducible span/lifecycle факты, сохранив raw history и старые rows.
   T44 отвечает за final warning geometry/timing; T53/GO03 — за lowering
   typed failures/headers из новых протоколов.
4. Ближайший owner fake-clock/RNG/provider test → bounded actual rebuilt
   binary PTY/headless → affected resource/security gates и workspace quality
   на code commit. Mocked failures бесплатны; перед разрешённой live
   квалификацией соблюсти отдельный durable ≤24 physical-request campaign
   envelope, включая title/compaction/children/retries. Это не runtime cap.

Expected consumers: `runtime/turn.rs`, `runtime_compaction.rs`,
`application.rs`, `oc-core/src/{core_app.rs,queries.rs}`,
`oc-tui/src/{app.rs,messages.rs,app/live.rs}`, `oc/src/{headless.rs,tui_cmd.rs}`;
называть новые файлы по фактическим seams, не по типам. Один mutation owner;
перед execution сверить активную T51 dirty работу и qualified readiness seam.

## Current Checkpoint / State

- 2026-09-29: RECON и независимый аудит completed; docs-only контракт
  утверждён, implementation RET01/VIS43/GO03 NOT_RUN. T51 active;
  T44 PAUSED до явного resume; T53 todo; T54 pending без start через
  `progress.py`. Исторические reports/PASS не менялись.
- Graph-change rationale: T12 был техническим bounded Responses retry,
  но не закрывал source-derived quota/continuation/TUI; reopening completed
  T12, назначение T51 generation owner или смешение backend с PAUSED T44
  ломали бы ownership. Поэтому одна новая T54/RET01 зарегистрирована
  `todo`, GO03/T53 и VIS43/T44 сохраняют своих владельцев; существующий
  active T51 и его checkpoint не переназначены. `progress.py reindex/check`
  нужен после добавления записи, без заявления о product PASS.
- Next: после корректного scheduling handoff подтвердить текущие
  provider/turn owner paths и frozen typed classification table, затем
  минимальный Responses owner test для quota-versus-throttle.
- Blocker: нет для планирования; исполнение не начинается этой правкой.

## Evidence / Completion

RET01 имеет только T54 как owner; подробные cases и команды —
`planning/acceptance.json`, `docs/TEST_PLAN.md`. Отчёт:
`evidence/T54/report.md` после исполнения, не от публикации плана.
**T54 complete** означает validated Responses/common runtime backend;
**полный parity retry для поставляемых wires и TUI** — RET01 + GO03 при
допуске Chat/Messages + VIS43. Без фактических тестов и парных кадров
документация/JSON validator не даёт implementation PASS.
