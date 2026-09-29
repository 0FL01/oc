# Goal: OpenCode Go, единые credentials и connect/model TUI

Status: active
Source: владелец после RECON и параллельного аудита утвердил план 2026-09-29; «единая логика хранения кред, без деприкейт фич», будущий Codex OAuth и custom llama.cpp/ludka2 учтены как границы общего owner.
Last updated: 2026-09-29
Task: T53 (todo; утверждение и доставка плана не являются implementation PASS).

## Objective

Native `oc` поддерживает `opencode-go`: public models.dev catalog, Console API key,
три требуемых wire-протокола, Go request headers и рабочие `/connect` → `/models`
с account management как в pinned OC2 TS. Один credential owner обслуживает Go и
admitted custom providers, не ломая OpenProxy и не требуя фиктивного ключа для
явно anonymous local server. Будущий OAuth помещается в тот же storage contract,
но Codex login/refresh и другие OAuth flows в этой задаче не реализуются.

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
  - Primary evidence: synthetic-secret owner/storage tests с reopen, transactional activation/removal, precedence/scope refusal и zero unauthorized requests.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R2: public Go catalog из models.dev, независимый от credentials.
  - Source: «модели фетчить с models.dev»; donor models-dev normalization, approved audit simplification.
  - Acceptance: GO02 доказывает bounded fetch/parse/cache/last-good, no-key browsing, finite package-to-protocol mapping и реализованные reasoning overlays. Public refresh не подтверждает key и не снимает request auth rejection; retired/deprecated choice остаётся явно unavailable, без fallback/local resurrection.
  - Primary evidence: source fixtures + fake-clock/client catalog tests, включая coexistence с неизменным OpenProxy discovery.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R3: три native wires и все Go request metadata lanes.
  - Source: «нюансы интеграции OpenCode Go, например хедеры»; pinned native AI routes и common model-request owner.
  - Acceptance: GO03 доказывает Responses/Chat/Messages text, complete tool roundtrip, reasoning/usage, terminal/error/cancel и bounded pre-commit retry. Every Go request lane получает immutable binding/metadata; custom providers переиспользуют admitted adapters, но не Go-only headers. Нет guessed route/model/paid fallback.
  - Primary evidence: parameterized fake wire/runtime tests; captured requests main/follow-up/title/summary-compaction/child/retry и concurrent-session barriers.
  - Status: pending
  - Evidence: pending — evidence/T53/report.md.

- R4: protocol-safe durable replay, DCP и forks.
  - Source: существующие immutable history/tool-causality contracts A02/A04/A07/A10 и необходимый multi-protocol boundary.
  - Acceptance: GO04 доказывает protocol/binding preservation через TurnLog, SQL projection, checkpoints и fork/reopen. Absent protocol читается как legacy Responses; explicit unknown protocol fail-safe. Incompatible opaque state не конвертируется/не отправляется другой deployment; raw rows и completed tool pairs сохраняются, unknown effects не переисполняются.
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

Connection config остаётся admitted `provider.<id>`: protocol alias, `baseURL`,
headers/timeouts/cache options и model source. Это не credential store. `/connect`
управляет credentials для известного connection, не создаёт config-authoring UI.

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
| Custom llama.cpp/compatible server | Admitted configured protocol/baseURL, static/custom catalog, explicit None либо Key. Для Chat пример prefix `http://127.0.0.1:8080/v1` + `/chat/completions`. Key source — explicit configured input, иначе scoped stored account; нет автоматического Go/OpenProxy env fallback. Loopback HTTP и dial/redirect/private-egress guards не расширяются до произвольной LAN. |

Credential/model/config changes дают новый admitted binding generation. In-flight
request/turn/title/child jobs держат cloned immutable binding; subsequent work использует
новый. Public catalog fetch credential-free и не зависит от account mutation.

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
| Responses | `/responses`, Bearer | Existing adapter; `store:false`, encrypted reasoning inclusion, exact selected effort/no overlay, native function outputs. |
| Chat | `/chat/completions`, Bearer | Streamed usage, indexed tool argument assembly, interleaved reasoning compatibility; не копировать Responses `store/include` defaults. |
| Messages | `/messages`, `x-api-key` | `max_tokens`, tool_use/tool_result, thinking/signature/redacted blocks; `anthropic-version: 2023-06-01`, merged `interleaved-thinking-2025-05-14` и только required feature betas. |

Every Go primary/tool-follow-up/title/summary-compaction/child/retry request несёт:
`User-Agent: oc/<version>`, `x-opencode-client: oc`, `x-opencode-project` от stable
`approval::project_identity` (не directory path), `x-opencode-session`,
`x-session-affinity`, `X-Session-Id` = actual persisted session ID; optional
`x-parent-session-id`. Child использует own ID + parent ID. Metadata immutable,
retry его не пересоздаёт; case-insensitive overlay не перебивает authoritative
Go identity/resolved auth и не оставляет competing auth scheme. Session/fork cache
lineage отделена от affinity/deployment; выводится только через supported wire field.
OpenProxy/custom cache и configured headers не меняются на Go policy.

