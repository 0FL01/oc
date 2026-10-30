# Goal: рефакторинг SQLite-хранилища под нагрузку

Status: done (R1–R6); один критерий приёмки выполнен частично — см. «Итоговая приёмка»
Source: пользовательский запрос «фундаментально расхуярить» дефект хранилища, RECON 2026-10-29 (замеры на синтетике 10k child jobs, копии живой БД и fsync-бенче), согласованный план «рефакторинг SQLite, без Turso»; инструкция «делай копию плана в цель и итеративно реализовать и коммит пуш билд после каждой итерации».
Last updated: 2026-10-31

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
  - Status: done
  - Evidence: групповой коммит с fsync вне мьютекса (итерация 4) и handoff tokio-воркера на каждом ожидании БД (итерация 6) — ожидание соединения, ожидание fsync своей пачки, WAL checkpoint (итерация 10) и fsync файлового хранилища (итерация 12) идут через `block_in_place`; пул читателей WAL (итерация 8); memo проекции DCP на счётчике коммитов писателя, одна версионированная цепочка миграций при открытии (итерация 12); clippy `await_holding_invalid_type` запрещает держать guard соединения через `.await`. Механизм отличается от плана «выделенный OS-поток писателя с каналами»: тот же результат — runtime не ждёт SQLite — достигнут без перевода ~360 мест `conn.lock()`; SQL держателя выполняется на вызывающем потоке (среднее удержание ~0.3 мс). Доказательства: `stalled_waits=0`, лаг таймера несвязанной задачи p99 1.13 мс при 16 сессиях, fault-тесты (итерация 11). Подтверждения по-прежнему гарантируют `synchronous=FULL` в WAL (fdatasync WAL до ответа).
- R4: Модель данных без квадратичной записи (Фаза 3).
  - Acceptance: TurnLog append-only (`input`, `opaque`, `spans`, `display_parts` строками), чтения History без `json_extract` по мегабайтным blob'ам, типизированные колонки вместо JSON в горячих предикатах, посессионные последовательности там, где позволяет контракт; миграция существующих данных проверена на копии реальной БД; legacy-таблицы удалены только подтверждённо мёртвые.
  - Primary evidence: turn на 100 tool calls — линейный объём записи; апгрейд копии реальной БД.
  - Status: done
  - Evidence: итерация 13. Журнал живого turn'а хранится контентно-нарезанными чанками (`storage/turn_chunks.rs`), `turns.result` — stub; правка в любом месте документа переписывает соседние чанки, при выходе из `started` журнал один раз пишется целиком. Тест `a_turn_writes_linear_volume_in_its_tool_calls`: 50 → 100 вызовов — 2287 → 5219 страниц WAL (×2.28; запись журнала целиком — 1476 → 5299, ×3.6). История читает головы журналов (`turn_heads`, `turn_raw_heads`: без тяжёлых значений, ~13% и ~11% байт) вместо мегабайтных `turns.result`: окно истории самой тяжёлой сессии копии живой БД p50 377 → 140 мс, p95 516 → 152 мс. Горячие предикаты по JSON уже обслуживаются выражениями индексов (типизированные значения в индексе); их остаточные проверки читают голову. Апгрейд копии живой БД: журналы байт-в-байт (SHA3), схема равна свежей, 3.3 с. Решения: глобальные `messages.seq`/`events.seq` сохранены (ID `m{seq}`, декодирование якорей, idle-токены по rowid событий и смежность legacy shell `admission.seq=input.seq+1` зависят от глобального порядка; выделение — O(log n) по `messages_seq`); триггерное версионирование DCP сохранено (замер: обновление отслеживаемой строки 7.7 страниц WAL и 0.27 мс против 1 страницы и 0.04 мс, но число таких обновлений линейно по generation, ~27 на turn в живой БД); legacy-таблицы не удалены — `dcp_tool_projection` v1 версионируется и восстанавливается из сохранённых точек, мёртвой не доказана.
