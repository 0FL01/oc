# Конфигурация и пользовательский профиль

## Разделение файлов

Upstream-compatible inputs: global/Location `opencode.json`/`opencode.jsonc`, `.opencode`, ordered `AGENTS.md`, local skill/agent/command definitions, `dcp.jsonc/json`. Global `cli.json` — отдельный read-only TUI domain: он не является project config и не участвует в runtime-config precedence. Native controls: `oc-rs.toml`. Никаких runtime writes в исходные configs, auto-install packages или credential migration.

В supplied примере provider snippet был фрагментом с лишними завершающими скобками/запятыми. `examples/opencode.jsonc` — заново собранный валидный полный пример, не побайтная копия; URL/keys остаются env references. Модели в основном примере не зафиксированы. Static модели из Q01 сохранены отдельно как fixture `examples/static-models.user.json` с отметкой «пользовательские metadata, не проверенные публичные спецификации».

Discovery native enabled для ludka2; static ludka можно включить и явно задать models. `enabled_providers` в стартовом примере содержит только ludka2, чтобы отсутствие LUDKA_* не ломало его работу. Это явно выбранный ready-to-start default, не удаление поддержки ludka.

## Source loading

Baseline discovery/precedence получить из pinned OpenCode config tests и сохранить source-derived fixture в T02. `G` — `$OPENCODE_CONFIG_DIR`, если задан, иначе platform XDG OpenCode config root; env override заменяет default, а не добавляет второй global layer. Для admitted sources low→high: `G/opencode.json`, `G/opencode.jsonc`, optional explicit `--config`, direct `opencode.json` затем `opencode.jsonc` от Location root к working directory, затем `.opencode` roots от Location root к working directory с JSON перед JSONC. Более широкий upstream walk за trusted Location boundary — documented difference, не скрытое чтение parent/home filesystem.

Pipeline: source discovery/provenance → explicit trust decision для canonical Location/source boundary → text substitutions/no-follow resource reads → JSONC parse → normalize supported shapes → domain-specific merge → typed capability validation → fully built candidate generation → atomic publication. До trust нельзя читать `{file:...}`, разрешать lower-trust endpoint/command с higher-trust credential или запускать network/process. Failure сохраняет прежнюю generation; mixed old/new config, instructions, catalogs, plugins или clients не публикуются. Подстановки выполняются ДО parsing согласно pinned recon, не переносить старую ошибку плана v1.

Missing required selected-provider key приводит к MissingCredential, а не fallback provider. Отсутствующие env references обрабатываются по pinned substitution contract (пустое значение), но обязательность credentials проверяется только для выбранных/enabled integrations после normalization. Disabled provider/MCP не запускает network/process и не требует своих credentials. Это НЕ обещает lazy file substitution: доверенный config с {file:...} может читать ссылку до entry filtering; trust gate применяется ко всему источнику. Пустая подстановка не является действительным ключом и не выводится как secret в effective config. Не вводить второй противоречащий parsing pipeline ради пропуска неиспользуемых env keys.

JSONC comments/trailing commas и source locations сохраняются для diagnostics. Unsupported arbitrary plugin/provider package даёт имя config field и capability, которой не хватает. Не «поддерживать» настройку только тем, что Serde её проглотил. Security-relevant invalid candidate не публикуется; malformed отдельная definition исключается с path/field/reason. Explicit selected-reference failure остаётся явным; configured default eligibility/fallback governed by approved R6, not the old blanket default-reference hard error.

DCP domains, provider model metadata/variants, MCP entries, skills, agents, commands и permissions имеют отдельные merge rules. Native own settings не переопределяют смысл известных upstream fields. Источник и shadowed origins доступны в `oc config explain`, secrets и sensitive absolute paths redacted.

## DCP configuration — approved target

