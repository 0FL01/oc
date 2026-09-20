# Application, состояния и ошибки

Этот документ — собственный контракт Rust-приложения. Upstream wire compatibility описана отдельно; внутренние DTO не обязаны копировать HTTP schema OpenCode.

## Application API

Команды: CreateSession(Location), SubmitInput, CancelTurn, AnswerPermission, CompressRequest (user prompt command), SelectModelVariant, SelectPrimaryAgent(expected generation), InvokeCommand(expected generation), ReloadConfig, SelectLocationSession, Shutdown. Запросы: ListSessions(cursor), ReadHistory(cursor, limit), GetSessionSnapshot, ListModels, WorkspaceSnapshot(redacted), EffectiveConfig(redacted), DcpStats. Конкретные Rust signatures фиксируются в M1; не генерировать универсальный RPC framework.

`SubmitInput` возвращает durable accepted ID либо ошибку. При переполнении очереди — Busy, не потеря input. Session IDs/application operation IDs непрозрачны; clock/ID provider в tests заменяемый. Ошибка provider не уничтожает пользовательский ввод.

Workspace actions несут expected `(LocationId, ConfigGenerationId)` и отвергают stale UI requests. `InvokeCommand` сохраняет command ID/generation, original invocation и одно expanded user input, затем использует тот же durable admission, что обычный `SubmitInput`; TUI не исполняет definition самостоятельно. `SelectLocationSession` выбирает/создаёт session target Location и не меняет Location существующей session.

Live hints: TextDelta, ToolProgress, ContextStats. Durable outcomes: InputAccepted, TurnStarted, ToolStarted/Finished, CompressionCommitted, TurnFinished/Failed/Interrupted. Каждое outcome имеет session_id, turn_id, seq и типизированные details. Не включать credentials/raw HTTP headers.

## Turn state machine

`idle → preparing → streaming → tool_pending → tool_running → preparing` до финального ответа; любой live state может перейти в `cancelling → interrupted`, `failed` или `completed`. WaitingApproval — отдельное ожидание с возможностью cancel, не held DB transaction.

Каждый provider response может иметь несколько tool calls. Аргументы собираются bounded по call ID; tool НЕ исполняется по частичному JSON. Сначала закрыть/validate response и завершить arguments, затем admission tools. Duplicate call IDs, unknown tool и несоответствие schemas — typed errors. Параллелизм tool calls из wire не вынуждает параллельный execution.

Пустой stream EOF без terminal completion — interrupted/failed, не успех. Не выполнять tool дважды из-за повторного done-event. Тесты включают arbitrary chunk split, CRLF, partial UTF-8 и terminal error after text.

## Tool operation state

`planned → authorized → started → succeeded | failed | cancelled | partial | unknown`.

Intent записывается до side effect; outcome — после подтверждения. Crash после начала и до фиксации результата даёт unknown, даже если transcript выглядит незаконченным. Read-only call можно повторить новым operation ID в рамках новой явной попытки; mutation, shell и MCP не autoretry по старому ID. Exactly-once не обещается.

Multi-file patch не атомарен целиком: preflight всех entries уменьшает риски, но каждый файловый commit отдельный. Partial outcome сообщает committed paths/ещё не применённые paths и recovery instructions. Не откатывать чужие правки автоматически.

## Errors

ConfigInvalid/UnsupportedCapability/UnsupportedPlugin/MissingCredential/StaleGeneration/UnknownAgent/UnknownCommand/UnknownSkill/UnknownModel/InvalidVariant; PermissionDenied/ApprovalRequired; Conflict/PathDenied/PatchInvalid; ProviderAuth/RateLimited/ProviderTimeout/ProtocolError/ContextLimit; McpUnavailable/McpProtocol/McpToolError; StorageFull/StorageBusy/DataRootBusy; Cancelled/Interrupted/UnknownOutcome.

Retry только в одном явно указанном месте. Discovery имеет собственный user-defined bounded retry. MCP invocation retry по умолчанию отсутствует. Генерация не повторяется после first committed response event/текстовых deltas; initial transient failure допускает не более 2 новых попыток в текущем turn при отсутствии side effects, с видимым счетчиком и bounded Retry-After. 400/401/403/413/422 не циклятся; context error не приводит к молчаливой потере сообщений.

Изначальные provider retries — технический default; они не копируются в OpenProxy, не размножаются в HTTP adapter/SDK layers. Суммарный request counter входит в turn limit.

## Permissions

Default product profile: read/search в trusted project allow; apply_patch/bash/webfetch/MCP ask; skill/compress allow. Для live tests выделенный temporary fixture workspace с явно allowlisted operations. Режим полномочий authoring-agent не меняет автоматически permissions самого `oc`.

`ask` без interactive channel — ApprovalRequired с ненулевым exit status. Parsing errors не превращаются в allow. Legacy `write`/`edit` permission entries нормализуются к patch operations; конфликтующие применимые policies разрешаются консервативно deny → ask → allow и фиксируются как difference. Нельзя объединять implicit default allow с explicit deny.

Project config/AGENTS, agent body, command template и skill body могут влиять на instruction/user/tool-result data, но не расширять trusted host boundary или central permissions. Agent restrictions только сужают policy, а admission всё равно повторяется при tool execution. Tool/MCP descriptions и fetched pages — untrusted input. Native extension — trusted code с правами процесса, не sandbox.

## Prompt assembly

Один runtime assembler строит provider input из typed lanes: compiled runtime/tool/security contract → selected primary-agent body → ordered AGENTS fragments → DCP fixed/nudge fragments → history projection → current user input. Каждая fixed lane имеет provenance/digest/byte accounting и stable delimiter. Expanded command остаётся user input; loaded skill остаётся ordinary tool result, никогда system text. DCP не сжимает fixed config lanes и не добавляет их повторную копию на каждом turn.

## Limits и context admission

Резидентная память ограничивается отдельными byte/item caps, а не одним параметром «context tokens». Проверять event size, tool argument bytes, active context serialization, attachment encoding, queued bytes и outputs независимо.

`output_budget = min(requested_or_default, model.output)`; допустимый input не больше `min(model.input если задан, model.context - output_budget) - safety_margin`. Если model limit отсутствует, не выдумывать модельную ёмкость по имени: UI указывает unknown; generation использует явно configured native fallback cap и предупреждение. В fallback caps нет утверждения о реальной upstream модели.

Универсального точного токенизатора для всех aliases не предполагается. Использовать доступный validated tokenizer или conservative estimate с отметкой estimated, затем калибровку по usage. В estimate входят instructions, tool schemas, summaries, opaque items и attachments (для неизвестной image token cost — дополнительный reserve, не нулевой учёт). Provider остаётся окончательным арбитром context error.

DCP soft nudges не равны hard admission. При превышении hard cap разрешён максимум один явный recovery/compress attempt только с context, который можно отправить; если неприменимо, ContextLimit и сохранённая история. Нельзя пытаться послать уже переполненный запрос бесконечно или тайно truncate protected messages.
