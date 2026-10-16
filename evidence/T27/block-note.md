## Result

T27 (bounded live campaign) остановлен на checkpoint по owner-направлению 2026-09-21:
приоритет передан задаче T43 (config compatibility parity + subagent system,
`docs/goals/2026-09-21-config-compat-and-subagents.md`).

## Checks

Живой путь продукта подтверждён ранее в этом цикле: `oc run` с реальными credentials
завершается ответом (`session s-...` + `pong`) на `ludka2` / `ocg/muse-spark-1.3-contributor`,
когда MCP-сервер `crw` выключен. `POST {LUDKA2_API_URL}/responses` → 200 для обеих
документированных моделей; `/models` → 40 ids; `codex_web` `initialize` → 200 за 0.03 s.

## Risks

- Owner config `mcp.crw` (Cloudflare 403 Error 1010 для не-браузерного клиента) делает attach
  фатальным; attach-фейл остаётся фатальным по контракту (AUD23/loud failure), поэтому
  кампания требует `crw` выключенным или не-Cloudflare endpoint.
- Harness `crates/oc/tests/live_bounded.rs` требует declared model; `ludka2` моделей не
  декларирует (native discovery) — нужен приём модели, резолвимой discovery-путём, и
  консервативные лимиты из каталога продукта.

## Next

1. T43: R1/R2 (warnings + лимиты), затем R3 (subagent system) по срезам из
   `evidence/subagents/upstream-v2.0.12-plan.md`, коммит+push каждого среза.
2. Вернуться к T27 после T43: harness model acceptance для discovery-провайдера, кампания с
   обязательным `codex_web` и `crw` disabled, затем T30 FINAL по A01–A13.
