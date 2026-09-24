# T44 — retained session tabs and real add/return interaction

## Source-backed lifecycle and implementation

The pinned v2.0.12 `packages/tui/src/component/session-tabs.tsx:1310-1339,1744-1769`
paints an actionable ` + ` only on an attached Session. A matching left
press/release invokes `tabs.add()`; `context/session-tabs.tsx:384-397` opens a
synthetic New session Home slot without a durable session. Its first accepted
prompt creates the root; the previous tab remains selectable. Previously the
native Home and first turn were sessionless/atomic, but `/new` replaced its
only view and there was no returnable tab or actual `+` action.

The binary now owns an ordered, at-most-16-view deck. Each parked `TuiState`
retains its editor/draft, bounded history window and scroll, with a per-tab
tool-card paging cursor. A separate sessionless Home view is the synthetic
slot: add never creates a root; its accepted first prompt takes the reserved
real-tab slot. Tab and add hit-testing uses exactly the adaptive rectangles
used to paint them; only a same-control unmodified left press/release activates
an action. Dialogs own their clicks, drags/modifiers/overflow/unpainted cells
do not activate tabs, and pending/streaming turns cannot be parked. A
successful Location switch clears the old deck; a failed switch preserves it.
The Sessions dialog focuses an already retained session instead of appending
a duplicate. Existing bare Home still has no tab strip until its first accepted
turn. `crates/oc-tui/src/{app,shell}.rs` and `crates/oc/src/tui_cmd.rs` implement
the interaction, with real PTY/SQLite tests in `crates/oc/tests/pty_t42.rs`.

Review found and the implementation fixed three corner cases before the final
capture: a parked Home reserves room for its accepted root at the 16-tab cap;
successful typed `/new` no longer survives as a stale draft when returning to
the old view (mouse add preserves ordinary drafts); and `/cards` continues
older paging from its own tab's cursor. A separate PTY fixture verified mouse
add makes no new session, Home submit makes exactly one, the old draft and
history survive return, busy-turn add is refused, and failed Location switch
does not destroy the deck. The shell's selected synthetic `+` uses the pinned
upstream bold indicator style; its own TestBackend assertion and a fresh paired
capture verify that correction.

## Independent original/native captures

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --tab-click true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-retained-tab-final
```

Two immutable attempts are retained: `recovery-v08-retained-tab-01/` before
the selected Home indicator's bold-style correction and
`recovery-v08-retained-tab-final/` after it. Both independently run the pinned
original and Rust binary under the same 120×40 terminal/profile ID
`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`,
fixture SHA-256 `d18132f88c4a7a638a244b0ea92007163246fc3ce0bbe5bcb2b90df02a67bb39`,
real read and fake Responses provider. The pinned original executable SHA-256
is `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
source commit `2670273ff17da96f85c5826ced57aa1b368754fa`; the final native
binary SHA-256 is recorded in `recovery-v08-retained-tab-final/capture.lock.json`.
Both sides report `provider_contract=true`, `EXECUTED`, and
`TAB_INTERACTION_CHECKS_PASS`. The runner independently finds and clicks each
side's painted `+`, verifies old Session → synthetic Home with preserved old
tab → old Session, and retains styled cells, PNG, VT, inputs and click predicates
for `session-wide-completed`, `tab-added` and `tab-returned`.

Final paired frame comparison remains **DIFFERENT** (styled cells out of 4,800;
PNG pixels out of 647,040): initial Home 139 / 1,253; completed Session
204 / 4,791; added Home 159 / 1,879; returned Session 210 / 5,058. The
selected synthetic Home `+` now matches its upstream indicator modifier/color
and symbol; a direct region comparison `--rect 32 0 34 1` of `tab-added` shows
only one mismatched cell: the upstream *hovered* close `✕` at x62. On return,
six tab cells x26–31 differ because upstream shows a hovered close glyph and
shortens/fades that title. Native deliberately does not paint a nonfunctional
close action; a future actual close operation and hover semantics are needed.
Independent random Home examples, the real application versions `2.0.12` vs
`0.1.0`, elapsed time, and Location text also prevent whole-frame equality.
No masks, clock freeze, forged versions or fabricated session IDs were used.

The earlier full workspace run first exposed one old tab-indicator assertion
expecting unbold selected numbers; pinned `TabIndicator` marks selected labels
bold and that expectation was corrected. A later run exposed two independent
PTY synchronization issues: startup.py sought a contiguous Location path in
raw VT although Ratatui redraws only changed cells; it now relies on the
target-only safe DCP warning and actual model-picker check. S07's archive
sample captured one partially repainted resize viewport; it now waits for the
fixed eight-row active tail to settle (without weakening the identical-view
assertion). The isolated cases and final serialized workspace suite pass.

Final checks: `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked
--workspace --no-fail-fast --quiet` (zero failures; TUI 199, existing opt-in live
ignores), `cargo fmt --all -- --check`, `cargo clippy --locked --workspace
--all-targets -- -D warnings`, `cargo build --locked`, `node --check` on the
runner, `python3 scripts/check_docs.py`, `python3 scripts/progress.py check`,
and `git diff --check` all PASS on this code. These are a bounded R3/R5
tab-interaction diagnosis, **not** VIS01–VIS24 or whole T44 PASS. The deck is
process-local: upstream persists tab order on restart; hover-close and
background-tab navigation/status, broader geometry/dialog/markdown cases,
S07 residual metrics and V08–V09 final qualification remain open. The old
untracked `.opencode/` was not read, edited or staged.
