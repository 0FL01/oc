# T44 real-profile startup credential checkpoint

## Result

Base `c994afd`. The exact current-profile configuration error was reproduced with one controlled difference: removing `LUDKA2_API_KEY` from the launch environment yields `ConfigError::MissingCredential` before discovery and a release TUI failure, while inheriting a nonempty value reaches Home. Product code now carries that typed cause to a static, redacted **Selected provider credential missing** screen; malformed configuration remains separately classified. No provider completion or MCP call was run, and no credential values were printed. The owner's interactive shell environment remains unobserved; a credential must be supplied by that shell for a working model session. No automatic `secrets.env` loading or validation bypass was introduced.

## Checks

- Parent: `cargo fmt --all -- --check && cargo test --locked -p oc --test recovery_startup -- --test-threads=1 && cargo build --locked --release -p oc && git diff --check`: exit 0, two targeted tests. Real-profile fresh release PTY at 120×40 with inherited/unset credential: Home/exit 0 vs missing-credential/exit 1; termios restored. With actual default data root and inherited credential: Home/exit 0. Both positive starts had no submitted prompt.
- Parent: `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0, five existing ignored. Test-first and fixture failures, independent review and privacy limits in `credential-report.md`.

## Risks

The actual owner's interactive zsh environment is not available to this API process: successful inheritance here does not prove their terminal exports the key. `secrets.env` could not be read under file policy; do not infer its content or execute it. No live paid model turn or full T44 product/VIS parity acceptance. Missing-key error takes precedence over another simultaneous malformed URL, without permitting startup.

## Next

The owner checks, without printing the value, whether `LUDKA2_API_KEY` is exported and nonempty **in the same terminal** as `target/release/oc`; if absent, export it from their trusted secret manager and retry. A new release error category, if any, is a distinct failure to investigate with the same typed boundary. T44 V07 remaining safety/resource and V08–V09 visual qualification remain open independently.
