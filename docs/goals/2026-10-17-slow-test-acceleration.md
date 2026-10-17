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
  - Status: verified.
  - Evidence: retry-11 PASS12.423s, все6 scenarios/11 HTTP attempts и десять emitted
    attempts2..11 сохранены. После фазового RECON matrix PASS20.420s вместо42.917s:
    все24 data roots независимы, копируется только закрытая пустая owner-DB.
    Native ASCII fast path: shell cap/quota PASS26.078s с прежними2×20MiB,
    641 ingress chunks/producer; остальные7 module cases также PASS (ANSI guard
    отдельно после исправления ожидаемого непустого prefix). Soak с единым dev
    opt-level1 и точным исходным телом PASS29.904s, `R4-soak-dev-opt1.log`/JUnit;
    cold compile3m24 отдельно. Полный конкурентный gate ещё не квалифицирован.
    Исправление атрибуции: `strace -c`0.571s отражает kernel CPU, не disk-wait.
    Завершённый `strace -w -c` измерил21.029s ожидания в2324 fsync при52.981s
    всего. Отдельный untraced run измерил23.18s userCPU/4.00s systemCPU. Оба
    вида работы существенны; нельзя заявлять, что прежний замер исключил I/O.
- R5 — сохранить настоящий UI07 discovery deadline и решить оставшийся бюджет явно.
  - Source: пункт плана о единственном очевидном 30s временном контракте; AGENTS.md.
  - Acceptance: production deadline 30000ms и реальная проверка не сокращаются;
    исключение с причиной/численным бюджетом возможно только после согласия владельца.
  - Primary evidence: UI07 case; решение владельца, если требуется исключение.
  - Status: in_progress.
  - Evidence: владелец явно ответил «Да, до 40 с» (2026-10-17). Единственное
    согласованное исключение — весь этот case до40s; причина/бюджет уже записаны
    рядом с кодом, осталось подтвердить текущим полным gate. Для других cases
    исключений нет; предложенное soak<=75s владелец отклонил: «Продолжить до ≤30 с».
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

R4 retry seam: enable only the existing Tokio dependency's dev `test-util`
feature. The unclosed fixture owns/polls its turn locally, observes the ten
durably published jittered retry deadlines, advances only their registered sleeps,
and resumes time before any next HTTP poll. No new package, production clock/API
or retry-policy change; other five scenarios keep real time. Profile matrix,
soak and shell setup/run phases before choosing any further edit.

R4 matrix profile measured24 independent setups at1.24–1.58s each. Seed each
still-independent data root from one test-owned, owner-created-and-closed empty
database before normal `Db::open`; do not share a live connection, runtime,
grants/history or persistent cache. Keep all24 executions and assertions.

R4 measured follow-up: empty-owner seeding makes matrix PASS20.420s with all24
isolated cases. Shell alone still takes30.579s:12.75s userCPU/2.18s systemCPU,
641 ingress chunks per unchanged20MiB producer (already full32KiB coalescing).
Allow one narrow production `TextStream::admit` fast path only when the native
parser equals its exact initial state and the normalized chunk contains only
printable ASCII/retained whitespace. Other parser states, controls, Unicode and
redaction retain the existing path; never bypass sync/publication/quota.
Soak remains54.523s across120 real turns. Inspection found two identical,
adjacent pre-dispatch checkpoints with no log mutation between them; remove only
the second redundant write. This does not waive the remaining30s investigation
or permit a new storage/schema/clock API.

R4 owner decision: continue soak to<=30s, rather than grant75s. Correct wall-time
profiling identifies21.029s in2324 fsync plus substantial CPU work, not an artificial
sleep. The current backend dev/test code is compiled at Cargo's default opt-level0.
The backend-only opt-level1 experiment passed43.839s. Adding bundled SQLite/JSON
at1 passed35.244s and cut measured turn CPU further, but still did not reach30s.
Optimizing all third-party dependencies plus the backend passed31.486s, still
above30s. The remaining workspace units still compile unoptimized. Use one Cargo
`profile.dev` opt-level1 rather than more package exceptions, preserving debug
assertions, overflow checks, SQLite API armor, release and every durable effect.
Remove temporary profiling callbacks/CPU clocks and restore the exact original
soak body; nextest still measures the whole case. Keep only measured benefit;
do not waive30s or add a new profile/runner/dependency/cache.

The original soak body passed29.904s with the single dev override; profiling
callbacks and process clocks are removed. Its measured2324 durable syncs dominate
host I/O and leave little room under30s. Reserve the existing nextest pool for
this one case, as for existing timing measurements, rather than benchmark other
cases' I/O contention. No test is omitted, retried or given a longer deadline.

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

- Closes: R5/R6. R1–R4 targeted checks завершены; исходный soak body PASS29.904s
  с единым dev opt1, без исключения/изменения120turns/fsync/assertions. Временная
  профилировочная обвязка удалена. Полный gate теперь проверит тот же состав тестов,
  все14 case budgets (только UI07<=40s), fmt/Clippy/doctests/debug build и реальные
  full timings. После commit/push R4 запустить его один раз, не повторять уже
  завершённые focused checks. T44 не закрывать.
- R1 delivered in `f493f191e523150752a93a7766aaa887f9a9c112`, pushed.
- R2 delivered in `9c87af602e93bf61efcdd2a797be51dbc34b225d`, pushed.
- R3 delivered in `a2867f2423231d23bf2bf338b00219f547e6b263`, pushed.
- R4 guarded retry delivered in `c329d206641f2a75661e38fdcc6a6b530e833c02`, pushed.
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
- 2026-10-17 R4 retry: guarded dev-only virtual waits PASS12.423s; no production
  clock/retry-policy change. Phase evidence identifies the next three edits;
  matrix/soak/shell are not silently accepted above30s.
- 2026-10-17 R4: matrix PASS20.420s; shell cap/quota PASS26.078s; all module
  security/publication checks pass, including split OSC/DCS/Unicode/redaction.
  Ранний soak sync summary показывал kernel CPU, не elapsed wait; его прежняя
  интерпретация как опровержения disk latency отозвана. No full-gate/completion claim.
- 2026-10-17 R4: backend opt1=43.839s, SQLite/JSON+backend=35.244s,
  third-party+backend=31.486s, single dev opt1=29.904s. Retained only the single
  coherent profile; restored original soak body. All120 turns/durable effects and
  debug/security checks remain. Cold compile is not included in case timing.

## Completion

Pending implementation, measurements, final qualification and reviewed Git delivery.
