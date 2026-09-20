# Итоговая квалификация oc

Global status: NOT_RUN. Code commit: NOT_SET. Dirty: NOT_SET. Delivery: NOT_RUN.

Разрешённые gate statuses: `PASS`, `FAIL`, `BLOCKED_EXTERNAL`, `NOT_RUN`; `NOT_RUN_DISABLED` — только conditional browser smoke. `READY` требует PASS всех A01–A13 и mandatory live T16/T27. `BUILD_READY_LIVE_BLOCKED` требует PASS всего offline scope и точный список отсутствующих live prerequisites. Delivery (`PUSHED`/`BLOCKED_REMOTE`) независим.

## Qualification runs

| Command/run | Exit/status | Method/environment | Evidence |
|---|---|---|---|
| NOT_RUN | NOT_RUN | NOT_SET | NOT_SET |

## A01
Status: NOT_RUN. Scenarios: BUILD01–BUILD03, OPS04. Evidence/limitations: NOT_SET.

## A02
Status: NOT_RUN. Scenarios: STORE01–STORE05 and owned-process cleanup. Evidence/limitations: NOT_SET.

## A03
Status: NOT_RUN. Scenarios: CFG01–CFG08, DISC01–DISC10. Evidence/limitations: NOT_SET.

## A04
Status: NOT_RUN. Scenarios: PROV01–PROV08, TOOL10. Evidence/limitations: NOT_SET.

## A05
Status: NOT_RUN. Scenarios: TOOL01–TOOL11. Evidence/limitations: NOT_SET.

## A06
Status: NOT_RUN. Scenarios: MCP01–MCP06, E2E04. Evidence/limitations: NOT_SET.

## A07
Status: NOT_RUN. Scenarios: DCP01–DCP09, E2E03. Evidence/limitations: NOT_SET.

## A08
Status: NOT_RUN. Scenarios: UI01–UI06. Evidence/limitations: NOT_SET.

## A09
Status: NOT_RUN. Scenarios: E2E01–E2E03. Evidence/limitations: NOT_SET.

## A10
Status: NOT_RUN. Scenarios: LOAD01–LOAD04 and referenced storage/UI evidence. Evidence/limitations: NOT_SET.

## A11
Status: NOT_RUN. Scenarios: ENV01, ENV02, OPS02, OPS03, OPS05. Evidence/limitations: NOT_SET.

## A12
Status: NOT_RUN. Scenarios: OPS01, OPS03 and FINAL review. Evidence/limitations: NOT_SET.

## A13
Status: NOT_RUN. Scenarios: CFG05–CFG08, TOOL11, UI06, E2E05. Evidence/limitations: NOT_SET.

## Запуск, differences и ограничения

Actual build/run command, required external tools, env NAMES only, supported differences, known limitations, remaining blockers и следующий шаг. Не включать секреты/raw endpoint payload и не обещать full OpenCode parity.
