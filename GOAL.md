# Goal: OC daily-direct / OpenProxy + DCP

Обязательное уточнение acceptance после аудита: [audit/GOAL.md](audit/GOAL.md); последовательность исправлений — [roadmap/M7.md](roadmap/M7.md). Старые PASS-claims не заменяют новую qualification actual binary.

## Обязательный конечный результат

В `0FL01/oc` создан модульный монолит на Rust 2024 для Debian 13 GNU/Linux x86_64. `cargo build --locked` создаёт `target/debug/oc`; `cargo build` также работает. Бинарник запускает локальный TUI и headless coding session без исходного OpenCode, Node/Bun и JS-плагинов как runtime-зависимостей. SQLite может быть bundled. Динамическая системная линковка допустима. Существующие dev tools, shell/git и явные внешние MCP не являются частью этого запрета.

Продукт работает с пользовательским OpenProxy через нативный OpenAI Responses wire adapter; принимает config alias `@ai-sdk/openai`, baseURL/apiKey и указанные options. `ludka` поддерживает static models, `ludka2` — динамическую metadata discovery по присланному plugin без списка моделей в Rust. API/OAuth реальных upstream providers остаются за OpenProxy.

Целевой direct набор: `read`, `glob`, `grep`, `apply_patch`, `shell`, `webfetch`, `skill`, `question`, `subagent`, `compress`, `opencode_models`, `opencode_session_rename`, `opencode_session_move` и dynamically registered MCP tools. Канонический command-shell дополняется совместимостью прежнего `bash(argv)` через одного владельца исполнения/permissions; не две взаимозаменяемые схемы в каталоге. `write`/`edit`, built-in `websearch` и Code Mode/`execute` не выставляются модели. Встроенный DCP переносит required range/compress, nudges, protections, deduplication, purgeErrors, сохранение projection и минимальную панель управления. Модель может провести цикл read → patch → shell test → корректировка → ответ, выполнить webfetch и явно настроенный `codex_web` search, по запросу загрузить объявленный skill, сжать завершённый контекст и продолжить после restart. Это требуемый результат, не claim, что новые tools уже реализованы; контракт T50 ниже.

## Критерии приёмки — A01–A13

**A01 BUILD:** pinned toolchain на фактическом host, `Cargo.lock`; fmt, clippy, unit/integration tests, `cargo build --locked` и `oc --help` успешны. Проверка Rust отсутствует/падает — gate не пройден. Нет download/build upstream при запуске `oc`.

**A02 CORE:** тот же typed application runtime обслуживает TUI/headless; streaming, отмена, SQLite persistence, открытие истории и recovery проверены. Второй владелец data root отклоняется. После чистого shutdown нет собственных orphan tasks/processes.

**A03 CONFIG/DISCOVERY:** пользовательские формы config принимаются; подстановки и provenance работают; global/Location sources и определения имеют pinned deterministic order; весь discovery success/failure/timeout/merge/variant suite проходит; удалённые IDs исчезают после успешного refresh, не остаются из local overrides. Новый неизвестный model ID работает без изменения кода. Секреты/remote SDK overrides не проникают в логи/настройки.

**A04 PROVIDER:** Responses stream text/tool arguments/results/usage/reasoning metadata/image input/ошибки/cancel проверены fake server. Live text + tool roundtrip через реальный OpenProxy выполнен на опубликованной модели; нет незаметного Chat Completions/OAuth/vendor fallback.

**A05 TOOLS:** create/delete/update/empty file и конфликтный patch; read/search limits; shell stdout/stderr/exit/timeout/process cleanup; webfetch redirects/SSRF/size limits; native `skill` permission/snapshot/limits проверены. Ошибочная часть patch не выдаётся за success; tool-call/result graph валиден.

**A06 MCP:** remote `codex_web` подключается без OAuth по exact configured URL, поддерживает его JSON HTTP ответы и negotiation `2025-11-25`; получает каталог и вызывает search. Local stdio adapter работает с test server. Выключенный `chrome-devtools`, включая environment/cwd/timeout в admitted config, НЕ запускает npx/browser и не блокирует запуск. T46/R6/R7 добавляют обе donor config формы, реальные cwd/env effects и независимый async startup; optional MCP failure видим и не отменяет приложение. Его opt-in real browser smoke — условный, не обязательный для default profile.

**A07 DCP:** range schema и upstream-derived fixtures, stable IDs, nested/protected summaries, nudges, dedup/purgeErrors работают. History неизменна; projection действительно меньше на synthetic fixture; после compression и после restart агент продолжает задачу. Summary не теряет tool-result пары и не раскрывает secrets.

