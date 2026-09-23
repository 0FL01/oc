# T44 — paired root canvas and selected tab

Code commit `852cb0b`; pinned original v2.0.12 commit
`2670273ff17da96f85c5826ced57aa1b368754fa` and executable SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
The paired runner used the same `tools` fixture (SHA-256
`dbfc93c470dd18d3af79b33195a13c898e83944ec5e0f242cd32d88b65531644`),
120×40 terminal profile (`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`),
isolated HOME/data roots, loopback fake Responses provider, and a real `read`.
Neither side used production credentials. Both final sides report EXECUTED and
`provider_contract=true`; expansion and recollapse predicates pass on both.

## RECON and change

Before this slice, the identical-symbol blank canvas region in the completed
session (x=0–119, y=13–32) differed in **all 2,400 cells** solely in foreground:
original truecolor `#ffffff`, Rust terminal default `#eeeeee`. The Home region
(x=0–119, y=0–9) had the same difference for all 1,200 cells. The selected
session tab title (x=3–27, y=0) matched symbols/colors but lacked original
bold modifiers in all 25 cells. Pinned source `packages/tui/src/app.tsx:1309–1315`
paints the root; `component/session-tabs.tsx:1620,1688–1698` makes the active
title bold. Native root and selected tab styles live in `crates/oc-tui/src/shell.rs`.

The native frame now paints the root's truecolor white foreground over the
existing theme background and marks only the active tab title bold. Separate
TestBackend regressions first failed against the prior styling, then passed,
including a nonwhite explicitly styled user border. This does not change any
history, profile selection or real operation data.

## Paired evidence and checks

Two immutable attempts: `recovery-v08-canvas-fg-01/` after the root fix,
`recovery-v08-canvas-tab-02/` after both fixes. Run command (each attempt has
its own `--output` directory):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --columns 120 --rows 40 \
  --exploration-click true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-canvas-tab-02
```

`scripts/tui_capture/check_region.py` against the final independent styled
grids gives **0 symbol, width or styled differences** for both 2,400-cell
session and 1,200-cell Home rectangles and the 25-cell selected tab title.
The matching exploration header at x=5,y=6 and click disclosure are retained.
Previous completed-frame comparison (before root fix) differed in
4,580/4,800 cells; root-only attempt 01 differs in 388/4,800; final attempt
02 differs in 362/4,800 cells and 8,759/647,040 PNG pixels. The final capture
lock includes initial HEAD `42e682d`, tracked dirty diff SHA, binary SHA and
source manifest; the change was subsequently committed as `852cb0b`.

`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
--no-fail-fast --quiet` passed (TUI 176; existing opt-in live ignores);
`cargo fmt --all -- --check`, workspace all-target clippy `-D warnings`,
`cargo build --locked`, docs/progress checks and `git diff --check` passed
after a formatting-only import-order correction. Paired runner exit **1**
correctly reports **DIFFERENT**, not VIS parity.

## Open outcomes

Completed, expanded, recollapsed and Home whole frames remain different.
Additional discrepancies include tab indicator/fade, Home suggestions,
prompt/footer and upstream implicit Build versus the fixture's intentionally
unconfigured native agent. Durations are not frozen. No change to agent
identity is justified by the fixture alone. Exact paired matrix VIS01–VIS24,
S07 residual resource metrics and the remaining V08–V09 qualification remain
open; T44 is not done. Next: investigate a comparable stable tab indicator,
Home or prompt cell region with pinned source and a fresh paired capture.