Native DCP already reads optional `dcp.json`/`dcp.jsonc`; no automatic creation or
rewriting. Within each admitted root, inline `dcp` precedes standalone JSON, then
JSONC. The existing standalone layer order is `G` → Location project root → its
admitted `.opencode`, with later supplied fields overriding earlier fields. `G`
is nonempty OPENCODE_CONFIG_DIR, otherwise XDG_CONFIG_HOME/opencode, otherwise
HOME/.config/opencode; the override replaces, not duplicates, the default root.
Read-only regular UTF-8/no-follow/1 MiB and general trust/generation rules remain.
`cli.json` is not a DCP source; no wider ancestor search, npm host or updater.

Approved T45/R9/DCP12 omitted-field defaults are compress.minContextLimit=`"40%"`,
maxContextLimit=`"55%"`, summaryBuffer=false; frequency5/iteration15/soft unchanged.
They are native policy additions, not pinned DCP 3.1.15 defaults. Current compiled
defaults and examples/dcp.jsonc still use explicit 50000/100000/true until the
corresponding implementation; do not use that sample to qualify no-config defaults
or overwrite existing user settings. Semantics/evidence: [DCP.md](DCP.md#approved-percentage-defaults--t45r9dcp12-pending).

Target configurable groups (qualification is required, parsing alone is not support):

| Group | Fields and values |
| --- | --- |
| Core | `enabled`, `debug` (safe metadata only); `$schema` is metadata |
| Compression | `compress.mode:range`, `permission:allow\|ask\|deny`, `minContextLimit`/`maxContextLimit` positive tokens or valid `X%`, `summaryBuffer`, `showCompression` |
| Model policy | `compress.modelMinLimits`/`modelMaxLimits` exact provider/model maps; native `compress.modelOverrides` with minContextLimit/maxContextLimit/nudgeFrequency; no model-ID hardcoding |
| Reminder policy | `compress.nudgeFrequency`, `iterationNudgeThreshold`, `nudgeForce:soft\|strong` (`hard` is not a config value) |
| Protection | `compress.protectedTools`, `protectTags`, `protectUserMessages`, top-level `protectedFilePatterns` |
| Commands/manual | `commands.enabled`/`protectedTools`, `manualMode.enabled`/`automaticStrategies` |
| Turn/strategies | `turnProtection.enabled`/`turns`, `strategies.deduplication.enabled`/`protectedTools`, `strategies.purgeErrors.enabled`/`turns`/`protectedTools` |
| Display | `pruneNotification:off\|minimal\|detailed`, `pruneNotificationType:chat\|toast`; detailed/chat and showCompression=false defaults stay |
| Children | `experimental.allowSubAgents` default true target, explicit false child-only opt-out under T45/R9/DCP10 |

Existing legacy root limits/manualMode boolean/purgeAfterTurns and native
modelOverrides aliases retain their declared normalization; recommend canonical
nested fields. Effective min≤max validation follows model-context resolution.
Enabled/manual/module policy never widens central Deny/ask or parent-child ceilings.
Explicit autoUpdate:true, customPrompts:true and message mode remain unsupported;
false does not enable an updater/prompt engine. Child gates are pending R9, and
typed display modes/showCompression are pending T44/VIS38. Commands/debug are not
qualified merely by accepted fields or old snapshot tests; their required runtime
effects need current evidence. DCP12 proves default/threshold/config-resolution
behavior, not completion of all these other outcomes.

## Instructions и definitions

`AGENTS.md` — не config override. Effective instructions состоят из canonical-deduplicated `G/AGENTS.md`, затем applicable Location files в pinned nearest-working-directory-to-root order; distinct sentinel каждого admitted файла входит ровно один раз с provenance. Unreadable/disappeared file даёт diagnostic и не сохраняет текст из старой candidate. Reload возможен только между turns.

T45/R10 target adds the OC2 initial baseline/chronological changed or removed
instructions and successful-read nested AGENTS lifecycle. Reconcile after
compaction/Revert/reopen, without stale content or mixed in-flight generations.
Trust/canonical Location boundaries remain native differences from broader donor
walks; this is not permission to read arbitrary ancestors/home directories.

Automatic definitions читаются только из `G` и admitted `.opencode` roots: `{skill,skills}/<id>/SKILL.md`, `{agent,agents}/<id>.md`, `{command,commands}/<id>.md`. Singular root обрабатывается перед plural в одной source directory; inline JSON/JSONC declaration — перед Markdown definitions этого root. Exact order/collisions закрепляются fixture, а не filesystem enumeration. T45/R6 admits recursive agent/agents IDs and one-level compatibility mode/modes per pinned importer; legacy-mode exclusion is superseded only for these profiles. Flat skill files, `.claude`/`.agents`, URL/remote/package skills и symlink escape remain unsupported.

Skill ID выводится из canonical directory ID; supported frontmatter — bounded `name`/`description`, body — instructions. Later source целиком заменяет duplicate ID с shadowing provenance. При построении generation body проверяется как regular no-follow file, bounded и snapshot-ится вместе с digest; model catalog получает только bounded id/name/description. После publication файл повторно не открывается.

Approved T45/R6 target replaces the old primary-only subset: Build/Plan/General/Explore,
custom Markdown primary/subagent/all and typed system/model/variant/request/steps/color
follow pinned importer/merge/default semantics. Configured default_agent uses donor
visible-primary fallback; an explicit invalid selection still fails, and a disappeared
pinned session selection is diagnosed rather than silently widening authority.
Native central/parent/child permissions only narrow. Unsupported arbitrary hooks/env
or other unimplemented fields do not become silently accepted. Historical subset
evidence is not qualification of these pending outcomes.

Command body/description, `$ARGUMENTS`/positional expansion and durable original/expanded
input remain. T45/R3 supersedes subagent/subtask and child-background exclusions with
pinned agent/model/route precedence and background child admission; supported shell
interpolation follows the pinned contract, never runs in the loader or again on replay.
Reserved collisions, arbitrary recursive/direct tool execution and unsupported
file/URL/late-env includes remain explicit failures. Omitted context_message_ids
preserves OC2 delegation; the optional model-tool field attaches exact quoted parent
text, not config instructions or automatic parent-history inheritance (R8).

## Native plugin classification

До resolver/import/process/network классифицируются только exact identities. Bare `@tarquinen/opencode-dcp`, pinned `@tarquinen/opencode-dcp@3.1.15` и пользовательский exact alias `@tarquinen/opencode-dcp@latest` обозначают один compiled DCP module фиксированной repository revision: `@latest` здесь НЕ вызывает registry resolution и не меняет revision. Exact canonical `<effective-config-root>/{plugin,plugins}/openproxy-models.js` обозначает один compiled OpenProxy discovery module; файл не читается и не исполняется. Exact `@prevalentware/opencode-goal-plugin@0.1.49` из общей authoring-конфигурации даёт warning и нулевую runtime capability: код пакета не загружается. Basename вне admitted root, `.ts`, URL/arbitrary path, ranges, другие versions/packages и любой unknown JS/TS дают source-qualified `UnsupportedPlugin`. Duplicate aliases idempotent. Provider alias `@ai-sdk/openai` остаётся отдельным config domain, не plugin identity.

## Generation и limits

Session имеет текущий Location; один turn держит immutable `(LocationId, ConfigGenerationId)` и selected-agent digest до завершения. Reload/agent selection только между turns. Обычный Location switch полностью строит target generation и выбирает/создаёт Location-scoped session, не retarget-ит активную. T50 explicit admitted session_move supersedes только прежнюю пожизненную привязку: сохраняет session ID/history и применяет validated destination на safe boundary после durable terminal исходного turn; destination request относится к отдельному следующему turn. Config/agent/Location change открывает новую provider causality generation и не повторно использует incompatible opaque continuation items. Original operations/background jobs/children сохраняют execution context/provenance; move не сбрасывает permissions или unknown-effect MCP quarantine.

Pending [T50 tool target](goals/2026-09-27-native-tool-parity.md) exposes canonical
shell(command/workdir/timeout/background), with explicit legacy bash(argv/cwd/
timeout_ms) compatibility through one policy owner. No second interchangeable model
schema or alias-based Deny bypass. Search/read/fetch options and native question/
opencode_* controls require real runtime semantics, not silent config acceptance.
Built-in websearch/provider integrations, Code Mode/execute and built-in browser are
excluded; configured MCP search/browser remains opt-in, permission-gated and catalog-
derived. PDF stays outside scope. Do not modify runtime config examples before the
corresponding implementation exists or claim old grants automatically authorize
new session-control actions/destination resources.

Ограничить source count, admitted directory depth/path length, parsing nesting,
total generation и served/model-visible bytes. R2's removed artificial per-file/
frontmatter/name/description caps are not reintroduced; its documented total/serving
guards remain. Over-limit serving outcome явный; no silent skill advertisement for
uncallable body. TUI получает redacted projection без credentials, expanded secrets,
full skill bodies и sensitive paths.

## Commands — проектируемый CLI

`oc`/`oc tui`: local TUI, no daemon. `oc run <prompt> --model <provider/model-id> [--variant name] [--json]` — headless. `oc sessions list`, `oc run --session ID <prompt>` — история/продолжение. `oc models list [--refresh]`, `oc config check`, `oc config explain`, `oc --version`, `oc --help`.

`--config path` и `--native-config path` задают явные источники для tests/запуска. `--data-dir path` изолирует storage. Implement spelling один раз в M1/M2 и синхронизировать fixtures/help, не поддерживать десяток синонимов. Нет TTY без subcommand → usage error. `--json` stdout содержит только NDJSON; пользовательские diagnostics и безопасные notices — stderr.

Production default model: explicit config selection или persisted пользовательский выбор TUI. При отсутствии выбора headless сообщает ModelRequired и доступную команду list; не выбирает «самую умную/первую» автоматически. Test runner selection — отдельная политика TEST_PLAN.

## Native settings и units

T45/R9 planned DCP child default: dcp.experimental.allowSubAgents=true when omitted;
false disables child model compress/anchors/nudges/strategies only. Global enabled/
manual and effective Deny remain authoritative. No lifetime successful-compression
or block-count quota; keep per-call/active-context/model/turn resource bounds.
Existing examples/dcp.jsonc explicitly sets false as an opt-out; it is not the new
default or proof of implementation. Update executable samples/help with actual R9
implementation, not by advertising an unsupported setting as already functional.

Пример TOML — контракт будущего parser, не config существующего upstream. Числа имеют явные units в key. `provider.options.timeout/chunkTimeout` upstream имеют их собственную semantics; не применять discovery milliseconds как generation timeout.

Initial safety defaults предложены этой редакцией, меняются осмысленным config/decision и не являются измеренными performance budgets. Per-event 4 MiB, model tool args 2 MiB, serialized request 24 MiB (ниже observed proxy cap 32 MiB), preview 64 KiB, retained tool output 16 MiB, active queue 8 MiB, blob quota 2 GiB, max turns 128/запуск. Не выделять все capacity upfront.

Превышение explicit limit видно как error/truncated + counters; history не silently удаляется для продолжения. Адмиссия контекста отдельно учитывает input/output/model limits. Не забывать input base64 expansion у attachments.

## Config capability report

На конец M2 historical domains/differences фиксируются в `evidence/T07/compatibility.md`;
к финалу — current evidence in FINAL.md. Bounded Location discovery, direct MCP,
unified apply_patch, native authority narrowing, exact native mappings, strict security,
read-only inputs, own storage/CLI and resource bounds remain explicit differences.
Primary-only/no-child/executable-command exclusions are superseded only by approved
T45/R3/R6–R10; pending subagent/context/prompt/DCP extensions are not supported claims.
Unsupported audio/video/pdf, CodeMode/OAuth/arbitrary npm remain outside scope.
