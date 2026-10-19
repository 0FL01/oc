# Goal: undo и pending delegation — OC2 TS parity

Status: complete
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
  - Status: verified
  - Evidence: `cargo test -p oc-tui --lib --locked reverted_` — 4 PASS; `cargo test -p oc2 --bin oc2 --locked conversation_` — 3 PASS; new actual-binary `undo_replacement_hides_marker_while_response_is_held` — PASS (2.75s); existing `five_turn_reverted_tail_` — PASS; `cargo test -p oc-adapters --lib --locked conversation_` — 22 PASS. Все с `CARGO_INCREMENTAL=0` и approved TMPDIR.
- R2: pending subagent имеет точное inline-представление TS, без premature execution facts.
  - Source: второй баг, явное требование visual parity и исправленный план, часть B.
  - Acceptance: `Delegating…`, отступ 3, правильные glyph/gap/semantic colors и hover без действия; opaque input; один slot через Linked/Started; никаких child/effects из preview.
  - Primary evidence: production rendering/lifecycle tests и paired actual-binary captures.
  - Status: verified
  - Evidence: typed/opaque/Linked/hover/theme regressions PASS; `oc-tui --lib` — 607 PASS. Четыре paired static/animated captures 120×40 и 80×24: exact styled cells и component pixels PASS для normal/hover/inert-click; actual owner/nonexecution/one-child witnesses PASS (R4).
- R3: pending spinner соответствует lifecycle и animation policy TS.
  - Source: второй баг и исправленный план, часть B.
  - Acceptance: 10 braille frames/80ms, независимая фаза строки, без restart на repeated Pending; Linked сохраняет фазу до реального handoff; animation-off даёт `⋯`; правильный первый paint, cache invalidation и cleanup без собственного idle deadline.
  - Primary evidence: deterministic Instant/visible-cache tests и bounded unpaused actual-binary animation observation.
  - Status: verified
  - Evidence: deterministic phase/cache/80ms/nanosecond remainder/config/permission/owner cleanup PASS; actual-binary lifecycle suite — 13 PASS. В обоих animated paired geometries оба binaries естественно показали все 10 glyphs за bounded unpaused interval; read timestamps не выдаются за paint deadlines.
- R4: визуальная приёмка использует действительный reference и честные witnesses.
  - Source: visual parity, исправленный план, часть C.
  - Acceptance: same-config reference/native grid+PNG+cursor при 120×40 и 80×24, static/animated/hover pending; осмысленный prefix >=256 retained bytes; отсутствие execution подтверждается recognized native/TS schema и effect/provider counters; unmasked whole-frame differences не объявляются PASS. Reference текущего checkout либо доказанная scoped correspondence, без ложного current-HEAD claim.
  - Primary evidence: bounded paired probe artifacts и tests изменённых capture fixtures.
  - Status: verified
  - Evidence: `evidence/tui/pending-parity-{120x40-static-20261019-02,80x24-static-20261019-01,120x40-animated-20261019-01,80x24-animated-20261019-01}/pending-component-comparison.json` — TARGETED_COMPONENT_PASS во всех четырёх runs. Normal/hover/inert-click: styled cells и 152×16 RGBA pixels exact; обе стороны PASS_BEHAVIOR_ONLY, meaningful prefix 375 bytes, recognized schema/root, no premature effects, one real child после release. Полные grid/PNG/cursor seals и unmasked DIFFERENT сохранены, global PASS не заявлен. Fixture tests 12 PASS; comparator tests 25 PASS; JS syntax/negative-profile и PNG-analysis negative cases PASS.
- R5: релевантные проверки зелёные, изменения доставлены в Git.
  - Source: исправленный план, часть C; последний запрос commit/push.
  - Acceptance: targeted checks, tooling checks, full offline gate и diff-check успешны; goal отражает evidence; свои commits доступны в verified origin/master.
  - Primary evidence: реальные команды/exit codes, commit IDs и remote HEAD.
  - Status: verified
  - Evidence: финальный `CARGO_INCREMENTAL=0 TMPDIR=/home/stfu/.cache/opencode-tmp/opencode bash scripts/test.sh` — PASS: fmt, strict clippy, nextest 2143 PASS / 12 existing skips (386.646s), doc-tests, locked build, help/version. Перед ним три targeted workspace-feature gate cases PASS; Python guard tests — 13 PASS. `git diff --check` PASS. Финальные test fixtures — `977f9b0bf`, remote origin/master подтверждён тем же SHA.

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
- R5 gate diagnostics: `crates/oc/tests/pty_t42/shared_runtime.rs` — только bounded read-only turn outcomes в failure message исходного progress assertion; не менять condition, deadline, workload или product SourceMove.
- R5 HTTP-error fixture: `crates/oc/tests/live_bounded.rs::Peer::with_status` и `live_bounded/envelope.rs` — main error после наблюдённого completed title receipt; metadata-only failure diagnostics. Все исходные assertions, workload, deadlines, accounting и budgets сохранены; guard implementation не менялся.
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

