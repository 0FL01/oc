# Goal: устранить причины 14 медленных тестов

Status: active
Source: «Что это за медленные тесты? нужно подумать как ускорить, время = деньги»;
последующий RECON/план и «длеай копию плана в цель и итеративно реализовать и коммит пуш».
Base: `07a277d35c4e8f57910d7e15a43df1e1f9ee6560`.
Last updated: 2026-10-17

## Objective

Реализовать согласованный план ускорения перечисленных ниже тестов, сохранив
проверяемые контракты. Для каждого устранить подтверждённую дорогую фазу и
проверить время; оставшийся выход за 30 секунд требует дальнейшего RECON либо
явного решения владельца, не автоматической индульгенции. Доставлять проверенные
срезы commit/push; закончить текущим зелёным полным gate и фактическими замерами.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied. Stop
substantive work at a proven external blocker, an approved budget boundary, or when
no remaining in-scope action has a falsifiable expected result; record the exact
evidence and smallest unlock.

## Frozen contract

- R1 — убрать случайное ожидание retry exhaustion из fresh-provider-failure.
  - Source: первый пункт согласованного плана.
  - Acceptance: управляемый terminal provider rejection после admission; прежние
    receipt/root/history assertions сохранены, время измерено, retry policy не меняется.
  - Primary evidence: существующий `runtime::turns` case из таблицы.
  - Status: verified.
  - Evidence: `R1-terminal-failure.log` / `R1-junit.xml` в каталоге замеров ниже:
    1 PASS, nextest0.819s против исходных88.260s в полном конкурентном gate.
    Это не одинаковые условия benchmark; один реальный provider request подтверждён,
    все прежние acceptance/root/history assertions сохранены.
- R2 — пакетная подготовка семи архивных/session fixtures.
  - Source: второй пункт плана.
  - Acceptance: все сообщения, IDs, порядок, events, bindings и tool states сохранены;
    весь измеряемый workload неизменен, включая 47 prune-операций VIS38 и SIGKILL.
  - Primary evidence: семь существующих cases с пометкой `seed` ниже, setup/case timing.
  - Status: verified.
  - Evidence: `R2-bulk-seeds.log` / `R2-junit.xml`: все7 PASS, все <30s в targeted run;
    `R2-seed-parity.log`: 1 PASS, owner IDs/rows/events и synchronous=FULL совпадают.
    Production storage/schema не менялись. Все47 VIS38 prune calls сохранены.
- R3 — убрать лишние filler-publication boundaries из registered-stream case.
  - Source: третий пункт плана, где 2051 caller append не является assertion.
  - Acceptance: те же 16,785,408 filler bytes и два исходных secret-split appends;
    bounded caller chunks, прежние cap/redaction/quota/lease/expiry assertions;
    каждая фактическая публикация остаётся durable.
  - Primary evidence: существующий registered-stream case и ближайшие publication tests.
  - Status: verified.
  - Evidence: `R3-filler-publications.log` / `R3-junit.xml`: existing case PASS12.717s,
    nearest publication-fault/shared-quota cases PASS2.150s/1.531s. Filler uses513
    bounded calls instead of2049, first two calls unchanged; exact admitted bytes
    additionally asserted. Production writer/publication/fsync untouched.
- R4 — оставшиеся retry/matrix/soak/shell случаи исправлять по дорогой фазе.
  - Source: следующий этап плана после простых исправлений.
  - Acceptance: сохранить шесть guard-сценариев и 11 unclosed attempts, все 24 matrix
    cases, 120 normal soak turns и оба настоящих 20 MiB producers. По замеру выбрать
    минимальную правку владельца; не начинать с общего Runtime-clock рефакторинга.
  - Primary evidence: четыре соответствующих существующих case, фазовые замеры.
  - Status: pending. Evidence: pending.
- R5 — сохранить настоящий UI07 discovery deadline и решить оставшийся бюджет явно.
  - Source: пункт плана о единственном очевидном 30s временном контракте; AGENTS.md.
  - Acceptance: production deadline 30000ms и реальная проверка не сокращаются;
    исключение с причиной/численным бюджетом возможно только после согласия владельца.
  - Primary evidence: UI07 case; решение владельца, если требуется исключение.
  - Status: in_progress.
  - Evidence: владелец явно ответил «Да, до 40 с» (2026-10-17). Единственное
    согласованное исключение — весь этот case до40s; осталось записать рядом с
    кодом и подтвердить текущим полным gate. Для других cases исключений нет.
