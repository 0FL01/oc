# Goal: MCP attach parity с opencode v2.0.12

Status: active
Source: инструкция владельца 2026-09-22 (паритет поведения opencode 2, без костылей), reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (commit `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-09-27

## Objective

Сбой одного MCP-сервера не отменяет пользовательский turn. Native `oc` повторяет поведение
upstream v2.0.12: сервер получает статус failed с санитизированными stage/code, его инструменты
не публикуются в generation, turn выполняется дальше, а деградация видима пользователю.
Дополнительно outbound HTTP-клиенты отправляют `User-Agent`: JS runtime upstream всегда его
отправляет, `reqwest` по умолчанию — нет, из-за чего Cloudflare перед настроенным crw endpoint
отвечает `403 Error 1010` (проверено probe: с любым честным UA — 200 + MCP initialize).

Owner-approved 2026-09-27: минимальный status/control prerequisite для настоящей
MCP-модалки T44/VIS40 также входит в T46/R5. Это расширение существующего владельца
generation/clients, не новая служба или claim полного MCP/OAuth parity.

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

- R5: typed MCP status/control projection для T44/VIS40.
  - Source: owner-approved MCP modal plan 2026-09-27; pinned `packages/core/src/mcp/index.ts:380–447,593–609` and `packages/tui/src/component/dialog-mcp.tsx:37–175` (U50/U54).
  - Acceptance: MCP08 proves bounded current-Location/config-generation server snapshots, including disabled entries, from actual owner state; configured-enabled is not connected. Genuine pending/connected/disabled/failed and any typed auth-required outcome are not parsed from server names/free text. Actual async connect/disconnect/retry reuse existing clients/registry; connected requires initialize and catalog success, disconnect closes owned resources, catalog changes publish only at safe request boundaries. Reads/resize/cancel stay responsive during a turn instead of waiting behind its long-held MCP mutex. Pending actions are coalesced/revalidated by server/Location/generation, late completions cannot mutate a new generation. Failed attach remains visible per D13 without cancelling the turn; fatal cancellation/cleanup/caps remain. Runtime controls do not secretly rewrite config, auto-enable disabled browser, clear quarantine or replay uncertain tool effects. Reopen reads current state; restart rebuilds state from effective config, never restores stale connected labels or claims persisted runtime toggles. T44 consumes this slice and separately qualifies the modal; no all-T46 or reverse completion dependency.
  - Primary evidence: rebuilt actual binary + fake HTTP/stdio initialize/catalog/disconnect/retry counters and owned process cleanup, current/next request catalogs and Location/restart/late-action assertions under MCP08, reusing MCP07/AUD23/MCP05.
  - Status: pending
  - Evidence: pending — `evidence/T46/report.md`; paired visual evidence belongs to T44/VIS40.

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
- C4: R5 uses the existing application/runtime/resource owner, immutable request/config generations and bounded snapshots/events/actions. No second client registry/store/event bus, daemon, JS host or periodic UI polling. Retry is a new admitted connection attempt, not permission to repeat an unknown MCP tool call; sticky remote quarantine and cleanup failure remain actionable, non-success states.
- C5: native OAuth remains outside GOAL. Donor `needs_auth`/integration sign-in is recorded in the capability mapping before capture: render only actual typed state and make unsupported sign-in honest/actionable, never launch a fictitious OAuth flow. Safe diagnostics only in details/copy/investigation drafts, never raw remote error text, URL/header/env values. MCP result/media/prompts/resource work and R4 live qualification are not closed by MCP08/VIS40.

## R5 sequence and visual consumer

1. Add minimal typed current-Location/generation status snapshots/events to existing
   owners; preserve lazy/startup admission truth, disabled zero-spawn and no false
   connected state before a successful handshake/catalog. Modal reads cannot spawn
   disabled entries or repeatedly reconnect healthy clients.
2. Deliver real connect/disconnect/retry, per-server pending/coalescing/cleanup and
   safe catalog publication without changing an in-flight request or config on disk.
3. Qualify MCP08 actual-binary fake-server effects, then T44
   [VIS40](../../tui-recovery/T44_CONTRACT_AMENDMENT.md#mcp-modal-parity--vis40)
   full paired dialog frames/focus/actions/colors/state transitions. Preserve failed
   attempts/provenance and report backend versus visual results separately.

Approval freezes this plan, not implementation PASS. Existing execution statuses,
evidence and dependencies remain factual; independent T44 slices stay ready.

## Non-goals

- `oc mcp list` CLI и отдельная персистентная status service/panel остаются вне этого среза. Typed snapshots/control для T44/VIS40 входят в R5; прежняя отсрочка всех статусов superseded владельцем 2026-09-27.
- OAuth, Code Mode, отдельный browser-like UA для MCP (webfetch R3 сохраняется), расширение `{file:}` (absolute/`~/`) и прочие parity-дельты, не влияющие на согласованные R1–R5.
