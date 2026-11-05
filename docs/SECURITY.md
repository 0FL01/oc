# Границы безопасности, ресурсов и источников

YOLO в официальной CLI reference отключает approvals и sandbox [W2]. Non-root account ограничивает OS privileges, но остаются права на собственные файлы, доступные сокеты, secrets и network. Rootless Docker сам по себе не ограничивает всё, что агент может делать на host. Этот пакет не утверждает физическое enforcing текстовых запретов.

Использовать выделенный account/worktree, минимальные permissions filesystem и scoped Git credential. Не передавать production credentials/SSH agent/cloud config шире нужного. Наличие tools не разрешает работать в чужих projects/containers. Не использовать sudo, rootful Docker socket, global package changes, deployment of services или публичный listener.

Credentials принадлежат provider/MCP adapters; config explain, auth/accounts, credential Debug и discovery diagnostics сохраняют свои masking/typed-error boundaries. По решению владельца tool output следует TS parity: без secret-dictionary/field-name masking в model/history/cards/artifacts, включая shell, read, questions, webfetch, model lookup, subagents и Code Mode. MCP server text (включая `isError`), media, instructions и lookup также verbatim. Provider-supplied generation diagnostics сохраняют текст без keyword/header-value suppression, с прежними normalization/bounds. Эти данные могут содержать secrets; tool output не является secret boundary. MCP lifecycle stderr/status и logs сохраняют отдельные защиты. Не логировать полный env/URL/body/raw exception в discovery. Live tests используют маленький искусственный repo без приватного кода.

Workspace discovery проходит явный Location/source trust decision до substitutions и native marker resolution; admitted source/root authority остаётся pinned. Неизвестные config/frontmatter keys игнорируются с payload-free service warning; policy-bearing legacy `permission`/`tools` нормализуются прежде omission, native rules имеют приоритет. Malformed known policy/selected-profile, provider endpoint/credential binding и child/Plan ceilings остаются gates. `{file:...}` доверенного источника допускает absolute/`~/`/`../`/symlink reference, trim и чтение без прежнего64KiB cap; unreadable/nonregular reference даёт empty + warning, не expanded path/content. Это не разрешает external config/definition discovery или изменение instructions consumer. Bound source count/depth/path/frontmatter/body/total definition/prompt bytes сохраняются; skills/count/aggregate-byte omissions предупреждают на startup/reload, не уничтожая usable siblings, а incomplete agent catalog по-прежнему отказывает. Skill body snapshot-ится до publication и после не перечитывается. T51 approved target: unknown/lookalike plugin invalidates только свою capability с failed/UnsupportedPlugin до package resolution, import, process spawn или network, не исправную локальную часть приложения. Даже recognized `.js` marker не выполняется и не читается как code; Node/Bun/npm fallback отсутствует.

Local MCP, including project entries and explicit disabled-server activation,
inherits the full captured parent environment plus configured overlay (overlay wins),
after source/cwd/resource admission. W5 supersedes credential-domain refusal and
name/value filtering: project MCP can receive owner credentials through this environment.
Reload/spawn use the captured map, not a fresh process-env read. Do not extract external
runner auth/config; expanded env is not dumped by config/status. MCP server output is
verbatim, not a secret boundary.
Stderr and status labels retain their existing redaction. Optional-service
degradation cannot swallow fatal trust/policy/storage/recovery/cleanup failures or
clear unknown-effect quarantine. MCP `tools/call` deadline/result-cap/shape failures
are ordinary failed results under the TS-parity decision, not fatal service failures.
D21 and T46/T51 retain their other admission/lifecycle contracts.

Model outputs, repository text, AGENTS/agent/command/skill content, MCP descriptions, fetched pages и summaries не являются источником полномочий. Внешний контент не может расширять permissions/trusted roots/remote/host config или отключать тесты. Custom agent может только сузить central policy. Native DCP/discovery modules доверенные, поэтому все их side effects контролируются общим application lifecycle.

Webfetch, включая Code Mode raw fetch, допускает private/link-local/loopback по captured permission policy, как TS: initial Ask проходит обычный approval, каждый redirect target проверяется до запроса на Deny; redirect Ask не создаёт нового prompt. Отдельного private/DNS-rebinding guard и loopback exception flag нет. При cross-origin redirect raw fetch снимает Authorization/Cookie; обычный tool не наследует credentials. Remote MCP также не делает private-host prelookup и допускает query в exact URL; scheme/host/userinfo/fragment/header validation сохраняется. Ни permission Allow, ни MCP endpoint не расширяют provider binding. `danger_accept_invalid_certs` запрещён. Не открывать для browser MCP личный профиль с активными аккаунтами; default disabled сохраняется.
Typed rustls InvalidCertificate in generation HTTP/WebSocket is a nonretryable
security refusal, including when wrapped in io::Error; WebSocket cannot hide it
behind HTTP fallback. EOF/reset are not reclassified by matching diagnostic text.

