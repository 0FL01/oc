# Глубокий аудит статуса OC Rust

## Итог

Ветка `agent/oc-rust-port`, проверенный commit `fc796830cf336655bebf63b591fe0fd36cdd4310`. Работа существенная: код workspace, storage/tools/provider/discovery/MCP/DCP/TUI components и множество тестов уже существуют. Но компонентная готовность заметно обгоняет соединение компонентов в поставляемый продукт.

**Моя оценка выполненного объёма согласованного scope — около 50%, с неопределённостью примерно 40–60%.** Это экспертная оценка, а не измеренный coverage, вероятность успеха или доля оставшегося времени. Нормальный executable path всё ещё mock; это hard blocker даже если средневзвешенный score был бы выше.

В STATE закрыты 29/31 tasks: 93,548…%. Эта величина описывает записи roadmap. T27 и T30 todo. После проверки application wiring, protocol continuation и data safety она не пригодна как процент готовности замены OpenCode. В опубликованном handoff указаны 194 passed + 3 ignored; сборка/тесты в этом аудите повторно не запускались. `evidence/FINAL.md` на снимке не найден. GitHub Actions для ветки вернул 0 runs — локальные тесты это не опровергает.

## Метод оценки

Весовая модель соответствует существующим A01–A13 и находится в [COMPLETION_ESTIMATE.json](COMPLETION_ESTIMATE.json). Каждый gate получает credit за переиспользуемые реализации и report evidence с discount за разрывы реального call path. Веса суммируются в 100; расчёт даёт 49,3%, для разговора округлён до 50%. Точность десятых не заявляется.

Повышенный вес имеют общий runtime, provider protocol и tools: без них приложение не выполняет пользовательскую coding task. Журнал/сборка важны, но не компенсируют отсутствующий functional path. Один unresolved P0/P1 запрещает READY независимо от средней оценки. Для production readiness действует AND по mandatory gates, не среднее.

## Что действительно полезно и должно остаться

Четыре Cargo packages и Rust edition закреплены; не требуется начинать с нуля. SQLite имеет root lock, WAL и транзакционный append_message. Discovery явно моделирует пользовательский oracle, retries и atomic catalog replacement, хотя крайние случаи нуждаются в исправлении. MCP использует rmcp, а не произвольный самодельный transport. Есть раздельные modules для DCP projection, patch, shell, webfetch и bounded UI components. Progress journal имеет canonical state, небольшие checkpoint, derived indexes и честное предупреждение, что существование report не доказывает его смысл. Источники: [S10](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/storage.rs), [S13](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/discovery.rs), [S14](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/mcp_remote.rs), [S18](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-tui/src/app.rs), [S25](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/scripts/progress.py), [S28](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/Cargo.toml).

## Главная архитектурная причина

Пользовательский `oc run/tui` соединён с MockProvider/CoreApp, а более полноценный Runtime тестируется напрямую. TUI components также проверяются отдельно от terminal dispatch/render. Поэтому можно получить зелёные component tests, не получив работающий пользовательский путь. Это не доказательство намеренного обмана автора: это конкретный разрыв test boundary. Исправление — общий application path и binary E2E, не ещё один слой framework.

## Границы проверки

Исследованы критические entry points, runtime/tool dispatch, Responses parser/request, storage/durability, config/definitions/discovery, MCP, shell/patch/webfetch, TUI integration и selected evidence/planner scripts. Не заявляю построчный аудит каждого файла репозитория. Findings получены статическим анализом; для наиболее явных дефектов есть deterministic reproduction specifications и заготовки tests. Никакие настоящие credentials или live endpoints не использовались. Memory values из T28 — данные автора, не мои измерения.

## Отдельно про ранее запрошенные global/local файлы

A13 уже включён в branch GOAL. Но наличие loader/picker не доказывает применение global/local content из real process. В agent definitions теряется body, commands с обычным Markdown отклоняются, а CLI/TUI не собирает этот runtime path. Generic JS execution по-прежнему намеренно вне scope; нужны explicit native DCP/discovery bindings и unsupported diagnostics. Исправлять T35 и проверять T39/T41.

## Находки

