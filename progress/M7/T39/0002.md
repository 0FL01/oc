## Result
T39 (F14/AUD29-AUD31) implemented and verified: oc-tui is a storage-free bounded view-model driven by typed intents, the application owner serves catalog/skills/paged history/tool cards/DCP snapshots and applies model/variant/agent selection, and the binary runs the real loop with panels, paste/resize handling, compress turns and terminal recovery.

## Checks
cargo test --workspace --locked exit 0 (317 passed / 0 failed; only the 3 pre-existing external harnesses ignored); targeted pty_t39 3, pty 13, oc-tui 34, mcp_application 7; clippy -D warnings, fmt --check, build, oc --help, progress.py check, check_docs.py, git diff --check all exit 0.

## Risks
PTY qualification is Linux-specific (openpty/termios/flock). The metrics probe is opt-in and inert without OC_TUI_TEST_METRICS. Cards paging is asserted for the newest page; deep paging is covered by storage/query tests. VariantEntry surfaces only reasoningEffort.

## Next
Commit T39 implementation and evidence, close the task, start T40 (archive/queue/lifetime bounds).
