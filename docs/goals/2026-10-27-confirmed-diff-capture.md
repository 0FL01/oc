# Goal: полный подтверждённый diff при ограниченном preview

Status: active
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
  - Status: pending
  - Evidence: RECON HEAD `378058f0d`: archive получает короткий status, не diff.
- R2: Одно итоговое уведомление вместо каскада hunk/file/global.
  - Source: выделенные пользователем строки `[diff hunk preview truncated]` /
    `[diff file preview truncated]` и согласованный UI-план.
  - Acceptance: для edit/apply_patch один summary при усечении, без двух
    дублирующих footers; inline line-marker, точные totals, failure и syntax
    warnings сохраняются. Reference/state полного diff видим в карточке.
  - Primary evidence: ближайший `patch_view` regression для producer-derived
    truncated flags и полного fixture, unified/split и replay.
  - Status: pending
  - Evidence:
- R3: Проверить, собрать release, закоммитить и отправить результат.
  - Source: последняя инструкция пользователя.
  - Acceptance: targeted checks и обязательный full offline gate выполнены с
    честным результатом; `target/release/oc2` собран; свои изменения закоммичены и
    обычным push отправлены в проверенный `origin/master` либо точный внешний
    blocker записан без заявления о завершении.
  - Primary evidence: test/build exits, scope review, commit IDs и push result.
  - Status: pending
  - Evidence:

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

- Closes: R1/R2.
- Smallest next action: реализовать streaming confirmed-diff capture у общего
  commit/effect owner, связать reference/state с publication, дедуплицировать TUI.
- Expected evidence: воспроизводящий длинную строку/omitted tail regression,
  artifact identity через replay/reopen, focused renderer assertions.
- Stop or replan if: для результата требуется новый store/public API/migration.

## Current State

- RECON перепроверен на HEAD `378058f0d`; worktree чистый, origin/master совпадает.
- Текущие limits/каскад/отсутствие full diff archive подтверждены исходниками;
  существующие tests закрепляют усечение, не проверяют full-diff continuation.
- Blocker: нет. Next: implementation и targeted checks, затем integration/build.

## Checkpoint History

- 2026-10-27: контракт и копия согласованного плана сохранены до реализации.

## Completion

- Pending R1–R3; tests/build/push ещё не выполнены.
