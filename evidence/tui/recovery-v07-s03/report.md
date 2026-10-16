# T44 V07 S03 — raw PTY key modifiers and event kinds (2026-09-23)

Base HEAD: `19181bb` (active T44). The only pre-existing worktree entry was untracked `.opencode/`; it was not opened or changed. The test uses the actual `oc tui` binary, a fixture-owned PTY and a loopback fake Responses endpoint under isolated HOME/XDG; all prompts, model IDs and credentials are synthetic.

## Test first and observed behavior

- Added `v07_raw_modifiers_release_and_modal_interrupt_keep_exact_draft` to `crates/oc/tests/pty_t39.rs`, reusing the V04/V05 fake Responses/PTY fixture. `cargo test --locked -p oc --test pty_t39 v07_raw_modifiers_release_and_modal_interrupt_keep_exact_draft -- --nocapture`: **exit 0**, 1 passed / 0 failed / 0 ignored, before any production edits. No S03 runtime defect was reproduced at this HEAD: `events.rs` already uses `intersects(CONTROL | ALT)` for printable characters and filters `KeyEventKind::Release`. The `contains(CONTROL | ALT)` hypothesis in `SAFETY_REGRESSIONS.md` does not describe the current source.
- Raw Ctrl+P opens Commands without inserting `p`; CSI-u Alt+z and legacy ESC-prefix Alt+z do not insert `z`; release of Ctrl+P and printable `x` do not alter the draft. Raw CSI-u Shift+Enter plus Enter release leaves **0** provider requests until a genuine Enter. The single main Responses request contains exactly `v07-draft\nline-two`. A post-completion Enter release followed by an unsent editor draft leaves the main request count at **1**, and persisted history has exactly that one user prompt.
- Esc closes Commands without exiting or losing the editor draft. In Commands, first Ctrl+C clears a nonempty search, second Ctrl+C dismisses the dialog; the binary remains alive and the editor draft survives. Ctrl+C at the root editor exits normally, restores the terminal and does not submit the unsent draft. The wire/persisted prompt also detects unexpected inserted modifier/release characters, duplicate input and lost newline.

## Commands and checks (from repository root)

1. `cargo test --locked -p oc --test pty_t39 v07_raw_modifiers_release_and_modal_interrupt_keep_exact_draft -- --nocapture`: **exit 0**, 1 passed.
2. `cargo test --locked -p oc --test pty_t39`: **exit 0**, 19 passed, including existing V04/V05 cases.
3. `cargo test --locked -p oc-tui events::tests`: **exit 0**, 5 passed.
4. `cargo fmt --all -- --check`: **exit 1**, formatting of four assertions in the new test only. Applied the formatter's suggested layout; repeat: **exit 0**.
5. `cargo clippy --locked --workspace --all-targets -- -D warnings`: **exit 0**.
6. `cargo test --locked --workspace`: **exit 0**, zero failures, five pre-existing ignored tests (live/external fixtures); all 19 PTY T39 tests passed again.
7. `git diff --check && git diff -- crates/oc/tests/pty_t39.rs`: **exit 0**, reviewed only the focused test addition. `git status --short --branch && git rev-parse --short HEAD`: **exit 0**, HEAD still `19181bb`; test file modified and private untracked `.opencode/` unchanged (this new report is added after that status snapshot).

Observed gaps: no additional S03 production defect on this HEAD; this qualification exercises the listed encodings and focused Commands/editor contexts on this host, not every possible terminal key encoding or every informational panel. No staging, commit, progress/goal edit or live request.

Final report check: `git diff --check && git status --short --branch && git rev-parse --short HEAD` **exit 0**, HEAD `19181bb`, only `crates/oc/tests/pty_t39.rs`, this new report directory and the pre-existing untracked `.opencode/` appear in status. `python3 scripts/check_docs.py` **exit 0** (structure-only check).
