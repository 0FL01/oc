# Goal: MCP attach parity с opencode v2.0.12

Status: active
Source: инструкция владельца 2026-09-22 (паритет поведения opencode 2, без костылей), reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (commit `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-09-22

## Objective

Сбой одного MCP-сервера не отменяет пользовательский turn. Native `oc` повторяет поведение
upstream v2.0.12: сервер получает статус failed с санитизированными stage/code, его инструменты
не публикуются в generation, turn выполняется дальше, а деградация видима пользователю.
Дополнительно outbound HTTP-клиенты отправляют `User-Agent`: JS runtime upstream всегда его
отправляет, `reqwest` по умолчанию — нет, из-за чего Cloudflare перед настроенным crw endpoint
отвечает `403 Error 1010` (проверено probe: с любым честным UA — 200 + MCP initialize).

## Required Outcomes

- R1: per-server деградация вместо отказа turn.
  - Source: upstream `packages/core/src/mcp/index.ts` (status `pending|connected|disabled|failed|needs_auth`, `logWarning`, turn продолжается) и `docs/DECISIONS.md` D13.
  - Acceptance: `attach_mcp` при ошибке одного enabled-сервера записывает санитизированную деградацию (`mcp <id> <stage>: <code> (retryable=<bool>)`), выполняет cleanup этого сервера и продолжает; turn завершается, инструменты упавшего сервера в реестре отсутствуют.
  - Status: pending
  - Evidence: `evidence/T46/report.md`

- R2: деградация видима, а не silent.
  - Acceptance: `TurnReport.warnings` несёт деградации generation; TUI показывает `warning: …` note, headless печатает `warning: …` в stderr; в предупреждении нет URL, заголовков и значений секретов.
  - Status: pending
  - Evidence: `evidence/T46/report.md`

- R3: `User-Agent` на outbound HTTP-клиентах.
  - Acceptance: MCP remote, provider generation и discovery отправляют статический `oc/<version>`; webfetch — browser-like `oc-user/1.0` (паттерн upstream `OpenCode-User/1.0`: страницы блокируют клиента без UA); тесты фиксируют наличие UA на каждом клиенте.
  - Status: pending
  - Evidence: `evidence/T46/report.md`

- R4: живая проверка в окружении владельца.
  - Acceptance: `oc run` с включённым crw подключает сервер (каталог доступен), а `codex_web` остаётся connected с search; при недоступном сервере turn завершается с видимым warning.
  - Status: pending (требует live credentials владельца)
  - Evidence: `evidence/T46/report.md`

## Constraints

### Backend follow-up (2026-09-22)

R1–R3 code is delivered in `3ce5e03`/`b7ab06d`; historical report is not a new
full-live PASS. Current additive evidence: `evidence/T46/backend-parity.md`.
Required backend repair adds structured JSON/textual resource/link tool results,
safe fixed error categories, and bounded permission-filtered initialize guidance
in provider projection only. Existing fatal lifecycle/caps and redaction hold.
R4 remains pending until full bounded owner-live evidence. Media tool outputs,
MCP prompts/resource catalogs remain explicit open parity work, not silently
excluded by the older non-goals list. This follow-up does not change T44 surfaces.

- C1: остаются фатальными: `Cancelled`, ошибки cleanup/`McpShutdown`, `MAX_MCP_SERVERS` и generation-капы каталога; AUD23 reaping ранее подключённых серверов сохраняется.
- C2: диагностика санитизирована: только server id, stage и safe code; никаких URL, заголовков и значений.
- C3: изменение прежней политики (fatal attach) фиксируется записью D13; тесты переписываются как осознанное изменение контракта, а не ради зелени.

## Non-goals

- `oc mcp list` CLI и персистентная панель статусов (отдельный follow-up).
- OAuth, Code Mode, browser-like UA, расширение `{file:}` (absolute/`~/`) и прочие parity-дельты, не влияющие на наблюдаемый сбой.
