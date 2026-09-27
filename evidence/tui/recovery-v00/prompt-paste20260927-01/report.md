# VIS07 prompt/paste fresh-source diagnostic — 2026-09-27

**Result: BOUNDED_BEHAVIOR_VERIFIED_WITH_WIRE_DIFFERENCE; VIS07_NOT_PASS.**
Every comparable full grid and PNG differs. Behavioral observations do not qualify
all mandatory VIS07 frames or cases.

## Captured behavior

- Four widths, 79/80/120/121×40: the existing default leader sequence captures the
  same actual three-line chip in normal/pending/restored states and actual mouse
  click expansion. Original and native both expose all three original lines;
  both retain bold chip labels and symbols/caret through pending/restored. No
  provider request occurs in any default-chip pair.
- The unchanged existing extra sequence captures the real wrapped longdraft
  pending-leader Enter, next and restored states at all four widths. One main
  Responses request and one auxiliary title occur per side, and all eight main
  wires contain the exact same user draft:

  `VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord`
- Final repeated-paste/navigation attempt `repeat-nav-120-03`: same three-line
  paste repeated at chip end expands without a second insertion. Three actual
  Up keys and three Down keys move the visible caret and return to its original
  position on both binaries. Pending-leader Enter completes the real local
  fixture exchange with two valid requests per side.
- Exact repeated-paste wires differ: OC2 retains one extra trailing ASCII space
  after `VIS11-PASTE-2`; native retains only the original pasted input. The
  donor inserts that space outside the chip extmark at
  `opencode/packages/tui/src/component/prompt/index.tsx:1393`; expansion
  replaces only the extmark at `:1422–1424`. Actual full inputs are preserved
  in `protocol.json` and `prompt-paste-analysis.json`. This is an observed
  difference, not an acceptance exception.

## Counts and failures

Current qualification selection (four chip, four extra, final navigation pair):
374 full per-side captures;
187 grid and
187 PNG comparisons DIFFERENT;
16 paired cursor differences.

Including both retained navigation failures:
418 full captures,
208 DIFFERENT + 2 BLOCKED grid comparisons,
208 DIFFERENT + 2 BLOCKED PNG comparisons,
16 cursor differences. Zero EQUAL comparisons;
0 unstable captures.

| Run | Full captures (both sides) | Grids different/blocked | PNGs different/blocked | Cursor differences | Requests original/native | Behavior original/native |
|---|---:|---:|---:|---:|---:|---|
| chip-120 | 52 | 26/0 | 26/0 | 0 | 0/0 | OBSERVED/OBSERVED |
| chip-121 | 52 | 26/0 | 26/0 | 4 | 0/0 | OBSERVED/OBSERVED |
| chip-79 | 52 | 26/0 | 26/0 | 4 | 0/0 | OBSERVED/OBSERVED |
| chip-80 | 52 | 26/0 | 26/0 | 0 | 0/0 | OBSERVED/OBSERVED |
| extra-120 | 36 | 18/0 | 18/0 | 0 | 2/2 | OBSERVED/OBSERVED |
| extra-121 | 36 | 18/0 | 18/0 | 4 | 2/2 | OBSERVED/OBSERVED |
| extra-79 | 36 | 18/0 | 18/0 | 4 | 2/2 | OBSERVED/OBSERVED |
| extra-80 | 36 | 18/0 | 18/0 | 0 | 2/2 | OBSERVED/OBSERVED |
| repeat-nav-120 | 22 | 11/0 | 11/0 | 0 | 2/2 | FAIL/FAIL |
| repeat-nav-120-02 | 22 | 10/2 | 10/2 | 0 | 2/2 | FAIL/OBSERVED |
| repeat-nav-120-03 | 22 | 11/0 | 11/0 | 0 | 2/2 | OBSERVED/OBSERVED |

All requests are to the isolated local fake provider: 14
original + 14 native = 28
(14 main, 14 auxiliary title), including
3 rejected requests retained from the first two navigation
attempts. Each individual side/run made 0 or 2 requests. Live/remote requests: 0.
Current selected requests: 20, all valid.

