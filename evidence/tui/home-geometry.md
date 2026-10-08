# T44 — mounted Home footer geometry and retained-tail raster diagnosis

## Result and scope

`shell.rs::render_home` now reserves the actual separate footer flex child:
zero rows below44 columns or12 rows, padding only when height≥16, and a content
row only for a mounted MCP item or the version slot. A current MCP inventory
cannot displace Home when its footer is not mounted. At43×24 this fixes the
one-row logo/prompt/caret shift without altering the editor, typed inventory,
permissions, stored history, service ownership or demand-driven draw scheduler.

Pinned source: `opencode/packages/tui/src/routes/home.tsx` and
`feature-plugins/home/footer.tsx`, commit2670273ff17da96f85c5826ced57aa1b368754fa.
Existing Home tests now include43 with empty and every admitted inventory status;
wide/short positions remain covered. No new production dependency or API.

This is a qualified geometry/diagnostic slice, **not whole T44/VIS/R6/V09 PASS**.
Full raw styled cells, cursor and PNG are compared without masks or normalization.

## Current-source association

Primary `tool-preview-attempt-021`, additional WebGL020 and temporal023 share:

- Captured base: `a974704be99dfff877fef4bf7264e871be7a92eb`;
  tree `45d291842402d9e44cdf1479482030fa7184cc44`.
- Dirty diff: `f94f11deb3f59effd3afe19464c640206ed7186bf960046c48e30ee2235c4e04`.
- Source manifest: `fcec44d9e7620087e3b682bfc2118c7727397e5aacd88c2c48f85345e59a37ad`.
- Actual `cargo build --locked`; native ELF SHA-256
  `60d1b05000042a12845b041126d156acc3c9ea498124023adb39a08f40884f9a`.
- Original executable remains pinned SHA-256
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

No Rust or capture-runtime edits follow these final captures; subsequent changes
are documentation/progress only. Locks retain commands, protocol, actual inputs,
versions, external addon lock/hash and profile-specific source association.

## Actual paired ordinary profiles

Every run uses the same isolated actual PTYs, fake Responses provider and real
stdio MCP/Shell fixture, with27 paired stages and two native-only `/cards` frames.
Both sides report `PASS_BEHAVIOR_ONLY`: Home controls issue zero provider requests,
real disconnect/failed initialization/retry restore the Unicode draft and caret.
The completed session has six provider requests, four tool effects, three real
MCP calls and exactly one17-byte Shell counter line. Those effects, artifact hashes
and native read-only operation/resource/presentation observations stay unchanged
through expansion, paging, resize, `/new`, reopen and clean same-root restart.

| Attempt | Frontend control | Full comparison outcomes |
| --- | --- | --- |
| 018 | Initial unrefreshed DOM after geometry fix | 4 EQUAL /50 DIFFERENT +4 native-only entries |
| 019 | Explicit refresh of unchanged DOM buffer | 4 EQUAL /50 DIFFERENT +4 native-only entries |
| 020 | Additional mature WebGL profile | 6 EQUAL /48 DIFFERENT +4 native-only entries |
| 021 | Final current-source primary unrefreshed DOM | 4 EQUAL /50 DIFFERENT +4 native-only entries |

Runner exit1 is the honest visual-difference result, not a green parity gate.
For021,43×24's entire1032-cell grid and cursor are EQUAL (previously327 cells
and cursor differed);44×24 and63×24 grids/cursors are also EQUAL. DOM PNG43 has
49 differing pixels, all atx361/y112–263;44 has41 atx370/y240–280.63's complete
PNG is EQUAL.019's explicit refresh leaves those edge differences unchanged.

On020,43/44/63 full grids, cursor and PNG are EQUAL. WebGL's actual measured
cell width is8 pixels versus DOM's fractional~8.43 for the same DejaVu14/DPR1
profile settings. Both binaries share each profile, but these are distinct
frontend geometries: WebGL is additional evidence, not replacement DOM pixels.

