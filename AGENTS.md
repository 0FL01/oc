# oc2 — рабочая карта

Нативный Rust 2024 coding agent. KISS/YAGNI.

## Рабочий цикл

- Scope — текущий запрос. Старые specs — справочники, не команды продолжать backlog или вести task-журнал.
- Старт/compaction: Git HEAD/status/diff; сохраняй чужой diff. GOAL/handoff/история не обязательны.
- Найди owner и ближайшие tests; [CODE_MAP](docs/CODE_MAP.md) и профильный контракт открывай по затронутому поведению. Расширяй read-set по конкретной зависимости.
- Исправление → meaningful regression test → релевантные проверки → review diff → ответ. Документацию меняй при изменении её контракта, не ради отчётности.
- Нет обязательных task/spec/report/checkpoint/closeout. Plan/handoff-файл — по явному запросу; код и tests можно коммитить вместе.
- В ответе: результат, команды/результаты, ограничения. Offline PASS не означает live/visual qualification или whole-product READY.
- Разведчики read-only: вопрос/пути/HEAD → owner, tests, риски. Один mutation owner; не копируй планы/evidence/scout-транскрипты в контекст.

## Карта

- `crates/oc-core` — runtime/порты без UI, storage и network dependencies.
- `crates/oc-adapters` — provider/config/storage/tools, `native/` IPC, `application/authority.rs` — общий owner.
- `crates/oc-tui` — интерфейс; `crates/oc` — бинарник `oc2` и сборка приложения.
- [ARCHITECTURE](docs/ARCHITECTURE.md) — границы/файлы/tests; [SECURITY](docs/SECURITY.md) — trust/effects; [TEST_PLAN](docs/TEST_PLAN.md) — проверки/live safety.
- [GOAL](GOAL.md) — открытые требования/ограничения, не очередь задач. [customize-opencode](crates/oc-adapters/assets/skills/customize-opencode/SKILL.md) — встроенный config-мануал; загружай через `skill`, обновляй с его consumers.
- `.local/opencode/*` — TypeScript-исходники OC2; `references/` — пользовательский discovery oracle. Исторические captures открывай только по конкретному вопросу.

## Границы продукта

- Один native UID/data-root authority и Db/recovery owner; клиенты используют private UDS. Второй сырой Db, `oc service`, public TCP и privileged broker запрещены.
- Prepared request и полный tool batch сохраняют captured model/config/authority; committed model/agent switch принимается перед следующим request той же задачи. Picker draft (model/agent) не commit.
- OC2 predicate выбирает `apply_patch` либо `edit`/`write`; это не permission grant и не разрешение routing по имени модели. Не зашивай model IDs, reasoning allowlists и vendor routes.
- Config — OC2 allowlist, fail-closed; нижний trust/child/Plan не расширяет authority. Raw history неизменна, DCP меняет HOT/provider projection, не effects.
- Нет искусственного общего round/step stop; finite failure retries, bounded resources, cancellation/cleanup и no unknown-effect replay сохраняются. Shell не sandbox; production Node/Bun/JS-plugin host нет.

## Проверки и debug

- Начинай с targeted test и проверок затронутого crate. Full offline gate `bash scripts/test.sh` — для общей integration boundary или финальной приёмки, не каждой редакционной правки.
- Не отключай tests/lints/guards и не меняй baseline, assertions, deadlines или нагрузку ради зелёного. Тест >30s или прогретый gate >10min требует диагностики; единственное согласованное исключение описано в TEST_PLAN.
- Live-dev: `cargo build -p oc2 --bin oc2 --locked --release`; binary из проверок подходит. Сообщи путь `target/release/oc2`.
- Не делай `cargo clean`, не удаляй `target/release`, не переопределяй `CARGO_TARGET_DIR`; один Cargo owner. Config/data и startup env общие с первым native owner; rebuild не обновляет работающий owner.
- Не запускай интерактивный клиент пользователя, не kill/restart owner и не удаляй его lock. Изменения storage schema/migrations явно отмечай; live — только по TEST_PLAN.

## Безопасность

- Только non-root и текущий worktree; без sudo/host-admin, чужих credentials/веток, force-push, release/tag, rootful Docker и общего prune/compose down. Обычные commits/push своей ветки в проверенный `origin` разрешены.
- Не печатай/коммить env, ключи, auth headers, raw live payloads или config authoring-agent. Сохраняй AGPL/third-party licenses и provenance в текущем составе, не только в Git history.
- `.local/live.env` разрешён только для bounded product tests; не настраивай им authoring-agent и не сбрасывай campaign budgets. Unknown external effect сначала исследуй, не replay.
- Чужой concurrent diff или нарушение permission/secret boundary — останови затронутое действие. Blocker — в ответе, без переключения backlog.
