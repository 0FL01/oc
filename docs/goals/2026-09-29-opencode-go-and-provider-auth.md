# Goal: OpenCode Go, единые credentials и connect/model TUI

Status: active
Source: владелец после RECON и параллельного аудита утвердил план 2026-09-29; «единая логика хранения кред, без деприкейт фич», будущий Codex OAuth и custom llama.cpp/ludka2 учтены как границы общего owner.
Last updated: 2026-10-05
Task: T53 (active; partial backend slices не являются GO01–GO06 PASS).

## Scope boundary clarification (2026-10-06)

[T57](2026-10-06-codex-oauth-and-openai-key.md) now owns the explicitly requested
Codex browser/device OAuth, refresh, ordinary OpenAI key, native auth CLI and shared
functional consumers; T44/VIS45 owns paired auth visuals. The no-Codex/OAuth-execution/
auth-CLI/WS exclusions below still describe **T53**, not a product-wide prohibition
against T57. GO01–GO06 ownership/status and Go/custom credential priority are unchanged.
T57 reuses qualified existing storage/catalog/wire/connect seams, not a second store
or an all-T53/T44 done dependency. Active T53 and PAUSED T44 remain unchanged.

## Objective

Native `oc` поддерживает `opencode-go`: public models.dev catalog, Console API key,
три требуемых wire-протокола, Go request headers и рабочие `/connect` → `/models`
с account management как в pinned OC2 TS. Один credential owner обслуживает Go и
admitted custom providers, не ломая OpenProxy и не требуя фиктивного ключа для
явно anonymous local server. Будущий OAuth помещается в тот же storage contract,
но Codex login/refresh и другие OAuth flows в этой задаче не реализуются.

Owner-approved RECON уточнение 2026-10-02: host OC1 TS config — набор примеров,
не истина поведения. Нормализация и wires следуют pinned OC2 v2.0.12; admitted
custom Responses/Messages/Chat providers поддерживают явные localhost **и LAN**
connections. Свои тонкие adapters поверх existing reqwest/SSE предпочтительны;
provider/agent framework вне scope, узкий crate допустим только при доказанном blocker
и сохранении existing transport/runtime contracts.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: один credential owner и явная auth policy для Go/custom providers.
  - Source: требование владельца о едином non-legacy storage и уточнение про llama.cpp/ludka2/будущий Codex OAuth; donor credential/integration owners ниже.
  - Acceptance: GO01 доказывает общий native SQLite owner, labeled accounts add/activate/rename/remove и restart; tagged Key/OAuth material без `auth.json`/import/dual-write. Auth None отличается от missing Key и unsupported OAuth; Go и custom precedence/endpoint authority соответствуют таблице ниже. Нет автоматической отправки stored/default-env key на новый endpoint. Safe DTO и secret form не раскрывают material.
  - Acceptance clarification (2026-10-02): static Messages `authToken` — Key material с Bearer scheme, не OAuth execution; `apiKey` — x-api-key, оба вместе диагностируются. Explicit admitted localhost/LAN scope действует во всех lanes/discovery, не ослабляя credential/trust/redirect admission.
  - Primary evidence: synthetic-secret owner/storage tests с reopen, transactional activation/removal, precedence/scope refusal и zero unauthorized requests.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R2: public Go catalog из models.dev, независимый от credentials.
  - Source: «модели фетчить с models.dev»; donor models-dev normalization, approved audit simplification.
  - Acceptance: GO02 доказывает bounded fetch/parse/cache/last-good, no-key browsing, finite package-to-protocol mapping и реализованные reasoning overlays. Public refresh не подтверждает key и не снимает request auth rejection; retired/deprecated choice остаётся явно unavailable, без fallback/local resurrection.
  - Acceptance clarification (2026-10-02): source-derived legacy `provider` и canonical `providers` fixtures дают одинаковые normalized connection/model/variant facts и captured wire. Model package/API ID, overlays/headers/body, capabilities/compatibility и variants следуют precedence ниже; custom static/configured discovery и public Go не смешиваются.
  - Primary evidence: source fixtures + fake-clock/client catalog tests, включая coexistence с неизменным OpenProxy discovery.
  - Consumer clarification (owner-approved 2026-10-01 CLI plan): admitted Go/custom metadata is published into the same provider-qualified read-view consumed by T50/R7 `oc models`, TUI and model-facing lookup. Public/no-key visibility does not authorize generation; auth rejection, endpoint scope, retirement and local merge remain unchanged. T50 owns complete ID-only CLI output/TOOL18, T53 GO02 owns catalog fetch/cache and GO05 owns provider-qualified selection. Basic OpenProxy CLI does not wait for whole-T53 or new protocol/live qualification; no second catalog/cache/credential owner or CLI auth/bind command.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R3: три native wires и все Go request metadata lanes.
  - Source: «нюансы интеграции OpenCode Go, например хедеры»; pinned native AI routes и common model-request owner.
  - Acceptance: GO03 доказывает Responses/Chat/Messages text, complete tool roundtrip, reasoning/usage, terminal/error/cancel и повторное использование T54/RET01 typed provider-error policy: bounded pre-output retry либо continuation после записанного partial output, без replay effects. Every Go request lane получает immutable binding/metadata; custom providers переиспользуют admitted adapters, но не Go-only headers. Нет guessed route/model/paid fallback.
  - Acceptance clarification (2026-10-02): exact per-wire options/chronological system/effort/cache/media semantics и synthetic host-shaped custom matrix из TEST_PLAN обязательны, а не только generic text smoke. Каждый adapter call — один physical attempt; Chat/Messages failures интегрируются с существующим retry owner.
  - Primary evidence: parameterized fake wire/runtime tests; captured requests main/follow-up/title/summary-compaction/child/retry и concurrent-session barriers.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R4: protocol-safe durable replay, DCP и forks.
  - Source: существующие immutable history/tool-causality contracts A02/A04/A07/A10 и необходимый multi-protocol boundary.
  - Acceptance: GO04 доказывает protocol/binding preservation через TurnLog, SQL projection, checkpoints и fork/reopen. Absent protocol читается как legacy Responses; explicit unknown protocol fail-safe. Incompatible opaque state не конвертируется/не отправляется другой deployment; raw rows и completed tool pairs сохраняются, unknown effects не переисполняются.
  - Acceptance clarification (2026-10-02): chronology system/effort updates сохраняется через DCP/compact/fork/reopen. Next-request committed switch той же задачи сохраняет ordinary outcomes, но не alien opaque/signatures/checkpoints; prepared requests/tools сохраняют captured identity/view по T50.
  - Primary evidence: owning storage/runtime tests legacy/new logs → projection/DCP/fork → restart/next captured request, с raw-history integrity assertions.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R5: fresh-start `/connect` и provider-qualified model selection.
  - Source: Go authorization «как у опенкод 2 ts (с TUI паритетом)»; donor account/connect/model flow.
  - Acceptance: GO05 доказывает usable TUI/history/connect при отсутствии config/key/catalog, реальное account management и acknowledged connect → Go-filtered picker. Optional exact ModelRef не выбирается автоматически; provider-ID collisions, variants/drafts/tabs/agents и restart/fork сохраняют выбор. Unready submission/headless отказывает до root/turn acceptance, generation/title/tools; malformed mandatory policy/storage/recovery остаются fatal.
  - Primary evidence: actual-binary PTY flow fresh offline → masked/cancelled input → accounts → explicit model → fake generation/cancel → reopen; headless refusal и held-request binding test.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R6: current offline и bounded Go live qualification.
  - Source: API smoke permission владельца; A01–A13 и runbook live envelope.
  - Acceptance: GO06 требует green impacted regressions/workspace gates и bounded real Go text/tool representatives для трёх protocols, плюс проверку известных catalog/docs conflicts. Report различает fake/live/NOT_RUN и exact selection; отсутствующий/revoked key блокирует только live, не превращает его в PASS.
  - Primary evidence: factual evidence/T53/report.md с implementation commits, commands/exits и sanitized durable live-campaign counters.
  - Status: pending
  - Evidence: pending; API smoke не запускался при подготовке плана.

