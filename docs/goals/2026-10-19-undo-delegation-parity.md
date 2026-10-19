# Goal: undo и pending delegation — OC2 TS parity

Status: active
Source: запрос пользователя «делай копию плана в цель и итеративно реализовать и коммит пуш»; исправленный план после параллельного аудита.
Last updated: 2026-10-19

## Objective

Устранить волочащееся уведомление undo и добиться визуального паритета pending-строки subagent с локальным OC2 TS. Реализовать итеративно, проверить, закоммитить и запушить свою работу в verified origin/master без force-push.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: undo-marker снимается при подтверждённом принятии новой ветки, не при Enter и не при completion.
  - Source: исходный undo-баг и исправленный план, часть A.
  - Acceptance: pending/refused/stale receipt сохраняют boundary, draft и redo; accepted prompt/compress/Shell снимают marker; failure/interruption и завершившийся старый recovery не восстанавливают его. Whole-tail redo до нового admission не replay tools.
  - Primary evidence: receipt/controller regressions и actual-binary held-response PTY.
  - Status: pending
  - Evidence: —
- R2: pending subagent имеет точное inline-представление TS, без premature execution facts.
  - Source: второй баг, явное требование visual parity и исправленный план, часть B.
  - Acceptance: `Delegating…`, отступ 3, правильные glyph/gap/semantic colors и hover без действия; opaque input; один slot через Linked/Started; никаких child/effects из preview.
  - Primary evidence: production rendering/lifecycle tests и paired actual-binary captures.
  - Status: pending
  - Evidence: —
- R3: pending spinner соответствует lifecycle и animation policy TS.
  - Source: второй баг и исправленный план, часть B.
  - Acceptance: 10 braille frames/80ms, независимая фаза строки, без restart на repeated Pending; Linked сохраняет фазу до реального handoff; animation-off даёт `⋯`; правильный первый paint, cache invalidation и cleanup без собственного idle deadline.
  - Primary evidence: deterministic Instant/visible-cache tests и bounded unpaused actual-binary animation observation.
  - Status: pending
  - Evidence: —
- R4: визуальная приёмка использует действительный reference и честные witnesses.
  - Source: visual parity, исправленный план, часть C.
  - Acceptance: same-config reference/native grid+PNG+cursor при 120×40 и 80×24, static/animated/hover pending; осмысленный prefix >=256 retained bytes; отсутствие execution подтверждается recognized native/TS schema и effect/provider counters; unmasked whole-frame differences не объявляются PASS. Reference текущего checkout либо доказанная scoped correspondence, без ложного current-HEAD claim.
  - Primary evidence: bounded paired probe artifacts и tests изменённых capture fixtures.
  - Status: pending
  - Evidence: —
- R5: релевантные проверки зелёные, изменения доставлены в Git.
  - Source: исправленный план, часть C; последний запрос commit/push.
  - Acceptance: targeted checks, tooling checks, full offline gate и diff-check успешны; goal отражает evidence; свои commits доступны в verified origin/master.
  - Primary evidence: реальные команды/exit codes, commit IDs и remote HEAD.
  - Status: pending
  - Evidence: —

### Constraints

- RAW append-only, conversation-only undo; без filesystem rollback, migrations и второго authority/Db owner.
- Не разбирать partial tool JSON, не подменять child/effect/navigation authority визуальным loading state.
- Сохранить session/request/generation guards, newer draft/images, no replay, permission/cleanup guarantees.
- Один mutation/Cargo owner; не менять deadlines/load/baselines ради green; не трогать живой пользовательский owner/config/credentials.
- Только текущий non-root worktree. Bounded fake fixtures; реальные платные providers не нужны.

### Non-goals

- Полная переделка running/background/permission subagent UI, общий animation framework, новый protocol/API/registry/Cargo target.
- Plan Mode color и несвязанные whole-TUI расхождения.
- Маски differing regions, переписывание старых seals, global parity PASS на основе component evidence.

## Change Envelope

- Undo: `crates/oc-tui/src/app/{live,user_shell,transcript}.rs`, tests; `crates/oc/src/tui_cmd.rs` recovery и ближайшие tests; existing `pty_t39/interaction.rs`.
- Pending: `oc-tui/src/{app,tools,messages}.rs`, `app/{live,transcript}.rs`, direct indexed hover/paint consumers и ближайшие tests.
- Visual: существующие `scripts/tui_capture/{capture,composer}.mjs`, `composer_fixture.py` и direct fixture/config/bridge consumers, их tests; bounded gitignored artifacts.
- Goal — этот файл. Нет новых runtime dependencies/services/stores/migrations/frameworks.