Нужен private finite Protocol enum и smallest common ordered text/reasoning/complete
validated tool calls/linked results/usage seam; native opaque continuation отдельна.
Share bounded transport/SSE framing, не Responses event state machine. Runtime/title
readers больше не интерпретируют любой результат как Responses JSON. Incomplete/EOF/
refusal/error/cancel не становятся success и не исполняют partial tools. Один bounded
pre-commit retry owner, без replay committed generation/effects.
Existing admitted text/image attachments и MCP results проходят protocol-specific
lowering; unsupported modality отказывает явно, не превращается в текстовый success.
Это сохранение A04/current capabilities, не добавление audio/video/PDF execution.

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
   diagnostics и T47 ordered view; frozen protocol/auth/binding facts. Dirty код или
   исторический task PASS не заменяют qualification нужного среза.
2. **Независимые backend части:** credential owner в existing Db; public models.dev
   owner; protocol/history seam с legacy/SQL/fork/checkpoint fixtures. Порядок между
   ними свободный, state/secret ownership не дублируется.
3. **Bindings/wires/startup.** Сначала провести existing Responses через seam, затем
   Chat/Messages и Go metadata; provider-qualified optional selection/configless
   local startup используют готовые owner facts и immutable admitted binding.
4. **Connect/models consumers.** Core typed actions/safe DTO → application owner →
   TUI commands/dialog/picker/input/live; headless consumes тот же resolver, без новых
   auth CLI commands. T50 model lookup получает тот же catalog view.
5. **Qualification.** Nearest owner tests → actual binary → final workspace gates →
   bounded opt-in Go live; один factual report. Не duplicate full matrix на всех слоях.

Expected owners: adapters `credentials.rs`/`storage_credentials.rs` (new, existing
Db ownership), `models_dev.rs` (new), config/models/composition/provider_readiness,
application/provider_catalog и application_selection; provider Chat/Messages seams,
tools TurnLog, runtime turn/context/compaction, storage_dcp_view/storage_fork; core
CoreApp/queries ModelRef и binary/TUI consumers. Naming follows фактическую раскладку,
не обязательство создать файл на каждый тип. T51 seam paths из RECON ещё dirty;
перед execution сверить владельческие commits, не переносить чужой diff вслепую.

## Current Checkpoint / State

- 2026-09-29: read-only RECON + independent general audit завершены; frozen docs-only
  план утверждён с единым storage и custom-auth уточнением. Implementation NOT_STARTED;
  R1–R6/GO01–GO06 pending, current active task остаётся T51, T44 PAUSED.
- Next: после scheduling handoff проверить qualified T51 seam; подготовить R1
  synthetic credential/scope fixture, затем smallest owner/storage slice.
- Route qualification risk: public recon выявил `qwen3.8-max`/`qwen3.7-plus` как
  default Chat в models.dev, но official Go docs называют Messages. Это dated source
  conflict, не hardcoded ID exception. Follow models.dev aliases; bounded probes
  должны разрешить конфликт до полного Go PASS. Если mismatch подтверждён — safe
  unavailable diagnostic и recorded catalog/protocol blocker, без paid fallback.
- Blocker: none для начала разрешённой реализации; source conflict — проверяемая
  qualification hypothesis, не заранее объявленный external blocker.

## Evidence / Completion

GO01–GO06 принадлежат только T53; ownership остальных IDs не меняется. Method и
final commands — [TEST_PLAN](../TEST_PLAN.md#t53--opencode-go-и-единые-provider-credentials-approved-2026-09-29-pending).
Live campaign journal/envelope создаётся **до** requests: ≤24 physical generation
HTTP requests всего, включая retries/title/compaction/children/protocol probes,
smoke output ≤2048 tokens; no all-model sweep, budget reset или uncertain replay.
Existing OpenProxy mandatory live gates не заменяются Go smoke. Supplied product test
key — secret input для authorized bounded tests, не repo artifact/runner credential.

Pending: complete только после R1–R6 и impacted green gates с factual report;
delivery approved plan и doc validator не означают provider/TUI readiness.

## RECON sources

Donor `opencode` pinned v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`:
`packages/core/src/plugin/models-dev.ts`, `credential.ts`, `credential/sql.ts`,
`integration.ts`, `models-dev.ts`, `variant.ts`, `aisdk-native.ts`,
`session/model-request.ts`; `packages/ai/src/protocols/` Chat/Responses/Messages;
`packages/tui/src/component/dialog-integration.tsx`, `dialog-model.tsx` и `app.tsx`.
Go — generic Console key integration, не Zen OAuth. Donor `auth.json` используется
только legacy import migration; native T53 не читает/не пишет этот формат.
Public sources: [models.dev API](https://models.dev/api.json),
[Go](https://opencode.ai/docs/go/), [Go v2](https://opencode.ai/v2/docs/console/go).
