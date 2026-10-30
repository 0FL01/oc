# Goal: рефакторинг SQLite-хранилища под нагрузку

Status: active
Source: пользовательский запрос «фундаментально расхуярить» дефект хранилища, RECON 2026-10-29 (замеры на синтетике 10k child jobs, копии живой БД и fsync-бенче), согласованный план «рефакторинг SQLite, без Turso»; инструкция «делай копию плана в цель и итеративно реализовать и коммит пуш билд после каждой итерации».
Last updated: 2026-10-29

## Objective

Хранилище owner'а выдерживает 10k накопленных child jobs и 8–16 параллельных сессий: async runtime не блокируется SQLite, надёжные подтверждения не деградируют, объём записи на turn растёт линейно, а не квадратично.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop substantive work at a proven external blocker or when no remaining in-scope action has a falsifiable expected result; record the exact evidence and smallest unlock. После каждой итерации: релевантные проверки, commit, push, `cargo build -p oc2 --bin oc2 --locked --release`.

## RECON: измеренные узкие места

1. `synchronous=FULL` на btrfs: ~9 мс на любой коммит; все коммиты идут через один `Arc<Mutex<Connection>>`, потолок ~100 коммитов/с на весь owner.
2. ~500 синхронных вызовов `Db` прямо в async-коде на 2 tokio-воркерах; один медленный запрос замораживает owner. Location worker обслуживает запросы последовательно.
3. Одно соединение: чтения ждут записи и fsync, хотя WAL допускает параллельных читателей.
4. `turns.result` (среднее 343 КиБ, максимум 3.1 МиБ; 182 МиБ на 545 строк) переписывается целиком на каждый tool intent/outcome; 70% объёма — append-only `input` и `opaque`.
5. Коррелированные `EXISTS` по `events(kind,payload)` без индекса: `child_job_outstanding`/`deliver_child_notices` 40.8 с на 10k jobs (квадратично), `child_notice_in` 17 мс на строку истории.
6. `fsync` файла tool output под мьютексом БД (`storage_tool_output.rs` `publish_extent`).
7. Глобальные счётчики: `messages.seq = MAX(seq)+1`, `events AUTOINCREMENT`, однострочная `conversation_revision`.
8. Схема собирается лениво 31 функцией `*_schema`, есть ad-hoc `ALTER`, legacy-таблицы, `cache_size` 512 КиБ.

## Frozen Contract

### Required Outcomes