### Credentials, connection config и authority

Connection config принимает legacy `provider.<id>` и canonical OC2 `providers.<id>`
через одну нормализацию до merge/admission: package/protocol, `baseURL`, overlays,
timeouts/cache options и model source. Это не credential store. `/connect` управляет
credentials для известного connection, не создаёт config-authoring UI.

#### Config/model normalization и precedence — GO02/GO03

- Legacy provider `npm/options` → canonical `package/settings`, с `headers/body`
  отдельно; SDK-compatible `extraBody` переносится в body по pinned alias rewrite.
  Admitted legacy provider `api` → settings.baseURL поверх options.baseURL по migrate;
  это local config input, не разрешение исполнять remote catalog `api`.
  Model `id` → `modelID`, `provider.npm/api` → package/settings.baseURL, `options`
  → overlays, `modalities/tool_call` → capabilities.input/output/tools, string/`{field}`
  interleaved → compatibility.reasoningField; boolean interleaved не выдумывает field.
  Legacy `reasoning` получает pinned unsupported diagnostic, не новое capability;
  declared reasoning metadata/options отдельно и не синтезируют effort controls.
  Variant map → ordered array `{id,settings,headers,body}`. Canonical input уже имеет
  эти поля, limit/cost/name/disabled и provider canonical/env metadata.
- Использовать existing admitted source order и provenance, merge supplied fields
  same-ID provider/model и variant overlays, не wholesale replacement provider entry.
  Settings/body recursively merge objects, headers merge case-insensitively. Effective
  request overlay — provider → model → selected variant; model package wins over
  provider package. Catalog/selection ID остаётся отдельным от API `modelID ?? id`.
  T47 ordered view после merge, Default не добавляет variant overlay/inferred effort.
  Auth resolution после overlay использует таблицу ниже, не donor credential priority.
- При одновременных legacy/canonical roots в одном документе не угадывать precedence
  конфликтующих connection/auth fields: safe diagnostic до effects. Это узкая native
  conflict policy; эквивалентность раздельных форм и merge между sources проверяются
  against pinned normalizer, не convenience текущего Rust parser.
- Finite generic packages: `@ai-sdk/openai` и `@opencode/ai/providers/openai` → Responses;
  `@ai-sdk/anthropic` и `@opencode/ai/providers/anthropic` → Messages;
  `@ai-sdk/openai-compatible` и `@opencode/ai/providers/openai-compatible` → Chat.
  Explicit unknown package unsupported до effects; omitted package сохраняет existing
  admitted default. Compatible alias **не** делает auto Responses→Chat fallback;
  другой wire требует нового explicit admitted package/binding, не probe/host/model guess.
