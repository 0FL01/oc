# Конфигурация и пользовательский профиль

## Разделение файлов

Upstream-compatible inputs: global/Location `opencode.json`/`opencode.jsonc`, `.opencode`, ordered `AGENTS.md`, local skill/agent/command definitions, `dcp.jsonc/json`. Global `cli.json` — отдельный read-only TUI domain: он не является project config и не участвует в runtime-config precedence. Native controls: `oc-rs.toml`. Никаких runtime writes в исходные configs, auto-install packages или credential migration.

В supplied примере provider snippet был фрагментом с лишними завершающими скобками/запятыми. `examples/opencode.jsonc` — заново собранный валидный полный пример, не побайтная копия; URL/keys остаются env references. Модели в основном примере не зафиксированы. Static модели из Q01 сохранены отдельно как fixture `examples/static-models.user.json` с отметкой «пользовательские metadata, не проверенные публичные спецификации».

Discovery native enabled для ludka2; static ludka можно включить и явно задать models. `enabled_providers` в стартовом примере содержит только ludka2, чтобы отсутствие LUDKA_* не ломало его работу. Это явно выбранный ready-to-start default, не удаление поддержки ludka.

## Source loading

Baseline discovery/precedence получить из pinned OpenCode config tests и сохранить source-derived fixture в T02. `G` — `$OPENCODE_CONFIG_DIR`, если задан, иначе platform XDG OpenCode config root; env override заменяет default, а не добавляет второй global layer. Для admitted sources low→high: `G/opencode.json`, `G/opencode.jsonc`, optional explicit `--config`, direct `opencode.json` затем `opencode.jsonc` от Location root к working directory, затем `.opencode` roots от Location root к working directory с JSON перед JSONC. Более широкий upstream walk за trusted Location boundary — documented difference, не скрытое чтение parent/home filesystem.

Pipeline: source discovery/provenance → explicit trust decision для canonical Location/source boundary → text substitutions/no-follow resource reads → JSONC parse → normalize supported shapes → domain-specific merge → typed capability validation → fully built candidate generation → atomic publication. До trust нельзя читать `{file:...}`, разрешать lower-trust endpoint/command с higher-trust credential или запускать network/process. Failure сохраняет прежнюю generation; mixed old/new config, instructions, catalogs, plugins или clients не публикуются. Подстановки выполняются ДО parsing согласно pinned recon, не переносить старую ошибку плана v1.

Missing required selected-provider key приводит к MissingCredential, а не fallback provider. Отсутствующие env references обрабатываются по pinned substitution contract (пустое значение), но обязательность credentials проверяется только для выбранных/enabled integrations после normalization. Disabled provider/MCP не запускает network/process и не требует своих credentials. Это НЕ обещает lazy file substitution: доверенный config с {file:...} может читать ссылку до entry filtering; trust gate применяется ко всему источнику. Пустая подстановка не является действительным ключом и не выводится как secret в effective config. Не вводить второй противоречащий parsing pipeline ради пропуска неиспользуемых env keys.

JSONC comments/trailing commas и source locations сохраняются для diagnostics. Unsupported arbitrary plugin/provider package даёт имя config field и capability, которой не хватает. Не «поддерживать» настройку только тем, что Serde её проглотил. Security-relevant invalid candidate не публикуется; malformed отдельная definition исключается с path/field/reason, пока selected/default reference на неё не превращает всю candidate generation в hard error.

DCP domains, provider model metadata/variants, MCP entries, skills, agents, commands и permissions имеют отдельные merge rules. Native own settings не переопределяют смысл известных upstream fields. Источник и shadowed origins доступны в `oc config explain`, secrets и sensitive absolute paths redacted.

## Instructions и definitions

`AGENTS.md` — не config override. Effective instructions состоят из canonical-deduplicated `G/AGENTS.md`, затем applicable Location files в pinned nearest-working-directory-to-root order; distinct sentinel каждого admitted файла входит ровно один раз с provenance. Unreadable/disappeared file даёт diagnostic и не сохраняет текст из старой candidate. Reload возможен только между turns.

Automatic definitions читаются только из `G` и admitted `.opencode` roots: `{skill,skills}/<id>/SKILL.md`, `{agent,agents}/<id>.md`, `{command,commands}/<id>.md`. Singular root обрабатывается перед plural в одной source directory; inline JSON/JSONC declaration — перед Markdown definitions этого root. Exact order/collisions закрепляются fixture, а не filesystem enumeration. Flat skill files, `.claude`/`.agents`, legacy modes, URL/remote/package skills и symlink escape unsupported.

Skill ID выводится из canonical directory ID; supported frontmatter — bounded `name`/`description`, body — instructions. Later source целиком заменяет duplicate ID с shadowing provenance. При построении generation body проверяется как regular no-follow file, bounded и snapshot-ится вместе с digest; model catalog получает только bounded id/name/description. После publication файл повторно не открывается.

