# T44 V07a discovered-source admission checkpoint

## Result

Base `da711b4cb543a932983c46ba83f965d2964dad56`, active T44. Discovered `.opencode` root is checked against the canonical Location before any config/definition/instruction contents are read. Root descriptors pin admitted config and definition opens; regular-file and byte limits reject FIFO, device and oversized sources without blocking. External nested skills/agents/commands and an atomic symlink-swap were reproduced against the previous path-based loader, then excluded; explicit external global config and in-root symlinks still work. Isolated actual binary fake-provider effects and bare PTY preflight were exercised. Independent final review found no reproducible S02 blocker. The private untracked `.opencode/` was not read or staged.

## Checks

- Parent `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0 throughout; five existing ignored tests.
- Final independent review: `cargo test --locked -p oc-adapters v07a_ -- --test-threads=1` (7 passed), `cargo test --locked -p oc --test configured_workspace v07a_ -- --test-threads=1` (6 passed), `cargo test --locked -p oc --test pty v07a_ -- --test-threads=1` (1 passed), `git diff --check` all exit 0. Original failures, first full-suite PTY flake, and subsequent passing reruns remain in `report.md`.

## Risks

Concurrent ancestor mutation of the separate `{file:}` substitution reader is not qualified. Actual device nodes and kernels without `openat2` are not qualified; unsupported syscall fails closed. V07b in-flight MCP tools/call cancellation and V07c S03–S08/resource measurements remain open. No paired visual/parity/VIS gate is closed. Real-profile release startup root cause remains unknown; no private config access was attempted.

## Next

V07b: reproduce remote and stdio in-flight `tools/call` under actual application/PTY cancellation; ensure no overlapping side effects or orphan owner and report cleanup errors. Then V07c exercise remaining negative checks and measure post-Markdown RSS/PSS/CPU/frame-time against equal active viewport and varied archive.