- Closes: R5.
- Result: полный gate PASS; final test-fixture commit `977f9b0bf` опубликован, remote SHA подтверждён.
- Next: нет; frozen objective закрыт, substantive work остановлена.

## Current State

- Resolved: R1–R5 verified; plan `74162b02e`, undo `0232f6d17`, pending `448f40d2e`, visual qualification `ed7164b39`, gate fixtures `977f9b0bf` опубликованы.
- Last relevant evidence: full gate 2143 PASS; 607 TUI / 13 actual-binary lifecycle / 22 conversation / 12 capture fixture / 13 Python guard / 25 comparator tests PASS; JS syntax и diff-check PASS. Cargo с `CARGO_INCREMENTAL=0`, без clean/target override. Final binary byte-identical всем четырём successful paired captures; cursor equality проверена для 12 stage pairs.
- Blocker: нет.
- Next: нет; complete.

## Material Decisions

- 2026-10-19: literal source parity — `Delegating…`, не `Delegation`; scope — reported pending component, не full subagent lifecycle.
- 2026-10-19: audit found Linked-handoff/cache/first-paint/recovery/capture gaps; исправленный план выше является execution scope.
- 2026-10-19: повторный full gate воспроизвёл SourceMove failure. Минимальное расширение R5 — actionable failure-only diagnostics существующего assertion, без изменения поведения теста/продукта. Visual evidence независимо от полного gate; окончательная приёмка требует оба результата.
- 2026-10-19: owned 2.0.12 binary — scoped pending oracle (`2670273…`), не current `bc5ebf…` executable. Spinner/frame blobs совпадают; OpenTUI 0.5.10/0.5.17 различие остаётся явно указанным. Компонентный отступ 3 находится внутри session inset 2: actual global glyph column 5 и caption column 7 подтверждены TS source и обеими captures, без production adjustment.
- 2026-10-19: R5 receipt failure captured: основной generation complete/400, параллельный title generation остаётся reserved после client cancellation, включая поздний snapshot. Это корректно по TEST_PLAN (unknown/reserved spent) и existing Python cancellation test. Минимальное расширение envelope — только `Peer::with_status` в `crates/oc/tests/live_bounded.rs` и HTTP-error fixture: задержать основную ошибку до title receipt, без изменения всех исходных assertions/нагрузки/deadlines и без product/envelope accounting changes.

## Checkpoint History

- 2026-10-19: preflight/freeze, goal commit `74162b02e` pushed в verified origin/master.
- 2026-10-19: R1 receipt/controller checkpoint — 4 TUI + 3 binary tests PASS; минимальная recovery-only актуализация history, без schema/protocol изменений. Далее held-response PTY и existing conversation regressions.
- 2026-10-19: R1 complete — held-response PTY, existing five-turn redo и 22 conversation tests PASS; далее pending component.
- 2026-10-19: R2/R3 implementation checkpoint — 607 TUI tests и 13 actual-binary lifecycle tests PASS; typed renderer, per-slot clock, Linked/cache/prepaint/permission cleanup и inert hover. Tooling prototype tests PASS; actual paired visual qualification ещё не выполнена.
- 2026-10-19: первый full gate — 2142/2143 PASS; source-move progress case failed also in isolation, затем 7 diagnostic samples PASS. Временный диагностический diff полностью удалён; повторный gate без изменения deadlines/load/expectations. R4 tooling: 12 fixture + 25 comparator tests PASS, invalid-profile и PNG-analysis negative cases PASS; actual captures pending.
- 2026-10-19: R2/R3/R4 complete — четыре actual paired runs static/animated, 120×40/80×24, exact normal/hover/inert-click cells+pixels и nonexecution/one-child PASS. Первый probe обнаружил ошибочное ожидание global inset; исправлен только probe по TS source. Full-frame comparators сохранили DIFFERENT; далее только R5.
- 2026-10-19: третий full gate — source-move PASS, HTTP-failure accounting mismatch (journal 2 / peer 1). Минимальная R5 диагностика расширена только safe receipt failure message; guards/budgets и assertions неизменны.
- 2026-10-19: R5 complete — title receipt/main error упорядочены в fake peer, исходные assertions сохранены; targeted 3/3 и полный gate 2143/2143 PASS. Test-only commit `977f9b0bf` pushed; closure сверила все R1–R5, scope/constraints и current-binary visual evidence.

## Completion

- Resolved outcomes: R1–R5 verified; closure check PASS.
- Commands and artifacts: targeted suites и четыре paired episodes указаны выше; final `bash scripts/test.sh` и `git diff --check` PASS. Собран `target/debug/oc2`, SHA-256 `4b7167f8aa0188a7a1bc4739ce55d7a29aff4803ed6bf997af51912ebc97c2a8`, совпадает с sealed paired binaries.
- Constraint and diff-scope check: без runtime dependency/schema/protocol/authority changes, без user-owner restart; original test assertions/deadlines/load и guard accounting сохранены. Scoped component parity подтверждён; unmasked whole-frame DIFFERENT не скрыт и global parity не заявлен.
- Final status: complete.
