# Tools и MCP — один понятный рабочий путь

Product tool list является явным выбранным подмножеством OpenCode, не «все инструменты upstream». Схемы/имена каждого инструмента стабильны; owner-approved T50/R1/R9 выбирает файловое семейство по точному OC2 model.id predicate, а не provider/display name. Это pending tool-selection exception, не model-ID routing/discovery/reasoning allowlist.

Owner-approved [T50 contract](goals/2026-09-27-native-tool-parity.md) defines the
pending target catalog: read/glob/grep/(apply_patch OR edit/write)/shell/webfetch/skill/question/
subagent/compress plus opencode_models/opencode_session_rename/opencode_session_move.
Effective policy/config still filters tools. No built-in websearch/provider selector,
Code Mode/execute or built-in browser; explicit admitted MCP search/browser remains.
PDF stays unsupported. This amendment is not a claim that the new tools work today.

File-tools rule is case-sensitive: model.id contains `gpt-` and neither `oss` nor
`gpt-4` → only apply_patch; otherwise only edit/write. On user model switch the very
next request removes incompatible definitions/catalog/managed guidance, including
follow-ups, root/own-model-child, restart and DCP/native-compaction reconstruction.
One captured view per prepared request drives budgets/fingerprints/previews and its
tool execution. Picker changes a local draft; committed model/variant switch is
accepted while busy and reloaded before the next request of the same task, not only
the next user turn. Blank Enter in an existing ordinary composer can commit without
new text. A's already issued file tool keeps A's allowset/approval identity after B
is selected, with normal policy/preimage checks; B receives compatible retained
outcomes, not alien opaque state or replay. Raw historical calls/results are not
erased/translated; draft alone or commit after final answer creates no request.
Detailed [live-selection contract](CONTRACTS.md#live-model-selection--t50r1tool12-approved-2026-10-01-pending)
and evidence: T50/R1/R9/TOOL12/TOOL20, T45/PRM01 and T44 visual consumers.

## Built-ins

`read`: bounded чтение файла/диапазона строк с path/offset/limit, явные truncated/next cursor и blob reference при необходимости. Директории и binary contents обрабатываются явно; не грузить весь репозиторий. `read`, `glob`, `grep` и файловые mutations (`apply_patch`/`edit`/`write`) отклоняют own data root, включая direct path и symlink escape. `glob`/`grep` дают stable sorted paginated matches с bounded files/bytes/time. Plain literal/regex режимы явно различимы; не писать custom regex engine.

`apply_patch`: один JSON argument `patchText` с upstream-style `*** Begin Patch` / Add File / Update File / Delete File / Move to / End Patch. Не смешивать с provider-hosted Responses `apply_patch` schema. В M2 сохранить parser fixtures выбранного OpenCode baseline; политика unsafe paths — наша.

Native strict profile: Add body состоит только из `+`-строк (префикс снимается);
empty Add создаёт zero-byte файл. `*** Move to:` идёт непосредственно после
`*** Update File:`, до hunks. `@@ section` выбирает точный уникальный context,
`*** End of File` привязывает последний hunk к концу. Неоднозначный preimage,
повторные/пересекающиеся normalized paths и пустые update hunks — conflict/error.
Нет fuzzy matching, shell/heredoc wrappers и aliases `patch`/`text`. CRLF и
отсутствие final newline существующего файла сохраняются (объявленное отличие
от upstream derive, добавляющего newline). Выбранные source-derived fixtures:
`fixtures/patch.json`, pinned O1; MIT notice: `fixtures/OpenCode-MIT.txt`.

Обязательные случаи: новый текстовый файл; новый пустой файл; добавление текста в существующий пустой файл через Update; точечные hunks; полная замена текстового содержимого через patch; delete; move/update без overwrite чужого target. Add existing path — conflict, не silent overwrite. CRLF/Unicode/last newline проверены; binary patch вне scope с точной диагностикой.

Парсер сначала строит plan всех операций, validates grammar/sizes/paths/conflicts и checks permissions. Затем per-file preimage check и запись temp file в той же директории, fsync, atomic rename; preserve mode в рамках разрешений. Не обещать all-files atomicity. Ошибка после части commits → partial result с точными paths/hashes; crash ambiguity → unknown. Preimage fingerprint защищает от обычных concurrent edits, но сам по себе не исключает malicious TOCTOU: применять directory-relative no-follow APIs и запрещать symlink mutation в первом профиле. Не выдавать `canonicalize()` за полную защиту.

Весь preflight вычисляет before/after и проверяет каждый hunk до первого изменения;
лимиты: patch 2 MiB, файл 8 MiB, сохранённые preimages/results плана 64 MiB.
Linux implementation удерживает directory handles, проходит parents через
`openat(O_DIRECTORY|O_NOFOLLOW)` и использует только single-component имена в
mutation syscalls. Temporary inode — `O_CREAT|O_EXCL|O_NOFOLLOW`, новое случайное
имя; final Add/Move — `renameat2(RENAME_NOREPLACE)`. Update/Delete повторно
сверяют identity/metadata/bytes перед commit; это не kernel compare-and-swap и
не защита от враждебного same-UID writer в последнем syscall window. Права нового
файла ограничены 0600/umask, update сохраняет ordinary mode bits. Перед rename
синхронизируется файл после chmod, после namespace changes — затронутые directories,
включая созданные parents. Если sync или Move после Update падает, outcome уже
содержит совершённый Update с полными hashes; rollback не обещается. Crash может
оставить staging file, но не вызывает автоматический replay.

Для удаления/rename сохранять достаточную operation metadata в own storage; не превращать это в snapshot/undo subsystem. Не запускать `git reset`, не делать auto-rollback на пользовательские файлы. Модель получает concise diff/status; полный diff в bounded blob.

`write` (T50/R9 pending): `{path,content}` создаёт/перезаписывает файл, включая пустой
content и missing parents; donor BOM/newline semantics, bounded regular-file preimage
и реальные target/resource/existed/operation metadata. Не эмуляция Add File patch.

`edit` (T50/R9 pending): `{path,oldString,newString,replaceAll?}`; существующий файл,
unique match по умолчанию, все matches при true, фактический replacements count.
Donor precedence: exact nonoverlapping → typography-normalized → trailing-whitespace
line matching; CRLF/BOM сохраняются. Empty/identical old, no match и ambiguity —
error; empty new удаляет совпадение. Это отдельная edit семантика, не fuzzy patch.

Все три mutation tools используют общий canonical apply_patch permission/grants
owner, actual resources, before/after approval preview и approved-preimage recheck
после ожидания. Effective Deny/Plan/child/data-root/no-follow/protected paths и caps
не обходятся выбором модели. Intent до effects, outcome/confirmed diffs после; crash
не разрешает replay. Нет prerequisite отдельного read call. T44 VIS35 показывает
Write/Edit/ApplyPatch отдельно; replay использует persisted input/results/effects,
не сегодняшние файлы. No Formatter/LSP subsystem в этом файловом срезе.

`shell` (T50 target): command/workdir/timeout/background through the actual configured Linux shell; default foreground timeout120000ms, explicit0 disables execution timeout, background default has no execution timeout. Preserve legacy `bash(argv/cwd/timeout_ms)` through one supervisor/policy owner with unambiguous schema normalization, not duplicate advertised tools or a Deny bypass. Process group, trusted cwd, minimal documented credential-free env, concurrent drains, bounded preview/retained output and TERM→grace→KILL→wait remain. Background returns running/shellID after launch and later an automatic durable terminal notice, without polling; output/jobs/queues/teardown stay bounded even with no execution timeout. Native unknown effects never auto-replay; no hidden persistent terminal manager or sandbox claim.

Owner amendment 2026-10-01 extends T50/R2/TOOL13 with authoritative source-session
running shell inventory/status, bounded live output/final flush, selected-job kill
and same-process foreground Ctrl+B conversion. Immutable initial `running` result
is not live job status. Original session/Location/generation and durable notice
identity remain; repeated/terminal/cancel races never spawn again or duplicate results.
T44/VIS39 owns paired Shell rows/output viewer. This is command-shell lifecycle,
not an attached interactive PTY or a new model polling/terminal tool.

The old arbitrary-terminal-manager exclusion is narrowly superseded by approved
[T56/TERM01](goals/2026-10-01-native-session-terminals.md): explicit user-created
session-local native PTYs selected from Terminals and shown in the right pane.
They survive hide/show/session view changes within live `oc`, not application
shutdown/daemon handoff. Native sanitized shell env/trust/credential ceilings,
bounded VT/input/output/process ownership, reaping and no unknown-command replay
remain. No model-visible terminal tool, hidden startup process, Node/Bun/WASM host
or donor inherited-provider/runner env. Shell, child jobs and PTYs are different owners.

`webfetch`: read-only GET, http/https, text/HTML/JSON response; T50 adds requested text/markdown/html (default markdown), timeout seconds default30/max120 and truthful original/final URL/status/content-type/format. One total budget includes conversion; bounded download/redirect/output and Unicode conversion remain. Не browser automation и не OpenProxy private admin fetch. Private/link-local/loopback targets запрещены по умолчанию; DNS resolution, фактический dial и каждый redirect проверяются вместе. Explicit trusted endpoint exception для provider/MCP НЕ распространяется на модельный webfetch. Credential headers не наследуются; proxy env не должен обходить egress policy. Для test fixtures использовать отдельный explicit loopback allowlist. No arbitrary methods/upload/cookies.

`glob`: bounded glob pattern, stable sorted pagination. T50 adds donor path/hidden
(default false)/limit and matching/ignore behavior; current memoized matcher is not
evidence of full donor semantics. Pattern ≤4096 bytes,
не более 64 segments; memoized `**` matcher. Walk budget 10,000 entries считается
при enumeration, до накопления результата. Nonregular nodes не читаются.

`grep`: current literal-only implementation is a historical subset, superseded by
T50's pending regex-default/literal/path/include/caseSensitive/limit contract. Use a
vetted pinned donor-compatible engine, not a custom approximation. Stable coherent
pagination and explicit malformed/budget outcomes remain. Pattern ≤4096 bytes,
per-file read ≤1 MiB, aggregate scan ≤16 MiB,
hit text ≤2 KiB на UTF-8 boundary. Files открываются no-follow/nonblocking и после
fstat читаются только regular; FIFO/device не блокируют runtime. Budget exhaustion
видим, не подменяется silent partial success.

`compress` — ordinary function tool из DCP.md; файл на диске не меняет. Schema
точно `{topic,content:[{startId,endId,summary}]}`. Он использует тот же permission
и durable dispatcher, что остальные built-ins; model call atomically сохраняет
projection/outcome и следующий Responses round получает новую projection. Direct
host helper не является отдельным слабым commit path: он использует тот же planner
и atomic storage boundary. Disabled/manual/deny убирают model-visible schema.

`skill`: input `{id}` выбирает skill только из pinned generation текущего turn. Model-visible descriptor/catalog содержит bounded id/name/description, но не body. Executor проверяет stale generation и central/agent-narrowed permission, записывает durable intent/outcome и возвращает immutable bounded snapshot body с digest и redacted provenance. Unknown/removed/oversized/unreadable skill — visible failure. Tool не перечитывает filesystem, не регистрирует другие tools/MCP, не запускает scripts и не меняет permissions/agent/model.

T50 read target adds paged directory entries and actual image tool results to text
line reads (1-based offset/limit, donor default2000). Images use validated provider
media lowering/accounting; unsupported model/PDF/invalid/oversized media is not
path-only success. Keep one successful-read nested AGENTS owner with T45/R10.

`question` is an application-owned form, not permission approval: nonempty typed
questions/options/multiple/free-form, cancellable wait, real ordered answers, bounded
operation/session/generation identities. No-consumer headless fails explicitly;
--auto never supplies answers. Dismissal interrupts, stale replies never authorize.

`opencode_models` searches the existing catalog without selecting a model;
`opencode_session_rename` persists an authorized current/explicit session title;
`opencode_session_move` admits a trusted destination for the same ID and applies it
at a safe boundary. No Code Mode, second registry/store or child authority over
arbitrary parent/sibling sessions. Original execution/jobs retain source context;
move never resets history, grants, unknown-effect quarantine or cleanup obligations.

## T38 historical qualification — supervision и egress

This records the argv/deadline subset, not T50 command/background qualification.
No-execution-timeout jobs still require bounded teardown/output and explicit outcomes.

`bash` supervisor: единый deadline начинается **до** spawn и покрывает spawn,
запись stdin, исполнение и ограниченное окно teardown. stdin пишет отдельный
поток, stdout/stderr дренируются конкурентно в bounded state с флагом truncation;
после cap чтение продолжается без retention, поэтому flood не блокирует pipe.
Завершение leader не считается завершением группы: drains получают ограниченное
окно, затем owned session group (setsid, pgid == pid) получает TERM → grace →
KILL, а reader-потоки никогда не join'ятся бесконечно — возвращается частичный
вывод. Cancel проверяется на всех ожиданиях. Child env — строгий allowlist имён
(`PATH`, `HOME`, `TMPDIR`, `TERM`, `USER`, `LOGNAME`, `SHELL`, `CARGO_HOME`,
`RUSTUP_HOME`, `XDG_*`, `LANG`/`LC_*`), а не substring-фильтр: всё прочее, включая
безобидные имена, отбрасывается; `PATH`/`LANG` имеют рабочие defaults, поэтому
обычные `cargo`/`rustc` команды резолвятся без абсолютных подсказок.
`cwd` проверяется лексически и канонически: symlink внутри root, ведущий наружу,
даёт `BadCwd`. Ошибка exec теперь сообщает OS error kind, а не bare `reap failed`.

`webfetch`: URL разбирается и joins через `reqwest::Url` (relative,
protocol-relative, query-only, fragment-only, IPv6, query сохраняется; userinfo и
не-http(s) отвергаются, fragment снимается). Egress проверяется на dial-time
через `reqwest::dns::Resolve` guard: весь ответ отвергается, если любой адрес не
public (loopback — только explicit test flag), поэтому DNS-переброс между
pre-check и connect не может отправить запрос к private endpoint; redirect
повторно валидируется и снова проходит guard, bearer идёт только на первый hop.
Один total budget покрывает DNS, все hops и body. HTML→text сканирует только по
ASCII-границам (`<`, `>`, `;`), поэтому UTF-8 сохраняется точно и не может
паниковать; named/numeric entities декодируются, malformed/unclosed разметка
переносится без паники.

## T40 qualification — bounded active context и retention

Активная projection строится по ссылкам/страницам: `read_history_full`
остаётся только у явного owner-действия (manual `/dcp-compress` над видимым
транскриптом), а per-turn путь читает `active_history` (prune-bounded,
covered rows пропускаются) и `wire_logs_for_window` (turn logs до anchor
floor). Независимый `ACTIVE_CONTEXT_BYTES_CAP` ограничивает память сборки и
не подменяет token admission: превышение даёт `ContextOverflow` с точными
bytes/cap и советом compress/prune, а не потерю фактов. Эвристика оценки
токенов (bytes/4) документирована как оценка, а не как точный счётчик
конкретной proxy-модели.

Tool output: UI-строка операции отдаёт bounded preview с маркером `…[+N]` и
точным `output_bytes`; полный result хранится один раз в
`tool_operations.output` и читается продолжением
`read_tool_op_output(op, offset, limit)` (байтовые окна по UTF-8-границам,
`next_offset`). Модель по-прежнему получает полный result в turn log —
preview никогда не подменяет доступные модели данные. Ввод TUI ограничен тем
же лимитом, что и core, а превышение видно в note.

## Permissions caveat

Выбранное совместимое файловое семейство не является sandbox: shell технически может писать через cat/python/компилятор, а MCP может иметь свои side effects. Инструкция предпочитать выбранные file tools для source edits — behavioral contract, не OS isolation. Runtime и агент не должны заявлять обратное. Build artifacts нормально создаются dev tools.

## MCP transport

Использовать официальный Rust SDK rmcp; выбрать/pin version/features, которые реально проходят протокол нужного server. Наличие SDK dependency не означает готовый adapter. Не писать собственный MCP stack ради неподтверждённого недостатка; сначала bounded compatibility spike.

`codex_web`: exact URL `{env:LUDKA2_API_URL}/mcp`, bearer из headers, enabled:true, oauth:false, timeout 60000 ms. Pinned OpenProxy route `/v1/mcp` принимает POST и JSON responses, требует negotiation/header version `2025-11-25`, предоставляет tool `search`; этот конкретный path проверен в [P3]. BaseURL обычно должен уже содержать `/v1`. Не чинить URL probing/redirects автоматически.

HTTP client умеет JSON и SSE ответы там, где transport их предлагает. У OpenProxy stateless JSON mode отсутствие session ID или GET event stream не должно порождать reconnect loop. Если SDK optional GET получает 405 — не считать это отказом исправного POST path; соответствие спецификации и поведение SDK проверить. Отправлять нужный `MCP-Protocol-Version` после initialize; no OAuth auto-discovery при oauth:false. Remote service timeout может быть меньше нашего клиентского 60s — сохранять исходный error, не ложно ждать/повторять операцию.

Local `chrome-devtools`: argv точно из admitted config, без shell splitting/rewrite. `enabled:false` (или canonical disabled:true) означает: не launch npx, не probe browser, не требовать Node, не делать install, включая environment/cwd/timeout. При true — stdio JSON-RPC, stdout только protocol, stderr в ограниченный redacted log, process group lifecycle. T46/R6 pending target заменяет прежний минимальный MCP env на inherited product-process environment + configured overlay после command/resource/credential-domain admission; обычный shell остаётся credential-free/minimal. Нужны runtime и уже доступный browser-url; `oc` не запускает Chrome с произвольным профилем пользователя.

`chrome-devtools-mcp@latest` — явная пользовательская команда; её не подменять pinned silently. Core adapter acceptance — pinned fake stdio server. Optional real-browser smoke записывает фактически разрешённую версию/digest в evidence; это не воспроизводимость `@latest` навсегда.

Approved config/startup target — [T46 R6/R7](goals/2026-09-22-mcp-attach-parity.md):
legacy mcp.<name> и canonical mcp.servers/mcp.timeout через одну нормализацию;
actual cwd/environment/stage deadlines, visible failed config/capability entries,
async initial connections без ожидания всех серверов/первого turn. Invalid optional
server не отменяет TUI или healthy siblings. No tools до initialize/catalog success;
only safe-boundary publication, существующие clients/owner и responsive typed state.
Полная field/capability matrix в T46, не предполагать поддержку OAuth/CodeMode/нового
protocol или file-reference expansion. MCP09/MCP10 добавляют доказательство нового
поведения; прежний MCP04/T37 PASS не квалифицирует inheritance/async startup.

## Catalog и вызовы

После initialize/capabilities — bounded paginated tools/list; registry отображает server/tool identity в stable wire-compatible function name с collision checks. Exact original имя сохраняется для tools/call. Никакого исполнения MCP description как инструкций. Не truncate каталог так, чтобы часть необходимого tools исчезла без diagnostics; oversized catalog явно LimitedCatalog/UnsupportedCapability.

Каждый call, включая native `skill`/`compress`: schema validation, generation lookup, общий permission pipeline, call timeout/cancel, structured result/error, bounded content/blob, durable outcome. annotations не заменяют permission policy. Protocol/network error и MCP `isError:true` различаются. Незнакомая output modality не превращается в plain success text. Повтор side-effect tool после network error запрещён без нового явного решения.

MCP клиент живёт в Location/config generation и не разделяется между разными auth/cwd. Reload между turns закрывает старый client; disabled entries не держат processes/requests. List-changed events инвалидируют только соответствующий catalog с controlled refresh. No session-global unbounded map of old generations.

## Generation ownership (T37)

Один `Runtime` держит ровно один connected MCP generation: второй turn той же
config generation переиспользует child/handshake/catalog, а reload, disable или
shutdown закрывают старую generation до публикации новой. Partial attach,
cancelled attach и отменённый turn освобождают single-flight lease через RAII,
поэтому поздний вызов не зависает в `TurnActive`. Cleanup выполняется в owned
task, поэтому drop caller future не отменяет закрытие. Ошибка закрытия/reap
возвращается наружу: `WorkerGuard::join` сообщает её, и actual binary завершается
ненулевым кодом вместо заявления clean shutdown.

Owned stdio child лидирует отдельную process group (`setpgid` до exec). Historical
T37 implementation uses project cwd/minimal env; approved T46/R6 target resolves
configured cwd от effective Location workspace (absolute only if admitted) и
inherits admitted product-process env plus overlay, including executable PATH.
Source/command/resource/credential-domain admission precedes inheritance; lower-trust
command cannot capture higher-trust credentials. No external runner-auth extraction;
configured env values/inherited secrets join redaction, no env dumps. TERM→grace→KILL адресуется только собственной group, а
не runner. SIGKILL fallback остаётся armed, пока wait не подтвердил reap.
Закрытие generation ограничено общим бюджетом 10 s; превышение — честный
`mcp shutdown failed`, а не бесконечное ожидание. Число enabled MCP servers
ограничено (`MAX_MCP_SERVERS = 8`) до первого spawn; aggregate catalog
(128 tools / 1 MiB metadata) и per-server 64 tools / 32 KiB schema отвергаются
целиком, без частичной публикации. Model-visible wire name ограничен 64 bytes:
небезопасные/длинные identity кодируются provider-safe именем с hash-suffix, а
exact server/tool сохраняется в dispatch map (никакого `split_once("__")`).

Вызов `tools/call` удерживает rmcp `RequestHandle`: Esc/deadline отправляют
`notifications/cancelled` с исходным request id под ограниченным deadline. Это
сигнал намерения, а не подтверждение отката side effect. Если ответ не получен,
durable operation получает `unknown`, turn — `failed`, дальнейший batch/round не
выполняется, подключённая generation закрывается. Local stdio можно явно
повторить только после успешного kill/reap всей owned process group и нового
handshake; при ошибке cleanup runtime отказывает дальнейшему подключению и
shutdown завершается с ошибкой. Для remote закрытый HTTP/request и HTTP 202 на
уведомление не доказывают остановку сервера: текущий owner блокирует повтор с
`unsafe_retry`; после restart прежний `unknown` требует внешнего выяснения,
автоматического replay нет. Случайный drop future во время вызова также помечает
generation poisoned и атомарно сохраняет unknown operation/turn; reload и
shutdown сначала переносят remote quarantine в runtime, затем закрывают старую
generation. Повтор в том же runtime после них остаётся `unsafe_retry`.

При смене Location приложение переносит sticky remote quarantine из закрытого
Runtime в новый до публикации target. Любой включённый remote MCP в этом
application owner получает безопасный `unsafe_retry` **до** вызова модели:
без надёжной идентичности remote side effect нельзя гарантировать, что другой
URL/Location независим. Это консервативно для других remote endpoints; локальная
Location только со stdio MCP продолжает работу после успешного закрытия прежнего
owner. Ошибка cleanup по-прежнему прерывает switch/shutdown, а не сбрасывается
созданием нового Runtime. Перезапуск процесса не доказывает исход старого
remote вызова и не выполняет автоматическую сверку.

Структурно неподдерживаемый или пустой/некорректный ответ после `tools/call`
также означает `unknown`: сервер мог уже выполнить side effect. Он проходит тот
же путь retirement и remote quarantine (либо stdio kill/reap перед явным
повтором). Валидное `isError:true` — окончательный ответ сервера: остаётся
`failed`, без ложного `unknown` и без принудительного закрытия generation.

`tools/list_changed` claim-ится атомарно перед relist; перечитывается только
изменившийся server, а notification, пришедшая во время relist, остаётся pending
для следующего turn. Failed relist восстанавливает claim и сохраняет прежний
catalog. Пропущенный или конфликтующий `Authorization` даёт явную ошибку до
network: headers нормализуются в typed `HeaderMap` case-insensitively, а
duplicate с разными значениями — terminal config error конкретного server (T46/R6 target:
failed entry, не app-wide optional-service failure). Search вызывает
server schema `query` + `response_length`, не helper `limit`.

## Direct exposure vs Code Mode

В `oc-rs.toml` профиль явно `tool_exposure = "direct"`. При отсутствии upstream codemode field выбранный профиль означает direct — это объявленное отличие. Explicit true = actionable unsupported error. Не запускать JS interpreter, Node eval, Code Mode shim или remote execute to emulate missing interpreter.

The owner explicitly keeps this exclusion under T50. Native opencode_* tools are
ordinary direct definitions, not a shim for tools.opencode via execute. Built-in
websearch and its auth/provider selection are absent; configured MCP tools keep
their real identities/schemas and can search without inventing a built-in alias.

## T42 qualification — config source trust

`opencode.json`/`opencode.jsonc` sources are admitted per root (global config dir, Location
root, `.opencode`) only when the canonical file stays inside that root's canonical path; a
symlinked config resolving outside is refused with the resolved path in the diagnostic, so a
source can never be canonicalised outside and then marked trusted. The `.opencode` definition
root must stay inside the Location root, and `AGENTS.md` files must stay inside their own
root. In-root symlinks are still admitted (containment policy, not a blanket symlink ban), and
`{file:...}` substitution keeps its no-follow, relative-only, 64 KiB-bounded reader rooted at
the admitted source directory. The Location switch itself re-runs this admission for the
target root, so a switch cannot smuggle in an outside source.
