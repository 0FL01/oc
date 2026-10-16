# Independent full-PNG sample review — attempt 03

Opened the following 16 complete, unmasked terminal PNGs independently of the
automated comparator reports. Paths are relative to this immutable campaign.
This is a sample review; it does not claim manual review of every capture or
qualification of the mandatory VIS07 matrix.

| Complete PNGs opened | Observation |
|---|---|
| `chip-80/{upstream,oc}/leader-chip-normal.png` | Both show the actual bold amber chip wrapped between `~3` and `lines]`, with the caret after it. The former upstream cold-frame race is absent in these sealed frames. Version text remains different. |
| `chip-120/{upstream,oc}/leader-chip-normal.png` | Both show the actual chip with the same wrap and visible caret. |
| `chip-120/{upstream,oc}/leader-chip-pending.png` | Surrounding raw draft, agent label and stripe become muted; the bold amber chip retains its own colors. Wrap and caret remain visually unchanged. |
| `chip-120/{upstream,oc}/leader-chip-restored.png` | Original draft/agent/stripe colors return; chip, wrap and caret remain visible. |
| `chip-120/{upstream,oc}/leader-chip-expanded-fulltext.png` | Actual mouse expansion displays all three original lines in both binaries. Complete version/style differences remain in comparator scope. |
| `extra-120/{upstream,oc}/leader-extra-enter-pending.png` | Same real wrapped longdraft, pending muted colors and caret inside the trailing `ord`, before submission. |
| `suffix-space-120/{upstream,oc}/leader-suffix-repeat-two-chips.png` | Both show two actual separate bold chips after an ordinary typed suffix space and repeated bracketed paste. |
| `suffix-space-120/{upstream,oc}/leader-repeat-enter-restored.png` | Both show the completed assistant answer and an empty restored composer. Native has no panic overlay. The original's additional separator space is visible in the submitted user text; duration/footer differences remain visible. |

The paired original/native suffix restored PNGs show actual different durations
(134 ms versus 140 ms) and a different raw separator. These are observed
differences, not normalized fields or acceptance exceptions. Exact raw input,
resolved cell colors, styled blanks, all cursor samples, full frame/pixel counts
and differing rows are retained in `prompt-paste-analysis.json`, `summary.json`
and the per-run sealed capture/comparator files.