- R1: Синтетика и страховочные тесты (Фаза 0).
  - Acceptance: в gate детерминированные тесты масштабирования реальных функций `Db` по VM-шагам SQLite (работа операции не растёт с числом накопленных jobs/событий), равенство схемы свежей и мигрированной БД, fault-тесты `kill -9` между intent/effect/outcome без переисполнения неизвестного эффекта; opt-in метрики хранилища (ожидание/удержание мьютекса, коммиты, fsync) и opt-in синтетика 1k/10k child jobs и 1/4/8/16 сессий; инвентарь durable-точек.
  - Primary evidence: тесты и opt-in печать базовой линии.
  - Status: partial
  - Evidence: общий тестовый счётчик `storage::vm_steps` (rusqlite feature `trace` только в dev; `OC_VM_STEPS_DEBUG=1` печатает самые дорогие операторы) проверяет реальные функции `Db`: child jobs (150 → 600 накопленных jobs, 20 событий родителя на job) и чтения истории (50 → 200 turn'ов по три tool-операции). Открыты: равенство свежей и мигрированной схемы целиком, fault-тесты, opt-in метрики и синтетика сессий, инвентарь durable-точек.
- R2: Точечные исправления масштабирования (Фаза 1).
  - Acceptance: частичные индексы для всех `events(kind,payload)`-поисков из горячих путей; `child_job_outstanding`, `deliver_child_notices`, `background_child_job`, `child_history` не зависят от числа исторических jobs/событий; fsync файла вне мьютекса БД; `cache_size` выбран по замеру.
  - Primary evidence: тесты R1 до/после.
  - Status: partial
  - Evidence: путь child jobs (итерация 1), чтения истории, пикер и активность вкладок (итерация 2), fsync файлового хранилища вне мьютекса БД (итерация 3) закрыты. Открыты прочие `events(kind,payload)`-поиски вне измеренных путей (например `tool_output_question` на строках вопросов, поиск ресурса по `json_extract(descriptor,'$.path')` в `open_tool_output`) и `cache_size`.
- R3: Хранилище как компонент вне async runtime (Фаза 2).
  - Acceptance: выделенные OS-потоки: один писатель с групповым коммитом (SAVEPOINT на операцию, один fsync на пачку, подтверждение только после fsync своей пачки, `synchronous=FULL` сохраняется) и пул читателей WAL; async-код не держит мьютекс БД и не выполняет fsync; кэши на `total_changes` переведены на счётчик коммитов писателя; одна версионированная цепочка миграций при открытии.
  - Primary evidence: метрики R1 (ожидание на async-воркерах ≈0), synthetic 8–16 сессий, fault-тесты.
  - Status: partial
  - Evidence: групповой коммит с fsync вне мьютекса реализован для всех существующих мест `Db` без их переписывания (итерация 4). Открыты: авто-checkpoint SQLite ещё выполняет fsync под мьютексом примерно раз в 1000 кадров; ожидание fsync всё ещё идёт на вызывающем потоке, в том числе на tokio-воркерах; пул читателей; единая цепочка миграций.
- R4: Модель данных без квадратичной записи (Фаза 3).
  - Acceptance: TurnLog append-only (`input`, `opaque`, `spans`, `display_parts` строками), чтения History без `json_extract` по мегабайтным blob'ам, типизированные колонки вместо JSON в горячих предикатах, посессионные последовательности там, где позволяет контракт; миграция существующих данных проверена на копии реальной БД; legacy-таблицы удалены только подтверждённо мёртвые.
  - Primary evidence: turn на 100 tool calls — линейный объём записи; апгрейд копии реальной БД.
  - Status: open
- R5: Чтения не встают в последовательную очередь Location worker (Фаза 4).
  - Acceptance: History/Workspace/ChildJobs обслуживаются пулом читателей параллельно.
  - Status: open
- R6: Итерационная поставка.
  - Acceptance: после каждой итерации релевантные тесты и scoped Clippy, commit и push в проверенный `origin`, release build; full offline gate перед интеграционной приёмкой фаз R3–R5; документация контракта обновлена.
  - Status: open

### Критерии приёмки целиком

- async-воркеры не ждут fsync/мьютекс БД;
- 8–16 сессий: p99 надёжного подтверждения не больше одной пачки fsync, пропускная способность записи кратно выше ~100 tps;
- 10k child jobs: запуск p95 < 50 мс, страница History < 200 мс;
- turn на 100 tool calls: объём записи линейный;
- `kill -9` в любой точке: ноль переисполненных неизвестных эффектов, история не теряется;
- gate и `perf_tui_pty` без регрессий.

### Constraints

- Один native UID/data-root authority и один Db/recovery owner; клиенты не открывают БД. SQLite остаётся движком; Turso вне scope.
- Durable intent до побочного эффекта и outcome после него сохраняются; ослабленный `synchronous=NORMAL` для подтверждаемых записей не вводится.
- Каждая миграция схемы явно отмечается, проверяется на апгрейде и на копии реальной БД; не изменять и не останавливать работающий пользовательский owner, не удалять его lock.
- Не ослаблять тесты, deadlines и assertions ради зелёного; тест >30s требует диагностики (TEST_PLAN).

## Change Envelope и копия плана

1. **Фаза 0.** Opt-in метрики хранилища; синтетика расширяет `perf_long_session`/`memory_soak`; тесты в gate: масштабирование по VM-шагам на реальных функциях `Db`, равенство схемы, fault-тесты; инвентарь durable-точек.
2. **Фаза 1.** Частичные индексы `events(payload) WHERE kind=…`; `child_job_outstanding`/`deliver_child_notices` без перебора исторических foreground jobs; fsync файла до короткой транзакции публикации; `cache_size` по замеру.
3. **Фаза 2.** Модуль `store`: писатель + пул читателей на OS-потоках, async API через каналы и блокирующий вариант для синхронных мест; групповой коммит; перевод вызовов по подсистемам (children → shell/execute → runtime/turn → application → DCP), в конце `conn.lock()` вне `store` нет; счётчик коммитов вместо `total_changes`; единая цепочка миграций.
4. **Фаза 3.** `turn_items` вместо роста `turns.result`; History по строкам и индексам; типизированные колонки; посессионные последовательности; решение по триггерному версионированию DCP по метрикам; удаление подтверждённо мёртвых legacy-таблиц.
5. **Фаза 4.** Чтения через пул читателей вне последовательной очереди Location worker.

Allowed: `oc-adapters` (storage, runtime, application, native), `oc-core` при необходимости типов, `oc2` для метрик/синтетики, ближайшие tests, эта цель, `GOAL.md`, затронутая архитектурная документация.

## Iterations

### 1. Child jobs без роста от истории (R1 partial, R2 partial)

- Миграция схемы **v16 `child-job-delivery-mode`**: колонка `child_jobs.background` — эффективный режим (launch identity или конверсия), бэкфилл по прежнему предикату; индексы `child_job_state`, частичный `child_job_undelivered`, частичные `events_subagent_background`, `events_subagent_notice` и expression-индекс `events_subagent_launch` по `(session_id, operation)`. Режим пишется при допуске и при конверсии.
- `child_job_outstanding`, проверка лимита при допуске, `deliver_child_notices`, `child_tool_jobs`, `child_tool_job_in`, `child_history`, `background_child_job` идут по индексам; планы закреплены `INDEXED BY` и снятием affinity (`+column`) там, где планировщик без статистики выбирал общий индекс.
- VM-шаги при 150 → 600 накопленных jobs: outstanding 1.24M → 19.8M стало 62 → 62; deliver 1.24M → 19.8M стало 21 → 21; карточки 342k → 1.37M стало 1986 → 1986; notice-строка истории 1.8k → 7.0k стало 122 → 122; конверсия 16.7k → 66.2k стало 273 → 273.
- Проверки: `storage::children` 24 PASS (включая апгрейд со старой формы и повторное открытие), `child_schema` 3 PASS, `oc-adapters --test subagent` 45 PASS, `oc-adapters --lib` до правки ожиданий миграций 1023 PASS; Clippy `oc-adapters --all-targets -D warnings` PASS. Копия реальной БД: v16 применилась при открытии, 206 jobs, 0 расхождений режима со старым предикатом, `integrity_check` ok, старые и новые запросы outstanding/доставки совпадают.

### 2. Чтения истории, пикер и активность без роста от длины сессии (R1 partial, R2 partial)

- Тест `storage::history_scale_tests` наполняет сессию настоящими accept/tool/checkpoint/commit путями и меряет последнюю страницу истории, presentation последнего turn'а, построчные `child_history`/`user_shell_result`/`model_shell_notice`/`user_images`, `session_list` и `session_activity`. До правки росли: `child_history` 1 520 → 5 270 VM-шагов (поиск `fork_native_notice` по всем событиям сессии на каждой строке истории), `session_list` 342 → 792 и `session_activity` 227 → 677 (проход по индексу всех turn'ов БД или автоматический индекс на каждый вызов).
- Новые индексы при открытии (без версии схемы, как существующий hot-path блок): частичный `turns_started` и `events_fork_native_notice`. Проверка «работает ли семейство» идёт вверх от работающих turn'ов и активных child jobs, а не вниз по всем потомкам. После правки все восемь операций константны (`child_history` 265, `session_list` 174, `session_activity` 66 шагов при 50 и 200 turn'ах).
- Семантика: новый тест `busy_family_follows_running_turns_and_active_child_jobs_upward` (внук с работающим turn'ом, активный child job, посторонний корень). На копии реальной БД (252 сессии, три последних turn'а переведены в `started`) старый и новый probe совпали для всех сессий.
- Проверки: `oc-adapters --lib` (nextest) 1027 PASS; `oc2 --bin` 183 PASS; `pty_t42` 48 PASS; Clippy `oc-adapters --all-targets -D warnings` PASS.

