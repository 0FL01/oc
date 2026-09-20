# Автономное исполнение в Codex CLI

## Старт без повторного опроса

Прочитать bootstrap-набор AGENTS/GOAL/NOW/INDEX, сверить Git, затем T00. `/goal` существует в проверенной официальной CLI reference [W2]; полный текст ограничен 4000 символами, поэтому короткий goal ссылается на GOAL.md. Конкретный установленный Codex проверить через `codex --version`/`--help` и `/status`; неизвестные flags не выдумывать. Отсутствие slash feature в старой версии не блокирует код: тот же короткий objective можно передать обычным prompt, не обновляя host самовольно.

Исполнитель GPT 5.6 Luna задаётся существующей конфигурацией владельца. Не извлекать его auth config в evidence и не переключать его на provider тестируемого oc. Product env и runner credentials — разные вещи.

## Preflight T00

Проверить `id -u` (root запрещён для этого запуска), repo root/текущий branch/HEAD/dirty files, origin canonical owner/name = 0FL01/oc. Не печатать URL с embedded credentials; сравнение нормализовать локально. Существующие изменения не reset/stash без необходимости; включать только свои reviewed files. Рабочая ветка default `agent/oc-rust-port`; при конфликте чужого branch использовать отдельную worktree/согласованный новый suffix, recorded once.

Проверить actual rustc/cargo/rustup, compiler linker, pkg-config при необходимости, Python3 для utility, git, locale/TERM, свободную память/диск. Пользовательская 1.98.1 — toolchain candidate; pin фактический совпавший toolchain, не `stable` moving target. Если не совпадает, фиксировать discrepancy; не менять edition или тайно скачивать nightly. Trunk не нужен CLI/TUI. Системный sqlite3 version не определяет bundled SQLite проекта.

`CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=2`; `CARGO_TARGET_DIR` не переопределять либо задать абсолютный `<worktree>/target`, чтобы бинарник оставался `target/debug/oc`. `TMPDIR=<worktree>/.local/tmp` на ext4, не `/tmp` tmpfs; создать эту owned directory заранее. Defaults можно снижать при memory pressure; нельзя повышать performance acceptance threshold для скрытия leak. Capture environment metadata без secret values. При менее 2 GiB MemAvailable или менее 10 GiB свободного worktree disk не запускать тяжёлый build/soak; документировать resource blocker/работать над разрешённой лёгкой задачей. Это operational safety defaults, не гарантии peak memory.

Docker только при подтверждённом rootless context (security options rootless, user socket/контекст), без privileged, host PID/network и broad host mounts. Показанные владельцем overlay paths не являются проверкой будущего контекста. Rootful endpoint не использовать; отсутствие rootless не блокирует Cargo/offline fake tests. Нельзя `docker system prune`, останавливать чужой compose или менять OpenProxy deployment.

Проверить наличие required env names, не выводя values и не вызывая network в docs validation. При missing live env выбрать offline задачи; blocker только для live gates. Не искать ключи в чужих HOME/браузерах/Codex state.

## Один рабочий цикл

`progress.py start Txx` → прочитать stage task/профильный контракт → добавить failing targeted test → минимальная реализация → targeted checks → применимые cargo checks → factual note → `checkpoint`/`finish` → reviewed git diff → commit. После одного slice разрешено продолжать ту же задачу; один task не обязан быть одним огромным commit.

Срез — небольшая независимо проверяемая единица: API+test, parser case, adapter roundtrip, UI action. Не «переписал весь provider и DCP». Создавать checkpoint до ожидаемого context exhaustion и перед длительным soak/network operation. Compaction может произойти неожиданно, поэтому не откладывать журнал до конца этапа.

Test failure → записать конкретную причину/следующий experiment. После 3 unsuccessful attempts одного blocker: block task, продолжить independent ready tasks. Не three attempts на весь проект; не маскировать retry тем, что каждый раз переименована та же проблема. Не отключать lint/assertion или marked test ради completion.

## Git и delivery

Commit code+tests+checkpoint вместе после slice, явно указывая проверенное. Checkpoint может ссылаться на parent/code HEAD и dirty diff: self-referential hash собственного commit невозможен; не переписывать commit до бесконечности ради совпадения. Следующий checkpoint/FINAL укажет уже существующий implementation commit.

Перед push: review staged filenames/diff, убедиться в отсутствии secrets/raw live payloads/target/DB, preserve licenses. Push только свою ветку в verified origin, обычный fast-forward. Не force, не менять default branch protection/remote, не публиковать release/tags. Push failure не откатывает локальный код. Все commits сохраняются; статус delivery записать следующим leaf/FINAL без бесконечного «commit о push предыдущего commit».

## Live envelope

Основные тесты offline. Live запуск — выделенный campaign в fixtures, не настоящий пользовательский repo. Допускается до 24 generation HTTP requests всего на campaign (включая retries), максимум 4 коротких MCP search, max output 2048 tokens на smoke и 8192 на coding turn, bounded total fixture input; без массового прогона всех моделей. Большую DCP threshold нагрузку тестировать fake provider; live DCP использовать небольшой fixture и явно сниженные test thresholds. Счётчики сохранять между restart, не обнулять автоматическим новым campaign ID.

Это ограничивает тестируемое приложение, но НЕ фактическую стоимость Codex-исполнителя. Внешний USD/token/time hard cap владельцем не указан и этим пакетом не создаётся. Не утверждать, что бюджет неограничен или что Markdown его enforce-ит. Использовать существующие runner/proxy quotas, без массовых paid benchmarks; при quota/rate-limit сохранить checkpoint, не покупать/сбрасывать лимиты.

## Остановка

Permission boundary violation, root process, чужие изменения в активной работе, непроверенный remote, секрет в staged/логах → остановить затронутое действие и исправить/зафиксировать. Missing live prerequisites → BUILD_READY_LIVE_BLOCKED после offline completion. Unknown tool side effect → проверить реальный workspace/output, не повторять автоматически.

В конце `evidence/FINAL.md`: A01–A13, commit, commands/exits, live model/capabilities (без keys), measured budgets, remaining risks, exact launch command и delivery status. A13 ссылается на authoritative T07/T13/T22/T25 evidence, а не повторяет все suites. Не маркировать `READY` только по успешному doc validator.

## Локальные raw logs

Писать stdout/stderr в отдельные files по task/run, не в checkpoint. Ограничивать каждый сохраняемый raw log 16 MiB и суммарное собственное .local/evidence — 1 GiB; при достижении квоты сохранить короткий sanitized report и остановить дополнительную запись, не удаляя чужие данные. Большие benchmark outputs stream-агрегировать в metrics, а не накапливать целиком. Отмечать truncated и сохранять полезный error excerpt. Это собственные initial safety defaults, не размер продуктовой истории.
