# T06 — Минимальный Rust TUI

Status: PASS. Implementation commit: `a779f07981730af4163369dc7c7a230bf1a40a56`. Method: offline `cargo` unit (TestBackend, no PTY device) + manual `oc --help`/`tui --help`/no-TTY probe; no network, no live requests, no Docker.

## Work — PASS

- Same handle: `oc-tui` depends only on `oc-core` (`CoreApp`/`MockProvider`/`SessionId`); persistence stays in the `oc` binary (`Db`), preserving `oc-tui -> oc-core`, `oc-adapters -> oc-core`, `oc -> all` directions. New `oc` deps `ratatui`/`crossterm` are UI-only in the binary.
- `oc-tui/src/events.rs`: `KeyAction` (Char/Backspace/Enter/Cancel/Up/Down/Quit); `Esc` cancels, `Ctrl-C/D` quits, `/quit` at idle quits.
- `oc-tui/src/app.rs`: `TuiState` (bounded `input 4KiB`, `lines`, `scroll`, `Idle/Streaming/Cancelled/Quit`) over a bound session; `ScriptDriver` holds one broadcast subscription like the real loop and pumps deltas/finish/interrupt into view lines.
- `oc-tui/src/views.rs`: bounded render (`VIEWPORT_LINES 20` + 3-line prompt pane) on `TestBackend`; Unicode assertions.
- `oc/src/tui_cmd.rs`: real terminal `oc tui [--session]` with alternate-screen + raw-mode `TermGuard` (always restored on drop/error), 50 ms key poll vs worker-event drain, Db seeding of viewport on resume, user persisted on submit, assistant on finish, no-TTX guard (`use oc run headless`).
- CLI: `tui` subcommand wired in `cli.rs`/`bootstrap.rs`; bare `oc` without subcommand/`--smoke` stays usage exit 2.

## Checks

- `cargo fmt --all -- --check` — exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — exit 0 (fixed `collapsible_if` in app/views/tui_cmd, removed unused import).
- `cargo test --workspace --locked` — exit 0, 36 total (oc 4 + adapters 12 + core 13 + tui 7: prompt/stream/exit, cancel-reuse, viewport/scroll, render unicode, keymap).
- `cargo build --locked`, `oc --help` (now lists `tui`), `oc tui --help` — exit 0. Non-TTY `oc tui` prints the headless hint (exit-code check masked by pipe in manual probe; unit exit-code discipline already covered by T05 UI05).
- `python3 scripts/check_docs.py` — exit 0.

## Scope and limitations

- TestBackend only; PTY paste/resize/Unicode-at-TTY, slow-consumer paging at terminal speed, and panic-path restoration proof remain T26 per `roadmap/M5.md`.
- `MockProvider::echo` in the binary loop; real OpenProxy streaming arrives M3. TUI model/picker/sessions UI arrives M5 (T22–T24).
