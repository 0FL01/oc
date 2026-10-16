# T44 R4/V06 — paired two-turn spacing evidence

Immutable attempts: [`two-turn-20260925-01/`](two-turn-20260925-01/) and [`two-turn-20260925-02/`](two-turn-20260925-02/). The R4 acceptance contract is [`docs/goals/2026-09-21-tui-pixel-parity.md:45–48`](../../docs/goals/2026-09-21-tui-pixel-parity.md). Both runner invocations used `--geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 --two-turn true`; the second additionally used `--build-oc true`. Exact argv and comparator commands are in each attempt's `commands.json`. Both runners exited **1** because all full-frame comparators returned `DIFFERENT`, despite the two-turn protocol/spacing checks passing on both sides.

## Provenance and scope

Both [`capture.lock.json` files](two-turn-20260925-02/capture.lock.json) pin the same upstream source commit `2670273ff17da96f85c5826ced57aa1b368754fa`, reference executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` (opencode v2.0.12), fixture SHA-256 `2d73511167c58ab919f9a8f940118a55a88aede6e7bc480d8609599bdf2df3f2`, and environment ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. Profile: 120×40, `xterm-256color`/truecolor, `C.UTF-8`, dark `opencode`, requested hidden sidebar and horizontal tabs, xterm.js 6.0.0/Unicode 11, Chromium 145.0.7632.6, DejaVu Sans Mono 14 px, scale 1. No masks or frozen application clock were used; elapsed times are independent real wall-clock measurements.

- [`-01/capture.lock.json`](two-turn-20260925-01/capture.lock.json): native HEAD `bc6bac663f9069299a29be54f1807b66492f9acd`, tree `60bf03894374258817da5a7aa8f2325e874335a7`, dirty diff SHA-256 `a27f45abaaa47e5a328f5669d78954298cf99790a44121e3765454dbcd2816cb`, [`source-manifest.json`](two-turn-20260925-01/source-manifest.json) SHA-256 `5ec7e5035ee6054c0d1355f207f2cc485ec9ce0331b5f553a9c19beca8a9333e`; existing `oc 0.1.0` executable SHA-256 `4e0508879521e1ea748f0f9b180ed56fcf6a20ede212457b94f7dff82785a2b0`. **Its build/source association was not attested by this run.**
- [`-02/capture.lock.json`](two-turn-20260925-02/capture.lock.json): same native HEAD/tree, dirty diff SHA-256 `647d7be6f2c52523f18a6f19821e0b6a5aba95e214372347850aded525d55b9f`, [`source-manifest.json`](two-turn-20260925-02/source-manifest.json) SHA-256 `eaba51ed71759dc9d03c8f83ee28dee35516cc45be41e4ad0f4d1fff25539ea2`; `cargo build --locked` exited 0 in this attempt and produced `oc 0.1.0` executable SHA-256 `dd7db3e4b2368fb09cc03c2b8b4500a83b89a5a4b0e8765283902391a8b730d4`. HEAD alone does not identify the tested dirty source; the two attempts used different native source snapshots.

## Measured boundary and provider operations

Rows below are **blank rows** between the final first-answer text and its agent/model footer, between that footer and the top of the second user's shaded block, and between that footer and the second user's text, respectively. From each side's `two-turn-checks.json`:

| Attempt | Original | Native |
| --- | --- | --- |
| `-01`, existing native binary | `1, 1, 2` | `1, 0, 1` |
| `-02`, source-built native binary | `1, 1, 2` | `1, 1, 2` |

In `-02`, the first answer is at x5,y8 and footer y10 on both sides; the next user's block begins y12, its text x5,y13, and the second footer is y20. The unchanged original and newly built native thus agree at this **specific unwrapped, live two-turn boundary**. In each attempt, both sides' first-completed checks recorded two completed transcript requests and one completed title request; the final checks recorded **four completed transcript requests** (two real `read` tool rounds per turn), **one completed title request for the same session**, and zero invalid requests. Their `stable_capture`, `same_session_title`, `ordered_rows`, and `two_valid_read_roundtrips` predicates are true; see each `upstream/` and `oc/two-turn-checks.json` and `protocol.json`.

## Unmasked whole-frame comparisons

Each grid checks 4,800 styled cells with `cursor_differs=false`; each PNG compares 647,040 decoded RGBA pixels (1011×640). **Every entry is `FAIL`/`DIFFERENT`**, from the six `*.grid-diff.json`/`*.png-diff.json` files in each attempt:

| Attempt | Frame | Different cells / 4,800 | Different pixels / 647,040 |
| --- | --- | ---: | ---: |
| `-01` | `home` | 46 | 1,834 |
| `-01` | `session-wide-completed` | 1 | 59 |
| `-01` | `session-two-turn-completed` | 488 | 40,365 |
| `-02` | `home` | 6 | 298 |
| `-02` | `session-wide-completed` | 2 | 108 |
| `-02` | `session-two-turn-completed` | 7 | 395 |

For `-02` two-turn, five differing cells are on the second footer (y20, x37–41), two on the first footer (y10, x38–39): the independent elapsed durations are original **115ms / 70ms** versus native **139ms / 143ms**. The single-turn footer likewise has real elapsed differences; the `-02` Home difference is at x112–117,y38. These observations are diagnostic, not a mask or a claim that the full frames match.

## Qualification and remaining gates

Both locks label qualification `DIAGNOSTIC_BASELINES_ONLY`: the fake fixture's default tools sample is text-only despite requesting reasoning time, and the common isolated Location differs from the screenshot reference. This probe covers the live, unwrapped two-turn boundary at 120×40. Indexed/full-renderer regressions additionally cover wrapped text and viewport slices; the native real-binary `t44_two_accepted_turns_keep_footer_to_user_spacing_after_restart` test checks the same painted gap after two accepted turns and after a session reopen, durable history, and no replay provider calls. The **paired original/native** capture does not cover wrapped text or replay/restart, so these tests do not establish their external pixel parity. No VIS or R4 PASS is claimed. The final serialized development gate on this dirty tree passed: `cargo fmt --all -- --check`, `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 cargo test --locked --workspace --no-fail-fast` (0 failures; `oc-tui` 300), `CARGO_BUILD_JOBS=3 cargo clippy --locked --workspace --all-targets -- -D warnings`, locked build, docs/progress checks, JS/Python syntax and `git diff --check`; full output: `/home/opencode/.local/share/opencode/tool-output/tool_0d86aec720019Tw6yGEjx3RstS`. This is not the mandatory final-code-SHA V09 visual qualification. The comparator JSONs attest only frame equality; provenance and build association come from the capture locks and recorded commands.
