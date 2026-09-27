# Independent full-PNG sample review — campaign 04

Result: sampled composer behavior is observed on the newly built binaries;
**VIS07_NOT_PASS**. These are 16 independently opened full PNGs from this campaign,
not a claim that every retained PNG or mandatory VIS07 case was manually reviewed.
All 404 captured full PNGs and styled grids remain available, with their original
versions, paths, clocks, footer/token labels, styles and cursors unmasked.

Paths below are relative to this immutable campaign directory. For every listed
`upstream` PNG, its independently opened `oc` counterpart is also listed/covered.

| Pair | Original PNG | Native PNG | Actual observation |
|---|---|---|---|
| Cold 80-column chip | `chip-80/upstream/leader-chip-normal.png` | `chip-80/oc/leader-chip-normal.png` | Both show the actual bold amber three-line chip wrapping onto the next composer row; the cursor is after the chip. No prior cold-frame cells/PNG race appears in this pair. |
| Normal 120-column chip | `chip-120/upstream/leader-chip-normal.png` | `chip-120/oc/leader-chip-normal.png` | Both show the same wrapped chip, normal light draft text and blue stripe/agent label. |
| Pending 120-column chip | `chip-120/upstream/leader-chip-pending.png` | `chip-120/oc/leader-chip-pending.png` | Surrounding draft/stripe/agent text is visibly dimmed; bold amber chip and caret remain. |
| Restored 120-column chip | `chip-120/upstream/leader-chip-restored.png` | `chip-120/oc/leader-chip-restored.png` | Normal draft/stripe colors return with the same chip and caret. |
| Actual mouse expansion | `chip-120/upstream/leader-chip-expanded-fulltext.png` | `chip-120/oc/leader-chip-expanded-fulltext.png` | Both expose `VIS11-PASTE-0`, `VIS11-PASTE-1`, `VIS11-PASTE-2`; no chip label remains. Full comparator retains the blank-cell style and version differences. |
| Longdraft pending Enter | `extra-120/upstream/leader-extra-enter-pending.png` | `extra-120/oc/leader-extra-enter-pending.png` | Actual wrapped raw draft is dim, with the caret in the same visible position on both binaries. |
| Real suffix-space false-repeat | `suffix-space-120/upstream/leader-suffix-repeat-two-chips.png` | `suffix-space-120/oc/leader-suffix-repeat-two-chips.png` | Both contain two actual amber chips separated by the ordinary suffix space. |
| Suffix submission completed/restored | `suffix-space-120/upstream/leader-repeat-enter-restored.png` | `suffix-space-120/oc/leader-repeat-enter-restored.png` | Both show the two expanded copies in the real user message, fixture answer, and empty composer. The original's double separator versus native's single separator is visible; duration remains 100 ms versus 127 ms. No panic overlay is present. |

Version `2.0.12` versus `0.1.0` remains visible on home captures. Completed-session
raw spacing, duration and footer/context differences remain in full comparison
scope. Exact wire input and provider completion are independently recorded in
`protocol.json` and `prompt-paste-analysis.json`; visual completion is not used
as a substitute for those checks.

Native executable SHA-256:
`8f32da3b6a52db123ce6c8eb93108890812da81cd03915995a10b8d66c84f459`.
The complete source/build association is in `report.md` and each capture lock.
