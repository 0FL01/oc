# Решения после Q01–Q07

## Подтверждено владельцем

Q01 закрыт: provider family `@ai-sdk/openai` через собственный OpenProxy, bearer/env config, ludka static и ludka2 dynamic metadata. OAuth и остальные provider implementations не нужны. Точные live endpoint/key не запрашивать в документах: они подключаются в среде.

Q02 закрыт: DCP functional port обязателен, codex_web remote bearer/no OAuth, Chrome stdio integration с default disabled. Пользовательский discovery script — часть required behavior, а не временная справка.

Q03/Q04 закрыты для daily-direct: read/file mutation/webfetch/shell, MCP, compress + reminders, исходный TUI product direction сохраняется. Историческое решение «один built-in apply_patch вместо write/edit для всех моделей» superseded узким owner amendment D22 ниже; остальные ответы не переоткрываются.

Q05 по runner/полномочиям уточнён: authoring-agent не привязан к GPT, модели, provider или CLI. Compatible agent работает в non-root account с dev tools; YOLO-подобные режимы, rootless Docker, git commit/push разрешены только в пределах runbook. Конкретный monetary cap/время watchdog владелец не указал. Не выдавать отсутствие cap за unlimited budget approval или блокировать все offline работы из-за незаполненного старого вопросника.

Q06 закрыт по target: пользовательский report Debian 13.4 x86_64 GNU, 6 vCPU AMD EPYC, около 11.68 GiB RAM, ext4, kernel 6.12.86, active rustc/cargo 1.98.1. Это report владельца; он не доказывает фактическую среду будущего процесса. Проверяется preflight. Наличие root в показанном prompt и rootful overlay не отменяет заявленный будущий non-root/rootless запуск.

Q07 закрыт: fresh start, no migration, no publishing. Требуется запускаемый результат cargo build. Разрешение push исходников не означает разрешение release/service deployment.

Configured workspace добавлен владельцем в первый daily-driver release: global/local config и AGENTS, `.opencode`, skills, primary agents, commands и известные native mappings DCP/OpenProxy. Arbitrary JS/TS/npm plugins явно не требуются и не исполняются.

## Defaults этой редакции, не ответы владельца задним числом

D01: четыре packages и небольшой typed application API; бинарник oc, namespace oc-rs. Local TUI/headless; serve/attach отложены.

D02: один native Responses adapter, explicit projection/store:false, no previous_response_id в первом scope. Основан на OpenAI package/default family и observed proxy route; exact replay проходит wire spike. Chat Completions не добавлять «на всякий случай».

D03: DCP range core и базовая панель; experimental message mode/custom prompts/subagents вне goal. Required source-derived fixtures обязательны. Automatic strategies не удалять только ради минимального demo.

D04: explicit direct exposure в native profile. Отсутствующий codemode field имеет direct semantics в этом профиле, explicit true отклоняется. Так устраняется прежний blocker на пропущенном ключе в пользовательском MCP config без скрытой подмены true→false.

D05: один data-root owner, SQLite worker, sequential tool execution; no distributed locks/frameworks. История на диске, DCP — projection. D19 supersedes sequential execution only for independent subagent calls/background jobs; serial non-child mutation ordering remains.

D06: AGPL-3.0-or-later-compatible путь для прямого производного DCP-порта. До первого push такого кода получить LICENSE pinned DCP, сохранить notices/атрибуцию и ясно обозначить лицензию соответствующей производной работы. Не переименовывать DCP в MIT. Пакет не делает правового заключения о любых будущих способах сочетания кода; при конфликте лицензий затронутый push блокируется, unrelated local work продолжается. У OpenProxy проверенный GitHub metadata не определяет лицензию; не копировать его реализацию без отдельного основания, достаточно protocol/reference tests.

D07: dev build jobs=2, test threads=2, target/tmp на диске worktree, не /tmp tmpfs. Rootless Docker — optional инструмент, не требование запуска oc. Начальные safety byte caps и bounded live test envelope заданы runbook, не утверждены как пожелание к latency/RAM.

D08: рабочая ветка `agent/oc-rust-port` от текущего согласованного checkout; обычный push только origin этой repo, без force/merge в default. Владелец разрешил push, безопасная ветка — выбранная реализация этого разрешения.

D09: нет большого монолитного progress log. Один маленький state, current handoff, indices и immutable factual leaves. No RAG, embeddings, database или новый agent framework для планирования.

