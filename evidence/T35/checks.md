# T35 executed checks

All application regressions used isolated temporary roots and loopback fakes.
No live provider/MCP, paid call, package resolver or real credential was used.
Initial failures: [regression.md](regression.md).

## Targeted integrated checks

- `cargo fmt --all && cargo test --locked -p oc --test configured_workspace -- --nocapture`
  exit 0: **6 passed**, .36 s after implementation (later rerun .38 s).
- `cargo test --locked -p oc-adapters --lib`: **117 passed**, 2.10 s.
- `cargo test --locked -p oc-adapters --test runtime --test e2e_offline --test soak`:
  runtime 19, offline E2E 3, soak 4 passed; command expansion initially exposed
  missing `$9` preservation, then passed after a single-pass literal fix.
- Discovery RED was 5 failed/3 passed. Final `aud18` set is 9 passed; adapter
  all-target check in delegated workstream was 185 executed pass/3 external ignored.

## Final gate after last production edit

```sh
cargo fmt --all && cargo test --workspace --locked && \
cargo clippy --locked --workspace --all-targets -- -D warnings && \
cargo fmt --all -- --check && cargo build --locked && \
target/debug/oc --help && python3 scripts/check_docs.py && git diff --check
```

Exit **0**. Workspace compile 12.10 s. Bounded suite summary from captured output:

| Suite | Passed | Time |
|---|---:|---:|
| oc unit | 1 | .06 s |
| actual binary application | 1 | .12 s |
| actual configured workspace | 6 | .33 s |
| actual SIGKILL durability | 1 | .23 s |
| PTY/application resume | 13 | 8.02 s |
| actual Responses | 4 | .36 s |
| adapters unit | 117 | 2.08 s |
| blob audit | 11 | .27 s |
| offline E2E | 3 | .57 s |
| remote MCP | 15 | .41 s |
| stdio MCP | 6 | 1.32 s |
| patch audit | 10 | .03 s |
| runtime | 19 | 2.97 s |
| soak | 4 | 25.21 s |
| storage ownership | 1 | .03 s |
| core | 16 | .02 s |
| TUI | 31 | .26 s |

All executed tests passed. Exactly the three existing external harnesses remain
ignored/NOT RUN: live workflow, live search, real stdio server. No T35 test is
ignored. Clippy 3.77 s, fmt, locked build 2.85 s, help, docs structure and diff
check all passed. Cargo manifests/lock/schema unchanged.

Final review covered config/definition trust and precedence, runtime fixed lanes,
command expansion, skill snapshots/errors, application diagnostics, plugin policy,
and all discovery request/oracle changes. Secrets are absent from diff/evidence.
Broad tests do not close T36+ findings.
