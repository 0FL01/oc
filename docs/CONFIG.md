# Конфигурация и пользовательский профиль

## Разделение файлов

Upstream-compatible inputs: global/Location `opencode.json`/`opencode.jsonc`, `.opencode`, ordered `AGENTS.md`, local skill/agent/command definitions, `dcp.jsonc/json`. Global `cli.json` — отдельный read-only TUI domain: он не является project config и не участвует в runtime-config precedence. Native controls: `oc-rs.toml`. Никаких runtime writes в исходные configs, auto-install packages или credential migration.

В supplied примере provider snippet был фрагментом с лишними завершающими скобками/запятыми. `examples/opencode.jsonc` — заново собранный валидный полный пример, не побайтная копия; URL/keys остаются env references. Модели в основном примере не зафиксированы. Static модели из Q01 сохранены отдельно как fixture `examples/static-models.user.json` с отметкой «пользовательские metadata, не проверенные публичные спецификации».

Discovery native enabled для ludka2; static ludka можно включить и явно задать models. `enabled_providers` в стартовом примере содержит только ludka2, чтобы отсутствие LUDKA_* не ломало его работу. Это явно выбранный ready-to-start default, не удаление поддержки ludka.

## Source loading

Baseline discovery/precedence получить из pinned OpenCode config tests и сохранить source-derived fixture в T02. `G` — `$OPENCODE_CONFIG_DIR`, если задан, иначе platform XDG OpenCode config root; env override заменяет default, а не добавляет второй global layer. Для admitted sources low→high: `G/opencode.json`, `G/opencode.jsonc`, optional explicit `--config`, direct `opencode.json` затем `opencode.jsonc` от Location root к working directory, затем `.opencode` roots от Location root к working directory с JSON перед JSONC. Более широкий upstream walk за trusted Location boundary — documented difference, не скрытое чтение parent/home filesystem.

Pipeline: source discovery/provenance → explicit trust decision для canonical Location/source boundary → text substitutions/no-follow resource reads → JSONC parse → normalize supported shapes → domain-specific merge → typed capability validation → fully built candidate generation → atomic publication. До trust нельзя читать `{file:...}`, разрешать lower-trust endpoint/command с higher-trust credential или запускать network/process. Fatal candidate/reload failure сохраняет прежнюю generation; mixed old/new config, instructions, catalogs, plugins или clients не публикуются. T46/T51 target допускает fully admitted candidate с isolated optional-service failures и typed diagnostic inventory, не half-valid policy. Подстановки выполняются ДО parsing согласно pinned recon, не переносить старую ошибку плана v1.

Missing required selected-provider key даёт typed `MissingCredential`: локальные TUI/history/model picker работают, но запрос выбранной недоступной модели fails до generation/tool effects; headless nonzero. Нет fallback provider/model. Отсутствующие env references обрабатываются по pinned substitution contract (пустое значение); публичный strict config assembler сохраняет MissingCredential contract, application допускает valid-shaped empty credential только с central pre-request refusal. Disabled provider/MCP не запускает network/process и не требует своих credentials. Это НЕ обещает lazy file substitution: доверенный config с {file:...} может читать ссылку до entry filtering; trust gate применяется ко всему источнику. Пустая подстановка не является действительным ключом и не выводится как secret в effective config. Не вводить второй противоречащий parsing pipeline ради пропуска неиспользуемых env keys.

Cold TUI discovery выполняется асинхронно существующим application owner. `TuiChrome.provider` и read-only Settings показывают отдельно request availability (`ready/pending/unavailable/failed`) и catalog attempt. Первый native attempt остаётся `pending` для requests даже с configured metadata: известная строка не обходит ещё не завершившийся auth check. `ready` означает admitted credential + metadata выбранного model ID после этого attempt, не проверенную connection и не обход остальных admission/caps. Configured/persisted ID сохраняется явно; unavailable model в UI имеет owner-derived opaque identity, не raw URL/path/control string. Native discovery 401/403 блокирует requests текущего effective wire binding; timeout/connect/catalog failure сохраняет configured metadata, а явный выбор известной модели может использовать исправный Responses endpoint.