D10: одна immutable config generation на turn. Skills snapshot-ятся bounded при построении generation и раскрывают body только через native tool result; primary agents могут только сужать central policy; commands проходят ровно один durable SubmitInput. Location switch выбирает Location-scoped session, не перепривязывает активную session.

T50/D20 adds a distinct durable same-session move; ordinary Location switch and
in-flight immutable generation remain as above. Only permanent binding is superseded.

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

### D19 — owner-approved prompt/subagent/context/DCP plan (2026-09-27)

[T45 R3/R6–R10](goals/2026-09-21-config-compat-and-subagents.md) supersedes only
the child-DCP exclusion in D03, sequential execution for independent child calls
and background jobs in D05 (not non-child tool/mutation ordering), and the
primary-only/fixed-instruction interpretation of D10. Original decisions/evidence
remain historical; one data-root owner, immutable generation, skill tool-result
contract, SQLite durability and authority narrowing are unchanged.

- Foreground is default but independent child calls overlap; explicit background
  progresses while parent is active. Bounded session-owned jobs/notices and safe
  restart replace a parent-terminal-only queue. Donor recovery is at-least-once;
  native unknown side effects never automatically replay.
- Optional context_message_ids attaches exact parent text/roles as quoted user data,
  not system authority. Validate branch/revision/cutoff/budgets before admission;
  immutable durable snapshot, independent stable IDs and effective capability preview
  distinguish automatic child profile/AGENTS/tools/skill metadata from supplied history.
- Child DCP defaults true with false/off/manual/Deny gates, narrow Explore compress
  and session isolation. Protect active task/pack, release on completion and restore
  for safe recovery; old packs do not become permanent fixed lanes.
- No lifetime compress-call/block quota; repeated recompression must avoid aging
  depth/full-archive-load exhaustion while preserving graph/protections/replay and
  active memory/model/turn guards. No existing cumulative cap was found; historical
  OC1 failure cause is unverified, not falsely attributed to a removed counter.
- Shared base/custom system/environment/tool guidance and initial/dynamic AGENTS
  lifecycle follow pinned OC2 within native trust/tool boundaries. R7 host extension
  is retained. Explicit excluded donor features are not smuggled into parity.

All are pending implementation; new scenarios have one owner T45. No change to task
status, historical PASS, A10 baseline thresholds, mandatory workspace/live gates or
T44 approval authority. Native compaction and DCP are independent context mechanisms.

### D20 — selected native tools and explicit exclusions (2026-09-27)

Owner-approved [T50 R1–R8](goals/2026-09-27-native-tool-parity.md) adds real Linux
command-shell/background notices, donor search options, question, read directories/
images, webfetch formats/timeout and direct model/session controls. Keep canonical
shell plus legacy bash compatibility under one permission/execution owner. Question
is user input, not approval; --auto never answers it. New controls require real
target authorization, not model-description authority or silent missing-action allow.

Built-in websearch and its provider integrations are intentionally omitted. Code
Mode/execute remains excluded, including shims/remote interpreter emulation. Explicit
MCP search/browser stays opt-in and catalog-derived; no built-in browser or PDF.
Native opencode_* tools expose existing owners directly, not a Code Mode dependency.

Only permanent session→Location binding is superseded: admitted same-session move
validates destination and applies on a safe boundary, preserves ID/history and yields
source execution before fresh destination generation. Ordinary SelectLocationSession
is unchanged. Immutable in-flight context, original background/child provenance,
parent-child ceilings, source trust, MCP quarantine/cleanup and unknown-effect safety
remain; no file snapshots, family migration or second storage/scheduler framework.

T50 owns TOOL12–TOOL19; AUD14's permanent-binding clause is narrowly amended with
the original audit/evidence retained. Add todo state without changing existing task
statuses or PASS. T45/T46/T44 remain independently owned, no done-dependency cycle;
current request/effect/PTY/resource/workspace/live evidence is required before claims.

### D21 — service config/startup/error isolation (owner-approved 2026-09-27)

