# T42 checks — exact commands, exit codes, counts

Working tree SHA at the start of the qualification run: `451cbdbc6237fd110ec2620962fff19ad3c50a7f` (the fixes of this task
were uncommitted; the committed SHA is recorded in the task journal and in the follow-up
commit). All commands ran from `/home/opencode/ai/oc`.

| # | Command | Exit | Observed |
|---|---|---|---|
| 1 | `cargo fmt --all -- --check` | 0 | no diff |
| 2 | `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0 | no warnings |
| 3 | `cargo test --workspace --locked` (log: `qual-test.log`) | 0 | 331 passed / 0 failed / 4 ignored |
| 4 | `cargo build --locked` | 0 | `Finished dev profile` |
| 5 | `target/debug/oc --help` | 0 | usage printed |
| 6 | `env PATH=<tmp>/bin:~/.cargo/bin <e2e_offline binary> --test-threads=1` (log: `qual-cleanhome.log`) | 0 | 3 passed / 0 failed, `node`/`bun` absent from that PATH |
| 7 | `env -i HOME=<tmp> XDG_CONFIG_HOME=<tmp>/config XDG_DATA_HOME=<tmp>/data PATH=/usr/bin:/bin target/debug/oc --smoke` | 0 | `oc smoke core=oc-core adapter=oc-adapters tui=oc-tui` |
| 8 | `git grep -nE '(sk-[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|BEGIN [A-Z ]*PRIVATE KEY)'` | 1 | no matches (no secret material) |
| 9 | `git diff --stat Cargo.toml Cargo.lock crates/*/Cargo.toml` | 0 | empty (no dependency change) |
| 10 | `python3 scripts/progress.py check` | 0 | journal structure OK |
| 11 | `python3 scripts/check_docs.py` | 0 | docs/registry/journal structure OK |
| 12 | `git diff --check` | 0 | clean |

Targeted runs during the task (each also covered by row 3):

| Command | Exit | Observed |
|---|---|---|
| `cargo test -p oc-adapters --lib composition::tests::symlinked` (pre-fix worktree) | 101 | 0 passed / 2 failed → `red-config-trust.txt` |
| `cargo test -p oc --test pty_t42` (pre-fix worktree) | 101 | 0 passed / 3 failed → `red-pty.txt` |
| `cargo test -p oc-adapters --lib patch::tests::diff_summary` | 0 | 1 passed |
| `cargo test -p oc --test pty_t42` (after the fix) | 0 | 3 passed |
| `cargo test -p oc-tui` | 0 | 35 passed |

Versions: rustc 1.93.0 (254b59607 2026-01-19), cargo 1.93.0 (083ac5135 2025-12-15).
