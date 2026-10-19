# Goal: устранить причины 14 медленных тестов

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: unmet
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

## Frozen contract

- R1 — убрать случайное ожидание retry exhaustion из fresh-provider-failure.
  - Source: первый пункт согласованного плана.
  - Acceptance: управляемый terminal provider rejection после admission; прежние
    receipt/root/history assertions сохранены, время измерено, retry policy не меняется.
  - Primary evidence: существующий `runtime::turns` case из таблицы.
  - Status: verified.
- R2 — пакетная подготовка семи архивных/session fixtures.
  - Source: второй пункт плана.
  - Acceptance: все сообщения, IDs, порядок, events, bindings и tool states сохранены;
    весь измеряемый workload неизменен, включая 47 prune-операций VIS38 и SIGKILL.
  - Primary evidence: семь существующих cases с пометкой `seed` ниже, setup/case timing.
  - Status: verified.
- R3 — убрать лишние filler-publication boundaries из registered-stream case.
  - Source: третий пункт плана, где 2051 caller append не является assertion.
  - Acceptance: те же 16,785,408 filler bytes и два исходных secret-split appends;
    bounded caller chunks, прежние cap/redaction/quota/lease/expiry assertions;
    каждая фактическая публикация остаётся durable.
  - Primary evidence: существующий registered-stream case и ближайшие publication tests.
  - Status: verified.
- R4 — оставшиеся retry/matrix/soak/shell случаи исправлять по дорогой фазе.
  - Source: следующий этап плана после простых исправлений.
  - Acceptance: сохранить шесть guard-сценариев и 11 unclosed attempts, все 24 matrix
    cases, 120 normal soak turns и оба настоящих 20 MiB producers. По замеру выбрать
    минимальную правку владельца; не начинать с общего Runtime-clock рефакторинга.
  - Primary evidence: четыре соответствующих существующих case, фазовые замеры.
  - Status: in_progress; acceptance remains unmet, see final disposition below.
- R5 — сохранить настоящий UI07 discovery deadline и решить оставшийся бюджет явно.
  - Source: пункт плана о единственном очевидном 30s временном контракте; AGENTS.md.
  - Acceptance: production deadline 30000ms и реальная проверка не сокращаются;
    исключение с причиной/численным бюджетом возможно только после согласия владельца.
  - Primary evidence: UI07 case; решение владельца, если требуется исключение.
  - Status: verified.
- R6 — итоговая квалификация и Git delivery.
  - Source: итеративная реализация/commit/push; обязательные repository gates.
  - Acceptance: сохранены все исходные тесты/assertions и штатные opt-in ignores;
    `bash scripts/test.sh` PASS, опубликованы текущие individual/full timings,
    reviewed commits pushed. Новый процент ускорения заранее не обещан.
  - Primary evidence: targeted logs, nextest inventory/JUnit, полный gate, Git HEAD/origin.
  - Status: pending; final budget qualification not achieved.
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

## Исходные 14 случаев

Исходное время — из зелёного `fast-gate-7`; последнее — из функционально зелёного,
но не прошедшего временную квалификацию `R6-last-exclusive-junit.xml`. Оба полных
прогона используют десять slots; последняя priority-проба затем отменена. Время
без компиляции/очереди, не изолированные фазовые замеры и не обещание экономии
wall time. Все пути ниже относительно `crates/`.

| Case | Было, s | Последний full, s | Owner | Этап / сохранённая нагрузка |
|---|---:|---:|---|---|
| `foreground_children::batched_pre_effect_guards_and_child_budget_fail_without_sibling_cancellation` | 99.333 | 12.459 | `oc-adapters/tests/fixtures/foreground_children.rs` | R4: 6 scenarios, requests 2/1/2/3/3/11 |
| `lifecycle::aud31_pty_bounded_backing_state` | 95.298 | 8.119 | `oc/tests/pty_t39/lifecycle.rs` | R2 seed: 21 sessions, 4200 messages, 260 intents/outcomes |
| `storage::tool_output::tests::tool21_registered_stream_cap_redaction_leases_and_shared_quota` | 89.601 | 23.301 | `oc-adapters/src/storage_tool_output/tests.rs` | R3: exact filler/redaction/cap/quota/leases |
| `turns::fresh_provider_failure_after_accept_retains_root_and_reports_failed_turn` | 88.260 | 1.574 | `oc-adapters/tests/runtime/turns.rs` | R1: acceptance/root/Failed/history |
| `lifecycle::s07_pty_equal_view_archive_resource_samples` | 86.204 | 7.879 | `oc/tests/pty_t39/lifecycle.rs` | R2 seed: 0/3000 archive + 200 shared each, actual UI/read |
| `lifecycle::vis38_pty_dcp_equal_active_archive_resource_samples` | 83.409 | 9.826 | `oc/tests/pty_t39/lifecycle.rs` | R2 seed: 3000×16KiB, 47 prunes, genuine manual compression |
| `dcp::renewal_tests::addressed_legacy_coverage_does_not_transfer_4096_historical_members` | 82.583 | 2.070 | `oc-adapters/src/dcp/renewal_tests.rs` | R2 seed: 4100 archived IDs, same legacy block/snapshot |
| `soak_two_epochs_bounded_growth` | 66.824 | **33.763 — unmet** | `oc-adapters/tests/soak.rs` | R4: 2×6×10 turns, real tools/DB/RSS/context |
| `pty_long_history_starts_and_pages` | 60.525 | 2.905 | `oc/tests/pty.rs` | R2 seed: 3000 distinct messages, real paging/restoration |
| `approval_structural_invalid_child_never_asks_saves_or_starts` | 51.495 | 20.685 | `oc-adapters/tests/subagent.rs` | R4: 8×3 isolated cases, 48 requests |
| `shell::jobs::output_tests::tool21_shell_cap_and_shared_quota_keep_draining_effects_and_bounded_hot_state` | 48.460 | 20.610 | `oc-adapters/src/shell/jobs/output_tests.rs` | R4: 2×20971520 actual bytes, cap/quota/effect/final flush |
| `aud21_binary_sigkill_never_publishes_a_partial_multi_range_compression` | 42.289 | 4.265 | `oc/tests/dcp_runtime.rs` | R2 seed: 1825 messages, 32 ranges, actual durability barrier/kill/reopen |
| `explicit_foreign_and_unbound_rows_are_refused_without_claiming_or_listing` | 35.005 | 3.095 | `oc/tests/pty_t42.rs` | R2 seed: 1502 roots, 3 actual launches, final 1503 |
| `provider_readiness::ui07_native_discovery_deadline_is_local_and_explicit_known_model_works` | 32.822 | 32.884 — approved≤40 | `oc/tests/pty_t39/provider_readiness.rs` | R5: real 30000ms, 2 discovery requests, repair/model on wire |
