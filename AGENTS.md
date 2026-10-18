# OpenCode Rust — рабочая карта

Нативный Rust-бинарник `oc`, не launcher. Текущий scope и обязательная приёмка — `GOAL.md`; старые планы и архивы их не расширяют.

## Старт и навигация

- На старте и после compaction: `GOAL.md`, `progress/NOW.md`, затем `git status`/HEAD/diff. Git определяет фактическое состояние; handoff — подсказка, не замена проверки.
- Работай над одним task из `planning/tasks.json`. Если active task нет, перед изменениями запусти `python3 scripts/progress.py start Txx`. Читай только его spec и нужные контракты.
- `crates/oc-core` — runtime и порты без UI; `crates/oc-adapters` — provider, storage и tools; `crates/oc-tui` — интерфейс; `crates/oc` — бинарник и сборка приложения.
- Workflow и разрешения: `docs/AGENT_RUNBOOK.md`; критерии проверки: `docs/TEST_PLAN.md`. Старые journal leaves, архивы и upstream открывай только по конкретному вопросу.
- `customize-opencode` — встроенный [ранбук native-конфигурации](crates/oc-adapters/assets/skills/customize-opencode/SKILL.md); загружать через `skill`, обновлять вместе с описанными config consumers.

## Границы продукта

- Rust 2024, небольшой workspace, KISS/YAGNI. Ядро не зависит от UI; в production нет Node/Bun/JS-host. Внешний MCP через npx — только явная пользовательская зависимость.
- [Long Horizon](docs/LONG_HORIZON.md) — явная дивергенция форка: без искусственных round/step stops; finite failure retries и resource/security guards сохраняются. DCP Compress встроен, отключаем через config; решение не означает implementation PASS.
- OpenProxy подключается нативным Responses adapter. `providers.<id>.package: "@ai-sdk/openai"` — alias конфигурации, не npm dependency. Не зашивать model IDs, reasoning allowlists по именам и vendor-specific маршруты; discovery сверять с владельческим кодом в `references/`.
- Бинарник `oc2`; TUI/run/auth/sessions подключаются к одному native UID/data-root owner через private UDS (T68, supersedes D26). `native/` — attach/leases/wire; `application/authority.rs` — одна Db/recovery, Location sources и session runners. Второй сырой `Db` всё ещё запрещён; `oc service`/TCP/privileged broker нет. Config — только OC2 allowlist, fail-closed (D27/T62, D28/T66).
- Файловые tools выбираются по OC2 predicate: `apply_patch` либо `edit`/`write`; T50/R1/R9 в `GOAL.md` требует live switch перед следующим request той же задачи, не только новым user turn. Prepared request и его tools сохраняют captured model/view; picker draft отличается от committed selection. Это узкое model-name exception, не provider/reasoning routing. Shell не sandbox; raw history неизменна, DCP меняет provider projection.

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
- Запускай проверки затронутого crate; workspace fmt/clippy/tests/build — когда этого требует task, интеграционная граница или финальная приёмка. Полный offline gate — `bash scripts/test.sh`. Обязательные IDs/gates из `GOAL.md`, task и `docs/TEST_PLAN.md` остаются обязательными. Не отключай failing tests, не меняй baseline или GOAL ради зелёного статуса.
- Бюджет времени: один тест дольше 30 секунд, включая setup/teardown, но без компиляции и ожидания слота nextest, требует обязательного RECON. Полный прогретый offline gate дольше 10 минут на текущем 16-core host — такой же сигнал. Превышение нельзя молча принимать как норму; это не основание автоматически отключать тест.
- При превышении определить конкретный тест и дорогую фазу, установить причину в fixture, production-коде или окружении и устранить её у владельца поведения. Подтвердить результат узким замером с теми же проверками. Не ждать и не перезапускать неизменённый полный suite вслепую.
- Запрещено скрывать медлительность увеличением timeout, повторами упавшего теста или `ignore`, ослаблением assertions, уменьшением контрактной ресурсной нагрузки либо отключением durability/security guards. Искусственные sleeps заменять наблюдаемой готовностью; реальные измерительные окна сохранять.
- Исключение из бюджета — только явно согласованный с владельцем временной/нагрузочный контракт; рядом с тестом указать причину и численный бюджет. Агент не назначает исключение самостоятельно. Название `slow`/`e2e`/`soak` и исторически зелёный результат исключением не считаются.
- Если для разработки нужен реальный API-вызов, владелец разрешил брать тестовые `LUDKA2_API_URL`, `LUDKA2_API_KEY` и `OC_TEST_MODEL` из gitignored `.local/live.env`: там уже реальные значения. Используй их только для конкретного bounded live-теста по `docs/AGENT_RUNBOOK.md`, не для настройки authoring-agent; значения не печатай и не добавляй в Git.
- `checkpoint` — перед interruption/неизвестным внешним эффектом, при blocker или существенном незакоммиченном handoff, не на каждый commit. Для завершённого task — factual report и `python3 scripts/progress.py finish ...`; utility проверяет структуру, не истинность PASS.

## Live dev-сборка владельца (`oc2-dev`)

- `oc2-dev` — alias в `~/.zshrc` владельца (T67): грузит `secrets.env` как `oc2` и запускает `~/ai/oc/target/debug/oc2` из текущего каталога, ничего не собирая. Config/data общие с release `oc2`; клиенты используют первый native owner и его startup env. Старый in-process owner требует обычного quit; lock не удалять, чужой процесс не kill/restart-ить. Новый ELF подхватывается owner после завершения всех его leases.
- Мягкое правило: если изменение стоит посмотреть вживую, в конце задачи собери `cargo build -p oc2 --bin oc2 --locked` (debug, `<worktree>/target`) и сообщи, что `oc2-dev` готов; подходит и бинарник, уже собранный тестами. Это не gate и не замена обязательных проверок. Новая сборка подхватывается при следующем запуске `oc2-dev`.
- Не делай `cargo clean`, не удаляй `target/debug` и не переопределяй `CARGO_TARGET_DIR`: на этот путь опирается `oc2-dev`.
- Сам `oc2-dev` не запускай: это интерактивный запуск владельца с его секретами. Live-проверки агента — только по `docs/AGENT_RUNBOOK.md`.
- Хранилище общее с рабочим `oc2`: изменения схемы или миграций storage сразу затрагивают живые данные владельца. Явно отмечай такие изменения в отчёте.

## Безопасность и остановка

- Только выделенный non-root account и текущий worktree. Commits и обычный push своей ветки в проверенный `origin` разрешены; без force-push, чужих веток/credentials, sudo, host-admin действий, release/tag, rootful Docker и общего prune/compose down.
- Не публикуй env целиком, ключи, auth headers, raw live responses и конфигурацию authoring-agent с секретами. Перед push производного DCP-кода сохраняй AGPL provenance и уведомления.
- Внешний blocker фиксируй с фактическим результатом и следующим инженерным шагом; продолжай независимую ready-задачу. Не повторяй неизвестный внешний side effect после crash и не запрашивай повторно Q01–Q07.

## Исходники оригинала

- `.local/opencode/*` — TypeScript-исходники OpenCode 2.
