# T44 — real caret ordering / prompt blink slice

## Result
FrameBackend stages one actual frame, defers Show until final input MoveTo and
uses advisory synchronized publication with safe unsupported fallback. First
background/resize/color/content share the boundary; partial/write/flush failures
never replay. Restore closes partial sync and restores visibility/modes. Read-only
panels have no composer caret; editable Search owns its caret and closes back to
the exact Unicode draft/position. No blink clock/permanent timer or dependency fork.
Receipt: evidence/tui/cursor-frame-presentation.md. T44 remains ACTIVE.
Implementation0becfcb09402e25eea0d3f66361d70c2d322156a was checked and PUSHED by
ordinary fast-forward to verified origin0FL01/oc, branchagent/oc-rust-port.

## Checks
Final2 fmt/strict locked workspace Clippy, workspace1731/0/11 across46 records,
normal debug/release/help and actual release startup/discovery PASS. Extra VIS31
idle/fairness/S07/AUD32 actual resource samples PASS with unchanged thresholds.
Pinned paired018–021 qualify six real input states at160×48default,80×24blink
fallback,120×40blink synchronized,80×24steady fallback: each blink state≥3 full
cycles, same-owner idle cadence, bounded sample gaps, no transient caret and exact
Search/composer restoration.576 full actual temporal PNGs audited;275 phase-matched
pairs/550 unmasked grid/PNG comparisons ALL DIFFERENT, not visual PASS. Normal
tool-preview013 requalifies resize/reopen/restart/no replay and read-only cards.

## Risks
No whole VIS16/VIS31/R6/V09/T44 PASS. DOM hover blink suppression and failed/heavy
observer attempts remain diagnostic. Original Search hover has continuous input,
not continuous repaint. Full temporal frames are actual renderer canvas and
explicitly unsettled, not settled/atomic screenshot claims. No image/cell mask,
baseline/cap/queue/permission weakening, RAW rewrite or tool replay. AUTH06 deferred;
original24/24/Go13/24 and unrelated .opencode/ untouched; no paid generation.

## Next
Functional slice and immutable current proofs are committed/pushed. Continue
Home's typed live MCP footer slot and every remaining frozen R1–R6/VIS01–VIS45
styled/temporal/interaction outcome; final current-source R6/V09 after full closure.
No task finish on this slice or an own-golden/final-cursor-only claim.
