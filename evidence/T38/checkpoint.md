## Result

T38 (F12/AUD27–AUD28, F13/AUD25–AUD26) implemented and verified: shell supervisor
with concurrent stdin/output supervision, bounded group teardown, strict child
env allowlist, canonical cwd containment; webfetch with Unicode-safe HTML
extraction, `reqwest::Url` handling, dial-bound guarded resolver, single total
budget and hop-scoped auth. RED captured on `4574a2d` for all four audit items;
workspace gate green (308 passed / 0 failed).

## Checks

`cargo test --workspace --locked` exit 0 (308 passed / 0 failed, exactly the three
pre-existing external harnesses ignored); targeted: adapters unit 129,
`shell_watchdog` 2, `e2e_offline` 3, `soak` 4, `runtime` 31;
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --
--check`, `cargo build --locked`, `target/debug/oc --help`,
`python3 scripts/progress.py check`, `python3 scripts/check_docs.py`,
`git diff --check` — all exit 0. See `evidence/T38/checks.md`.

## Risks

- AUD27 cases are Linux-specific (session groups).
- One non-reproducible `reap failed` was observed before the spawn diagnostics
  change; three reruns and the full workspace run are green. Now reported with the
  OS error instead of a bare `Reap`.
- HTML scanner is intentionally not spec-complete; exotic markup degrades to text.

## Next

Commit T38 implementation and evidence, close the task, then start T39 (TUI
panels/switch/commands, F14/AUD29–AUD31) from `audit/repairs/T39.md`.
