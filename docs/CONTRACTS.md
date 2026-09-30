# Application, состояния и ошибки

Этот документ — собственный контракт Rust-приложения. Upstream wire compatibility описана отдельно; внутренние DTO не обязаны копировать HTTP schema OpenCode.

## Application API

Команды: CreateSession(Location), SubmitInput, CancelTurn, AnswerPermission, CompressRequest (user prompt command), SelectModelVariant, SelectPrimaryAgent(expected generation), InvokeCommand(expected generation), ReloadConfig, SelectLocationSession, Shutdown. Запросы: ListSessions(cursor), ReadHistory(cursor, limit), GetSessionSnapshot, ListModels, WorkspaceSnapshot(redacted), EffectiveConfig(redacted), DcpStats. Конкретные Rust signatures фиксируются в M1; не генерировать универсальный RPC framework.

T50 pending extension adds typed question query/answer/dismiss and session rename/
admitted move operations through these same owners. Exact Rust signatures follow the
minimal slice, not a new RPC registry. Direct model definitions opencode_models,
opencode_session_rename and opencode_session_move lower to authorized existing catalog/
session actions; model lookup does not select a model. Validate actor/target/generation,
especially child versus parent/sibling session authority, before mutation.

`SubmitInput` возвращает durable accepted ID либо ошибку. При переполнении очереди — Busy, не потеря input. Session IDs/application operation IDs непрозрачны; clock/ID provider в tests заменяемый. Ошибка provider не уничтожает пользовательский ввод.

Workspace actions несут expected `(LocationId, ConfigGenerationId)` и отвергают stale UI requests. `InvokeCommand` сохраняет command ID/generation, original invocation и одно expanded user input, затем использует тот же durable admission, что обычный `SubmitInput`; TUI не исполняет definition самостоятельно. `SelectLocationSession` выбирает/создаёт session target Location и не меняет Location существующей session.

Explicit T50 session_move is different from SelectLocationSession: same sessionID,
durable admitted/pending/applied placement and complete trusted destination generation.
Never mutate an in-flight turn/request/tool context; close the source turn durably
under its pinned generation, then use a distinct subsequent destination turn.
Same-batch destination assumptions fail or keep
source context. History/DCP/original job/child provenance and unknown-effect MCP
quarantine survive; failure/stale/untrusted destination leaves placement unchanged.
No permission grants, family migration, arbitrary foreign-session control or replay
of unknown effects. Only the permanent-binding ban is superseded (AUD14/TOOL19).

Live hints: TextDelta, ToolProgress, ContextStats. Durable outcomes: InputAccepted, TurnStarted, ToolStarted/Finished, CompressionCommitted, TurnFinished/Failed/Interrupted. Каждое outcome имеет session_id, turn_id, seq и типизированные details. Не включать credentials/raw HTTP headers.

## Turn state machine

`idle → preparing → streaming → tool_pending → tool_running → preparing` до финального ответа; любой live state может перейти в `cancelling → interrupted`, `failed` или `completed`. WaitingApproval — отдельное ожидание с возможностью cancel, не held DB transaction.

Каждый provider response может иметь несколько tool calls. Аргументы собираются bounded по call ID; tool НЕ исполняется по частичному JSON. Сначала закрыть/validate response и завершить arguments, затем admission tools. Duplicate call IDs, unknown tool и несоответствие schemas — typed errors. Wire parallelism не снимает ordering/safety гарантий; T45/R3 явно требует concurrent execution независимых subagent calls и immediate background. Native закрытие response до admission остаётся отдельным streaming difference, не основанием сериализовать детей.

T50 background shell follows the same durable admission/output graph: running is
not terminal success, and a later owner-generated notice is deduplicated by job/
delivery identity. No execution timeout still has bounded capture/cancel/teardown.
Question pending wait has its own typed replies, distinct from permissions; stale/
duplicate/foreign replies fail, dismissal interrupts, no-consumer headless is explicit
non-success and --auto never invents an answer. No held DB transaction or UI-only state.

Пустой stream EOF без terminal completion — interrupted/failed, не успех. Не выполнять tool дважды из-за повторного done-event. Тесты включают arbitrary chunk split, CRLF, partial UTF-8 и terminal error after text.

## Tool operation state

`planned → authorized → started → succeeded | failed | cancelled | partial | unknown`.

Intent записывается до side effect; outcome — после подтверждения. Crash после начала и до фиксации результата даёт unknown, даже если transcript выглядит незаконченным. Read-only call можно повторить новым operation ID в рамках новой явной попытки; mutation, shell и MCP не autoretry по старому ID. Exactly-once не обещается.

Multi-file patch не атомарен целиком: preflight всех entries уменьшает риски, но каждый файловый commit отдельный. Partial outcome сообщает committed paths/ещё не применённые paths и recovery instructions. Не откатывать чужие правки автоматически.

