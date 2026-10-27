# Goal: полный подтверждённый diff при ограниченном preview

Status: complete
Source: пользовательский RECON edit/apply_patch и инструкция «копию плана в цель и итеративно реализовать и коммит пуш билд», 2026-10-27.
Last updated: 2026-10-27

## Objective

Ограниченный diff preview не теряет единственную доступную запись подтверждённых
изменений: новый mutation outcome сохраняет diff в существующий filesystem
artifact и выдаёт путь/реальный capture-state. TUI не дублирует уведомления hunk/file.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: Сохранять подтверждённый diff до preview-усечения и возвращать его FS reference.
  - Source: согласованный RECON; вопрос пользователя о tool-output file/path.
  - Acceptance: длинная UTF-8 строка и хвост за лимитами preview доступны через
    зарегистрированный artifact `read`/`grep`; outcome и live/replay/reopen несут
    один и тот же reference/state. Запрошенный, но не выполненный patch не выдаётся
    за confirmed diff. Partial failure включает только подтверждённые effects.
    Complete означает полный нормализованный/redacted diff; cap/quota/IO остаются
    явно неполными, не вызывают повтор mutation или unlimited inline fallback.
  - Primary evidence: ближайший fake-provider runtime regression с реальным
    mutation, artifact continuation и reopen; focused effect-producer test для
    сохраняемого контента за пределами preview.
  - Status: verified
  - Evidence: output tests 12/12, patch tests 23/23, finish-failure regression PASS.
    На `bc6693882` runtime long-line/restart, external patch/edit/write и serialized
    cap/replay tests повторно PASS (0.91s / 2.41s / 0.93s).
- R2: Одно итоговое уведомление вместо каскада hunk/file/global.
  - Source: выделенные пользователем строки `[diff hunk preview truncated]` /
    `[diff file preview truncated]` и согласованный UI-план.
  - Acceptance: для edit/apply_patch один summary при усечении, без двух
    дублирующих footers; inline line-marker, точные totals, failure и syntax
    warnings сохраняются. Reference/state полного diff видим в карточке.
  - Primary evidence: ближайший `patch_view` regression для producer-derived
    truncated flags и полного fixture, unified/split и replay.
  - Status: verified
  - Evidence: patch_view tests 10/10 PASS; один summary, inline markers и
    capture failures проверены. Guard для отсутствующих confirmed effects PASS.
    Финальная подпись `Recorded capture` не называет старый status artifact diff;
    финальный renderer regression PASS для edit/apply_patch, replay и двух layouts.
- R3: Проверить, собрать release, закоммитить и отправить результат.
  - Source: последняя инструкция пользователя.
  - Acceptance: targeted checks и обязательный full offline gate выполнены с
    честным результатом; `target/release/oc2` собран; свои изменения закоммичены и
    обычным push отправлены в проверенный `origin/master` либо точный внешний
    blocker записан без заявления о завершении.
  - Primary evidence: test/build exits, scope review, commit IDs и push result.
  - Status: verified
  - Evidence: первоначальный `bash scripts/test.sh`: fmt/strict Clippy PASS,
    nextest 2439 PASS / 1 FAIL / 18 skipped; единственное падение —
    `dcp05_thresholds_frequency_turn_reset_no_accumulation` (ожидание `200000`).
    Новый commit `e31cf17b7` исправил ожидание: failed DCP slice теперь PASS.
    Affected runtime/TUI, scoped strict Clippy, doctests, debug help/version и
    release build PASS. После уточнения подписи renderer regression, fmt, strict
    TUI Clippy, release build и `target/release/oc2 --version` повторно PASS.
    Scope review PASS; реализация `79dd1aa38` отправлена обычным fast-forward push
    в `origin/master`. Исходный full gate не объявляется PASS.

### Constraints

- Сохранить resource limits preview, bounded diff work и existing artifact
  redaction/cap/quota/TTL/session/no-follow/identity guards.
- Один Db/artifact owner; не добавлять dependency, service, schema/migration,
  вторую full-copy в SQLite/TurnLog или новый public API.
- Сохранять approved-preimage, permissions, partial/unknown outcomes и no replay.
- Не перечитывать нынешние workspace-файлы для historical diff; не запускать,
  не перезапускать и не останавливать native owner пользователя.

