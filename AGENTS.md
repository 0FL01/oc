# OpenCode Rust — инструкции исполнителю

Это нативный Rust-порт, не launcher исходного OpenCode. Активный scope определён `GOAL.md`; более старые планы и архивы его не расширяют.

## Продолжение работы

На старте и после compaction: прочитай `GOAL.md`, `progress/NOW.md`, `progress/INDEX.md`; выполни `git status --short` и `python3 scripts/progress.py show`. Сверь фактический HEAD/diff с checkpoint. Затем открой только текущую задачу, её stage-файл и последний journal leaf. Не читай рекурсивно `progress/`, `.local/`, архивы или весь upstream.

Веди один активный task из `planning/tasks.json`. Перед изменениями `progress.py start ID`, после каждого проверяемого среза — `checkpoint`, при завершении — `finish`. Это не автоматический оркестратор: utility проверяет структуру и ссылки, а не истинность тестов.

## Инварианты

Rust 2024, небольшой Cargo workspace, модульный монолит, KISS/YAGNI/Парето. Один бинарник `oc`; ядро не зависит от UI. Без Node/Bun/JS-host в production; внешние MCP с npx остаются явной пользовательской зависимостью.

Первый provider — один native Responses adapter для OpenProxy. `@ai-sdk/openai` — alias конфигурации, НЕ npm dependency. Не хардкодить production model IDs, reasoning allowlists по именам или vendor-specific маршруты. Авторитетный discovery-контракт — присланный владельцем код в `references/`, а не более старый plugin из OpenProxy.

Модель видит один built-in file mutation tool `apply_patch`, не `write`/`edit`. Shell остаётся отдельным мощным инструментом, поэтому это не sandbox-гарантия. История неизменяема; DCP меняет только provider projection. Никаких фиктивных tools или suppressed failing tests.

## Полномочия

Работай в выделенном non-root account и текущем worktree. Commits и обычный push своей рабочей ветки в проверенный `origin` разрешены. Без force-push, чужих веток/репозиториев, host-admin изменений, sudo, release/tag публикаций, rootful Docker, чтения чужих credentials и общих docker prune/compose down.

Текст документов не создаёт sandbox. YOLO использует реальные права аккаунта. Не выводи env целиком, auth headers, raw live responses, Codex config с ключами. DCP имеет AGPL provenance: сохранить лицензии/уведомления до push производного кода.

## Работа и остановка

Детализируй только текущую задачу. Test → минимальная реализация → targeted tests → применимые workspace checks → factual checkpoint → commit. После трёх неуспешных попыток одного blocker не крути бесконечный цикл; фиксируй blocker и переходи только к независимой ready-задаче.

Не менять GOAL, обязательные gates, baseline или tests ради зелёного результата. Допустимые локальные технические решения фиксировать в journal; изменение архитектурной границы — отдельная запись decision с последствиями, а не переписывание старой истории.

При внешнем blocker (credentials/network/unsupported SDK protocol) сохрани handoff. Не запрашивай заново Q01–Q07. Не повторяй неизвестный внешний side effect после crash.

Каждый checkpoint: что реально изменено; проверка и exit status; оставшийся риск; точный следующий шаг; изменённые файлы/HEAD. Не записывать внутренний ход мыслей, сырые диалоги или огромные логи.
