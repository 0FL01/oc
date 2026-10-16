# T44 S03 focused PTY qualification and owner pause

## Result

At base `19181bb`, a new actual-binary PTY test exercised raw Ctrl+P, Alt encodings, Shift+Enter, key releases, Esc/Ctrl+C focus and the real fake-Responses request/durable prompt. No production defect reproduced on this HEAD; modifier intersects/release filtering was already present. This closes only the listed S03 cases in the tested host/terminal profile, not V07 as a whole or any VIS gate. Owner explicitly requested a T44 pause and priority RECON for their `oc` startup; factual startup state and a discriminating plan are in `evidence/tui/recovery-startup/owner-catalog-recon.md`.

## Checks

Implementing agent: focused PTY 1 passed, T39 19 passed, TUI events 5 passed, workspace tests zero failures/five pre-existing ignores, clippy/docs/diff exit 0; initial fmt check exit 1 on new test layout, corrected repeat exit 0 (`report.md`). Parent reran `cargo test --locked -p oc --test pty_t39 v07_raw_modifiers_release_and_modal_interrupt_keep_exact_draft -- --test-threads=1 && cargo fmt --all -- --check && git diff --check`: exit 0 (1 passed). This checkpoint does not claim a new live catalog smoke or a real owner-profile launch.

## Risks

Exact owner shell/catalog status 401 versus 403 and effective identity remain unknown. Independent S05/S06/S08, resource S07, VIS01–VIS24 and missing backend capabilities remain open. Do not treat the 200 result from a different product environment as proof that the owner's binary starts, or Rust-only/golden checks as pixel parity.

## Next

Keep T44 active but paused at the owner's instruction, with no new implementation slices until resumed. Compare same-shell status-only catalog GET with freshly built release's typed category, without exporting, logging or copying credentials outside the user's process. Apply the branch-specific plan in `owner-catalog-recon.md`; then continue remaining V07 and V08–V09 only when pause is lifted.
