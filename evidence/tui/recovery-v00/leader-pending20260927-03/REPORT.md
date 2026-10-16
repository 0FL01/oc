# VIS11 post-fix actual-source-built qualification — 2026-09-27

**Result: the three reported pending semantics fixes are observed working; full
VIS11 visual qualification remains NOT PASS.** Selected final campaigns contain
**152 full styled-grid comparisons DIFFERENT / 152 full PNG comparisons DIFFERENT**,
zero EQUAL, BLOCKED or unstable selected captures. Every compared frame is complete
120×40; all 304 actual side captures, raw VT, inputs, request records and failures
are preserved. No masks, synthetic panels or application-clock replacement.

## Final-source association

HEAD at execution: `97a55789b87cee491eb0d8823715c02c83086ab6`, plus captured dirty
Rust source. Every selected campaign independently ran **`cargo build --locked`**
through `--build-oc true`, exit 0. All have identical compiled Rust-input digest:

`08359df951e90bd449746084adb9cf3ef1bafdf1dc7cd6c8dddc662308ed9d74`

Native executable SHA-256:

`c1832649accb66f0716ad589cf6aab7ef473247b7c938662acc6abfedaafee7a`

Full source manifest SHA-256 for main five and isolated Enter:

`b748e223cc5fa170011210f24c290d794db3ca462c275e67c1068ae916e943dd`

For final extra, after a test-only provider-validation correction:

`d73d9d86ceafbd1774ce66abe7a34605df00a43af25f8af8ba96540c06e0d247`

Only tooling changed between these manifests; analyzer verifies identical Rust
inputs and executable across all seven. This associates the actual dirty-source
build, not HEAD alone. Pinned original SHA remains
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

## Selected full matrix

| Campaign directory | Stages per side | Grid / PNG DIFFERENT | Cursor-different grids | Provider requests original / native |
|---|---:|---:|---:|---:|
| paired-default-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-nested-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-configured-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-precedence-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-legacy-v1-02-fresh | 26 | 26 / 26 | 4 | 0 / 0 |
| paired-extra-fresh-02 | 18 | 18 / 18 | 1 | 2 / 2 |
| paired-enter-isolated-fresh | 4 | 4 / 4 | 0 | 2 / 2 |
| **Total** | **152 paired stages** | **152 / 152** | **21** | **4 / 4** |

`paired-analysis-final.json` contains **every stage's** full cell/pixel difference
count, bounding box and cursor verdict, plus request input, lifecycle observations,
source/build hashes and raw expiry-write timing. Each campaign also has the direct
`leader-*.grid-diff.json` / `.png-diff.json` outputs. No chip-state failures are
omitted from these counts.

## Three fixes: exact observed results

1. **Pending Enter now submits the exact full Unicode draft.**
   `paired-enter-isolated-fresh/{upstream,oc}/leader-enter-only-pending` starts with
   equal draft and caret `(77,21)`. Both issue one transcript request and one
   auxiliary title request; all four are valid local fixture requests. Actual
   transcript user input on both sides is exactly:
   `VIS11 full draft αβ caret-middle preserving every word`.
   Both finish with empty composer and caret `(5,34)`. Original title/transcript
   arrival order differs from native; neither request was hidden.
2. **Pending Ctrl+C preserves the nonempty draft/caret.**
   Final extra `leader-extra-ctrlc-{pending,next,restored}` keeps the whole draft
   and caret `(77,21)` on both sides. It restores normal text color and issues
   zero requests; no exit or draft clear occurs.
3. **Commands underlying pending muting/expiry now matches.**
   Final extra `leader-extra-modal-{filtered,pending,expired,restored}` shows
   underlying text `#626262 → #353535 → #626262 → #eeeeee`, with modal search
   caret `(54,13)` until dismissal and composer caret `(77,21)` afterward.
   Full filtered/pending/expired grids differ only by the real six-cell version
   footer; PNGs also retain a search-caret paint difference, totaling 425 pixels.
   There is no additional input between pending and expired.

Extra's final Enter submits the actually modified full draft on both sides:
`VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord`.
It produces two valid requests per side; earlier Left/Right/Ctrl+C/modal stages
produce none. Actual wire input, not a screen substring, establishes full text.

## Core leader lifecycle and color roles

Five main pairs observe normal→pending→restored with full draft/caret preservation,
invalid `z` insertion at the actual middle caret (`word`→`wzord`), Esc/Backspace
restoration without draft editing, repeated-leader cancellation rather than timer
restart, successful model/palette chord execution and modal-focus retention.
Pending Left/Right in extra restores without moving the caret in this fixture;
this is not a claim about all unmatched nonprintable keys.

Both binaries' actual metadata includes
`Build · MiMo-V2.6-Flash Free OpenCode Zen · fast`.

- Draft/model: `#eeeeee` normal → `#808080` pending → `#eeeeee` restored.
- Prompt border/agent: `#5c9cf5` → `#484848` → `#5c9cf5`.
- Provider stays `#808080`; variant stays `#f5a742`.
- Real paste chip stays foreground `#0a0a0a` / background `#f5a742` in all three
  phases on **both** sides. Surrounding text/border follow pending roles correctly.