### Non-goals

- Восстановление старых усечённых diffs, undo/snapshots, снятие native limits.
- Полный TS/pixel parity, большой diff-viewer/virtualization, unrelated backlog.

## Change Envelope

- Target: общий mutation/effect producer → existing artifact capture → outcome
  presentation → `patch_view::render`.
- Expected paths: `crates/oc-adapters/src/patch{.rs,/effects.rs,/mutation.rs}`,
  `tools{.rs,/output.rs}`, `runtime/turn.rs`, их ближайшие tests;
  `crates/oc-tui/src/patch_view.rs`; `docs/TOOLS_MCP.md`, эта цель и ссылка GOAL.
- Internal streaming sink/capture plumbing допустимы только для записи реального
  diff до ограничения; существующие public mutation helpers сохраняют контракты.
- Allowed artifacts: код/tests/docs и существующий native tool-output resource.
  Новые runtime stores, fixtures с пользовательскими данными и платные live-запуски
  запрещены. Только текущий non-root worktree, один Cargo/mutation owner.

## Current Checkpoint

- Complete: R1–R3 verified. Дальнейшая работа вне этой цели.

## Current State

- RECON baseline: `378058f0d`. Возобновлено на `bc6693882`; новые compaction commits
  `e31cf17b7` / `bc6693882` сохранены. Реализация `79dd1aa38` на `origin/master`.
- Реализованы capture до display caps, единый outcome reference и TUI summary.
- Static review выявил три риска нового diff: multiline secret после добавления
  signs; повторный capture status после transient failure; невидимая capture
  failure при полном DTO preview. Исправлены в том же scope, добавлены regressions.
- R1/R2 verified. Release `target/release/oc2` собран из финального runtime diff,
  version smoke: `oc2 0.2.0`.
- Blocker: нет. Next: нет.
  Полный gate не перезапускается и не объявляется PASS.

## Material Decisions

- 2026-10-27: каждый новый подтверждённый runtime mutation пишет один contextual
  diff в existing artifact. Это позволяет не хранить дополнительную полную RAM/DB
  копию до выяснения глобального preview-budget; calls без committed namespace
  change не создают diff artifact. Путь малого полного diff доступен в result/details, а TUI footer
  появляется при усечении или неполном capture.
- 2026-10-27: capture использует один буфер ≤64 KiB и прежние redaction/cap/quota/
  TTL guards. Nonempty lines многострочных known secrets также маскируются до
  публикации artifact; status clipping после diff-capture не может создать другой
  resource для того же operation. Schema/public API не меняются.
- 2026-10-27: footer использует нейтральную подпись `Recorded capture`: существующий
  metadata contract не различает старый status artifact и новый confirmed diff.

## Checkpoint History

- 2026-10-27: контракт и копия согласованного плана сохранены до реализации.
- 2026-10-27: plan commit `b32960c76`; первый targeted capture check — 2 PASS.
  Последующий batch закрывает конкретные review findings, проверка продолжается.
- 2026-10-27: targeted capture/patch/TUI/runtime checks PASS; full gate завершён
  с одним DCP failure, исправленным последующим независимым commit. После обрыва
  сохранены новые commits; выполняется только failed/affected continuation.
- 2026-10-27: failed/affected continuation и оставшиеся стадии gate PASS; после
  последней правки подписи renderer/Clippy/release PASS. R1/R2 verified.
- 2026-10-27: scope review PASS; code commit `79dd1aa38`, fast-forward push
  `bc6693882..79dd1aa38` в `origin/master`. R1–R3 verified, цель закрыта.

## Completion

- R1–R3 verified. Реализация: `79dd1aa38`, ordinary push `origin/master`.
- Release: `target/release/oc2`; финальные renderer/fmt/strict Clippy/build/version
  checks PASS. Исходный full gate: 2439 PASS / 1 FAIL / 18 skipped; затем исправленный
  failed DCP slice, affected checks и оставшиеся стадии gate PASS, без blind rerun.
- Constraints/scope review PASS: нет новых dependencies, stores, schema/migrations
  или public API; native owner и пользовательская БД не изменялись.
- Старые утраченные diff данные не восстанавливаются; artifact cap/quota/TTL и
  redaction сохраняются. Live/visual parity и whole-product READY не заявляются.