- Static custom catalogs сохраняют local-only IDs. Configured dynamic source использует
  свой admitted discovery owner/source identity и successful-refresh retirement; наличие
  compatible package не разрешает guessed `/models` probe. Existing ludka2 `/models`
  oracle и Go public models.dev cache различны; raw remote api/env/headers не executable.
- Validate recognized settings/compatibility и overlays на владельческой границе;
  неподдержанная executable настройка имеет safe diagnostic, не silent Serde success.
  Reserved auth/identity/stream/model/tool fields не обходят captured binding, policy,
  validation/caps через headers/body. Provider `env` metadata не создаёт foreign fallback.

#### Auth и endpoint authority

- Один owner в существующем native Db; минимальная additive таблица:
  `id, provider_namespace, label, tagged_value_json, active, created_at`.
  Transaction + partial unique index дают максимум один active account в namespace;
  удаление active выбирает newest remaining account, удаление последнего — none.
- Material tagged Key либо OAuth(access/refresh/expiry; provider metadata по нужде).
  None не создаёт credential row. OAuth storage roundtrip не означает OAuth execution:
  без implemented provider method resolver даёт honest unsupported, не API-key fallback.
- Config/env — inputs того же resolver, не автоматически скопированные rows.
  Public summaries: ID/label/kind/source/active/effective/safe status, никогда material.
  Secret-bearing values/actions не имеют plaintext Debug/логирования; DB/root/WAL
  сохраняют restrictive storage guarantees. Encryption/keyring этим планом не обещаны.
- Auth policy имеет None/Key/OAuth. Existing config с omitted policy остаётся Key;
  для anonymous custom server нужен explicit None. None не посылает auth headers,
  dummy key или пустой Bearer; конфликт None с credential/auth-header input — явная
  config diagnostic, не скрытое игнорирование. Не выводить None из localhost/empty key.
- Go preset разрешает stored/default-env material только для provider-owned normalized
  HTTPS prefix `https://opencode.ai/zen/go/v1`. Authority проверяется **до** credential
  resolution. Project override вне scope отказывает без lower-priority auto-env fallback;
  `{env:OPENCODE_API_KEY}` в project config не становится project-owned secret.
- Custom namespace включает provider ID + normalized endpoint/auth scope. Смена
  `baseURL` не переносит stored key автоматически: новый scope связывается явно.
  Нужен узкий admission predicate, не grants registry/domain framework.

| Connection | Effective auth и endpoint/model contract |
| --- | --- |
| Built-in `opencode-go` | Active stored Key → `OPENCODE_API_KEY` → configured Key, только после Go authority admission. Catalog — models.dev; endpoint — trusted Go preset, не remote `api`. |
| Existing `ludka2`/OpenProxy | Configured `apiKey` (включая admitted env/file substitution) сохраняет приоритет; scoped stored Key допустим при его отсутствии. Native клиент авторизуется перед proxy, upstream OAuth остаётся за OpenProxy. Exact base + `/responses`, discovery base + `/models`; prefix, headers/options/budgets/oracle неизменны, никакого `/v1` rewrite. |
| Custom Responses/Chat/Messages server | Admitted configured package/baseURL, static/custom catalog, explicit None либо Key. Exact base prefix + соответствующий suffix, без `/v1` rewrite. Key source — explicit configured input, иначе scoped stored account; нет автоматического Go/OpenProxy env fallback. Messages static `apiKey` → x-api-key, `authToken` → Bearer; оба вместе или competing auth headers — diagnostic, не OAuth flow. Explicit localhost/LAN HTTP(S) поддерживаются только по binding admission ниже. |

Private endpoint exception привязана к явно configured и trusted/admitted connection
с normalized scheme/host/port/base-path prefix и source provenance, не global
`allow_private` bypass. Это supersedes прежнее loopback-only ограничение T53 только
для выбранных custom connections. DNS/address и actual-peer/redirect проверки сохраняют
узкий scoped admission; нельзя перейти к чужому origin/prefix или metadata/service
endpoint вне разрешённого local/LAN scope. Lower-trust endpoint override проходит
новую проверку по собственной provenance и не наследует разрешение/credentials.
Узкая интерпретация LAN — private unicast RFC1918/IPv6 ULA плюс loopback;
не blanket admission link-local/metadata, multicast, unspecified/reserved addresses.
Main/follow-up/title/summary/child/retry и corresponding configured discovery используют
один captured route admission. Webfetch/tool-network SSRF policy остаётся независимой;
никакого нового grants/network framework или test-only env flag как production support.

Credential/config changes дают новый admitted binding generation. Prepared requests,
их tools/approval и already launched title/child jobs держат cloned immutable binding.
Committed model/variant switch применяется на следующем request той же задачи по T50,
с новой admission и compatible history; это не whole-turn model pin или config reload.
Public catalog fetch credential-free и не зависит от account mutation.

### Go models.dev contract

- Production source **`https://models.dev/api.json`**, не donor mirror. Bounded full
  response/deadline; normalize/cache только `opencode-go`. Remote `api/env/headers`
  не являются executable connection/credential settings.
- Effective `model.provider.npm ?? provider.npm`: `@ai-sdk/openai-compatible` → Chat,
  `@ai-sdk/openai` → Responses, `@ai-sdk/anthropic` → Messages. Это finite native alias
  map, не SDK dependency/download. Unknown explicit alias остаётся unsupported.
