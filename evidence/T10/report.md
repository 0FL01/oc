# T10 — Shell supervisor

Status: PASS. Implementation commit: `7b31ea8f31d43fd83469d6e0203c86a5e67dbdf7`. Method: offline `cargo` unit execution spawning real child processes locally; no network, no live requests, no Docker, no persistent shells.

## TOOL05 Shell output/environment — PASS

- New `oc-adapters/shell.rs`: one-shot `Shell::execute` (argv, no shell joining) plus single bounded `run_sh` with quote-aware shape validation (pipes/chains/backticks/`$()`/background refused; `>&2` fd redirection and `$(( ))` arithmetic allowed; quoted operators are data).
- Fresh process group per call (`setsid` pre-exec); cwd pinned inside the trusted root; minimal child env (`PATH`, `LANG` base + scrubbed extras — 14-case drop list incl. `*_key`/`secret`/`token`/`ludka`/`openai`/`mcp_`), proven by `env` child output containing `MYAPP_OK=1` and no `parent-secret`.
- Concurrent stdout/stderr drain threads into 1 MiB bounded buffers (overflow flagged + drained, never deadlocking the child); 3 MiB `head -c` output truncates at exactly the cap with exit code preserved; exit 42 propagates.

## TOOL06 Shell cancel — PASS

- Deadline → `TERM` to the group → grace → `KILL` → verified reap: plain `sleep 30`, TERM-trapping `trap '' TERM` child, and subshell grandchild `(sleep 30)` all terminate inside the bound with `timed_out=true`, `code=None` (Unknown, never success).
- Explicit `AtomicBool` cancel during `sleep 30` reports `cancelled=true`, `timed_out=false` (Cancelled vs Unknown distinguished); cwd/argv guards (`..`, empty argv) refused pre-spawn.

## Checks

- `cargo fmt --check` exit 0; `clippy --workspace --all-targets -- -D warnings` exit 0 (documented `multiple_unsafe_ops_per_block` allow on the coupled pre-exec setup).
- `cargo test -p oc-adapters shell` 6/6; workspace 65 total (oc 4 + adapters 41 + core 13 + tui 7); `cargo build --locked`, `check_docs.py` exit 0. No new dependencies (`libc` already pinned).

## Scope and limitations

- Not a persistent shell manager (fresh process per call by design) and no sandbox isolation claim (shared fs/network/uid) — both documented in-module.
- Registry wiring into the model `bash` tool arrives with the tool executor; T10 proves supervision semantics.
