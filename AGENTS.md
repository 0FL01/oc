# OpenCode Rust — рабочая карта

Нативный Rust-бинарник `oc`, не launcher. Текущий scope и обязательная приёмка — `GOAL.md`; старые планы и архивы их не расширяют.

## Старт и навигация

- На старте и после compaction: `GOAL.md`, `progress/NOW.md`, затем `git status`/HEAD/diff. Git определяет фактическое состояние; handoff — подсказка, не замена проверки.
- Работай над одним task из `planning/tasks.json`. Если active task нет, перед изменениями запусти `python3 scripts/progress.py start Txx`. Читай только его spec и нужные контракты.
- `crates/oc-core` — runtime и порты без UI; `crates/oc-adapters` — provider, storage и tools; `crates/oc-tui` — интерфейс; `crates/oc` — бинарник и сборка приложения.
- Workflow и разрешения: `docs/AGENT_RUNBOOK.md`; критерии проверки: `docs/TEST_PLAN.md`. Старые journal leaves, архивы и upstream открывай только по конкретному вопросу.

## Границы продукта

- Rust 2024, небольшой workspace, KISS/YAGNI. Ядро не зависит от UI; в production нет Node/Bun/JS-host. Внешний MCP через npx — только явная пользовательская зависимость.
- OpenProxy подключается нативным Responses adapter. `@ai-sdk/openai` — alias конфигурации, не npm dependency. Не зашивать model IDs, reasoning allowlists по именам и vendor-specific маршруты; discovery сверять с владельческим кодом в `references/`.
- Файловые tools выбираются для текущей модели по OC2 predicate: `apply_patch` либо `edit`/`write`; обязательный switch каталога/guidance — T50/R1/R9 в `GOAL.md`. Это узкое исключение из запрета model-name heuristics, не provider/reasoning routing. Shell не sandbox; raw history неизменна, DCP меняет provider projection.

## Файлы, слайсы и контекст

- Код поддерживают LLM-агенты с общим окном 400k; это потолок всей задачи, не бюджет чтения исходников. Начинай с владельца поведения и ближайших тестов; расширяй read-set по конкретной зависимости. Маршруты: `docs/CODE_MAP.md`; правила модулей: раздел «Файлы и тесты» в `docs/ARCHITECTURE.md`.
- 5 000 физических строк — мягкий верхний ориентир для рукописного code/test-файла, не CI-запрет и не цель заполнения. Обычно достаточно укрупнённых модулей порядка 1–4k строк. Превышение требует краткого объяснения и следующего естественного шва; пересматривай его при существенном росте. Не уменьшай счётчик минификацией, `include!` или переносом всего монолита в соседний файл.
- Делить по ответственности и совместным изменениям, не по числу функций. Не создавать файл на тип/метод/тест, универсальные `utils`/`helpers`/`manager` и новые crates/traits ради раскладки. Небольшие фасады и самостоятельные узкие модули допустимы; нижнего лимита строк нет.
- Новые существенные unit-сценарии держать отдельно от production: `src/<owner>/tests.rs`, при росте — несколько тематических test-модулей. Существующие маленькие inline-тесты не переносить массово. Integration tests сохраняют свои Cargo targets; fixtures не становятся отдельными targets. Не расширять production API только ради тестов.
- Состояние/ресурсы сохраняют одного владельца; дочерние файлы могут содержать `impl` существующего типа. Сохранять внешние пути через узкие re-exports; private по умолчанию, `pub(super)` только для реального шва, без массового `pub(crate)`.
- Разведку делегировать read-only: конкретный вопрос/пути/HEAD → символы, зависимости, риски, ближайшие тесты. Возвращать краткое резюме, не исходники/логи целиком. Один mutation owner в worktree; root инструкции, большие планы, весь `evidence/` и чужие scout-транскрипты не размножать в контексте.
- При перемещении обновлять текущую карту, живые fixture/source paths и test filters; старые evidence/checkpoint leaves не переписывать. Доказательства: Git base/commits, reviewed diff и проверки; новых SHA256-манифестов для рефакторинга не создавать.
- Первый проход этой раскладки — T52. Применение плана не снимает PAUSED у T44; порядок переключения после явного resume — `roadmap/M10.md`.

## Быстрый цикл без лишних тестов

- Выбери один наблюдаемый контракт или риск → минимальная правка → ближайший targeted test → review diff. Добавляй тест на новое поведение или самостоятельный риск, а не по тесту на каждую ветку/строку и не дублируй тот же сценарий на всех слоях.
- Запускай проверки затронутого crate; workspace fmt/clippy/tests/build — когда этого требует task, интеграционная граница или финальная приёмка. Обязательные IDs/gates из `GOAL.md`, task и `docs/TEST_PLAN.md` остаются обязательными. Не отключай failing tests, не меняй baseline или GOAL ради зелёного статуса.
- Если для разработки нужен реальный API-вызов, владелец разрешил брать тестовые `LUDKA2_API_URL`, `LUDKA2_API_KEY` и `OC_TEST_MODEL` из gitignored `.local/live.env`: там уже реальные значения. Используй их только для конкретного bounded live-теста по `docs/AGENT_RUNBOOK.md`, не для настройки authoring-agent; значения не печатай и не добавляй в Git.
- `checkpoint` — перед interruption/неизвестным внешним эффектом, при blocker или существенном незакоммиченном handoff, не на каждый commit. Для завершённого task — factual report и `python3 scripts/progress.py finish ...`; utility проверяет структуру, не истинность PASS.

## Безопасность и остановка

- Только выделенный non-root account и текущий worktree. Commits и обычный push своей ветки в проверенный `origin` разрешены; без force-push, чужих веток/credentials, sudo, host-admin действий, release/tag, rootful Docker и общего prune/compose down.
- Не публикуй env целиком, ключи, auth headers, raw live responses и конфигурацию authoring-agent с секретами. Перед push производного DCP-кода сохраняй AGPL provenance и уведомления.
- Внешний blocker фиксируй с фактическим результатом и следующим инженерным шагом; продолжай независимую ready-задачу. Не повторяй неизвестный внешний side effect после crash и не запрашивай повторно Q01–Q07.

# Donor:

`./opencode` — upstream `https://github.com/anomalyco/opencode/tree/v2.0.12`, pinned at `2670273ff17da96f85c5826ced57aa1b368754fa`.
