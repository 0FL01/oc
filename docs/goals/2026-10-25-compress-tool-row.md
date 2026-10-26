# Goal: компактная строка compress без ложного legacy

Status: active
Source: пользовательские screenshots и согласованный RECON-план; «legacy — выкидываем»; «делай копию плана в цель и итеративно реализовать и коммит пуш билд».
Last updated: 2026-10-26

## Objective

Обычный `compress` в transcript выглядит как spinner + `compress` во время
подготовки/выполнения и `✓ compress [topic=…]` после завершения. Task renewal без
`DcpRunSnapshot` не объявляется legacy. Live/replay показывают одинаковый typed
topic; проверенные изменения доставлены в origin и release binary.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker, an approved budget boundary, or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: компактная operation row для любого compress, а не только fallback.
  - Source: screenshots и согласованный план, шаг 2.
  - Acceptance: pending/running — анимированный spinner + `compress`; completed — `✓ compress [topic=…]` либо `✓ compress`, если typed topic отсутствует. Нет большой карточки/arguments/summary по умолчанию и надписи `metadata unavailable (legacy)`; negative states не получают success.
  - Primary evidence: ближайшие DCP renderer/animation/cached-indexed tests.
  - Status: verified
  - Evidence: initial TUI run 692 PASS/1 FAIL; failed full/indexed regression после canonical padding PASS, affected generic 9/9 PASS. Pending animation/negative states и default/opt-in tests PASS. Public multi-range event/query PASS (1.53s; 1.831s в полном повторе). Actual-binary AUD29 PTY PASS (3.33s): panel accounting и `✓ compress [topic=t39 span]`, без старого default report.
- R2: topic принадлежит owner и сохраняется live → final → history, включая task renewal без run snapshot.
  - Source: согласованный план, шаг 1; подтверждённый RECON.
  - Acceptance: runtime передаёт existing typed topic для renewal; live finish сохраняет его; snapshot topic имеет приоритет; TUI не разбирает raw JSON и не фабрикует accounting.
  - Primary evidence: existing root task-renewal scenario и TUI lifecycle regression без snapshot.
  - Status: verified
  - Evidence: root task-renewal backend scenario PASS (1.35s): Started typed topic, completed without snapshot, durable history topic и реальный HOT summary. Новая TUI live/final/replay renewal regression PASS в package run.
- R3: доставка и актуальный display-контракт.
  - Source: последний запрос пользователя; согласованный план, шаг 3.
  - Acceptance: relevant tests/Clippy, integration gate по правилам AGENTS и release build выполнены; изменения закоммичены и отправлены обычным push в проверенный origin; `target/release/oc2` собран.
  - Primary evidence: команды/результаты, Git local/remote HEAD, release version.
  - Status: in_progress
  - Evidence: relevant tests, docs checker, fmt и strict workspace Clippy PASS. Повтор полного nextest: 2411 PASS, 2 FAIL, 18 skipped, 853.595s, exit 100. Оба failed slices затем PASS: auth05 18.42s без auth edits, AUD29 3.33s после display assertion update. Scoped oc2/pty Clippy PASS (1.99s), workspace doctests PASS (0 cases), debug build/help/version PASS (51.69s). Commit/push и release build ещё впереди; single-run whole-suite GREEN не заявляется.

### Constraints

- Один mutation/Cargo owner, non-root, текущий worktree; все Cargo-запуски максимум 4 jobs и 4 logical CPU, nextest максимум 4 slots.
- Не менять исходную пользовательскую БД, config, native owner/lock, RAW/HOT, алгоритм compression, permissions, no-replay, durability или accounting. Private DB backup не попадает в Git/fixtures.
- Отсутствие snapshot — не возраст записи и не failure. Не создавать legacy flag, fake snapshot, миграцию или backfill.
- Сохранить `/dcp`, bounded summary queries и frozen accounting. Compact row — operation, не дополнительное notification. Toast/off policy не создаёт duplicate reports; явно включённый `showCompression` сохраняет opt-in details, но не большую карточку по умолчанию.
- Не добавлять dependencies, public API/schema, таймер, слой или test framework. Не ослаблять существующие assertions вне изменённого визуального контракта, deadlines и нагрузку.

### Non-goals

- Удаление старой истории/совместимости, replay пользовательских tool effects, paid/live provider campaign, whole-product/pixel-parity qualification и остальные GOAL outcomes.

## Change Envelope

