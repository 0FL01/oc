# VIS11 effective-leader hints: final-source paired qualification

**The effective-leader hint fix is observed working, with leader lifecycle/color
contracts retained. Full visual gate remains NOT PASS: 152 full styled-grid
comparisons DIFFERENT / 152 full PNG comparisons DIFFERENT.** No selected frame
is unstable or blocked. Chip geometry/expansion remains the historical VIS07 gap;
its pending RGB roles are correct and constant. Evidence01–03 remain immutable.

## Actual final-source/build association

HEAD: `97a55789b87cee491eb0d8823715c02c83086ab6`, plus recorded dirty source.
Every selected campaign independently ran `cargo build --locked` through
`--build-oc true`, exit 0. Analyzer verifies identical Rust inputs and binary
across all seven selected campaigns:

- **Native binary SHA-256:**
  `33d4f3a8f96ae299399aed4878b3eda9858e4617b3ec1b52b288384a324eedeb`
- **Rust-input digest:**
  `23a32902b0d39e5bd1832966766c835f6a5d5c7b3383ffc4b8d88331313772fc`
- Full source-manifest digest, main five:
  `2b69011b257861937f621f1dbad5b004e397e4b7f3b704578138cb44c87ed4f0`
- Full source-manifest digest, TPS-aligned extra/isolated Enter:
  `02b733b57e2ecd45b8f7b3ede1ecfc6e4aecd2644106d760db6778f8cc5fdc12`

The two manifest epochs differ in test tooling; compiled Rust and executable are
identical. Every selected state was freshly recaptured from this source-built
binary. No evidence03 states are substituted into the final-source matrix.

Pinned original SHA-256:
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
All roots are path-specific and new, with common project identity and separate
HOME/data per side. Full styled 120×40 grids and shared-xterm Chromium PNGs come
from actual PTY bytes; real version, path, title and clocks remain visible.

## Exact selected matrix

| Directory | Paired stages | Grid / PNG DIFFERENT | Cursor-different grids | Provider requests original / native |
|---|---:|---:|---:|---:|
| paired-default-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-nested-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-configured-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-precedence-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-legacy-v1-02-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-extra-fresh-tps-matched | 18 | 18 / 18 | 1 | 2 / 2 |
| paired-enter-isolated-fresh-tps-matched | 4 | 4 / 4 | 0 | 2 / 2 |
| **Total** | **152** | **152 / 152** | **21** | **4 / 4** |

There are 304 complete side captures. `paired-analysis-tps-matched.json` contains
every selected stage's complete cell/pixel difference count, bounds and cursor
verdict, plus source/build hashes, actual request input and lifecycle observations.
Direct per-stage `.grid-diff.json` / `.png-diff.json`, raw VT, timestamped byte
timeline, keys, protocol/config/launch and PNG render metadata are retained.
All failing chip states are included in the counts, without masks or clock changes.

## Effective hints now match captured leader configuration

`roles-and-hints.json` independently checks the actual complete captured grids in
all five main modes. Commands visible Switch model, Switch session and New session
use `ctrl+x m/l/n` in default/nested, and **`ctrl+g m/l/n`** in
configured/precedence/admitted-v1-legacy. Native configured Commands additionally
shows `ctrl+g b` for Show sidebar. No stale `ctrl+x` hint remains in those three
captured Ctrl+G Commands frames. Actual remapped palette chord `<leader>p` opens
Commands, and default model chord opens Select model.

Concrete corrected frame:
`paired-configured-fresh/oc/leader-modal-normal.{cells.json,png,txt}`.
Evidence03's stale Ctrl+X hint defect is historical, not repeated on this source.
Configured Commands normal/pending now differ by 210 cells rather than evidence03's
213; remaining differences are command content/sections and actual version.

## Lifecycle, draft, caret, provider wire

- Normal/pending/restored preserve full draft and caret. Invalid printable `z`
  inserts at the middle caret (`word`→`wzord`). Esc and Backspace restore without
  editing; repeated leader cancels rather than restarts. Idle restoration requires
  no further input. Main five modes issue **zero requests** throughout.
- Pending Ctrl+C preserves nonempty draft and caret `(77,21)` on both sides.
- Filtered Commands retains search caret `(54,13)` and changes underlying text
  `#626262 → #353535 → #626262` on leader and idle expiry. Dismissal restores
  composer `#eeeeee` and caret `(77,21)`. No expiry input is injected.
- Pending Left/Right restores with unchanged draft/caret in this fixture.
- Isolated pending Enter submits exactly
  `VIS11 full draft αβ caret-middle preserving every word` on both sides.
  Both have two **valid** local requests: transcript and auxiliary title. Actual
  full user input is compared, not inferred from a partial visible substring.
  Both restore an empty composer and caret `(5,34)` after completion.
- Extra's final Enter submits exactly
  `VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord`
  on both sides, two valid requests each; earlier extra stages produce none.
  Original emits title before transcript; native emits transcript before title.
  Both actual requests are retained. No live API or external effects are used.

## Actual phase/child roles, including chip invariance

