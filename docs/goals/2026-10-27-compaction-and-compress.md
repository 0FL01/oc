# Goal: compaction после renewal и compress без искусственного 8k-предела

Status: complete
Source: RECON чата «Кеширование вывода тулов в файл и обрезка длинных строк diff», воспроизведение дефектов, исправленный план после сравнения с upstream DCP; «План утверждаю, делай копию плана в цель и итеративно реализовать»; «и коммит пуш билд».
Last updated: 2026-10-27

## Objective

Сессия с большим журналом и короткими сообщениями продолжает работу после native shell notice, task renewal и compaction. Native `/compact` ставится в очередь занятого root; допустимый полезный summary не отклоняется из-за локального 8192-character cap. Проверенные изменения закоммичены, отправлены в origin и собраны в `target/release/oc2`.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker, an approved budget boundary, or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: убрать искусственный предел 8192 для authored compress summary.
  - Source: утверждённая коррекция плана после upstream RECON.
  - Acceptance: schema и обе validation-проверки не содержат этого cap; полезный summary длиннее 8192 принимается. Итоговый/раскрытый summary не упирается в другой произвольный 64 KiB потолок вместо существующих ресурсных границ.
  - Primary evidence: ближайшая range/renewal regression с большим summary, guards для настоящего resource overflow.
  - Status: verified
  - Evidence: pinned DCP 3.2.0 `d637981555a18c3992472268a0657a948925d5fa`, `lib/compress/range.ts:30–56` и `range-utils.ts:15–40` не задают maxLength. Cap удалён; large-summary 2/2 PASS и schema assertion в incident PASS. `dcp_atomic` 4/4 PASS (1.05s), включая настоящий expansion byte/depth/cycle overflow, no-gain/no-write и rollback.
- R2: compaction выбирает тяжёлый старый префикс по реальной HOT/provider-проекции и не выдаёт неустранённый overflow за восстановление.
  - Source: утверждённый план после RED runtime-теста.
  - Acceptance: native notice → renewal → automatic compaction → следующий primary проходит admission; summary покрывает выполненную работу. Replacement и хвост учитываются вместе; no-gain не блокирует полезное уменьшение. Текущий summary, protections и целые незакрытые группы сохраняются.
  - Primary evidence: `incident_shell_notice_renewal_compacts_large_journals_before_continuing`, ближайшие compaction/RAW/protection tests.
  - Status: verified
  - Evidence: исходный RED превращён в GREEN: compress >8192 принят с первой попытки, summary_has_task=true, checkpoint_is_initial=false, terminal=null. `full_background_notice_overflow_` 2/2 PASS (1.73s): irreducible notice выше host cap не отправляется для summary ни при большом, ни при недостаточном model budget; notice остаётся вне checkpoint. Прежний t47 model-admission PASS, исходный incident PASS (1.06s).
- R3: native `/compact` при занятом root использует существующую очередь worker без отмены jobs.
  - Source: утверждённый план после native ingress RED.
  - Acceptance: команда не получает TurnBusy; shell completion во время summary и child completion после него доставляются, без повторения effects/child launch.
  - Primary evidence: `incident_native_compact_with_shell_and_child_completions` через framed UDS ingress.
  - Status: verified
  - Evidence: native incident GREEN (1.12s): CompactSession Queued при active root + shell + child, оба completion assigned, один effect/child request. Joined-compaction client identity regression PASS: ownership сохраняется через TurnStarted/Finished и удаляется по typed terminal ID, включая terminal-before-ACK. Existing same-session reservation/overlap regression PASS (0.85s).
- R4: устранить воспроизведённый blocker reminder-теста без ослабления cadence/threshold assertions.
  - Source: утверждённый шаг проверки; отдельная причина Shell failed.
  - Acceptance: `dcp05_thresholds_frequency_turn_reset_no_accumulation` согласован с действующим request-reminder контрактом и проходит.
  - Primary evidence: существующий dcp05 и ближайшие reminder tests.
  - Status: verified
  - Evidence: dcp_auto::tests 4/4 PASS; dcp05 проверяет configured threshold 100000 и стабильность текста при изменении estimate, сохранив cadence/reset/manual-mode assertions.
- R5: проверки, обычные commit/push и locked release build.
  - Source: «и коммит пуш билд».
  - Acceptance: relevant regressions/Clippy и один full offline gate, review diff; собственный diff в origin/master, `target/release/oc2` собран. При external blocker сохранить точную диагностику, не объявлять доставку завершённой.
  - Primary evidence: команды/результаты, Git local/remote SHA, `cargo build -p oc2 --bin oc2 --locked --release`.
  - Status: verified
  - Evidence: единственный `bash scripts/test.sh`: fmt и workspace Clippy PASS; nextest 2443 PASS, 2 FAIL, 1 SIGTERM, 18 skipped, exit 100. Зависший fixture завершён адресным SIGTERM после 3499s; test DB показала 13 compaction dispatches. После исправления все 3 неуспешных теста PASS, notice tests 2/2, incident и native `/compact` PASS, affected compaction 29/29 и context 15/15 PASS. Scoped all-target Clippy, fmt, doctests, debug help/version и `cargo build -p oc2 --bin oc2 --locked --release` PASS. Исходный full gate не является PASS; весь suite повторно не запускался. Release binary включает незастейдженный пользовательский diff, поэтому не является точным образом commit. Код доставлен обычным push `e31cf17b72c7f8ed1b7169ae8d3638f3240bd364` на `origin/master`, SHA проверен через `ls-remote`.

