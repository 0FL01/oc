# T05 — Headless CLI

Status: PASS. Implementation commit: `24353634e06df5dd3fbcf1985a1d1b3b013f9971`. Method: offline `cargo` unit + manual binary runs with temp data-dirs; no network, no live requests, no Docker.

## STORE01 Vertical session — PASS

- Same runtime local/headless: both use `CoreApp` + `MockProvider::echo` + `Db`. `store01_persist_resume_across_restart` runs `run_once_to_writers("hello", s-1)` then `("again", s-1)` in the same data-dir, drops/reopens between runs: history is 4 messages (`user hello`, `assistant echo: hello`, `user again`, `assistant echo: again`). Manual binary confirms: `oc --data-dir ABS run hello --session s-man --json` → deltas + done, `sessions list` → `s-man`, second `run again` → `echo: again`.
- User persisted on accept (survives interrupts), assistant only on `TurnFinished`; `begin_turn`/message+event transactions from T04 reused.

## UI05 Headless JSON — PASS

- `ui05_ndjson_stdout_only_and_slow_consumer`: `--json` stdout lines all parse as JSON with `type` (`delta`/`done`); stderr carries `session …`/`done …` diagnostics only; slow `Write` (5 ms/write) completes without deadlock (broadcast lag tolerated, incremental flush per delta).
- `ui05_interrupted_exit_is_nonsuccess`: cancel probe (50-token fixed stream, cancel after first delta) exits 130 with `interrupted; user input preserved` on stderr; reopened history has only the user message, no partial assistant.
- `sessions_list_stdout_ids_only`: stdout carries ids only, stderr empty on success. Bare `oc` without subcommand/`--smoke` is usage exit 2; `--help`/`--smoke` preserved.

## Storage follow-up in slice

- `--data-dir` auto-secures existing `0755` dirs to `0700` (owner-checked first) instead of refusing normal UX; foreign-owner/symlink/system paths still refused. Manual `stat` confirms `700 1003`. T04 quota test updated to assert securing, not refusal.

## Checks

- `cargo fmt --all -- --check` — exit 0.
- `cargo clippy --workspace --all-targets -- -D warnings` — exit 0 (cancel probe `#[cfg(test)]`).
- `cargo test --workspace --locked` — exit 0, 31 total (oc 4 + adapters 12 + core 13 + tui 2).
- `cargo build --locked`, `oc --help` — exit 0. Manual `run --json`/`sessions list`/`run again` — exit 0 with NDJSON/diagnostics split verified.
- `python3 scripts/check_docs.py` — exit 0.

## Scope and limitations

- Single-turn-per-process headless; `Ctrl-C` signal wiring and TUI remain T06. Mock echo only; real provider arrives M3.
- Broadcast `Lagged` on very slow consumers is skipped (coalesced), not replayed; full backpressure policy belongs to soak.
