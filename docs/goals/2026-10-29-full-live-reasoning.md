# Goal: полный раскрытый live reasoning

Status: active
Source: пользовательский запрос убрать `thinking truncated` и показывать весь раскрытый CoT во время работы; исправленный после RECON и независимого аудита план; инструкция «Делай копию плана в цель и итеративно реализовать и коммит пуш билд».
Last updated: 2026-10-29

## Objective

Пользователь может развернуть и прокрутить весь публичный reasoning, реально принятый приложением, без silent preview-усечения во время генерации и после её окончания, в том числе при штатном восстановлении активного view.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: Полный раскрытый живой reasoning доступен интерактивно без 16-КиБ preview.
  - Source: пользовательский запрос, исправленный план пункта 1.
  - Acceptance: во время потока раскрываются и прокручиваются начало, середина и хвост уже принятого публичного reasoning; новые дельты не блокируют безопасный клик, scroll, selection или freeze. Большой источник не копируется целиком на UI при каждой дельте; один bounded/cancellable worker на view.
  - Primary evidence: focused TUI regression для live >16 KiB и взаимодействия во время delta; существующий held-worker тест для ownership/cancellation; один реальный offline PTY-сценарий клика и scroll.
  - Status: pending
  - Evidence: RECON и аудит только по исходникам; проверки не запускались.
- R2: Полный reasoning доступен внутри страниц Markdown и после завершения.
  - Source: пользовательский запрос и исправленный план пункта 2.
  - Acceptance: реальный viewport позволяет прокрутить строки за 512-row clipping и длинную не последнюю ячейку таблицы за 16 KiB; count/paint/scroll согласованы, Markdown и bounded cache сохранены. Вспомогательный bounded snapshot не выдаётся за полный интерактивный просмотр.
  - Primary evidence: ближайшие indexed viewport regressions для узкой длинной строки и длинной первой table cell.
  - Status: pending
  - Evidence: конкретные пути потерь определены статическим аудитом, проверки не запускались.
- R3: Late attach и восстановление gap не теряют уже принятый live reasoning.
  - Source: пользовательский запрос, исправленный план пункта 3.
  - Acceptance: для текущего session/turn присоединение и gap восстанавливают принятые публичные части и новые дельты ровно один раз; checkpoint/tool insertion не сдвигает принятую границу, остальные классы событий и другие views не пропадают. При настоящем сбое или утрате owner неполное восстановление не выдаётся за полное.
  - Primary evidence: focused authority/TUI attach/gap regression >256 KiB с delta и checkpoint/tool insertion race.
  - Status: pending
  - Evidence: static audit; проверки не запускались.
- R4: Проверить, собрать release, закоммитить и отправить результат.
  - Source: последняя инструкция пользователя и исправленный план пунктов 4–5.
  - Acceptance: затронутая архитектурная документация актуальна; узкие проверки и scoped Clippy выполнены с честным результатом, full offline gate выполняется один раз при runtime/native lifecycle или wire changes; `target/release/oc2` собран; свои изменения закоммичены и отправлены обычным push в проверенный `origin` либо точный blocker записан без объявления завершения.
  - Primary evidence: test/build exits, diff review, commit IDs и push result.
  - Status: pending
  - Evidence: ещё не выполнялось.

### Constraints и границы

- «Весь CoT» означает весь публичный `ReasoningDelta`, полученный и принятый приложением после действующей redaction; скрытый CoT, opaque/encrypted content и signatures не раскрываются. Terminal-only summary без дельт — отдельный механизм, вне scope.
- Сохранять provider/SSE/IPC resource guards и явные ошибки, существующие permissions, RAW/HOT, no unknown-effect replay, terminal sanitization и bounded cache. Не объявлять ранее потерянные данные восстановленными.
- Не удалять общий `WINDOW_BYTES` вслепую: устранить только подтверждённую потерю принятого reasoning. Не снимать tool/output и обычные text previews или неограниченно материализовать вспомогательный `transcript() -> Vec<Line>`.
- Один native UID/data-root authority и существующий Db/recovery owner; не добавлять новую БД, schema/migration, service, dependency, event journal или public/generic API. Не изменять и не останавливать работающий пользовательский owner, не удалять lock.
- Не расширять workspace snapshots для всех вкладок; использовать адресное восстановление одного активного turn, bounded staging и честные неполные ошибки. Не повторять provider/tools и не сопоставлять части по содержимому.

