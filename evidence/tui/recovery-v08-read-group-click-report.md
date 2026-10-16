# T44 — paired exploration disclosure

## RECON and bounded goal

Pinned upstream v2.0.12 groups adjacent read/glob/grep parts and starts the
exploration header collapsed (`packages/tui/src/routes/session/grouping/session.ts:67-71`,
`routes/session/index.tsx:1874-1929`). A mouse-up on its header expands the
original parts, and another click collapses them. The prior Rust grouping had
no disclosure interaction; `/cards` offered separate durable inspection.

The 120×40 real-read fixture defines the measurable slice: with the same PTY
mouse click, display `→ Explored — 1 read` and then `→ Read fixture-note.txt`
immediately below it; clicking again must remove the detail without losing the
header. Do not hide errors, denials, unknown/cancelled outcomes or truncated
results; the application-owned operation graph and `/cards` remain unchanged.

The paired fixture intentionally has no configured native profiles or default
agent. Native composition does not invent a built-in; upstream has its own
implicit Build profile. The footer mismatch cannot safely be fixed by fabricating
a Build ID in the Rust snapshot or the fixture.

## Change and verification

Implementation commit `0e85958`: `app.rs` stores ephemeral expanded operation
IDs per session, maps mouse presses to the visible header (accounting for
wrapping, viewport scroll and actual session/sidebar/prompt geometry), anchors
the viewport when expansion adds rows and ignores drag/selection, modal or
offscreen clicks. `tui_cmd.rs` now actually forwards unhandled mouse events to
the view state while retaining wheel scrolling. `messages.rs` expands the
same indexed and full transcript projections. `tools.rs` draws inline group
members without an extra blank line or the standalone read result summary.
When a new page/session/workspace replaces the view, expansion is cleared.

`scripts/tui_capture/capture.mjs` adds an opt-in `--exploration-click true`:
it locates each side's live header in the actual styled grid, sends SGR down/up
through its PTY, captures expanded and recollapsed cells/PNG/VT, and records
inputs and predicates. Default runner behavior is unchanged. The standalone
fixture provider is loopback-only; no real credentials or owner data used.

- `cargo test --locked -p oc-tui`: 174 passed (including multi-size mouse,
  wrapped hit, long-result scroll anchoring and expanded/uncertain outcome
  tests). Final serial `cargo test --locked --workspace --no-fail-fast --quiet`
  passed with zero failures and existing opt-in live ignores; full workspace
  output includes 174 oc-tui unit tests.
- `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets
  -- -D warnings`, `cargo build --locked`, `node --check
  scripts/tui_capture/capture.mjs`, `python3 scripts/check_docs.py`,
  `python3 scripts/progress.py check`, and `git diff --check`: passed.
- Three immutable attempts with identical fixture and profile:
  `recovery-v08-read-group-click-{01,02,03}/`. Command, replacing the suffix:

  `node scripts/tui_capture/capture.mjs --reference
  /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --geometry true
  --sample tools --sidebar hide --columns 120 --rows 40 --exploration-click true
  --output /home/opencode/ai/oc/evidence/tui/<attempt>`

  Attempt 01: original toggled; native timed out because `tui_cmd.rs` discarded
  transcript mouse events before `handle_mouse`. Attempt 02: both toggled but
  native expanded row had a separate blank line and `↳ Loaded …` whereas the
  original rendered a single `→ Read fixture-note.txt` row. Attempt 03 (final
  code): both independently execute the real read with `provider_contract=true`,
  both pass the collapsed → expanded → recollapsed interaction predicates,
  including header and member wording/row positions. Pinned upstream binary
  SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`;
  final native binary SHA-256
  `0dcf5ea607496aa418be06493bd6237a05f90c49d6eb5a9e363be430690e9752`.
  Final lock seals initial HEAD `58c5600`, tracked dirty diff
  `9756e515a44b62633173882af2d9f42066358df06e7747a4d94586de0f7b1adc`,
  source manifest, fixture SHA
  `dbfc93c470dd18d3af79b33195a13c898e83944ec5e0f242cd32d88b65531644`,
  shared profile ID `e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`.

## Open parity gates

The final whole-frame comparison remains **DIFFERENT**, not parity: expanded
frame 4598/4800 styled cells and 11296/647040 PNG pixels differ. The matching
group-member text/placement does not prove that its every cell style matches;
root chrome, blank-cell foreground, implicit upstream Build identity and
unfrozen duration still differ. The original's `session.grouping=none` setting
has no native equivalent, nor have all read/search/permission/restart states
been compared in both binaries. VIS16, VIS17 and the remaining mandatory VIS
gates stay OPEN. The T44 task remains active, not completed.

Next: qualify the remaining S05/S06/S08 safety cases, S07 resource regression,
and the mandatory VIS01–VIS24 exact frame/interaction matrix. For footer agent
comparison, obtain an equivalent *verified* effective upstream profile and a
real native configuration rather than inferring an ID from a rendered string.
