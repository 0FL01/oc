# OpenCode Rust — инструкции исполнителю

Это нативный Rust-порт, не launcher исходного OpenCode. Активный scope определён `GOAL.md`; более старые планы и архивы его не расширяют.

## Продолжение работы

На старте и после compaction: прочитай `GOAL.md` и `progress/NOW.md`, затем сверь фактический HEAD/status/diff с handoff. Git является источником истины о файлах и delivery. Открой текущую задачу и только нужные ей контракты; indices, старые leaves, `.local/`, архивы и upstream читай только targeted.

Веди один активный task из `planning/tasks.json`. Если active task отсутствует, перед изменениями выполни `progress.py start ID`. `checkpoint` нужен перед interruption/non-idempotent external action, при blocker или существенном незакоммиченном handoff; обычный проверенный slice сохраняет Git commit. При завершении task используй `finish`. Utility проверяет структуру и ссылки, а не истинность тестов.

## Инварианты

Rust 2024, небольшой Cargo workspace, модульный монолит, KISS/YAGNI/Парето. Один бинарник `oc`; ядро не зависит от UI. Без Node/Bun/JS-host в production; внешние MCP с npx остаются явной пользовательской зависимостью.

Первый provider — один native Responses adapter для OpenProxy. `@ai-sdk/openai` — alias конфигурации, НЕ npm dependency. Не хардкодить production model IDs, reasoning allowlists по именам или vendor-specific маршруты. Авторитетный discovery-контракт — присланный владельцем код в `references/`, а не более старый plugin из OpenProxy.

Модель видит один built-in file mutation tool `apply_patch`, не `write`/`edit`. Shell остаётся отдельным мощным инструментом, поэтому это не sandbox-гарантия. История неизменяема; DCP меняет только provider projection. Никаких фиктивных tools или suppressed failing tests.

## Полномочия

Работай в выделенном non-root account и текущем worktree. Commits и обычный push своей рабочей ветки в проверенный `origin` разрешены. Без force-push, чужих веток/репозиториев, host-admin изменений, sudo, release/tag публикаций, rootful Docker, чтения чужих credentials и общих docker prune/compose down.

Текст документов не создаёт sandbox. Runner mode использует реальные права аккаунта. Не выводи env целиком, auth headers, raw live responses или конфигурацию authoring-agent с ключами. DCP имеет AGPL provenance: сохранить лицензии/уведомления до push производного кода.

## Работа и остановка

Детализируй только текущую задачу. Test → минимальная реализация → targeted tests → только применимые workspace checks → review → commit. При task finish добавь factual report и journal transition. После трёх неуспешных попыток одного blocker не крути бесконечный цикл; фиксируй blocker и переходи только к независимой ready-задаче.

Не менять GOAL, обязательные gates, baseline или tests ради зелёного результата. Допустимые локальные технические решения фиксировать в journal; изменение архитектурной границы — отдельная запись decision с последствиями, а не переписывание старой истории.

При внешнем blocker (credentials/network/unsupported SDK protocol) сохрани handoff. Не запрашивай заново Q01–Q07. Не повторяй неизвестный внешний side effect после crash.

Checkpoint: что реально изменено; проверка и exit status; оставшийся риск; точный инженерный следующий шаг; изменённые файлы/HEAD. Не создавать checkpoint только ради staging/commit/push и не записывать внутренний ход мыслей, сырые диалоги или огромные логи.