Ниже перечислено 18 групп, а не все возможные дефекты. Несколько issues объединены по одному corrective acceptance task, чтобы не строить отдельный трекер. Каждая группа имеет pinned source link и тестируемый результат. Указанные эффекты выведены из кода; до исполнения tests они не именуются результатом нового динамического тестирования.

## F01 · P0 · Обычные точки входа всё ещё используют MockProvider

**Наблюдение:** bootstrap передаёт MockProvider::echo() в headless, а run_tui создаёт mock CoreApp. Более полный runtime/adapters не подключён к нормальному пути запуска бинарника.

**Требуемый результат:** Настоящие oc run и oc tui должны использовать один application composition root с effective config, Responses, tools, MCP и DCP; mock — только явный тестовый/демо-режим.

Repair: [**T31**](repairs/T31.md). Источники: [S03](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc/src/bootstrap.rs); [S04](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc/src/tui_cmd.rs); [S05](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc/src/headless.rs).

## F02 · P0 · Теряются structured continuation и отдельный call_id

**Наблюдение:** request_body отправляет одну строку с ролью user. Runtime превращает прошлые tool results в обычный текст. ToolCallStarted хранит item_id, но не отдельный call_id; output helper использует item ID. Сохранённые reasoning items не replay-ятся этим путём запроса.

**Требуемый результат:** Сохранить typed roles, завершённые provider items, отдельные call_id и function_call_output, необходимые opaque reasoning и images в реальном следующем запросе и после restart.

Repair: [**T34**](repairs/T34.md). Источники: [S06](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/runtime.rs); [S07](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/provider.rs); [S08](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/tools.rs).

## F03 · P0 · EOF и отмена могут интерпретироваться неверно

**Наблюдение:** После framing-complete EOF с непустым списком items возвращается Ok без response.completed. response.failed/incomplete игнорируются. Cancellation проверяется до ожидания очередного chunk, а config.chunk_timeout_ms не используется default-путём.

**Требуемый результат:** Typed terminal states, отменяемое ожидание headers/body, byte-bounded parser и реальные effective options. Незавершённые calls не исполняются; исчерпание rounds не означает Completed.

Repair: [**T34**](repairs/T34.md). Источники: [S07](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/provider.rs); [S21](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/evidence/T25/report.md).

## F04 · P0 · Предсказуемый временный путь patch следует symlink

**Наблюдение:** write_atomic использует имя с PID и std::fs::write вместо exclusive no-follow creation. На предсказуемом временном пути уже может существовать symlink.

**Требуемый результат:** Безопасное эксклюзивное создание temp file и handle-relative containment. Предсозданный symlink и parent swap не должны менять outside sentinel.

Repair: [**T32**](repairs/T32.md). Источники: [S09](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/patch.rs); [S27](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/docs/TOOLS_MCP.md).

## F05 · P1 · Grammar/preimages/partial results расходятся с контрактом patch

**Наблюдение:** Add File сохраняет ведущий + вместо декодирования addition syntax. Тесты используют additions без +. Hunk preimages проверяются при исполнении отдельных операций, а не при whole-plan preflight; финальной защиты от concurrent edit не видно.

**Требуемый результат:** Одна schema patchText, source-derived grammar fixtures, предварительная проверка детерминированных ошибок, защищённые per-file commits и точные partial outcomes, без фиктивной глобальной атомарности.

Repair: [**T32**](repairs/T32.md). Источники: [S09](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/patch.rs); [S27](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/docs/TOOLS_MCP.md).

## F06 · P0 · Tool intent может записываться после side effect

**Наблюдение:** execute_units выполняет builtin batch до того, как record_call записывает intent. Ошибки записи MCP intent игнорируются. Builtins группируются перед MCP и меняют порядок смешанного batch.

**Требуемый результат:** Успешный durable intent должен предшествовать dispatch. Сохранить порядок calls, failure/unknown после crash; storage error запрещает новую внешнюю операцию.

Repair: [**T33**](repairs/T33.md). Источники: [S06](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/runtime.rs); [S10](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/storage.rs).

## F07 · P1 · Модель не получает запланированный compress и nudges

**Наблюдение:** В model-facing builtins нет compress, glob и grep. run_compress вручную вызывается тестовым harness. nudge_hint возвращается в report, а не добавляется в request.

