## Result

T42 offline qualification executed on the four compatibility fixes plus the mandatory
regressions. Working tree: bare `oc` launches the local TUI (and is an actionable error
without a terminal), `/location <path>` performs a real Location switch inside one running
application lifecycle (target generation built before publication, old MCP closed after,
sessions Location-bound, refused during a turn, generation caches dropped), `apply_patch`
tool cards carry a bounded diff summary, and config sources/`.opencode` roots/AGENTS files
must canonicalise inside their admitted root (symlink escape fails closed). New tests: 3
composition trust tests, 1 patch diff test, 3 actual-binary PTY tests.

## Checks

`cargo test --workspace --locked` -> 331 passed / 0 failed / 4 ignored (exit 0, log in
`evidence/T42/qual-test.log`). `cargo fmt --all -- --check`, `cargo clippy --locked
--workspace --all-targets -- -D warnings`, `cargo build --locked`, `oc --help`,
`git diff --check`, `progress.py check`, `check_docs.py` all exit 0. Clean HOME/XDG with a
Node/Bun-free PATH: `e2e_offline` 3/3 passed (log in `evidence/T42/qual-cleanhome.log`).
RED captured before the fixes: `evidence/T42/red-config-trust.txt` (0/2),
`evidence/T42/red-pty.txt` (0/3).

## Risks

- The Location switch is explicitly refused while a turn streams (nothing lost); a queued
  switch is out of scope.
- Live provider/MCP behaviour is not covered here: T27 remains the mandatory live gate.
- 4 ignored tests (3 pre-existing external harnesses + credential-gated live campaign);
  none counted as passing.

## Next

1. Commit the fixes, close T42 with `evidence/T42/report.md` (AUD38–AUD40 mapping).
2. Start T27: bounded live campaign on the production `oc` path (Responses + mandatory
   `codex_web` MCP) with the owner's config; never optional.
3. Then T30 FINAL over A01–A13; READY only after T27 passes.