- Preserve IDs, limits/modalities/tool support, basic cost/status/interleaved и exact
  `reasoning_options`. Effort/toggle/budget появляются в picker только с implemented
  protocol lowering; missing controls не выводятся из `reasoning:true` или model name.
  Null effort sentinels фильтруются; T47 ordered view и Default ≠ named none сохраняются.
  Advertised unsupported modalities — metadata, не новое execution permission.
- New remote base → текущие admitted local overrides только surviving IDs; не old
  merged snapshot. Local scalars win, limits field-merge, same-ID variant overlay
  replace без смешивания несовместимых protocol fields. Deprecated исключаются до
  overlay, removed IDs не resurrect. Valid empty Go record означает retirement;
  malformed/missing record — failed refresh с last-good retention.
- Source-qualified persisted **public** last-good record + fetch timestamp; validate
  before atomic publication, cache write best effort. Cache immediately usable;
  один single-flight refresh на startup/picker-open при absent/stale (~5 minutes),
  explicit refresh bypasses TTL. Late results проверяют owning generation/source.
  Unchanged result не требует повторной публикации. Offline: last-good, иначе usable
  empty/failed UI. Нет bundled snapshot или permanent periodic poller.

### Wire, metadata и replay boundary

| Protocol | Go suffix/auth | Минимальный wire contract |
| --- | --- | --- |
| Responses | `/responses`, Bearer | Existing adapter; `store:false`, encrypted reasoning inclusion, exact selected reasoning.effort/no variant overlay, configured reasoning.summary/text.verbosity, max_output_tokens, native function outputs. Не вводить donor model-name reasoning defaults. |
| Chat | `/chat/completions`, Bearer | Streamed usage и indexed/fragmented parallel tool arguments, validated complete IDs/JSON; compatibility.reasoningField (в том числе reasoning_content) и maxTokensField → max_tokens либо max_completion_tokens. Exact selected reasoning_effort; не копировать Responses store/include defaults и не угадывать dialect по модели/vendor hostname. |
| Messages | `/messages`, `x-api-key` | Custom дополнительно static authToken Bearer; max_tokens в общем output budget, tool_use/tool_result, enabled budget_tokens/adaptive/disabled thinking и output_config.effort, signatures/redacted blocks только compatible. anthropic-version:2023-06-01, merged interleaved-thinking-2025-05-14 и только required feature betas; Go auth не меняется на custom scheme. |

Every Go primary/tool-follow-up/title/summary-compaction/child/retry request несёт:
`User-Agent: oc/<version>`, `x-opencode-client: oc`, `x-opencode-project` от stable
`approval::project_identity` (не directory path), `x-opencode-session`,
`x-session-affinity`, `X-Session-Id` = actual persisted session ID; optional
`x-parent-session-id`. Child использует own ID + parent ID. Metadata immutable,
retry его не пересоздаёт; case-insensitive overlay не перебивает authoritative
Go identity/resolved auth и не оставляет competing auth scheme. Session/fork cache
lineage отделена от affinity/deployment; выводится только через supported wire field.
OpenProxy/custom configured headers не меняются на Go policy.

Transport/cache distinctions фиксируются явно, не выдаются за OC1 SDK parity:
`timeout:false` означает native отсутствие overall deadline, `chunkTimeout` — bounded
idle timeout; numeric timeout/chunkTimeout задаются в milliseconds с validation,
без отключения cancel/byte caps. В OC2 эти core fields stripped перед native package,
старый SDK wrapper
не oracle native timeout behavior. `setCacheKey` сохраняется как compatibility control,
но его current Rust body hash не OC2 lineage: target key стабилен для session/fork
root lineage. Responses emits supported prompt_cache_key; Chat только при
compatibility.supportsPromptCacheKey, Messages использует explicit cache_control
на tools/system/message parts, максимум четыре breakpoints по donor priority.
Без enabled/supported cache нет invented wire field; lineage не auth/deployment identity.

Нужен private finite Protocol enum и smallest common ordered text/reasoning/complete
validated tool calls/linked results/usage, initial system + chronological system/effort
seam; native opaque continuation отдельна. Chronological system — не новый initial
prompt: Responses lowers developer update; Chat — escaped `<system-update>` user-text
in-place; Messages native только при explicitly supported capability, иначе такой же
lower-authority fallback. Effort markers сохраняются in-place только при declared
per-message support; иначе strips markers и uses captured selected top-level effort.
При supported path и совпадении final marker с captured selection top-level effort
остаётся `previous` первого marker, updates lower in-place: Responses configuration_update
либо Messages system/output_config.effort с required mid-conversation beta. Так новый
effort не применяется до своего chronological change point. Reset/default сохраняет
позицию; final-marker mismatch после fork/Revert strips markers и uses captured current
top-level effort, не guessed model allowlist. T45/PRM01 потребляет этот wire seam.
Share bounded transport/SSE framing, не Responses event state machine. Runtime/title
readers больше не интерпретируют любой результат как Responses JSON. EOF/
refusal/error/cancel без genuine terminal не становятся success и не исполняют
partial tools. Responses `response.incomplete(max_output_tokens)` сохраняет
явный `length`, `content_filter` остаётся failure, unknown-incomplete проходит
общий typed T54/RET01 bounded retry/continuation contract. Один runtime
owner без replay исходной committed generation/effects и без нового
per-protocol retry-loop; backend Responses slice T54 не ждёт T53 целиком.
Existing admitted text/image attachments и MCP results проходят protocol-specific
lowering; unsupported modality отказывает явно, не превращается в текстовый success.
Это сохранение A04/current text/image input и text output, не добавление audio/video/
PDF execution, Images endpoint/image-generation output или hosted tools.

