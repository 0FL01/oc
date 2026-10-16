# T44 — hovered tab close: real interaction and paired diagnostic

## Source and implementation

Pinned v2.0.12 `packages/tui/src/component/session-tabs.tsx:1564-1567,1707-1733`
shortens the hovered title, paints a close glyph one cell from the right edge,
and closes on a hovered release (not on press). `context/session-tabs.tsx:322-343,
398-412` removes the tab from the open deck without deleting its durable session.
The previous native strip did not show or implement close.

`crates/oc-tui/src/{app,layout,shell}.rs` now shares the painted close cell with
hit-testing; the glyph appears only after a real mouse move over an eligible
tab. An unmodified left press/release on that exact cell sends `CloseTab`.
Dragging, right-clicking, clicking a blank/overflow cell or closing a dialog
does not trigger it. Busy turns have no close control: this is deliberately
stricter than upstream's background-turn navigation, which this single-turn
native UI cannot safely retain. At rest and at narrow widths the glyph is
absent. `crates/oc/src/tui_cmd.rs` removes only the process-local tab view and
its matching card cursor; surviving drafts/history remain. A sole real tab
falls back to parked Home (preserving its draft), or queries a sessionless Home
before changing the deck. A closed durable session is still reopenable via
`/sessions`, without inserting another root. The real-binary PTY+SQLite test
exercises pointer move, click, Home fallback, and reopening.

## Paired original / Rust captures

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --tab-click true --tab-close true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-tab-close-final
```

Three immutable attempts are retained: `-01` recorded an overreaching
post-close *re-add* predicate that failed on original although the close
itself succeeded; the runner no longer requires re-add. `-02` passed both
sides' close and provider predicates. `-final` reran the corrected code after
fixing a separate last-tab/parked-Home draft edge case. The lock in `-final`
attests the pinned original executable SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
upstream source `2670273ff17da96f85c5826ced57aa1b368754fa`, native binary
SHA-256 `e1a6f040cfcdbdf50e559f2c3c8be77eb08759d967494ba10ae5b59b44b003d5`,
fixture SHA-256 `d18132f88c4a7a638a244b0ea92007163246fc3ce0bbe5bcb2b90df02a67bb39`,
and shared terminal profile
`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`.
Both real PTYs returned `EXECUTED`, `provider_contract=true`,
`TAB_INTERACTION_CHECKS_PASS` and `TAB_CLOSE_CHECKS_PASS`. Both observed three
provider requests/completions before and after closing the synthetic Home
tab; the prior session's transcript returned on each side. The runner records
per-side actual mouse bytes, predicates, `.cells.json`, `.png` and raw VT.

The independently hovered synthetic Home tab at x32–65,y0 matches **all 34
styled cells** (including its functional `✕`); the paired whole hovered frame
still differs in 157/4800 styled cells and 2043/647040 PNG pixels. After
closing, whole-frame comparison remains **DIFFERENT**: 248/4800 cells and
9550/647040 pixels. The returned original tab remained pointer-hovered and
expanded its title farther than the native view; x28–34,y0 and dynamic elapsed
text, independently random Home examples, Location strings and actual binary
versions remain unequal. These differences were not masked or asserted away.
This is a VIS interaction slice, **not** a VIS01–24 or T44 parity PASS. The
deck remains process-local (upstream tab order is persisted); inactive-tab
attention and background-turn navigation are still unsupported.

## Verification and limits

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
--no-fail-fast --quiet`: PASS 0 failed, 202 TUI tests (existing opt-in live
ignores). `cargo fmt --all -- --check`, `cargo clippy --locked --workspace
--all-targets -- -D warnings`, `cargo build --locked`, `node --check
scripts/tui_capture/capture.mjs`, `python3 scripts/check_docs.py`,
`python3 scripts/progress.py check` and `git diff --check` all PASS. An
initial full run timed out at a test that expected another Home query after
the now-correct parked-Home restoration; the test was corrected to assert
preserved draft and zero query, then targeted and full workspace reruns passed.

Next: qualify/persist real tab order and wider/narrower paired interactions;
finish the frozen VIS01–VIS24 and remaining V08–V09/safety outcomes. T44 stays
active; the existing untracked `.opencode/` was not touched.
