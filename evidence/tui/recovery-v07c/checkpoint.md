# T44 V07c trusted file-reference admission checkpoint

## Result

Base `ed954230ddce9ad0ac1149f184f333edd5d764f1`, active T44. A deterministic ancestor swap between admitted config read and `{file:}` substitution sent an outside fixture secret into provider config before the fix. Application substitutions now use the pinned admitted root fd; public trusted-source callers pin and verify their own source directory. Descriptor-relative nonblocking no-symlink opens enforce a 64 KiB UTF-8 regular-file budget. Explicit external global and legitimate nested regular references work. Isolated actual binary fake-provider and bare PTY show refused external symlink without sending requests or revealing the fixture value. Independent review found no concrete escape in the corrected path. No private `.opencode/` or authoring credential access.

## Checks

- Parent `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0 throughout; five existing ignored tests.
- Independent targeted review: two adapter tests, external-global binary and bare-PTY refusal tests plus diff check exit 0. Test-first failed ancestor-swap and corrected successful reruns, earlier formatting attempts and exact paths recorded in `report.md`.

## Risks

This verifies a deterministic ancestor replacement, not exhaustive race fuzzing. Unsupported `openat2` fails closed. Other S03–S08 raw PTY/security and post-render memory/process measurements remain open; all paired VIS/pixel gates unverified, real-profile startup cause unknown.

## Next

V07 negative qualification S03–S06/S08 using actual binary/owner effects (including injected terminal controls and disabled/required/working MCP), then RSS/PSS/CPU/frame-time measurements for equal active view and varied archive. V08–V09 full paired captures and qualification follow.
