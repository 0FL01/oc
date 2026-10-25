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
  - Status: verified
  - Evidence: independent measured-context/AGENTS/opaque next-submit + tool-continuation + literal v0 reopen при auto=false PASS (1.41s); unknown opaque suffix и source identity unit PASS; usage rollback/reopen, resumed prefix и checkpoint guards PASS. Никакой summary, eager upgrade или повторный tool effect не понадобились.
- R3: captured preparation, атомарность inbox/nudge и честная безопасная capacity-диагностика сохраняются.
  - Source: исправленный аудит, lifecycle/error projection; просьба объяснить причины комментариями к коду.
  - Acceptance: optional nudge preference коммитится с существующим inbox checkpoint, память публикуется один раз после commit; capacity не становится initialize/invalid_config; code comments объясняют изменённые инварианты.
  - Primary evidence: ближайший inbox rollback test, safe diagnostic test, affected checkpoint/irreducible/byte/retry/no-replay regressions.
  - Status: verified
  - Evidence: inbox/nudge write rollback и withdrawn-steer conflict PASS; safe typed budget diagnostic PASS; schema-inclusive irreducible, native capability, byte overflow, summary retry/reload и no-replay slices PASS. Scoped adapter Clippy и diff check PASS; комментарии объясняют accounting/legacy/transaction boundaries.
- R4: доставить проверенные изменения, обычные commits/push в проверенный origin и release binary.
  - Source: последний запрос пользователя.
  - Acceptance: targeted evidence и final offline gate успешны, diff в envelope, commits находятся в `origin/master`, locked release build создаёт `target/release/oc2`.
  - Primary evidence: scoped tests/Clippy, `bash scripts/test.sh`, Git local/remote HEAD, `cargo build -p oc2 --bin oc2 --locked --release`.
  - Status: in_progress
  - Evidence: commits `d647adb6d` и `d88b0090c` готовы. Полный gate: fmt/workspace Clippy PASS; initial nextest — 2378 passed, 27 failed, 18 skipped. Failed-slice rerun: 26 PASS без edits; remaining envelope исправлен после socket RED (reserved перед terminal bytes). Все 14 Python envelope tests PASS (7.848s), исходный failed native campaign PASS (4.46s), budgets/cancel guards сохранены. Remaining affected envelope consumers, doctests/build/help/version/release/push ещё не выполнены.

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
- Direct consumers/tests: существующий Owner replay в `application/authority/tests/shell.rs`, DCP/recompression/context/resume/compaction/storage-inbox/usage tests, `tests/runtime/context.rs` и capacity-assertion в `tests/runtime/turns.rs`; safe diagnostics в `application/fatal_tests.rs`.
- R2 fallout: удалить ставший неиспользуемым content-only estimator в `mcp_result/media.rs`; MCP delivery/serialization не менять.
- R4 blocker: минимальная правка существующего test-only HTTP envelope в `scripts/bounded_live.py` и его socket regression в `scripts/test_bounded_live.py`. Завершённый finite upstream фиксируется до выдачи терминальных байтов; quota, неизвестные/прерванные attempts, cancellation и deadlines не ослабляются.
- Документы: эта цель и короткое уточнение accounting-контракта в `docs/ARCHITECTURE.md`.
- В Git не попадают копия БД, ciphertext, raw user payloads и диагностические дампы.
- Origin проверен: SSH github.com/0FL01/oc.git, текущая ветка master. Только обычный push, без force/tag/release.

## Исполняемый план — копия исправленного аудита

1. **DCP owner.** Добавить ordered eligible-anchor helper: user/assistant либо точный ID captured block. Formatter использует eligible tail; rows/blocks обновляются вместе на общей preparation boundary. Не дублировать текущий user при resume. В DCP measurement сохранить checkpoint одинаковым before/after и использовать полный candidate graph. Off/manual/deny stable-ID lane не перепроектировать. Комментарии: visibility не даёт compression authority, system-role block разрешает metadata, исчезновение checkpoint не является gain.
2. **Accounting и совместимость.** Один helper проверяет scope/prefix; только измеренный prefix получает provider usage. Новый suffix считается существующим serialized admission estimator, не content-only `estimate_tail`. Нормализовать только точные trusted Fact renderings; ordered active source semantics связываются со scope, служебные labels/placement не меняют accounting identity. Один `noncausal_tokens: Option<u64>` хранит стоимость фактически отправленных facts/anchors/nudge. Формула: measured input+output + новый serialized suffix + положительный рост noncausal overhead. Не вычитать heuristic cost из measured baseline. Resumed accepted prefix не считать новым irreducible prompt; genuinely new input и fixed/schemas сохраняют guard. Byte cap независим.
   - **v0 read path:** missing scalar означает literal v0. Воспроизвести producer scope (route, fixed до typed reconciliation, tools, variant), проверить старый prefix/items на bounded active projection с обычной provenance validation. Не переносить старый items в нормализованный v2 input. Считать новый v0 suffix и начислять полный текущий служебный слой без неподтверждённого credit. Не менять сохранённый anchor до следующего настоящего usage; не искать RAW archive и не делать скрытый model call. Несовпавший v0 scope/prefix остаётся честным fallback, не слепым rebind.
