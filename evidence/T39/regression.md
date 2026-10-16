# T39 initial regressions (RED)

Base HEAD `33ab80c` (T38 closeout). RED was captured in a detached worktree at
that commit with a standalone probe (`crates/oc/tests/t39_red.rs`), using an
isolated HOME, a scripted loopback Responses peer and temporary data roots.

## 1. Panels were never rendered by the real binary

`t39_red_panel_never_rendered`: the TUI started (`Idle` visible), then `/model`
was typed. The pre-repair renderer draws exactly two `Paragraph` widgets
(history + prompt); the panel state existed only in unit tests, and the binary
never called `handle_panel_key`, never loaded a catalog and never executed a
command intent.

Result: exit 101, `model panel must render after /model: ""` — no panel content
ever reached the terminal.

## 2. Tool cards were limited to the 200 oldest operations

`t39_red_tool_cards_are_oldest_only`: 260 operations were recorded, then the
reader was asked for the session's cards.

Result: exit 101,
`the newest operation must be reachable: 200 rows, last Some("tool-0199")`.
The old query was `ORDER BY rowid ASC LIMIT 200`, so the newest operations —
the ones a user actually needs — were unreachable no matter what the view did.

## 3. Backing state grew with the whole transcript

Source-level fact at `33ab80c`: `crates/oc/src/tui_cmd.rs` seeded the view with
`app.read_history(session)` (every committed message) into `TuiState::lines:
Vec<String>`, and session switches cleared the vector only by discarding it
without any byte/row bound. The view-model API also took `&Db` handles
(`open_picker(catalog, &db)`, `handle_panel_key(action, &db)`,
`resume_session(&db, limit)`), i.e. the UI owned a second storage path next to
the application worker. Both were replaced by the bounded `HistoryWindow` plus
application-owned page queries; `evidence/T39/checks.md` shows the bounded
metrics asserted by the PTY qualification.

## 4. Existing PTY behaviour had to survive

The T26 PTY suite (13 tests) asserts observable behaviour of the same loop:
`you:`/`ai:` lines, `turn busy` and `Cancelled` visibility, Up/Down scrolling
of persisted rows, tiny-screen rendering, Ctrl-C quit, slow-consumer drain and
panic-path restoration. Those tests were kept unchanged and are part of the T39
gate; the only renderer change they needed was the transient note in the title
(it was previously appended as a history line).
