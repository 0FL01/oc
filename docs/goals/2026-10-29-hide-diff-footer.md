# Goal: скрыть служебный блок подтверждённого diff в TUI

Status: active
Source: запрос пользователя скрыть выделенный блок Edit/Patch и согласованный после аудита план; инструкция «копию плана в цель и итеративно реализовать и коммит пуш билд».
Last updated: 2026-10-29

## Objective

Убрать две выделенные строки под подтверждённым mutation diff, сохранив сам diff,
понятный признак усечённого preview, ошибки и факты capture.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: В TUI скрыть отдельный `[diff preview truncated; …]` и `Recorded capture: Complete; read(path=…)` при усечённом подтверждённом diff с reference.
  - Source: выделенный пользователем блок и исправленный после аудита план.
  - Acceptance: первый заголовок отображаемого усечённого diff несёт краткую пометку preview вместо отдельной summary-строки. Capture footer сохраняется при non-Complete либо при truncated Complete без reference, только если ранее было `total_files > 0` и валидная presentation. Ошибки, partial effects, inline warnings и успешный Write preview не меняются.
  - Primary evidence: существующие `patch_view` regression tests для edit/apply_patch, failed write, truncated/complete, недоступного reference, Quota, ошибок и replay.
  - Status: verified
  - Evidence: `CARGO_INCREMENTAL=0 cargo test -p oc-tui --lib --locked patch_view::` — 11 passed; edit/apply_patch, failed and completed write, replay, `Complete + None`, Quota, literal data, inline warnings и видимость пути при ширине 18/30.
- R2: Проверить передачу effects/presentation через live, настоящий checkpoint adoption и page/reopen; привести актуальное описание TUI в соответствие.
  - Source: исправленный после аудита план.
  - Acceptance: существующий lifecycle test действительно задаёт checkpoint window и part state; simulated fork без reference не приравнивается к полному рендеру source и не выдаётся за storage fork. `docs/TOOLS_MCP.md` описывает новое отображение.
  - Primary evidence: scoped lifecycle regression и review документации.
  - Status: verified
  - Evidence: `CARGO_INCREMENTAL=0 cargo test -p oc-tui --lib --locked app::tests::lifecycle::vis35_live_checkpoint_switch_fork_projection_and_reopen_keep_effects -- --exact` — 1 passed. `docs/TOOLS_MCP.md` обновлён; fork в TUI fixture смоделирован, а не проверен через storage.
- R3: Проверить, собрать release, закоммитить и отправить результат.
  - Source: инструкция пользователя «коммит пуш билд».
  - Acceptance: targeted tests, scoped fmt/Clippy и diff review завершены; `target/release/oc2` собран; изменения закоммичены и обычным push отправлены в проверенный `origin/master`, либо зафиксирован конкретный blocker.
  - Primary evidence: результаты команд, commit и push.
  - Status: in_progress
  - Evidence: scoped fmt и `CARGO_INCREMENTAL=0 cargo clippy -p oc-tui --all-targets --locked -- -D warnings` PASS; `CARGO_INCREMENTAL=0 cargo build -p oc2 --bin oc2 --locked --release` PASS, SHA-256 `5c846aa857432720b8eae81dad5496be8ccee600ce6bcc55b2a22e59118eb01e`. Остались diff review, commit и push.

### Constraints

- Не менять backend, storage, provider/model-facing output, capture и права доступа; не фильтровать пользовательский output по тексту.
- Не обещать полный diff или читаемость reference: отображается bounded preview, артефакт отдельно ограничен и может быть недоступен.
- Сохранить видимые non-Complete предупреждения там, где они уже показывались; не расширять поведение empty effects, legacy и completed Write.
- Один Cargo owner; не запускать пользовательский интерактивный клиент и не перезапускать native owner.

### Non-goals

- Изменение shell/generic/approval renderer, schema, зависимостей, public API, исторической завершённой цели или политика доступа к артефактам.
- Полный offline gate и live/visual qualification не требуются для локальной TUI-правки.

## Change Envelope

- Target: `crates/oc-tui/src/patch_view.rs::render`, его ближайшие tests, lifecycle test и `docs/TOOLS_MCP.md`; этот goal document.
- Capture guard: `total_files > 0 && (state != Complete || (truncated && reference.is_none()))` при существующей валидации presentation.
- Live и durable cards используют `card_from_row` → `tool_block` → общий renderer. Completed `write` возвращается раньше; failed `write` с effects проходит общий путь.
- Проверки последовательно: `cargo test -p oc-tui --lib --locked patch_view::`; точечный lifecycle test; `cargo fmt -p oc-tui -- --check`; `cargo clippy -p oc-tui --all-targets --locked -- -D warnings`; `git diff --check`; `cargo build -p oc2 --bin oc2 --locked --release`.

## Current Checkpoint

- Closes: R3.
- Smallest next action: итоговый diff review; затем commit и push в проверенный origin.
- Expected evidence: успешные команды, scope review и опубликованный commit.
- Stop or replan if: обнаружен чужой concurrent diff, обязательная проверка падает либо push требует изменения authority.

## Current State

- Resolved: R1 и R2; предыдущая историческая цель сохранена без изменений.
- Last relevant evidence: после правки узкого заголовка повторно PASS 11 patch-view tests, 1 lifecycle test, scoped fmt/Clippy и release build на исходном HEAD `4551870a`; `master` отслеживает `origin/master`.
- Blocker: нет.
- Next: проверки и доставка R3.

## Material Decisions

- 2026-10-29: отдельную summary-строку заменить краткой пометкой в заголовке: отсутствие указания может визуально выдать усечённый diff за полный. В узком заголовке сначала сохранять видимый путь, сокращая или обрезая пометку без новой строки. При `Complete + None` сохранить `no usable path`; `Some(path)` не гарантирует доступность.

## Checkpoint History

- 2026-10-29: R1–R3 зафиксированы после read-only RECON и независимого аудита.
- 2026-10-29: R1–R2 реализованы и проверены targeted tests. Первый компилятор был прерван по timeout, следующая incremental-линковка получила undefined LLVM symbols; после диагностики тот же тест PASS с `CARGO_INCREMENTAL=0`, без удаления artifacts.
- 2026-10-29: при review исправлено вытеснение пути меткой в узком заголовке; targeted tests, fmt/Clippy и release build повторно PASS.

## Completion

Pending.
