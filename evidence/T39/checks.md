# T39 checks

Environment: offline workspace, isolated HOME/data roots, scripted loopback
Responses peer, real PTY pairs. No live provider calls, no real credentials,
no mutation of user files.

## Targeted

| Command | Result |
| --- | --- |
| `cargo test -p oc --test pty_t39` | ok, 3 passed / 8.9 s (AUD29 panels, AUD30 paste/resize/error, AUD31 bounded state) |
| `cargo test -p oc --test pty` | ok, 13 passed (T26 regressions kept green) |
| `cargo test -p oc-tui` | ok, 34 passed (history window, intents, paste mapping, panels) |
| `cargo test -p oc --test mcp_application` | ok, 7 passed (TUI two-turn MCP ownership still green) |
| `cargo test -p oc --test pty_t39 aud30` ×3 | ok each run (paste/resize/Ctrl-C/error-handle stability) |

AUD29 (real state changes, asserted from the peer's request bodies and durable rows):

- model picker: request `model` switched to `alt-model`;
- variant cycle (Right): request body `reasoning.effort == "high"` from the
  declared `fast` variant;
- agent picker: agent prompt text reached the request and the agent's pinned
  model stayed effective;
- skills panel: real card `t39skill` rendered from the runtime catalog;
- custom command: template expanded by the application
  (`custom command payload for hello` in the request, `/t39cmd hello` persisted);
- `/dcp-compress`: the model called the `compress` tool, one compression block
  was stored, the tool op is `completed`, and the panel showed real saved tokens;
- session switch: the following turn landed in the switched session and not in
  the original one.

AUD30: bracketed paste with Cyrillic + emoji arrives as one event; a resize
during a stream keeps the turn running to completion; a session switch during
the stream is explicitly refused (`turn active; session switch refused`); Ctrl-C
exits cleanly; the alternate screen is left and slave termios is cooked+echo
afterwards; the persisted history is exactly the two expected user/assistant
pairs. With stdout pointed at a closed pipe (stderr/stdin on the PTY) the
renderer fails visibly (`error: draw: …`), exits nonzero, restores the terminal
and persists no user message.

AUD31: 3000-row history plus 20 extra sessions; 40 page-ups; `/cards` shows the
newest operation (`tool-0259`) — unreachable under the old oldest-200 query; a
session switch replaces the window. The opt-in metrics probe reports
`window_rows <= 240` and `retained_bytes <= WINDOW_BYTES + MAX_INPUT_BYTES`
while `window_total` reports the switched session's own size.

## Full workspace

`cargo test --workspace --locked` → exit 0, **317 passed, 0 failed, 3 ignored**:

| Suite | Passed |
| --- | --- |
| oc unit / application / configured_workspace / dcp_runtime / durability | 1 / 1 / 6 / 2 / 1 |
| oc mcp_application / pty / pty_t39 / responses | 7 / 13 / 3 / 4 |
| oc-adapters unit | 132 |
| oc-adapters blob_audit / dcp_atomic / e2e_offline / mcp_remote / mcp_stdio | 11 / 4 / 3 / 20 / 10 |
| oc-adapters patch_audit / runtime / shell_watchdog / soak / storage_lock | 10 / 31 / 2 / 4 / 1 |
| oc-core unit | 17 |
| oc-tui unit | 34 |

Ignored: exactly the three pre-existing external harnesses (`e2e_live`,
`mcp_remote` live, `mcp_stdio` live) — not run, not counted as passing.

## Static and structural

| Command | Result |
| --- | --- |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo build --locked` + `target/debug/oc --help` | exit 0 |
| `python3 scripts/progress.py check` | OK (journal structure only) |
| `python3 scripts/check_docs.py` | OK |
| `git diff --check` | clean |
| `rg "storage::Db\|oc_adapters::storage" crates/oc-tui/src` | no matches |