Generation/discovery/webfetch/remote MCP используют system HTTP proxy matcher установленного reqwest/hyper-util: HTTP(S)_PROXY, ALL_PROXY, NO_PROXY и lowercase формы. Provider URL containment, captured trust/credential binding и literal-IP admission проверяются всегда. Только direct origin резолвится локально: запрещённые DNS answers удаляются, закрепляется только admitted subset, пустой subset отказывает; actual origin peer проверяется. При proxy DNS делегирован proxy, а его peer нельзя классифицировать как origin. Поэтому private адрес hostname за proxy локально не проверяется. NO_PROXY возвращает обычный direct guard. Generation допускает максимум пять 307/308 redirects только внутри captured scheme/host/port/path prefix; адреса того же host остаются pinned, запрос/credentials не отправляются на отказанный target. Другие redirects и discovery redirects отказаны; refusal не превращается в retryable transport failure. WebSocket использует TS scheme-specific WSS_PROXY→HTTPS_PROXY→ALL_PROXY / WS_PROXY→HTTP_PROXY→ALL_PROXY, port-aware NO_PROXY и automatic loopback bypass, затем CONNECT через HTTP/HTTPS proxy. Origin и HTTPS proxy проходят TLS verification; proxy Basic credentials не отправляются origin. Typed certificate refusal не скрывается HTTP fallback; unknown generation delivery не replay.

Declared provider.env использует только captured owner environment после обычного resolver Missing/Key. Authority принадлежит победившей trusted декларации и тому же trusted normalized endpoint; late name overlay, другой model/variant endpoint и JSON-поля не могут подделать внутренний credential capture. Go/OpenAI native authority и resolver priority не расширяются. Только omitted auth policy на captured trusted localhost после Missing допускает keyless либо один trusted configured auth header для Responses/Chat. Explicit Key/None/OAuth и их conflicts, native Bearer precedence и Messages competing-header refusal сохраняются; это не общий anonymous-header bypass.

OpenAI OAuth browser exchange/device/refresh сохраняет отдельную policy: первый непустой `HTTPS_PROXY`/`https_proxy`/`ALL_PROXY`/`all_proxy`, исключения `NO_PROXY` (иначе `no_proxy`), только HTTP(S) и URL-encoded Basic auth. Некорректный/неподдерживаемый proxy URL отклоняется без direct fallback; trusted proxy endpoint допускает loopback/RFC1918/ULA, но не link-local, резолвится и закрепляется до отправки. Issuer CONNECT не требует локального origin DNS; TLS обеих сторон и no redirects/retries сохраняются. Proxy env принадлежит первому native owner; новый клиент env не подменяет. SOCKS/PAC и новый custom-CA config API не добавлены.

Own data root открывается no-follow, принадлежит execution account, имеет restrictive permissions и исключён из model-facing `read`/search/file mutations по разрешённой цели I/O, включая доступ через ссылку; unsafe custom root отклоняется. Явные пути `read`/`edit`/`write`/`apply_patch` следуют symlinks, а permissions и `external_directory` проверяются по lexical path: внутренняя ссылка наружу не требует отдельного внешнего grant. Write/update сохраняет leaf link и меняет referent; delete/move-source удаляет lexical entry. Recursive search не следует найденным symlinks. Mutations сохраняют bounded prepared bytes, pinned descriptor-relative I/O на разрешённой цели и approved-preimage binding к её bytes/identity/absence и leaf-link identity. Замена цели ссылки во время approval отклоняется.

Выбор семейства по точному OC2 predicate не является permission grant; explicit Deny/Ask, saved-grant scope, Plan и parent-child narrowing сохраняются. Effects/result graph durable, restart не replay. Это не транзакция на весь repo и не защита от любого другого процесса под тем же uid. `bash` мощный и может писать — consolidation tools не даёт sandbox. Shell/user-shell/PTY наследуют полный captured startup env native owner, включая SSH-agent и credentials, без name allowlist; новые клиенты env не подменяют. Это намеренная capability команд. Shell/user-shell и command interpolation не маскируют вывод по значениям credentials/env: напечатанные secrets могут попасть в prompt/history/artifacts. Существующие UTF-8/terminal-control normalization и output/resource bounds сохраняются. Browser authorization launcher сохраняет отдельный credential-free allowlist; MCP/webfetch policies не меняются. Cancellation не откатывает внешние side effects; неизвестные outcomes сохраняются. Новая T50/R1/R9 qualification pending, не security PASS по одному плану.