- R5: Чтения не встают в последовательную очередь Location worker (Фаза 4).
  - Acceptance: History/Workspace/ChildJobs обслуживаются пулом читателей параллельно.
  - Status: done
  - Evidence: итерация 14. Все чтения страницы History (транскрипт, границы, длина, сообщения, shell, child history, notices, images, окно turn'а с презентацией) и ChildJobs идут через пул читателей; lease файла question допускает писатель (атомарно с expiry). Location worker отдаёт History, ChildJobs и TabDeck в `spawn_blocking` вместо своей последовательной очереди; `deck_gate` — RwLock: History и Workspace читают параллельно, исключают их только записи deck и переносы. Workspace: tab deck вне очереди; `session_root`/`inbox_pending` — одиночные запросы писателя (`session_root` выбирает очереди approval/question). Тест `history_page_reads_do_not_wait_behind_the_writer`.
- R6: Итерационная поставка.
  - Acceptance: после каждой итерации релевантные тесты и scoped Clippy, commit и push в проверенный `origin`, release build; full offline gate перед интеграционной приёмкой фаз R3–R5; документация контракта обновлена.
  - Status: done
  - Evidence: итерации 1–14 — каждая с релевантными проверками, scoped Clippy, commit и push в `origin/master` и release build; полный offline gate на интеграции R3 (итерация 12), R4 (13) и R5 (14, `EXIT 0`); `docs/ARCHITECTURE.md` обновлялся вместе с контрактом.

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

### 12. Единая цепочка миграций, memo на счётчике коммитов, handoff файловых fsync (R3 done)

- `Db::migrate`: все шаги схемы (прежние 16 функций `*_schema`, индексы горячих путей, ранее ленивые DCP- и grants-схемы runtime) выполняются одной упорядоченной цепочкой при открытии и пропускаются, когда `PRAGMA user_version` равен `SCHEMA_CHAIN_VERSION` (17); номерные шаги по-прежнему пишут свои маркеры `schema_migrations` (свежая БД теперь записывает и 2, и 9). Сбой до записи версии повторяет цепочку при следующем открытии. Восстановление после рестарта (отмена прерванной компакции, `Interrupted` для активной записи tool output) вынесено в `recover_on_open` и выполняется при каждом открытии. Runtime больше не создаёт схему лениво.
- Тесты: `schema_version_tracks_the_schema` (отпечаток FNV свежей схемы привязан к `SCHEMA_CHAIN_VERSION`: изменение любого шага без повышения версии падает), `the_chain_runs_only_below_the_current_version`. Девять тестов, моделирующих старую БД правкой схемы уже открытой текущей, теперь сбрасывают и `user_version` в 0, как у настоящей старой БД; ожидаемые списки маркеров дополнены 2 и 9.
- Memo `presentation_wire_logs` проверяет счётчик коммитов писателя вместо `total_changes()` (читается под его мьютексом, транзакций других держателей нет).
- `Db::sync_durable` (fsync файлов tool output и blob'ов) идёт через тот же `blocking`. `clippy.toml`: `await-holding-invalid-types` для `storage::writer::Guard` (проверено временным нарушением — Clippy падает).
- Метрика удержания считает SQL держателя без handed-off checkpoint'а: среднее 0.27–0.36 мс при 1–16 сессиях, единичные максимумы 9–18 мс; лаг таймера p99 1.13 мс, максимум ≤ 1.9 мс.
- Проверки: `oc-adapters` целиком 1344 PASS; Clippy workspace `--all-targets -D warnings` PASS; полный offline gate `scripts/test.sh` 2537 PASS, 1 FAIL, 22 skipped (963 с): `pty_t39 aud29_pty_panels_change_runtime_state` не дождался эха ответа на экране за отведённое время (провайдер запрос получил); отдельно 4 из 4 PASS за 3.3 с, весь `pty_t39` 68 PASS — расценено как сбой под нагрузкой gate, повторный полный gate — при интеграции R4/R5.

### 13. Журнал живого turn'а чанками, головы журналов для истории (R4 done)

- `storage/turn_chunks.rs`: пока turn `started`, его точные байты журнала лежат в `turn_chunks` (ключ — rowid turn'а × 2^36 + позиция, без второго индекса; rowid стабилен, VACUUM не выполняется), нарезанные gear-хешем по старшим 12 битам (граница — в среднем раз в 4 КиБ после 1 КиБ, на 16 КиБ — по последней «слабой» границе, тоже по содержимому, поэтому повторяющиеся участки режутся одинаково после правки перед ними) только по границам символов. Запись сопоставляет SHA-256 (16 байт) старых и новых чанков по порядку, удаляет исчезнувшие и вставляет новые в зазоры между сохранёнными (упаковка у начала зазора, перенумерация при исчерпании). `turns.result` — stub: тот же объект с опустошёнными растущими значениями, поэтому выражения индексов и чтения мелких полей работают и для живого turn'а. View `turn_logs` собирает журнал байт-в-байт; триггер `turn_chunks_settle` записывает его в `turns.result` при выходе из `started` без нового журнала (settlement с журналом просто удаляет чанки).
- Запись: все `UPDATE turns SET result` идут через `Db::write_turn_result` (те же проверки сессии/`started` и число затронутых строк); патчи `json_set` (display, восстановление живого span) читают целый журнал через `turn_logs`. Чтения целого журнала и растущих значений (CAS RAW, fork, child recovery, проекция DCP через `conversation_turns`, байтовые бюджеты, `turn_result`) — через `turn_logs`. Тест `user_images_tests` читает живой журнал через `turn_logs`.
- Пробы чанкера: мелкие чанки (128–512 Б мин.) и чанки размером страницы дают больше страниц WAL (стоимость определяется числом изменённых строк, а не байтами); ключ-rowid без индекса `UNIQUE(turn_id,pos)` — 2610/5697 → 2287/5219 страниц.
- Головы: `turn_heads` (для завершённых turn'ов, триггеры на смену статуса/журнала и вставку; бэкфилл при апгрейде) и `turn_raw_heads` (триггер на вставку RAW-сегмента; RAW-строки не меняются) — журнал без `HEAVY`-значений, из `input` только элементы с ролью или содержимым (остальные — `null`, индексы сохраняются). View `turn_log_heads` (для живого turn'а — целый журнал). Отдельные таблицы, а не колонка: обновление колонки строки `turns` переписывало её мегабайтную overflow-цепочку (+90 МиБ файла при апгрейде) и чтение колонки после `result` проходит цепочку. История (спаны, части, тексты сообщений, display/model, якоря транскрипта, native notice, child terminal, images) читает головы; смещение `raw_prefix` вычисляется один раз (`MATERIALIZED`), а не на каждую часть. Индексы `events_shell_notice`/`events_execute_notice` (shell notice в окне 30 → 1.6 мс).
- Проба `load_probe_history_on_copy` (`OC_PROBE_COPY`, `OC_PROBE_PROFILE` печатает самые дорогие операторы): окно истории сессии с 10 МиБ журналов p50 377 → 140 мс, p95 516 → 152 мс. Проба `load_probe_dcp_versioning`.
- Цепочка схемы 20; `writer` считает `wal_frames` (страницы, записанные в WAL).
- Диагностика роста gate (836 → 963 → 1146 с за итерации 8/12/13): открытие свежей БД занимало 1.15 с — 114 fsync, по одному на транзакцию каждого шага цепочки (с итерации 12 цепочка включает и бывшие ленивые DCP-схемы), а тесты открывают свежие БД тысячи раз. Цепочка теперь идёт с `synchronous=OFF` и одной FULL-фиксацией версии (контрольные суммы WAL оставляют прерванную цепочку на целом шаге, версия пишется последней): свежее открытие 1149 → 168 мс (10 fsync), существующее — 11–20 мс; тест `typed_effect_uncertainty_and_unanswered_calls_never_claim` 26.7 → 6.8 с. Медленное свежее открытие объясняет и таймауты `aud29_pty_panels_change_runtime_state` под нагрузкой gate в итерациях 12–13.
- Проверки: полный offline gate `scripts/test.sh` 2542 PASS, 1 FAIL, 25 skipped, 410 с (было 1146 с): `vis12_user_shell_application_admits_history_before_owned_effect_without_model_turn` читал файл эффекта сразу после его появления, до записи shell'ом (гонка теста, проявившаяся с ускорением); тест теперь ждёт содержимое — 4 из 4 PASS. Пробы `load_probe_history_on_copy`, `load_probe_dcp_versioning`, `load_probe_open`; Clippy workspace `--all-targets -D warnings` PASS.

### 14. History, ChildJobs и TabDeck вне очереди Location worker (R5 done)

- Хранилище: `history_bounds`, `history_len`, `read_history_message_typed`, `read_history_after_typed`, `user_shell_result`, `model_shell_notice`, `user_images`, `history_turn_window` и `turn_presentation_range` выполняются на пуле читателей (одна read transaction на вызов). `question_presentation_in` получает признак соединения: на писателе — прежний lease под тем же соединением, на читателе — `open_tool_output`, который берёт писателя и перепроверяет ресурс (не `Expired`, тот же dev/ino/extent), поэтому допуск lease остаётся атомарным с expiry. Порядок блокировок задокументирован в `storage/readers.rs`: чтение может взять писателя, писатель никогда не ждёт соединение пула (внутри хранилища нет вызовов пула под писателем).
- Приложение: тело обработчика History вынесено в `history_page(db, location, catalog, …)`, привязка сессии — `runtime::session_bound` (тот же предикат, что `Runtime::open_session`), загрузка tab deck — `tab_deck::load_at(db, location)`. `off_queue` в цикле Location worker отправляет History, ChildJobs и TabDeck в `spawn_blocking` с общим handle БД и снимком каталога (copy-on-write); синхронный `query()` для внутренних вызовов не изменился.
- Сервер: `deck_gate` — `tokio::sync::RwLock`; History и Workspace берут чтение, сохранение/удаление deck, открытие сессии из picker и перенос — запись.
- Тест `history_page_reads_do_not_wait_behind_the_writer`: писатель держит соединение с незакоммиченной правкой, все чтения страницы History из другого потока завершаются.
- Проверки: полный offline gate `scripts/test.sh` 2544 PASS, 25 skipped, `EXIT 0` (418 с). Самый долгий case — `latest_windows_do_not_stop_execution_and_each_part_keeps_its_actual_span` 34 с под нагрузкой gate (отдельно 9.6 с; с отключённой записью чанков 11.2 с — CPU-ёмкий тест, не регрессия хранилища); `ui07` 31 с в пределах согласованного исключения ≤40 с.

## Итоговая приёмка

| Критерий | Результат | Доказательство |
|---|---|---|
| async-воркеры не ждут fsync/мьютекс БД | выполнен | Каждое ожидание соединения, fsync своей пачки, checkpoint и fsync файлов идёт с handoff воркера (`block_in_place`); `stalled_waits=0`; лаг таймера несвязанной задачи p99 1.13 мс при 16 сессиях; Clippy запрещает держать guard через `.await`. SQL держателя выполняется на вызывающем потоке (в среднем ~0.3 мс). |
| 8–16 сессий: p99 подтверждения ≤ одной пачки fsync; пропускная способность кратно выше ~100 tps | частично | 148/344/566/1016 надёжных записей/с при 1/4/8/16 сессиях (≈10× исходных ~100); p50 6.4–13.9 мс (свой fsync плюс идущий). p99 12/25/40/45 мс: хвост задают паузы WAL checkpoint (~25 мс раз в 1000 страниц), во время которых писатели ждут. Вынос checkpoint на отдельное соединение измерен дважды (итерации 5 и 10) и отклонён: без перезапуска WAL `fdatasync` дорожает и пропускная способность падает до 2×; порог 250/4000 страниц хуже 1000. |
| 10k child jobs: запуск p95 < 50 мс, страница History < 200 мс | выполнен (запуск — на грани) | Запуск в форме owner'а p95 48–50 мс при 10k и 31–47 мс при 1k — одинаково, работа не растёт; время — три подтверждённых коммита по fsync диска. Страница истории с классификацией p95 ≤ 2.2 мс; на самой тяжёлой сессии копии живой БД (10 МиБ журналов) окно истории p95 152 мс. |
| turn на 100 tool calls: объём записи линейный | выполнен | `a_turn_writes_linear_volume_in_its_tool_calls`: 50 → 100 вызовов ×2.28 страниц WAL против ×3.6 при записи журнала целиком. |
| `kill -9` в любой точке: ноль переисполненных эффектов, история не теряется | выполнен | `kill_at_each_durable_point_never_replays_the_effect` (после intent, эффекта, outcome) и бинарный AUD06. |
| gate и `perf_tui_pty` без регрессий | выполнен | Полный offline gate 2544 PASS, `EXIT 0`, 418 с (было 785–1146 с за время работы). Синтетическая сессия 600 turn'ов (`perf_long_session`): страница owner 65–70 мс (база ~70 мс), кадр скролла p95 1.6–1.8 мс (база ~1.9 мс). `perf_tui_pty`: набор 60 Гц p95 1.22 мс (база 18–19 мс), 165 Гц 9.9 мс, PageUp 1.09 мс, набор во время стрима 10.7 мс, во время shell-стрима/spinner 1.42/1.28 мс, простой — 0 тиков CPU. Сценарий F пробы ждал `function_call_output` последним элементом запроса, а DCP с 2026-10-28 добавляет после него `<dcp-message-id>`; проба теперь пропускает этот хвост. p50 кадра скролла 1.41 мс против ~0.8 мс базы 2026-10-23 — сторона TUI (кадры без подгрузки страниц хранилище не читают). |

Не входит в цель и замечено попутно: время генерации синтетической сессии растёт с длиной истории (каждый запрос несёт весь контекст, 10.7 МиБ к 600-му turn'у, без DCP-сжатия в синтетике); инвентарь durable-точек фиксирует эффекты без durable intent вне хранилища (WebSocket upgrade, spawn PTY, spawn MCP-сервера, окно spawn → PID shell).
