# Границы безопасности, ресурсов и источников

YOLO в официальной CLI reference отключает approvals и sandbox [W2]. Non-root account ограничивает OS privileges, но остаются права на собственные файлы, доступные сокеты, secrets и network. Rootless Docker сам по себе не ограничивает всё, что агент может делать на host. Этот пакет не утверждает физическое enforcing текстовых запретов.

Использовать выделенный account/worktree, минимальные permissions filesystem и scoped Git credential. Не передавать production credentials/SSH agent/cloud config шире нужного. Наличие tools не разрешает работать в чужих projects/containers. Не использовать sudo, rootful Docker socket, global package changes, deployment of services или публичный listener.

Credentials принадлежат provider/MCP adapters; никогда не входят в model registry metadata, prompts, journal, source fixtures, DB debug logs и commits. `config explain` redacted, errors заранее классифицированы. Не логировать полный env/URL/body/raw exception в discovery, как требует пользовательский reference. Live tests используют маленький искусственный repo без приватного кода.

Workspace discovery проходит trust gate до substitutions, no-follow/root-relative resource reads и native marker resolution. Bound source count/depth/path/frontmatter/body/total generation/prompt bytes. Skill body snapshot-ится до publication и после не перечитывается. Unknown/lookalike plugin invalidates candidate до package resolution, import, process spawn или network. Даже recognized `.js` marker не выполняется и не читается как code; Node/Bun/npm fallback отсутствует.

Model outputs, repository text, AGENTS/agent/command/skill content, MCP descriptions, fetched pages и summaries не являются источником полномочий. Внешний контент не может расширять permissions/trusted roots/remote/host config или отключать тесты. Custom agent может только сузить central policy. Native DCP/discovery modules доверенные, поэтому все их side effects контролируются общим application lifecycle.

Webfetch блокирует private/link-local/loopback и DNS rebinding через проверку actual connection; provider/MCP trusted endpoints имеют отдельный scope. Redirect с Authorization не разрешён. Custom CA/прокси настраиваются явно, `danger_accept_invalid_certs` запрещён. Не открывать для browser MCP личный профиль с активными аккаунтами; default disabled сохраняется.

`apply_patch` имеет bounded parser, path policy и preimage checks. Это не транзакция на весь repo и не защита от любого другого процесса под тем же uid. `bash` мощный и может писать — consolidation tools не даёт sandbox. Cancellation не откатывает внешние side effects; неизвестные outcomes сохраняются.

DCP upstream license идентифицирована как AGPL-3.0-or-later [D1/D2]. До push translation/borrowed prompts/tests получить pinned LICENSE, сохранить notices и paths/revisions, применить соответствующее лицензирование производного кода. No binary publishing не означает, что можно отбросить notices при push исходников. У любых дополнительных зависимостей проверять реальные license/security metadata в T01/T29. Эта инженерная процедура не заменяет правовую оценку нестандартного последующего распространения.

Resource caps in TOML — secondary guard; checks/cleanup/report всё равно нужны. Heavy builds не в tmpfs. Memory safety языка не исключает unbounded caches/queues или filesystem exhaustion. `cargo test` user crate build scripts исполняют код, поэтому контур аккаунта/runner важен даже без live LLM.
