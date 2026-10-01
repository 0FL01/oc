# Goal: OC daily-direct / OpenProxy + DCP

Обязательное уточнение acceptance после аудита: [audit/GOAL.md](audit/GOAL.md); последовательность исправлений — [roadmap/M7.md](roadmap/M7.md). Старые PASS-claims не заменяют новую qualification actual binary.

## Обязательный конечный результат

В `0FL01/oc` создан модульный монолит на Rust 2024 для Debian 13 GNU/Linux x86_64. `cargo build --locked` создаёт `target/debug/oc`; `cargo build` также работает. Бинарник запускает локальный TUI и headless coding session без исходного OpenCode, Node/Bun и JS-плагинов как runtime-зависимостей. SQLite может быть bundled. Динамическая системная линковка допустима. Существующие dev tools, shell/git и явные внешние MCP не являются частью этого запрета.

Продукт работает с пользовательским OpenProxy через нативный OpenAI Responses wire adapter; принимает config alias `@ai-sdk/openai`, baseURL/apiKey и указанные options. `ludka` поддерживает static models, `ludka2` — динамическую metadata discovery по присланному plugin без списка моделей в Rust. API/OAuth реальных upstream providers остаются за OpenProxy.

Целевой direct набор: `read`, `glob`, `grep`, файловое семейство `apply_patch` либо `edit`/`write` по текущей модели, `shell`, `webfetch`, `skill`, `question`, `subagent`, `compress`, `opencode_models`, `opencode_session_rename`, `opencode_session_move` и dynamically registered MCP tools. Точный OC2 selector и обязательное обновление каталога/guidance при смене модели — amendment T50 от 2026-10-01 ниже. Канонический command-shell дополняется совместимостью прежнего `bash(argv)` через одного владельца исполнения/permissions; не две взаимозаменяемые схемы в каталоге. Built-in `websearch` и Code Mode/`execute` не выставляются модели. Встроенный DCP переносит required range/compress, nudges, protections, deduplication, purgeErrors, сохранение projection и минимальную панель управления. Модель может провести цикл read → совместимая файловая правка → shell test → корректировка → ответ, выполнить webfetch и явно настроенный `codex_web` search, по запросу загрузить объявленный skill, сжать завершённый контекст и продолжить после restart. Это требуемый результат, не claim, что новые tools уже реализованы; контракт T50 ниже.

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

**A13 CONFIGURED WORKSPACE:** global и Location-local `opencode.json/jsonc`, ordered `AGENTS.md`, `.opencode`, skills, primary agents и current-session commands загружаются с pinned precedence, provenance и actionable diagnostics. Один turn использует одну immutable Location/config generation; switch сохраняет global и полностью убирает старое project-local состояние. Skill body появляется только как bounded result native `skill`; exact DCP и admitted `openproxy-models.js` aliases включают native modules, а прочие JS/TS/package plugins дают per-entry failed/`UnsupportedPlugin` до исполнения и без Node/Bun, не обрушая исправную локальную часть приложения (T51). Унаследованные OpenCode TS конфиги/MCP и недоступные saved agent/model/variant не блокируют исправный локальный Home/history/selectors; недоступный выбор остаётся явным и запрещает запрос до effects, без скрытого fallback/reset или ослабления policy. CFG05–CFG10, TOOL11, UI06/UI07 и E2E05/E2E06 проходят; E2E06 обязательно квалифицирует rebuilt release-бинарник в реальной текущей пользовательской среде под PTY/strace, с existing и fresh native data roots.

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
skill body только через native skill и отсутствие
JS/CodeMode/cloud host. Donor recovery at-least-once не является exactly-once effects.
Прежнее «apply_patch вместо write/edit» superseded только файловым amendment T50
от 2026-10-01 ниже; остальные ограничения этой записи сохраняются.
T45 владеет новыми scenarios; T44 UI qualification отдельна, без done-dependency.
Утверждён только план: execution statuses/evidence не становятся PASS.

