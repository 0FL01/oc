# T44 V04 modal pointer and focus checkpoint

## Result

Base `0909f22182266755f132b93c316110b5327a6d04`, active T44. Mouse capture and restoration, modal-only pointer routing, geometry-aligned option hits, wheel scroll, hover and click, backdrop dismissal and Model→Variant focus replacement implemented. Raw xterm SGR PTY confirms actual selected model/variant on provider wire, retained draft, durable selection, and terminal restoration. Independent read-only review found that a DCP informational row triggered compression and a held press could activate a replacement Variant row; both corrected with unit regressions. Historical V04 attempts preserved in `modal-focus-report.md`.

## Checks

- Parent `cargo fmt --all && cargo test --locked -p oc-tui v04_mouse -- --nocapture && cargo test --locked -p oc --test pty_t39 v04_raw_ -- --nocapture && git diff --check`: exit 0, 2 unit tests + 3 raw PTY tests.
- First full `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet` exit 101: existing 10k catalog debug latency gate observed 2.111s > 2s on one iteration amid concurrent tests (other 113 TUI tests passed); threshold/expectation unchanged. Follow-up full `cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && cargo fmt --all -- --check && python3 scripts/check_docs.py && python3 scripts/progress.py check && git diff --check`: exit 0 at every step, all groups passed with 5 pre-existing ignores; docs 48 tasks/125 specs. Serial execution prevents competition for the fixed cold-search latency threshold, not suppression of the test.
- Previous paired executable frames at `evidence/tui/recovery-v04/followup-truecolor-{160x48,80x24,121x41}` remain unequal; no new parity capture or VIS gate is claimed for this change.

## Risks

Editor caret remains end-of-draft; in-draft grapheme editing/paste and full keymap are V05. Terminal mouse encodings beyond SGR and redirected-stderr interactive output not qualified. Missing genuine backend palette actions and exact full-frame parity remain open; VIS01–VIS24 NOT_RUN, R3/R4 unverified. No live provider qualification or product READY claim. `.opencode/` untracked and untouched.

## Next

V05 Unicode multiline editor, focus-priority bindings and bounded paste, tested through raw PTY with a single durable submission and cancellation guarantees. Continue V06–V09 separately with a factual checkpoint for each verified slice; unsupported service/OAuth commands remain undisplayed until an owner-scope decision.
