# Решения после Q01–Q07

## Подтверждено владельцем

Q01 закрыт: provider family `@ai-sdk/openai` через собственный OpenProxy, bearer/env config, ludka static и ludka2 dynamic metadata. OAuth и остальные provider implementations не нужны. Точные live endpoint/key не запрашивать в документах: они подключаются в среде.

Q02 закрыт: DCP functional port обязателен, codex_web remote bearer/no OAuth, Chrome stdio integration с default disabled. Пользовательский discovery script — часть required behavior, а не временная справка.

Q03/Q04 закрыты для daily-direct: read/file mutation/webfetch/shell, MCP, compress + reminders, исходный TUI product direction сохраняется. Один built-in apply_patch вместо write/edit для всех моделей. Это намеренное отличие, не ошибка parity.

Q05 по runner/полномочиям уточнён: authoring-agent не привязан к GPT, модели, provider или CLI. Compatible agent работает в non-root account с dev tools; YOLO-подобные режимы, rootless Docker, git commit/push разрешены только в пределах runbook. Конкретный monetary cap/время watchdog владелец не указал. Не выдавать отсутствие cap за unlimited budget approval или блокировать все offline работы из-за незаполненного старого вопросника.

Q06 закрыт по target: пользовательский report Debian 13.4 x86_64 GNU, 6 vCPU AMD EPYC, около 11.68 GiB RAM, ext4, kernel 6.12.86, active rustc/cargo 1.98.1. Это report владельца; он не доказывает фактическую среду будущего процесса. Проверяется preflight. Наличие root в показанном prompt и rootful overlay не отменяет заявленный будущий non-root/rootless запуск.

Q07 закрыт: fresh start, no migration, no publishing. Требуется запускаемый результат cargo build. Разрешение push исходников не означает разрешение release/service deployment.

Configured workspace добавлен владельцем в первый daily-driver release: global/local config и AGENTS, `.opencode`, skills, primary agents, commands и известные native mappings DCP/OpenProxy. Arbitrary JS/TS/npm plugins явно не требуются и не исполняются.

## Defaults этой редакции, не ответы владельца задним числом

D01: четыре packages и небольшой typed application API; бинарник oc, namespace oc-rs. Local TUI/headless; serve/attach отложены.

D02: один native Responses adapter, explicit projection/store:false, no previous_response_id в первом scope. Основан на OpenAI package/default family и observed proxy route; exact replay проходит wire spike. Chat Completions не добавлять «на всякий случай».

D03: DCP range core и базовая панель; experimental message mode/custom prompts/subagents вне goal. Required source-derived fixtures обязательны. Automatic strategies не удалять только ради минимального demo.

D04: explicit direct exposure в native profile. Отсутствующий codemode field имеет direct semantics в этом профиле, explicit true отклоняется. Так устраняется прежний blocker на пропущенном ключе в пользовательском MCP config без скрытой подмены true→false.

D05: один data-root owner, SQLite worker, sequential tool execution; no distributed locks/frameworks. История на диске, DCP — projection.

D06: AGPL-3.0-or-later-compatible путь для прямого производного DCP-порта. До первого push такого кода получить LICENSE pinned DCP, сохранить notices/атрибуцию и ясно обозначить лицензию соответствующей производной работы. Не переименовывать DCP в MIT. Пакет не делает правового заключения о любых будущих способах сочетания кода; при конфликте лицензий затронутый push блокируется, unrelated local work продолжается. У OpenProxy проверенный GitHub metadata не определяет лицензию; не копировать его реализацию без отдельного основания, достаточно protocol/reference tests.

D07: dev build jobs=2, test threads=2, target/tmp на диске worktree, не /tmp tmpfs. Rootless Docker — optional инструмент, не требование запуска oc. Начальные safety byte caps и bounded live test envelope заданы runbook, не утверждены как пожелание к latency/RAM.

D08: рабочая ветка `agent/oc-rust-port` от текущего согласованного checkout; обычный push только origin этой repo, без force/merge в default. Владелец разрешил push, безопасная ветка — выбранная реализация этого разрешения.

D09: нет большого монолитного progress log. Один маленький state, current handoff, indices и immutable factual leaves. No RAG, embeddings, database или новый agent framework для планирования.

D10: одна immutable config generation на turn. Skills snapshot-ятся bounded при построении generation и раскрывают body только через native tool result; primary agents могут только сужать central policy; commands проходят ровно один durable SubmitInput. Location switch выбирает Location-scoped session, не перепривязывает активную session.

D11: known plugin compatibility — обычный exact enum/match по capability kind, не generic registry. T07 классифицирует и отклоняет unknown до side effects; T14/T19 связывают marker с compiled discovery/DCP. Exact marker не исполняет JS-файл и не меняет authority пользовательского discovery snapshot.

D12: `oc-tui` зависит от read-side `oc-adapters` (models select/admit, config explain/skills, storage paged reads + `tui.*` prefs). DAG сохраняется: `core <- adapters <- tui <- oc`; TUI не порождает network/process, storage writes только pref-ключи, Db handle lifecycle остаётся в binary. Чистый `core`-only TUI не может показать picker/history/workspace без дублирования доменной логики.