The first navigation fixture wrongly searched literal newlines in serialized
JSON, rejecting both main requests. The second corrected that and completed
natively, while rejecting OC2's additional trailing space. The third explicitly
accepts each observed exact wire and still reports their mismatch. No earlier
capture is overwritten. `campaign-interruption.md` records the helper stop
before the final extra pair when another Cargo release build was detected; the
remaining pair was launched only after Cargo became idle. No unknown external
effect was retried.

## Unmasked full-frame differences

All styled cells, resolved colors/modifiers/widths, cursor, text, PNG geometry and
pixels remain in comparator scope. Version 2.0.12 versus 0.1.0 remains visible;
no version/footer/duration/title/token/path normalization or masking is used.
The analyzer records every differing row and per-field counts, not just the
comparator's first 20 sample coordinates. In particular native completed-session
footer context/tokens versus donor hints, modal cursor/blank styling and original
failure overlays remain in evidence.

| Current key grid | Differing cells (full frame) | Cursor differs |
|---|---:|---|
| chip-120/leader-chip-normal | 6 | false |
| chip-120/leader-chip-pending | 6 | false |
| chip-120/leader-chip-restored | 6 | false |
| chip-120/leader-chip-expanded-fulltext | 7 | false |
| chip-121/leader-chip-normal | 6 | false |
| chip-121/leader-chip-pending | 6 | false |
| chip-121/leader-chip-restored | 6 | false |
| chip-121/leader-chip-expanded-fulltext | 7 | false |
| chip-79/leader-chip-normal | 6 | false |
| chip-79/leader-chip-pending | 6 | false |
| chip-79/leader-chip-restored | 6 | false |
| chip-79/leader-chip-expanded-fulltext | 7 | false |
| chip-80/leader-chip-normal | 6 | false |
| chip-80/leader-chip-pending | 6 | false |
| chip-80/leader-chip-restored | 6 | false |
| chip-80/leader-chip-expanded-fulltext | 7 | false |
| extra-120/leader-extra-enter-pending | 6 | false |
| extra-120/leader-extra-enter-next | 5 | false |
| extra-120/leader-extra-enter-restored | 5 | false |
| extra-121/leader-extra-enter-pending | 6 | false |
| extra-121/leader-extra-enter-next | 5 | false |
| extra-121/leader-extra-enter-restored | 5 | false |
| extra-79/leader-extra-enter-pending | 6 | false |
| extra-79/leader-extra-enter-next | 6 | false |
| extra-79/leader-extra-enter-restored | 6 | false |
| extra-80/leader-extra-enter-pending | 6 | false |
| extra-80/leader-extra-enter-next | 6 | false |
| extra-80/leader-extra-enter-restored | 6 | false |
| repeat-nav-120-03/leader-repeat-expanded | 7 | false |
| repeat-nav-120-03/leader-repeat-enter-restored | 6 | false |

Representative full PNGs independently opened for visual inspection:
`chip-120/{upstream,oc}/leader-chip-normal.png` and
`chip-120/oc/leader-chip-expanded-fulltext.png`. The chip wraps/bold treatment
and actual expanded text are visible; this sample inspection does not replace
the full comparators or mandatory-case qualification.

## Exact source/build association

Pinned original commit `2670273ff17da96f85c5826ced57aa1b368754fa`, executable
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

Captured native HEAD `5a2ec11411ab4276d2e13cf4379065aadc64475b`, plus the uncommitted three-file TUI
patch sealed below. All 11 independent `--build-oc true` invocations ran
`cargo build --locked` successfully. Every run used
`/home/opencode/ai/oc/target/debug/oc` SHA-256
`640edbd02839af8d3557e674985f80681718015b0d962ce62a7ce47ef0d6225c`, with one Rust input-set digest
`b545f31f6cb8ba7468cc8a120b062a85d9281d9225cfde45b60f166ce1ad71b4` (sorted Rust manifest entries, JSON SHA-256).
Source inputs matched before/after every build and capture. HEAD alone is not
claimed to contain the dirty TUI fix. Complete dirty diffs are in each
`commands.json`; complete source manifests are retained per run.

