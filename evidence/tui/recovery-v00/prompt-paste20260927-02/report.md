# VIS07 prompt/paste fresh-source diagnostic — 2026-09-27

**Result: BOUNDED_BEHAVIOR_FAILED; VIS07_NOT_PASS.**
Every comparable full grid and PNG differs. Behavioral observations do not qualify
all mandatory VIS07 frames or cases.

## Captured behavior

- Four widths, 79/80/120/121×40: the existing default leader sequence captures the
  same actual three-line chip in normal/pending/restored states and actual mouse
  click expansion. Expansion checks expose all three original lines. Normal /
  pending / restored symbol, bold and cursor checks are listed below; any
  unstable or nonmatching state is retained as a failure. No
  provider request occurs in any default-chip pair.
- The unchanged existing extra sequence captures the real wrapped longdraft
  pending-leader Enter, next and restored states at all four widths. One main
  Responses request and one auxiliary title occur per side, and all eight main
  wires contain the exact same user draft:

  `VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord`
- Current repeated-paste/navigation attempt `repeat-nav-120`: same three-line
  paste repeated at chip end expands without a second insertion. Three actual
  Up keys and three Down keys move the visible caret and return to its original
  position on both binaries. Pending-leader Enter completes the real local
  fixture exchange with two valid requests per side.
- Optional `suffix-space-120` uses the same three-line paste, but inserts one
  real ordinary suffix space before repeating. The actual two-chip observation,
  both mouse expansions, visual navigation and exact two-paste wire are checked
  independently. Original's chip spacers remain additional raw input bytes.
  Submission/completion is reported separately and is not inferred from a
  successful pre-submit two-chip observation.
- Exact repeated-paste wires differ: OC2 retains one extra trailing ASCII space
  after `VIS11-PASTE-2`; native retains only the original pasted input. The
  donor inserts that space outside the chip extmark at
  `opencode/packages/tui/src/component/prompt/index.tsx:1393`; expansion
  replaces only the extmark at `:1422–1424`. Actual full inputs are preserved
   in `protocol.json` and `prompt-paste-analysis-v02.json`. This is an observed
  difference, not an acceptance exception.

## Counts and failures

Current diagnostic selection (10 original/native pairs):
404 full per-side captures;
201 grid and
201 PNG comparisons DIFFERENT;
18 paired cursor differences.

All retained attempts (0 older attempts excluded from the current selection):
404 full captures,
201 DIFFERENT + 2 BLOCKED grid comparisons,
201 DIFFERENT + 2 BLOCKED PNG comparisons,
18 cursor differences. Zero EQUAL comparisons;
1 unstable captures.

| Run | Full captures (both sides) | Grids different/blocked | PNGs different/blocked | Cursor differences | Requests original/native | Behavior original/native |
|---|---:|---:|---:|---:|---:|---|
| chip-120 | 52 | 26/0 | 26/0 | 0 | 0/0 | OBSERVED/OBSERVED |
| chip-121 | 52 | 26/0 | 26/0 | 4 | 0/0 | OBSERVED/OBSERVED |
| chip-79 | 52 | 26/0 | 26/0 | 4 | 0/0 | OBSERVED/OBSERVED |
| chip-80 | 52 | 26/0 | 26/0 | 1 | 0/0 | OBSERVED/OBSERVED |
| extra-120 | 36 | 18/0 | 18/0 | 0 | 2/2 | OBSERVED/OBSERVED |
| extra-121 | 36 | 18/0 | 18/0 | 4 | 2/2 | OBSERVED/OBSERVED |
| extra-79 | 36 | 18/0 | 18/0 | 4 | 2/2 | OBSERVED/OBSERVED |
| extra-80 | 36 | 18/0 | 18/0 | 0 | 2/2 | OBSERVED/OBSERVED |
| repeat-nav-120 | 22 | 11/0 | 11/0 | 0 | 2/2 | OBSERVED/OBSERVED |
| suffix-space-120 | 30 | 14/2 | 14/2 | 1 | 2/1 | OBSERVED/FAIL |

All requests are to the isolated local fake provider: 12
original + 11 native = 23
(12 main, 11 auxiliary title), including
0 rejected requests. The bounded contract is 0 or 2 requests
per side/run; actual counts, including any crash-shortened run, are listed above.
Live/remote requests: 0.
Current selected requests: 23, all valid.

