# Goal: исправить DCP и устойчивый отказ context admission

Status: active
Source: запрос пользователя о полном устранении блокировки; исправленный план после параллельного аудита; «Делай копию плана в цель и итеративно реализовать и коммит пуш билд».
Last updated: 2026-10-25

## Objective

Объявленные DCP anchors разрешаются планировщиком, а продолжение измеренного контекста не отвергается из-за повторного подсчёта ciphertext. Воспроизведённая сессия и совместимый v0 anchor продолжаются без повторения settled effects. Изменения проверены, закоммичены, отправлены в origin и собраны в `target/release/oc2`.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker, an approved budget boundary, or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: DCP предлагает только разрешимые anchors; resume/refresh не закрывают unfinished tail; измерение DCP сохраняет неизменённый checkpoint.
  - Source: исправленный аудит, часть DCP.
  - Acceptance: исходный replay успешно compress-ит объявленный диапазон; metadata-backed block поддерживает recompression; RAW/protections и checkpoint остаются неизменными вне спланированной проекции.
  - Primary evidence: `advertised_dcp_range_does_not_strand_large_context`, ближайшие formatter/recompression/resume/checkpoint-measurement tests.
  - Status: verified
  - Evidence: formatter, advertised-block recompression, checkpoint-neutral gain и resumed unique-tail tests PASS; исходный Owner replay GREEN (2.92s): m0002..m0005, один block, однократный effect, следующий submit принят. Scoped adapter Clippy и diff check PASS.
- R2: admission и automatic compaction используют согласованное measured-plus-estimated accounting, включая совместимое чтение v0 и resumed accepted prefix.
  - Source: исправленный аудит, accounting и compatibility.
  - Acceptance: next submit и tool continuation с настоящим admitted AGENTS и opaque payload проходят без summary/compress при подходящем measurement; совместимый сохранённый v0 anchor после reopen работает с auto=false; новый неизмеренный suffix не тарифицируется за ноль.
  - Primary evidence: независимый context-only regression, v0 reopen regression, anchor/suffix unit tests.
  - Status: in_progress
  - Evidence:
- R3: captured preparation, атомарность inbox/nudge и честная безопасная capacity-диагностика сохраняются.
  - Source: исправленный аудит, lifecycle/error projection; просьба объяснить причины комментариями к коду.
  - Acceptance: optional nudge preference коммитится с существующим inbox checkpoint, память публикуется один раз после commit; capacity не становится initialize/invalid_config; code comments объясняют изменённые инварианты.
  - Primary evidence: ближайший inbox rollback test, safe diagnostic test, affected checkpoint/irreducible/byte/retry/no-replay regressions.
  - Status: pending
  - Evidence:
- R4: доставить проверенные изменения, обычные commits/push в проверенный origin и release binary.
  - Source: последний запрос пользователя.
  - Acceptance: targeted evidence и final offline gate успешны, diff в envelope, commits находятся в `origin/master`, locked release build создаёт `target/release/oc2`.
  - Primary evidence: scoped tests/Clippy, `bash scripts/test.sh`, Git local/remote HEAD, `cargo build -p oc2 --bin oc2 --locked --release`.
  - Status: pending
  - Evidence:

### Constraints

- Один mutation/Cargo owner; текущий worktree, non-root. Не ослаблять model limits, output/safety reserve, 32 MiB byte cap, protections, FULL durability, cancellation или no-effect-replay.
- RAW/provider input/instruction delivery не переписывать ради accounting. Сохранять captured model/config/authority и opaque provenance.
- Не читать authoring config/auth/env, не изменять исходную пользовательскую БД, не replay пользовательских shell commands, не restart/kill native owner. Offline fixtures изолированы.
- Не менять test assertions/deadlines/load для green; не делать cargo clean, не удалять target/release и не менять CARGO_TARGET_DIR.
- Нет новых dependencies/services/tables/schema migration/public IPC. Разрешён один optional scalar в существующем JSON UsageAnchor и узкий RuntimeError для capacity.

### Non-goals

- Shell timeout redesign, косметика composer, live/visual qualification, произвольное восстановление несовместимого legacy scope, общий migration/context framework.

## Change Envelope

- Runtime owners: `dcp.rs`, `compaction.rs`, `runtime.rs`, `runtime/context.rs`, `runtime/turn.rs`, `runtime_compaction.rs`, `application.rs`, существующая `storage_children/parent.rs::checkpoint_turn_with_inbox`.
- Direct consumers/tests: существующий Owner replay в `application/authority/tests/shell.rs`, DCP/recompression/context/resume/compaction/storage-inbox/usage tests и `tests/runtime/context.rs`; safe diagnostics в `application/fatal_tests.rs`.
- Документы: эта цель и короткое уточнение accounting-контракта в `docs/ARCHITECTURE.md`.
- В Git не попадают копия БД, ciphertext, raw user payloads и диагностические дампы.
- Origin проверен: SSH github.com/0FL01/oc.git, текущая ветка master. Только обычный push, без force/tag/release.

