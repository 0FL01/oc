# T44 isolated release-startup diagnostic checkpoint

## Result

Base `804268753f82ca06b5312ff142ec0fdcadde1e69`. The freshly rebuilt `target/release/oc` launches and exits normally with an isolated valid product config; startup failures now expose a static typed stage/category and actionable remedy rather than the generic preflight message or raw underlying error. An independent review reproduced a `/location` raw-error leak and a corrupted saved-selection misclassification; both were fixed and requalified with actual release PTYs. Non-fatal warnings identify allowlisted source categories. The real user's profile was neither inspected nor executed; its particular startup failure remains unknown.

## Checks

- Parent `cargo test --locked -p oc --test recovery_startup -- --nocapture` exit 0, two tests; `cargo test --locked -p oc --test recovery_v03 -- --nocapture` exit 0.
- Parent `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked --release -p oc && env -i PATH=/usr/bin:/bin python3 crates/oc/tests/support/startup.py target/release/oc && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0 at every step, five existing ignored tests. The isolated release matrix checks success, lock/reacquire, query, corrupt saved selection, failed/successfully retried Location switches, malformed/missing/CLI config, warning redaction and unsafe/unavailable data roots, with terminal restoration and no provider traffic. Earlier failing attempts and their fixes remain in `report.md`.

## Risks

The owner's actual release startup failure category is not established: their private `.opencode/`, configuration and credentials were deliberately not accessed. This isolated slice does not verify R3/R4, VIS01–VIS24, remaining command/service capabilities, V06–V09, or product READY. The historical paired frame comparators remain unequal; five existing live ignores remain.

## Next

V06: implement/render and directly compare Markdown tables, reasoning, tool cards/diff/footer with actual live and durable replay; verify with raw PTY/provider/tool effects and fresh paired frames. Subsequently V07 security and V08–V09 full capture/qualification. A user-executed rebuilt release binary can reveal only a safe category of their own profile failure; do not request their secrets.