## Копия исправленного плана

### A. Согласовать undo-state с принятием новой ветки

1. Маленький общий helper снимает `reverted` и принадлежащий исчезнувшей строке transcript interaction state.
2. Вызов только из matching successful model/compress/Shell receipt; для Shell сохранить authority-queried availability, не выдумывать checkpoint.
3. Pending, admission rejection и stale receipt не снимают boundary.
4. Управляемый controller regression: recovery получил staged page, затем принят новый запрос, recovery завершается позже.
5. Не применять устаревшую recovery page: актуальный bounded history read перед recovery apply; при ошибке сохранить view и retry, без fallback на старый page.
6. Проверить acceptance до completion, rejected/stale ACK, failure/interruption, mouse-down через admission и Shell refusal против post-admission failure.

### B. Исправить pending subagent

1. Сохранить `ToolRender::Subagent` вместо Generic в provider-draft handler. Preview остаётся opaque.
2. Row-local clock на существующем live-slot: 10 existing frames/80ms, независимая фаза, repeated Pending не restart, Linked сохраняет clock до настоящего handoff.
3. Инициализировать clock перед первым paint; reset при effective animation toggle, owner/terminal cleanup. Не использовать admission `ToolCard.started_at`.
4. Glyph только в render-copy. Инвалидировать cache на semantic transition и изменившемся glyph.
5. Точный `Delegating…`, padding/gap/colors, static-off separator и presentation-only hover без navigation.
6. Расширить existing lifecycle/goldens: warm cache, Linked без Started, Clear, permission/outcome cleanup, отсутствие duplicates; проверять delegation deadline, а не отсутствие scanner deadline во время busy.

### C. Доказать поведение и визуальный результат

1. Existing `vis28 held stream`: три завершённых обмена → два undo → marker → replacement admission → marker отсутствует до освобождения ответа. Current-screen positive anchors и structured request context; heartbeat hold — не partial-token evidence.
2. Opt-in composer fixture: flush meaningful incomplete prefix >=256 retained bytes; bounded acknowledged release; normal/hover, animations off/on; после release один настоящий child slot.
3. Animated readiness — observed state, не five-identical-frames. Bounded unpaused VT interval; paired stills по observed glyph/phase. Static screenshot не доказывает cadence.
4. Native/TS recognized schema и root witness, нет admitted child/execution/effects/child requests. Missing DB/schema не равно нулю; scripted proposal не execution.
5. Зафиксировать source/runtime reference, effective font/config, tools path. Готовый 2.0.12 допустим только как явно scoped oracle при подтверждённой correspondence; не выдавать его за текущий HEAD executable.
6. Full grid/PNG/cursor 120×40 и 80×24. Whole-frame comparators без masks; посторонние differences не PASS.
7. Targeted Rust + изменённые Python/JS fixtures + comparator checks, затем `bash scripts/test.sh`, `git diff --check`. Review, commit, push.

## Current Checkpoint

- Closes: R1.
- Smallest next action: receipt regressions и controlled recovery interleaving, затем минимальная правка.
- Expected evidence: targeted oc-tui и oc2 controller tests; held-response PTY.
- Stop or replan if: observed blocker требует выхода за envelope; не расширять scope из speculative audit findings.

## Current State

- Resolved: frozen contract и plan copied.
- Last relevant evidence: Rust HEAD `7c9298869e37e5885210dea7fa543f3bb10b6004`, clean worktree; branch master tracks origin/master; origin `git@github.com:0FL01/oc.git`; UID1000, available memory/disk выше TEST_PLAN minimum.
- Blocker: нет подтверждённого.
- Next: R1.

## Material Decisions

- 2026-10-19: literal source parity — `Delegating…`, не `Delegation`; scope — reported pending component, не full subagent lifecycle.
- 2026-10-19: audit found Linked-handoff/cache/first-paint/recovery/capture gaps; исправленный план выше является execution scope.

## Checkpoint History

- 2026-10-19: preflight и freeze; реализация/проверки ещё не начаты.

## Completion

- Final status: active.
