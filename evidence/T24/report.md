# T24 — Интеграция lifecycle

Status: PASS (offline: scripted Responses SSE fake + tempdir Db/roots).

## STORE05 Cancel/shutdown — PASS

- `oc-adapters/runtime.rs::Runtime::run_turn`: single-flight guard mirrors the single-turn worker; cancel observed between rounds and inside streaming/executor; cancel drains to durable records (`finish_turn` cancelled via `TurnLog::save`, user message kept, no partial assistant message asserted).
- MCP attach lives inside the guard; `McpAttachment::close` shuts down stdio children (reap ladder) on every path including cancel/failure; remote clients drop-close. Second turn after cancel runs cleanly (no retained per-turn state asserted).

## TOOL09 Permissions — PASS

- One path: `RuntimePolicy` bridges the config `Permission` map onto `ToolPolicy::check`, which the executor calls for every builtin; MCP units are checked against the same policy object before any client call; `run_compress` checks the `compress` name first.
- Unlisted tools denied by default (invalid/untrusted config never allows); `Ask` fails everywhere with the tool named (no approval channel in T24, headless included — asserted for both `Deny` and `Ask`).
- Patch legacy deny wins: `guard_patch` runs `dcp::check_patch_protected` over recorded glob patterns before the executor; a protected patch becomes a visible failure and the file is provably absent from disk.

## DCP08 Recovery — PASS

- Generation guard: `reload`/`reload_dcp` refuse with `TurnActive` while a turn runs (asserted with a genuinely concurrent slow stream); durable commit (`commit_turn`) re-checks the publication id and, on mismatch, records `interrupted` without assistant message or turn log — stale generations never half-apply.
- `run_compress`: validates args before any write, snapshots block ids, compensates (deletes) blocks stored before a mid-apply failure, rolls back the whole attempt on supersede; stable `bNNNN` ids across calls asserted; invalid args store nothing.
- Restart continuation uses durable ids (`read_history_full`), `load_compression_blocks`/`load_prune_mark`; reasoning/opaque items stay in `TurnLog` for replay while reports carry only text + capped outputs (reasoning cleanup).

## Mechanics

- One prompt assembler (`assemble_turn_input`): history + user + prior round carryover, capped at 64 KiB by dropping oldest history first (user text always survives; asserted). Token estimates are a documented bytes/4 heuristic.
- Exact model/variant selection + `admit` before any side effect; no fallback. Command expansion is one bounded literal pass (`$ARGUMENTS`, `$1..$9`); the original invocation is stored as the user message, the expansion runs as the turn prompt (asserted).
- Multi-round tool loop (cap 16): stream → assemble → partition builtins (executor) vs namespaced MCP (attached clients) vs failures; every unit gets intent + outcome rows; prior outputs feed back as bounded text (documented carryover — the provider layer streams one text prompt per round).
- Location-scoped sessions via `tui.session_location.*` prefs: cross-Location open fails with owner named; unknown sessions fail.
- MCP attach fails fast (`McpAttach` naming the server) before `begin_turn`: no silent tool absence, no turn begun (asserted empty history).

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0.
- Workspace 169 total (oc 4 + adapters 87 + mcp_remote 15 + mcp_stdio 6 + runtime 12 + core 16 + tui 29); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies.

## Scope and limitations

- Skill snapshot is empty until workspace definitions load (T25/A13); workspace-defined command bodies arrive with T25 (expansion primitive proven here).
- `Ask` has no interactive approval channel yet: it denies with the tool named. Live provider/MCP shapes qualify in T27; T16 unblocks on the T24/T25 harness path per its block note.
