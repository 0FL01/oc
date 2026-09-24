# T44 `/review` command and VIS25 diagnostic (2026-09-24)

## Scope and executed checks

The bundled, MIT-attributed pinned `review.txt` is used only if this Location has
no admitted `review` definition. The application publishes the real command
and its source-backed description, expands the exact trimmed input without
shell interpolation, and persists the original slash invocation as public
history. The owner supplies the expanded prompt to the Responses provider;
the same prompt is recovered from the durable turn on a model/agent lane change
without replaying foreign opaque/tool state. Configured `review` overrides the
fallback, including its description. No new command is advertised as a TUI-only
action. A bare-`oc` Home → session PTY exercises two genuine provider turns and
the durable original invocations.

Serialized workspace gate after the final source changes: `cargo fmt --all -- --check`,
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 cargo test --locked --workspace --no-fail-fast`,
`CARGO_BUILD_JOBS=3 cargo clippy --locked --workspace --all-targets -- -D warnings`,
`CARGO_BUILD_JOBS=3 cargo build --locked`, documentation/progress checks,
`node --check scripts/tui_capture/capture.mjs`, and `git diff --check`: **PASS**, 0 test
failures (output `tool_0d50de0e6001YsRBF1nnxCp2TH`). Existing opt-in live
tests remained ignored. No live API request was made.

## Immutable paired attempts

The actual pinned original and freshly built native binary were run at the
same Reader/tools fixture, 120×40 truecolor profile, hidden sidebar. Both
attempts used `--autocomplete true --build-oc true` with the documented runner;
their lockfiles include the original executable, environment, source manifest,
dirty tree and native binary hashes. `-06` predates the metadata projection;
`-07` includes it but predates the subsequent provider-replay-only fix. These
are **diagnostic attempts, not final-SHA qualification**. Both runner invocations
exited **1** and preserve full VT, styled cells, PNGs and comparison JSON:

| Attempt | Home `/ren` cells different / 4800 | Session `/ren` cells different / 4800 | Result |
| --- | ---: | ---: | --- |
| `autocomplete-20260924-06` | 230 | 809 | grid + PNG DIFFERENT |
| `autocomplete-20260924-07` | 212 | 807 | grid + PNG DIFFERENT |

In `-07`, both routes now visibly offer the genuine `/review` command and
its pinned description. The Home Tab still invokes `/reload` on both sides;
session Tab still inserts `/rename `. The Home filtered frame still has an
extra native `/dcp-compress`, lacks original `/btw`, and differs in row spacing
and styles. Session still lacks original `/share`, `/fork`, `/export`, `/btw`
and has extra `/dcp-compress`. These are actual inventory/behavior gaps, not
screenshots to mask or commands to fake. All **eight** `-07` full-frame styled
grid comparisons and their PNG comparisons are DIFFERENT: Home 45, trigger 709,
filtered 212, post-Tab 45; completed session 2, trigger 1109, filtered 807,
post-Tab 2 cells. The session's two residual post-Tab cells are measured
elapsed time; the Home baseline/post-Tab include each binary's honest version
and the run-dependent placeholder. Captured behavior on both sides records
no provider request during the slash probe and a real fixture turn afterward.

**VIS25, VIS24, V09 and T44 remain OPEN.** Regional similarity or an
implementation test is not a full-frame pixel-parity PASS. Retain earlier
`autocomplete-*` attempts and rerun the paired suite on the eventual final
code SHA; the owner-reviewed acceptance still requires VIS01–VIS26 and full
grid/PNG equality without masks.