## Diagnosed DOM edge, not hidden product output

The read-only geometry observer now bounds observations to80 actual DOM rows,
four tail child boxes/styles and four outside-column color attributes per row.
It reads no tail glyphs/text, changes no DOM/CSS/buffer and draws no raster.
The geometry privacy/dimension qualifier passes.

At44 columns both buffers retain120-cell lines after alternate-screen shrink.
Original off-viewport cells retain RGB foreground/background modes with colors
8421504/657930; native's cleared tail has default modes. The original DOM span
begins atx370.734375 and extends beyond the371-pixel screenshot edge, whereas
native's last visible spaces end there. Pinned xterm6
`DomRendererRowFactory.ts` derives length from `getNoBgTrimmedLength()` and does
not cap that loop to current columns. The retained styled tail can therefore
paint the final fractional PNG column despite equal visible styled cells.
Refreshing the same buffer did not remove it. No synthetic native writes beyond
the viewport, CSS clipping patch, raster crop or comparison mask was introduced.

Short64×12 still has nine differing footer cells: original transparent spaces
overpaint underlying prompt-footer characters atx3/5/9, and actual native0.1.0
versus original2.0.12 occupiesx56–61. Cursor agrees. These genuine differences,
compact native service status and all modal/tool differences remain unmasked.

## Current-source cursor regression

`cursor-temporal-attempt-023`,120×40 expanded MCP, WebGL/blink/supported sync:
both `QUALIFIED_CURSOR_BEHAVIOR_ONLY` across six real composer/Search states.
Native cycles3/3/3/4/3/4; original3/3/4/4/3/4. Every state has≥3 full observed
raster cycles, adequate sampling, cadence relative to its own idle owner, exact
final caret/draft restoration and zero visible unsynchronized phantom states.
Native continuous hover has277 moves/2478 parsed commands; Search hover has
275/1367. Original Search has25 commands despite continuous moves; no claim of
continuous original Search repaint is inferred from input injection alone.
No tool replay is independently checked by actual effects and durable facts.

`check_cursor_temporal.py --compare-output evidence/tui/cursor-temporal-matched-004
evidence/tui/cursor-temporal-attempt-023` passes the144 full PNG/fixed-owner RGBA
and opacity audit (88,473,600 pixels).67 actual-phase matched pairs produce134
strict full comparisons, ALL DIFFERENT. Runner150 DIFFERENT/20 unmatched phase
entries remain honest, not equality inferred from unmatched samples. Prior
representative geometry/fallback/steady proofs remain separately recorded in
`cursor-frame-presentation.md`.

## Gates actually executed

Serial jobs3/test threads2/approved cache TMPDIR, current source:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked
cargo build
cargo build --locked --release
target/debug/oc --help
target/release/oc --help
python3 crates/oc/tests/support/startup.py target/release/oc
python3 crates/oc/tests/support/discovery_startup.py target/release/oc
```

ALL PASS; `.local/t44-home-geometry-gates-20261008.log` has46 completed results,
1733 passed/0 failed/11 unchanged opt-in ignores, TUI461 (26.19s), release2m06s.
Release startup covers safe refusals/retry/headless/no effects and terminal
restoration; discovery unauthorized/forbidden/oversized/slow/absent/present uses
the expected GET/no Responses and restoration. No ignored live campaign run.
Targeted Home23/0, geometry qualifier, modified Node syntax, Python repository
47/0, documentation/progress structure and source diff checks PASS.

Captured padded `.txt` and raw `.vt` are immutable data: only those exact capture
paths are excluded from Git whitespace checking, never from visual comparison.
No test/security/resource bounds are changed. AUTH06 remains deferred and the
original24/24/Go13/24 ledgers are unchanged. Whole frozen R1–R6/VIS01–VIS45 and
final R6/V09 remain open; no finish is justified by this slice.
