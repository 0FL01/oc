# VIS07 prompt/paste fresh-source diagnostic — 2026-09-27

**Result: BOUNDED_BEHAVIOR_VERIFIED_WITH_WIRE_DIFFERENCE; VIS07_NOT_PASS.**
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
   in `protocol.json` and `prompt-paste-analysis.json`. This is an observed
   remaining unapproved parity gap, not an acceptance waiver. In the suffix-space
   case the original also keeps one additional separator space between the two
   expanded copies, as well as its final trailing space.

## Counts and failures

Current diagnostic selection (10 original/native pairs):
404 full per-side captures;
202 grid and
202 PNG comparisons DIFFERENT;
16 paired cursor differences.

All retained attempts (0 older attempts excluded from the current selection):
404 full captures,
202 DIFFERENT + 0 BLOCKED grid comparisons,
202 DIFFERENT + 0 BLOCKED PNG comparisons,
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
| repeat-nav-120 | 22 | 11/0 | 11/0 | 0 | 2/2 | OBSERVED/OBSERVED |
| suffix-space-120 | 30 | 15/0 | 15/0 | 0 | 2/2 | OBSERVED/OBSERVED |

All requests are to the isolated local fake provider: 12
original + 12 native = 24
(12 main, 12 auxiliary title), including
0 rejected requests. The bounded contract is 0 or 2 requests
per side/run; actual counts, including any crash-shortened run, are listed above.
Live/remote requests: 0.
Current selected requests: 24, all valid.

No current behavioral or provenance checks failed.
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
| chip-80/leader-chip-normal | 6 | false |
| chip-80/leader-chip-pending | 6 | false |
| chip-80/leader-chip-restored | 6 | false |
| chip-80/leader-chip-expanded-fulltext | 7 | false |
| extra-120/leader-extra-enter-pending | 6 | false |
| extra-120/leader-extra-enter-next | 1 | false |
| extra-120/leader-extra-enter-restored | 1 | false |
| extra-121/leader-extra-enter-pending | 6 | false |
| extra-121/leader-extra-enter-next | 5 | false |
| extra-121/leader-extra-enter-restored | 5 | false |
| extra-79/leader-extra-enter-pending | 6 | false |
| extra-79/leader-extra-enter-next | 2 | false |
| extra-79/leader-extra-enter-restored | 2 | false |
| extra-80/leader-extra-enter-pending | 6 | false |
| extra-80/leader-extra-enter-next | 6 | false |
| extra-80/leader-extra-enter-restored | 6 | false |
| repeat-nav-120/leader-repeat-expanded | 7 | false |
| repeat-nav-120/leader-repeat-enter-pending | 7 | false |
| repeat-nav-120/leader-repeat-enter-restored | 3 | false |
| suffix-space-120/leader-suffix-repeat-two-chips | 6 | false |
| suffix-space-120/leader-suffix-chip-expanded-1 | 25 | false |
| suffix-space-120/leader-suffix-chip-expanded-2 | 20 | false |
| suffix-space-120/leader-repeat-expanded | 20 | false |
| suffix-space-120/leader-repeat-enter-pending | 20 | false |
| suffix-space-120/leader-repeat-enter-restored | 16 | false |

## Actual pending RGB and cursor

The compact table lists observed foregrounds on actual draft/chip rows. Full
coordinates, RGB foreground/background and modifiers remain in
`prompt-paste-analysis.json` and `summary.json` (`pending_rgb`); resolved
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
| chip-80/upstream/chip-normal | {"x":13,"y":22,"visible":true,"shape":"block"} | #5c9cf5, #eeeeee, #0a0a0a |
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

Captured native HEAD(s): `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2`, plus
the uncommitted build/runtime/test-fixture inputs sealed below. All
10 independent `--build-oc true` invocations are checked
for successful `cargo build --locked`; failures are listed above. Every run used
`/home/opencode/ai/oc/target/debug/oc` SHA-256
`8f32da3b6a52db123ce6c8eb93108890812da81cd03915995a10b8d66c84f459`, with one Rust input-set digest
`b142801c2c34b75f4f8a7b0b352ca08c78c0f86a7d2e51e4a52dd72c64f18f5a` (sorted Rust manifest entries, JSON SHA-256).
Source inputs matched before/after every build and capture. HEAD alone is not
claimed to contain the dirty changes. Complete dirty diffs are in each
`commands.json`; complete source manifests are retained per run.

| Build/runtime/test-fixture input | Captured SHA-256 | Matches current source |
|---|---|---|
| `Cargo.toml` | `e5e3d488f5381f9b7452d51981a42096fc370f5e2f074c56c37e525210b9e76a` | true |
| `Cargo.lock` | `8d8718e1966bf60373a514d2a5b4bdfa516c8e11b054d9c09373a28684a4453d` | true |
| `rust-toolchain.toml` | `b364b7e44eff5aaa6f8f8357d2536dd284fe5af3c3a64eea4f6d0555d79c2c7a` | true |
| `crates/oc/Cargo.toml` | `d06d1a6ae71b91640ec804d38b3fdcb868aa8294d14f0d2454b2c86ea070f140` | true |
| `crates/oc-tui/src/app.rs` | `477210f63c6a045e512427f96ed459eb7c894af1948d8632e442c8432c04d42c` | true |
| `crates/oc-tui/src/approval_view.rs` | `8936b7b4d6c55d424de6d1194ea784d54ed4f5ee0a3d334f6e867f47b20df711` | true |
| `crates/oc-tui/src/editor.rs` | `d39c42ce8257997d57bc5bc0943473c557d13c60548ae37329302f7374d75419` | true |
| `crates/oc-tui/src/shell.rs` | `8ed45964899c064760b1bea42e6a2a9bfd33281f0b97cf8d4ef498331ae622ae` | true |
| `crates/oc/tests/pty.rs` | `5ccb72bc231a23f34edf82e6a1b8b55669910e11b137954b8c5b33aaa3e48a93` | true |
| `crates/oc/tests/pty_t39.rs` | `b564154523d14fca615d31e9c99ee6fbe714fd694866258717acad68472e27dd` | true |

Complete source manifests seal the capture/probe/analyzer tooling as well as
Rust; these digests distinguish every actual input set explicitly:

| Run | Captured HEAD | Complete canonical source-manifest SHA-256 | Dirty-diff SHA-256 |
|---|---|---|---|
| chip-120 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| chip-121 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| chip-79 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| chip-80 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| extra-120 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| extra-121 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| extra-79 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| extra-80 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| repeat-nav-120 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| suffix-space-120 | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |

Shared capture profile: xterm.js/Chromium/DejaVu Sans Mono, Unicode 11, truecolor,
40 rows; sidebar hidden, animations disabled, real wall clock, both actual CLI
configs set `session.tps=false`. All launch versions/configuration, tooling
lock, build/bridge/comparator commands and exits, input events, raw VT and output
timelines remain per attempt. Capture shutdown uses the existing bridge's owned
process teardown; this is not a new graceful-exit/terminal-restoration gate.

## Checks and continuation

- JS syntax and Python AST checks passed before execution.
- Actual `check_frontend.mjs` and `check_capture_geometry.mjs` passed.
- Dedicated analyzer current behavioral failures: 0;
  provenance failures: 0. Retained failures
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

Capture provenance and observations are in `prompt-paste-analysis.json`;
counts/key-frame/source association are in `summary.json`.
Evidence size before closeout: 150964244 bytes.