### Constraints / Non-goals

- Один mutation/Cargo owner, non-root, текущий worktree. Сохранить существующий чужой diff по confirmed diff capture; не включать его в этот commit.
- Не менять model limits/output reserve, captured model/config/authority, RAW, protections, atomic call/result groups, FULL durability, cancellation и no unknown-effect replay.
- Общие request/argument/host-resource caps сохраняются. Удаление authored 8k cap не означает неограниченных allocations; byte/depth expansion guards остаются привязаны к действующей ресурсной границе.
- Без новых dependencies, service/store/schema migration/public IPC, второго root/Db, нового retry framework или переписывания task renewal/delivery целиком.
- Не изменять пользовательскую БД/config/env, не выполнять её tools, не kill/restart owner и не удалять locks. Только offline synthetic providers/fixtures; live/visual qualification вне scope.

## Change Envelope

- `dcp.rs`, `runtime.rs`, `tools.rs`, ближайшие DCP/range/renewal tests и `docs/DCP.md` для изменения контракта summary.
- `runtime_compaction.rs`, необходимый `runtime/context.rs` / `runtime/turn.rs` / `storage_compaction.rs` путь выбора и admission, существующие compaction tests.
- `native/server.rs`, `application/authority.rs` для атомарного выбора existing root / idle reservation, `native/client.rs` для affected compaction control identity; ближайшие server/client tests.
- `dcp_auto.rs` и ближайший reminder test для воспроизведённого gate blocker.
- Эта цель; без копии БД, приватных payloads/config, ciphertext и raw диагностических дампов в Git.
- Verified origin URL: `git@github.com:0FL01/oc.git`, ветка `master`, baseline HEAD `b32960c7650632ba4f04d3da9b20e358acd1d31c`; только обычный fast-forward push, без force/tag/release.

## Исполняемый план — копия утверждённого решения

1. **Compression и восстановление.** Убрать отдельный 8192-character cap из schema и validation, заменить ошибочную regression на принятие полезного summary >8192. Согласовать локальный 64 KiB summary/expansion guard с реальными ресурсными границами, сохранив bounded expansion. Выбирать старый префикс compaction по фактически отправляемой HOT-проекции, а не тексту messages или размеру RAW-архива. Проверять replacement вместе с хвостом; Completed/no_gain не равен устранению overflow. Начать с неверной границы, не переписывать renewal целиком.
2. **Управление.** Native `/compact` направлять в существующий root/worker, если сессия занята; новый root — только для idle operation. Не отменять shell/subagents и не вводить новый delivery/retry слой: контрольная доставка уже проходит. Сохранить конкретную безопасную диагностику реальных validation/resource отказов. Разрешить mismatch существующего reminder test с текущим контрактом, без ослабления cadence/threshold поведения.
3. **Доставка.** Итеративно relevant regression tests → scoped Clippy; затем один `bash scripts/test.sh`, после ошибки только failed/affected срез. Проверить diff, сохранить чужие изменения отдельно, обычный commit/push и locked release build. Исходный owner не перезапускается.

## Current Checkpoint

- Complete: R1–R5 resolved; дальнейшая работа вне текущей цели.

## Current State

- Resolved: R1/R2/R3/R4. Вызванный R2 регресс устранён и проверен.
- Last relevant evidence: notice tests 2/2, t47, исходный incident, native `/compact`, PTY test, compaction 29/29, context 15/15 PASS; scoped Clippy и remaining gate stages PASS; release собран из текущего worktree, включая сохранённый user diff. Первоначальный full gate не PASS. Код `e31cf17b72c7f8ed1b7169ae8d3638f3240bd364` отправлен на origin.
- Blocker: нет.
- Next: нет.

## Material Decisions

- 2026-10-27: текстовый summarizer не получает quoted ciphertext: переиспользуется существующая neutral projection для reasoning/compaction, public reasoning сохраняется. Native compaction и pricing получают исходную совместимую HOT-проекцию; source acquisition ограничен host cap, paid input — финальным summary frame.
- 2026-10-27: R3 требует сохранить compaction control после завершения исходного turn: joining разделяет root origin, а прежний client cleanup удалял все owned entries этого origin. Минимальный envelope включает existing client ownership state и regression; новый IPC/persistence слой не вводится.
- 2026-10-27: повышение expansion ceiling до общего request cap требует ограничить и сумму активных раскрытых blocks до materialization. Раскрытые consumed dependencies не учитываются повторно; существующий dcp_atomic resource test дополнен combined-expansion refusal.
- 2026-10-27: byte-overflow незакрытого notice не доказывает, что история сжимаема. Не запускать summary, если сам неизменяемый notice уже выше общего byte cap; сохранить прежнюю context diagnostic и не списывать такой случай на сетевой retry. Нулевой/отсутствующий model context не делает auto-compaction доступной в обход `required_for_budget`.

## Completion

- R1–R5 verified; отклонённый исходный full gate и успешный повтор только failed/affected slices указаны в R5, это не утверждение о full-gate PASS.
- Код: `e31cf17b72c7f8ed1b7169ae8d3638f3240bd364`, ordinary push `origin/master`. Release: `target/release/oc2`, собран с сохранённым пользовательским diff.
- Пользовательский diff не вошёл в кодовый commit; пользовательская БД, native owner, lock, schema и live-provider не изменялись.
