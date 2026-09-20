# Tools и MCP — один понятный рабочий путь

Product tool list является явным выбранным подмножеством OpenCode, не «все инструменты upstream». Названия/описания одинаковы для любых моделей; no GPT-vs-other write branch.

## Built-ins

`read`: bounded чтение файла/диапазона строк с path/offset/limit, явные truncated/next cursor и blob reference при необходимости. Директории и binary contents обрабатываются явно; не грузить весь репозиторий. `glob` и `grep`: stable sorted paginated matches, bounded files/bytes/time, исключение own data dir и symlink escapes. Plain literal/regex режимы явно различимы; не писать custom regex engine.

`apply_patch`: один JSON argument `patchText` с upstream-style `*** Begin Patch` / Add File / Update File / Delete File / Move to / End Patch. Не смешивать с provider-hosted Responses `apply_patch` schema. В M2 сохранить parser fixtures выбранного OpenCode baseline; политика unsafe paths — наша.

Обязательные случаи: новый текстовый файл; новый пустой файл; добавление текста в существующий пустой файл через Update; точечные hunks; полная замена текстового содержимого через patch; delete; move/update без overwrite чужого target. Add existing path — conflict, не silent overwrite. CRLF/Unicode/last newline проверены; binary patch вне scope с точной диагностикой.

Парсер сначала строит plan всех операций, validates grammar/sizes/paths/conflicts и checks permissions. Затем per-file preimage check и запись temp file в той же директории, fsync, atomic rename; preserve mode в рамках разрешений. Не обещать all-files atomicity. Ошибка после части commits → partial result с точными paths/hashes; crash ambiguity → unknown. Preimage fingerprint защищает от обычных concurrent edits, но сам по себе не исключает malicious TOCTOU: применять directory-relative no-follow APIs и запрещать symlink mutation в первом профиле. Не выдавать `canonicalize()` за полную защиту.

Для удаления/rename сохранять достаточную operation metadata в own storage; не превращать это в snapshot/undo subsystem. Не запускать `git reset`, не делать auto-rollback на пользовательские файлы. Модель получает concise diff/status; полный diff в bounded blob.

`bash`: process group, explicit cwd = trusted project, executable shell из проверенной настройки, command bounded, stdin protocol documented. Concurrent drain stdout/stderr предотвращает deadlock. Preview отдельно от bounded retained output; после output cap продолжать drain/discard со счётчиком, а не держать growing string. Exit code/signal/timeout/cancel различаются. TERM→grace→KILL→wait. Noninteractive tool не поддерживает скрытый бесконечный terminal session; долгие процессы получают явный timeout/outcome.

`webfetch`: read-only GET, http/https, text/HTML/JSON response; bounded download и redirect count, понятное преобразование HTML→text, исходный URL/status/content type и source references в результате. Не browser automation и не OpenProxy private admin fetch. Private/link-local/loopback targets запрещены по умолчанию; DNS resolution, фактический dial и каждый redirect проверяются вместе. Explicit trusted endpoint exception для provider/MCP НЕ распространяется на модельный webfetch. Credential headers не наследуются; proxy env не должен обходить egress policy. Для test fixtures использовать отдельный explicit loopback allowlist. No arbitrary methods/upload/cookies.

`compress` — отдельный контекстный tool из DCP.md; файл на диске не меняет.

`skill`: input `{id}` выбирает skill только из pinned generation текущего turn. Model-visible descriptor/catalog содержит bounded id/name/description, но не body. Executor проверяет stale generation и central/agent-narrowed permission, записывает durable intent/outcome и возвращает immutable bounded snapshot body с digest и redacted provenance. Unknown/removed/oversized/unreadable skill — visible failure. Tool не перечитывает filesystem, не регистрирует другие tools/MCP, не запускает scripts и не меняет permissions/agent/model.

## Permissions caveat

Универсальный patch убирает дублирование модельных tools, но shell технически может писать через cat/python/компилятор, а MCP может иметь свои side effects. Инструкция предпочитать patch для source edits — behavioral contract, не OS isolation. Runtime и агент не должны заявлять обратное. Build artifacts нормально создаются dev tools.

## MCP transport

Использовать официальный Rust SDK rmcp; выбрать/pin version/features, которые реально проходят протокол нужного server. Наличие SDK dependency не означает готовый adapter. Не писать собственный MCP stack ради неподтверждённого недостатка; сначала bounded compatibility spike.

`codex_web`: exact URL `{env:LUDKA2_API_URL}/mcp`, bearer из headers, enabled:true, oauth:false, timeout 60000 ms. Pinned OpenProxy route `/v1/mcp` принимает POST и JSON responses, требует negotiation/header version `2025-11-25`, предоставляет tool `search`; этот конкретный path проверен в [P3]. BaseURL обычно должен уже содержать `/v1`. Не чинить URL probing/redirects автоматически.

HTTP client умеет JSON и SSE ответы там, где transport их предлагает. У OpenProxy stateless JSON mode отсутствие session ID или GET event stream не должно порождать reconnect loop. Если SDK optional GET получает 405 — не считать это отказом исправного POST path; соответствие спецификации и поведение SDK проверить. Отправлять нужный `MCP-Protocol-Version` после initialize; no OAuth auto-discovery при oauth:false. Remote service timeout может быть меньше нашего клиентского 60s — сохранять исходный error, не ложно ждать/повторять операцию.

Local `chrome-devtools`: argv точно из config, без shell splitting/rewrite. `enabled:false` означает: не launch npx, не probe browser, не требовать Node, не делать install. При true — stdio JSON-RPC, stdout только protocol, stderr в ограниченный redacted log, process group lifecycle. Нужны runtime и уже доступный browser-url; `oc` не запускает Chrome с произвольным профилем пользователя.

`chrome-devtools-mcp@latest` — явная пользовательская команда; её не подменять pinned silently. Core adapter acceptance — pinned fake stdio server. Optional real-browser smoke записывает фактически разрешённую версию/digest в evidence; это не воспроизводимость `@latest` навсегда.

## Catalog и вызовы

После initialize/capabilities — bounded paginated tools/list; registry отображает server/tool identity в stable wire-compatible function name с collision checks. Exact original имя сохраняется для tools/call. Никакого исполнения MCP description как инструкций. Не truncate каталог так, чтобы часть необходимого tools исчезла без diagnostics; oversized catalog явно LimitedCatalog/UnsupportedCapability.

Каждый call, включая native `skill`/`compress`: schema validation, generation lookup, общий permission pipeline, call timeout/cancel, structured result/error, bounded content/blob, durable outcome. annotations не заменяют permission policy. Protocol/network error и MCP `isError:true` различаются. Незнакомая output modality не превращается в plain success text. Повтор side-effect tool после network error запрещён без нового явного решения.

MCP клиент живёт в Location/config generation и не разделяется между разными auth/cwd. Reload между turns закрывает старый client; disabled entries не держат processes/requests. List-changed events инвалидируют только соответствующий catalog с controlled refresh. No session-global unbounded map of old generations.

## Direct exposure vs Code Mode

В `oc-rs.toml` профиль явно `tool_exposure = "direct"`. При отсутствии upstream codemode field выбранный профиль означает direct — это объявленное отличие. Explicit true = actionable unsupported error. Не запускать JS interpreter, Node eval, Code Mode shim или remote execute to emulate missing interpreter.