## Errors

ConfigInvalid/UnsupportedCapability/UnsupportedPlugin/MissingCredential/StaleGeneration/UnknownAgent/UnknownCommand/UnknownSkill/UnknownModel/InvalidVariant; PermissionDenied/ApprovalRequired; Conflict/PathDenied/PatchInvalid; ProviderAuth/RateLimited/ProviderTimeout/ProtocolError/ContextLimit; McpUnavailable/McpProtocol/McpToolError; StorageFull/StorageBusy/DataRootBusy; Cancelled/Interrupted/UnknownOutcome.

Retry только в одном явно указанном месте. Discovery имеет собственный
user-defined bounded retry; MCP invocation retry по умолчанию отсутствует.
Owner-approved T54/RET01 supersedes только старый pre-first-event/two-retries
generation default: одна физическая попытка в adapter, одна runtime policy
на логический LLM step, первоначальный запрос + до 10 retries со source-derived
2/4/8/10s jitter, bounded Retry-After и отменяемым ожиданием. 400/401/403/
413/422 и исчерпанная quota по умолчанию terminal; структурированный
наблюдаемый `x-should-retry` может переопределить default provider-policy,
но не trust/validation/permissions/caps. Context error идёт в отдельную
compaction ветку без молчаливой потери сообщений.

До фактического output retry сохраняет assistant-step identity, не
публикует terminal turn failure. После output допускается только
source-derived continuation из зафиксированного partial context с новым
assistant-span и общей для этого шага конечной allowance, не повтор исходной
committed generation. Partial tool args не исполняются; committed/unknown
effects не replay и не превращаются в успех. EOF/failed/unknown-incomplete
без последующего terminal остаются non-success; явный
`response.incomplete(max_output_tokens)` — truthful finish `length`,
`content_filter` — failure, не generic retry. Title не получает main policy;
поддержанные compaction auxiliary paths имеют свою finite allowance.

Physical generation request counter включает фактически выданные attempts,
follow-ups/children/compaction/title и участвует в пределах owning turn;
logical round/step cap и finite retry allowances остаются отдельными.
Существующий test-campaign ceiling ≤24 requests — не новый production cap;
adapter/OpenProxy/SDK не получают второго retry слоя. Durable retry descriptor
на assistant-span — история и UI, не разрешение на dispatch после crash;
clearing привязан к смысловому началу следующего шага, не HTTP POST.
Точный contract/owners: [T54](goals/2026-09-29-provider-retry-parity.md),
T53/GO03 для будущих wires и T44/VIS43 для visual parity.

## Permissions

Default product profile: read/search в trusted project allow; apply_patch/bash/webfetch/MCP ask; skill/compress allow. Для live tests выделенный temporary fixture workspace с явно allowlisted operations. Режим полномочий authoring-agent не меняет автоматически permissions самого `oc`.

T50 canonical shell normalizes with legacy bash policy through the same ceiling;
legacy Deny/save patterns cannot be bypassed by changing names. New question/models
and session-control definitions require explicit native default/config admission,
never implicit allow for a missing central action. Profile overrides may only narrow;
session rename/move targets must be authorized independently of trusting the target
directory. Native target/path/credential ceilings remain declared donor differences.

`ask` без interactive channel — ApprovalRequired с ненулевым exit status. Parsing errors не превращаются в allow. Legacy `write`/`edit` permission entries нормализуются к patch operations; конфликтующие применимые policies разрешаются консервативно deny → ask → allow и фиксируются как difference. Нельзя объединять implicit default allow с explicit deny.

Project config/AGENTS, agent body, command template и skill body могут влиять на instruction/user/tool-result data, но не расширять trusted host boundary или central permissions. Agent restrictions только сужают policy, а admission всё равно повторяется при tool execution. Tool/MCP descriptions и fetched pages — untrusted input. Native extension — trusted code с правами процесса, не sandbox.

### Approved T44 approval contract (VIS36; implementation pending)

Application owns cancellable pending query/events and typed replies tied to actual
operation/session/pinned generation. Once is invocation-local; Always commits donor
project-scoped owner-generated save patterns before acknowledgement and reevaluates
pending requests. Ask is user-approvable; effective Deny and structural ceilings remain
authoritative. Plain rejection interrupts execution; feedback uses corrected continuation.
Waiting is pre-execution, not started/unknown effect; cancel/stale replies/restart never
authorize replay. Autoaccept answers once without grants with real Settings/config/CLI
controls; headless without a consumer fails, explicit supported --auto has a once-consumer.
This is a future implementation contract, not evidence that the channel exists today.

## Prompt assembly