- R6 — итоговая квалификация и Git delivery.
  - Source: итеративная реализация/commit/push; обязательные repository gates.
  - Acceptance: сохранены все исходные тесты/assertions и штатные opt-in ignores;
    `bash scripts/test.sh` PASS, опубликованы текущие individual/full timings,
    reviewed commits pushed. Новый процент ускорения заранее не обещан.
  - Primary evidence: targeted logs, nextest inventory/JUnit, полный gate, Git HEAD/origin.
  - Status: pending. Evidence: pending.

### Ограничения и границы

Не уменьшать контрактную нагрузку, не повышать timeout/ресурсные thresholds, не
добавлять retry/ignore/filter, не отключать fsync/security. Реальные измерительные
окна сохраняются; искусственные задержки заменяются наблюдаемой готовностью.
Не менять пользовательские данные/config, schema/migrations, production retry
policy, public API или архитектуру ради fixtures. Никаких новых dependencies,
cache/service/worker/store/универсального fixture framework. Тестовый SQL допустим
для prelaunch seed по существующему образцу, не для измеряемых runtime effects.
Один mutation owner; один Cargo process/target; owned disk TMPDIR, без clean,
CARGO_TARGET_DIR override, реального API и запуска `oc2-dev`.
Это не закрытие всего T44 и не повтор уже завершённой цели selection/80%.

## Копия порядка плана

1. Сначала маленькая fixture-правка 88s case: terminal provider failure вместо
   retryable connection refusal; сохранить post-accept semantics/assertions.
2. Затем bulk **prelaunch** seeding семи cases; не оборачивать Db methods чужой
   SQL transaction и не добавлять production test API. VIS38 batching — между
   всеми обязательными prune boundaries, а не вместо них.
3. Затем bounded filler appends registered-output case, если inspection подтверждает
   отсутствие контракта на число publications. Каждый реальный append durable.
4. После этого retry-11, 24-isolated-Db matrix, 120-turn soak и 2×20 MiB shell:
   фазовый профиль → конкретная причина → минимальная коррекция, без урезания cases.
5. UI07 с реальными 30s — кандидат на явно согласованное исключение, не разрешение
   агента самому назначить его. Ancillary setup можно улучшать отдельно.
6. После каждой связной правки один ближайший timed check и review diff, затем
   commit/push; полный gate после связного batch, не неизменённые долгие повторы.

## Исходные 14 случаев

Время — из завершённого зелёного `fast-gate-7`, с конкуренцией десяти slots, без
компиляции/очереди. Это не изолированные фазовые замеры и не обещание экономии wall time.
Все пути ниже относительно `crates/`.

| Case | s | Owner | Этап / сохранённая нагрузка |
|---|---:|---|---|
| `foreground_children::batched_pre_effect_guards_and_child_budget_fail_without_sibling_cancellation` | 99.333 | `oc-adapters/tests/fixtures/foreground_children.rs` | R4: 6 scenarios, requests 2/1/2/3/3/11 |
| `lifecycle::aud31_pty_bounded_backing_state` | 95.298 | `oc/tests/pty_t39/lifecycle.rs` | R2 seed: 21 sessions, 4200 messages, 260 intents/outcomes |
| `storage::tool_output::tests::tool21_registered_stream_cap_redaction_leases_and_shared_quota` | 89.601 | `oc-adapters/src/storage_tool_output/tests.rs` | R3: exact filler/redaction/cap/quota/leases |
| `turns::fresh_provider_failure_after_accept_retains_root_and_reports_failed_turn` | 88.260 | `oc-adapters/tests/runtime/turns.rs` | R1: acceptance/root/Failed/history |
| `lifecycle::s07_pty_equal_view_archive_resource_samples` | 86.204 | `oc/tests/pty_t39/lifecycle.rs` | R2 seed: 0/3000 archive + 200 shared each, actual UI/read |
| `lifecycle::vis38_pty_dcp_equal_active_archive_resource_samples` | 83.409 | `oc/tests/pty_t39/lifecycle.rs` | R2 seed: 3000×16KiB, 47 prunes, genuine manual compression |
| `dcp::renewal_tests::addressed_legacy_coverage_does_not_transfer_4096_historical_members` | 82.583 | `oc-adapters/src/dcp/renewal_tests.rs` | R2 seed: 4100 archived IDs, same legacy block/snapshot |
| `soak_two_epochs_bounded_growth` | 66.824 | `oc-adapters/tests/soak.rs` | R4: 2×6×10 turns, real tools/DB/RSS/context |
| `pty_long_history_starts_and_pages` | 60.525 | `oc/tests/pty.rs` | R2 seed: 3000 distinct messages, real paging/restoration |
| `approval_structural_invalid_child_never_asks_saves_or_starts` | 51.495 | `oc-adapters/tests/subagent.rs` | R4: 8×3 isolated cases, 48 requests |
| `shell::jobs::output_tests::tool21_shell_cap_and_shared_quota_keep_draining_effects_and_bounded_hot_state` | 48.460 | `oc-adapters/src/shell/jobs/output_tests.rs` | R4: 2×20971520 actual bytes, cap/quota/effect/final flush |
| `aud21_binary_sigkill_never_publishes_a_partial_multi_range_compression` | 42.289 | `oc/tests/dcp_runtime.rs` | R2 seed: 1825 messages, 32 ranges, actual durability barrier/kill/reopen |
| `explicit_foreign_and_unbound_rows_are_refused_without_claiming_or_listing` | 35.005 | `oc/tests/pty_t42.rs` | R2 seed: 1502 roots, 3 actual launches, final 1503 |
| `provider_readiness::ui07_native_discovery_deadline_is_local_and_explicit_known_model_works` | 32.822 | `oc/tests/pty_t39/provider_readiness.rs` | R5: real 30000ms, 2 discovery requests, repair/model on wire |

