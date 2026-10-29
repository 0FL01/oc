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
  - Status: partial
  - Evidence: TUI regression проверил поток >16 KiB, delta между mouse down/up, anchor и freeze; held-worker regression проверил shared chunks. Offline PTY кликом раскрыл и прокрутил >16 KiB до хвоста при удерживаемой генерации. Невосстановимое вытеснение по `LIVE_PARTS_MAX` и ограничение `WINDOW_BYTES` ещё не устранены; при известной потере активный view показывает предупреждение.
- R2: Полный reasoning доступен внутри страниц Markdown и после завершения.
  - Source: пользовательский запрос и исправленный план пункта 2.
  - Acceptance: реальный viewport позволяет прокрутить строки за 512-row clipping и длинную не последнюю ячейку таблицы за 16 KiB; count/paint/scroll согласованы, Markdown и bounded cache сохранены. Вспомогательный bounded snapshot не выдаётся за полный интерактивный просмотр.
  - Primary evidence: ближайшие indexed viewport regressions для узкой длинной строки и длинной первой table cell.
  - Status: partial
  - Evidence: реальные viewport-регрессии для узкой строки, первой большой table cell, многоколоночной Unicode-геометрии и строк с `|` внутри/вне fence проходят; проверены также сохранение bold в помещающейся ячейке и 96 тестов `messages::tests::`. Очень большая ячейка с inline Markdown пока честно показывает `Markdown preview limited`: её полная пагинация с контекстом не реализована. Длинный заголовок таблицы и слишком узкий для сетки viewport также требуют решения; весь Markdown-контракт не закрыт.
- R3: Late attach и восстановление gap не теряют уже принятый live reasoning.
  - Source: пользовательский запрос, исправленный план пункта 3.
  - Acceptance: для текущего session/turn присоединение и gap восстанавливают принятые публичные части и новые дельты ровно один раз; checkpoint/tool insertion не сдвигает принятую границу, остальные классы событий и другие views не пропадают. При настоящем сбое или утрате owner неполное восстановление не выдаётся за полное.
  - Primary evidence: focused authority/TUI attach/gap regression >256 KiB с delta и checkpoint/tool insertion race.
  - Status: partial
  - Evidence: повторная независимая read-only проверка подтвердила, что snapshot, последующий history read и события не имеют общего cut; retry может checkpoint-ить середину item, а tool insertion сдвигает sequence. Нельзя безопасно устранить дефект увеличением snapshot или общим floor. TUI теперь явно предупреждает о известном усечении; восстановление и его regression отсутствуют.
- R4: Проверить, собрать release, закоммитить и отправить результат.
  - Source: последняя инструкция пользователя и исправленный план пунктов 4–5.
  - Acceptance: затронутая архитектурная документация актуальна; узкие проверки и scoped Clippy выполнены с честным результатом, full offline gate выполняется один раз при runtime/native lifecycle или wire changes; `target/release/oc2` собран; свои изменения закоммичены и отправлены обычным push в проверенный `origin` либо точный blocker записан без объявления завершения.
  - Primary evidence: test/build exits, diff review, commit IDs и push result.
  - Status: partial
  - Evidence: `cargo fmt --all -- --check`; `messages::tests::` 96 PASS; `app::projection::tests` 5 PASS; `selection` 44 PASS; bounded live-parts и long redacted live reasoning regressions PASS; offline PTY live click/scroll PASS; parked projection close PASS; `cargo clippy -p oc-tui -p oc2 --all-targets --locked -- -D warnings` PASS; `cargo build -p oc2 --bin oc2 --locked --release` PASS (`target/release/oc2`). Это проверки частичного checkpoint, не интеграционная приёмка; full offline gate и live/visual qualification не выполнялись.

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

- Closes: незавершённые части R2, затем R3 и R1.
- Smallest next action: заменить bounded fallback для воспроизведённой большой форматированной table cell на count/paint pagination с полным parsed Markdown context, сохранив согласованное выравнивание ячеек; проверить длинный header и слишком узкую таблицу. Затем согласовать выбранный live cut и безопасное вытеснение лишь при доступности canonical source.
- Expected evidence: `indexed_oversized_formatted_table_cell_reports_a_limited_preview` заменить проверкой полного содержимого/форматирования реального viewport; attach/gap и checkpoint/tool insertion regressions без потерь других классов событий.
- Stop or replan if: не удаётся доказать атомарное сочетание durable prefix, live cut и событий без подавления других классов либо без нарушения memory/IPC и exact-child fences.

## Current State

- Baseline: `0578b96b53b60a95606ec394887aed29c48d7735`, рабочее дерево было чистым.
- Resolved: цель и план зафиксированы до runtime edits; прямой live >16 KiB и конкретные потери Markdown исправлены, с фокусными regressions и offline PTY. Предупреждение честно сообщает об известной потере live output.
- Last relevant evidence: focused 96 Markdown, 5 projection, 44 selection, 1 live-parts, 1 long-redacted reasoning, 1 parked worker и 1 offline PTY tests PASS; scoped Clippy, fmt и release build PASS. Full offline gate, live/visual qualification и безопасное восстановление не проверялись.
- Unresolved: форматированные oversized table cells пока показывают preview; безопасная граница R3 не определена. Это не доказательство полного исправления. Решение recovery должно использовать producer-known stable prefix после tool outcomes и durable availability, адресный bounded cut и идентичности/offsets, не продвигать общий floor по одному reasoning и не терять остальные события; точные root/child/location fences сохраняются.

## Material Decisions

- 2026-10-29: исправленный после аудита план заменяет исходное небезопасное предложение устанавливать checkpoint/floor сразу после сохранения генерации.

## Checkpoint History

- 2026-10-29: заморожены результаты и скопирован исправленный план до реализации.
- 2026-10-29: прямое раскрытие live reasoning и конкретные viewport-потери исправлены и частично проверены; R1/R2 остаются partial до связанных проверок и устранения невосстановимого eviction.
- 2026-10-29: подтверждены и исправлены регрессии выравнивания и потери pipe-строк; oversized форматированная ячейка и согласование recovery остаются незавершёнными. Текущий ограниченный срез проверен offline; явное предупреждение не заменяет recovery.

## Completion

- Не завершено.