Один runtime assembler строит provider input из typed lanes: compiled runtime/tool/security contract → selected session-agent body/base prompt (primary или child) → environment/date → ordered AGENTS/skill metadata/permitted MCP fragments → DCP fixed/nudge fragments → history projection → current user input. Каждая fixed lane имеет provenance/digest/byte accounting и stable delimiter. Expanded command и delegation/context pack остаются user input; loaded skill остаётся ordinary tool result, никогда system text. DCP не сжимает fixed config lanes и не добавляет их повторную копию на каждом turn.

Approved T45/R6–R10 target: selected nonempty agent.system replaces the base harness
prompt; environment/date, applicable AGENTS, skill metadata and permitted MCP/DCP
guidance remain separate. Same assembler serves root/child, with real native tool
descriptions/schemas separate from messages. Initial instruction baseline and
chronological changed/removed/nested-read fragments survive restart/compaction/Revert
within trust and immutable-generation boundaries, without rewriting old raw messages.

subagent defaults foreground, background:true returns running/sessionID while child
work progresses; independent foreground children overlap. Fresh child receives its
own profile/context and delegated prompt, not parent system/history; sessionID retains
its own conversation. Optional context_message_ids is a native addition: validate
parent branch/revision/request cutoff and text-only supported content, then attach
exact deduplicated chronological messages as quoted user data, not system instructions.
Budget validation precedes child/input creation. Snapshot/digest/provenance is durable
and immutable after admission; replay never duplicates it. Runtime-owned candidate
IDs are visible independently of DCP; harness/opaque/tool items are not candidates.
Automatic-context guidance and policy-backed capabilities prevent duplicate AGENTS/
tool schemas/skill bodies without hiding task-specific restrictions or user quotes.

