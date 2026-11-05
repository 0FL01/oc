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

## Built-in profiles / MCP defaults — approved 2026-10-01, pending

[T45/R3/R6/R8/R10](goals/2026-09-21-config-compat-and-subagents.md#built-in-profiles--default-mcp-access--approved-2026-10-01-pending)
registers Build/Plan primary and General/Explore subagent without custom definitions.
Default MCP authority becomes Allow for **all actual registered tools** of configured/
enabled/admitted/connected servers, including root Plan and both children. Shared
central baseline precedes child narrowing; no web-search/server allowlist, global
native-tool wildcard Allow or ineffective child-only grant. Explicit parent/central/
profile Deny/Ask/resource rules and quarantine/trust/caps remain. Schema visibility,
delegation capability preview, permitted initialize instructions and invocation share
the exact collision-safe wire identity and compatible permission alias.

Denied tools are removed from new catalogs; Ask stays conditional and requires the
real consumer. Disabled/failed/pending servers give no new capability or automatic
enable/connect. Safe next-request catalogs and captured prepared leases use existing
T46 ownership, no second MCP registry. Default Explore MCP access is a deliberate
native difference from donor blanket Deny; arbitrary MCP tools are not classified
read-only by names/annotations. Native Explore filesystem/shell/question/delegation
limits and own-history compress gates remain. Plan native mutation policy/reminders
and General question/nested-delegation/session-control ceilings remain too; neither
Plan nor Explore is a sandbox of external MCP effects.

CTX02/PRM01 qualify actual no-custom-profile/default-MCP requests/effects and Plan
lifecycle; T50 keeps executors/selector and T46 lifecycle. This replaces only old
default MCP Ask/missing registered-action authority, not explicit user restrictions.
Approval/document validation is not runtime PASS; examples/config sources stay untouched.

## Built-ins

Tool arguments remain bounded at 4 MiB per call. Provider SSE lines/events have
an independent 8 MiB bound; the serialized provider request remains 32 MiB.
These are not the separate 64 MiB MCP transport-message budget. Argument overflow is a failed call with
its original identity, not an executable partial JSON prefix or a failed turn:
the parsers drain/reconcile the response, persist that refusal and run other
admitted calls in the batch once. Independent SSE/generation/protocol caps remain.
Invalid/non-object JSON is likewise a settled failed tool result, including when
it is the only call: the next request receives its original call identity. Only a
genuinely terminated generation can settle this input failure. Unfinished calls
on `finish=length` fail with that reason, even if a Chat prefix is valid JSON;
the prefix never executes. Tool-driven length recovery permits at most two
consecutive length continuations; a completed stop or accepted user input resets
the streak. A stream missing its genuine terminal remains an error, not length.
Unknown effects still stop with `tool outcome unknown; retry may duplicate side effects`;
the generic wording does not authorize replay for shell, execute or MCP.

`read`: потоковое чтение выбранного диапазона строк с path/offset/limit, без whole-file size admission для текста. Сохраняются ≤2000 lines, 65536-byte rendered page, bounded huge-line prefix, UTF-8/CRLF и явный next cursor; binary/NUL проверяется только в consumed prefix, включая пропущенные до offset строки. Artifact содержит выбранную страницу, не весь файл. Image media-ingest ≤20 MiB, отдельно от неизменных text/search bounds; PDF unsupported. Явные пути read/edit/write/apply_patch следуют symlinks; permissions/external_directory — по lexical path, own-data-root exclusion — по разрешённой цели I/O. Recursive search не следует найденным ссылкам. Точный registered artifact сохраняет отдельный authorized read/grep route, не общий доступ к native directory/blob/mutations. Complete search pages stable sorted/paginated; budget-limited subsets честно truncated, без обещания глобального sorted prefix/cursor.

`apply_patch`: один JSON argument `patchText` с upstream-style `*** Begin Patch` / Add File / Update File / Delete File / Move to / End Patch. Не смешивать с provider-hosted Responses `apply_patch` schema. В M2 сохранить parser fixtures выбранного OpenCode baseline; политика unsafe paths — наша.

Configured `formatter` может продолжить разрешённую write/edit/patch запись; output/effects отражают guarded фактические bytes после форматирования. Это не отдельный Shell ToolCall и не permission grant; trusted host-effect caveat и unknown/no-replay lifecycle — [SECURITY](SECURITY.md), конфиг — [CONFIG](CONFIG.md#configured-formatters).

Native strict profile: Add body состоит только из `+`-строк (префикс снимается);
empty Add создаёт zero-byte файл. `*** Move to:` идёт непосредственно после
`*** Update File:`, до hunks. `@@ section` выбирает точный уникальный context,
`*** End of File` привязывает последний hunk к концу. Неоднозначный preimage,
повторные/пересекающиеся normalized paths и пустые update hunks — conflict/error.
Нет fuzzy matching, shell/heredoc wrappers и aliases `patch`/`text`. W2 TS semantics:
BOM снимается перед matching и восстанавливается один раз; строки вне hunk сохраняют
исходный EOL, весь replacement (включая context) берётся из parsed patch с LF.
Непустой update заканчивается LF, без dominant-EOL политики. Выбранные source-derived fixtures:
`fixtures/patch.json`, pinned O1; MIT notice: `fixtures/OpenCode-MIT.txt`.

Обязательные случаи: новый текстовый файл; новый пустой файл; добавление текста в существующий пустой файл через Update; точечные hunks; полная замена текстового содержимого через patch; delete; move/update без overwrite чужого target. Add existing path — conflict, не silent overwrite. CRLF/Unicode/last newline проверены; binary patch вне scope с точной диагностикой.

Парсер сначала строит plan всех операций, validates grammar/sizes/lexical paths/conflicts и checks permissions. Затем per-file preimage check и запись temp file в директории фактической цели I/O, fsync, atomic rename; preserve mode в рамках разрешений. Write/update через leaf symlink сохраняет ссылку и меняет referent; delete/move-source действует на lexical entry. Preview и commit связаны с реальным target и identity ссылки. Не обещать all-files atomicity. Ошибка после части commits → partial result с точными paths/hashes; crash ambiguity → unknown. Pinned directory-relative no-follow I/O применяется после разрешения явных ссылок, не как blanket symlink ban. Это не kernel CAS и не полная защита от malicious same-UID TOCTOU.

Весь preflight вычисляет before/after и проверяет каждый hunk до первого изменения;
лимиты: patch 4 MiB, файл 8 MiB, сохранённые preimages/results плана 64 MiB.
Linux implementation после разрешения explicit links удерживает directory handles, проходит фактические parents через
`openat(O_DIRECTORY|O_NOFOLLOW)` и использует только single-component имена в
mutation syscalls. Temporary inode — `O_CREAT|O_EXCL|O_NOFOLLOW`, новое случайное
имя; final Add/Move — `renameat2(RENAME_NOREPLACE)`. Update/Delete повторно
сверяют identity/metadata/bytes перед commit; это не kernel compare-and-swap и
не защита от враждебного same-UID writer в последнем syscall window. Права нового
файла ограничены 0600/umask, update сохраняет ordinary mode bits. Перед rename
синхронизируется файл после chmod, после namespace changes — затронутые directories,
включая созданные parents. Move, в том числе cross-device, пишет prepared bytes в
destination до удаления source; source не обновляется предварительно. Если removal
падает, outcome содержит destination Create с полными hashes и прежний source;
после успешного removal учитывается Move, даже при следующем sync failure. Rollback не обещается. Crash может
оставить staging file, но не вызывает автоматический replay.

Для удаления/rename сохранять достаточную operation metadata в own storage; не превращать это в snapshot/undo subsystem. Не запускать `git reset`, не делать auto-rollback на пользовательские файлы. Модель получает concise diff/status; полный diff в bounded blob.

`write` (T50/R9 pending): `{path,content}` создаёт/перезаписывает файл, включая пустой
content и missing parents; donor BOM/newline semantics, bounded regular-file preimage
и реальные target/resource/existed/operation metadata. Не эмуляция Add File patch.
Старый preimage `write` может быть NUL/non-UTF-8 или больше8MiB: он не разбирается
как текст. Полные before bytes сохраняются и буквально перепроверяются перед commit;
before+after ограничены существующим64MiB plan budget. Новый content остаётся
валидным NUL-free UTF-8 ≤8MiB, старый BOM сохраняется. `edit`/strict patch сохраняют
прежние8MiB snapshot/text guards; digest/link/inode/mode/cleanup не ослаблены.

`edit` (T50/R9 pending): `{path,oldString,newString,replaceAll?}`; существующий файл,
unique match по умолчанию, все matches при true, фактический replacements count.
Donor precedence: exact nonoverlapping → typography-normalized → trailing-whitespace
line matching; CRLF/BOM сохраняются. Empty/identical old, no match и ambiguity —
error; empty new удаляет совпадение. Это отдельная edit семантика, не fuzzy patch.

Все три mutation tools используют общий canonical apply_patch permission/grants
owner, actual resources, before/after approval preview и approved-preimage recheck
после ожидания, включая замену symlink referent. Effective Deny/Plan/child/resolved-data-root/protected paths и caps
не обходятся выбором модели. Intent до effects, outcome/confirmed diffs после; crash
не разрешает replay. Нет prerequisite отдельного read call. T44 VIS35 показывает
Write/Edit/ApplyPatch отдельно; replay использует persisted input/results/effects,
не сегодняшние файлы. No Formatter/LSP subsystem в этом файловом срезе.

`shell` (T50 target): command/workdir/timeout/background through the actual configured Linux shell; default foreground timeout120000ms, explicit0 disables execution timeout, background default has no execution timeout. Preserve legacy `bash(argv/cwd/timeout_ms)` through one supervisor/policy owner with unambiguous schema normalization, not duplicate advertised tools or a Deny bypass. Process group, trusted cwd, captured owner env, concurrent drains, bounded preview/retained output and TERM→grace→KILL→wait remain. Background returns running/shellID after launch and later an automatic durable terminal notice, without polling; output/jobs/queues/teardown stay bounded even with no execution timeout. Native unknown effects never auto-replay; no hidden persistent terminal manager or sandbox claim.
Нет отдельного32KiB argv-entry cap и600000ms positive-timeout ceiling. Аргументы
по-прежнему проверяются на shape/NUL/arity; реальный OS `E2BIG` — failed tool result,
не success/unknown replay. Captured environment metadata сохраняет отдельный32KiB
selector-input bound; это не ограничение фактических shell argv.

Model `shell` source is analyzed by a native bash/zsh/posix scanner using the same
captured executable as execution. Every extracted command is an authorization
resource; quoted legacy `bash` argv remains one literal resource, not shell source.
Opaque input, including opaque nested syntax, refuses the whole invocation before
approval or spawn. Supported compounds, substitutions and heredocs are not blanket
opaque. A zero-command result still prepares/pins cwd and honors whole-tool Deny;
the execution preview retains the full original command, not the first resource.
Always confirmation displays the actual saved prefixes, not the narrower current
invocation; internal command/executable domain tags are not shown.
Static `cd`/`pushd` directories and lexical workdir use `external_directory` outside
Location/enclosing checkout. Approved external cwd is pinned for that invocation,
not added to read/discovery roots or made a new Location. Lexical and resolved
native-data-root exclusions remain; internal symlinks escaping Location still refuse.

Explicit user `!` input does not call model permission admission: Allow/Ask/Deny
do not gate it. It still pins cwd, commits history before effect, uses the shared
supervisor and never replays unknown effects. Configured command interpolation is
not this trusted user path and remains model-policy gated.

Shell commands (agent foreground/background, legacy `bash`, user `!`) and PTYs inherit
the full captured native-owner startup environment, including SSH-agent sockets,
proxy/CA/toolchain settings and credentials. No name allowlist or client-env refresh;
NUL-containing values are dropped and missing PATH/LANG receive working defaults.
Spawn installs that snapshot explicitly, without merging ambient process env. Output
is not secret-masked: printed credentials/env values can reach model/history/cards/artifacts.
Browser authorization launch keeps a separate credential-free allowlist; MCP and
webfetch retain their own environment/credential policies.

Owner amendment 2026-10-01 extends T50/R2/TOOL13 with authoritative source-session
running shell inventory/status, bounded live output/final flush, selected-job kill
and same-process foreground Ctrl+B conversion, which clears the execution timeout.
Immutable initial `running` result
is not live job status. Original session/Location/generation and durable notice
identity remain; repeated/terminal/cancel races never spawn again or duplicate results.
T44/VIS39 owns paired Shell rows/output viewer. This is command-shell lifecycle,
not an attached interactive PTY or a new model polling/terminal tool.

The old arbitrary-terminal-manager exclusion is narrowly superseded by approved
[T56/TERM01](goals/2026-10-01-native-session-terminals.md): explicit user-created
session-local native PTYs selected from Terminals and shown in the right pane.
They survive hide/show/session view changes within live `oc`, not application
shutdown/daemon handoff. Captured owner env, native trust/permission ceilings,
bounded VT/input/output/process ownership, reaping and no unknown-command replay
remain. No model-visible terminal tool, hidden startup process or Node/Bun/WASM
host. Shell, child jobs and PTYs are different owners.

`webfetch`: read-only GET, http/https, text/HTML/JSON response; requested text/markdown/html (default markdown), timeout seconds default30/max120 and truthful original/final URL/status/content-type/format. One total budget includes DNS, redirects, body and conversion. Defaults: 5 MiB downloaded/rendered-output bounds and ten redirects, with existing common preview/artifact limits. Private/link-local/loopback targets are permitted by captured permission policy, not a separate host/DNS guard or test flag. Initial Ask uses normal approval; before every redirect request Deny refuses its normalized target, while Ask does not prompt again. System HTTP proxy settings apply. Tool credentials are not inherited; no browser/JS/search, arbitrary methods/upload/cookies. Code Mode raw fetch retains its admitted method/body/header support and strips Authorization/Cookie on cross-origin redirects.

`glob`: ripgrep glob pattern, path/hidden (default false)/limit и ignore behavior.
Complete scans имеют stable sorted pagination. Pattern ≤4096 bytes, не более
64 segments. Walk budget 10,000 entries считается при enumeration; candidates
обрабатываются по ходу перечисления, а не отбрасываются вместе с широким каталогом.
Budget exhaustion возвращает completed + truncated, без выдуманного next cursor.
Recursive walk не следует symlinks; nonregular nodes не читаются.

`grep`: regex-default/literal/path/include/caseSensitive/limit через vetted
ripgrep-compatible engine, не custom approximation. Complete scans сохраняют
stable coherent pagination; malformed patterns остаются errors. Pattern ≤4096 bytes,
per-file read ≤1 MiB, aggregate scan ≤16 MiB,
hit text ≤2 KiB на UTF-8 boundary. Files открываются no-follow/nonblocking и после
fstat читаются только regular; FIFO/device не блокируют runtime. Budget exhaustion
видим, не подменяется полным покрытием. R10 exact-artifact route streams beyond
ordinary grep's 1 MiB file admission without relaxing ordinary project grep scans, aggregate/time/
hit budgets or no-follow/session/policy checks.

W2 amendment 2026-11-03 supersedes the former failed-on-budget outcome: resource-limited grep is `completed`
with `coverage:incomplete`, verified `matches`, typed budget facts and no
`next_offset`. Oversized content candidates are unsearched omissions, not absence;
mandatory ignore files cannot be omitted. Admitted binary content stops at the
first NUL-containing chunk and charges actual bytes. Cancellation discards all
partial matches and joins the worker. Complete pages retain stable pagination;
an incomplete result does not claim a global sorted prefix or exhaustive absence.

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

`question` is an application-owned form, not permission approval: a nonempty typed
question list, options/multiple/free-form, cancellable wait, real ordered answers, bounded
operation/session/generation identities. For a noninteractive Run origin,
NoConsumer is an ordinary failed tool result with the TS feedback:
`This non-interactive run cannot ask the user questions, so the question was cancelled. Continue without an answer; make reasonable assumptions and state them.`
The model may continue; --auto never supplies answers. Interactive dismissal still
interrupts, and stale replies never authorize.
TS string/schema semantics allow blank question/header/option labels, duplicate
labels and headers longer than the descriptive 30-character suggestion. Unknown
input fields are ignored; declared string/boolean/array types, finite payload/field/
count limits and reply/image bindings remain. Blank declared options are answerable.
A free-form answer may carry clipboard PNGs (`[Image N]` chips, prompt paste binding)
only when the issuing model accepts image input; owner re-validates label/caps/PNG and
the tool result lowers them to `input_image`, persisted as `native_question_results`.

Prompt/question attachments have a finite 20 MiB single /40 MiB total raw-byte
budget. Native local-image read and stored reconstruction use the same single
media bound and complete container/pixel/animation validation (decoded allocation
≤20 MiB, dimensions ≤4096). Cancellation and own-data-root refusal remain.
Read base64 is retained once in native facts, not charged again for the transient
wire projection or as text tokens. Admission measures the actual durable record,
including earlier results in the same batch; visual and nonimage text costs remain.
The independent 32 MiB retained-record and final request limits are unchanged.

RAW keeps every admitted prompt/read/question/MCP image. Before request byte admission,
capability/wire projection omits unsupported images; then image-only projection
replays the TS prefix budget: strictly above 25 MiB, oldest images become notes
until retained image size is at most 15 MiB. Ordinary inline media counts base64
characters, tool-result images their full URI length. Text/nonimage files and
call identity stay intact; bounded history reads load only retained image bodies,
without relaxing per-record or final request caps. Compaction transcripts never
quote base64. Removed images are no longer visible; retrieve/reattach rather than
claiming their contents from memory. A turn whose own unsupported images were dropped warns
"the selected model cannot see images; sent without them".

Other native input bounds remain finite: prompt2MiB (Core, TUI and native framing
agree), question input/reply128KiB with8192-byte question/description/answer and
512-byte option labels, clipboard/selection128KiB, protected DCP text128KiB,
instructions128KiB per source/512KiB total, fork8192 copied rows/32MiB copied bytes.
These budgets do not promise every maximum-sized combination fits one provider request.
`subagent` keeps a finite global16-job budget and16 child sessions per parent.
Transactional outstanding counts and real semaphore permits both remain enforced.
Recovery reads at most cap+1 identities to detect overflow, normal notice delivery
is bounded by the same16-job cap. Unknown jobs stay unresolved/non-replayable even
after leaving the outstanding count. The independent shell-job8 budget is unchanged.

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
вывод. Cancel проверяется на всех ожиданиях. Historical child env used a strict allowlist
(`PATH`, `HOME`, `TMPDIR`, `TERM`, `USER`, `LOGNAME`, `SHELL`, `CARGO_HOME`,
`RUSTUP_HOME`, `XDG_*`, `LANG`/`LC_*`), а не substring-фильтр: всё прочее, включая
безобидные имена, отбрасывается; `PATH`/`LANG` имеют рабочие defaults, поэтому
обычные `cargo`/`rustc` команды резолвятся без абсолютных подсказок. Этот старый env
контракт заменён полным captured owner env, описанным выше; supervision не изменён.
`cwd` проверяется лексически и канонически: symlink внутри root, ведущий наружу,
даёт `BadCwd`. Ошибка exec теперь сообщает OS error kind, а не bare `reap failed`.

`webfetch`: URL разбирается и joins через `reqwest::Url` (relative,
protocol-relative, query-only, fragment-only, IPv6, query сохраняется; userinfo и
не-http(s) отвергаются, fragment снимается). Redirect повторно валидируется и
проверяется на captured permission Deny до запроса; private/DNS-rebinding guard
не применяется. Allow/Ask на target не создаёт новый prompt. Compatibility API
передаёт явно заданный bearer только первому hop; обычный tool bearer не наследует.
Один total budget покрывает DNS, все hops и body. HTML→text сканирует только по
ASCII-границам (`<`, `>`, `;`), поэтому UTF-8 сохраняется точно и не может
паниковать; named/numeric entities декодируются, malformed/unclosed разметка
переносится без паники.

## T40 historical qualification — bounded active context и retention

Активная projection строится по ссылкам/страницам: `read_history_full`
остаётся только у явного owner-действия (manual `/dcp-compress` над видимым
транскриптом), а per-turn путь читает `active_history` (prune-bounded,
covered rows пропускаются) и `wire_logs_for_window` (turn logs до anchor
floor). Независимый `ACTIVE_CONTEXT_BYTES_CAP` ограничивает память сборки и
не подменяет token admission: превышение даёт `ContextOverflow` с точными
bytes/cap и советом compress/prune, а не потерю фактов. Эвристика оценки
токенов (bytes/4) документирована как оценка, а не как точный счётчик
конкретной proxy-модели.

Tool output в историческом T40 срезе: UI-строка операции отдаёт bounded preview с
маркером `…[+N]` и точным `output_bytes`; полный result доступен в
`tool_operations.output` и читается продолжением
`read_tool_op_output(op, offset, limit)` (байтовые окна по UTF-8-границам,
`next_offset`). Модель также получает полный result в turn log — это не доказательство
единственной full-copy во всём runtime и не файловый `read(path)` для модели.
Ввод TUI ограничен тем же лимитом, что и core, а превышение видно в note.
Ни исторический PASS, ни этот UI preview не квалифицирует общий tool-output limiter.

## Tool-output previews и filesystem continuation — T50/R10/TOOL21 (pending)

Confirmed mutation diffs (`apply_patch` / `edit` / `write`) are streamed from the
committed before/after images into the same registered tool-output artifact before
the independent `PatchEffects` preview caps. The result carries the recorded path
and capture state. In the shared confirmed-diff view, file headers show only the
operation label and recorded path, without a generated preview-truncation badge.
The TUI hides a Complete capture's reference while retaining existing non-Complete
and truncated Complete-without-reference warnings.
These warnings remain subject to the existing confirmed-effects and Write paths.
A recorded reference does not guarantee current readability. Partial failure captures
only committed changes, including the actual destination after a move. Small status
text is not a second full diff; SQLite/TurnLog keep only bounded metadata and the
reference. Capture cap/quota/IO loss is explicit and never reexecutes a mutation.
Existing historical truncated DTOs cannot recover omitted text from present-day
workspace files.

Owner-approved2026-10-01 [contract/RECON/ordered slices](goals/2026-09-27-native-tool-parity.md#tool-output--r10tool21-approved-2026-10-01-pending)
narrowly supersedes full-inline publication for **new oversized admitted text**,
discard-before-shell-capture and exact-artifact data-root read/search denial.
Small inline results and legacy raw records stay unchanged/immutable. The common
registered route and before-discard shell capture are native support; historical
T40 evidence remains factual.

- Common prepared-result boundary runs before provider input/new durable outcome/
  TurnLog/UI cloning for local built-ins, MCP text/structured-text projection, fetch
  and skill bodies. Configured2000-line/51200-byte defaults keep general head or shell
  tail; UTF-8, shared multipart text count and bounded reserved reference/notice within
  native64 KiB served-text envelope. No `metadata.truncated:true/false` bypass. Preserve
  typed control JSON/effects/error status and separately admitted media under its caps.
- One cold admitted text capture, verbatim within producer normalization/bounds, in `<native-data-root>/tool-output/` is
  registered by the existing Db owner with operation/session/source generation, actual
  readable extent and capture state. Hot result holds preview/ref/counts/guidance, not
  full text repeated across SQLite/TurnLog/resident history. Stream UI/model reads;
  keep legacy SQLite APIs compatible, no whole-artifact `read_blob`/second archive.
  Registered `read`/`grep` pages reuse that admitted text. No producer or reader-
  generation secret dictionary masks tool output, labels, Write/instruction receipts,
  question fields or Code Mode code/input/output/log previews.
- Cards reads the existing owner continuation in 64 KiB UTF-8 pages, including the
  shell-first and legacy SQLite paths. `next_offset` continues to the end of the
  stored result; 2048-byte presentation facts remain a local preview, not its
  accessibility limit. Paging checks session/operation ownership before reading.
- Shell drains tee admitted text before old1 MiB-per-stream discard and keep bounded
  recent/tail RAM. Foreground/background/live viewer/final notice/same-process Ctrl+B
  share one capture identity/final flush. Slow/failing disk, quota/cap or endless flood
  never require unbounded queues; continue drain/cancel/reap with explicit capture loss.
  Process exit/effects and log capture state are separate, no auto-rerun to repair a log.
  Recent stdout/stderr and the combined tail each retain at most64 KiB. Incremental
  UTF-8 normalization and per-stream incomplete-character carry precede disk/hot publication. The writer uses
  synchronous backpressure with no pending output queue. Artifact boundary labels
  record serialized normalized publication; per-stream UTF-8 carry can defer
  stdout past already publishable stderr. Fixed first/last Capture-ingress sequences
  and carry-release counters retain bounded raw-read provenance (not a complete
  per-byte arrival trace or total OS emission order). Terminal
  outcomes/background notices retain its same small typed descriptor. A live viewer
  pages only durable published extents; terminal preparation never recaptures the tail.
- Complete means all admitted text was captured, not full HTTP body rejected at input,
  media/base64 or discarded producer bytes. Tool text may contain unredacted secrets
  by the explicit TS-parity contract below. Native artifact cap32 MiB,
  shared existing quota default2 GiB and completed TTL7 days are explicit differences
  from donor full-file retention without a byte quota. Preview-truncated can still be capture-complete;
  producer/cap/quota/IO/cancel loss is incomplete with actual readable prefix or no path.
  Failed durable storage/cleanup remains non-success, never unlimited inline fallback.
- Exact registered artifact can be accessed by model `read(path,offset,limit)` or
  `grep` under session/already authorized owned-lineage/provenance/effective Deny,
  trust/no-follow/identity/regular-file checks. Artifact grep bypasses ordinary grep's 1 MiB cap; text reads also stream
  bounded page/scan/time/hit/model budgets and honest next cursors. No directory/glob,
  arbitrary blob/SQLite/WAL/config access or widening mutation authority; guessed path
  and quoted/forked child references grant nothing. No AGENTS discovery from log paths.
- Existing owner protects active writer/reader leases, publishes readable extents
  crash-safely and cleans completed captures after7 days. Expired/missing is explicit
  on read; immutable outcome/ref/history and ordinary referenced blobs are not deleted.
  Restart/model-switch/move/DCP/compact retain current bounded refs, not automatic
  full-artifact/archive loading or producer replay.

TOOL21 actual rebuilt debug/release large-output → next-request preview/path → real
read/grep → distant fact → useful continuation is mandatory. TOOL13/TOOL16 own their
existing lifecycle/media cases; AUD34/LOAD02/STORE04/A05/A10 retain ownership and provide
regressions/resource/crash receipts. Existing T44 presentation qualification stays
separate and PAUSED; no new task/store/framework/paid campaign or historical PASS rewrite.

## Native failure diagnostics

New native file/search, skill, fetch and subagent failures use a bounded JSON
diagnostic: `{status,error:{tool,code,message,detail?,hint?}}`. Admission control
fields such as permission `status`/`feedback` remain intact; process/media/partial
mutation results keep their existing richer owners. Execution state is trusted
metadata, never inferred from output text or a JSON-looking skill body. DCP error
purging likewise uses the exact durable tool occurrence, not an `error:` prefix.
No secret-dictionary masking precedes clipping; each clipped match/message retains its own fields.

Failed/cancelled subagent results retain an allocated `sessionID` when available.
Known stop sources are recorded before signalling and frozen at terminal selection;
generic parent/question cancellation stays explicitly unknown rather than becoming
"user cancelled". Accepted-turn diagnosis is committed in the exact linked terminal
TurnLog and reused for new notices/recovery. Old/missing provenance is not invented.
Committed outcome and cleanup health remain separate: failed reap/shutdown is fatal,
and neither an infrastructure error nor an incomplete search authorizes replay.

## Permissions caveat

Выбранное совместимое файловое семейство не является sandbox: shell технически может писать через cat/python/компилятор, а MCP может иметь свои side effects. Инструкция предпочитать выбранные file tools для source edits — behavioral contract, не OS isolation. Runtime и агент не должны заявлять обратное. Build artifacts нормально создаются dev tools.

## MCP transport

Использовать официальный Rust SDK rmcp; выбрать/pin version/features, которые реально проходят протокол нужного server. Наличие SDK dependency не означает готовый adapter. Не писать собственный MCP stack ради неподтверждённого недостатка; сначала bounded compatibility spike.

`codex_web`: exact URL `{env:LUDKA2_API_URL}/mcp`, bearer из headers, enabled:true, oauth:false, timeout 60000 ms. Pinned OpenProxy route `/v1/mcp` принимает POST и JSON responses, требует negotiation/header version `2025-11-25`, предоставляет tool `search`; этот конкретный path проверен в [P3]. BaseURL обычно должен уже содержать `/v1`. Не чинить URL probing/redirects автоматически.

HTTP client умеет JSON и SSE ответы там, где transport их предлагает. У OpenProxy stateless JSON mode отсутствие session ID или GET event stream не должно порождать reconnect loop. Если SDK optional GET получает 405 — не считать это отказом исправного POST path; соответствие спецификации и поведение SDK проверить. Отправлять нужный `MCP-Protocol-Version` после initialize; no OAuth auto-discovery при oauth:false. Remote service timeout может быть меньше нашего клиентского 60s — сохранять исходный error, не ложно ждать/повторять операцию.

Remote MCP preserves query in the exact URL and has no private-host/DNS prelookup
refusal; other URL/header validation, bounded protocol/lifecycle diagnostics and
cleanup remain. It shares the system HTTP proxy policy with webfetch and provider
HTTP/discovery (HTTP(S)_PROXY/ALL_PROXY/NO_PROXY, uppercase/lowercase), not provider
endpoint authority. Provider direct DNS/pinning/peer guards remain separate; proxied
origin hostnames are resolved by the proxy, with captured URL/literal-IP admission
still enforced. WebSocket proxy routing and TLS rules are in [CONFIG](CONFIG.md).

Encoded MCP message и SSE event ограничены64 MiB, независимо от webfetch budget.
Stdio проверяется до накопления строки SDK и до отправки oversized frame; HTTP JSON
читается ограниченными chunks, raw SSE event — до parser allocation. Декодирование
MCP остаётся у rmcp, SSE — у его locked `sse-stream0.2.4`. Превышение сообщает
observed bytes/limit и не закрывает healthy siblings. Неизвестный effect вызова
по-прежнему quarantined, а успешный cleanup требует подтверждения владельца.
Startup/catalog defaults —60000/120000ms с существующими overrides; отдельного
нового короткого execution deadline нет.

Local `chrome-devtools`: argv точно из admitted config, без shell splitting/rewrite. Canonical `disabled:true` означает: не launch npx, не probe browser, не требовать Node, не делать install, включая environment/cwd/timeout; legacy `enabled` теперь warning+ignore, не способ отключить сервер. При enabled effective entry — stdio JSON-RPC, stdout только protocol, stderr в ограниченный redacted log, process group lifecycle. Local project/global MCP наследуют полный captured parent env + configured overlay после source/cwd/resource admission; credential-domain refusal/filter удалены (W5), shell отдельно наследует captured owner env. Нужны runtime и уже доступный browser-url; `oc2` не запускает Chrome с произвольным профилем пользователя.

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
Source/cwd/resource admission precedes inheritance. W5 removes credential-domain
refusal and filtering: project and global entries receive the full captured parent
map, with configured overlay winning, including credential values. No fresh env read
at spawn/reload and no external runner-auth extraction;
no env dumps. MCP results/instructions/lookups/media are verbatim, including server
echoes of credentials; stderr/status labels keep their existing protection.
TERM→grace→KILL адресуется только собственной group, а
не runner. SIGKILL fallback остаётся armed, пока wait не подтвердил reap.
Закрытие generation ограничено общим бюджетом 10 s; превышение — честный
`mcp shutdown failed`, а не бесконечное ожидание. Отдельного допуска по числу
configured/connected servers, tools или страниц tools/list нет. Pagination идёт
до конца под общей отменой/catalog deadline. Сериализованные метаданные каталогов
ограничены64 MiB на сервер и256 MiB на Location; это не RSS ceiling. У schema и
description нет отдельных32 KiB/8 KiB порогов. Превышение byte budget отклоняет
кандидат целиком с actual/limit, не закрывая исправных соседей; relist сохраняет
предыдущий полный каталог. Model-visible wire name ограничен64 bytes:
небезопасные/длинные identity кодируются provider-safe именем с hash-suffix, а
exact server/tool сохраняется в dispatch map (никакого `split_once("__")`).

Вызов `tools/call` удерживает rmcp `RequestHandle`: Esc/deadline отправляют
`notifications/cancelled` с исходным request id под ограниченным deadline. Это
сигнал намерения, а не подтверждение отката side effect. Execution deadline возвращает
обычный `failed` (`timed out`), pending request удаляется, late reply игнорируется,
а тот же server остаётся доступен следующему вызову (TS parity). Только Transport,
MessageLimited, CleanupFailed и Cancelled означают unknown effect:
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

Структурно неподдерживаемый, пустой/некорректный или oversized ответ после
`tools/call` возвращает обычный `failed`, без retirement/quarantine. Text-only
result ограничен16 MiB (lookup использует тот же лимит); media-bearing result
использует тот же16MiB encoded-JSON aggregate, без отдельного64-part ceiling.
MIME/base64/SDK shape и ordered durable reconstruction остаются обязательными.
Превышение сообщает `MCP result N bytes exceeds L bytes`.
Большой допустимый текст публикуется существующим preview + `read(path)` artifact,
без redaction ни в preview, ни в cold capture. `isError:true` остаётся `failed`,
joined text parts видны модели verbatim; numeric JSON-RPC errors сохраняют только
fixed category, без server error message. Эти failures не обрывают turn и не
запускают automatic retry; следующий явный вызов использует тот же server.

`tools/list_changed` claim-ится атомарно перед relist; перечитывается только
изменившийся server, а notification, пришедшая во время relist, остаётся pending
для следующего turn. Failed relist восстанавливает claim и сохраняет прежний
catalog. Пропущенный или конфликтующий `Authorization` даёт явную ошибку до
network: headers нормализуются в typed `HeaderMap` case-insensitively, а
duplicate с разными значениями — terminal config error конкретного server (T46/R6 target:
failed entry, не app-wide optional-service failure). Search вызывает
server schema `query` + `response_length`, не helper `limit`.

## Direct exposure vs Code Mode

**Superseded by T60 (owner amendment 2026-10-13):** как у донора, omitted или `codemode: true` переводит инструменты сервера в каталог `execute` (`tools.<server>.<tool>`), `codemode: false` оставляет их прямыми. `execute` исполняется нативным Rust-интерпретатором (`crates/oc-adapters/src/codemode`), не Node eval/shim/remote execute; вложенные вызовы проходят обычный admission/approval/MCP lease. Ниже — история прежнего direct-only решения.

В `oc-rs.toml` профиль явно `tool_exposure = "direct"`. При отсутствии upstream codemode field выбранный профиль означает direct — это объявленное отличие. Explicit true = actionable unsupported error. Не запускать JS interpreter, Node eval, Code Mode shim или remote execute to emulate missing interpreter.

The owner explicitly keeps this exclusion under T50. Native opencode_* tools are
ordinary direct definitions, not a shim for tools.opencode via execute. Built-in
websearch and its auth/provider selection are absent; configured MCP tools keep
their real identities/schemas and can search without inventing a built-in alias.

## T42 qualification — config source trust

`opencode.json`/`opencode.jsonc` sources are admitted per root (global config dir, Location
root, `.opencode`) only when the canonical file stays inside that root's canonical path; a
symlinked config resolving outside is refused with a safe source diagnostic, so a
source can never be canonicalised outside and then marked trusted. The `.opencode` definition
root must stay inside the Location root, and `AGENTS.md` files must stay inside their own
root. In-root symlinks are still admitted (containment policy, not a blanket symlink ban), and
the admitted source directory itself remains pinned/no-follow. W5 `{file:...}`
references may instead be absolute, `~/` from captured HOME, parent-relative or
symlinks; content is trimmed without double escaping and without the old64KiB cap.
Unreadable/nonregular references become empty plus a payload-free warning, including
explicit disabled-MCP activation. A connected server's warning remains visible in
the Services indicator and `/mcps` Enter-details without changing Connected/Disconnect.
This does not widen config/discovery/instruction
source roots. The Location switch itself re-runs source admission for the
target root, so a switch cannot smuggle in an outside source.
