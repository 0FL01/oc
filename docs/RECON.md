# Recon v3 — что действительно проверено

На 2026-09-20 документарно прочитаны GitHub metadata/ref и перечисленные файлы DCP/OpenProxy, официальная документация Codex/MCP/AI SDK, а также configured-workspace implementation/tests OpenCode `v2.0.10` на pinned commit. Executable upstream runner не запускался; source-derived fixtures не называются differential parity.

OpenCode recon подтвердил global config root и discovered `.opencode` roots, `opencode.json` перед `opencode.jsonc`, отдельные source streams для direct config и directory definitions, singular/plural `skill(s)`, `agent(s)`, `command(s)`, `plugin(s)`, а также domain-specific collisions. `AGENTS.md` — ordered concatenated prompt inputs, не override. Skills имеют каталог metadata и отдельную загрузку body; commands поддерживают argument substitution, но upstream shell/subagent execution в native scope намеренно запрещены. Primary-agent subset отделён от upstream subagent modes.

Upstream JS/TS plugin source directories исполняются JavaScript runtime, чего Rust goal не переносит. V3 использует только exact declarative markers: admitted `{plugin,plugins}/openproxy-models.js` и exact DCP package aliases; их contents не являются исполняемым authority. Семантика OpenProxy discovery по-прежнему определяется `references/openproxy-models.user.mjs`.

DCP default branch pinned к `11f6517780a502512a3467645074be447cb0369e`; package 3.1.15, AGPL-3.0-or-later; README описывает range/message, compress, unchanged history, dedup и purgeErrors. Range public argument types прочитаны отдельно. Полный compressor/hook test suite НЕ исполнялся; точные edge cases должны быть закреплены T02/T17–T19.

OpenProxy pinned к `4ef76dbce2cdbb85206cbe5e59acbad9d96ae387`; source routes содержит Responses endpoint и 32 MiB request body limit. `codex_web_mcp.rs` имеет POST `/v1/mcp`, strict 2025-11-25, bearer auth и search tool. Не подтверждены фактическая версия/наличие живого пользовательского deployment и конкретные advertised models.

`plugins/openproxy-models.js` в этом commit имеет DISCOVERY_TIMEOUT_MS=10000. Присланный владельцем вариант с attempt 15000/total 30000/retries/empty list handling приоритетнее. В архиве сохранён нормализованный по форматированию user source; byte hash архива не является оригинальным byte hash сообщения пользователя.

В публичной CLI reference подтверждены `/goal`, предел objective 4000 chars и `--yolo` semantics. Это не проверка установленного CLI пользователя. Rust 1.98.1/Debian13.4/11.68GiB — предоставленные user observations, проверка host предстоит.

Не выполнялись: сборка Rust oc, upstream runner, Rust port tests, обращения с credentials к OpenProxy/MCP, browser smoke, benchmarks на host. Пакет содержит specs, не результаты этих проверок. Собственные Python utility checks, offline tests присланного discovery JavaScript и структура архива проверены отдельно в `evidence/PACKAGE_VALIDATION.md`.

## Входные материалы, не переносить их смысл скрыто

Новый GOAL сохраняет сужение исходного плана: no OAuth/providers zoo, no importer/publishing, no required Code Mode/API daemon. Configured workspace — явно добавленный owner scope, но subagents, shell commands, remote skills и JS/TS runtime не добавлены. Unified apply_patch — выбор владельца. Native byte caps, immutable generation/snapshot policy, output/retry/live envelope, initial Responses store:false replay и branch policy — implementation defaults этой редакции, не якобы уже подтверждённое upstream behavior.
