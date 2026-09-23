# T44 V06b reasoning, recorded tools and owner-output checkpoint

## Result

Base `efa4cbb1ce896d272d613f85902f84f3dbc01aa2`, active T44. Public reasoning can be expanded/collapsed without exposing opaque continuation; failed partial patch cards show only effects confirmed by the durable operation result, with unknown/truncated outcomes distinguished from success. An existing SQLite operation owns UTF-8-safe, session-scoped result continuation; `/cards` renders bounded pages, actual dialog-cell wrapping and scroll-before-advance, and clears cached previews on session/Location switch. Actual provider/PTy/restart tests exercise recorded operation IDs, real read/patch effects, result continuation past the preview, and no cross-session or cross-Location access. Independent review reproduced four card-cache/navigation/output-boundary defects and the corrections passed targeted re-review. No second transcript or permission expansion.

## Checks

- Parent `cargo fmt --all -- --check && cargo test --locked -p oc-tui v06b_ -- --test-threads=1 && cargo test --locked -p oc-adapters v06b_ -- --test-threads=1 && cargo test --locked -p oc --test recovery_v02 -- --test-threads=1 && git diff --check`: exit 0 (5 TUI tests, 2 adapter unit tests, 1 real-provider binary restart test).
- Parent `cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && python3 -m unittest discover -s scripts -p 'test_*.py' && git diff --check`: exit 0 throughout; five existing ignored tests; Python 29 passed.
- Actual pinned original and Rust executable 120x40 reasoning/read-tool captures in `paired-120x40-{reasoning,read-tool}-detail-ui/` each executed provider contracts; capture runners exited 1 on genuine whole-frame differences (reasoning 4500/4800, read-tool 4627/4800 styled cells). Earlier failed fixture/capture attempts and the direct patch-engine partial-execution test are preserved in `report.md`. This is not a parity pass.

## Risks

No paired original partial-execution patch, unknown outcome, expanded reasoning pointer interaction, cross-restart rendered screenshot or actual parent/child subagent frame yet. Upstream read grouping (`→ Explored — 1 read`) differs from native card text and reasoning elapsed duration is unfreezable with current fixture; original/Rust full frames unequal. VIS15–VIS17, R3/R4, and all VIS/product gates remain unverified. Real-profile release-startup cause remains unknown; private configuration not accessed. Archive cache RSS/PSS has not been measured since V06a.

## Next

V07: move discovered `.opencode` admission ahead of all reads and exercise full SAFETY_REGRESSIONS against real PTY/application effects, including in-flight MCP tools/call cancellation and memory/process measurement. Then V08–V09 paired captures and independent full-parity/qualification, including remaining V06b original/live/restart scenarios and honest capability gaps.
