# OpenProxy: native Responses и model discovery

Источники: USER-Q01 и USER-DISCOVERY; pinned OpenProxy [P1–P4], AI SDK [W1], OpenAI reasoning [W4] в `SOURCES.md`. Внешние source-факты и принятый Rust transport design не смешивать.

## Граница

Одна native protocol family: OpenAI Responses. `npm: "@ai-sdk/openai"` — compatibility alias. OpenProxy делает свои OAuth/vendor/account routing; `oc` отвечает за историю, tools, context projection, workflow и свои retries. Не обращаться напрямую к Codex/GLM/OpenCode Go API и не внедрять их model lists.

Pinned proxy имеет `/v1/responses`, `/responses` и совместимые alias routes; это не основание для blind probing всех URL. Конечный generation URL = `baseURL` без завершающих `/` + `/responses`; discovery = такой же base + `/models`. Prefix path сохраняется. MCP URL задаётся отдельно. Нет автоматического добавления/удаления `/v1`.

Rust design default: `store:false`, explicit local input projection, function tools, streaming SSE. `previous_response_id` не используется в первом scope, чтобы DCP и restart не зависели от неизвестного server-side transcript. Proxy поддержка exact request shape устанавливается T02/T16; при несовместимости — recorded protocol blocker, не тихая смена API.

## Config options

`baseURL`: http/https, без userinfo/query/fragment. Взять только из trusted provider config; remote catalog не может изменить URL. HTTPS по умолчанию; явно настроенный loopback/local HTTP разрешён. Не отключать certificate validation из-за ошибки.

`apiKey`: Bearer header; env/file resolution с redaction. Не попадать в model-visible options, logs и key hash, который можно восстановить. Пользовательский Authorization переопределяется apiKey для discovery, как в присланном коде. Headers остаются scoped этому provider origin; redirects для authenticated discovery/generation запрещены, чтобы не пересылать ключи.

`timeout:false`: нет общего generation deadline в provider adapter; отдельно остаются connect timeout и cancellation. `chunkTimeout:6000000` = 6 000 000 миллисекунд = 100 минут idle между body chunks. Не заменить «разумными 60s». Idle timer по полученным bytes; complete SSE event/arguments независимо ограничены по размеру. Heartbeats могут поддерживать stream при unlimited total — это видимая пользовательская настройка, не гарантия конечности. Discovery имеет совершенно отдельные сроки.

`setCacheKey:true`: передавать стабильный, не содержащий secrets, session/provider-scoped `prompt_cache_key`; конкретное upstream-compatible построение сравнить с baseline wire fixture. Cache key не меняется на каждый chunk/retry; reset на смене provider identity. DCP меняет prompt prefix, поэтому cache hit не обещается. Не добавлять vendor-specific cache anchors.

Reasoning effort — непустая строка из выбранного effective variant, передаваемая в `reasoning.effort`; без жёсткого enum по имени модели. Disabled variants не отображаются/не выбираются. Не объявлять несуществующую возможность из одного имени модели.

## Discovery: авторитетная версия

Эталон `references/openproxy-models.user.mjs` — присланная владельцем версия, форматирование нормализовано. В pinned OpenProxy plugin обнаружен более старый timeout 10000 ms. Его нельзя подставить вместо user contract.

Discovery на startup/config generation только для `ludka2` в приложенном native profile; механизм reusable для любого явно включённого provider ID. `ludka` static по умолчанию; можно явно opt in тем же native setting. Exact admitted `{plugin,plugins}/openproxy-models.js` marker включает этот compiled module, но JavaScript contents не импортируются/исполняются и не заменяют user reference semantics. Basename вне effective config root, `.ts`, URL и arbitrary path unsupported. Duplicate native profile/marker activation idempotent. Никаких background polls до реальной потребности. UI не блокирует rendering: models = loading/ready/failed, только dependent run ждёт результата.

Перед запросом: enabled/disabled provider filters → snapshot исходных user-configured models → validate URL/apiKey → build headers. Disabled provider не делает network, не требует отсутствующего секрета. GET, `Accept:application/json`, redirects:error.

Timing: monotonic total deadline 30000 ms, per attempt `min(15000, remaining)`; не более 4 attempts; delays 250/750/1500 ms clipped к remaining. Timer покрывает headers И чтение/parse body. Нет спящих задач после cancel. 401/403 и прочие не-retryable статусы terminal; retryable 408,425,429,5xx, network/timeout, invalid JSON и empty data list. Структурно валидный JSON неправильной shape — terminal. Invalid model row обнаруживается после fetchModels и НЕ запускает ещё один сетевой retry.

Require body.object == `list`, data array и nonempty. Empty list после bounded retries не авторитетное удаление всего каталога. Дополнительный safety cap: body 8 MiB и 10000 rows; это наш announced difference, превышение — bounded failure без частичной публикации.

## Row normalization — точный перенос semantics