### 3. fsync файлового хранилища вне мьютекса БД (R2 partial)

- Tool output (начало, каждая публикация append'а, завершение с rename, заморозка префикса, expiry) и blob'ы (запись, GC) выполняли fsync файлов и каталогов под мьютексом соединения: стриминг shell-вывода одной сессии задерживал любой SQL остальных.
- Новый `file_store` замок упорядочивает квоту, публикацию, rename, GC и expiry; порядок захвата `file_store` → `conn` → `output_readers`, соединение берётся только на SQL. Все fsync идут через `Db::sync_durable`, который в тестах считает fsync при занятом мьютексе БД.
- Expiry помечает строку `Expired` атомарно с допуском читателей (под `conn`), а unlink и fsync каталога выполняет после; файл `Expired`-строки, оставшийся после падения между ними, собирает скан сирот.
- Проверки: новые `file_store_fsyncs_run_outside_the_database_mutex` (с временно возвращённым удержанием мьютекса в `publish_extent` фиксирует 4 нарушения) и `expired_row_file_left_by_a_crash_is_collected_as_an_orphan`; `tool_output` 16 PASS; `oc-adapters` целиком (nextest, unit и integration) 1330 PASS, 5 skipped; `pty_t39` 68 PASS и `terminals` 1 PASS (в первом общем прогоне один раз упал `vis15_real_pty_adjacent_reasoning_parts_group_and_replay_after_restart` по таймауту экрана TUI, в одиночку и при повторе всего набора PASS — не связан с файловым хранилищем); Clippy `oc-adapters --all-targets -D warnings` PASS.

### 4. Групповой коммит с fsync вне мьютекса соединения (R3 partial)

- `storage::writer::Conn` заменяет `Mutex<Connection>`: соединение коммитит в WAL с `synchronous=NORMAL`; освобождение соединения держателем, который коммитил, ждёт уже вне мьютекса `fdatasync` WAL-файла, покрывающего его завершённые коммиты. Один fsync покрывает все завершённые до него коммиты; чтения не ждут fsync. Гарантия подтверждения — как у `synchronous=FULL` в WAL. Все 358 существующих мест `conn.lock()` получили это без переписывания.
- Независимый аудит (Plan-агент) нашёл дефекты первой версии; исправлено: (1) commit hook SQLite срабатывает до записи кадров, поэтому водяной знак fsync теперь — коммиты, чей держатель освободил соединение, а не срабатывания хука; (2) сбой fsync защёлкивается — после него ничего не подтверждается (Linux errseq сообщает ошибку writeback один раз); (3) при раскрутке паники ожидание fsync не пропускается, сбой в этот момент — `abort`; (4) удаление файла по состоянию БД (сканер сирот tool output, GC blob'ов) сначала делает все завершённые коммиты durable (`fence`). Прерывание shell по прочитанному состоянию признано безопасным: процессы не переживают потерю питания. Ограничения: авто-checkpoint SQLite (~1000 кадров) и restart WAL всё ещё синхронизируют под мьютексом; на файловых системах, сообщающих ошибку writeback только записывавшему дескриптору (например NFS), отдельный fd WAL может её не увидеть — поддерживаемая платформа локальная.
- Проба `commit_probe` (opt-in, btrfs): 1 поток ~158 надёжных записей/с (без изменений), 8 потоков 610/с против ~155 до правки, 242 fsync на 800 записей.
- Тесты `storage::writer`: объединение (8 записей — 1 fsync), чтения не ждут идущий fsync, fsync покрывает только освобождённые коммиты (держатель делает свой fsync), защёлка сбоя; 30 повторов без падений. Популяции тестов масштабирования отключают fsync тестовым переключателем (иначе >60 с).
- Проверки: полный offline gate `scripts/test.sh` 2528 PASS, 19 skipped, `EXIT 0` (773 с против 893 с до правки; единственный SLOW — тест масштабирования до тестового переключателя, затем 3.4 с); `storage::` 182 PASS; Clippy `oc-adapters --all-targets -D warnings` PASS. `docs/ARCHITECTURE.md` обновлён.