**Требуемый результат:** Опубликовать разрешённые compress/glob/grep через общий dispatcher. Проверить именно модельный вызов compression и nudge в запросе, а не только host helper.

Repair: [**T36**](repairs/T36.md). Источники: [S06](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/runtime.rs); [S08](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/tools.rs); [S19](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/tests/e2e_live.rs).

## F08 · P1 · Workspace definitions теряют значимое поведение

**Наблюдение:** В AgentDef отсутствуют Markdown body и permissions; loader отбрасывает body. Commands с любым backtick отклоняются. Предполагаемые inline filenames и переданные тестом roots не доказывают production global/local discovery.

**Требуемый результат:** Global/local sources должны достигать prompt assembly. Сохранить body и поддержанные ограничения primary agent; Markdown обрабатывать буквально, порядок sources проверить по baseline.

Repair: [**T35**](repairs/T35.md). Источники: [S11](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/config.rs); [S12](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/defs.rs); [S18](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-tui/src/app.rs).

## F09 · P0 · Legacy permission aliases не участвуют в assembly

**Наблюдение:** legacy_key существует, но assemble сохраняет исходные ключи. RuntimePolicy проверяет exact tool names: конфликтующий write/edit deny не сужает явный apply_patch allow.

**Требуемый результат:** Нормализовать aliases до policy merge и консервативно разрешить конфликт. Проверять итоговый runtime, не только helper.

Repair: [**T35**](repairs/T35.md). Источники: [S11](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/config.rs); [S06](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/runtime.rs).

## F10 · P1 · Регистр Authorization ломает MCP config

**Наблюдение:** Конфигурация сохраняет исходное написание map key. CodexWebConfig::from_entry ищет только authorization в нижнем регистре. Пользовательский пример содержит Authorization.

**Требуемый результат:** Обрабатывать HTTP headers без учёта регистра и проверять конфликты дубликатов. Неизменённый пользовательский пример должен проходить strict fake initialize/list/call.

Repair: [**T37**](repairs/T37.md). Источники: [S11](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/config.rs); [S14](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/mcp_remote.rs).

## F11 · P1 · MCP ownership привязан к turn, а не generation

**Наблюдение:** Runtime подключает и закрывает MCP на каждом turn. Stdio path завершает прямой child, не собственную process group. from_entry не сохраняет Location cwd/environment context.

**Требуемый результат:** Client принадлежит effective Location/config/auth/cwd generation. Cleanup охватывает собственные descendants, disabled entry не запускается, registry сохраняет dispatch identity.

Repair: [**T37**](repairs/T37.md). Источники: [S06](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/runtime.rs); [S14](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/mcp_remote.rs); [S15](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/mcp_stdio.rs); [S27](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/docs/TOOLS_MCP.md).

## F12 · P1 · UTF-8 HTML вызывает panic; egress guard запаздывает

**Наблюдение:** html_to_text увеличивает byte offset и на следующем шаге делает str slice. DNS precheck и соединение reqwest независимы; remote_addr проверяется уже после отправки запроса.

**Требуемый результат:** Unicode-safe extraction и стандартный URL parser. Разрешённый address должен управлять actual dial; redirects и общий deadline ограничены.

Repair: [**T38**](repairs/T38.md). Источники: [S17](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/webfetch.rs).

## F13 · P0 · Shell может зависнуть на stdin или дочерних pipe

**Наблюдение:** stdin.write_all выполняется до reader threads и запуска timeout clock. После выхода leader join drainers может ждать surviving descendants; TERM grace заканчивается по leader, даже если его дети остались.

**Требуемый результат:** Конкурентные stdin/stdout/stderr под общим отменяемым deadline. Отдельный bounded cleanup descendants после leader exit, тесты с outer watchdog.

Repair: [**T38**](repairs/T38.md). Источники: [S16](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/shell.rs).

## F14 · P1 · TUI components не подключены к terminal loop

**Наблюдение:** Бинарник рисует chat/input, но не управляет panel rendering/navigation, model/workspace snapshots и paging. Начальная история читается целиком; lines накапливается.

**Требуемый результат:** PTY tests actual binary должны управлять model/variant/session/agent/commands/skills/DCP, cancel/paste/resize через тот же application API, что и headless.