| Dirty TUI input | Captured SHA-256 | Matches current source |
|---|---|---|
| `crates/oc-tui/src/app.rs` | `a3791bd37e09c25756c687b3bf4581a4878a441e4fb04db6f753c2ce27efd6f9` | true |
| `crates/oc-tui/src/editor.rs` | `0fee05cb6b1d4aaf1ec4c82e7790ba9f0251287729aad7791d5426ad547c2d20` | true |
| `crates/oc-tui/src/shell.rs` | `8ed45964899c064760b1bea42e6a2a9bfd33281f0b97cf8d4ef498331ae622ae` | true |

Tooling revisions differ only across the diagnosed fixture corrections; the full
manifest and dirty-diff digests distinguish them explicitly:

| Run | Complete canonical source-manifest SHA-256 | Dirty-diff SHA-256 |
|---|---|---|
| chip-120 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| chip-121 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| chip-79 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| chip-80 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| extra-120 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| extra-121 | `339aff64f089c5dda87010fd7f308079b51458aeecf7a4815fbe8d5c47a5f8f9` | `3d1157809a50c365427062d56396ba162bbc92b3b93f217f214a6e22e973208e` |
| extra-79 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| extra-80 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| repeat-nav-120 | `1fd2c53b66bd3f42e1d1316987e534570a66077210cdfe97c52d8b8750765ac3` | `3231675ea14bb0ea497f6ce3d6b7455d7b16e46b2bd2e16819945f3b6d312303` |
| repeat-nav-120-02 | `339aff64f089c5dda87010fd7f308079b51458aeecf7a4815fbe8d5c47a5f8f9` | `3d1157809a50c365427062d56396ba162bbc92b3b93f217f214a6e22e973208e` |
| repeat-nav-120-03 | `fffaee74eca10e51820e2a80b6baef61100349dddfe6aad0554be7c1de04e1f5` | `69125813f117f1c36c56995851beae92e810e6031ecfc15c070f52ba35eaba20` |

Shared capture profile: xterm.js/Chromium/DejaVu Sans Mono, Unicode 11, truecolor,
40 rows; sidebar hidden, animations disabled, real wall clock, both actual CLI
configs set `session.tps=false`. All launch versions/configuration, tooling
lock, build/bridge/comparator commands and exits, input events, raw VT and output
timelines remain per attempt. Capture shutdown uses the existing bridge's owned
process teardown; this is not a new graceful-exit/terminal-restoration gate.

## Checks and continuation

- JS syntax and Python AST checks passed before execution.
- Actual `check_frontend.mjs` and `check_capture_geometry.mjs` passed.
- Dedicated `analyze_prompt_paste.mjs` exited 0: no current behavioral or
  provenance failures; retained failures and wire mismatch remain explicit.
- Capture runners all exited 1, preserving full-frame differences and the
  earlier rejected fixture requests. No Rust, acceptance or task-status changes
  were made by this script/evidence work. Historical analyzer/evidence intact.
- Continue from current `VIS07_NOT_PASS`: observed trailing raw spacer,
  full-frame version/footer/modal differences and the remaining mandatory
  prompt/paste/Unicode/resize cases require qualification before any PASS claim.

Reproduce with a fresh campaign path:

`node scripts/tui_capture/run_prompt_paste.mjs NEW_CAMPAIGN`

`node scripts/tui_capture/analyze_prompt_paste.mjs NEW_CAMPAIGN`

`node scripts/tui_capture/summarize_prompt_paste.mjs NEW_CAMPAIGN`

Capture provenance and observations are in `prompt-paste-analysis.json`;
counts/key-frame/source association are in `summary.json`.
Evidence size before closeout: 156608165 bytes.