## Baseline / change envelope

Previous complete normal gate: 454.464s, nextest 405.502s, 2054 passed / 0 failed /
12 opt-in ignored, retries0. JUnit run `d4d0a9b5-9cc9-48bb-be5f-66001d65d953`;
logs `/home/stfu/.cache/opencode-tmp/opencode/text-selection-bHd1g1/fast-gate-7.{log,json}`.
Current measurements: `/home/stfu/.cache/opencode-tmp/opencode/slow-tests-bWYisv/`;
исходный JUnit сохранён как `baseline-junit.xml` до любых новых запусков.
14 slow cases total962.108 test-seconds, not additional wall time. Existing scheduler
is near its observed work/slot floor; first reduce unnecessary work, not shuffle priority.

Expected edits: listed test owners, their existing fixture/seed seams; private
test-local SQL within those targets. Production changes require a measured R1–R5
blocker at the actual owner and a recorded minimal envelope expansion first.
Test names and acceptance assertions remain discoverable; no unrelated cleanup.

### R2 targeted evidence

| Case (сокращённо) | Исходный full gate, s | Текущий targeted, s |
|---|---:|---:|
| legacy coverage4100 | 82.583 | 2.366 |
| PTY long history3000 | 60.525 | 3.086 |
| foreign/unbound roots1502 | 35.005 | 3.295 |
| actual32-range SIGKILL/reopen | 42.289 | 6.037 |
| AUD31 history/tool cards | 95.298 | 8.197 |
| S07 equal active/archive | 86.204 | 9.446 |
| VIS38 real DCP/archive | 83.409 | 15.430 |

Условия различны: семь выбранных cases против2054 в старом full gate. Setup
отдельно наблюдаем:3000 history messages119.7ms,4100 rows/events в одной транзакции;
VIS38 commits по64 сообщения перед прежними prune calls. Измеряемые runtime
операции и все прежние assertions не урезаны. Один новый parity test защищает
копию SQL-owner semantics; не новый framework или Cargo target.

## Current checkpoint / state

- Closes: R4. Drive only the unclosed fixture's emitted retry waits with guarded
  Tokio time advancement; first profile current isolated matrix/soak/shell phases.
- R1 delivered in `f493f191e523150752a93a7766aaa887f9a9c112`, pushed.
- R2 delivered in `9c87af602e93bf61efcdd2a797be51dbc34b225d`, pushed.
- R3 targeted PASS and diff reviewed; deliver this checked batch, then continue.
- Active repository task remains T44; this is its
  owner-requested qualification-performance slice, not a new general backlog.
- Stop/replan on a proven contract mismatch or required public/storage/clock redesign;
  never self-grant a time-budget exception. No final PASS until R1–R6 are resolved.

## Checkpoint history

- 2026-10-17: copied the approved plan and existing RECON; no new benchmark invented.
- 2026-10-17 R1: retained all assertions, replaced incidental retry exhaustion;
  exact existing case PASS in0.819s, production policy unchanged.
- 2026-10-17 R2: all7 existing cases and one new owner-parity check PASS; setup
  transaction amortization only, no removed rows/events/actions/fsync guarantees.
- 2026-10-17 R5: owner explicitly approved only UI07's40s real-deadline budget.
- 2026-10-17 R3: exact filler/redaction/cap/quota/lease assertions PASS12.717s;
  each actual caller append still uses the unchanged durable publication path.

## Completion

Pending implementation, measurements, final qualification and reviewed Git delivery.