**A08 TUI:** prompt/paste, streaming, cancel, model picker/variants из discovery, sessions/resume, выбор primary agent, slash commands, каталог skills, Location switch, tool cards/diff, DCP context/stats и ручной `/dcp-compress` доступны. Resize, Unicode, SSH-like PTY и восстановление terminal state протестированы. Pixel parity не требуется.

**A09 CODING E2E:** seeded Rust bug fixture исправлена реальной моделью через `read` + `apply_patch` + `bash`; unit tests fixture проходят, API/чужие файлы не повреждены; повторное открытие истории и дальнейшая команда успешны. Не проверять точный natural-language ответ модели.

**A10 LONG SESSION:** offline bounded soak/large output/scroll/switch/cancel/compress и crash-recovery suite пройдены; фактические memory/task/process/queue measurements приложены. Нет linear retained-history growth; regression thresholds фиксируются после baseline и не поднимаются для сокрытия регрессии.

**A11 OPERATIONS:** handoff/block/task finish оставляют продолжимый factual checkpoint; resume после принудительного interruption проверен без reset незакоммиченной работы. Compatible coding agent не требует конкретной модели, CLI или tool API; секреты не staged; own-branch push разрешён и отражён как отдельный delivery status, без force/release. Если remote недоступен, код не теряется и readiness не подделывается.

**A12 HANDOFF:** `evidence/FINAL.md` содержит code commit, команды проверки, отдельный результат каждого A-gate, supported differences, инструкцию запуска и известные ограничения. Лицензии и provenance DCP/upstream сохранены до push производного кода. Нет обещания full OpenCode parity.

**A13 CONFIGURED WORKSPACE:** global и Location-local `opencode.json/jsonc`, ordered `AGENTS.md`, `.opencode`, skills, primary agents и current-session commands загружаются с pinned precedence, provenance и actionable diagnostics. Один turn использует одну immutable Location/config generation; switch сохраняет global и полностью убирает старое project-local состояние. Skill body появляется только как bounded result native `skill`; exact DCP и admitted `openproxy-models.js` aliases включают native modules, а прочие JS/TS/package plugins дают per-entry failed/`UnsupportedPlugin` до исполнения и без Node/Bun, не обрушая исправную локальную часть приложения (T51). CFG05–CFG10, TOOL11, UI06/UI07 и E2E05 проходят.

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

## Owner scope amendment (2026-09-26)