New TurnLog сохраняет protocol + non-secret binding provenance. Одна compatibility
rule для opaque history **и checkpoints**: provider/API model/protocol/deployment/
admitted auth scope. Session telemetry/token bytes не deployment identity. Legacy
absence означает Responses, но отсутствие binding не разрешает приписать старым
items текущий endpoint: сохраняется только доказуемо compatible lane, иначе public
history без alien opaque state. Raw journal bytes не переписываются.

Не пропустить `storage_dcp_view.rs` SQL `json_object` reconstruction и tool-pair/MCP
index filtering, `storage_fork.rs` validation, runtime context classifiers/estimates,
checkpoint injection и оба title readers. Добавление enum только в TurnLog/events
не закрывает durable compatibility. Completed calls/results и unknown-effect quarantine
сохраняются; protocol switch не является replay authorization.

### Startup и TUI

Admit local config/trust/policy → existing single Db/recovery → credential/catalog/
preference facts → application snapshot. Missing config — valid first-run absence;
malformed mandatory policy, lock/storage/recovery/cleanup failure — fatal. Built-in
Go доступен до key/model. Catalog, credential availability и request auth rejection
различны; saved key/public fetch не равны server-validated authorization.

Existing `ModelRef {provider,id,variant}` — canonical optional selection во всех actions,
preferences, tabs/agent drafts, fork/restart и UI values. Provider-keyed catalogs/
readiness не смешивают одинаковые IDs; retired selection сохраняется без substitute.
UI показывает stored active/effective/env/config source truthfully, включая override.
Env connection отключается вне приложения, не удалением stored row.

`/connect` → provider → masked ephemeral key form/account management → owner ack →
provider-filtered model picker. Add/activate/rename/confirmed remove — реальные owner
actions; cancellation/paste/errors/tab switch не отправляют secret в обычный composer,
draft/history/copy/diagnostic. Functional flow проверяется T53; paired affected
connect/picker visual qualification остаётся T44 после explicit resume, не full-T44
done dependency или claim общего pixel parity.

### Constraints and non-goals

- A01–A13, OpenProxy DISC/PROV oracle, immutable generations/history, trust/permissions,
  bounded memory/transport, redaction/cancel/cleanup и no unknown-effect replay сохранены.
- No auth.json compatibility/import/dual-write или доступ к donor/user/runner secrets.
  Native storage additive migration допустима; это не legacy credential importer.
- No Codex/OAuth login/browser/callback/token refresh, hosted tools/WebSockets/native
  Messages compaction, новый JS/SDK host, all-provider catalog/registry/retry framework,
  keyring service, CLI `auth login/list/logout` trio или config-authoring UI.
- Не full favorites/recents/cycling/pixel-matrix rewrite, новый crate, full journal
  rewrite, unused grant/value-revision machinery или задачи на каждый срез.

## Change Envelope и порядок срезов

Одна T53, не расширение T51. Scouts read-only, один mutation owner; coarse modules
и existing public paths соблюдают AGENTS/ARCHITECTURE, substantive unit tests отдельно.

1. **Согласовать seams и fixtures.** Проверить минимальные qualified T51 readiness/safe
   diagnostics и T47 ordered view; frozen legacy/canonical config, protocol/auth/binding
   facts и synthetic host-shaped fixtures. Dirty код или исторический task PASS не
   заменяют qualification нужного среза.
2. **Независимые backend части:** credential owner в existing Db; public models.dev
   owner; protocol/history seam с legacy/SQL/fork/checkpoint fixtures. Порядок между
   ними свободный, state/secret ownership не дублируется.
3. **Bindings/wires/startup.** Один config normalizer + source merge и scoped local/LAN
   admission, затем сначала провести existing Responses через seam, затем
   Chat/Messages и Go metadata; каждый admitted wire подключить к T54/RET01
   typed error/headers и общему retry owner, не ждать whole T54/T44 task status,
   но не выдавать GO03 PASS без соответствующей квалификации. Provider-qualified
   optional selection/configless local startup используют готовые owner facts
   и immutable admitted binding.
   KISS: existing provider.rs facade + coarse responses/chat/messages/transport owners
   и existing failure.rs по реальным responsibilities; reuse HTTP/byte→SSE framing,
   не универсальный JSON/event parser. Enum dispatch достаточно; smoke ProviderPort
   не превращать в production framework. Full SDK/provider framework вне scope;
   narrow crate допустим только при доказанном blocker к собственному thin layer и
   сохранении one-attempt/cancel/caps/error/effect contracts.
4. **Connect/models consumers.** Core typed actions/safe DTO → application owner →
   TUI commands/dialog/picker/input/live; headless consumes тот же resolver, без новых
   auth CLI commands. T50 model lookup и owner-approved `oc models` получают тот же
   provider-qualified catalog view после минимального qualified catalog slice.
   CLI сохраняет свой полный ID-only output вместо tool family/paging semantics;
   его TOOL18 и T45 profile-binding evidence не дублируют GO02/GO05.
5. **Qualification.** Nearest owner tests → actual binary → final workspace gates →
   bounded opt-in Go live; один factual report. Не duplicate full matrix на всех слоях.