- chip-80: upstream chip normal/pending/restored preserve symbols/cursor: undefined
- chip-80: upstream chip label observed bold in all three states: {"chip-normal":[],"chip-pending":[{"x":61,"y":21,"cell":{"symbol":"[","fg":"#0a0a0a","bg":"#f5a742","width":1,"modifiers":["bold"]}}],"chip-restored":[{"x":61,"y":21,"cell":{"symbol":"[","fg":"#0a0a0a","bg":"#f5a742","width":1,"modifiers":["bold"]}}]}
- suffix-space-120: oc observed scenario: "Error: Bridge exited 0 waiting for repeat paste actual completion"
- suffix-space-120: oc valid bounded zero-or-two requests: [{"operation":"transcript","valid":true}]
- suffix-space-120: oc one main plus one title: 1
- chip-80: upstream/leader-chip-normal stable geometry: undefined
Retained-attempt failed checks: 0.
The fixture accepts the explicitly expected original/native wires separately;
their actual mismatch is retained in full. No prior capture is overwritten.
Runner exits, interruption/blocker information and exact invocations are in
`campaign.json` and per-run logs; this report does not infer a successful
capture from an expected comparison exit 1.

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
| chip-80/leader-chip-normal | 288 | true |
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
| repeat-nav-120/leader-repeat-expanded | 7 | false |
| repeat-nav-120/leader-repeat-enter-pending | 7 | false |
| repeat-nav-120/leader-repeat-enter-restored | 6 | false |
| suffix-space-120/leader-suffix-repeat-two-chips | 6 | false |
| suffix-space-120/leader-suffix-chip-expanded-1 | 25 | false |
| suffix-space-120/leader-suffix-chip-expanded-2 | 20 | false |
| suffix-space-120/leader-repeat-expanded | 20 | false |
| suffix-space-120/leader-repeat-enter-pending | 20 | false |
| suffix-space-120/leader-repeat-enter-restored | blocked | blocked |

## Actual pending RGB and cursor

The compact table lists observed foregrounds on actual draft/chip rows. Full
coordinates, RGB foreground/background and modifiers remain in
`prompt-paste-analysis-v02.json` and `summary.json` (`pending_rgb`); resolved
styled cells are retained without masking. Bold chips can retain their own
foreground while the surrounding raw draft changes color on pending leader.

