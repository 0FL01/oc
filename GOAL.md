# Goal: OC daily-direct / OpenProxy + DCP

Обязательное уточнение acceptance после аудита: [audit/GOAL.md](audit/GOAL.md); последовательность исправлений — [roadmap/M7.md](roadmap/M7.md). Старые PASS-claims не заменяют новую qualification actual binary.

## Обязательный конечный результат

В `0FL01/oc` создан модульный монолит на Rust 2024 для Debian 13 GNU/Linux x86_64. `cargo build --locked` создаёт `target/debug/oc`; `cargo build` также работает. Бинарник запускает локальный TUI и headless coding session без исходного OpenCode, Node/Bun и JS-плагинов как runtime-зависимостей. SQLite может быть bundled. Динамическая системная линковка допустима. Существующие dev tools, shell/git и явные внешние MCP не являются частью этого запрета.

Продукт работает с пользовательским OpenProxy через нативный OpenAI Responses wire adapter; принимает config alias `@ai-sdk/openai`, baseURL/apiKey и указанные options. `ludka` поддерживает static models, `ludka2` — динамическую metadata discovery по присланному plugin без списка моделей в Rust. API/OAuth реальных upstream providers остаются за OpenProxy.

Доступны built-ins `read`, `glob`, `grep`, `apply_patch`, `bash`, `webfetch`, `skill`, `compress` и dynamically registered MCP tools. `write`/`edit` не выставляются модели как альтернативные built-ins. Встроенный DCP переносит required range/compress, nudges, protections, deduplication, purgeErrors, сохранение projection и минимальную панель управления. Модель может провести цикл read → patch → shell test → корректировка → ответ, выполнить webfetch и `codex_web` search, по запросу загрузить объявленный skill, сжать завершённый контекст и продолжить после restart.

## Критерии приёмки — A01–A13

**A01 BUILD:** pinned toolchain на фактическом host, `Cargo.lock`; fmt, clippy, unit/integration tests, `cargo build --locked` и `oc --help` успешны. Проверка Rust отсутствует/падает — gate не пройден. Нет download/build upstream при запуске `oc`.

**A02 CORE:** тот же typed application runtime обслуживает TUI/headless; streaming, отмена, SQLite persistence, открытие истории и recovery проверены. Второй владелец data root отклоняется. После чистого shutdown нет собственных orphan tasks/processes.

**A03 CONFIG/DISCOVERY:** пользовательские формы config принимаются; подстановки и provenance работают; global/Location sources и определения имеют pinned deterministic order; весь discovery success/failure/timeout/merge/variant suite проходит; удалённые IDs исчезают после успешного refresh, не остаются из local overrides. Новый неизвестный model ID работает без изменения кода. Секреты/remote SDK overrides не проникают в логи/настройки.

**A04 PROVIDER:** Responses stream text/tool arguments/results/usage/reasoning metadata/image input/ошибки/cancel проверены fake server. Live text + tool roundtrip через реальный OpenProxy выполнен на опубликованной модели; нет незаметного Chat Completions/OAuth/vendor fallback.

**A05 TOOLS:** create/delete/update/empty file и конфликтный patch; read/search limits; shell stdout/stderr/exit/timeout/process cleanup; webfetch redirects/SSRF/size limits; native `skill` permission/snapshot/limits проверены. Ошибочная часть patch не выдаётся за success; tool-call/result graph валиден.

**A06 MCP:** remote `codex_web` подключается без OAuth по exact configured URL, поддерживает его JSON HTTP ответы и negotiation `2025-11-25`; получает каталог и вызывает search. Local stdio adapter работает с test server. Выключенный `chrome-devtools` НЕ запускает npx/browser и не блокирует запуск. Его opt-in real browser smoke — условный, не обязательный для default profile.

**A07 DCP:** range schema и upstream-derived fixtures, stable IDs, nested/protected summaries, nudges, dedup/purgeErrors работают. History неизменна; projection действительно меньше на synthetic fixture; после compression и после restart агент продолжает задачу. Summary не теряет tool-result пары и не раскрывает secrets.

**A08 TUI:** prompt/paste, streaming, cancel, model picker/variants из discovery, sessions/resume, выбор primary agent, slash commands, каталог skills, Location switch, tool cards/diff, DCP context/stats и ручной `/dcp-compress` доступны. Resize, Unicode, SSH-like PTY и восстановление terminal state протестированы. Pixel parity не требуется.

