# Fresh final-build prompt/paste closeout — campaign 04

**BOUNDED_BEHAVIOR_VERIFIED_WITH_WIRE_DIFFERENCE; VIS07_NOT_PASS.**

All ten unchanged paired scenarios completed on the updated Crossterm build:
default chip and extra-longdraft at 79/80/120/121×40, plus raw-end repeated
paste/visual navigation and real suffix-space false-repeat at 120×40. All twenty
side observations are `OBSERVED`; there are zero current behavioral/provenance
failures, rejected requests, unstable captures or blocked comparisons.

## Results

- 404 full per-side captures, comprising 202 paired grids and 202 paired PNGs.
  All 202 grid and 202 PNG comparisons are `DIFFERENT`; zero are `EQUAL`.
- 16 paired cursor differences: four each in chip-79/chip-121 and
  extra-79/extra-121. Every main composer key frame listed in `report.md` has
  matching paired cursor; modal differences remain retained.
- 24 valid isolated local fake-provider requests: 12 original + 12 native,
  comprising 12 main + 12 auxiliary title requests. Default-chip cases use zero
  requests; every submitting side uses exactly one main plus one title. No
  live/remote requests occurred.
- Both repeated-paste scenarios record one completed main and one completed
  title per side, a stable restored capture, and no `panicked at` in raw VT.
  Raw-end repeat expands without duplicate insertion; real suffix-space repeat
  creates two actual chips and preserves both copies through mouse expansion and
  submission. Three actual Up then three Down keys return to the exact caret.
- Existing cold-chip settlement requires the actual chip and five unchanged
  full styled-grid/cursor polls 200 ms apart. Existing exact capture seals still
  require unchanged pre/post PNG grid; no unstable frame is replaced or excluded.
- Pending RGB remains observed on both sides: draft `#eeeeee` → `#808080`,
  stripe/agent `#5c9cf5` → `#484848`; chip `#0a0a0a` foreground / `#f5a742`
  background and bold styling remain. Normal colors return after restoration.
- All geometry, styled cells, PNG pixels, raw VT, output timelines, actual input
  events, configuration/version metadata, protocol and comparator results remain
  unmasked. The independent 16-PNG sample review is in `visual-review.md`.

## Exact source/build association

All ten independent `--build-oc true` invocations ran `cargo build --locked`
successfully and used the same new `target/debug/oc` binary. Source inputs
matched both before/after build and before/after capture in every run.

| Association | Actual value |
|---|---|
| Captured HEAD, all ten runs | `f5af2499b07ca1f27cec9b3c348fa2ecdfdf4bf2` plus sealed dirty changes |
| Native binary SHA-256 | `8f32da3b6a52db123ce6c8eb93108890812da81cd03915995a10b8d66c84f459` |
| Sorted Rust/build input-set JSON SHA-256 | `b142801c2c34b75f4f8a7b0b352ca08c78c0f86a7d2e51e4a52dd72c64f18f5a` |
| Complete canonical source-manifest SHA-256, all ten runs | `99ac1a66b3c6695a284e0af5f0ca43f30818d1b7f1f189e7d6569090d72e7116` |
| Captured dirty-diff SHA-256, all ten runs | `547b6d6eb782061236759af86820ae7ccd79449f49c4279238704b5fb55c9dcf` |
| `Cargo.lock` SHA-256 | `8d8718e1966bf60373a514d2a5b4bdfa516c8e11b054d9c09373a28684a4453d` |
| `crates/oc/Cargo.toml` SHA-256 | `d06d1a6ae71b91640ec804d38b3fdcb868aa8294d14f0d2454b2c86ea070f140` |
| Pinned original commit | `2670273ff17da96f85c5826ced57aa1b368754fa` |
| Pinned original executable SHA-256 | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |

The original executable remains
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`.
Native executable is `/home/opencode/ai/oc/target/debug/oc`.

Comparing the sealed chip-120 source manifests from campaigns 03 and 04 with
`git diff --no-index` shows exactly four changed Rust/build/test input entries:
`Cargo.lock`, `crates/oc/Cargo.toml`, `crates/oc/tests/pty.rs` and
`crates/oc/tests/pty_t39.rs`. The four TUI source hashes and other production
source entries are unchanged. `crates/oc/Cargo.toml` now enables Crossterm
`use-dev-tty` alongside `event-stream`; lock changes include `filedescriptor`.
The two test-file changes remain test fixtures, not production runtime code.
The only additional tooling-manifest delta is the summarizer change made before
capture: explicit Cargo/toolchain/test hashes and per-run/multiple-HEAD reporting.
No scenario, fixture request expectation, strict analyzer check, timing threshold
or comparator contract was relaxed for campaign 04. All ten reported input-file
hashes match current worktree source at summary time.

Complete source manifests, exact commands/exits and full dirty diffs remain in
each run. `report.md` lists every association and all input hashes;
`summary.json` supplies machine-readable results. The owner's reported 30 targeted
and 83 PTY passing tests are context for the backend correction, not tests claimed
to have been independently rerun by this capture campaign. This campaign qualifies
the requested bounded prompt scenarios against the rebuilt actual binary; it
does not add a new simultaneous SIGWINCH/stdin replay or a full VIS07 claim.

## Actual wire gap remains unapproved

With exact prefix

```text
P = "VIS07 visual UpDown draft αβ caret-middle preserving every word with a boundary actual separator "
T = "VIS11-PASTE-0\nVIS11-PASTE-1\nVIS11-PASTE-2"
```

the structured `input_text` is:

| Scenario | Native actual input | Original actual input |
|---|---|---|
| raw-end repeat | `P + T` | `P + T + " "` |
| ordinary suffix-space repeat | `P + T + " " + T` | `P + T + "  " + T + " "` |

All eight extra-longdraft main requests contain exactly:

```text
VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord
```

Per-side actual inputs are retained in full in `protocol.json` and the dedicated
analysis. Separately accepting the observed source-backed original/native wires
does not normalize them or waive parity. The original's extra raw spaces,
version labels, blank-cell/style differences and completed-session duration /
footer differences remain unapproved full-frame gaps. All mandatory full VIS07
frames/cases must match before a PASS claim.

## Execution and return

Before capture, six JS syntax checks, Python bridge AST, real frontend checks
(RGB, styled blanks, eight attributes, CJK/combining text, cursor/DSR), geometry
checks and `git diff --check` passed. Executed:

```text
node scripts/tui_capture/run_prompt_paste.mjs evidence/tui/recovery-v00/prompt-paste20260927-04
node scripts/tui_capture/analyze_prompt_paste.mjs evidence/tui/recovery-v00/prompt-paste20260927-04
node scripts/tui_capture/summarize_prompt_paste.mjs evidence/tui/recovery-v00/prompt-paste20260927-04
```

Campaign driver exit 0; every paired capture runner exit 1 for retained full
comparison differences; dedicated analyzer and summarizer exit 0. Evidence size
before summary/closeout files: 150,964,244 bytes. Historical 01/02/03 captures
remain intact, including the prior failure evidence. No Rust, owner docs,
acceptance, task state or `.opencode` files were edited by this work.

Final owned-process checks found no Cargo/rustc or capture/bridge/campaign runner
processes. Cargo is idle and returned to the parent.
