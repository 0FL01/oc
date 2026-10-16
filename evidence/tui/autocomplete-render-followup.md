# T44 VIS25 — slash overlay render follow-up (2026-09-24)

Immutable paired captures: [`-08`](autocomplete-20260924-08/) after `Clear` erased Home logo glyphs beneath the menu; [`-09`](autocomplete-20260924-09/) after route-inventory-based column padding before fuzzy filtering; [`-10`](autocomplete-20260924-10/) after separate muted description spans for unfocused rows. `-08` and `-09` are preserved intermediate attempts. Each has `capture.lock.json`, `commands.json`, source manifest, original/native PTY cells/text/PNG and per-frame grid/PNG diffs. Pinned reference: `2670273ff17da96f85c5826ced57aa1b368754fa`, `opencode v2.0.12`; native source locks are **dirty HEAD** `c48a82c326bf41ae40ca038a4ea42b9337f43d13` with distinct dirty-diff/source-manifest and built-binary hashes per attempt, **not final-SHA qualification**. Same Reader/tools fixture, 120×40 truecolor, dark opencode, hidden sidebar; runner `--autocomplete true --build-oc true` exits **1** each time. Lock qualification: `DIAGNOSTIC_BASELINES_ONLY`.

**Unmasked whole frames:** each entry is differing styled cells / differing decoded PNG pixels (out of 4,800 cells / 647,040 pixels). Every grid and PNG comparator is **FAIL/DIFFERENT**, exit 1, for every frame in all three attempts; no pixel-parity PASS.

| Frame | -08 | -09 | -10 |
| --- | ---: | ---: | ---: |
| Home | 27 / 1,247 | 6 / 298 | 27 / 1,247 |
| Home `/` trigger | 709 / 15,618 | 685 / 15,677 | 678 / 15,441 |
| Home `/ren` filtered | 212 / 8,179 | 205 / 7,966 | 203 / 7,845 |
| Home after Tab | 27 / 1,247 | 6 / 298 | 27 / 1,247 |
| Completed session | 3 / 170 | 2 / 125 | 2 / 121 |
| Session `/` trigger | 1,110 / 15,201 | 1,085 / 15,051 | 1,078 / 14,790 |
| Session `/ren` filtered | 808 / 70,071 | 801 / 70,127 | 796 / 69,886 |
| Session after Tab | 3 / 170 | 2 / 125 | 2 / 121 |

Previous `-07` filtered grid baseline was Home **212**, session **807**; `-08` **212/808**, `-09` **205/801**, `-10` **203/796**. The Home trigger `-10` **678** compares with **685** in `-09` and **709** in `-08` (the captured `-09` value is not 709). The `-10` Home post-Tab **27** cells are still different; `-09`'s 6-cell Home baseline/post-Tab was run-dependent, not a parity result.

Observed genuine actions: both sides show `/` and `/ren` overlays without a provider request; Home Tab chooses `/reload`, clears the draft and shows `Configuration reloaded`; session Tab inserts `/rename` without submitting it. The completed Reader/tools fixture makes 3 valid provider requests per side (0 invalid). The Home filtered reference lists `/reload`, `/review`, `/btw`; native lists `/reload`, `/review`, `/dcp-compress`. Session reference also includes `/share`, `/fork`, `/export`, `/btw` absent natively; native includes extra `/dcp-compress`. Different inventory, column widths and row styling remain visible; neither missing behavior nor menu entries are fabricated. Home baseline/post-Tab differences include honest binary versions `2.0.12` vs `0.1.0` and independently random placeholders (in `-10`, `Fix broken tests` vs `Fix a TODO in the codebase`); completed-session/post-Tab differences are real elapsed-time digits. VIS25/T44 remain **OPEN**.

**Implementation gate already executed (not rerun for this report):** serialized `cargo fmt --all -- --check`; `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 cargo test --locked --workspace --no-fail-fast`; `CARGO_BUILD_JOBS=3 cargo clippy --locked --workspace --all-targets -- -D warnings`; `CARGO_BUILD_JOBS=3 cargo build --locked`, documentation/progress and diff checks: **PASS**, zero test failures, oc-tui **263 passed** (existing opt-in live ignores). Reference output: `/home/opencode/.local/share/opencode/tool-output/tool_0d527b71b001KJTumamXKmGG1z`. Green implementation gates do not override the 24 failing frame pairs.