Repair: [**T39**](repairs/T39.md). Источники: [S04](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc/src/tui_cmd.rs); [S18](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-tui/src/app.rs).

## F15 · P1 · Маленький outgoing cap не ограничивает чтение истории

**Наблюдение:** Runtime читает всю историю, затем строит projection и сокращает text до 64 KiB. Это не ограничивает промежуточную память и не реализует согласованное поведение 500k-token context. Soak небольшой и преимущественно component-level.

**Требуемый результат:** Активная projection по страницам/ссылкам, независимые byte safety limits и token admission, без silent truncation. Long-history измерения actual binary при одинаковом активном контексте.

Repair: [**T40**](repairs/T40.md). Источники: [S06](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/runtime.rs); [S10](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/storage.rs); [S22](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/evidence/T28/report.md).

## F16 · P1 · Blob recovery может вернуть нечитаемый digest

**Наблюдение:** write_blob немедленно возвращает digest существующего файла, даже если crash оставил его без blobs row. read_blob требует эту row. Directory fsync и recovery ordering также требуют исправления.

**Требуемый результат:** Распознавать и валидировать orphan content; исправлять metadata или явно отказывать. Успешно возвращённый digest читается. Fault injection на file/DB boundaries.

Repair: [**T33**](repairs/T33.md). Источники: [S10](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/storage.rs).

## F17 · P1 · Крайние случаи config/discovery расходятся с исходным контрактом

**Наблюдение:** ProviderOptions не хранит headers; file substitutions идут относительно cwd. Выбор/отключение providers и root fields обработаны неполно. Discovery допускает 2^53 и иначе разбирает display name многоуровневого model ID.

**Требуемый результат:** Проверить исходный user config и oracle через executable. Unsupported semantics диагностировать, а не игнорировать. Headers заменять case-insensitively, file references разрешать относительно источника.

Repair: [**T35**](repairs/T35.md). Источники: [S11](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/config.rs); [S13](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/src/discovery.rs).

## F18 · P1 · Зелёная evidence может обходить поставляемый путь

**Наблюдение:** Live harness создаёт Runtime напрямую и возвращается нормально без credentials. DCP вызывается самим harness. Documentation checks проверяют структуру, не реализованную capability.

**Требуемый результат:** Обязательный binary-level deterministic E2E и раздельные machine-readable pass/skip/block/fail. Отсутствие env или пропуск обязательного MCP не закрывают live gates.

Repair: [**T41**](repairs/T41.md). Источники: [S19](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/crates/oc-adapters/tests/e2e_live.rs); [S20](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/evidence/T24/report.md); [S21](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/evidence/T25/report.md); [S22](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/evidence/T28/report.md); [S23](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/progress/NOW.md); [S26](https://github.com/0FL01/oc/blob/fc796830cf336655bebf63b591fe0fd36cdd4310/scripts/check_docs.py).


## Важные дополнительные проверки

Provider options/defaults должны реально достигать transport, включая custom timeout, max output и headers. HTTP HeaderMap должен быть case-insensitive и в discovery, и в MCP. Developer/system/user role semantics не могут быть заменены печатными префиксами внутри одного user message. Real model variance не оправдывает malformed protocol.

MCP registry split/renaming и schema limits должны сохранять original identity; images/unsupported content нельзя превращать в empty success. Stdio process group — отдельное ownership ограничение, не группа authoring runner. DCP file protection нельзя автоматически интерпретировать как запрет писать файл: это разные политики.

HTML UTF-8 и patch Add File могут быть воспроизведены очень маленькими deterministic fixtures. Для filesystem race, shell descendant и crash tests нужен subprocess/outer watchdog и временные безопасные пути; не использовать живые домашние каталоги/сервисы.

## Приоритет

Начать с binary smoke, который раскрывает disconnected path, и немедленно исправить опасные patch/durability boundaries до любых рабочих mutations. Затем structured Responses и config, DCP/MCP, shell/webfetch, TUI и ресурсную qualification. После этого — mandatory live и FINAL. Последовательность T31–T42 сознательно небольшая и использует уже существующий код/журнал.

Успешное завершение этих задач не гарантируется календарным сроком или названием модели. Оно определяется выполненными acceptance tests на конкретном commit и actual binary path. Новую документацию нельзя считать заменой исправленному коду.