D13 (2026-09-22, инструкция владельца о паритете с opencode v2.0.12): MCP attach перестаёт быть фатальным для turn. Enabled-сервер, который не подключился или не отдал каталог, получает per-server деградацию (`mcp <id> <stage>: <code> (retryable=<bool>)`) и не публикует инструменты; turn выполняется дальше, деградация видима (TUI note, headless stderr). Основание: upstream `packages/core/src/mcp/index.ts` ведёт статусы `pending|connected|disabled|failed|needs_auth` и продолжает сессию; прежнее правило «MCP attach failure stays fatal» из spec T43 (C3) отменяется этой записью, старый текст не переписывается. Остаются фатальными: `Cancelled`, ошибки cleanup/`McpShutdown`, `MAX_MCP_SERVERS`, generation-капы каталога; AUD23 reaping ранее подключённых серверов сохраняется. Последствия: тесты `mcp_attach_failure_is_loud`/`aud23_partial_attach_failure_*` и v01-сценарии переписываются под новый контракт (turn завершается + warning виден + нет утечки child), а не удаляются. Отдельно: outbound HTTP-клиенты (MCP remote, provider generation, discovery, webfetch) отправляют `User-Agent: oc/<version>` — JS runtime upstream всегда отправляет UA, `reqwest` по умолчанию нет, из-за чего Cloudflare перед crw endpoint отвечал `403 Error 1010`.

D14 (2026-09-22, backend audit / owner-authorized bug repair): отсутствие положительных model limits не является причиной безусловного отказа turn. DISC05 и owner discovery oracle не меняются: отсутствующие metadata остаются unknown. `provider.<id>.options.nativeFallbackLimits` задаёт local request policy (defaults context=32768, output=4096; context > output+1024); это не заявленная capacity модели и не wire extension. Positive metadata ограничивает request; fallback используется только для неизвестных полей и даёт видимый warning. Output request по умолчанию равен configured output, explicit request clamped к known output либо fallback output; input ceiling = min(positive model.input, effective context-output)-1024. Каждый provider round, child и title проходит admission; title сохраняет запрос 256, но ограничивается тем же selected-model budget. Нет selected variant — нет overlay, explicit enabled variant валидируется. Это исправляет T15 implementation gap относительно `docs/CONTRACTS.md`, не заменяет DISC05. Evidence: `evidence/T47/report.md`.

D15 (2026-09-22): permission resource maps сохраняют порядок правил, last matching rule внутри source, unmatched resource требует approval; отсутствие central action — deny. Независимые central/agent/child constraints пересекаются, не расширяют authority. Native `ask` не исполняет side effect без отсутствующего пока approval channel. MCP successful structured/textual resource output санитизируется до history/provider; arbitrary remote error text остаётся закрытым, structured error codes дают фиксированные безопасные категории. Initialize instructions — bounded permission-filtered provider projection, не immutable history. Media-output, prompts/resource catalog и interactive approval остаются открытыми parity-различиями, не новыми owner waivers. Evidence: `evidence/T43/backend-permissions.md`, `evidence/T46/backend-parity.md`.

D18 (2026-09-23, решение владельца): поддержка OpenCode Zen полностью удалена из продукта. Удалены `zen_catalog.rs`, `zen_chat.rs`, actual-binary suite `zen_free.rs`, вся wiring в config/composition/runtime/application/provider/lib, scope-документ `docs/goals/2026-09-23-zen-free-chat.md`, регистрация T48 и acceptance ZEN01–ZEN03; `GOAL.md`, `docs/TEST_PLAN.md` и planning-реестры возвращены к состоянию до Zen. Основания: free tier провайдера закрыт для любого не-OpenCode клиента (`403 FreeTierError`, подтверждено ограниченными пробами), а единственный известный обход — подмена идентичности — запрещён. Ранее оформленные под этот scope решения (D16/D17) сняты вместе с ним; общий принцип сохраняется: `oc` не мимикрирует под другие клиенты и не обходит access-контроль провайдеров, в том числе через shell или subagent. Реальные live-прогоны остаются на OpenProxy; платный Zen key, OpenCode Go или локальный OpenAI-compatible сервер возможны только как новый отдельно утверждённый scope. История: `evidence/T48/report.md`, `evidence/T48/free-tier-gate.md`, `evidence/T48/removed.md`.

### Owner-approved T44 permission follow-up (2026-09-26)

VIS36 в T44 amendment закрывает открытый interactive approval gap D15 обязательной
реализацией owner request/reply, typed rejection, project Always, original UI и
autoaccept/config/Settings/CLI dependencies. Это утверждение плана, не executed PASS.
Ask user-approvable; effective Deny и structural central/agent/child/trust ceilings
сохраняются. Saved grants не переписывают restrictive policy. No-channel headless
ApprovalRequired остаётся; explicit supported --auto требует реального once-consumer.
Новые policy/UI различия не скрывать под claim identical donor algebra. Не добавлять
новый task/framework или dependency на completion всего T43/T45.

## Остаточные prerequisites, не новые Q

Secrets/connectivity/наличие нужной live модели проверяются just-in-time в T16/T27, не в T00. Missing → конкретный external blocker, не угадывание credentials. Docker проверяется только перед первым использованием и иначе `NOT_USED`. Exact versions Cargo dependencies/rmcp protocol/TLS проверяются compile spike. Реальная browser-служба требуется только для opt-in smoke. Monetary hard limit внешнего authoring-agent не задан; документация его не исполняет. Semantic DCP качества проверяются fixtures/live task, а не декларацией.