Child DCP defaults allowed by allowSubAgents=true, gated by global/manual/policy and
own-session identity. Protect active task/pack while admitted work is pending/running;
terminal packs become normally compressible, safe recovery restores active protection.
No lifetime compress-call/block quota; keep per-call/graph/protected/model/active-memory
bounds. Owner amendment 2026-09-30 requires standalone hot replacement/intentional
forgetting, not flattening all previous summary/tool content. Fully covered old blocks
and unneeded closed call/result/reasoning groups leave hot context; retained groups and
current explicit protections stay intact. Release obsolete runtime task/pack ancestry.
Stable raw history/provenance/Undo is separate from provider memory; ordinary restart
and `/compact` restore latest committed hot state, never forgotten archive implicitly.
User `.md`/Git cold memory is entirely optional, with no automatic writes/commits/recall.
No full-archive/covered-member/mark loading is needed to renew hot context. Detailed
replacement, manual compact/recovery and qualification contract:
[R9/DCP11](DCP.md#infinite-hot-context--optional-cold-path--t45r9dcp11-pending).
This is approved work, pending implementation/evidence, not executed PASS.

## Canonical effort ordering — T47/VAR01 (approved 2026-09-27; pending)

One shared model/catalog ordering policy applies **after effective discovery + local/
static merge**, not to raw JSON/config keys or the discovery oracle. Available choices
and Ctrl+T traversal are `Default → none → minimal → low → medium → high → xhigh → max
→ custom → Default`, omitting absent/disabled entries and the exact reserved `default`
UI sentinel. “At least low/medium/high/xhigh/max” means their relative order **when
supported**, never manufacturing capabilities or enabling missing allowlist entries.

- Rank by the exact known explicit `reasoningEffort` first. An alias `fast` with
  effort `low` belongs at low; name `low` with effort `high` belongs at high.
- Only if effort is absent, use a known standard variant name as the rank. An
  explicitly unknown effort remains custom even when its name is standard. Match
  case-sensitively; case/whitespace/unknown spellings stay custom. Existing validation
  still rejects malformed metadata; ordering does not repair it or trim identifiers.
- Known ranks precede custom entries. Equal-rank aliases and all custom entries retain
  effective source order; aliases remain separate choices, never deduplicated by effort.
  No independent lexical sort may compete with this view. Model/variant pickers,
  application snapshots/cycle, enabled-choice diagnostics and T50 `opencode_models`
  consume the same policy. Model/provider ordering itself is unchanged.
- Default means no variant overlay and differs from the named `none`. Ranking by name
  alone does not synthesize a wire effort. Exact name/ID, not position, is selected and
  persisted; reorder/refresh/reopen/restart preserves identity. Retired/disabled choice
  keeps actionable diagnostics, not a silent substitute. Cycle from a stale choice
  follows the existing explicit Ctrl+T → Default rule; no named choices means a no-op.
- Preserve exact configured wire values, allowlist/merge precedence, profile/subagent
  selection and immutable turn/config-generation boundaries. No global JSON sorting,
  serde order-feature change, model-ID/reasoning allowlist or provider fallback.

This is an owner-approved native ordering difference from pinned OC2's declared order,
not full upstream pixel parity. T47 owns VAR01 behavior; T44 VIS09/VIS29 qualifies
presentation using the same ordered fixture on both sides and separately records
unsorted-fixture differences. Existing statuses and historical PASS remain unchanged.

## Provider connections и единые credentials — T53 (approved 2026-09-29; pending)

Frozen behavior/precedence, endpoint authority, protocol/replay boundary и ordered
slices: [T53](goals/2026-09-29-opencode-go-and-provider-auth.md). Это future contract,
не утверждение, что connect/no-auth/Go уже реализованы.

- Connection config (provider/protocol/baseURL/headers/options/model source), auth
  policy None/Key/OAuth и secret material — разные факты. Один existing native Db
  owner хранит tagged Key/OAuth accounts; no auth.json/import/dual-write. None не
  credential row и не missing Key; unsupported OAuth никогда не становится Key.
- Go: active stored → OPENCODE_API_KEY → configured Key после preset authority
  admission. OpenProxy/custom: explicit configured Key сохраняет приоритет, scoped
  stored account — при отсутствии; no implicit foreign provider env inheritance.
  Custom None явно anonymous, без auth header/dummy key. Existing config default Key.
- Stored/default-env credentials не авторизуют произвольный URL: built-in Go имеет
  owned HTTPS prefix, custom namespace включает provider + endpoint/auth scope.
  Source trust/env substitution сами по себе не credential-domain admission.
- Public catalog availability, credential source и generation auth rejection
  независимы. Go models.dev refresh не валидирует key/не очищает auth failure.
  Effective source/active/effective account публикуются safe DTO, не material.
- Optional provider-qualified ModelRef проходит actions/preferences/UI/restart;
  unavailable selection не подменяется. Unready request отказывает до effects.
  Credential/config/model changes создают новый binding; in-flight работа pinned.
- Typed credential operations используют existing CoreApp/application/storage
  owners и expected generation; labels/summaries и acknowledgements safe, secret
  input ephemeral/redacted вне composer/history/drafts/Debug. Без RPC/auth framework.
- New protocol/non-secret binding provenance сохраняются во всех journal/SQL/fork
  projections; opaque replay/checkpoints только compatible, raw history неизменна.
  Absence protocol — legacy Responses, explicit unknown — safe failure. Token bytes
  и session telemetry не deployment identity; no unknown-effect replay.

## Limits и context admission

Резидентная память ограничивается отдельными byte/item caps, а не одним параметром «context tokens». Проверять event size, tool argument bytes, active context serialization, attachment encoding, queued bytes и outputs независимо.

`output_budget = min(requested_or_default, model.output)`; допустимый input не больше `min(model.input если задан, model.context - output_budget) - safety_margin`. Если model limit отсутствует, не выдумывать модельную ёмкость по имени: UI указывает unknown; generation использует явно configured native fallback cap и предупреждение. В fallback caps нет утверждения о реальной upstream модели.

Native policy (D14/T47): `provider.<id>.options.nativeFallbackLimits` accepts positive
`context`/`output`, defaults 32768/4096; context must exceed output + 1024 reserve.
Missing fields use defaults; unknown option keys/types are rejected. These values
are request policy, never discovery metadata or provider wire options. Each known
positive model limit remains authoritative. The configured output is also the
default output request for known models; explicit output requests are clamped.
The same budget/admission applies to primary, child, every tool continuation and
title requests. Missing/zero metadata produces visible fallback warnings; title
admission failure skips only the ancillary title. No selected variant means no
reasoning overlay; an explicitly selected variant must be enabled.

Универсального точного токенизатора для всех aliases не предполагается. Использовать доступный validated tokenizer или conservative estimate с отметкой estimated, затем калибровку по usage. В estimate входят instructions, tool schemas, summaries, opaque items и attachments (для неизвестной image token cost — дополнительный reserve, не нулевой учёт). Provider остаётся окончательным арбитром context error.

DCP soft nudges не равны hard admission. Existing one-rebuild/recovery guard относится
к одному provider logical step, не к сроку жизни сессии и не запрещает последующие
явные операции сокращения. По owner amendment 2026-09-30 user-reachable `/compact`
может подготовить bounded eligible hot prefix/replacement по metadata, даже если
полный before не проходит host/model admission; summary request проходит собственный
бюджет, а забывание целой допустимой закрытой группы не требует чтения её payload.
Проверять реальный wire gain/partial progress и возможность продолжения той же сессии;
не требовать full overflowing before для measurement и не предлагать недоступный
compress как единственный выход. Current irreducible input/policy даёт явный outcome,
а bad/no-gain attempt оставляет возможность другого eligible selection/replacement.
Нельзя бесконечно отправлять тот же переполненный запрос, добавлять hidden chunk-summary
loop или тайно truncate protected/current messages. Намеренное admitted hot forgetting
закрытых диапазонов по R9/DCP11 не является таким несанкционированным truncation.
