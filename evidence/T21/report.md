# T21 — Local MCP stdio

Status: PASS (pinned fake + bounds). Real-server smoke: opt-in only, not executed here.

## MCP04 Stdio — PASS

- Fake child JSON-RPC on stdout: `initialize` handshake, `tools/list`, `tools/call` (`env`/`args`/`boom`) over rmcp child-process transport; unknown methods get `-32601`.
- Bounded stderr: 64 KiB flood kept to an 8 KiB prefix with `truncated` set; `SECRET_TOKEN=fake-secret-123` emitted by the child is redacted to `***` via configured secrets.
- Kill/reap: `sleep 30` spawned bare, alive-probed, `kill_reap` (stdin close → SIGTERM → 500 ms grace → SIGKILL → wait) reaps; post-wait signal probe proves no zombie.
- Restart config generation: same retained argv respawns and re-handshakes, generation 1→2, catalog intact.
- No cross-auth/cwd sharing: child env is `env_clear` + explicit extras only — parent `OC_T21_POISON`, real `LUDKA2_API_KEY`/`LUDKA_API_KEY`, and `PATH` all absent from the child dump; `HELLO=world` extra passes through; credential-named extras (`*KEY*`, `*TOKEN*`, …) refused at validation; cwd defaults to the system temp dir, never the runner cwd.
- `argv[0]` resolves via read-only parent-PATH lookup (`libc::access(X_OK)`); the child never inherits `PATH`; argv tail passed verbatim.

## MCP05 Disabled browser — PASS

- `enabled: false` (direct or `McpEntry`) returns `Disabled` before any spawn: zero process, zero probe, no Node requirement.
- `McpEntry` mapping keeps the exact `command` argv (`["npx","-y","pkg@latest"]` asserted); remote kind, OAuth, Code Mode, and empty command refused.
- Real smoke exists as ignored `real_server_smoke`, gated on explicitly set `MCP_SMOKE_ARGV` JSON array; nothing auto-includes Node or a browser.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- `cargo test -p oc-adapters --test mcp_stdio` 6/6 (+1 ignored real smoke); workspace 133 total (oc 4 + adapters 85 + mcp_remote 15 + mcp_stdio 6 + core 16 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. New fixture `fixtures/fake-mcp-stdio.sh` (mode 0755, POSIX sh only).

## Scope and limitations

- Turn-loop/executor wiring of stdio tools arrives with the runtime consumer; T21 proves lifecycle, isolation, and mapping semantics.
- `sleep`/`sh` fixtures assume a Linux runner; documented in tests.
