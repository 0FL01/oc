# OC: пакет обязательных исправлений после аудита

**Снимок:** `fc796830cf336655bebf63b591fe0fd36cdd4310`. **Оценка выполненного объёма:** около 50% (экспертный диапазон 40–60%), а не 93,5% из счётчика T-задач. **Статус:** продукт пока не квалифицирован; остаются code/integration/safety blockers, не только live credentials.

Архив содержит документацию, fragments для уже существующего плана и предложенные regression tests. Он НЕ содержит исправленную Rust-кодовую базу и НЕ меняет remote repository. При копировании сохраняется весь текущий прогресс.

## Использование

Скопировать папку `audit/` в корень рабочего checkout. Начать с `GOAL.md` внутри этой папки; общий обзор — `REPORT.md`. Текст задания для исполнителя находится в `AGENT_PROMPT.txt` и не привязан к модели/runner.

### Однократная интеграция плана

1. Сверить HEAD/status/diff и сохранить текущие пользовательские изменения. Если HEAD новее audited commit — проверить существование defects заново, не делать reset. Если найдены чужие T31–T42/AUD01–AUD40 — не перезаписывать: согласовать mappings в этом пакете и документации.
2. Завершить checkpoint/block активной задачи перед registry migration. Не запускать второго journal writer. Выполнить согласованное изменение registries под существующим journal lock/в неработающей сессии, с резервными копиями и просмотром Git diff.
3. **Добавить**, не заменить: `tasks.append.json.tasks` в `planning/tasks.json.tasks`, `acceptance.append.json.tests` в `planning/acceptance.json.tests`, `state.append.json.tasks` в `progress/STATE.json.tasks`. Существующие statuses, sequence, latest, evidence, current и last_checkpoint_task сохранить. Новые tasks только todo/seq0, никаких invented PASS.
4. Применить union зависимостей из `dependencies.patch.json`: T27 и T30 дополнительно ждут T42. На audited snapshot они todo. Если к моменту применения стали done, сначала явно reopen соответствующий handoff/live task по правилам journal и перепроверить descendants; не ломать validate. Новые задачи не зависят от T27, поэтому цикла нет.
5. Выполнить `python3 scripts/progress.py reindex`, затем `python3 scripts/progress.py check` и `python3 scripts/check_docs.py`. Проверить точный CLI через --help перед вызовом в изменённой версии scripts. Если операция была прервана между JSON-файлами, восстановить согласованный набор из backup или закончить merge; не запускать init и не обнулять журнал.
6. Добавить короткую ссылку на `audit/GOAL.md` в корневой GOAL/актуальный roadmap, не менять существующие заголовки A01–A13. Commit только просмотренных documentation changes. Начать T31. Source baseline и старые authoritative fixture hashes не обновлять произвольно.

Почему append, а не массовый reopen: `progress.py` намеренно запрещает reopen родителя с завершёнными dependents. Новый последовательный repair slice M7 сохраняет audit trail и использует **тот же** journal, не заставляя переписывать 29 исторических срезов. Старые done — история работы; исправления и новая qualification имеют собственные evidence. Добавление задач само по себе не меняет техническую готовность продукта.

## После compaction

```text
AGENTS.md → root GOAL.md → audit/GOAL.md
→ progress/NOW.md → текущая audit/repairs/Txx.md
→ последний checkpoint / нужный исходник
```

Не читать весь audit inventory и архив checkpoint после каждого context reset. `NOW` и task spec достаточно для продолжения; REPORT/FINDINGS/SOURCES нужны при проверке основания дефекта. После каждого meaningful среза использовать прежние Result/Checks/Risks/Next и byte limits, без giant stdout в журнале.

## Содержимое

`REPORT.md` — оценка, проблемы и пределы проверки. `ARCHITECTURE_DELTA.md` — как соединить существующий код без второго проекта. `repairs/T31.md`–`T42.md` — конкретные изменения и тесты. JSON fragments совместимы с текущей схемой planner; FINDINGS и COMPLETION_ESTIMATE не являются вторым mutable tracker. `regressions/audit_regressions.rs` — независимые заготовки тестов для адаптации после чтения README в той папке.

## Что проверено составителем

Статические пути исполнения и контракты через GitHub connector; branch HEAD перепроверен. JSON/ссылки/graph/package integrity проверяются локальным `validate_bundle.py`. Rust tests/build/live/PTY/soak здесь **не выполнялись**: cargo/rustc недоступны, полного локального checkout нет. Опубликованные в репозитории 194 passed + 3 ignored не выдаются за независимый повторный результат.

Не заменять `progress/STATE.json` данным из старого ZIP или данным audit fragments целиком. Не копировать audit tests в production module. Secrets/реальные credentials в этот пакет не входят.
