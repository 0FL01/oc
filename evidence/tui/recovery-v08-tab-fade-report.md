# T44 — paired selected-tab overflow fade

Code commit `641c287`; pinned original v2.0.12 source
`2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
The immutable capture `recovery-v08-tab-fade-01/` locks the same 120×40 terminal
profile `e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`
and tools fixture SHA-256
`dbfc93c470dd18d3af79b33195a13c898e83944ec5e0f242cd32d88b65531644`
as the preceding `recovery-v08-canvas-tab-02/` pair. The capture was made with
the code diff before commit; its lock records initial HEAD `2cd4b9e`, dirty
diff hash, source manifest, and native binary SHA. No product credentials used.

## RECON and change

Original `packages/tui/src/component/session-tabs.tsx` defines a four-grapheme
trailing fade for overflowing tab titles, using `fadeTitleColor` and the selected
tab background. Previously native `crates/oc-tui/src/shell.rs::tab_line` painted
the entire title with the same color. In the independent preceding pair, the
four rightmost visible title cells x=28–31, y=0 matched symbols and background
but differed in foreground: original `#c4c4c4`, `#929292`, `#616161`,
`#2f2f2f`; native `#eeeeee` throughout.

The native tab now clips on whole Unicode graphemes by terminal cell width and
applies the source-derived 0.2/0.44/0.68/0.92 trailing fade only when the
selected title actually overflows. Short and exactly fitting titles remain
unfaded; bold, indicator, and blank styling are unchanged. TestBackend
regressions for overflow, exact fit, combining marks, wide glyphs and padding
failed against the previous renderer and pass after the change.

## Paired evidence

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --columns 120 --rows 40 \
  --exploration-click true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-tab-fade-01
```

Both sides EXECUTED with `provider_contract=true`. Both passed the real
read-group expand/recollapse predicates. `check_region.py` on independently
captured completed and expanded cells reports **0/4** styled/symbol/width
differences for x=28–31, y=0 (previously **4/4** foreground differences).
The comparator still reports DIFFERENT: completed full frame **361/4,800**
styled cells, **8,761/647,040** PNG pixels, exit 1. The prior frame was
362/4,800 cells and 8,759 pixels, but other dynamic state changes between
runs: those totals must not be interpreted as a deterministic improvement of
one or four cells. The matched fade region is the bounded observation, not a
VIS gate PASS.

Final code: serialized `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test
--locked --workspace --no-fail-fast --quiet` 0 failures (TUI 179;
pre-existing live ignores), workspace fmt/clippy `-D warnings`, locked build,
docs/progress and diff checks PASS.

## Still open

The default upstream tab indicator is status-mode (blank when idle); native
still paints `1`, does not expose equivalent indicator config or unread/attention
state, and must not fake those states. The upstream add-tab control is not
implemented; painting an inert `+` as working would be misleading. Home
placeholder examples are randomized upstream; prompt/footer agent identity
differs because the native paired fixture intentionally has no configured
profile, while upstream implicitly selects Build. Durations remain variable.
No full VIS01–VIS24 parity gate is closed; S07 residual measurements and the
remaining V08–V09 matrix remain open. Next: trace indicator config/status
projection and real add-tab action before showing either control, then use
fresh paired captures and preserve incomplete results honestly.
