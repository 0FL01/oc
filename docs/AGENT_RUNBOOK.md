# Контракт compatible coding agent

## Старт без повторного опроса

Compatible agent читает `AGENTS.md`, `GOAL.md`, `progress/NOW.md`, сверяет реальный Git HEAD/status/diff и продолжает текущую задачу. Indices и старые leaves открываются только при targeted recovery. Objective из `prompts/AGENT_GOAL.txt` можно передать через native prompt/task/job механизм конкретного runner; repository не требует конкретного slash command, CLI, flags, model или provider.

Модель, provider и credentials compatible agent принадлежат внешнему runner и не являются частью product config. Не извлекать runner auth config в evidence и не переключать authoring-agent на provider тестируемого `oc`. `OC_TEST_MODEL` выбирает только модель live-теста продукта; product env и runner credentials — разные вещи.

Минимальная совместимость: agent умеет читать и изменять assigned worktree любым механизмом, запускать разрешённые shell-команды и тесты, фиксировать exit status/redacted evidence, проверять `git status`/`HEAD`/diff и вести progress checkpoint после interruption/resume. `apply_patch` — product-tool contract для модели `oc`, не обязательный интерфейс authoring-agent. Agent обязан соблюдать one mutation owner per worktree, OS/filesystem/network/secret/Git permissions и остановиться при неизвестном side effect или concurrent diff.

## Preflight T00

Проверить `id -u` (root запрещён для этого запуска), repo root/текущий branch/HEAD/dirty files, origin canonical owner/name = 0FL01/oc. Не печатать URL с embedded credentials; сравнение нормализовать локально. Существующие изменения не reset/stash без необходимости; включать только свои reviewed files. Рабочая ветка default `agent/oc-rust-port`; при конфликте чужого branch использовать отдельную worktree/согласованный новый suffix, recorded once.

В T00 проверить только identity/repository/origin, one mutation owner и ресурсы для ближайшего действия. Rustc/cargo/rustup и linker проверить непосредственно перед T01; pkg-config — только если его потребует выбранная dependency; locale/TERM — перед PTY/TUI; live env names — перед T16/T27. Пользовательская 1.98.1 — toolchain candidate; pin фактический совпавший toolchain, не `stable` moving target. Если не совпадает, фиксировать discrepancy; не менять edition или тайно скачивать nightly. Trunk не нужен CLI/TUI. Системный sqlite3 version не определяет bundled SQLite проекта.

Для полного test gate по умолчанию `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2` (warm comparison: 2/1 — 408.07s PASS, 3/2 — 283.16s PASS; 4/2 — 281.39s PASS, 4/3 — 262.63s FAIL из-за PTY cursor timeout). Не запускать одновременно несколько Cargo-команд с одним target. `CARGO_TARGET_DIR` не переопределять либо задать абсолютный `<worktree>/target`, чтобы бинарник оставался `target/debug/oc`. `TMPDIR` направить в заранее созданную owned directory на disk вне worktree, не в `/tmp` tmpfs. Defaults можно снижать при memory pressure, не ослабляя обязательные gates и не меняя deadlines; нельзя повышать performance acceptance threshold для скрытия leak. Capture environment metadata без secret values. При менее 2 GiB MemAvailable или менее 10 GiB свободного worktree disk не запускать тяжёлый build/soak; документировать resource blocker/работать над разрешённой лёгкой задачей. Это operational safety defaults, не гарантии peak memory.

Docker проверять только непосредственно перед первым реальным Docker-вызовом. Тогда разрешён лишь подтверждённый rootless context без privileged, host PID/network и broad host mounts. Если Docker не нужен, T00 фиксирует `NOT_USED`; отсутствие rootless не блокирует Cargo/offline fake tests. Нельзя `docker system prune`, останавливать чужой compose или менять OpenProxy deployment.

Наличие required live env names проверять перед T16/T27, не выводя values. При missing live env выбрать offline задачи; blocker только для live gates. Не искать ключи в чужих HOME/браузерах или state внешнего runner.