- Runtime metadata: `runtime/turn.rs::ToolCallEvent::Started`, существующий `dcp::presentation_topic`; прямой consumer `app/live.rs`.
- TUI: `dcp_view.rs`, `tools.rs` при необходимости для existing inline geometry/animation, existing lifecycle and rendering tests; `ToolRender::Dcp` и accounting DTO остаются.
- Backend evidence: `tests/subagent.rs::r9_root_task_renewal_and_spanning_range_keeps_tail_refusal`.
- R1 direct consumer: `crates/oc/tests/dcp_runtime.rs::vis38_public_multi_range_event_query_replay_and_bounded_summary_agree` ожидает старый pending block; обновляется только этот display assertion и проверка compact completion.
- R1 direct PTY consumer: `crates/oc/tests/pty_t39/interaction.rs::aud29_pty_panels_change_runtime_state` ждёт старое `Compression #1`, хотя реальный экран уже содержит `✓ compress [topic=t39 span]`. Заменить только это ожидание/комментарий; panel accounting и effect assertions не ослабляются.
- Docs: эта цель, ссылка в `GOAL.md`, актуальные DCP presentation sections `docs/DCP.md` и `tui-recovery/T44_CONTRACT_AMENDMENT.md`.
- Delivery: current master, verified `git@github.com:0FL01/oc.git`; no force/tag/release publication. Binary: `target/release/oc2`.

## Исполняемый план — копия согласованного плана

1. **Typed topic.** Дополнить producer existing `presentation_topic` для renewal; сохранить owner topic при started → completed без snapshot. Run snapshot остаётся источником frozen topic/accounting при его наличии. Не разбирать сырой JSON в TUI и не добавлять поля/миграции.
2. **Компактный renderer.** Заменить большую default DCP карточку обычной inline row: spinner + compress во время argument stream/running, success + typed topic после completed. Удалить ложную legacy-надпись, оставить честные negative states и existing clock/animation marker. Default не раскрывает summary/arguments; existing opt-in details и `/dcp` сохраняются.
3. **Регрессии и доставка.** Расширить ближайшие backend и live/final/replay/cached-indexed tests синтетическим renewal без snapshot, обновить старый display-контракт, выполнить scoped checks и integration gate один раз. После ошибки — только failed/affected slices. Собрать locked release на 4 CPU, commit/push, закрыть цель и остановиться.

## Current Checkpoint

- Closes: R3.
- Smallest next action: commit/push проверенного implementation diff, locked release build на 4 CPU, затем запись completion и её публикация. Полный suite повторно не запускать.
- Expected evidence: fmt/strict Clippy/workspace nextest/doctests/debug smoke; release version и совпадающие local/remote commits. После ошибки — только failed/affected slices.
- Stop or replan if: требуется новый persistent/public contract, private data fixture или изменение алгоритма compression.

## Current State

- Resolved: R1/R2. Compact row, existing animation и typed topic сохраняются без выдуманного snapshot или legacy classification; explicit details, accounting и controls остаются.
- Last relevant evidence: все стадии offline gate подтверждены по совокупности полного повтора и failed/remaining slices. Initial exit 100 сохранён. Причина первого auth05 failure не доказана; isolated exact rerun PASS без auth edits. Единственный case >30s — approved UI07 32.875s; aggregate process time по 2413 уникальным test positions — 3221.471s, четыре slots дают ideal lower bound 805.368s против actual 853.595s, поэтому warmed gate >10min объясняется пользовательским cap, не отключением guards.
- Blocker: нет; осталось доставить проверенный diff и собрать release. Пользовательская БД/config/owner не менялись.
- Next: R3 integration/delivery.

## Checkpoint History

- 2026-10-25: первый связанный batch готов; TUI 692/693 PASS (5.31s). R1 сравнение full/indexed выявило разную segmentation нейтрального отступа; следующий шаг — canonical padding и только failed/affected slices, затем ещё не выполненные backend/public checks. Docs checker и diff whitespace PASS.
- 2026-10-25: R1/R2 verified. Canonical padding исправлен без ослабления assertion; failed/affected slices PASS. Root renewal 1.35s и exact public multi-range 1.53s PASS; title-only failure первого public run сохранён в evidence, не назван whole-suite PASS. Следующий checkpoint — R3.
- 2026-10-26: пользователь явно запросил «повтори оффлейн гейт». Разрешён повтор полного run, несмотря на обычное правило failed/affected-only. Предыдущий лог `target/compress-row-offline-gate.log` сохранён без summary; новый run пишет `target/compress-row-offline-gate-rerun.log`, лимиты остаются 4 jobs/slots/CPU.
- 2026-10-26: полный повтор завершён (2411 PASS/2 FAIL/18 skipped). AUD29 является ещё одним прямым consumer нового display-контракта: scope расширен только на его stale literal/comment; auth05 проверяется exact isolated rerun без edits. Runtime diff не расширяется на auth. Следующие шаги — failed slices и оставшиеся стадии, не повтор всего suite.
- 2026-10-26: failed slices PASS (auth05 18.42s без auth edits, AUD29 3.33s после exact display update). Scoped Clippy, workspace doctests и debug build/help/version PASS. Runtime implementation не менялся после полного gate; последнее test-only ожидание проверено. Origin/master подтверждён на исходном `7d2c7a3c4`; следующий шаг commit/push → release build → closure document.

## Completion

- Resolved outcomes: pending.
- Commands and artifacts: pending.
- Constraint and diff-scope check: pending.
- Final status: active.
