# T44 V07e S04 raw terminal-control checkpoint

## Result

Base `9ad9533`, active T44. Added one actual bare-PTY test with fake Responses and local stdio MCP to check that untrusted model/reasoning/tool/result controls (CSI erase, OSC52, OSC8, BEL, CR) are inert in raw VT, decoded live grid and replay after restart. It verifies durable model text, normal Unicode/newlines, tool calls and footer. Existing production rendering passed this exercised S04 path; no production behavior changed. Detailed byte observations, failed fixture attempts and remaining coverage are in `report.md`.

## Checks

- Parent `cargo fmt --all -- --check && cargo test --locked -p oc --test mcp_application v07e_bare_pty_model_and_stdio_tool_controls_are_inert_across_replay -- --test-threads=1 && git diff --check`: exit 0; one raw-PTY test passed.
- Implementing agent's final `cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && git diff --check`: exit 0, five existing ignored tests. Stronger last test assertions were checked by the parent's focused run, not a fresh full-suite run.

## Risks

This is not an exhaustive terminal injection or parity qualification. S03/S05/S06/S08 and measured S07 remain open. The owner's real `target/release/oc` configuration/runtime initialization failure has **not** been reproduced or explained: all prior startup checks used isolated synthetic configuration, so they did not test the failing profile. The owner has now explicitly authorized reading `~/.config/opencode`, but no such inspection was performed during this slice. All VIS and product-ready gates remain open.

## Next

Pause T44 slice work after delivery as requested. Next investigation must prioritize the owner's actual release startup failure with the newly authorized config scope, identify the failure stage from typed application errors without printing secrets, reproduce under the affected profile, and fix its cause. Do not present isolated-fixture startup tests as proof that the owner profile works.