Agent subset — selectable primary profiles: Markdown body/system, description, validated model/variant и optional permission restrictions, которые только пересекаются с central policy. `default_agent` и persisted selection обязаны ссылаться на effective profile; исчезновение selected profile блокирует следующий turn, без silent fallback. `mode:subagent`, `mode:all`, tools/hooks/env/request headers/body, child/background behavior и любое расширение authority дают `UnsupportedCapability`. Later agent source обновляет только supported fields с field provenance; permissions сохраняют ordered restrictive semantics.

Command subset — Markdown body, description, `$ARGUMENTS` и positional `$1...`; duplicate later source заменяет definition целиком. Invocation делает одно bounded literal expansion и один обычный durable `SubmitInput` текущей session. Original invocation и expanded prompt сохраняются; restart их не разворачивает повторно. Reserved built-in collision, subagent/subtask, shell interpolation, recursive/background/direct tool execution, file/URL/late-env include дают `UnsupportedCapability`.

## Native plugin classification

До resolver/import/process/network классифицируются только exact identities. Bare `@tarquinen/opencode-dcp`, pinned `@tarquinen/opencode-dcp@3.1.15` и пользовательский exact alias `@tarquinen/opencode-dcp@latest` обозначают один compiled DCP module фиксированной repository revision: `@latest` здесь НЕ вызывает registry resolution и не меняет revision. Exact canonical `<effective-config-root>/{plugin,plugins}/openproxy-models.js` обозначает один compiled OpenProxy discovery module; файл не читается и не исполняется. Exact `@prevalentware/opencode-goal-plugin@0.1.49` из общей authoring-конфигурации даёт warning и нулевую runtime capability: код пакета не загружается. Basename вне admitted root, `.ts`, URL/arbitrary path, ranges, другие versions/packages и любой unknown JS/TS дают source-qualified `UnsupportedPlugin`. Duplicate aliases idempotent. Provider alias `@ai-sdk/openai` остаётся отдельным config domain, не plugin identity.

## Generation и limits

Одна session навсегда принадлежит одному Location; один turn держит immutable `(LocationId, ConfigGenerationId)` и selected-agent digest до завершения. Reload/agent selection только между turns. Location switch полностью строит target generation и выбирает/создаёт Location-scoped session, не retarget-ит активную. Config/agent change открывает новую provider causality generation и не повторно использует opaque continuation items.

Ограничить source count, directory depth/path length, frontmatter nesting, body/metadata bytes, total generation bytes и model-visible catalog/prompt bytes. Over-limit entry отклоняется явно; silent truncation не должна рекламировать skill, который нельзя вызвать. TUI получает redacted projection generation без credentials, expanded secrets, full skill bodies и sensitive paths.

## Commands — проектируемый CLI

`oc`/`oc tui`: local TUI, no daemon. `oc run <prompt> --model <provider/model-id> [--variant name] [--json]` — headless. `oc sessions list`, `oc run --session ID <prompt>` — история/продолжение. `oc models list [--refresh]`, `oc config check`, `oc config explain`, `oc --version`, `oc --help`.

`--config path` и `--native-config path` задают явные источники для tests/запуска. `--data-dir path` изолирует storage. Implement spelling один раз в M1/M2 и синхронизировать fixtures/help, не поддерживать десяток синонимов. Нет TTY без subcommand → usage error. `--json` stdout содержит только NDJSON; пользовательские diagnostics и безопасные notices — stderr.

Production default model: explicit config selection или persisted пользовательский выбор TUI. При отсутствии выбора headless сообщает ModelRequired и доступную команду list; не выбирает «самую умную/первую» автоматически. Test runner selection — отдельная политика TEST_PLAN.

## Native settings и units

Пример TOML — контракт будущего parser, не config существующего upstream. Числа имеют явные units в key. `provider.options.timeout/chunkTimeout` upstream имеют их собственную semantics; не применять discovery milliseconds как generation timeout.

Initial safety defaults предложены этой редакцией, меняются осмысленным config/decision и не являются измеренными performance budgets. Per-event 4 MiB, model tool args 2 MiB, serialized request 24 MiB (ниже observed proxy cap 32 MiB), preview 64 KiB, retained tool output 16 MiB, active queue 8 MiB, blob quota 2 GiB, max turns 128/запуск. Не выделять все capacity upfront.

Превышение explicit limit видно как error/truncated + counters; history не silently удаляется для продолжения. Адмиссия контекста отдельно учитывает input/output/model limits. Не забывать input base64 expansion у attachments.

## Config capability report

На конец M2 поддержанные domains и known differences фиксируются в `evidence/T07/compatibility.md`; к финалу — единый раздел FINAL.md. Минимум differences: bounded Location discovery; direct MCP default; unified apply_patch; primary-only agents; non-executable commands/skills; exact native plugin mappings; strict malformed security config; read-only config/native extensions; DCP scope; own storage/CLI names; safe byte caps. Unsupported audio/video/pdf, subagents, Code Mode/OAuth/другие npm packages не объявляются supported.
