# T44 V05 editor and paste checkpoint

## Result

Base `f9a93b62e9be4ec1242094dced473dc5985a8406`, active T44. Grapheme-aware multiline editor, movable caret/selection, undo/history, focus-priority keymap, responsive pending draft revision, bounded one-event paste and an editor-only compact paste-chip projection over the actual application-owned prompt. Original `a\nb\nc` appears as `[Pasted ~3 lines]`; actual PTY submits the real text exactly once, not the marker. Independent read-only reviews identified and corrected five editor/cancellation/navigation defects and three chip/grapheme/history/trim defects; all reproductions and failures are preserved additively in `evidence/tui/recovery-v05/report.md`. No production JS runtime.

## Checks

- Parent final `cargo fmt --all -- --check && cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1 && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo build --locked && target/debug/oc --help && python3 scripts/check_docs.py && python3 scripts/progress.py check && python3 -m unittest discover -s scripts -p 'test_*.py' && git diff --check`: exit 0 at every step, all workspace groups green, five existing ignored tests, Python 29 passed. Agent focused V05 15 editor/unit cases, 8 raw PTY cases, stalled-MCP pending test all exit 0. Initial post-chip workspace failure/fixture correction retained in report, no threshold/test disabled.
- Parent final-code paired capture `paired-paste-chip-postreview` at 160x48 with 80x24–160x48 resize matrix: runner exit 1 DIFFERENT, both actual executables/protocol fixtures executed. Both multiline rows 43 contain `[Pasted ~3 lines]`; explicit unmasked comparator exits 1 for grid (7,293 differing cells of 7,680) and PNG (7,823 differing pixels of 1,036,032), cursor matches. Earlier `paired-editor-matrix`, `paired-paste-chip`, `paired-paste-chip-final` and `paired-paste-chip-utf16` remain immutable; all capture commands/exits and fixture/profile/source manifests in attempts.

## Risks

V05 raw PTY behavior and local text observation do not verify VIS11–VIS12, R3/R4, or whole-frame pixel parity. Full dialog actions and V06–V09 remain. User reports `target/release/oc` shows generic startup-error screen and exits; root cause unproved. Read-only investigation traced a discarded `spawn` error to the generic preflight renderer; no user `.opencode/`, real config or credentials were inspected. No product READY/live claim; five existing ignores remain, other ignored/failed capture attempts retained.

## Next

Reproduce release startup with isolated HOME/XDG/config/data and PTY, compare debug/release, preserve safe actionable stage/category without printing raw error data, and exercise success/failure through actual binary. Then V06 Markdown/tool/reasoning/diff live/replay, V07 safety, V08–V09 exact paired VIS and full qualification; checkpoint each verified slice separately.
