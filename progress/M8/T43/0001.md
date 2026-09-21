## Result

R1/R2 закрыты: owner config загружается без единого config-warning (остались только
DCP `allowSubAgents`, который снимет R3, и внешний `crw` Cloudflare 403). Искусственные
size-лимиты убраны; совместимость markdown-config приведена к upstream v2.0.12.

## Checks

`cargo fmt` clean; clippy `-D warnings` exit 0; `cargo test --locked --workspace --no-fail-fast`
337 passed / 0 failed / 4 ignored; release build ok; live `oc run` на owner config (crw off) →
`session s-...` + `pong`; real HOME → только DCP warning + `mcp attach failed for crw`;
orphan-процессов нет. `aud30_pty_paste_resize_error_recovery` — pre-existing PTY-flakiness
(в изоляции 3/3 passed).

## Risks

`mode: subagent|all` и command `agent/model/subagent/subtask` парсятся, но ещё не исполняются
(R3). `crw` остаётся недоступным из клиента (Cloudflare 1010) — для запуска нужен disabled
или другой endpoint.

## Next

R3: срезы 1–8 из `evidence/subagents/upstream-v2.0.12-plan.md` (config admission → child
persistence → catalog/route → foreground subagent tool → background/notices/reap → command
routing → DCP allowSubAgents → TUI/history), коммит+push каждого среза.