Actual metadata is `Build · MiMo-V2.6-Flash Free OpenCode Zen · fast`.
Independent role checks verify on both sides in every main mode:

| Role | Normal → pending → restored |
|---|---|
| Draft/model | #eeeeee → #808080 → #eeeeee |
| Prompt border/Build | #5c9cf5 → #484848 → #5c9cf5 |
| Provider | #808080 unchanged |
| Variant | #f5a742 unchanged |
| Real paste chip | foreground #0a0a0a / background #f5a742 unchanged |

Chip wrap/expansion failures below do **not** indicate a pending-color failure.
Full idle-pending/restored symbol vectors and caret are independently equal within
each side; transcript content is not rewritten by leader presentation.

## Deadline/no-input restoration proof

Main idle windows contain exactly one acknowledged key: the leader. Raw substantial
PTY restoration writes relative to that input, milliseconds:

| Mode | Configuration/admission | Original / native |
|---|---|---:|
| default | Ctrl+X, 2000 default | 2008.60 / 2006.88 |
| nested | Ctrl+X, leader.timeout 1200 | 1206.87 / 1207.61 |
| configured | Ctrl+G, nested 1200, palette remap | 1217.02 / 1208.23 |
| precedence | Ctrl+G, nested 1800 + legacy sibling 600 | 1816.00 / 1806.30 |
| admitted legacy | Ctrl+G, legacy 1400 | 1410.90 / 1403.64 |

Original legacy is actual v1 `tui.json` migration with leader_timeout and
command_list, no preexisting cli.json; native uses owner-admitted explicit legacy
configuration/command_list alias. Raw v2 sibling leader_timeout is not claimed
effective in original. Config manifests truthfully distinguish the admission paths.
Full raw timelines retain small terminal-control writes too. This proves no-input
expiry redraw, not a separate CPU/polling qualification.

## Remaining exact visual failures

- **Historical VIS07 chip wrap/expansion:** every main pair's chip normal/pending/
  restored is 28 cells / 2016 pixels DIFFERENT; original caret `(33,22)` versus
  native `(29,22)`. Actual chip expansion is 387 cells / 28837 pixels DIFFERENT:
  original mouse click expands all three lines; native retains its placeholder.
  Hidden content loss is not inferred. Chip RGB stays constant in all phases.
- **Long-draft wrapping:** extra `leader-extra-enter-pending` remains 24 cells /
  1339 pixels DIFFERENT, original caret `(40,22)` versus native `(41,22)`.
  Wire text is exactly equal and both submit; this is wrapping/caret geometry,
  not invalid-printable/Enter consumption or pending RGB.
- **Commands content/sections:** original Connect an integration and Prompt/
  Stash/Queue/Skills versus native's different System/Session/Permissions/Undo/Redo
  entries. Commands normal/pending frames remain 210 cells DIFFERENT; configured
  PNGs are 9224 pixels DIFFERENT. Effective displayed leader prefixes now agree.
- **Model-dialog footer/backdrop:** original View all integrations ctrl+a footer
  absent from native, with differing exposure of underlying real path/footer.
  Default/nested model-dialog frames are 94 cells DIFFERENT and 9404/9409 pixels
  DIFFERENT, respectively. Actual model dialog and caret focus still work.
- **Actual clocks:** final extra completion/restoration is 5 cells / 284 pixels
  DIFFERENT; isolated Enter is 5 cells / 294 pixels DIFFERENT, only the actual
  duration field in the completed footer. No synthetic fixed duration is used.
- **Version footer:** most otherwise-matching core lifecycle frames differ only
  by six styled cells / 298 pixels: actual original 2.0.12 versus native 0.1.0.
  Filtered modal frames additionally retain actual search-caret paint differences
  (425 PNG pixels total) despite matching buffer cursor state.

No new leader lifecycle/hint semantic defect is observed in this matrix. Remaining
full visual differences above stay open; this is not VIS11 or VIS07 PASS.

## Preserved fixture correction and checks

Initial `paired-extra-fresh-02` / `paired-enter-isolated-fresh` are preserved with
their first analysis: original fixture explicitly disabled TPS, while native used
its default and rendered a TPS suffix. This was a configuration mismatch, not a
new native leader failure. The two final `-tps-matched` campaigns explicitly set
session.tps=false on both sides and independently source-build again. Completion
differences now reduce to real duration fields. Main five states do not submit;
their source-built captures remain valid for lifecycle/hints/chips and are selected.

Syntax checks (runner/probe/analyzers/role checker, Python bridge AST) and
`git diff --check`: PASS. Independent integrity/contract checks: PASS; their exit
status does not override the full visual DIFFERENT results.

```sh
node scripts/tui_capture/analyze_leader_pairs.mjs \
  evidence/tui/recovery-v00/leader-pending20260927-04 --fresh --matched-tps
node scripts/tui_capture/check_leader_roles.mjs \
  evidence/tui/recovery-v00/leader-pending20260927-04
```

Both refuse to overwrite existing outputs. No Rust/acceptance/progress/.opencode
changes, commits or pushes by this capture work. **Cargo ownership returned.**