Admitted `/reload` повторяет только существующий bounded discovery. Failed refresh сохраняет предыдущую complete generation/catalog/selection и безопасный latest-attempt cause; success не сохраняет удалённые remote IDs через local overrides. Mandatory config syntax/type/security и retained-selection validation остаются atomic gates. Public embedded/headless spawn ждёт bounded catalog result; TUI запускает local owner до завершения optional discovery. Headless preflight существующего root читает его scoped Current selection; недоступный config default не заменяет и не блокирует другую явно сохранённую admitted session choice.

JSONC comments/trailing commas принимаются; диагностика показывает qualified opaque source и schema-only field, не исходный parser payload/path. Unsupported arbitrary plugin/provider package даёт safe field и capability code. Не «поддерживать» настройку только тем, что Serde её проглотил. Security-relevant invalid candidate не публикуется; malformed отдельная definition исключается с typed cause. Explicit selected-reference failure остаётся явным; configured default eligibility/fallback governed by approved R6, not the old blanket default-reference hard error.

DCP domains, provider model metadata/variants, MCP entries, skills, agents, commands и permissions имеют отдельные merge rules. Native own settings не переопределяют смысл известных upstream fields. Источник и shadowed origins доступны в `oc config explain`, secrets и sensitive absolute paths redacted.

## Service config/startup — native diagnostic contract