| Run/side/state | Actual cursor | Observed draft-row foregrounds |
|---|---|---|
| chip-120/upstream/chip-normal | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-120/upstream/chip-pending | {"x":33,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-120/upstream/chip-restored | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-120/oc/chip-normal | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-120/oc/chip-pending | {"x":33,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-120/oc/chip-restored | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-121/upstream/chip-normal | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-121/upstream/chip-pending | {"x":33,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-121/upstream/chip-restored | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-121/oc/chip-normal | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-121/oc/chip-pending | {"x":33,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-121/oc/chip-restored | {"x":33,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-79/upstream/chip-normal | {"x":12,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-79/upstream/chip-pending | {"x":12,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-79/upstream/chip-restored | {"x":12,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-79/oc/chip-normal | {"x":12,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-79/oc/chip-pending | {"x":12,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-79/oc/chip-restored | {"x":12,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-80/upstream/chip-normal | {"x":61,"y":21,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee |
| chip-80/upstream/chip-pending | {"x":13,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-80/upstream/chip-restored | {"x":13,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-80/oc/chip-normal | {"x":13,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| chip-80/oc/chip-pending | {"x":13,"y":22,"visible":true,"shape":"block"} | #484848, #808080, #0a0a0a |
| chip-80/oc/chip-restored | {"x":13,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
| extra-120/upstream/extra-enter-pending | {"x":40,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| extra-120/oc/extra-enter-pending | {"x":40,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| extra-121/upstream/extra-enter-pending | {"x":40,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| extra-121/oc/extra-enter-pending | {"x":40,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| extra-79/upstream/extra-enter-pending | {"x":19,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| extra-79/oc/extra-enter-pending | {"x":19,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| extra-80/upstream/extra-enter-pending | {"x":20,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| extra-80/oc/extra-enter-pending | {"x":20,"y":22,"visible":true,"shape":"block"} | #484848, #808080 |
| repeat-nav-120/upstream/repeat-expanded | {"x":39,"y":23,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee |
| repeat-nav-120/upstream/repeat-enter-pending | {"x":39,"y":23,"visible":true,"shape":"block"} | #484848, #808080 |
| repeat-nav-120/oc/repeat-expanded | {"x":39,"y":23,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee |
| repeat-nav-120/oc/repeat-enter-pending | {"x":39,"y":23,"visible":true,"shape":"block"} | #484848, #808080 |
| suffix-space-120/upstream/repeat-expanded | {"x":39,"y":24,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee |
| suffix-space-120/upstream/repeat-enter-pending | {"x":39,"y":24,"visible":true,"shape":"block"} | #484848, #808080 |
| suffix-space-120/oc/repeat-expanded | {"x":39,"y":24,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee |
| suffix-space-120/oc/repeat-enter-pending | {"x":39,"y":24,"visible":true,"shape":"block"} | #484848, #808080 |

Full PNG review, when performed, is separately recorded in `visual-review.md`;
the automated full comparators do not substitute for independent visual review.

## Exact source/build association

Pinned original commit `2670273ff17da96f85c5826ced57aa1b368754fa`, executable
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

Captured native HEAD `5a2ec11411ab4276d2e13cf4379065aadc64475b`, plus the uncommitted four-file TUI
patch sealed below. All 10 independent `--build-oc true` invocations ran
`cargo build --locked` successfully. Every run used
`/home/opencode/ai/oc/target/debug/oc` SHA-256
`1510bff437729ee4a16f1237619f94389f1ce75893a26e057ed3e528b3449d58`, with one Rust input-set digest
`354f43a355c3aa4f04a88b6582770a099371e1343290a8db7c19caf4eb37d964` (sorted Rust manifest entries, JSON SHA-256).
Source inputs matched before/after every build and capture. HEAD alone is not
claimed to contain the dirty TUI fix. Complete dirty diffs are in each
`commands.json`; complete source manifests are retained per run.

| Dirty TUI input | Captured SHA-256 | Matches current source |
|---|---|---|
| `crates/oc-tui/src/app.rs` | `dfba81c27438d1cf1c133974bcc8481f4b8b34f6946cd0bc4df5f66d8f90e157` | true |
| `crates/oc-tui/src/approval_view.rs` | `8936b7b4d6c55d424de6d1194ea784d54ed4f5ee0a3d334f6e867f47b20df711` | true |
| `crates/oc-tui/src/editor.rs` | `d39c42ce8257997d57bc5bc0943473c557d13c60548ae37329302f7374d75419` | true |
| `crates/oc-tui/src/shell.rs` | `8ed45964899c064760b1bea42e6a2a9bfd33281f0b97cf8d4ef498331ae622ae` | true |

Complete source manifests seal the capture/probe/analyzer tooling as well as
Rust; these digests distinguish every actual input set explicitly:

| Run | Complete canonical source-manifest SHA-256 | Dirty-diff SHA-256 |
|---|---|---|
| chip-120 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| chip-121 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| chip-79 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| chip-80 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| extra-120 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| extra-121 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| extra-79 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| extra-80 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| repeat-nav-120 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |
| suffix-space-120 | `a45a5977e1b2c7c98856c8ef4ada9b9291ed6941e9342d3824c26fc4a62ca865` | `8c812501a0bd2c7920463a5138217cdb7729daeb90a40c2ca0b237eac01616b1` |

Shared capture profile: xterm.js/Chromium/DejaVu Sans Mono, Unicode 11, truecolor,
40 rows; sidebar hidden, animations disabled, real wall clock, both actual CLI
configs set `session.tps=false`. All launch versions/configuration, tooling
lock, build/bridge/comparator commands and exits, input events, raw VT and output
timelines remain per attempt. Capture shutdown uses the existing bridge's owned
process teardown; this is not a new graceful-exit/terminal-restoration gate.

## Checks and continuation

- JS syntax and Python AST checks passed before execution.
- Actual `check_frontend.mjs` and `check_capture_geometry.mjs` passed.
- Dedicated analyzer current behavioral failures: 5;
  provenance failures: 1. Retained failures
  and actual wire mismatches remain explicit.
- Capture runner exits remain in `campaign.json`, preserving full-frame
  differences and any rejected fixture requests. No Rust, acceptance or task-status changes
  were made by this script/evidence work. Historical analyzer/evidence intact.
- Continue from current `VIS07_NOT_PASS`: observed trailing raw spacer,
  full-frame version/footer/modal differences and the remaining mandatory
  prompt/paste/Unicode/resize cases require qualification before any PASS claim.

Reproduce with a fresh campaign path:

`node scripts/tui_capture/run_prompt_paste.mjs NEW_CAMPAIGN`

`node scripts/tui_capture/analyze_prompt_paste.mjs NEW_CAMPAIGN`

`node scripts/tui_capture/summarize_prompt_paste.mjs NEW_CAMPAIGN`

Capture provenance and observations are in `prompt-paste-analysis-v02.json`;
counts/key-frame/source association are in `summary.json`.
Evidence size before closeout: 151935480 bytes.
