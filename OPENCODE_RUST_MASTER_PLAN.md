# OpenCode на Rust — мастер-план v3

## Основание и статус

Редакция 2026-09-20 дополняет v2 обязательным configured-workspace workflow. Требования владельца, проверенные факты upstream и наши технические defaults разделены в `docs/DECISIONS.md`. Документы НЕ означают, что Rust-продукт уже существует или тесты уже пройдены.

Миссия неизменна: независимый нативный Rust agent runtime + CLI/TUI, без production JS/TS extension host и скрытого upstream daemon. Приоритет: сохранность данных → предсказуемые ресурсы → полезный ежедневный workflow → дальнейший parity.

## Зафиксированный продукт

Rust 2024, workspace resolver 3, модульный монолит с четырьмя packages: `oc-core`, `oc-adapters`, `oc-tui`, `oc`. Бинарник `oc`, data/config namespace `oc-rs`. Начальная платформа — предоставленный Debian 13.4 x86_64, GNU/Linux, ext4. Toolchain-кандидат — установленный владельцем 1.98.1; факт проверяется на host, затем pin, без автоматического upgrade.

Local TUI (`oc` / `oc tui`) и `oc run` используют одно приложение напрямую, без loopback HTTP. Первый результат — `daily-direct`: OpenProxy + DCP + tools + MCP + сохранение/возобновление сессий. Все остальные modes — backlog, не зависимость первого goal.

`@ai-sdk/openai` разрешается в native Responses adapter, не устанавливается как npm package. Upstream OAuth и vendor routing принадлежат внешнему OpenProxy. Имена моделей и reasoning variants приходят из config/discovery; no production model allowlist. `ludka` — static config, `ludka2` — discovery, keyed per configured provider instance. Присланный discovery-plugin имеет приоритет над более старым файлом OpenProxy.

Обязательное встроенное расширение — DCP range: `compress`, nudges, protections, deduplication, purgeErrors и persistence. Discovery — второй небольшой native module, а не повод строить generic plugin platform. DCP работает над provider projection, не удаляет историю.

Tools: `read`, `glob`, `grep`, `apply_patch`, `bash`, `webfetch`, `skill`, `compress`, namespaced MCP tools. `apply_patch` — единый built-in mutation tool для всех моделей. Без `write`/`edit` duplicates и без model-name ветвления. `skill` только возвращает bounded snapshot выбранного local skill через общий permission/tool lifecycle. Shell может изменять файлы технически; tool consolidation не объявлять security sandbox.

MCP: `codex_web` remote bearer/no OAuth; `chrome-devtools` local stdio, default disabled, command без автоматической подмены. `direct` exposure выбрано явно. Отличие от upstream Code Mode defaults записано в compatibility ledger; explicit `codemode:true` отклонять как unsupported, отсутствие ключа в выбранном профиле означает direct.

## Baselines

OpenCode: `anomalyco/opencode`, `v2.0.10`, `b8cedc1a7a5e2916bbb65dc1d4b620729c261638` — переносимый baseline из предыдущего подтверждённого recon.

DCP: `Opencode-DCP/opencode-dynamic-context-pruning`, source package `@tarquinen/opencode-dcp` 3.1.15, commit `11f6517780a502512a3467645074be447cb0369e` — проверено GitHub в этой редакции.

OpenProxy protocol reference: `0FL01/openproxy`, commit `4ef76dbce2cdbb85206cbe5e59acbad9d96ae387` — проверено в этой редакции. Это reference, НЕ требование менять живой proxy deployment.

Baseline lock: `planning/baseline.lock.json`; пользовательский JS snapshot — `references/openproxy-models.user.mjs`. Никаких silent baseline updates. Исполняемые fixtures ещё предстоит получить; source/documentary check не выдавать за runtime parity.

## Конфигурация и совместимость

Discovery источников → provenance/trust → текстовые env/file substitutions → JSON/JSONC → baseline normalization → domain-specific merge → canonical config → capability validation. Не universal deep merge. До запуска side effects ошибки не превращать в allow. Upstream config читается без записи; DCP auto-create/auto-update отключены для native port. Собственные settings — `oc-rs.toml`.

