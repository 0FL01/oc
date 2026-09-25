# T44 R5/V05 — paired Ctrl+C interaction evidence

Immutable attempt: [`ctrl-c-20260925-01/`](ctrl-c-20260925-01/). The runner invocation in [`commands.json`](ctrl-c-20260925-01/commands.json) used `--build-oc true --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 --ctrl-c true`; **runner exit 1**. [`capture.lock.json`](ctrl-c-20260925-01/capture.lock.json) records both `CTRL_C_CHECKS_PASS` and `provider_contract=true`, but all eight paired grid and all eight decoded-RGBA PNG comparators returned `DIFFERENT` (exit 1). This is interaction evidence, **not full VIS05/T44 pixel-parity PASS**.

## Provenance and environment

The reference is opencode **v2.0.12**, pinned source `2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`. Native is `oc 0.1.0` built by successful `cargo build --locked` in this captured run (see `commands.json`): HEAD `cc2a6173d139bb640df7eda8d571fbef75fd9a51`, tree `462435ca8ab3d7cb1cd34b22aba574e67b16dd05`, **dirty** diff SHA-256 `9f3e231fbd169c8e00ffc0501d7d2ad78251c2a3fbb3eae0266aa7d84d7ca4af`, source manifest SHA-256 `c9147c3eba0008cea2e2a124a4e2795809da45d45d000b40a2cc7d8f183d7eed`, built executable SHA-256 `6b2c254f62bf0d512385b0a3b02c693b9efaf4b246cb84e37d410ed5b1bb8f88`. HEAD alone does not describe the tested native source.

Shared fixture SHA-256 `3dce22b803ba229615462b8c3f89aabe19b9697baabcf9849d3d697fcbf63759`; paired environment ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. Profile: 120×40 `xterm-256color`, truecolor, `C.UTF-8`, dark `opencode`, sidebar hidden, horizontal tabs, `@xterm/xterm`/xterm.js 6.0.0, Unicode 11, Chromium 145.0.7632.6, DejaVu Sans Mono 14 px, scale 1; no clock/content masking. Each captured side has inputs, protocol, raw VT, styled cells, text, PNG and render metadata. Both bridge processes exited 0 and both `bridge.stderr.txt` files are empty.

## Observed behavior

Both [`upstream/ctrl-c-checks.json`](ctrl-c-20260925-01/upstream/ctrl-c-checks.json) and [`oc/ctrl-c-checks.json`](ctrl-c-20260925-01/oc/ctrl-c-checks.json) record the same 17-stage Home/session sequence with true predicates. On Home, a nonempty pasted multiline draft and chip were visible at the same coordinates (prefix x26,y21; chip x47,y21); Ctrl+C removed both, showed the empty prompt and left the PTY alive. In a completed session, the nonempty multiline draft and chip were visible (prefix x5,y34; chip x29,y34). Ctrl+P opened Commands over the retained root draft; with a typed modal query, Ctrl+C **cleared the query while leaving Commands open and retaining the root draft** (`modal_outcome: query_cleared` on each side). After dismissing Commands, Ctrl+C cleared the root draft and chip without exiting. A subsequent Ctrl+C on the empty session prompt exited naturally with code 0, left the alternate screen and restored the cursor on both sides (recorded exit event `termination: natural`, generation 0). The modal test does not establish that the first Ctrl+C closes Commands.

Fake-provider baselines and observed counts were **0 requests / 0 completed / 0 invalid on Home** and **3 / 3 / 0 after the fixture-backed session turn** on each side; all Ctrl+C/modal/exit stages preserve their baseline, hence zero additional provider requests or draft submissions. Captures passed their stable-state predicates. Exit VT SHA-256 differs between binaries (reference `d920fb76eef47d859448bd3be2e032ce732e7bd3a265717dc215ed554a6da3b3`, 1671 bytes; native `7e472e322d0a3a1407ff8f7ec775a8184ec38f4d9d3aab66a9846ce73ca59b41`, 144 bytes); restoration is supported by the recorded predicates, not VT byte equality.

## Full-frame comparisons (no masking)

Every named grid comparison checks **4,800 styled cells**; every PNG compares **647,040 pixels** (1011×640). All 16 comparators report `FAIL`/`DIFFERENT`; no paired cursor position differs. Exact counts from each scenario's `*.grid-diff.json` and `*.png-diff.json`:

| Paired scenario | Differing cells / 4,800 | Differing pixels / 647,040 |
| --- | ---: | ---: |
| `home` | 45 | 2,037 |
| `ctrl-c-home-draft-before` | 23 | 2,586 |
| `ctrl-c-home-root-cleared` | 45 | 2,037 |
| `session-wide-completed` | 2 | 134 |
| `ctrl-c-session-draft-before` | 19 | 2,422 |
| `ctrl-c-session-modal-query-before` | 17 | 2,432 |
| `ctrl-c-session-modal-after` | 174 | 9,233 |
| `ctrl-c-session-root-cleared` | 2 | 134 |

Diagnostic differences: the cleared Home frame reproduces the respective initial Home PNG on each side but the sides have different real placeholder text (reference `Fix a TODO in the codebase`, native `What is the tech stack of this project?`) and visible version/footer differences; pasted-draft frames also differ in chip styling. Completed session text shows real elapsed `104ms` versus `139ms` at x38–39,y10, accounting for its two-cell difference and the two-cell cleared-session difference. The modal-after frames differ substantially in Commands inventory: the original shows `Connect an integration`, `Open settings`, `Share session`, `Prompt`, `Skills`; native shows a `Session` group with `Close tab`, `Rename session`, `Show sidebar`, `Expand thinking` and `Agent`. The modal outcome predicates agree despite this full-frame mismatch.

## Gates and qualification limits

The serialized full gate after the native Ctrl+C fix and accepted-Home deck persistence repair passed: `cargo fmt --all -- --check`, `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 cargo test --locked --workspace --no-fail-fast`, all-target workspace Clippy with `-D warnings`, locked build, docs/progress checks, Node runner syntax and `git diff --check` (output: `/home/opencode/.local/share/opencode/tool-output/tool_0d857668300141tMisRi6y4Lwp`). An earlier full run failed stale single-Ctrl+C exit expectations in three PTY suites; the tests now verify clear/exit separately. That run exposed an accepted-Home tab-deck save race: an ACK consumed while handling a key could attach the root before the next loop's save predicate. The owner-backed save predicate now tests the unsaved deck state after receipt polling; a channel regression and real restart PTY test pass. The immutable paired capture predates this later persistence fix, so it is not a final-source-SHA qualification.

The lock labels qualification `DIAGNOSTIC_BASELINES_ONLY`: elapsed/token-rate values use real wall clock; the default tools sample does not execute reasoning qualification; Location is the common isolated project path, not the screenshot reference path; requested settings are not proof of effective native settings; MCP error/stall was not run. This attempt checks the specified Ctrl+C interaction at this geometry/profile, not the whole keymap, all modal routes, or the remaining VIS05/T44 gates. Frame diff JSON attests frame equality only; the lock and recorded build provide capture/source provenance. The runner's exit 1 is consistent with the sixteen full-frame inequalities, not an interaction-predicate failure.
