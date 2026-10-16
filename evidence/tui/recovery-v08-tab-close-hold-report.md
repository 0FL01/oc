# T44 — retained pointer hover after tab close (paired diagnostic)

## Recon and change

The pinned v2.0.12 `packages/tui/src/component/session-tabs.tsx:237-274,1446-1467`
temporarily holds surviving tab geometry after a mouse close so the close glyph
remains under the pointer. In `recovery-v08-tab-close-final`, native dropped the
pointer on view replacement, shrank the returned tab and moved the add control.

`crates/oc-tui/src/{app,layout,shell}.rs` now records only actual SGR mouse
coordinates, revalidates the successful close against the old/new tab lists,
and holds the surviving tab's painted/hit-tested rectangle for at most five
seconds. The close glyph is bright only when the pointer is on that cell. Mouse
activation restores hover; keyboard activation does not. A modal, resize,
pointer leaving the strip, wheel event outside the strip or another mouse down
invalidates the hold. `crates/oc/src/tui_cmd.rs` applies the hold only after a
successful close, retaining existing busy, session durability and tab bounds.

## Independent reference/native captures

The same pinned v2.0.12 executable and public fake Responses fixture were run
separately at 120x40 using:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --tab-click true --tab-close true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-tab-close-hold-qualified
```

Immutable attempts `recovery-v08-tab-close-hold-01` (before final close-glyph
color fix), `-final` (before wheel-path fix) and `-qualified` (final source) preserve locks,
actual mouse inputs, full styled grids, PNG and VT. The latter locks original
source `2670273ff17da96f85c5826ced57aa1b368754fa`, binary SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
native binary `d2596b804627a96ac924ac48268f5c117f2e78a1e53f632d36a97c48fa02610d`,
fixture SHA-256 `d18132f88c4a7a638a244b0ea92007163246fc3ce0bbe5bcb2b90df02a67bb39`
and terminal profile `e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`.
Both sides report `provider_contract=true`, `TAB_INTERACTION_CHECKS_PASS`,
`TAB_CLOSE_CHECKS_PASS` and three provider completions with no additional
requests on close. The runner exited 1 because **full frames differ**.

`scripts/tui_capture/check_region.py` on the qualified `tab-close-closed-after`
styled grids reports **0/70 differences** for x0-69,y0 versus 1/70 in the
first hold attempt and the earlier narrower-tab mismatch. Full closed frame
still differs **201/4800 styled cells and 4579/647040 PNG pixels**; actual
elapsed time, location, version and other pixels are not frozen or masked.
This is an exact local region comparison, not VIS01-24 or T44 PASS.

## Checks and limits

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
--no-fail-fast --quiet` PASS, 0 failures (204 TUI tests, existing opt-in live
ignores); `cargo fmt --all -- --check`, workspace all-target Clippy `-D warnings`,
`cargo build --locked`, docs/progress checks and `git diff --check` PASS.
The first narrow review found nonmodal wheel input bypassed pointer updates;
the wheel path now updates the pointer without losing transcript scrolling, and
the targeted TUI/bin/PTY tests were rerun and the qualified immutable capture
locks the post-fix source and binary. Earlier captures remain unmodified.

Next: persist validated, Location-scoped tab order and selected route across
restart without creating a session on Home; then paired-qualify wider and
narrower tab interactions. VIS01-24, V08-V09 and remaining S07 are open.