В T44 утверждены conversation/context-only `/undo`, `/redo` и Message Actions Revert:
не изменять workspace/files/Git, не реализовывать файловые checkpoints. Snapshots off
по умолчанию; системный Git для файлов остаётся под контролем пользователя. Пошаговый
redo и точное восстановление сохранённой LLM/DCP-проекции — сознательное отличие от
оригинала с включёнными snapshots. Контракт и план:
[T44 amendment](tui-recovery/T44_CONTRACT_AMENDMENT.md#owner-amendment-2026-09-26-conversation-only-undoredo).
Прежнее R5/VIS10 требование отката файлов superseded владельцем, остальные gates сохраняются.
Утверждение плана не является claim реализации; припаркованный код пока не возобновлён.

## Owner scope amendment (2026-09-27 — prompt/subagents/DCP)

Владелец утвердил весь обсуждённый план T45:
[M8 contract R3/R6–R10](docs/goals/2026-09-21-config-compat-and-subagents.md) и
[ordered slices](roadmap/M8.md). Обязательны настоящий background и concurrent
foreground children, durable result/recovery/cancel lifecycle; собственный профиль
и автоматический workspace/tools/skills контекст ребёнка без копии parent transcript;
optional context_message_ids с точными цитируемыми parent messages, стабильными IDs
независимо от DCP и truthful effective-capability preview. Общая сборка base/custom
system, Linux environment/date, AGENTS baseline/nested reads/chronological updates
и restart/compaction/Revert reconciliation проходит через одного runtime owner.

Child DCP allowSubAgents defaults true; explicit false/global/manual/effective Deny
сохраняют ограничения. Compress собственной сессии не меняет parent/siblings;
активные task/context pack защищены без пожизненного накопления. Нет default lifetime
compress-call/block-count quota или неизбежного nesting/archive-loading тупика;
per-call/graph/model/active-memory/turn safeguards и A10 resource gates остаются.
В проверенном DCP 3.1.15/native коде cumulative quota не найдена; причина старого
OC1-сбоя не установлена. Контекст-пакеты/host fields/default child DCP — native
extensions, не обещание полного parity или бесконечных ресурсов.

Эта запись дополняет, не ослабляет A02/A04/A05/A07/A10/A13 и не отменяет запрет
unknown-effect replay, parent-child authority narrowing, canonical trust boundaries,
skill body только через native skill, apply_patch вместо write/edit и отсутствие
JS/CodeMode/cloud host. Donor recovery at-least-once не является exactly-once effects.
T45 владеет новыми scenarios; T44 UI qualification отдельна, без done-dependency.
Утверждён только план: execution statuses/evidence не становятся PASS.

## Owner scope amendment (2026-09-27 — selected native tools)

Утверждён [T50 tool contract](docs/goals/2026-09-27-native-tool-parity.md) и срезы
[M8](roadmap/M8.md): Linux shell command/background/automatic notices; grep regex/
literal/path/include/case, glob path/hidden; настоящий question; read text/directories/
images (PDF вне цели); webfetch text/markdown/html/timeout; прямые native model lookup,
session rename и same-session move без Code Mode. Built-in websearch и его provider
integrations сознательно исключены; поиск и browser через explicit MCP сохраняются.
Не добавлять встроенный browser, LSP, PDF или JS host под видом parity.

Только прежняя пожизненная session→Location привязка superseded: явный admitted move
сохраняет ID/историю и применяется на безопасной границе с новой target generation.
In-flight turn/request/tool не перепривязывается; старые operations/background jobs/
children сохраняют execution provenance. Trust admission, permissions, MCP quarantine,
immutable raw history и запрет unknown-effect replay остаются. Обычный UI Location
switch выбирает/создаёт target-scoped session, не является скрытым move.

TOOL12–TOOL19 имеют одного владельца T50; T45/T46 и T44 visual gates сохраняют свой
scope, без circular done-dependencies. Дополнение не ослабляет A01–A13/живые gates,
не переписывает исторические PASS и не меняет existing task execution statuses.
Утверждён и доставляется план, новая реализация ещё pending.

## Owner scope amendment (2026-09-27 — service config/startup/error isolation)

После RECON владелец утвердил [T46 R6/R7](docs/goals/2026-09-22-mcp-attach-parity.md),
[T51 R1–R3](docs/goals/2026-09-27-startup-fault-isolation.md) и
[T44 VIS19/VIS40/VIS42](tui-recovery/T44_CONTRACT_AMENDMENT.md#service-configstartup-error-parity--vis19vis40vis42).
MCP legacy/canonical config, environment/cwd/timeouts имеют реальную семантику;
ошибка отдельной записи/optional plugin/provider connection не отменяет TUI/history/
model picker. Enabled MCP initial startup асинхронен через существующего owner;
failed/pending/disabled и безопасные details доступны до первого prompt. Selected
model не подменяется, запрос к недоступной модели даёт явную ошибку; fatal native
startup содержит safe source/field/stage/code/action, не только общую категорию.

Admitted **local MCP** наследует product-process environment плюс configured overlay:
это явное изменение старого T37/MCP04 минимального env, не policy обычного shell.
Command/resource/credential-domain admission предшествует inheritance; lower-trust
command не получает higher-trust secrets автоматически. No runner-auth extraction,
env/config/raw-error dumps или чтение/редактирование пользовательского config ради green.

Superseded только app-wide optional-service rejection, lazy first-turn MCP attach и
minimal local-MCP env/cwd restriction. Trust/security-critical invalid policy, storage/
data-root/recovery, cancellation, cleanup/McpShutdown/caps остаются non-success;
immutable generations, discovery oracle/budgets, redaction и unknown-effect quarantine
не меняются. OAuth/CodeMode/arbitrary JS host и новый protocol вне утверждённого среза
имеют честное per-service unsupported состояние, не working parity. Historical PASS
не переписываются. MCP09/MCP10 — только T46, CFG09/CFG10/UI07 — только T51; T44 владеет
visual qualification отдельно, без circular whole-task done-dependencies. План pending.

## Исполнение

Исполнение не привязано к GPT, модели, provider или CLI. Любой compatible coding agent, удовлетворяющий контракту `docs/AGENT_RUNBOOK.md`, может продолжать работу в выделенном worktree. Модель/CLI authoring-agent не являются частью product config и не выбираются через `OC_TEST_MODEL`. Не обещать завершение за фиксированное число суток. Остановки при rate limit/компакции/crash должны оставлять продолжимый worktree, а не стирать незавершённую работу.