Expected owners: adapters `credentials.rs`/`storage_credentials.rs` (new, existing
Db ownership), `models_dev.rs` (new), config/models/composition/provider_readiness,
application/provider_catalog и application_selection; provider Chat/Messages seams,
tools TurnLog, runtime turn/context/compaction, storage_dcp_view/storage_fork; core
CoreApp/queries ModelRef и binary/TUI consumers. Naming follows фактическую раскладку,
не обязательство создать файл на каждый тип. Перед execution сверить владельческие
commits/текущий diff, не переносить чужую незакоммиченную работу вслепую.

## Current Checkpoint / State

- 2026-09-29: read-only RECON + independent general audit завершены; frozen docs-only
  план утверждён с единым storage и custom-auth уточнением. Implementation NOT_STARTED;
  R1–R6/GO01–GO06 pending; **тогда** active была T51, T44 PAUSED.
- 2026-10-02: host-shaped RECON при HEAD b9d02c090, dirty T50; owner утвердил plan-only
  commit/push и explicit LAN + localhost. Canonical config, thin wires, dual Messages
  auth, chronology/cache/compatibility уточнены; implementation/GO01–GO06 pending.
  Active T50, T53 todo, T44 PAUSED, T55 safe-handoff priority/statuses unchanged.
  T51 completed readiness baseline не доказывает новый DCP3.2.0 follow-up;
  T45/R9 owns child semantics, не T53 и не whole-task prerequisite.
- 2026-10-05: T53 active после scheduling handoff из T45. `1748ba73b` добавил
  protocol/chronology seam, `d66db2eaa` — первый native Chat wire; `9b9482a7d`
  исправил refusal/late-content boundary. Текущий corrective slice ограничивает
  replay reasoning одной assistant-группой и квалифицирует три actual application
  requests с двумя read results. Current evidence: `evidence/T53/protocol-seam.md`
  и `evidence/T53/chat-wire.md`; workspace 1579/0/10, strict Clippy/fmt/build/help
  PASS. R1–R6/GO01–GO06 remain pending; live не выполнялся, T44 PAUSED.
- 2026-10-05 follow-up: native Messages wire + static apiKey/authToken binding,
  independent stream decoder и real application tool roundtrip обоих schemes;
  `evidence/T53/messages-wire.md`. Crate unit 564/0/0, strict crate Clippy/fmt PASS.
  Это partial R1/R3, не полный GO01/GO03. No live; T44 PAUSED.
- 2026-10-05 credentials slice: additive SQLite accounts + transactional lifecycle,
  Key/OAuth roundtrip, redacted summaries/errors, DB/WAL/SHM 0600; narrow fixed-Go/
  custom-prefix auth resolver и captured anonymous transport. Crate 568/0/0 и strict
  Clippy/fmt PASS; `evidence/T53/credentials.md`. Пока resolver не подключён к config/
  application, private endpoint admission и account UI: GO01 остаётся pending.
- Next: интеграция scoped auth/endpoint admission и оставшиеся per-wire options/
  protocol-safe durable provenance; catalog/connect owners остаются незакрытыми.
- Endpoint follow-up:
  Endpoint follow-up: shared captured origin/prefix/provenance admission, trusted
  localhost/RFC1918/ULA, DNS pinning before credentials and connected-peer guard;
  configured anonymous discovery. `evidence/T53/credentials.md`; GO01 remains pending
  effective canonical overlays/account owner controls, no full T53 PASS.
- Route qualification risk: public recon выявил `qwen3.8-max`/`qwen3.7-plus` как
  default Chat в models.dev, но official Go docs называют Messages. Это dated source
  conflict, не hardcoded ID exception. Follow models.dev aliases; bounded probes
  должны разрешить конфликт до полного Go PASS. Если mismatch подтверждён — safe
  unavailable diagnostic и recorded catalog/protocol blocker, без paid fallback.
- Blocker: none для начала разрешённой реализации; source conflict — проверяемая
  qualification hypothesis, не заранее объявленный external blocker.

## Evidence / Completion

Implementation checkpoint 2026-10-05: application startup/reload/Location/session move
resolve existing Db scoped accounts before discovery/runtime publication; explicit
local `options.authPolicy` None/Key/OAuth and safe unsupported OAuth admission tested.
Restart, endpoint change and real anonymous fake transport regressions: adapter unit
suite 571/0/0, strict crate clippy/fmt green; [credentials evidence](../../evidence/T53/credentials.md).
GO01 remains pending trusted endpoint admission/canonical overlays/connect integration.

