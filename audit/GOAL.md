# Корректирующий goal: довести существующий oc до A01–A13

Основание: аудит `fc796830cf336655bebf63b591fe0fd36cdd4310` ветки `agent/oc-rust-port`. Это дополнительный обязательный contract к корневому `GOAL.md`; прежний scope не расширяется. Если исходный код уже изменился после снимка, проверить каждый finding на новом HEAD и приложить новый regression/evidence; не откатывать репозиторий к audit SHA.

## Цель

Пользователь запускает собранный `oc`, и **сам этот бинарник** читает global/project конфигурацию, использует OpenProxy Responses и динамический discovery, выполняет все заявленные tools/MCP/DCP, сохраняет/возобновляет сессии и предоставляет рабочий TUI. Отдельно работающие modules, test-only Runtime и render helpers не удовлетворяют этому goal.

Сохранить Rust 2024, модульный монолит, четыре packages, KISS/YAGNI/Pareto. Не добавлять OAuth, ChatCompletions fallback, daemon/HTTP attach, Code Mode, migration/release, произвольный JS/TS/WASM plugin host, subagents или cloud orchestrator. Native plugin mappings и primary-agent definitions обязательны в границах A13.

## Иерархия исполнения

Долговечные ограничения `AGENTS.md` и подтверждённые пользовательские требования остаются в силе. Для описанных дефектов этот файл и `repairs/T31.md` … `T42.md` уточняют acceptance и отменяют ошибочный смысл прежних PASS-claims. Корневые A01–A13 и исходные test IDs сохраняются. Не подменять их новыми слабее.

## Начало работы

Прочитать `audit/README.md`, `audit/REPORT.md`, `progress/NOW.md`, `progress/INDEX.md`, актуальный Git status/diff. Однократно выполнить явный merge fragments из `audit/`, описанный в README, в существующие registries. Не создавать второй progress engine. Затем работать по одной ready-задаче T31–T42. В каждой задаче сохранять checkpoint после среза; текущий статус имеет единственный источник `progress/STATE.json`.

Первые integration проверки выполняются только на temporary fixture/fake endpoints. До устранения P0 не испытывать file tools на ценных пользовательских данных. Новые paid/live probes до offline protocol qualification не нужны.

## Обязательные инварианты

- Normal run/tui не используют MockProvider и не игнорируют user config. Тестовый provider — явный opt-in, не fallback.
- Один application владеет session, transitions, policy и persistence. UI не сохраняет input самостоятельно до принятия приложением.
- Responses: typed roles/items/call IDs, complete terminal, bounded real-time streaming, interruptible cancellation, faithful continuation/restart.
- Любая мутация следует только после durable intent и permission check. Ошибка storage не становится allow; crash ambiguity не вызывает auto-replay.
- Patch не следует symlinks через temporary/parent paths, не теряет concurrent edits, соответствует выбранной grammar и честно сообщает partial failure.
- MCP использует реальную пользовательскую Authorization-конфигурацию и generation lifetime, а не new child каждый turn.
- Модель действительно вызывает compress; nudge действительно присутствует в request; raw history остаётся неизменной.
- Global/local agents/commands/skills/AGENTS/config влияют на effective behavior, не только отображаются в списке.
- Output/history/UI/state ограничены по bytes и lifetime; большой архив не материализуется целиком, silent 64KiB truncate не заменяет DCP.

## Завершение

T42 завершает mandatory offline qualification и обновляет ссылки на доказательства. T27 затем выполняет существующую bounded live campaign через actual binary; T30 составляет FINAL по A01–A13. Требуются mapping всех F01–F18 и результаты AUD01–AUD40.

`READY` возможен только при реальном прохождении mandatory gates и отсутствии unresolved findings. `BUILD_READY_LIVE_BLOCKED` допустим лишь когда весь mandatory offline продукт исправлен/проверен, а оставшаяся причина действительно external live prerequisite. Наличие mock normal path, unsafe patch, broken continuation или непроверенной durability — product gap, не отсутствие credentials.

Не использовать число закрытых T-задач, строк кода или тестов как самостоятельное доказательство завершения. Не ослаблять тесты, не менять expected grammar/protocol под текущую реализацию, не утверждать «исправлено» без исполнения regression.