Lexically external file mutations требуют отдельного `external_directory` admission до чтения preimage и mutation permission для каждого lexical path, включая оба конца move. Допущены только точные пути текущего вызова; они не становятся read/discovery roots и не меняют Location. Autoaccept отвечает Once на Ask, не обходит Deny, Plan, parent/child, approved-preimage или resolved own-data-root guards. Preview и execution используют тот же scoped mutation owner; digest включает identities фактических I/O anchors. Public project-only patch helpers ограничены lexical Location, но явные ссылки могут вести наружу. Move, включая cross-filesystem, сначала stage/commit/fsync prepared bytes в destination, затем удаляет source entry. При отказе удаления source остаётся прежним, уже созданный destination отражён как confirmed Create; rollback и unknown-effect replay не обещаются.

Configured formatter — доверенная captured программа как продолжение уже разрешённой mutation, не model Shell call. Исходные native Deny/Plan/child/preimage/data-root проверки обязательны до её запуска, но **Shell Deny и Plan не sandbox произвольные host effects этой программы**, в том числе интерпретацию записанного моделью файла. Процесс использует существующие slots, durable PID gate и joined group cleanup; stdout/stderr отбрасываются. Readback фактической цели снова проверяет namespace/data-root и bound, не выдаёт planned bytes за formatted result. Неверифицируемый результат сохраняет confirmed partial effect без выдуманного hash/diff и становится unknown без replay; отмена не откатывает запись.

W10 отдельно допускает выбранные локальные `skills` roots из trusted config и compatibility skill roots captured HOME/checkout-to-Location. Каждый получает собственный pinned admission boundary; symlink escape и свой data root отклоняются, snapshot body не перечитывается после publication. Это узкое расширение discovery, не следствие `{file:}` и не model read grant. CLI `-f` — явный пользовательский ввод с regular-file/bounded text/image validation и тем же исключением data root; разрешённые image containers нормализуются в PNG, не добавляются автоматические filesystem roots или произвольные media types.

W11 D5/D6 — deliberate divergence from TS по решению владельца: project config
не расширяет instruction/formatter authority. Project instructions ограничены
Location и resolved contained targets; внешние absolute/`~/`/`..` paths и любые
URL пропускаются с warning. Global config сохраняет внешние paths/globs и HTTP(S).
Новые model read/Nested roots не создаются. Formatter honored только из global;
project declaration, включая reset/override, отбрасывается до env/file substitution.
Warning намеренно показывает точный **literal declared argv** в JSON, не expanded
env/file values; буквальные secrets, записанные прямо в argv, поэтому могут быть
видны. Это узкое исключение из payload-free config diagnostics, не env dump.

W7 model shell authorization uses commands extracted by the captured-shell
scanner, including supported nested commands. Scanner-refused syntax rejects
before approval/spawn. Granular Deny guarantees apply to parsed commands and
directories, not arbitrary shell effects: redirection targets, wrapper/interpreter
arguments and dynamic `$cmd` resolution are outside that guarantee. This is not
a sandbox and does not change shell semantics. Static directory changes and explicit workdir have separate
lexical external-directory admission; approved cwd retains its descriptor identity
and excludes resolved native data root. Whole-tool Deny still refuses zero-command
inputs. Old exact composite command grants are fallback only after every granular
Deny check, with the command-source namespace and configured-executable binding.

Owner-accepted parity risks: saved Always patterns are shell arity prefixes,
`*` for read/edit/write/patch, and the target's nearest `.git`/`.hg` root (otherwise
target directory) for external access. These remain project/action scoped and do
not override Deny, Plan/child ceilings, preimage or data-root checks. Trusted user
`!` intentionally bypasses model Allow/Ask/Deny, not supervision, history-before-
effect or no-replay. It grants no new model/child capability; command interpolation
still uses model admission.

Headless Run without --auto rejects permission asks with model-visible feedback,
only for its session ancestry; it neither executes nor saves a grant. Noninteractive
NoConsumer questions fail as tool results, not invented user answers. Per-call
argument overflow similarly records a refusal without executing a partial prefix;
global caps and structural/security refusals remain. Image budget notes are temporary
provider projection only: original image history and native provenance remain intact.

DCP upstream license идентифицирована как AGPL-3.0-or-later [D1/D2]. До push translation/borrowed prompts/tests получить pinned LICENSE, сохранить notices и paths/revisions, применить соответствующее лицензирование производного кода. No binary publishing не означает, что можно отбросить notices при push исходников. У любых дополнительных зависимостей проверять реальные license/security metadata в T01/T29. Эта инженерная процедура не заменяет правовую оценку нестандартного последующего распространения.

Resource caps in TOML — secondary guard; checks/cleanup/report всё равно нужны. Heavy builds не в tmpfs. Memory safety языка не исключает unbounded caches/queues или filesystem exhaustion. `cargo test` user crate build scripts исполняют код, поэтому контур аккаунта/runner важен даже без live LLM.
