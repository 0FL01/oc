# VIS37 question/status sibling geometry — qualified slice, T44 active

Date: 2026-10-12. Producer base: `d4cfa8698b67e57036a0275a94f541b8f1b6c6d2`.
Delivery also preserves the separately landed planning commit `9aac88423901455ff9173614a9678bb1d1caeee6`.
Pinned Original: `2670273ff17da96f85c5826ced57aa1b368754fa`.
This is not whole VIS37, R6/V09 or T44 PASS.

## Source-proven cause and minimal fix

The initial “extra Messages spacer” hypothesis was refuted: full and indexed
transcript positions both already include that leading line. No Messages row,
cache, margin, paging expectation, RAW record or test baseline was changed.

Pinned TUI `routes/session/index.tsx:1331–1350` reserves its one-row status sibling
before the bottom prompt/form stack even while the sibling is empty. Native's
Question branch gave all pre-form content to the transcript and returned before
painting status. In the small long/custom/cancel-pending captures, bottom-sticky
content therefore sat one row below Original while the form itself matched.

`shell.rs::session_regions` now clamps form height, reserves up to one existing
`STATUS_ROW_HEIGHT` immediately above it, and gives only the remainder to the
transcript. `render_session` preserves Approval/composer priority, paints status
before the Question return, and still never paints the ordinary composer over
the form. When the form fills the available content, status/transcript clamp to
zero. Drawing and input retain the shared transcript-area geometry.

The new owner regression uses real typed question input/live operation, actual
full-shell TestBackend painting at 80×24/120×40/160×48, sticky full/indexed paint
equality, a blank status sibling, actual small user/Asked rows, then older history
and the existing scroll owner to paint the Jump state. Request binding and the
unsent ordinary draft remain unchanged. No new production API or runtime phase.

## Actual checks

- Corrected RED: 0 PASS / 1 FAIL at the old missing status row. The first command
  accidentally used an unqualified `--exact` filter and ran zero tests; retained
  as a zero-test observation, never counted as RED or PASS.
- First broader GREEN exposed a new-test assumption about Line container styles;
  spans cover the complete width, so the regression now compares actual full
  TestBackend styled paint instead. No production or old expectation changed.
- Final shell targeted: 72 PASS / 0 FAIL; actual VIS37 PTY/headless: 3 PASS.
- Complete current workspace: **1859 PASS / 0 FAIL / 11 unchanged opt-in ignores**,
  **46 targets**, TUI 529, PTY 58. Command: `cargo test --workspace --locked --no-fail-fast`.
- Strict locked all-target workspace Clippy `-D warnings`, fmt check, locked and
  ordinary debug build/help, locked release build/help: PASS. Release build ran
  177.720 s; actual release Cargo targets ran **9 cases, all PASS**: startup and
  discovery (2), TERM01, panic restoration, VIS31 idle/pacing/burst (2), S07 archive
  resources, VIS38 equal-active/archive resources, AUD32 growing-archive bounds.
- Root Python 68 plus question-fixture Python 2: PASS; docs/progress/diff checks
  verified for delivery. The checked source stayed unchanged after capture.

First full workspace was 1856 PASS / 3 FAIL / 11 ignores. Failures were the
questionless Unicode caret, provider-burst latency and Dialog catalog CPU cases.
All three fresh isolated cases passed on unchanged code. A later complete warm
two-thread run passed 1859/0/11 after observed host load decreased and available
memory recovered. Failed full output and isolated outputs are retained; no
deadline, performance threshold, test, baseline or ignore was weakened.

## Current011 acquisition and exact comparisons

Exclusive existing-harness campaign `vis37-form-current-011/{small,wide,tall}`;
80×24, 120×40, 160×48, sequential builds/runs. Each side passed all 18 real stages,
including separately painted multiselect focus, owned custom edits, Tab/BackTab,
explicit submit, answer continuation, reopen/restart and owned cancellation.
No replay of previous episodes, mask, crop, tolerance, fake clock/cursor or golden.

| Profile | Whole PNGs | Strict comparisons | DIFFERENT | EQUAL |
|---|---:|---:|---:|---:|
| small | 36 | 36 | 20 | 16 |
| wide | 36 | 36 | 22 | 14 |
| tall | 36 | 36 | 22 | 14 |
| total | **108** | **108** | **64** | **44** |

Independent full recount checked **54 pairs / 259200 styled cells / 34953984 RGBA
pixels**: **265 differing cells / 13168 differing pixels**, **54/54 cursors agree**,
25 whole grids and 19 whole PNG pairs equal. Additionally all 51 complete header
rows, 27 pending-form regions, 3 answered regions, 6 cancelled rows and three small
long/custom transcript-plus-status regions match every styled cell/geometry.
Named regions are diagnostics, not masks or replacements for whole comparisons.