- Leader/expiry do not alter transcript symbols. Idle-pending/restored full-grid
  symbol vectors and caret are independently checked equal within each side.

## Timeouts: actual no-input PTY expiry

Each main idle window has exactly one acknowledged input: its leader. Raw
timestamped PTY chunks record restoration without another key. Substantial expiry
writes relative to that input, in milliseconds:

| Mode | Admitted setup | Original / native |
|---|---|---:|
| default | Ctrl+X, 2000 default | 2007.64 / 2004.56 |
| nested | Ctrl+X, leader.timeout 1200 | 1209.04 / 1204.74 |
| configured | Ctrl+G, palette remap, nested 1200 | 1213.87 / 1204.66 |
| precedence | Ctrl+G, nested 1800 + legacy sibling 600 | 1806.55 / 1809.49 |
| legacy | Ctrl+G, admitted legacy 1400 | 1409.51 / 1404.14 |

Original legacy uses actual v1 `tui.json` migration, with no preexisting cli.json:
leader_timeout 1400 and command_list `<leader>p`. Native uses owner-admitted
explicit legacy configuration and command_list alias. Different configuration
paths are recorded truthfully. Raw v2 sibling legacy field is **ignored by the
original**; it is not presented as an admitted original legacy oracle.

Actual `output-timeline.jsonl`, `protocol.json`, inputs and raw VT retain all small
control writes too. These observations establish no-input expiry redraw; they do
not independently establish a production CPU/polling audit.

## Remaining visible differences — concrete paths

These are retained visual gaps, not failures of the now-matching pending RGB roles.

- **VIS07 chip geometry/expansion remains unfixed.** Every main pair's
  `leader-chip-{normal,pending,restored}` differs by 28 cells / 2016 pixels and
  caret original `(33,22)` versus native `(29,22)`. Actual chip click expands all
  three original lines; native leaves the placeholder. `leader-chip-expanded-fulltext`
  differs by 387 cells / 28837 pixels. Hidden native content loss is not inferred
  from the absence of mouse expansion. Chip RGB is constant in all phases.
- **Configured Commands shortcut hints remain wrong:**
  `paired-configured-fresh/oc/leader-modal-normal` still shows `ctrl+x m/l/n/b`,
  where pinned original shows configured `ctrl+g m/l/n`. Actual remapped palette
  chord works; displayed command hints/list contents do not fully match.
- **Commands content/sections differ:** original includes Connect an integration,
  Prompt/Stash/Queue/Skills; native shows different System/Session/Permissions/
  Undo/Redo entries. Normal/pending Commands frames differ by 210 or 213 cells.
- **Select model footer/backdrop differs:**
  `paired-default-fresh/oc/leader-success-next` lacks original's
  `View all integrations ctrl+a` footer; actual path/footer exposure differs.
  Default/nested model-dialog frames differ by 94 cells (9405/9406 pixels).
- **Long modified-draft wrapping/caret:** final extra's `leader-extra-enter-pending`
  is original `(40,22)` versus native `(41,22)`, 24 cells / 1339 pixels. Both wire
  submissions preserve the exact same full text. This is a composer-wrap gap,
  not a pending-input consumption failure.
- **Completed response footer:** isolated Enter completion differs by 17 cells /
  565 pixels; final extra by 18 cells / 591 pixels, in the actual duration/usage
  metadata row. Real clocks/metadata remain visible.
- **Actual version:** most core states differ by only six styled cells / 298
  pixels: original `2.0.12` versus native `0.1.0`. No version substitution/mask.

## Preserved failures and corrected tooling

All original evidence01/02 remains untouched. First seven evidence03 directories
without `-fresh` reused temporary roots because the old runner derived root from
output basename. Original extra/isolated Enter visibly reopened prior tabs and
shifted Home; those captures remain preserved and are excluded from final claims.
The leader-only runner now derives a path-specific hashed root and rejects an
already existing root. Final selected roots are genuinely fresh per attempt,
common project identity on both sides, isolated separate HOME/data, real launch
configuration and paths recorded. No old state was deleted.

`paired-extra-fresh` is also preserved: its added final draft was not recognized
by the fixture validator, yielding a real HTTP 400 on both sides after Enter.
`paired-extra-fresh-02` admits the two exact scenario texts and validates all
requests. This corrects the test fixture, not production behavior; failed error
frames and the earlier `paired-analysis.json` are retained.

## Checks / handoff

Syntax: runner/probe/analyzer `node --check`; Python bridge AST — PASS.
Independent final analyzer — PASS integrity, **not visual acceptance**:

```sh
node scripts/tui_capture/analyze_leader_pairs.mjs \
  evidence/tui/recovery-v00/leader-pending20260927-03 --fresh --extra-refreshed
```

Analyzer verifies all sealed capture hashes, actual source builds, equal Rust/binary
digests, exact isolated Enter input and main no-input expiry preservation; its
immutable final report is `paired-analysis-final.json`.

**Cargo ownership returned.** No Rust/acceptance/progress/.opencode edits, commits
or pushes by this capture work. Remaining full visual differences above must stay
open; this report does not declare VIS11 or VIS07 PASS.
