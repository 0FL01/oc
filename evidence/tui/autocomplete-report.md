# T44 VIS25 — real PTY slash-autocomplete probe

`scripts/tui_capture/capture.mjs --autocomplete true` ran the pinned original
(`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
source `2670273ff17da96f85c5826ced57aa1b368754fa`) and existing native
`target/debug/oc` (`c4a9f099ceee41ea7dd3ec3664fc03e3bdca60039d17a65f9d48b8ca3b9d065c`)
in isolated PTYs. Native source HEAD was `35cd290` with uncommitted test-only
capture changes; the runner records a source manifest but, without
`--build-oc true`, does **not** attest binary-to-source build provenance.
Both used the Reader/tools 120×40 truecolor profile, fixture SHA-256
`16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`
and profile ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`.

Command: `node scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 --autocomplete true --output /home/opencode/ai/oc/evidence/tui/autocomplete-20260924-03`.
Exit **1** (all six full-frame comparisons differ); `capture.lock.json` lists
the separate grid and PNG comparator exit codes and locked input hashes.
Each side's `autocomplete-checks.json`, `inputs.json`, `protocol.json`, raw VT,
six named `.cells.json` / `.png` / `.vt` frames and diff JSON are in
`evidence/tui/autocomplete-20260924-03/`. Both sides have `provider_contract=true`
and no provider requests during the autocomplete keystrokes; the intervening
real fixture read completes.

| Frame | Grid different / 4800 cells; cursor | PNG different / 647040 pixels | Observed difference |
| --- | --- | --- | --- |
| Home `/` | 709; same | 20862 | Original starts `/agents`, `/btw`, `/cd`; native starts `/agent`, `/agents`, `/cards`; both display menus. |
| Home `/ren` | 230; same | 23298 | Original lists `/reload`, `/review`, `/btw`, no `/rename`; native lists `/rename`, `/dcp-compress`. |
| Home after Tab | 154; different | 14367 | Original executes highlighted `/reload`, clears draft and displays **Configuration reloaded**; native inserts `/rename ` with caret after space. No model request on either side. |
| Session `/` | 1109; same | 15167 | Both display command menus, but option inventory and painted rows differ. |
| Session `/ren` | 813; same | 96560 | Original has seven visible options (`/rename`, `/reload`, `/review`, `/share`, `/fork`, `/export`, `/btw`); native has two (`/rename`, `/dcp-compress`). |
| Session after Tab | 2; same | 136 | Both insert `/rename ` and hide the menu; remaining difference is the genuine elapsed-time glyphs at grid x38–39,y10 (PNG bbox x320–336,y163–172). |

All six comparator statuses are **DIFFERENT**; same fixture/profile and
record-only predicates do not make VIS25 PASS. This probe does not exercise
Up/Down/Enter/Esc, workspace commands or empty-filter behavior. The original
Home `/ren` outcome is a meaningful behavioral mismatch, not a failed capture.
Earlier immutable `autocomplete-20260924-01/` (first draft-menu predicate
incorrectly marked menus absent) and `autocomplete-20260924-02/` (capture of
Home stages, then clearing predicate timed out because the visible placeholder
is text) remain retained; attempt `-03` fixes those runner observations. Its
historical `expected_draft` and after-Tab `query_visible` fields describe a
candidate `/rename ` completion and evaluate false on upstream Home, **not**
VIS25 pass criteria. The subsequent runner polish records after-Tab outcomes
without that candidate expectation; immutable `-03` hashes and results are
not retroactively changed.