## Исполняемый план — копия исправленного аудита

1. **DCP owner.** Добавить ordered eligible-anchor helper: user/assistant либо точный ID captured block. Formatter использует eligible tail; rows/blocks обновляются вместе на общей preparation boundary. Не дублировать текущий user при resume. В DCP measurement сохранить checkpoint одинаковым before/after и использовать полный candidate graph. Off/manual/deny stable-ID lane не перепроектировать. Комментарии: visibility не даёт compression authority, system-role block разрешает metadata, исчезновение checkpoint не является gain.
2. **Accounting и совместимость.** Один helper проверяет scope/prefix; только измеренный prefix получает provider usage. Новый suffix считается существующим serialized admission estimator, не content-only `estimate_tail`. Нормализовать только точные trusted Fact renderings; ordered active source semantics связываются со scope, служебные labels/placement не меняют accounting identity. Один `noncausal_tokens: Option<u64>` хранит стоимость фактически отправленных facts/anchors/nudge. Формула: measured input+output + новый serialized suffix + положительный рост noncausal overhead. Не вычитать heuristic cost из measured baseline. Resumed accepted prefix не считать новым irreducible prompt; genuinely new input и fixed/schemas сохраняют guard. Byte cap независим.
   - **v0 read path:** missing scalar означает literal v0. Воспроизвести producer scope (route, fixed до typed reconciliation, tools, variant), проверить старый prefix/items на bounded active projection с обычной provenance validation. Не переносить старый items в нормализованный v2 input. Считать новый v0 suffix и начислять полный текущий служебный слой без неподтверждённого credit. Не менять сохранённый anchor до следующего настоящего usage; не искать RAW archive и не делать скрытый model call. Несовпавший v0 scope/prefix остаётся честным fallback, не слепым rebind.
3. **Интеграция и доставка.** Полный borrowed candidate и speculative nudge готовятся до существующего compaction delivery; replacement отбрасывает preview и перестраивает запрос. Сохранить manual/overflow delivery, guards и captures. Существующий soft threshold ограничить фактическим hard budget без повторного переопределения buffer. Optional nudge pref включить в existing inbox-checkpoint transaction; после commit опубликовать cache/hint/stats один раз до следующей fallible operation. Receipt пишет enforced estimate. Узко типизировать input-budget refusal, byte overflow использует ContextOverflow; не переклассифицировать остальные InvalidArgs. Проверить targeted slices, scoped Clippy, final full gate один раз; обновить контракт, сделать commits/push и locked release build.

## Current Checkpoint

- Closes: R2/R3.
- Smallest next action: общий measured-prefix/serialized-suffix accounting и strict v0 branch; затем интеграция candidate preparation с атомарным nudge preference.
- Expected evidence: независимый контекст без DCP, v0 reopen при auto=false, suffix без usage не бесплатный; прежние byte/irreducible/checkpoint guards сохранены.
- Stop or replan if: обнаружен чужой concurrent diff, нарушена data/permission boundary или минимальный fix выходит за frozen envelope.

## Current State

- Resolved: R1. DCP domain/refresh/resume/measurement исправлены; исходный replay GREEN. Это не заменяет независимую проверку accounting R2.
- Last relevant evidence: direct adapter unit binary `oc_adapters-0406160fd5967598`, пять targeted cases PASS; после использования canonical accepted-tail ID resume и Owner replay повторно PASS (1.02s / 2.92s); scoped `cargo clippy -p oc-adapters --lib --tests --locked -- -D warnings` PASS.
- Blocker: none.
- Next: R2/R3 → R4.

## Material Decisions

- 2026-10-25: literal v0 compatibility вместо перепривязки старого measurement; новая запись только после подлинного usage. Исторический source set не угадывается.
- 2026-10-25: существующий compaction buffer сохраняется; hard budget ограничивает soft ceiling, а не маскируется повышением model limits.
- 2026-10-25: nudge preference использует существующий inbox transaction, без отдельного write после consumption.

## Checkpoint History

- 2026-10-25: контракт зафиксирован; текущий checkpoint R1.
- 2026-10-25: R1 verified; original replay GREEN, captured blocks и rows обновляются вместе, checkpoint не является ложным gain. Следующий checkpoint R2/R3.

## Completion

- Resolved outcomes: pending.
- Commands and artifacts: pending.
- Constraint and diff-scope check: pending.
- Final status: active.
