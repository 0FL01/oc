# T44 — adaptive tab geometry prerequisite

## RECON and bounded change

Pinned original v2.0.12 `packages/tui/src/context/session-tabs-model.ts:33-37,175-257`
uses preferred/minimum/maximum widths 22/8/32, a three-pass visible window,
digit-sized overflow markers, and a wider active tab in a cramped strip.
`packages/tui/src/component/session-tabs.tsx:1316-1339,1739-1772` reserves
three cells for an actionable add control. The native renderer had a single
32-cell tab and no shared layout/hit-test model. Upstream add navigation opens
a sessionless Home slot and does **not** create a durable session until submit;
native `PanelIntent::NewSession` currently creates one immediately. Painting
`+` and routing it to that intent would be a false interaction-parity claim.

Commit `1ca34aa` introduces a pure, clipped horizontal strip geometry model
with visible tab indices, tab rectangles, count/rectangles for overflow
markers, an optional add rectangle reserved **only when an action is
available**, and a hit-test that accepts only visible tab cells. The current
single-tab renderer consumes this model with add capability disabled; it
does not paint or expose an inert `+`, change the application session owner,
or claim multi-tab interaction. Four layout tests cover the one-, two- and
three-tab solutions, active-window retention, count digits, capability,
narrow/edge widths, disjoint clipped rectangles, and hit testing. The
single-tab renderer has a no-inert-control test. Tests initially failed before
the new layout existed; final `cargo test --locked -p oc-tui` passed 189/189.
Independent source/diff review reported no actionable layout discrepancy for
the reviewed cases. This is a prerequisite API+test slice, not a VIS gate.

## Independent executable check

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --columns 120 --rows 40 \
  --exploration-click true --agent-profile true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-tab-geometry-01
python3 scripts/tui_capture/check_region.py \
  evidence/tui/recovery-v08-tab-geometry-01/upstream/session-wide-completed.cells.json \
  evidence/tui/recovery-v08-tab-geometry-01/oc/session-wide-completed.cells.json \
  --rect 0 0 32 1
```

Both real PTYs executed the fake provider's genuine read
(`provider_contract=true`); the configured Reader instruction gate passed,
and both SGR click/recollapse predicates passed. The installed upstream
executable SHA-256 is
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`;
native executable SHA-256 is
`6f0ae544f3f0733bd83aa58df341e3334fae46c45a6bd8909bd957d57ac16ee1`.
The capture lock records original source commit
`2670273ff17da96f85c5826ced57aa1b368754fa`, source HEAD `3ac87c7`
plus dirty-diff hash `6e9b024c0d28664936bbecfaf2fbfae9a7f9717800e92cc5cc0b22893ddfee50`,
shared fixture SHA-256
`d18132f88c4a7a638a244b0ea92007163246fc3ce0bbe5bcb2b90df02a67bb39`
and terminal-profile ID
`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`.
The selected one-tab rectangle x=0–31,y=0 has **0/32** symbol/style
differences. Completed whole-frame comparison is **DIFFERENT**, 211/4,800
styled cells and 5,028/647,040 PNG pixels; runner exit 1 is expected, not
PASS. This capture does **not** exercise a second tab or add action.

Full serialized workspace tests passed with zero failures (189 TUI; existing
opt-in live tests ignored); workspace fmt, all-target Clippy with `-D warnings`,
locked build and diff checks passed. No pre-existing `.opencode/` files were
opened or staged. Full VIS01–VIS24 and V08–V09 remain open. Next real step:
application-owned sessionless Home acceptance and bounded retained tab state,
then wire the add/tab selection action to this layout, exercise it through a
paired real PTY, and requalify without disguising version or timing differences.