ID: строка с непустым trim, исходные bytes не обрезать; `/` внутри ID сохраняется. Не split vendor/model для routing. Pretty name убирает только первый prefix для отображения; исходный request ID не меняется. GLM/GPT formatting и source suffix как в reference; source добавляется ровно один раз.

Положительное число означает JS safe integer `1..9007199254740991`, не arbitrary u64. Row `context_length`/`max_completion_tokens` и `opencode.limit` преобразуются в limits. `opencode:null` эквивалентно отсутствию (`?? {}`); array/non-object invalid. Source должен быть уже trimmed и nonempty. Если modalities задан, input И output должны быть arrays со значениями text/image/audio/video/pdf. Booleans attachment/reasoning/tool_call validate строго. Неподдержанная продуктом modality может оставаться metadata; это не разрешение реально отправлять audio/video/pdf.

Remote metadata allowlist: name/source/limit/modalities/attachment/reasoning/tool_call/variants. Не принимать remote npm/baseURL/apiKey/headers/options/extensions. Unknown data не исполняется.

Variants: либо непустой `reasoningEffort` и отсутствующий disabled, либо disabled:true при отсутствующем effort. Если remote variants задан, отсутствующие standard keys none/minimal/low/medium/high/xhigh/max явно disabled. Arbitrary extra variant names допустимы. Это authoritative allowlist, не повод добавить default reasoning values SDK.

Merge: для ID из успешного remote list remote ← local top-level override; limit и variants — отдельный shallow per-field merge. Original configured snapshot не заменяется результатами предыдущей попытки. Explicit local overrides могут переопределить remote disabled; UI показывает origin. Context/input только discovery-path clamp к 500000; output не clamp этой константой. Static `ludka` 628000 не обрезать из-за discovery другого provider. Модельный input/output затем совместно проверяются admission rule в CONTRACTS.

После merge нет positive context И output — весь limit удаляется, как в JS. Не вставлять фиктивную пару значений «ради schema». Duplicate IDs в successful list — последняя запись wins (Object.fromEntries behavior); тестировать и документировать, не незаметно менять на first-wins/reject. Всё response проходит validation до атомарной публикации.

Successful discovery REPLACES set of model IDs: local-only/removed IDs не переживают удаление upstream. Failure оставляет существующий catalog unchanged; на cold startup это configured models. Runtime explicit refresh может сохранять предыдущую опубликованную generation, но local merge source всегда только исходный user config; это explicit extension к startup hook, не смешение discovered values с local overrides. Failed cold startup с пустыми models = NoModels + diagnostic, без hidden catalog fallback.

Ошибки: только заранее заданные redacted categories/status; не логировать raw exception, URL, request/response body и auth.

## Wire stream и reasoning

Сохранить causal порядок output items, call_id/item_id, usage, reasoning summary и opaque continuation content. Official Responses reasoning documentation описывает передачу relevant output/reasoning items между tool steps [W4]; Rust не должен сохранять только видимый текст. Opaque data не интерпретируется и не показывается в TUI/logs. Проверить `reasoning.encrypted_content`/store:false replay в wire spike, не считать все proxy aliases одинаковыми.

Stream parser incremental: bounded SSE buffer, multiline data/CRLF/UTF-8 fragmentation/comments. Complete function arguments validate before execution. Terminal error/incomplete/EOF различимы. Unknown event type не ломает stream, но unknown mandatory output item не silently dropped. При несовместимой schema — явная ProtocolUnsupported.

Для tools использовать ordinary function schema `apply_patch`/others, НЕ provider-hosted special apply_patch tool. При generic MCP schemas явно выбрать non-strict function input mode, если поддерживается wire; не превращать все properties в required автоматически. Зафиксировать тестом фактическое shape.

Изображения — ограниченные локальные attachments с MIME validation и controlled inline encoding. Не fetch произвольный model-supplied image URL с credentials; webfetch отдельный path. Advertised modality и реально квалифицированная совместимость — разные статусы.

Live verification подтверждает конкретный proxy deployment/model/variant. Не распространять один успешный run на все модели из /models.

## Live test credentials (боевые тестовые, проверяемы)

Владелец выдал тестовые credentials для live-проверок (T16/T27): файл `.local/live.env` (gitignored, в индекс не попадает):

```sh
source .local/live.env   # LUDKA2_API_URL, LUDKA2_API_KEY, OC_TEST_MODEL
cargo test -p oc-adapters --test e2e_live -- --ignored --nocapture
```

Опционально: `OC_TEST_VARIANT`, `LUDKA2_MCP_URL` (exact codex_web URL, тот же ключ как bearer).
Проверенные deployment/models: `https://ludka2.bash8.de/v1`, `ocg/muse-spark-1.3-contributor`, `cx/gpt-5.6-luna` (39 IDs в /models на момент проверки).

Правила: сам ключ — только в `.local/live.env`, никогда в docs/evidence/логи/diff; в отчётах фиксировать только факт presence, deployment, model/variant, счётчики и sanitized статусы. Отсутствие env — `BUILD_READY_LIVE_BLOCKED` для live-части, не PASS и не провал офлайна.