The remaining whole-cell differences independently classify as rotating examples
(178), actual native versus donor package version (54), and real elapsed labels
(33), totaling all 265. Six grid-equal custom-empty/custom-edited PNG pairs differ
only at the active caret cell (795 pixels total). Pinned custom textarea sets a
focused-form-field cursor color; native no-panel color currently returns Reset.
That is the next concrete source seam; color versus blink phase still needs its
own direct proof. Versions, randomness and real elapsed data were not rewritten.

## Retention and independent inverse

- `vis37-form-r11-proof.tar.xz`: **1573812 encoded bytes**, SHA-256
  `c7dee0afadba3e83ec137e4ac8bed1a5f43c03a978c31fdc35f34cdab79fddc6`.
- Decoded canonical USTAR: **55541760 bytes**, SHA-256
  `e984a004aab6d62969415b8c0adf602c87fd1d0fdba092e32006b0f79dd6a41c`.
- **704 original files / 55979473 logical bytes**, 27 direct same-run/same-side
  byte-identical original encoded PNG aliases; member inventory
  `49ca54319b0086ec53ccd897cfd25e46e2cd9fc5db18accfe6bcf8747a2ca819`.
- `vis37-question-status-audit.py`, sealed before/after reports independently use
  manual single-XZ/canonical-USTAR readers, not the producer codec. Every original
  member byte, alias, metadata stability, source/ELF/build/provenance and full
  comparison metric checked. **97 prior proof files and sealed helpers unchanged**.
- After report 125109 B, SHA-256
  `eb9cec8f051f80247932cb5486599493632d5c78a62a88740a84a27b10505ad5`;
  measured VmPeak239718400 / VmHWM165658624 B, below hard AS384 MiB.
- First independent-before AST guard failed before writing an artifact because
  two prior-source constants had distinct path forms. The exact two-value map
  was corrected; failure retained, old helpers and captures unchanged.
- Complete gate logs/receipts and archival/audit helpers retained separately in
  `vis37-question-status-gates.tar.xz`: 63752 B, SHA-256
  `af6cd4a4f3cf753d98c7e98ac4e67915e88854ab20bf6eb7d4ad1eaa68118827`;
  38 complete command observations, 79 original files/473911 logical bytes,
  independent every-original memory and durable inverse. Gate verifier measured
  VmPeak241520640 / VmHWM71974912 B, below hard AS384 MiB.

Current 713-file source seal:
`c5004540f17136e55e22ef42de7a38c71c835a9622fd5e62b711622058e2b886`.
Only `shell.rs` and `shell/tests.rs` changed from010. Native debug ELF484336384 B:
`52cba8d040441bfcc6bebfd9e8d702e1d512aa14d1289c4fc9b08f709340338c`.
Donor ELF200529376 B:
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Captured producer identity stays d4cfa, not rewritten to later Git HEAD.

Observed protocol: 24 valid provider requests (18 main + 6 title), 12 question
calls, 12 user submissions + 6 explicit answers, 30 read-only snapshots; native
typed and Original V2 RAW IDs/outcomes persist through reopen/restart/cancel.
No observed extra project/Shell/MCP effects or edit/review/restart/cancel
continuation. Generation0 natural exit0; generation1 forced harness teardown is
not clean-final-lifecycle proof. DCP tables unread; no blanket lifecycle PASS.

## Capacity and continuation

Existing own untracked/unstaged **actual FAILED model-shell-005** was losslessly
archived: 62 files/5016919 logical bytes/three directories, full byte and
path/mode/UID/GID/mtime inverse, original-path FAILED pointer. Archive286892 B:
`09bd272c619c2279ac2a81135959a6a9e9171c5d2ccb31797ed957475d7f491e`;
reservation3145728 B, measured allocation reclaimed4853760 B. Root003 was not
FAILED and remains untouched; qualified proofs/old receipts were not removed.

After011 report owned1069072384 /1073741824 B, headroom4669440 B before bounded
delivery records; after receipt/gate retention actual1069154304 B, headroom4587520 B.
Final actual recount is authoritative. Limits unchanged:
16 MiB file/encoded,128 MiB TAR/logical,2000 members,XZ dict16 MiB/decoder32 MiB,
Python hard AS384 MiB, owned1 GiB; approved TMP separately accounted, no offload.

Continue the focused-question cursor-color seam and **all frozen R1–R6/VIS01–45
and supplements**, including final current-source R6/V09. Current011 is not full
VIS37 or T44 PASS. Commit/push each checked major slice; no finish or source-proof
promotion after subsequent production changes.