[T46 R6 matrix](goals/2026-09-22-mcp-attach-parity.md#r6-fieldcapability-matrix--approved-target-not-supported-claim)
defines legacy `mcp.<name>` and canonical `mcp.servers.<name>`, global/server stage
timeouts, enabled inversion/precedence, actual argv/cwd/environment and explicit
OAuth/CodeMode/protocol differences. Admission is per server after source trust;
malformed/unsupported entry stays in bounded failed diagnostic inventory while valid
siblings survive. Unknown non-security donor metadata may be omitted, not recognized
unsupported behavior or malformed policy. Valid disabled entries remain disabled and
zero-spawn even with environment/cwd/timeout; loader support is not proven by Serde alone.

Admitted local MCP inherits product-process env plus configured string-map overlay;
relative cwd resolves from effective Location workspace directory, not source file.
Full environment/credential authority requires command/resource/credential-domain
admission before launch; untrusted/lower-trust commands do not inherit higher-trust
secrets. Ordinary shell keeps its minimal env; remote MCP gets no arbitrary local env.
New env values/inherited secrets join redaction; never dump expanded effective config.

[T51](goals/2026-09-27-startup-fault-isolation.md) uses the existing `ServiceDiagnostic`
for typed source/field/service/stage/safe-code/retryability/allowed-action facts across
loader, application, TUI and headless. Source qualifiers are stable opaque IDs with an
allowlisted config filename; entry identities are opaque, schema fields never retain
provider/MCP/definition/env names. Remote exceptions, expanded values and parser/panic
payloads are not diagnostic values. Public strict config assembly keeps its legacy
error API; native application boundaries expose the safe projection.

Standalone definition files and admitted inline definition namespaces are the narrow
optional authority: invalid siblings have bounded failed inventory while healthy
definitions and completely admitted policy survive. Whole malformed/unreadable JSONC
cannot prove that it contains no policy and stays fatal, even above healthy global
policy. Selected mandatory definitions, trust/security, storage/recovery, cleanup and
resource caps remain non-success; cancellation remains cancelled. Failed reload keeps
the previous complete generation, not mixed catalogs/instructions/clients/policy.
Corrupt active stored selections do not fall back to defaults; a broken inactive tab
preference may only be projected read-only with a visible safe diagnostic.

Syntactically valid saved references to a removed primary agent, retired pinned
model or disabled variant remain exact in the selection owner. Home and restored
active/parked tabs show an opaque `SelectionReadiness` and safe cause; history,
pickers and drafts stay local. No sibling/default selection is executed or silently
persisted: the next turn or headless request refuses before acceptance and wire/tool
effects. Explicit admitted agent/model/Default variant choices repair only their
selected scope through the existing prefs store; a Home agent choice is keyed by
Location. Deny and the mandatory source/policy admission are unchanged. This
distinguishes a missing/nonprimary saved agent (`AgentUnavailable`) from an existing
primary's unresolvable model pin (`ModelUnavailable`): model-only repair retains that
primary's admitted body and constraints. A malformed mandatory default or permission
definition stays fatal; it is not permission to borrow built-in authority. This
offline inherited-source fixture is R4A; the separate real-user HOME/XDG check
required for E2E06 remains a later R4B gate.

Read-only Settings shows bounded common diagnostics and omitted counts. Enter opens
safe details; Ctrl+Shift+C copies the selected diagnostic and Ctrl+Shift+I shows an
unsent investigation note, preserving the ordinary prompt. Fatal native frames retain
the category plus precise Stage/code, Source, Field and action; this is a declared
native presentation difference, not donor pixel qualification. Headless diagnostics
stay on stderr, errors are nonzero, and successful `--json` stdout stays NDJSON only.

Owner-approved T44/VIS42 presentation target (2026-10-02, pending/NOT_RUN):
[clean dialogue and service diagnostics](../tui-recovery/T44_CONTRACT_AMENDMENT.md#clean-dialogue-and-service-diagnostics).
Nonfatal background config/plugin/provider/MCP diagnostics are not conversation rows.
Use compact status/counts and a brief notification for a new or changed problem;
unchanged refresh/reload/parked-view updates remain silent. Provider pending is a
connection status, not a request error; recovery clears its stale status. Settings,
`/plugins` and `/mcps` retain actual failed inventory and safe details/copy/investigate.
Opaque IDs, source/field/stage/code/retryability belong in those details, not the normal
dialogue or an expanded technical toast. Do not replace opaque values with unsafe raw
config identities, add a hide-errors config switch, change headless stderr/NDJSON,
or suppress genuine requested-operation failures/fatal boundaries. Existing T51
backend completion does not verify this new T44 presentation contract.

Selected-model/provider readiness is separate from local availability. Discovery
deadlines/retries/metadata and last-healthy publication remain intact. Async MCP
startup/status/control and cleanup use the existing T46 resource owner. Native worker
joins preserve typed cleanup/cap causes and discard unsafe legacy/panic payloads.

## MCP/permissions compatibility — approved 2026-10-02; external access pending

OC1 host config is an example; pinned OC2 v2.0.12 is the normalization oracle.
[T46/MCP09 follow-up](goals/2026-09-22-mcp-attach-parity.md#mcppermission-config-follow-up--approved-2026-10-02-pending)
keeps the completed backend baseline separate from new qualification. Canonical root
`mcp.servers` uses `disabled`, not `enabled`; legacy enabled:false migrates to
disabled:true. Moving entries without inversion is not equivalent. Scalar positive-ms
timeout migrates only to catalog/execution, not startup; defaults are30,000/30,000/
43,200,000 ms. Global supplied timeout leaves merge, per-server leaves override;
later same-ID server replaces the whole entry, canonical wins mixed-root conflicts
with a safe diagnostic. Typed legacy servers literally named servers/timeout remain
distinct from canonical container/defaults keys.

Canonical **direct-mode example** below preserves the supplied URLs/headers/argv/env
and numeric timeout intent. `codemode:false` explicitly selects direct OC2 transport;
it is not an automatic migration rewrite. Native omitted codemode remains direct,
unlike donor default CodeMode query/fallback behavior. crw's URL gets no extra `/mcp`;
oauth:false means no discovery, while native omitted OAuth is also no-OAuth (declared
difference). Native protocol omitted/legacy negotiates up to2025-11-25; unsupported
auto/2026-07-28 remains a per-entry capability failure. Disabled local chrome has zero
npx/browser/spawn/network effects. Local cwd/env inherit only the admitted product
execution context and overlay, not authoring-agent configuration.

Canonical `permissions` is an ordered array. The legacy `permission` map is normalized
before it, preserving rule order; later matching Allow overrides broad Deny **inside
that authority only**. Independent native central/source/profile/parent ceilings and
tools:false remain, not donor's unrestricted cross-layer last-Allow behavior. Malformed
security policy is fatal, not skipped into Allow. MCP aliases come from actual tools/list:
server crw + scrape → crw_scrape, crw + crw_scrape → crw_crw_scrape. `codex_web` matches
that exact action, not all codex_web tools; explicitly use codex_web_* if that is the
intended server-wide rule. Do not silently widen the user's literal rules.

```jsonc
{
  "mcp": {
    "servers": {
      "codex_web": {
        "type": "remote",
        "url": "{env:LUDKA2_API_URL}/mcp",
        "oauth": false,
        "codemode": false,
        "headers": { "Authorization": "Bearer {env:LUDKA2_API_KEY}" },
        "timeout": { "catalog": 60000, "execution": 60000 }
      },
      "crw": {
        "type": "remote",
        "url": "{env:CRW_API_URL}",
        "oauth": false,
        "codemode": false,
        "headers": { "Authorization": "Bearer {env:CRW_API_KEY}" },
        "timeout": { "catalog": 30000, "execution": 30000 }
      },
      "chrome-devtools": {
        "type": "local",
        "command": ["npx", "-y", "chrome-devtools-mcp@latest", "--browser-url", "http://127.0.0.1:9222"],
        "disabled": true,
        "environment": { "npm_config_offline": "true" },
        "timeout": { "catalog": 3000, "execution": 3000 }
      }
    }
  },
  "permissions": [
    { "action": "codex_search", "resource": "*", "effect": "deny" },
    { "action": "codex_web", "resource": "*", "effect": "allow" },
    { "action": "crw_*", "resource": "*", "effect": "deny" },
    { "action": "crw_crw_*", "resource": "*", "effect": "deny" },
    { "action": "crw_scrape", "resource": "*", "effect": "allow" },
    { "action": "crw_crw_scrape", "resource": "*", "effect": "allow" },
    { "action": "external_directory", "resource": "~/.cargo/registry/src/*", "effect": "allow" },
    { "action": "external_directory", "resource": "~/.local/lib/python*/site-packages/*", "effect": "allow" },
    { "action": "external_directory", "resource": "~/.cache/uv/archive-v0/*/lib/python*/site-packages/*", "effect": "allow" },
    { "action": "external_directory", "resource": "~/go/pkg/mod/*", "effect": "allow" },
    { "action": "read", "resource": "*", "effect": "allow" },
    { "action": "glob", "resource": "*", "effect": "allow" },
    { "action": "grep", "resource": "*", "effect": "allow" }
  ]
}
```

These external patterns are not merely accepted syntax: [T50/R3/R5 target](goals/2026-09-27-native-tool-parity.md#external-readsearch--r3r5tool14tool16-approved-2026-10-02-pending)
must perform real bounded read/glob/grep outside Location. Effective Allow for the
home-expanded external directory resource **and** actual tool action executes without
Ask, approval event, saved grant, manual pre-opening or per-path trust dialog, first
time and after restart. Genuine Ask still needs the normal consumer; Deny/independent
ceilings/no-follow/data-root/cancel/budgets still refuse or bound access. Boundary Allow
alone is not read-only policy and does not grant mutations or arbitrary tools; it never
adds a config/AGENTS source or starts an automatic HOME crawl. At RECON native ordinary
outside-root tools still refuse: this external-access example is a pending target, not
current runtime support. Existing T45 default MCP Allow/effective-view target remains
separate from these literal configured restrictions; no executable examples or user
files are rewritten by this plan.

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
| Compression | native `compress.enabled` default true target; `compress.mode:range`, `permission:allow\|ask\|deny`, `minContextLimit`/`maxContextLimit` positive tokens or valid `X%`, `summaryBuffer`, `showCompression` |
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
effects need current evidence. The percentage-default slice of DCP12 proves
default/threshold/config-resolution behavior, not every control/display outcome;
the approved effective-control supplement below separately qualifies its flags/routes.

Owner-approved 2026-10-02 [effective controls](DCP.md#compression-switch-and-effective-config-controls--t45r9dcp12-approved-2026-10-02-pending)
extend T45/R9/DCP12, implementation/qualification **pending**:

- `compress.enabled:false` is a native hard off for new model/manual/API compression;
  omission defaults true. It preserves permission choice, existing committed summaries
  and immutable history, never overrides global off/Deny/child opt-out or Ask.
- Disabled manual requests fail before Submit/provider. Manual mode instead allows
  owner-admitted explicit `/dcp-compress` with a scoped typed trigger, not arbitrary
  prompt text; autonomous calls/nudges remain off. Commands false really disables
  native `/dcp` and `/dcp-compress`; debug true enables only safe metadata diagnostics.
- Current `compress.enabled` is silently ignored, commands/debug effects are unwired,
  and manual mode hides the tool that the current command asks the model to call.
  Typed notification/channel/showCompression consumers exist; full VIS38 remains pending.
- Keep admitted native root order and array replacement as declared donor differences;
  protect-tool scopes/defaults and nudge timing need executable fixtures, not parser-only
  parity claims. Unknown nested compress keys must get safe source/field diagnostics.
  Examples/schema/help change with implementation; no user config rewrite or updater.

Host OC1 pin `@tarquinen/opencode-dcp@3.2.0` is a **pending** exact compatibility
target, not a current admitted alias or baseline change. Admission/provenance follow-up
is T51/CFG09/CFG10; actual `experimental.allowSubAgents` behavior remains T45/R9/
DCP10–DCP12. A package spelling alone never qualifies child DCP or full3.2.0 parity.

## Custom providers — T53 (approved clarification 2026-10-02; pending)

The [T53 config/auth/wire contract](goals/2026-09-29-opencode-go-and-provider-auth.md)
admits both OC1 legacy `provider` and OC2 canonical `providers` through one normalizer.
Host config is an example; pinned OC2 schema/migration/requests are the oracle. This
section describes the target, **not implemented Chat/Messages/canonical-config support**.

- Legacy npm/options/model id/provider.npm/api/modalities/tool_call/interleaved/variant
  maps normalize to package/settings/headers/body, API modelID, capabilities,
  compatibility.reasoningField and array variants. Canonical provider/model/variant
  supplied overlays merge in admitted source order; request order is provider → model
  → selected variant, recursive settings/body and case-insensitive headers. Model
  package overrides provider; catalog reference differs from API `modelID ?? id`.
  Conflicting connection/auth values in mixed legacy/canonical roots in one document
  receive a safe pre-effect diagnostic, a declared native conflict policy.
  Canonical capabilities are tools/input/output; legacy reasoning has pinned
  unsupported diagnostic, not an invented capability or implicit effort controls.
  Legacy provider api overrides options.baseURL per pinned migration; remote catalog
  api metadata is not an executable endpoint override.
- Finite aliases `@ai-sdk/openai` → Responses, `@ai-sdk/anthropic` → Messages,
  `@ai-sdk/openai-compatible` → Chat, plus their generic pinned OC2
  `@opencode/ai/providers/{openai,anthropic,openai-compatible}` package identities.
  They select Rust adapters, never npm downloads/JS host or Responses→Chat fallback.
  Explicit unknown package or recognized unimplemented wire setting is diagnosed.
- Custom configured auth input takes priority over scoped stored Key. Messages
  `apiKey` sends x-api-key; static `authToken` sends Bearer and does not imply OAuth.
  Both together, competing auth headers or explicit None with auth input conflict.
  None must be explicit; localhost and absent key never infer it. Go preset precedence
  stays separately frozen; remote metadata/env never authorizes foreign credentials.
- Explicit trusted/admitted localhost **and LAN** HTTP(S) connections are scoped to
  normalized scheme/host/port/base prefix and source provenance. The same immutable
  binding admits discovery/main/follow-up/title/compaction/child/retry; lower-trust
  overrides do not inherit allowance/secret. Peer/DNS/redirect restrictions remain
  scoped, not globally disabled. This does not widen webfetch/tool-network policy.
- Keep configured static models and source-qualified dynamic retirement semantics;
  compatible alias alone never starts guessed discovery. Existing OpenProxy `/models`
  oracle and Go public models.dev cache remain distinct, with one catalog read-view.
  Thinking/effort/textVerbosity/reasoning_content and dialect max-token/cache fields
  require actual protocol lowering, not only retaining metadata. Vision input/text
  output remain; image-generation/audio/video/PDF are not added.
- Preserve native timeout:false/no-overall-deadline, numeric timeout/chunkTimeout
  milliseconds with validation and idle policy plus cancellation/byte caps. OC2's
  native package strips these core settings; the old SDK wrapper is not native parity.
  Keep setCacheKey compatibility control, but move key identity from request-body hash
  to session/fork-root lineage; Responses prompt_cache_key, capability-gated Chat and
  explicit Messages cache_control (max four breakpoints) have separate lowering.
  No Go-only headers/cache policy leaks into custom bindings.

Qualification uses synthetic host-shaped configs, not copied secrets: legacy/canonical
equivalence, API-ID/overlay precedence, real tool roundtrips/variants, private-route
admission, errors/cancel and protocol-switch/reopen. GO01–GO06 only T53; PRM01 consumes
chronological system/effort and T50's next-request switch, without new tasks/frameworks.
Snapshot false aliases remain T44 conversation-only target; compaction.auto works,
legacy compaction.prune remains diagnosed unsupported, not imported OC1 pruning.

## Tool-output configuration — T50/R10/TOOL21 (approved 2026-10-01; pending)

Frozen [tool-output contract](goals/2026-09-27-native-tool-parity.md#tool-output--r10tool21-approved-2026-10-01-pending)
adds a root upstream-compatible section in admitted `opencode.json`/`opencode.jsonc`.
At RECON HEAD8a4291d13 native `Generation`/loader ignore this section: this is an
approved target, **not current configurable support**. Do not add it to executable
examples or rewrite user configuration to imply otherwise.

```jsonc
{
  // Target configuration; both fields are optional, defaults shown.
  "tool_output": {
    "max_lines": 2000,
    "max_bytes": 51200,
  },
}
```

- Both fields are positive integers: no0/negative/fraction/string/overflow, null,
  disabled/unlimited sentinel or malformed section. Units are logical lines and UTF-8
  bytes of admitted text, not characters/tokens/media. Default byte count is50 KiB,
  not50,000. No per-agent/per-tool override or new retention/quota key in this section.
- Use existing global/explicit/Location/.opencode source order and provenance, but
  **last defining section replaces the whole section**, not field-wise deep merge.
  Earlier `{max_bytes:4096}` then later `{max_lines:40}` gives40/51200. Later `{}`
  restores2000/51200; an absent section does not shadow an earlier definition. Expose
  selected section/shadowed origins and defaulted fields without expanded secrets.
- Reject `max_bytes` above native64 KiB served-text safety ceiling with safe source/
  field diagnostics; positive line values never remove independent byte bounds.
  Preview body uses both configured caps, plus a separately bounded reserved artifact/
  count/status notice within total served-text64 KiB. Tiny limits can give empty body
  plus useful reference; selected request/model admission still applies. Further
  native limiting is explicit, not silent unlimited/clamping or a false full result.
- General text previews keep head, shell tail; joined text parts share one budget,
  terminal newline adds no synthetic line, huge lines clip on UTF-8 boundaries with
  explicit continuation/loss facts. Existing producer `truncated` metadata does not
  waive the common check. Image/media, HTTP/MCP/input and structured-control caps are
  separate; a file reference does not bypass them or turn unsupported media into success.
- Parse into the same typed immutable config generation. Publish only the complete
  admitted candidate; invalid reload retains the previous generation and effective
  settings. Existing safe-boundary reload affects newly admitted turns/jobs, not
  in-flight prepared results or running background jobs. Model/variant switch is not
  config reload; stored previews/references/counts are not recomputed on reopen.

Oversized admitted normalized/redacted text is retained by the existing native
Db owner under `tool-output/`, with a model-readable exact registered path. Initial
artifact ceiling16 MiB, existing shared storage quota default2 GiB and completed
artifact TTL7 days are native safety differences, **not donor `tool_output` fields**
or implemented TOML controls. Capture loss/IO/quota/expiry is explicit; no full-inline
fallback, producer replay, arbitrary native-root access or automatic cold rehydration.
TOOL21 qualifies config → bounded provider result → filesystem → authorized read/grep
continuation, reusing TOOL13/TOOL16/AUD34/LOAD02/STORE04. Parser success, a UI2048-byte
preview and old SQLite continuation alone do not qualify this behavior.

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

**Built-ins/MCP amendment, approved 2026-10-01; pending:**
[T45 contract](goals/2026-09-21-config-compat-and-subagents.md#built-in-profiles--default-mcp-access--approved-2026-10-01-pending)
seeds `build`/`plan` primary and `general`/`explore` subagent before configured transforms,
without creating `.opencode` files. Supplied fields override under existing R6 merge/
authority rules; omitted mode/system must not erase the builtin profile. Hidden/disabled
and automatic primary/subagent eligibility remain distinct from explicit addressing.
Custom `plan` without real Plan policy/reminders is not implementation of this target.

For configured/enabled/admitted/connected MCP, all registered tools get native default
Allow in the shared permission owner before root/child narrowing, including Plan and
both built-in subagents. No per-server/tool allow config is required by default; explicit
central/parent/profile Ask/Deny/resources/tools:false remain authoritative. This is not
`permission:"allow"` for all built-ins, auto-enabling a disabled MCP or admitting unknown
actions. Use actual registry identities and existing compatible aliases for both schemas
and invocation. Explore's default MCP exception is native, not donor blanket-deny parity;
its native file/shell/question/delegation constraints and own-history compress gates stay.
MCP tools are not inferred read-only, and Plan/Explore are not external-effect sandboxes.

Plan must reconcile enter/leave after DCP/compact/Revert/reopen, forbid ordinary native
file edits, narrowly admit explicitly requested plan files under `~/.opencode/plan`, and
exit only on agent selection. It consumes both selected T50 file families and common
default MCP policy; `run --agent plan` belongs to R6. No new config keys, writer or generated
profile files are introduced. Executable examples change only when implementation lands;
this approval does not prove runtime support or change progress statuses.

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

Approved 2026-10-02 follow-up: exact3.2.0 may enter both classifier and DCP resolver
only after source-delta/config/semantic qualification and AGPL provenance review,
with truthful requested/current compiled identities. Current3.1.15 revision, latest
alias semantics, unknown-version rejection and historical PASS remain unchanged.
See [T51 follow-up](goals/2026-09-27-startup-fault-isolation.md#dcp-320-compatibility-follow-up--approved-2026-10-02-pending).

R1/CFG09 admission isolates `UnsupportedPlugin` to the rejected request. The existing
Core catalog exposes a bounded current-generation inventory in `TuiChrome.plugins`:
opaque stable requested identity, compiled current identity only for active aliases,
typed status/module, safe source/field and `ServiceDiagnostic`. Settings shows these
read-only facts before the first prompt and after reload/reopen; headless diagnostics
stay on stderr. All requests are classified even beyond the presentation window.
Known native aliases and definitions survive; the ignored authoring marker has no
current module. The exact classifier remains the pre-resolution gate. Compiled binding
is independent of the typed provider readiness above; shared fatal diagnostics use the
same safe source/field contract without converting trust/storage failure to plugin status.

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

`oc`/`oc tui`: local TUI, no daemon. `oc run <prompt> --model <provider/model-id> [--variant name] [--json]` — headless. `oc sessions list`, `oc run --session ID <prompt>` — история/продолжение. **`oc models`** (без обязательного `list`) — catalog-only список точных IDs. `oc config check` и `oc config explain` остаются проектируемыми командами; наличие модели в каталоге не подтверждает generation readiness.

### CLI models и привязка профиля

[T50/R7/TOOL18](goals/2026-09-27-native-tool-parity.md#cli-models--r7tool18-approved-2026-10-01-pending)
предоставляет `oc models`: полный список включённых моделей поддерживаемых
admitted каталогов как точные `provider/model-id`, один ID на строку, стабильный
лексикографический порядок. Пустой исправный каталог — пустой stdout/exit 0;
diagnostics — stderr, incomplete required dynamic/fatal/output failures — nonzero.
Нет family-collapse или default 20-row limit из отдельного `opencode_models` tool.
Native listing доступен до выбора default model, без paid generation, session/prefs/
config mutations или MCP/browser startup; metadata visibility ≠ request readiness.
Static/public metadata не требует generation key, authenticated OpenProxy discovery
требует ключ admitted endpoint и сохраняет existing oracle/budgets/retirement.

Pinned OC2 `opencode models` имеет ServerParams, а не OC1 positional provider/
`--verbose`/`--refresh`. Прежнее проектируемое `oc models list [--refresh]` superseded;
native server/standalone/JSON/filter/bind flags и compatibility alias не добавляются.
Existing global `--data-dir` принимается; catalog-only команда не открывает native
database/data-root, не читает saved preferences и не запускает recovery.
Например: `target/debug/oc --data-dir /path/to/native-data models`.

Целевой путь пользователя: скопировать ID из `oc models`, затем вручную указать его
в global `model`, canonical `agents.<id>.model` либо Markdown profile
`.opencode/agents/<id>.md`. Для JSON legacy `agent` проходит donor normalization;
это не каноническое имя OC2 и не повод удалить существующую совместимость.
Пример **target profile**, не claim текущего полного R6 parity (значения placeholders):

```yaml
---
mode: primary
model: provider/model-id
---
Инструкции профиля.
```

T45/R6 квалифицирует `model: provider/model-id#variant`/structured selection и
совместимое отдельное `variant`: embedded/structured native choice не подменяется
отдельным legacy полем. Первый `/` отделяет provider; дальнейшие `/` сохраняются
в model ID. CLI не добавляет вариант к каждой строке, не пишет/биндит профили и не
переключает модель. Existing command/child model settings используют тот же exact
reference под своими контрактами. Actual ID → профиль → request → reopen/restart
проверяется T45 на реальном TOOL18 CLI receipt; unavailable choice отказывает до
effects без silent fallback. T53 подключает future Go/custom catalogs через тот же
read-view после minimal catalog slice, без whole-task prerequisite. CLI receipt:
`evidence/T50/cli-models.md`; profile qualification остаётся T45/R6 pending.

### Общие CLI controls — проектируемый target

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

Initial safety defaults предложены этой редакцией, меняются осмысленным config/decision и не являются измеренными performance budgets. Per-event 4 MiB, model tool args 2 MiB, serialized request 24 MiB (ниже observed proxy cap 32 MiB), preview 64 KiB, retained tool output 16 MiB, active queue 8 MiB, blob quota 2 GiB, max turns 128/запуск. Не выделять все capacity upfront. T50/R10 отдельно freezes tool-output body defaults2000/51200, served-text ceiling64 KiB, artifact ceiling16 MiB/shared2 GiB/TTL7 days и readable registered artifacts; это pending план, не реализация всего TOML limits parser и не замена актуальных transport caps.

Превышение explicit limit видно как error/truncated + counters; history не silently удаляется для продолжения. Адмиссия контекста отдельно учитывает input/output/model limits. Не забывать input base64 expansion у attachments.

## Config capability report

На конец M2 historical domains/differences фиксируются в `evidence/T07/compatibility.md`;
к финалу — current evidence in FINAL.md. Bounded Location discovery, direct MCP,
canonical mutation permission identity, native authority narrowing, exact native mappings, strict security,
read-only inputs, own storage/CLI and resource bounds remain explicit differences.
Primary-only/no-child/executable-command exclusions are superseded only by approved
T45/R3/R6–R10; pending subagent/context/prompt/DCP extensions are not supported claims.
Unsupported audio/video/pdf, CodeMode/OAuth/arbitrary npm remain outside scope.

The historical universal apply_patch/no-write-edit difference is superseded only
by the 2026-10-01 [T50/R1/R9 file-tools contract](goals/2026-09-27-native-tool-parity.md).
Selected-model predicate changes schemas/managed guidance, not configured model IDs,
reasoning/discovery/provider routes. Existing write/edit/patch permission aliases still
normalize to apply_patch and preserve explicit Deny, grants and profile ceilings.
No new file-tool override setting or executable sample is added in this plan delivery;
supported implementation/compatibility claims require TOOL12/TOOL20 qualification.
