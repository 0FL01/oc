# T44 — paired selected-tab indicator

Code commit `fcdcddc`; original v2.0.12 commit
`2670273ff17da96f85c5826ced57aa1b368754fa`,
executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
New immutable attempt: `evidence/tui/recovery-v08-tab-indicator-01/`, using
the same tools fixture SHA-256
`dbfc93c470dd18d3af79b33195a13c898e83944ec5e0f242cd32d88b65531644`
and 120×40 terminal profile
`e3cf33539f0e1d6485c01217ef4f632171a7de480eea7bdd9f9c598ea70f45be`
as the preceding `recovery-v08-tab-fade-01/` attempt. Both sides have isolated
HOME/data and the fake Responses endpoint, no production credentials.

## RECON and change

Pinned `packages/tui/src/config/index.tsx` defaults `tabs.indicators` to
`status`, with explicit `numbers` supported. `component/session-tabs.tsx`'s
`TabIndicator` leaves the selected status indicator blank when idle and shows
a spinner when busy (its first dot frame is `⠋`, from `spinner-frames.ts`).
Native `crates/oc-tui/src/shell.rs` formerly hardcoded ` 1 `, even in an idle
status-mode capture: original x=0–2,y=0 were white-foreground blanks, native
included `1` and tinted all three spaces `#bababa`.

`TuiChrome` now carries typed status/numbers mode, defaulting to status;
`composition.rs` reads `/tabs/indicators` through the same admitted-root,
ordered `cli.json/jsonc` path as `/tabs/layout`, rejecting unsupported values
without reflecting them into diagnostics. The horizontal tab preserves its
three-cell geometry, paints idle indicator blanks with the root foreground,
uses the genuine `TuiState::is_busy()` for a static first spinner frame, and
shows the number only for explicit numbers mode. Test-first shell/config
regressions cover idle, busy, explicit numbers, override precedence and bad
values. The existing real-PTY model-dialog assertion was updated to require
the **correct** idle status-tab geometry rather than hardcoded `1`.

## Pair and gates

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --columns 120 --rows 40 \
  --exploration-click true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v08-tab-indicator-01
```

Both sides EXECUTED with `provider_contract=true` and both expand/recollapse
the real read group. `check_region.py` reports **0/32** symbol/style/width
differences across the *entire selected tab* x=0–31,y=0 in completed and
expanded captures; prior x=0–2 alone differed **3/3** cells. Full completed
frame is still DIFFERENT in **360/4,800** styled cells and **8,860/647,040**
PNG pixels, runner exit 1. Elapsed time and other variable state can shift
the overall count between captures. The lock attests the precommit tracked
dirty diff and binary; after this capture, only the corrected PTY assertion
was added to the code/test diff, not the renderer.

The first full serialized workspace run exposed the old `1 Untitled session`
PTY expectation (21/22 passed in that target); after replacing it with the
source-backed default idle status geometry, the targeted PTY case and fresh
`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked --workspace
--no-fail-fast --quiet` PASS (TUI 180; existing live ignores). Workspace fmt,
all-target clippy with `-D warnings`, locked build, diff and docs/progress
checks PASS. Neither the corrected golden nor local region equality proves
whole-frame pixel parity.

## Open

Native status lacks unread, permission/question attention, multi-tab state and
real add-tab interaction; the static first spinner frame is a conservative
busy presentation, not a claim of animation parity. Original add-tab control
` + ` remains absent: do not paint a misleading inert button. Home examples,
prompt/footer identity (upstream implicit Build versus native deliberately
unconfigured fixture) and wall-clock durations also differ. VIS01–VIS24,
V08–V09 and S07 residual qualification remain open; T44 stays active.