**A09 CODING E2E:** seeded Rust bug fixture исправлена реальной моделью через `read` + `apply_patch` + `bash`; unit tests fixture проходят, API/чужие файлы не повреждены; повторное открытие истории и дальнейшая команда успешны. Не проверять точный natural-language ответ модели.

**A10 LONG SESSION:** offline bounded soak/large output/scroll/switch/cancel/compress и crash-recovery suite пройдены; фактические memory/task/process/queue measurements приложены. Нет linear retained-history growth; regression thresholds фиксируются после baseline и не поднимаются для сокрытия регрессии.

**A11 OPERATIONS:** handoff/block/task finish оставляют продолжимый factual checkpoint; resume после принудительного interruption проверен без reset незакоммиченной работы. Compatible coding agent не требует конкретной модели, CLI или tool API; секреты не staged; own-branch push разрешён и отражён как отдельный delivery status, без force/release. Если remote недоступен, код не теряется и readiness не подделывается.

**A12 HANDOFF:** `evidence/FINAL.md` содержит code commit, команды проверки, отдельный результат каждого A-gate, supported differences, инструкцию запуска и известные ограничения. Лицензии и provenance DCP/upstream сохранены до push производного кода. Нет обещания full OpenCode parity.

**A13 CONFIGURED WORKSPACE:** global и Location-local `opencode.json/jsonc`, ordered `AGENTS.md`, `.opencode`, skills, primary agents и current-session commands загружаются с pinned precedence, provenance и actionable diagnostics. Один turn использует одну immutable Location/config generation; switch сохраняет global и полностью убирает старое project-local состояние. Skill body появляется только как bounded result native `skill`; exact DCP и admitted `openproxy-models.js` aliases включают native modules, а прочие JS/TS/package plugins дают `UnsupportedPlugin` до исполнения и без Node/Bun. CFG05–CFG08, TOOL11, UI06 и E2E05 проходят.

Детальные test IDs и методики: `docs/TEST_PLAN.md`. Задачи/зависимости: `planning/tasks.json`.

## Статусы завершения

`READY`: A01–A13 подтверждены в требуемом объёме, обязательные live checks PASS. Условный browser smoke может быть NOT_RUN_DISABLED. Git delivery отдельно: PUSHED либо BLOCKED_REMOTE.

`BUILD_READY_LIVE_BLOCKED`: offline обязательные проверки PASS, бинарник собран, конкретные live prerequisites отсутствуют. Это полезный промежуточный handoff, НЕ полный goal success.

`BLOCKED`: нет дальнейших разрешённых ready-задач; описать минимальный blocker, выполненное и точное продолжение. Не менять обязательный gate в N/A из-за сложности.

## Вне этого goal

OAuth providers/MCP, другие native provider families, Chat Completions adapter, Code Mode interpreter, служба serve/attach и HTTP parity, web/desktop, LSP, snapshot/undo, subagents/task orchestration, arbitrary JS/TS/npm plugin runtime, general plugin SDK/hot-load, migration/importer, publishing/packages/auto-update, embeddings/RAG журнала. DCP experimental message mode, executable skills/commands и config-authoring UI отложены и не должны приниматься как working config.

## Owner scope amendment (2026-09-21)

Владелец расширил scope: subagent system теперь входит в обязательный результат —
[docs/goals/2026-09-21-config-compat-and-subagents.md](docs/goals/2026-09-21-config-compat-and-subagents.md),
milestone [roadmap/M8.md](roadmap/M8.md). Пункт «subagents/task orchestration» в списке
«Вне этого goal» и соответствующий запрет в `audit/GOAL.md` остаются историей аудитов
A01–A13 и не ограничивают T43. Совместимость markdown-config с upstream v2.0.12
(frontmatter/permissions/skills/commands) и снятие искусственных size-лимитов — обязательные
требования T43. Остальные границы (без OAuth, ChatCompletions fallback, daemon/serve/attach,
Code Mode, JS/TS/WASM plugin host, cloud orchestrator) не меняются.

Не превращать browser `enabled:false` в true автоматически. Не переносить OpenProxy внутрь `oc` и не редактировать его deployment.

## Исполнение

Исполнение не привязано к GPT, модели, provider или CLI. Любой compatible coding agent, удовлетворяющий контракту `docs/AGENT_RUNBOOK.md`, может продолжать работу в выделенном worktree. Модель/CLI authoring-agent не являются частью product config и не выбираются через `OC_TEST_MODEL`. Не обещать завершение за фиксированное число суток. Остановки при rate limit/компакции/crash должны оставлять продолжимый worktree, а не стирать незавершённую работу.
