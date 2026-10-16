# T44 VIS26 — mention styling follow-up (2026-09-24)

**Status: OPEN.** Immutable paired attempt [`mention-20260924-04/`](mention-20260924-04/) captures pinned original and rebuilt native `oc` at 120×40 under the same truecolor profile. All eight **full-frame** grid and PNG comparisons are **DIFFERENT** (each comparator `FAIL`, exit 1; runner exit 1). No frames were masked or rewritten.

`crates/oc-tui/src/{app,editor,shell}.rs` currently has uncommitted changes: accepted file mentions receive bounded editor extmarks (warning-colored, bold), survive draft edits/undo and latest accepted-prompt recall within the session, and render with the original mention-list surface/text styling. The selected `@fixture` file row and after-Tab `@fixture-note.txt` draft/cursor match on Home and session. In this attempt, **after-Tab styled frames differ only at six Home version cells or two session elapsed-time cells**; filtered frames have those same respective residuals. These are real application values, not normalized. Trigger frames still differ because the original displays `@opencode`/`@report` skills and `@general`/`@explore` agents, whereas native displays the file row. Skills/agents are accepted supported differences under the T44 amendment, but full-frame VIS26 parity remains open.

Each grid checks 4,800 styled cells (cursor matches); each PNG checks 647,040 decoded RGBA pixels (1011×640). Counts come directly from the corresponding `mention-20260924-04/<frame>.{grid,png}-diff.json`:

| Full frame | Different styled cells | Different PNG pixels | Both modes |
| --- | ---: | ---: | --- |
| Home baseline (`home`) | 6 | 298 | FAIL / exit 1 |
| Home trigger | 304 | 39,699 | FAIL / exit 1 |
| Home filtered | 6 | 298 | FAIL / exit 1 |
| Home after Tab | 6 | 298 | FAIL / exit 1 |
| Completed session baseline (`session-wide-completed`) | 2 | 117 | FAIL / exit 1 |
| Session trigger | 464 | 61,416 | FAIL / exit 1 |
| Session filtered | 2 | 117 | FAIL / exit 1 |
| Session after Tab | 2 | 117 | FAIL / exit 1 |

`-04/{upstream,oc}/mention-checks.json` records visible trigger/filtered menus, Tab insertion, matching cursor positions, and no *new* provider request during mention editing (Home 0, completed session 3 requests on each side). Separate existing bare-binary PTY test `vis26_bare_oc_mention_tab_submits_durable_location_relative_prompt` passes and supplies durable submission/Location evidence; the paired frames themselves are pre-submit.

Provenance: [`commands.json`](mention-20260924-04/commands.json) invokes the paired runner with `--build-oc true`; its `cargo build --locked` exited 0. [`capture.lock.json`](mention-20260924-04/capture.lock.json) records original source `2670273ff17da96f85c5826ced57aa1b368754fa`, unchanged original `opencode v2.0.12` executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`, rebuilt `oc` executable SHA-256 `65edefdef861225df5c6b0341c96078e2241bd267a98d2bcab3fcf5b61ddd1db`, fixture SHA-256 `16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`, and shared environment ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. Native source lock: HEAD `5bdf398233f9c6229ab17b1910e939293181791f`, tree `f087e1b989060981ecbf5b9762de49a263af9527`, **dirty** diff SHA-256 `f1d410f9bb174d3f1aca5754f3347c286a6a72deb5213f37f506fbde6815b775`, [`source-manifest.json`](mention-20260924-04/source-manifest.json) SHA-256 `d47e314d06cd26ec27d3ce2a250c7374cd484023ab790173208df3ab511597f6`. HEAD is **not** a final source commit for this binary.

After an earlier experimental slash-context filter caused three PTY failures, it was rolled back; targeted checks and the subsequent full workspace gate passed. Supplied full-gate output: `/home/opencode/.local/share/opencode/tool-output/tool_0d47e1a640012OCeP8BV9VoRbL` (workspace tests 0 failures, TUI 252/252, PTY 31/31 including VIS26; fmt, Clippy, build, docs and progress checks passed). That gate does not close visual parity or T44. `git diff --check` passed for this follow-up.