GO01–GO06 принадлежат только T53; ownership остальных IDs не меняется. Method и
final commands — [TEST_PLAN](../TEST_PLAN.md#t53--opencode-go-и-единые-provider-credentials-approved-2026-09-29-pending).
Live campaign journal/envelope создаётся **до** requests: ≤24 physical generation
HTTP requests всего, включая retries/title/compaction/children/protocol probes,
smoke output ≤2048 tokens; no all-model sweep, budget reset или uncertain replay.
Existing OpenProxy mandatory live gates не заменяются Go smoke. Supplied product test
key — secret input для authorized bounded tests, не repo artifact/runner credential.

Pending: complete только после R1–R6 и impacted green gates с factual report;
delivery approved plan и doc validator не означают provider/TUI readiness.

### Local normalization checkpoint (2026-10-05)

Legacy/canonical local provider documents now share one normalizer and supplied-field
source merge, with endpoint/key/header provenance, model migration and array variants.
`evidence/T53/config-normalization.md` records 576 adapter unit tests and strict
all-target gates. This is partial R2/R3, not GO02/GO03 PASS: effective selected request
binding/overlays and public Go cache remain next, followed by metadata/replay/connect
and bounded GO06 qualification. T44 stays PAUSED.

### Wire options consumer checkpoint (2026-10-05)

Provider-level normalized wire options now reach common dispatch: Responses
effort/summary/text verbosity, Chat effort, Messages thinking/output config, and
recursive body overlays with selected effort/profile priority. Typed admission
refuses malformed/alien options; all three fake-server routes and Messages app
roundtrip are green. Adapter suite **579/0/0**, strict all-target clippy/fmt green;
`evidence/T53/config-normalization.md` records scope and failed experiment.
This is partial R3, not GO03/T53 PASS: effective model/variant binding/API ID,
Go cache/metadata, durable provenance, connect and bounded GO06 remain pending.

### Effective local request binding checkpoint (2026-10-05)

Local provider→model→variant settings/headers/body, model-level package and API
modelID now produce immutable admitted templates. Tuple source provenance prevents
literal IDs from aliasing credential/endpoint trust. Application resolves each scope
from configured inputs before publication, without inheriting an already-resolved
parent secret; shared dispatch and variant-aware readiness/admission consume them.
Actual application main Messages and own-model title Chat routes use separate API
IDs/keys; missing/OAuth variants refuse fresh acceptance. Adapter suite **584/0/0**,
strict all-target clippy/fmt green; receipt `evidence/T53/config-normalization.md`.
This is partial R1/R3, not full GO01/GO03: public Go cache/catalog, all-lane metadata,
chronological effort/cache, durable protocol/binding replay, accounts/connect and
bounded GO06 remain pending. Next slice: public models.dev/cache source owner.

### Public catalog owner checkpoint (2026-10-05)

`models_dev::GoCatalog` now normalizes only public Go metadata, strips remote
connection/auth inputs before persistence, and owns source-qualified last-good
cache with timestamp, TTL and cancellable single-flight refresh. Valid empty
records retire IDs; malformed/missing records retain last-good. Current local
overrides apply only to surviving IDs and same-ID variants replace overlays.
Declared controls use finite implemented generic lowerings, without model-name
defaults. Adapter suite **588/0/0**, strict all-target clippy/fmt green;
receipt `evidence/T53/public-catalog.md`. This is partial R2, not GO02/T53 PASS:
application startup/picker/manual refresh and shared model read-view integration
are next, followed by metadata/replay/connect and bounded GO06 qualification.

### Public application integration checkpoint (2026-10-05)

Built-in Go preset now starts without a configured provider entry; existing Db
handles share one public cache owner. Application startup, picker queries and
explicit reload consume fresh/last-good metadata independently of credentials.
Finite per-model/variant bindings preserve API IDs, exact declared controls and
Go Bearer auth on all three wires; unknown aliases refuse before acceptance and
valid empty records remove bindings. Public HTTP auth statuses never become paid
connection auth facts. Cached reads remain immediate during in-flight GET.
Adapter suite **591/0/0**, strict all-target clippy/fmt green; receipt
`evidence/T53/public-catalog.md`. Partial R2/R3, not GO02/T53 PASS: shared CLI/tool
read-view, all-lane metadata/chronology/cache, durable replay, accounts/connect and
bounded GO06 qualification remain. Next slice: selection-independent read-view.

### Shared public read-view checkpoint (2026-10-05)

Selection-independent `oc models` and native `opencode_models` now share the public
Go normalizer/cache rules, current surviving local overrides and honest failure
status. CLI reads only bounded public cache metadata without Db startup/recovery/
store writes; actual binary byte-compares the entire live-owner WAL store before
and after listing. Native tool uses the existing Db single-flight owner. Configured
Go secret templates are not resolved for this public read-view. Broad verification
also restored standalone declared-key admission while retaining metadata-only
transport fixtures. Workspace **1615/0/10**, strict workspace clippy/fmt, locked
build and binary help green; factual receipt `evidence/T53/public-catalog.md`.
No opt-in live gate ran. This is partial R2, not full T53 PASS: Go all-lane metadata,
chronology/cache, durable replay, provider-aware connect/selection and bounded
GO06 qualification remain. Next slice: request metadata/lineage cache/chronology.

### Request identity/cache checkpoint (2026-10-05)

Immutable operation context now feeds main/tool-follow-up/retry/child, automatic
and manual title, and summary-compaction sends. Go reasserts stable approval project
identity, actual session/parent affinity, native User-Agent and resolved Bearer auth
after case-insensitive headers. Session/fork cache lineage replaces production body
hash keys; recursive forks copy the source lineage transactionally, while children
keep their own session lineage. Responses and explicitly supported Chat emit cache
keys; Messages emits at most four prioritized explicit breakpoints. Adapter unit
suite **596/0/0**, strict all-target clippy/fmt green; factual receipt
`evidence/T53/request-context.md`. Partial R3, not GO03/T53 PASS: chronological effort
consumers, durable compatibility, configless provider-aware connect/selection and
bounded GO06 qualification remain. Next slice: chronological capabilities/effort.

### Chronology checkpoint (2026-10-06)

Committed model/variant effort changes now have structured atomic event facts,
strict durable input markers and idle/reopen/busy-next-request projection. Explicit
effort capability consumes donor baseline/default/drift rules on Responses and
Messages; Chat strips markers. Messages native text uses a declared fork capability
and valid placement, otherwise escaped fallback; effort beta is conditional.
The existing T50 busy tool/approval fixture qualifies unchanged captured execution
and ordinary results with two effort transitions. Evidence: `evidence/T53/chronology.md`.
Final workspace **1624 passed / 0 failed / 10 existing ignored**, strict workspace
clippy/fmt/build/help/diff gates pass. A real controller stack overflow was diagnosed
and fixed by boxing the existing intent future, without enlarging test limits.
This is partial R3, not full GO03/T53 PASS. Full durable protocol/binding/checkpoint/
SQL/DCP/fork qualification, configless/connect/provider-qualified selection and
bounded GO06 live remain pending. Next slice: GO04 durable binding owner.

### Durable wire authority checkpoint (partial GO04)

Prepared primary receipts and TurnLog now persist finite protocol, actual API model,
provider, admitted deployment/provenance digest and effective auth/tenant digest.
Legacy absent protocol is Responses; unknown protocol/malformed binding fails decode.
Missing binding never authorizes opaque replay. Request-local projection preserves
ordinary text and settled call/result pairs while dropping incompatible opaque IDs,
encrypted reasoning and signatures. SQL presentation, selected HOT, raw segments,
fork rebasing and reopen preserve the original binding rather than current selection.
Workspace gates passed (1627/0/10; ignored opt-in tests unchanged); a final review
also prevented resumed legacy journals from acquiring current binding retrospectively.
This is partial R4, not GO04/T53 PASS. Native checkpoint replay, auxiliary/variant
route consumers, configless/connect/provider-qualified selection and GO06 live remain
pending. Next slice: native checkpoint and auxiliary captured-route qualification.

### Native/auxiliary captured-route checkpoint

Native opaque replay now requires explicit origin and exact captured wire
authority; unbound/missing legacy facts cannot authorize it. Main prepared
fingerprints, compaction and child recovery fences use the actual selected
model/variant binding. Actual runtime read→tool follow-up→reopen/fork→foreign
wire continuation qualifies all three protocols without replaying settled tools.
Workspace **1629/0/10**, strict workspace clippy, fmt, locked build, help and diff
checks PASS; factual follow-up is in `evidence/T53/durable-binding.md`. This does
not finish T53: configless/provider-qualified connect/accounts UI, actual binary
qualification, full acceptance review and bounded GO06 live remain pending.
Next slice: optional exact selection and configless startup through existing owners.

### Unchosen local startup checkpoint (2026-10-06)

Missing configuration/model and missing optional connection now preserve local
Home/history without selecting a fallback. Optional composer query returns no
model; unchosen submit/headless refuses before root/turn/title/tools. Invalid
policy, malformed/security-refused transport and storage/recovery remain fatal.
Existing Location rollback fixtures now exercise malformed mandatory policy,
while actual startup PTY separately qualifies configless success and restoration.
Evidence: `evidence/T53/configless.md`; workspace **1633 PASS / 0 FAIL / 10 IGNORED**,
strict clippy/fmt/build/help green. Final adapter fence checks: 610 PASS.
This is partial R5, not GO05/T53 PASS. Next: masked connect/account owner actions,
provider-qualified selection and complete actual-binary connect flow.

### Typed account owner checkpoint (2026-10-06)

Core account commands now expose only safe metadata and effective stored/env/config/
anonymous/unsupported/missing source. Native owner add/activate/rename/confirmed
remove use existing SQLite transactions; idle credential publication restores
configured inputs, retains catalog state and does not select a model or restart MCP.
Held requests refuse mutation while safe account queries remain responsive. Restart,
rollback and last-key removal checks pass without stale-key reuse or root creation.
Evidence: `evidence/T53/accounts.md`; adapter 612/0/0, core 32/0/0, strict workspace
clippy/fmt green. Partial R1/R5, not GO01/GO05/T53 PASS. Next: masked connect/account
UI, qualified picker/selection and actual-binary full flow.

## RECON sources

Donor `opencode` pinned v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`:
`packages/core/src/plugin/models-dev.ts`, `credential.ts`, `credential/sql.ts`,
`integration.ts`, `models-dev.ts`, `variant.ts`, `aisdk-native.ts`,
`session/model-request.ts`; `packages/ai/src/protocols/` Chat/Responses/Messages;
`packages/tui/src/component/dialog-integration.tsx`, `dialog-model.tsx` и `app.tsx`.
Normalization/precedence: `packages/schema/src/config/provider.ts`,
`packages/core/src/v1/config/{migrate,provider-options}.ts`, `provider.ts`,
`model-resolver.ts`, `aisdk-native.ts`; generic `packages/ai/src/providers/`
openai/openai-compatible/anthropic. Chronology/cache: `packages/ai/src/protocols/shared.ts`,
`effort-updates.ts`, `route/client.ts`, `session/model-request.ts` и nearest protocol tests.
Go — generic Console key integration, не Zen OAuth. Donor `auth.json` используется
только legacy import migration; native T53 не читает/не пишет этот формат.
Public sources: [models.dev API](https://models.dev/api.json),
[Go](https://opencode.ai/docs/go/), [Go v2](https://opencode.ai/v2/docs/console/go).
