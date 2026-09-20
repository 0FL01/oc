# T23 — DCP TUI и команды

Status: PASS (offline TestBackend + MockProvider + tempdir Db).

## UI04 DCP panel — PASS

- `oc-tui/dcp_panel.rs`: context/stats snapshot from runtime counters (estimated/max tokens, turns since compress, blocks, compressions, nudges, prunes — counts only, no transcript); manual `CompressRequest` with focus bounded to 256 bytes; outcomes (`Started`/`Done{saved}`/`Failed{reason}`) as transient notices capped at 120 bytes.
- `/dcp-compress [focus]` routed through the exact built-in table (now 6 entries) into `TuiPanel::Dcp` with the pending request recorded; focus becomes a bounded instruction the runtime executes (T24), never executed by the TUI; oversized focus refused visibly.
- Outcome notification is not duplicate history: `notify_dcp` sets only the transient notice (rendered in the history title), chat lines provably unchanged; the next submit clears the notice and chats normally (asserted end-to-end with echo provider + script driver).
- Nudge hints surface as transient panel rows, never messages; `set_snapshot` refreshes counts without duplicating runtime state.
- Bounded panel rows rendered in the panel pane; notice visible in the title on a 70×24 TestBackend frame.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- Workspace 157 total (oc 4 + adapters 87 + mcp_remote 15 + mcp_stdio 6 + core 16 + tui 29); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Compress execution, prompt assembly, and generation-bound turn loop arrive with the runtime (T24); T23 proves request/notice/stats mechanics.
- PTY/resize/terminal-restore qualification stays in T26.