## Change Envelope и копия исправленного плана

1. **Live TUI:** в `oc-tui/src/app/{live,transcript,projection}.rs` и ближайших владельцах убрать 16-КиБ `reasoning_preview` для раскрытого интерактивного источника; сохранять полный источник через immutable shared prefix/chunks или эквивалент без полного COW на UI при каждом append. Учитывать полный размер при выборе фоновой projection. Сохранить один worker/view, coalescing, cancellation и корректность freeze. Клик разрешать лишь по действительно нарисованному заголовку при сохранении identity/group/view/width/frame/scroll/selection и других structural fences; anchor и selection/copy используют согласованный снимок. Не вытеснять ещё не восстановимые parts только по `LIVE_PARTS_MAX`.
2. **Markdown:** в `oc-tui/src/messages.rs` и ближайших tests исправить доказанные потери внутри indexed page: узкая длинная строка за 512 визуальных строк и длинная непоследняя table cell за 16 KiB. Измеренная высота, scroll seek, paint, Markdown context и cache должны согласовываться; сохранять UTF-8 и безопасность терминала. Не предполагать заранее большой rewrite или неограниченный вспомогательный snapshot.
3. **Восстановление:** у `oc-adapters/src/application/authority/live.rs` и runtime/consumer owners сохранить упорядоченный публичный источник ещё не покрытого стабильной history ответа и согласовать stable durable prefix, live source и новые revisioned events. Граница не стабильна сразу после `checkpoint_turn_usage`: tool parts могут вставляться внутрь и сдвигать sequence, DB и publication revisions различны. Общий `projection_floor` нельзя продвигать по снимку только reasoning, так как он подавляет также tool, retry, usage и terminal. Сохранить session/turn/location и exact child launch fences, один coalesced repair на view с bounded staging; не полагаться на бесконечный restart при overflow. Проверить адресный `Workspace(selected)` без нарушения публичного контракта/раздувания deck; если невозможно, только узкий private read-only запрос на конкретный активный turn. Native wire VERSION 8→9 лишь при несовместимом изменении.
4. **Документация:** заменить утверждение о 16-КиБ live reasoning preview в `docs/ARCHITECTURE.md` только после фактического изменения контракта.
5. **Проверки:** focused regressions R1–R3 и PTY; затем scoped Clippy. Если изменён общий native wire/lifecycle, один full offline `bash scripts/test.sh` перед интеграционной приёмкой. Сборка `cargo build -p oc2 --bin oc2 --locked --release`, обычные commits/push в проверенный origin; не переопределять Cargo target и не запускать пользовательский клиент/owner.

Allowed: необходимые runtime edits и ближайшие tests в `oc-tui`, `oc-adapters`, `oc-core`, `oc2`, эта цель, `GOAL.md`, затронутый архитектурный контракт. Один mutation/Cargo owner, read-only разведка; без платных/live provider вызовов.

## Current Checkpoint

- Closes: R1.
- Smallest next action: устранить 16-КиБ preview открытого reasoning с bounded projection ownership; проверить достижимое интерактивное поведение и held-worker.
- Expected evidence: focused TUI regressions.
- Stop or replan if: невозможно совместить непрерывные дельты с безопасным click/selection и bounded worker в существующей архитектуре.

## Current State

- Baseline: `0578b96b53b60a95606ec394887aed29c48d7735`, рабочее дерево было чистым.
- Resolved: цель и план зафиксированы до runtime edits.
- Last relevant evidence: read-only RECON и четырёхчастный независимый аудит; тесты/продукт не запускались.
- Blocker: нет. Next: R1.

## Material Decisions

- 2026-10-29: исправленный после аудита план заменяет исходное небезопасное предложение устанавливать checkpoint/floor сразу после сохранения генерации.

## Checkpoint History

- 2026-10-29: заморожены результаты и скопирован исправленный план до реализации.

## Completion

- Не завершено.