## Один рабочий цикл

Если нет active task: `progress.py start Txx` → прочитать task и профильный контракт → failing targeted test → минимальная реализация → targeted checks → только применимые cargo checks → review diff → commit. При продолжении той же task административный checkpoint не нужен. При task completion implementation commit уже существует: добавить evidence report, выполнить `finish`, review и сделать один closeout commit.

Срез — небольшая независимо проверяемая единица: API+test, parser case, adapter roundtrip, UI action. Не «переписал весь provider и DCP». Создавать checkpoint до ожидаемого context exhaustion, перед non-idempotent external operation, при blocker или существенном незакоммиченном handoff. Semantic `Next` не должен быть только «stage/commit/push».

Test failure → записать конкретную причину/следующий experiment. После 3 unsuccessful attempts одного blocker: block task, продолжить independent ready tasks. Не three attempts на весь проект; не маскировать retry тем, что каждый раз переименована та же проблема. Не отключать lint/assertion или marked test ради completion.

## Git и delivery

Commit code+tests после проверенного slice. Finish report ссылается на уже существующий implementation commit; generated progress state и report идут одним closeout commit. Не создавать новый checkpoint только для фиксации SHA или результата обычного push: Git tracking divergence уже различает uncommitted, committed и delivered состояния.

Перед push: review staged filenames/diff, убедиться в отсутствии secrets/raw live payloads/target/DB, preserve licenses. Push только свою ветку в verified origin, обычный fast-forward. Не force, не менять default branch protection/remote, не публиковать release/tags. Push failure не откатывает локальный код. Все commits сохраняются; статус delivery записать следующим leaf/FINAL без бесконечного «commit о push предыдущего commit».

## Live envelope

Основные тесты offline. Live запуск — выделенный campaign в fixtures, не настоящий пользовательский repo. Допускается до 24 generation HTTP requests всего на campaign (включая retries), максимум 4 коротких MCP search, max output 2048 tokens на smoke и 8192 на coding turn, bounded total fixture input; без массового прогона всех моделей. Большую DCP threshold нагрузку тестировать fake provider; live DCP использовать небольшой fixture и явно сниженные test thresholds. Счётчики сохранять между restart, не обнулять автоматическим новым campaign ID.

Это ограничивает тестируемое приложение, но НЕ фактическую стоимость authoring-agent. Внешний USD/token/time hard cap владельцем не указан и этим пакетом не создаётся. Не утверждать, что бюджет неограничен или что Markdown его enforce-ит. Использовать существующие runner/proxy quotas, без массовых paid benchmarks; при quota/rate-limit сохранить checkpoint, не покупать/сбрасывать лимиты.

## Остановка

Permission boundary violation, root process, чужие изменения в активной работе, непроверенный remote, секрет в staged/логах → остановить затронутое действие и исправить/зафиксировать. Missing live prerequisites → BUILD_READY_LIVE_BLOCKED после offline completion. Unknown tool side effect → проверить реальный workspace/output, не повторять автоматически.

В конце `evidence/FINAL.md`: A01–A13, commit, commands/exits, live model/capabilities (без keys), measured budgets, remaining risks, exact launch command и delivery status. A13 ссылается на authoritative T07/T13/T22/T25 evidence, а не повторяет все suites. Не маркировать `READY` только по успешному doc validator.

## Локальные raw logs

Писать stdout/stderr в отдельные files по task/run, не в checkpoint. Ограничивать каждый сохраняемый raw log 16 MiB и суммарное собственное .local/evidence — 1 GiB; при достижении квоты сохранить короткий sanitized report и остановить дополнительную запись, не удаляя чужие данные. Большие benchmark outputs stream-агрегировать в metrics, а не накапливать целиком. Отмечать truncated и сохранять полезный error excerpt. Это собственные initial safety defaults, не размер продуктовой истории.