3. **Интеграция и доставка.** Полный borrowed candidate и speculative nudge готовятся до существующего compaction delivery; replacement отбрасывает preview и перестраивает запрос. Сохранить manual/overflow delivery, guards и captures. Существующий soft threshold ограничить фактическим hard budget без повторного переопределения buffer. Optional nudge pref включить в existing inbox-checkpoint transaction; после commit опубликовать cache/hint/stats один раз до следующей fallible operation. Receipt пишет enforced estimate. Узко типизировать input-budget refusal, byte overflow использует ContextOverflow; не переклассифицировать остальные InvalidArgs. Проверить targeted slices, scoped Clippy, final full gate один раз; обновить контракт, сделать commits/push и locked release build.

## Current Checkpoint

- Closes: R4.
- Smallest next action: commit проверенного finite-response fix; оставшиеся affected envelope consumers без повторного Python suite, затем незапущенные doctests/build/help/version, locked release и push.
- Expected evidence: все стадии offline gate успешны по совокупности полного run, failed/affected slices и оставшихся стадий; исходный exit 100 остаётся в evidence. `target/release/oc2`, совпадающие local/remote HEAD и чистый worktree. Полный suite повторно не запускать по AGENTS.md.
- Stop or replan if: обнаружен чужой concurrent diff, нарушена data/permission boundary или минимальный fix выходит за frozen envelope.

## Current State

- Resolved: R1/R2/R3. DCP replay и независимый accounting/v0 path GREEN; атомарный nudge и безопасная capacity projection проверены.
- Last relevant evidence: finite-response socket regression воспроизвёл reserved до terminal delivery; после owner fix все 14 envelope tests и исходный failed native campaign PASS. 26 прочих initial failures прошли без edits; их причина не объявляется доказанной. Полная компиляция ранее заняла 27m01s; 305s envelope failure был ожиданием невозможного settlement, не медленным coding campaign.
- Blocker: нет подтверждённого blocker после envelope fix; R4 ещё не закрыт до remaining gates/build/push.
- Next: affected envelope consumers, remaining gates/release/push/closure. Runtime и PTY fixture-классификацию дополнительно не менять на основании неповторившихся падений.

## Material Decisions

- 2026-10-25: integration evidence собирается по стадиям: один полный gate и только failed/affected reruns после ошибки, как требует AGENTS.md. Initial exit 100 не переименовывается в PASS; утверждение о single-run whole-suite GREEN запрещено.
- 2026-10-25: R4 expanded только на существующий offline HTTP envelope и regression: два одинаковых settlement failures блокируют обязательный gate. Не подменять uncertain/reserved на complete по cancellation; ранний complete допустим лишь после доказанного полного чтения finite upstream body.
- 2026-10-25: проверка irreducible MCP отказа выявила ожидаемый direct-consumer fallout нового InputBudget; обновить только capacity-assertions там и в fresh-turn retry, сохранив zero-provider/no-history/no-compaction и остальные InvalidArgs проверки.
- 2026-10-25: scoped Clippy подтвердил отсутствие runtime consumers старого content-only estimator после объединения accounting; удалить его и зависимый MCP helper вместо dead-code allow или сохранения двух оценок.
- 2026-10-25: literal v0 compatibility вместо перепривязки старого measurement; новая запись только после подлинного usage. Исторический source set не угадывается.
- 2026-10-25: существующий compaction buffer сохраняется; hard budget ограничивает soft ceiling, а не маскируется повышением model limits.
- 2026-10-25: nudge preference использует существующий inbox transaction, без отдельного write после consumption.

## Checkpoint History

- 2026-10-25: контракт зафиксирован; текущий checkpoint R1.
- 2026-10-25: R1 verified; original replay GREEN, captured blocks и rows обновляются вместе, checkpoint не является ложным gain. Следующий checkpoint R2/R3.
- 2026-10-25: R2/R3 verified; 15 targeted cases, scoped Clippy и diff check PASS. Accounting/v0, nudge transaction, typed capacity и документированный контракт готовы к commit; следующий checkpoint R4.
- 2026-10-25: R2/R3 committed (`d88b0090c`). R4 full gate failed (2378/27/18); runtime/test limits не менялись. Следующий checkpoint — exact native failure и offline-envelope diagnosis, не повтор полного suite.
- 2026-10-25: 26/27 failed-slice rerun PASS без edits; остался offline-envelope settlement (campaign steps PASS, relay attempt uncertain). Достижимая гонка локализована между terminal delivery и ledger finish; следующий checkpoint — socket RED/GREEN без ослабления envelope guards.
- 2026-10-25: finite-response regression RED→GREEN; Python 14/14 и native five-step campaign PASS. Завершённый upstream фиксируется до terminal delivery, действительно прерванные attempts остаются consumed/uncertain. Следующий checkpoint — remaining gates/release/push.

## Completion

- Resolved outcomes: pending.
- Commands and artifacts: pending.
- Constraint and diff-scope check: pending.
- Final status: active.