After RECON the owner approved [T46 R6/R7](goals/2026-09-22-mcp-attach-parity.md),
[T51 R1–R3](goals/2026-09-27-startup-fault-isolation.md) and T44 VIS19/VIS40/VIS42.
Optional MCP config/connect/catalog, plugin admission/setup and selected-provider
discovery/connect readiness cannot kill otherwise admitted TUI/history/model picker.
Failed services retain typed safe status/diagnostics before first prompt; no raw-error
or keyword/regex classification. Selected provider/model never silently falls back;
unavailable request is actionable non-success, headless remains nonzero.

Supersedes only app-wide rejection for an optional failed capability in D11/old
loader, lazy first-turn MCP attach and T37/MCP04's minimal local-MCP env/cwd policy.
Legacy/canonical MCP forms normalize per pinned donor field matrix; admitted local
MCP inherits **product-process** env + configured overlay and resolves relative cwd
from Location workspace. Command/resource/credential-domain admission is required
before inheritance; no lower-trust secret capture or external runner-auth extraction.
Ordinary shell TOOL05/AUD28 stays minimal. Env values/inherited secrets are redacted.

Use existing generation/client/application owners for async MCP startup and bounded
status/actions; publish tools at safe request boundaries. Invalid recognized entry is
failed, valid disabled remains zero-spawn; OAuth/CodeMode/unsupported protocol are
per-server capability errors, not pretend support. Security-critical malformed policy,
trust/storage/data-root/recovery, cancellation, cleanup/McpShutdown and caps stay
non-success. Reload preserves the previous healthy complete generation on fatal
failure; no partial unsafe policy or mixed state. Discovery oracle/budgets/atomic
catalogs and remote sticky quarantine/no unknown-effect replay stay unchanged.

This is pending plan delivery: MCP09/MCP10 only T46, CFG09/CFG10/UI07 only T51,
VIS42 only T44 (with startup transitions in existing VIS19/VIS40). No second registry/
daemon/store/framework, whole-task completion cycle, historical PASS rewrite or new
OAuth/JS host. Historical T37 env evidence remains factual but does not qualify R6.

### D22 — model-dependent file tools (owner-approved 2026-10-01; pending)

После RECON владелец утвердил подробный план и commit/push:
[T50/R1/R9](goals/2026-09-27-native-tool-parity.md),
[ordered slices](../roadmap/M8.md#model-dependent-file-tools--t50r1r9-approved-2026-10-01-pending).
Нативные `edit`/`write` добавляются с OC2 schemas/файловой семантикой. Точный
case-sensitive predicate `model.id.includes("gpt-") && !model.id.includes("oss") &&
!model.id.includes("gpt-4")` выбирает только `apply_patch`; иначе только `edit`/`write`,
после чего effective policy/capabilities сужают exposure. Имя `apply_patch` сохраняет
нынешний ordinary function `patchText` contract, не provider-hosted tool.

Обязателен совместимый следующий request при пользовательской смене модели в обе
стороны: tools/catalog/автоматическая guidance, preflight budgets и fingerprints
строятся из одного selected view для root/own-model child и follow-ups. История
calls/results неизменна; no tool translation/reexecution или alien opaque replay.
Общие legacy permission identity/grants, approval preview/preimage recheck,
descriptor-safe paths/protections, durable confirmed effects и resource caps остаются.

Superseded только Q03/Q04's single-patch/no-edit-write exclusion, такой же clause
TOOL03 и запрет model-name **file-tool selector** в действующих T45/T44 contracts.
Strict apply_patch matching не становится fuzzy; no production model allowlist,
reasoning-name inference, discovery/protocol/provider routing change или JS host.
TOOL12/TOOL20 имеют owner T50; PRM01 остаётся T45, VIS35/VIS36 — T44, без новых
tasks/store/framework/paid campaign или whole-task dependency cycle. Старые audits/
evidence/PASS и A09 patch flow сохраняются. Plan-only delivery не меняет active T50,
PAUSED T44 или остальные statuses и не является реализацией/qualification PASS.

## Остаточные prerequisites, не новые Q

Secrets/connectivity/наличие нужной live модели проверяются just-in-time в T16/T27, не в T00. Missing → конкретный external blocker, не угадывание credentials. Docker проверяется только перед первым использованием и иначе `NOT_USED`. Exact versions Cargo dependencies/rmcp protocol/TLS проверяются compile spike. Реальная browser-служба требуется только для opt-in smoke. Monetary hard limit внешнего authoring-agent не задан; документация его не исполняет. Semantic DCP качества проверяются fixtures/live task, а не декларацией.