Уточнение владельца 2026-09-30: [agent-cycle slice T44/T45](tui-recovery/T44_CONTRACT_AMENDMENT.md#agent-cycle-keybindings--уточнение-2026-09-30)
планирует исправление ошибочного Shift+Tab → agent picker. Дефолты остаются OC2:
Shift+Tab → следующий профиль, /agents и `<leader>a` → picker; обычный Tab обслуживает
автодополнение. Настраиваемые forward/reverse/list bindings позволяют явно вернуть
Tab/Shift+Tab из OC1 без перехвата активных forms/dialogs/autocomplete. Один selection
owner сохраняет выбор/draft и guards; eligibility — T45/R6, UI — существующие
VIS06/VIS10/VIS17, не новый gate. Реальное поведение configured Tab у pinned OC2
квалифицируется отдельно от native override; T44 PAUSED, active T50 и statuses intact.

## Owner scope amendment (2026-09-30 — infinite hot context / optional cold path)

Владелец уточнил Long Horizon: бесконечное продолжение достигается намеренным
забыванием ненужного контекста, а не сохранением всей истории во вложенных summaries.
Hot path — текущее окно модели: выбранный закрытый диапазон заменяется самостоятельным
рабочим summary; не выбранные для сохранения данные уходят из provider input и resident
active state. Старые summaries разрешено сокращать заново и забывать. Число/возраст
прошлых compressions, historical depth, покрытые member IDs и старые tool payloads не
становятся обязательным грузом следующего request. Flatten без реального забывания
этот контракт не закрывает.

Cold path — пользовательские `.md`-файлы/Git-история — полностью опционален. Compress
не требует журналирования, не пишет файлы/commits автоматически и не восстанавливает
забытое из архива. Existing native persistence/history/Undo остаются отдельными от
модельной памяти; это не разрешение удалять raw records или выполнять файловый rollback.
Restart восстанавливает последнее committed hot-представление. Явный read/context
selection или Undo могут вернуть выбранную пользователем информацию; обычное
продолжение, recompression и `/compact` не воскрешают её сами.

Ручной `/compact` совместим с DCP: новый самостоятельный checkpoint заменяет прежний,
сохраняет актуальную задачу/выбранные факты/свежий хвост и может забывать устаревшее.
Закрытые tool-call/result/reasoning группы могут уходить из hot path целиком; in-flight
batch не разрывается и tools не переисполняются. Runtime task/pack protection действует
в своём актуальном scope; завершённая работа не получает пожизненного наследования.
Явные effective user protections/permissions не обходятся молча.

Это уточняет только hot-retention смысл A07 и T45/R9/DCP11, не ослабляет A10, raw-history
immutability, trust/security или no unknown-effect replay. Самостоятельный hot state и
доступный bounded recovery проверяются без cold path; прежние failed/no-gain попытки
не исчерпывают способность выбрать более широкий диапазон/меньший replacement.
Подробный контракт — [DCP hot/cold](docs/DCP.md#infinite-hot-context--optional-cold-path--t45r9dcp11-pending),
порядок работ — [M8](roadmap/M8.md#infinite-hot-context--optional-cold-path--t45r9dcp11-approved-2026-09-30-pending).
Новых tasks/store/framework/paid campaign нет; T44 остаётся PAUSED, активная задача
не переключается plan patch. Реализация и qualification pending; historical PASS
и source baseline неизменны.

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

## Owner scope amendment (2026-09-27 — canonical effort ordering)

После RECON владелец утвердил [T47 R6/VAR01](docs/goals/2026-09-22-backend-blockers.md#current-checkpoint--approved-canonical-effort-plan-2026-09-27)
и [общий контракт](docs/CONTRACTS.md#canonical-effort-ordering--t47var01-approved-2026-09-27-pending).
Доступные уровни идут Default → none → minimal → low → medium → high → xhigh → max
→ custom → Default с пропуском отсутствующих/disabled. Это относительный порядок
поддержанных уровней, не обещание наличия всех пяти и не новый capability allowlist.
Явный известный effort имеет приоритет над именем; при его отсутствии ранжируется
стандартное имя, при неизвестном явном effort запись custom. Ties/aliases/custom
стабильны по effective source order; ID и wire не переписываются, selection по ID.

Это узкое native отличие от declared-order OC2. T44 VIS09/VIS29 и T50 TOOL18 используют
один ordered view после merge, не независимую лексикографическую сортировку. DISC06,
config precedence, no-overlay Default ≠ named none, no silent retired-choice fallback,
profile/child selection и immutable generations сохраняются. VAR01 владеет только T47;
минимальный backend slice предшествует presentation, не whole-task completion. План
pending/NOT_RUN; исторические PASS, execution statuses и baselines не изменяются.

## Owner scope amendment (2026-09-29 — OpenCode Go и единые provider credentials)

После RECON и независимого аудита владелец утвердил [T53 contract/ordered slices](docs/goals/2026-09-29-opencode-go-and-provider-auth.md)
и [M8 follow-up](roadmap/M8.md). Добавляются native `opencode-go`, public models.dev
catalog, Console API-key `/connect`/account management → provider-aware `/models`,
Go request headers на main/auxiliary lanes и необходимые Chat/Messages adapters
наряду с Responses. Admitted custom providers используют те же finite protocols:
explicit no-auth local server не требует dummy key; `ludka2` сохраняет configured
baseURL/apiKey/headers/options и OpenProxy `/models` oracle без изменения приоритета.

Один credential owner в existing native SQLite хранит tagged Key/OAuth material;
config/env остаются resolver inputs, endpoint/protocol — connection config. Stored
credentials привязаны к admitted provider/endpoint scope, не текут при URL override.
OAuth representation нужна для будущих providers, но Codex/OAuth execution/refresh,
`auth.json`/import/dual-write, SDK host и generic registry остаются вне этого среза.
Прежний запрет native families/Chat/direct Go superseded **только** для T53;
никакого скрытого model/protocol/OAuth fallback или расширения T51.

GO01–GO06 имеют только T53 как owner; A01–A13 и mandatory OpenProxy gates сохраняются.
Protocol-safe immutable journals/DCP/forks и pre-effect unavailable refusal обязательны.
Нужны минимальные qualified T51/T47 seams, не whole-task done-dependencies;
T50 потребляет общий catalog. T44 остаётся PAUSED и владеет отдельной visual
qualification после explicit resume. T53 todo, реализация/Go live pending/NOT_RUN;
existing execution statuses и исторические PASS не переписываются.

## Owner scope amendment (2026-09-29 — inherited TS config / real-user startup)

Владелец потребовал закрыть startup gap для пользователей с накопленными конфигами
и MCP от OpenCode TS, проверить текущую конфликтующую среду от лица пользователя
и запуск бинарника под `strace`; затем утвердил внесение этого дополнения в план.
[T51 R4/E2E06](docs/goals/2026-09-27-startup-fault-isolation.md) дополняет R1–R3:
saved missing/invalid agent/model/variant, включая restored/parked tabs, не обрушает
локальный TUI. Нужны явное unavailable состояние и ручное исправление выбора;
turn/headless с ним отказывают до generation/tools, не выбирают sibling/default.
Mandatory policy/trust/storage/recovery/cleanup/caps остаются настоящими fatal.

E2E06 имеет одного владельца T51: source-derived offline regression/recovery плюс
bounded PTY/strace текущего non-root пользователя, реальных HOME/XDG/PATH/Location
и неизменённых config sources. Обязательны bare `target/release/oc` с существующим
default native store и тот же config environment с fresh `--data-dir`. `--help`,
temp HOME, только fresh store или NOT_RUN не заменяют real-user qualification.
Методика и безопасные trace/effect assertions — [TEST_PLAN](docs/TEST_PLAN.md#e2e06--inherited-ts-config--real-user-startup-approved-2026-09-29-pending),
порядок — [M8](roadmap/M8.md). No paid generation в пользовательском workspace;
никакого TS DB import/migration, редактирования user config или сброса native prefs.
MCP ownership остаётся T46, visual — T44 после explicit resume; T44 остаётся PAUSED.
Это доставка плана, не runtime PASS и не завершение T51/A13; historical evidence
и execution statuses не переписываются.

## Owner scope amendment (2026-09-29 — LLM-provider retry и TUI)

Владелец запросил полный retry parity с pinned OC2 v2.0.12 для ошибок
LLM-провайдера (временные лимиты, исчерпанная квота, 5xx) и визуальный
паритет при retry. После RECON и независимого аудита утверждён
[T54 backend contract](docs/goals/2026-09-29-provider-retry-parity.md) с одним
RET01; [T44 VIS43](tui-recovery/T44_CONTRACT_AMENDMENT.md) владеет только
парным TUI-доказательством после явного resume. Полный retry parity нельзя
заявить по одним RET01/нативным golden. Это узкое исключение из общего A08
«pixel parity не требуется» для retry notice, не требование завершить весь T44.
Новые Chat/Messages из T53/GO03 обязаны потреблять общий retry owner при
их допуске; текущий Responses
result не ждёт завершения всей T53/T44/T51. T44 остаётся PAUSED.

Узко superseded старое pre-first-event-only/две попытки в
`docs/CONTRACTS.md`: типизированные ошибки, конечные задержки и
продолжение после частичного вывода разрешены **без** повторного исполнения
подтверждённых/неизвестных tools. `response.incomplete(max_output_tokens)`
сохраняет явный `length`, а не становится скрытым полным ответом; EOF/
unknown-incomplete/exhaustion без подлинного завершения — non-success.
Исчерпанная quota по умолчанию terminal, временный throttle/server — retry;
наблюдаемый provider header имеет строго bounded override. Immutable raw
history/DCP, binding/credential/config generation, validation, redaction,
trust/permissions, cancellation, timeout/resource caps и no unknown-effect
replay не ослабляются. T51 readiness refresh, discovery и MCP retries не
являются paid generation retry. План pending/NOT_RUN, historical PASS и
execution statuses не переписываются; A01–A13/mandatory gates сохраняются.

## Owner scope amendment (2026-09-30 — Middle Click закрытие вкладки)

Владелец сообщил, что в Rust TUI вкладка не закрывается средней кнопкой мыши,
в отличие от OpenCode TS 2, и потребовал внести подробное исправление в план.
[T44 / R5 / VIS44](tui-recovery/T44_CONTRACT_AMENDMENT.md#middle-click-tab-close--vis44)
добавляет обычный Middle Click по всей видимой области доступной вкладки:
закрытие на mouse-down, без предварительного выбора или попадания в крестик,
в горизонтальном и вертикальном layout, включая compact rail. Это закрытие
вкладки через существующий native CloseTab owner, не удаление сессии/истории;
нужны actual-binary PTY, reopen/restart и парное доказательство с pinned OC2.

Pinned original допускает закрытие busy-вкладок; существующий native контракт
его запрещает. Этот срез не разрешает снять busy/permission/Location/lifecycle
защиты: различие фиксируется явно, полный busy-close parity требует отдельного
утверждения безопасной семантики. VIS44 принадлежит только T44 и не дублирует
VIS39/VIS41. План pending/NOT_RUN; T44 остаётся PAUSED до explicit resume,
активная T50, historical evidence и execution statuses не меняются.

## Owner scope amendment (2026-10-01 — child TUI / Subagents / Shell / Terminals)

Владелец потребовал **полный визуальный и интерактивный паритет этого сегмента**
с pinned OC2 v2.0.12: из parent delegation/notice открывается собственный живой
TUI ребёнка, доступны возврат, выбор/прерывание детей, настоящий Ctrl+B, нижний
composer Subagents/Shell/Terminals, Shell output и рабочий terminal pane. После
RECON владелец явно выбрал «Включить Terminals», затем утвердил подробную запись
плана и commit/push. OCR иллюстрирует сценарий, не задаёт точные строки/цвета.

- [T45/R3/SUB01/SUB02](docs/goals/2026-09-21-config-compat-and-subagents.md)
  владеет live child events, bounded family/current-state projections и реальным
  independent lifecycle/control; [T50/R2/TOOL13](docs/goals/2026-09-27-native-tool-parity.md)
  — authoritative shell list/live bounded output/targeted kill и переводом уже
  запущенного foreground shell в background без повторного запуска.
- Новый [T56/TERM01](docs/goals/2026-10-01-native-session-terminals.md) владеет
  session-local interactive native PTY и реальным frontend consumer. Terminals
  не являются shell jobs и не ограничиваются декоративной вкладкой. PTY переживает
  hide/show и переключение представления в живом `oc`; чистый shutdown reaps,
  crash/restart не переисполняет неизвестные команды. Daemon/serve/attach/HTTP parity
  и наследование provider/runner credentials **не** разрешены.
- [T44/R4/R5/VIS39](tui-recovery/T44_CONTRACT_AMENDMENT.md#child-tui-shellterminals--уточнение-vis39-2026-10-01)
  квалифицирует весь сегмент парными full styled-cell/PNG/cursor captures, включая
  controls, геометрию, semantic colors/attributes, focus, selection и анимации.
  VIS14/VIS35 используют общий grammar-based syntax renderer для parent/child:
  donor language/alias/query inventory и точные стили, не четыре эвристики или
  только совпавшая палитра. Plain Shell text и VT/ANSI terminal colors отличны
  от syntax highlighting. Это узкое уточнение общего A08, не full-product parity.

Superseded только blanket refusal child navigation/control и исключение PTY
terminal-manager **для T56**. Child view не становится произвольным root-turn
editor; profile/model/Undo authority, trust/permissions, immutable history и
execution generations, bounded resources и no unknown-effect replay сохраняются.
Минимальные behavioral slices предшествуют visual qualification; нет circular
whole-task dependencies, второго store/framework или нового paid campaign.
Plan-only доставка не запускает T56, не меняет активную T50, не снимает PAUSED
у T44 и не переписывает historical PASS/baseline. A01–A13 остаются обязательными.

## Owner scope amendment (2026-10-01 — model-dependent file tools)

После read-only RECON владелец утвердил подробную запись и commit/push плана:
добавить native `edit`/`write` как в OC2 TS, оставлять `apply_patch` для совместимых
GPT-моделей и **обязательно** убирать несовместимые tools из следующего provider
request, если пользователь переключил модель. Контракт —
[T50/R1/R9](docs/goals/2026-09-27-native-tool-parity.md), срезы —
[M8](roadmap/M8.md#model-dependent-file-tools--t50r1r9-approved-2026-10-01-pending).

- Перенести точный case-sensitive donor predicate по выбранному `model.id`:
  содержит `gpt-`, не содержит `oss` и не содержит `gpt-4` → только `apply_patch`;
  иначе → только `edit`/`write`. Затем effective policy/config/capabilities могут
  сузить набор. Native имя `apply_patch(patchText)` сохраняется вместо donor `patch`.
  Это узкое исключение для tool selection, не список production model IDs,
  reasoning allowlist, guessed capability, изменение discovery или provider routes.
- T50/R1/TOOL12 владеет одним выбранным request-view для preflight budgets,
  каждого follow-up, root/own-model child, управляемой tool guidance и fingerprints.
  После GPT → non-GPT → GPT следующий request получает совместимые schemas/guidance;
  restart, DCP и `/compact` не воскрешают устаревшие определения. Уточнение live-switch
  ниже заменяет ошибочную фиксацию модели на весь turn: captured selection сохраняет
  подготовленный request с его tools, следующий request той же задачи принимает
  committed выбор. Calls/results не удалять/переименовывать; alien opaque continuation
  не переносить на новую модель.
- T50/R9/TOOL20 владеет реальными filesystem effects: write create/overwrite/empty/
  missing parents, edit unique/replaceAll и donor matching precedence/CRLF/BOM.
  Общие permissions/grants, truthful previews, approved-preimage recheck, no-follow
  paths/data-root/protected paths, durable intent/outcome и no unknown-effect replay
  обязательны для всех трёх tools. Две schemas без executor/admission не закрывают scope.
- T45/R6/R10/PRM01 потребляет выбранный набор для profiles/Plan/base/custom/child
  prompt и capability previews; T44/R4/VIS35 — отдельные Write/Edit/ApplyPatch cards,
  их replay и full paired styled-cell/PNG qualification, VIS36 — общие approval
  previews. Минимальные backend slices, не circular whole-task done-dependencies.

Superseded только универсальный patch/no-write-edit запрет в прежних Q03/Q04,
TOOL03/R1/VIS35 и относящийся к нему запрет model-name tool selection. Strict patch
grammar, native trust/policy narrowing, immutable raw history, bounded hot state,
прежний A09 read/apply_patch/bash coding flow и все A01–A13/live gates сохраняются.
TOOL12–TOOL20 принадлежат только T50; это не новый tracker, existing detailed
owners, historical PASS/audits/baseline и execution statuses неизменны. Реализация и
новая qualification pending/NOT_RUN; план не завершает T50 и не снимает PAUSED T44.

## Owner clarification (2026-10-01 — OC2 live model-switch parity)

Владелец потребовал **паритет переключения моделей во время работы OC2 TS**,
после read-only RECON утвердил развёрнутую правку плана и commit/push. Источник —
тот же pinned donor, appended U95–U102 в `tui-recovery/SOURCES.json`; подробный
контракт и срезы — T50/R1/TOOL12 и T45/R10/PRM01, метод — `docs/TEST_PLAN.md`.

- **Draft ≠ commit.** Picker во время busy меняет local composer draft, scoped к
  session/agent (до сессии — Location/agent), а не немедленно модель runtime.
  Отправка обычного сообщения/команды подтверждает captured выбор; пустой Enter
  в обычном composer существующей сессии тоже делает commit без нового user text.
  Session-model event/ack reconciles draft; ошибка не выдаётся за успешный commit.
- **Busy не запрещает committed switch.** Выбор model/variant принимается через
  существующий authorized owner, не очередь «только после завершения turn».
  Следующий LLM request/step той же автономной задачи перечитывает committed выбор,
  заново разрешает admission/budget/tools/guidance/fingerprints и совместимую историю.
  Retry/compaction rebuild используют тот же request-preparation boundary, не
  дополнительный retry слой или разрешение повторять неизвестные effects.
- **Prepared request остаётся captured.** Его stream, route/model/variant, context
  revision, advertised tools, tool execution/approval и фактическая attribution не
  отменяются и не перепривязываются от изменения выбора. Уже выданный A `apply_patch`
  не становится недопустимым только потому, что выбрана B с `edit/write`; новое
  несовместимое исполнение проверяется по snapshot выдавшего его request.
  Permissions/Deny/Plan/parent-child/path/preimage checks сохраняются.
- **Нет потери выполненной работы.** Продолжение B получает retained compatible
  call/result groups и подтверждённые outcomes A без удаления/translation/reexecution.
  Incompatible opaque state/checkpoint исключается; raw history и bounded latest
  hot projection сохранены, forgotten archive не загружается из-за смены модели.
  Selection event, request identity и assistant attribution — разные факты; нельзя
  переписать старые A requests/footers текущим B. Commit сам не запускает генерацию,
  если естественного continuation уже нет. Restart восстанавливает committed выбор,
  не подтверждает draft и не повторяет tools.
- **Qualification.** TOOL12 доказывает GPT → non-GPT → GPT внутри одной работающей
  задачи с provider/tool/approval barriers, без нового пользовательского prompt.
  Draft-only, blank Enter, captured submission ordering, old-request tool completion,
  next-request catalog/budget/wire/history и retry/compact/restart — части одного
  bounded actual-binary scenario. T44 VIS09/VIS29/VIS17/VIS35/VIS36 отдельно проверяет
  picker/composer, реальные request footers/cards/approval и paired styled frames.

Superseded только busy-refusal model/variant selection, whole-turn **model** pinning
и несовместимая с этим трактовка fresh-lane, теряющая retained tool results. Config/
Location generation, session/child/read-only authority, provider/credential admission,
response-close-before-tool-admission, one execution owner, retry limits/quarantine и
no unknown-effect replay неизменны. Это не новый model-facing selector tool или
general steering/agent-switch/config-reload task. TOOL12 остаётся только T50,
PRM01 только T45, VAR01 ordering только T47; новых IDs/tasks/dependency cycles нет.
Первый file-tools план в `760d54f7d` не квалифицирует это уточнение. Dirty R8 и T55
safe-handoff priority сохраняются; T50 active, T44 PAUSED, historical PASS/statuses
не переписываются. План pending, не runtime или visual PASS.

## Исполнение

Исполнение не привязано к GPT, модели, provider или CLI. Любой compatible coding agent, удовлетворяющий контракту `docs/AGENT_RUNBOOK.md`, может продолжать работу в выделенном worktree. Модель/CLI authoring-agent не являются частью product config и не выбираются через `OC_TEST_MODEL`. Не обещать завершение за фиксированное число суток. Остановки при rate limit/компакции/crash должны оставлять продолжимый worktree, а не стирать незавершённую работу.
