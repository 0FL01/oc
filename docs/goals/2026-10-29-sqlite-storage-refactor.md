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
  - Status: done
  - Evidence: общий тестовый счётчик `storage::vm_steps` (rusqlite feature `trace` только в dev; `OC_VM_STEPS_DEBUG=1` печатает самые дорогие операторы) проверяет реальные функции `Db`: child jobs (150 → 600 накопленных jobs, 20 событий родителя на job) и чтения истории (50 → 200 turn'ов по три tool-операции). Opt-in метрики и синтетика (итерация 10); равенство схемы целиком, fault-тесты `SIGKILL` и инвентарь durable-точек (итерация 11).
- R2: Точечные исправления масштабирования (Фаза 1).
  - Acceptance: частичные индексы для всех `events(kind,payload)`-поисков из горячих путей; `child_job_outstanding`, `deliver_child_notices`, `background_child_job`, `child_history` не зависят от числа исторических jobs/событий; fsync файла вне мьютекса БД; `cache_size` выбран по замеру.
  - Primary evidence: тесты R1 до/после.
  - Status: done
  - Evidence: путь child jobs (итерация 1), чтения истории, пикер и активность вкладок (итерация 2), fsync файлового хранилища вне мьютекса БД (итерация 3) закрыты; поиск результата вопроса в страницах tool-операций, expiry и открытие tool output, проверки живых shell/Code Mode jobs (итерация 7) закрыты. Поиск потреблённого parent notice и `cache_size` по замеру (итерация 9) закрыты; оставшиеся сканы ограничены не историей (см. итерацию 9).
- R3: Хранилище как компонент вне async runtime (Фаза 2).
  - Acceptance: выделенные OS-потоки: один писатель с групповым коммитом (SAVEPOINT на операцию, один fsync на пачку, подтверждение только после fsync своей пачки, `synchronous=FULL` сохраняется) и пул читателей WAL; async-код не держит мьютекс БД и не выполняет fsync; кэши на `total_changes` переведены на счётчик коммитов писателя; одна версионированная цепочка миграций при открытии.
  - Primary evidence: метрики R1 (ожидание на async-воркерах ≈0), synthetic 8–16 сессий, fault-тесты.
  - Status: partial
  - Evidence: групповой коммит с fsync вне мьютекса реализован для всех существующих мест `Db` без их переписывания (итерация 4). Авто-checkpoint SQLite (fsync под мьютексом примерно раз в 1000 кадров) сознательно оставлен: вынос его на отдельное соединение замедлил запись до 2× (итерация 5, откат). Ожидание занятого соединения и fsync отдаёт tokio-воркер через `block_in_place` (итерация 6). Пул читателей WAL для пяти чистых display-чтений (итерация 8). Открыты: SQL выполняется на вызывающем потоке; перевод остальных чтений (с file lease — только с отдельной синхронизацией лизов); единая цепочка миграций.
- R4: Модель данных без квадратичной записи (Фаза 3).
  - Acceptance: TurnLog append-only (`input`, `opaque`, `spans`, `display_parts` строками), чтения History без `json_extract` по мегабайтным blob'ам, типизированные колонки вместо JSON в горячих предикатах, посессионные последовательности там, где позволяет контракт; миграция существующих данных проверена на копии реальной БД; legacy-таблицы удалены только подтверждённо мёртвые.
  - Primary evidence: turn на 100 tool calls — линейный объём записи; апгрейд копии реальной БД.
  - Status: open
- R5: Чтения не встают в последовательную очередь Location worker (Фаза 4).
  - Acceptance: History/Workspace/ChildJobs обслуживаются пулом читателей параллельно.
  - Status: open
  - Evidence: страницы истории/транскрипта, `child_history`, `child_tool_jobs` и `session_activity` уже не ждут писателя (итерация 8), но Workspace (tab deck, `session_root`, inbox) и `turn_presentation`/`list_tool_ops_page` по-прежнему идут через писателя и очередь Location worker.
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

### 5. Вынос checkpoint на отдельное соединение — измерен и отклонён (R3)

- Попытка: `wal_autocheckpoint=0` на писателе, `PRAGMA wal_checkpoint(PASSIVE)` на отдельном соединении после fsync при WAL выше 4 МиБ, `journal_size_limit` для обрезки рестартованного WAL. Без обрезки длина WAL-файла не уменьшается и checkpoint запускался после каждого fsync (тест `latest_windows_…` >60 с); с обрезкой тесты зелёные (`oc-adapters` 1335 PASS без SLOW), но проба `commit_probe` упала: 1 поток 105–108 записей/с против 156–158, 8 потоков 305–327 против 610–636. Бисекция: без `journal_size_limit` — 157 и 454, с авто-checkpoint SQLite — 156 и 636. Регулярный checkpoint SQLite позволяет переписывать уже выделенный WAL вместо дописывания, и `fdatasync` на btrfs обходится дешевле; выигрыш выноса (редкий fsync под мьютексом) не измерен.
- Решение: откат к итерации 4, авто-checkpoint SQLite остаётся. Коммитов кода нет.

### 6. Ожидание БД не останавливает рабочие потоки tokio (R3 partial)

- Ожидание занятого мьютекса соединения и ожидание ещё не durable fsync в `storage::writer` идут через `tokio::task::block_in_place` на multi-thread runtime: рабочий поток передаёт свою очередь задач другому, и прочие async-задачи (UDS-клиенты, стриминг) продолжают выполняться, пока один держатель ждёт БД. Незанятое соединение и уже durable коммит handoff не делают; вне runtime и на current-thread runtime ожидание блокирует как раньше. SQL по-прежнему выполняется на вызывающем потоке — вынос на пул читателей остаётся в R3.
- Регрессия `waiting_for_the_connection_does_not_stall_the_runtime`: без handoff падает («the worker stalled behind the connection» за 10 с), с ним PASS.
- Бисекция показала, что handoff в `Guard::drop` меняет планирование и вскрыл существовавшую гонку фикстуры `approval_binary`: автоматический запрос заголовка сессии уходил раньше основного и потреблял ответ, предназначенный модели. Фикстура теперь узнаёт запрос заголовка (`max_output_tokens == 256`, без tools) и отвечает на него отдельно, не расходуя основной сценарий; 3 прогона подряд PASS.
- Проверки: полный offline gate `scripts/test.sh` 2529 PASS, 19 skipped, `EXIT 0` (785 с); `storage::writer` 20 повторов без падений; `commit_probe` 1 поток 157/с, 8 потоков 616/с (254 fsync на 800 записей) — без регрессии; Clippy `oc-adapters oc2 --all-targets -D warnings` PASS.

### 7. Tool output, вопросы и живые jobs без роста от накопленных операций (R2 partial)

- Новый тест `tool_output_and_job_probes_do_not_grow_with_accumulated_outputs` (100 → 400 tool output с результатом вопроса и столько же завершённых shell/Code Mode jobs) до правки фиксировал рост всех шести путей: `begin_tool_output` 4718 → 10118 VM-шагов (expiry сканировал все ресурсы), `open_tool_output` 2867 → 11267 (поиск по `json_extract(descriptor,'$.path')`), `has_active_shell_jobs`/`has_active_execute_jobs`/`live_execute_call_ids` 514 → 2014, `list_tool_ops_page` 90898 → 368398 (на каждую строку question — скан событий сессии). После правки все шесть постоянны (`list_tool_ops_page` 9646 → 9646).
- Индексы (без новой версии схемы, `CREATE INDEX IF NOT EXISTS` при открытии): `events_tool_question` по guarded-выражению операции, как `events_tool_presentation`; частичный `tool_output_expiry(completed)` для expiry; частичные `shell_jobs_live`/`execute_jobs_live(session_id) WHERE phase!='terminal'`. Три подзапроса вопроса используют то же guarded-выражение и `+id` (без TEXT affinity). `open_tool_output` ищет по уникальному имени файла из пути; полный путь, descriptor и identity файла проверяются, как раньше.
- Популяции тестов: `Db::sync_durable` тоже соблюдает тестовый `skip_sync` (тест 29 с → 4.5 с).
- Проверки: на копии живой БД (35662 событий, 2132 ресурса, 1784 shell jobs) четыре индекса строятся за 0.26 с, после `ANALYZE` все пять запросов идут по новым индексам; `oc-adapters` целиком (nextest) 1336 PASS, 6 skipped; тесты масштабирования 5 PASS; Clippy `oc-adapters --all-targets -D warnings` PASS.

### 8. Пул читателей WAL для display-чтений (R3 partial, R5 partial)

- `storage::readers`: до четырёх лениво открываемых соединений (те же `cache_size`, кэш statements и `oc_session_lower`, плюс `query_only`); каждое чтение — одна WAL read transaction, то есть один снимок закоммиченного состояния. Ожидание свободного соединения отдаёт tokio-воркер тем же `blocking`, что и писатель; соединение возвращается в пул и при панике. Соединения закрываются раньше снятия flock (порядок полей `Db`).
- На пул переведены только чистые чтения без file lease и без решений о необратимых действиях: `read_history_page_typed`, `read_transcript_rows`, `child_history`, `child_tool_jobs`, `session_activity`. Их `_in(&tx)`-помощники внутри write transaction (`session_viewed` и др.) остаются на писателе.
- Сознательно оставлены на писателе (аудит Plan-агента): `turn_presentation` и `list_tool_ops_page` — `question_presentation_in` берёт lease на файл tool output, атомарный с expiry под соединением писателя; `session_list` и `session_root` — их результат выбирает Location и очереди approval/question; GC/expiry — читатель может увидеть коммит, ещё не опубликованный писателем, и `fence` его не покрывает; memo `presentation_wire_logs` на `total_changes()` писателя.
- Проверки: новые `reads_do_not_wait_behind_the_writer` (писатель держит соединение с незакоммиченным UPDATE: чтение завершается и видит старое состояние, после COMMIT новое чтение видит новое; с чтением через писателя падает по таймауту) и `a_read_waits_for_a_free_connection`; тесты масштабирования считают VM-шаги и на соединениях пула, `assert_flat` требует ненулевой счёт; полный offline gate `scripts/test.sh` 2532 PASS, 19 skipped, `EXIT 0` (836 с); Clippy `oc-adapters oc2 --all-targets -D warnings` PASS. `docs/ARCHITECTURE.md` обновлён.

### 9. Потреблённые parent notices и `cache_size` (R2 done)

- Тест масштабирования child jobs теперь измеряет и `parent_notice_state`/`is_native_notice`: до правки `parent_notice_state` 29782 → 118882 VM-шагов (150 → 600 jobs; `parent_notice_assigned_in` сканировал все события родителя), после — 54 → 54. Индекс `events_parent_notice_consumed` по guarded-выражению `$.message` для трёх kind'ов потребления; запрос использует то же выражение (`NULL` вместо `0` для невалидного JSON — то же «не найдено»).
- Проверены и оставлены: `native_execute_notice_in` идёт от `messages` по первичному ключу и уникальному `execute_jobs.message_id` (`is_native_notice` 109 → 109); legacy-ветка `block_positions` ограничена числом DCP-блоков сессии (в живой БД 16 блоков, все standalone, `compression_members` пуст); `turn_notice_order_in` ограничен сегментами одного turn'а.
- `cache_size`: на копии живой БД 30 повторов чтения журналов самой тяжёлой сессии (10 МиБ `turns.result`) и страницы истории — 0.66 / 0.66 / 0.68 / 0.65 с при 512 КиБ / 2 / 8 / 32 МиБ; время уходит на разбор JSON, страницы отдаёт кэш ОС. Оставлено 512 КиБ на соединение (ограничение памяти пяти соединений).
- Проверки: nextest `oc-adapters` (storage/children/notice/subagent) 237 PASS; Clippy `oc-adapters --all-targets -D warnings` PASS.

### 10. Метрики, синтетика нагрузки и checkpoint с handoff (R1 partial, R3 partial)

- `storage::writer` ведёт всегда включённые счётчики (захваты и ожидания соединения, удержания всего и на потоках tokio runtime, коммиты, ожидания durability, fsync, checkpoint'ы, ожидания без handoff на current-thread runtime); `OC_STORAGE_METRICS=1` печатает их при закрытии БД, тесты читают `Db::storage_metrics`. Тесты: `waiting_for_the_connection_does_not_stall_the_runtime` проверяет счётчики, `waits_without_a_handoff_are_counted`.
- Opt-in пробы `load_probe_child_jobs` и `load_probe_sessions` (`--ignored`; реальный диск и fsync; печатают, не утверждают тайминги).
- Пробы показали, что авто-checkpoint SQLite (fsync файла БД внутри COMMIT держателя) — источник удержаний соединения 24–29 мс на потоке runtime; без checkpoint максимум ~1 мс, p99 подтверждения при 16 сессиях 20 мс против 45 мс. Второй вынос checkpoint на отдельное соединение (фоновый поток, триггер по числу страниц WAL из `wal_hook`, затем дочистка под мьютексом писателя для перезапуска WAL) измерен и отклонён: при 1–8 сессиях вровень, при 16 — 515–553 записей/с против 632–940 и выбросы p99 до 209 мс (шум диска между прогонами ±30%). Порог 250 и 4000 страниц хуже 1000: длинный WAL дорожает в `fdatasync`.
- Принято: `wal_hook` заменяет авто-checkpoint; тот же PASSIVE checkpoint на пороге SQLite (1000 страниц) выполняет освобождающий держатель под мьютексом через `blocking` — ввод-вывод и перезапуск WAL как у авто-checkpoint, но рабочий поток tokio отдаёт свои задачи на время его fsync. Тест `a_releasing_holder_checkpoints_the_wal` (3000 записей по 4 КиБ: ≥2 checkpoint'а, WAL не растёт).
- Замеры (btrfs, fsync ≈ 6.5–9 мс, шум диска заметен):
  - сессии 1/4/8/16: 148/344/566/1016 надёжных записей/с; p50 подтверждения 6.4/13.1/13.4/13.9 мс (свой fsync плюс идущий), p99 12/25/40/45 мс — хвост задают паузы checkpoint'а (~25 мс раз в 1000 страниц); лаг таймера несвязанной задачи p99 1.13 мс, максимум ≤ 3.2 мс; `stalled_waits=0`.
  - 1k/10k child jobs: запуск в форме owner'а (intent, свежая дочерняя сессия с job одним коммитом, running outcome — три подтверждённых коммита) p50 21–29 мс, p95 31–47 мс при 1k и 48–50 мс при 10k (одинаково: работа не растёт, время — три fsync); страница истории с классификацией строк и `child_tool_jobs` p95 1.5–2.2 мс; `child_job_outstanding` 0.04 мс.
- Проверки: `oc-adapters` целиком (nextest) 1340 PASS, 8 skipped; `storage::writer` 7 PASS; Clippy `oc-adapters --all-targets -D warnings` PASS. `docs/ARCHITECTURE.md` обновлён.

### 11. Равенство схемы, SIGKILL fault-тест и инвентарь durable-точек (R1 done)

- `a_database_upgraded_from_v1_has_the_fresh_schema`: БД старейшей поддерживаемой схемы (v1) после `Db::open` и DCP/grants-схемы runtime совпадает со свежей построчно — колонки (тип, NOT NULL, default, PK) каждой таблицы, индексы, триггеры и view (>100 строк). Opt-in `a_copied_database_upgrades_to_the_fresh_schema` (`OC_SCHEMA_COPY=<копия data root>`): копия живой БД после апгрейда совпадает со свежей схемой.
- `kill_at_each_durable_point_never_replays_the_effect`: тестовый бинарник перезапускает себя дочерним процессом-владельцем, который подтверждает intent, выполняет эффект (запись маркера с fsync), подтверждает outcome и после каждой точки ждёт; родитель шлёт `SIGKILL` в точке, затем открывает БД и восстанавливает: после intent — `unknown`, 0 эффектов; после эффекта — `unknown`, 1 эффект; после outcome — `completed`, 1 эффект. Повторное восстановление и переоткрытие ничего не меняют, эффект не переисполняется. Бинарный AUD06 (`crates/oc/tests/durability.rs`) по-прежнему покрывает kill настоящего owner'а между shell-эффектом и outcome.
- Инвентарь durable-точек (статический, Explore-агент, HEAD `40ebe4706`, уровень Evidence):

| Эффект | Durable до | Durable после | После сбоя |
|---|---|---|---|
| Generation (HTTP/WebSocket frame, compaction) | принятый turn, `generation_dispatch`/`generation_retry_dispatch` до отправки | checkpoints потока, usage, результат turn'а | `recover_interrupted_tools`: turn/span `unknown`; счётчики dispatch сохраняются |
| OAuth refresh | `begin_credential_refresh` (`refresh_pending`) | `rotate_credential` | pending refresh требует повторной авторизации, не повторяется |
| Model/user shell | tool intent; `admit_shell_job_mode`/`admit_user_shell_job` | `start_shell_job` (PID), `finish_shell_job`, tool outcome | `recover_shell_jobs`: карантин известного PID или `interrupted`, затем `unknown` |
| Patch/edit/write, MCP tool call, webfetch | tool intent | tool outcome (с patch effects) | `recover_interrupted_tools`: `unknown`, без повтора |
| Code Mode program (вложенные MCP/fetch покрывает intent программы) | tool intent, `admit_execute_job`, `start_execute_job` | `finish_execute_job`, `mark_execute_effect_unknown` | `recover_execute_jobs`: `unknown` |
| Subagent | tool intent, `admit_recoverable_child_job` | `finish_child_job`, результат/notice | `recover_child_jobs`; безопасный background checkpoint может продолжиться, pending calls не повторяются |
| Notices родителю/пользователю | terminal outcome job'а | сообщение и событие notice в одной транзакции до broadcast | durable notice остаётся; live broadcast не подтверждается |
| Expiry tool output / GC сирот | `Expired` до unlink; GC после `fence` | unlink + fsync каталога | скан сирот добирает оставшиеся файлы |
| Файлы blob/tool output | файл с fsync до строки (сирота безопасна) | строка ресурса/blob | GC/скан сирот |

  Без durable intent до эффекта (существующие контракты runtime, вне этого хранилищного scope): WebSocket upgrade (сам frame учтён), spawn PTY терминала (`record_terminal` после spawn), spawn/подключение MCP-сервера, окно spawn → `start_shell_job` (нет PID для карантина).
- Проверки: `storage::schema_tests` и `storage::fault_tests` 3 прогона PASS (3 с); Clippy `oc-adapters --all-targets -D warnings` PASS.