Обязательный daily-driver input: global и Location-local `opencode.json/jsonc`, `.opencode`, ordered `AGENTS.md`, local skills, selectable primary agents и current-session commands. Global `cli.json` остаётся отдельным read-only TUI domain и не участвует в runtime-config precedence. Определения имеют source provenance, bounded immutable generation и domain-specific merge rules из pinned fixtures; принятый ключ обязан иметь runtime/TUI consumer.

Plugin compatibility ограничена exact native mappings: `@tarquinen/opencode-dcp`/`@tarquinen/opencode-dcp@3.1.15` → compiled DCP и admitted `{plugin,plugins}/openproxy-models.js` → compiled discovery. Файлы JavaScript не исполняются. Unknown/lookalike JS/TS/package/path/version отклоняется до resolver/import/process/network. Generic extension host, npm install и plugin SDK отсутствуют.

Fresh start: собственная SQLite, не открывать upstream DB на запись. Никакого importer, releases или auto-install service. Product readiness определяется A01–A13, не процентом parity.

## Этапы 0–6

**M0 — контракт и старт.** Pin/check sources, licenses, host preflight, минимальные fixture contracts и working workspace. Проверить Responses wire, MCP strict protocol, configured-workspace source contracts, user discovery и DCP schema. Не аудит всего upstream.

**M1 — первый вертикальный срез.** MockProvider, typed application, SQLite, headless и минимальный TUI: input → stream → save → reopen → cancel → clean exit.

**M2 — конфиги и tools.** User config, ordered AGENTS, immutable catalogs, exact native-plugin classification, permissions, `read/glob/grep`, единый patch, shell supervision, webfetch. Filesystem conflicts и bounded output с самого начала.

**M3 — OpenProxy.** Native Responses adapter, exact discovery/merge/variants, native `skill`, errors/cancellation/reasoning/images, первый live loop. Не переносить чужой routing.

**M4 — DCP и MCP.** Нативный range/compress, projection/persistence, nudges/automatic pruning/protections; remote и stdio MCP. Без JavaScript Code Mode.

**M5 — повседневный workflow.** Model/variant/primary-agent picker, commands/skills/Location UI, sessions/resume, DCP UI, diff/cards, configured-workspace и real coding E2E, MCP search. Browser opt-in не превращается в обязательный deployment.

**M6 — проверка долгих сессий и handoff.** Soak, memory measurements, crash/restart, secret/provenance checks, fresh build без JS runtime, отчёт по каждому A-gate. Никакой миграции и публикации.

Точная task dependency graph — `planning/tasks.json`; stage contracts — `roadmap/`; detailed tests — `docs/TEST_PLAN.md`.

## Инварианты

Одна активная mutation ownership-сессия; один process owner на data root. Бounded channels и byte limits; нет unbounded accumulated streams, весь historical transcript не держится в RAM. История/blob на диске, UI viewport ограничен, summaries не образуют вторую полную history copy.

Cancellation — сигнал, не rollback и не убийство arbitrary blocking work. Все own tasks/processes имеют owner и cleanup; graceful deadline → kill/reap для child group. Не обещать exactly-once shell/MCP effects после аварии; unknown остаётся unknown и не autoretry.

Permissions едины для local и будущих transports. DCP summary и внешний текст — данные, не новый источник полномочий. Нельзя скрывать provider/MCP failures, stale catalog, partial patch или approximation токенов.

## Исполнение и журнал

Одна task за раз, один проверяемый slice за раз. После каждого — bounded factual checkpoint и commit; push своей ветки без force по runbook. В `progress/` маленькое активное резюме и дерево неизменяемых записей; no giant append-only log и no RAG service.

После compaction восстанавливать контекст через NOW → текущий этап/task → последний leaf → реальный Git diff. Goal Codex и DCP тестируемого `oc` — две разные системы; DCP не управляет Codex-сессией исполнителя.

## Не делать

Не менять scope обратно на полный upstream parity; не тащить OAuth, Code Mode, subagents, микросервисы, десятки crates, generic SDK/registry, динамический loader, JS host, npm install, HTTP daemon, GUI, LSP, importer и releases. Не переписывать OpenProxy, не хардкодить model families, не заявлять benchmarks до замера. Не откладывать TUI до конца.
