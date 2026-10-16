# NOW — актуальный handoff

State updated: 2026-10-16T20:58:20+00:00
Active: T44

Сверить Git status/diff до выполнения команд.
Task: T44 — TUI pixel parity с opencode v2.0.12, включая Approve (VIS36), question (VIS37), DCP (VIS38), child/Subagents/Shell/Terminals (VIS39), MCP modal/startup (VIS19/VIS40), tab hover-marquee (VIS41), plugin/service errors и чистый диалог без diagnostic spam (VIS42), компактный live-preview indicator (VIS17), скрытые synthetic tool-preview markers и Generic/MCP expand (VIS16/VIS17), provider retry (VIS43), Middle Click tab close (VIS44), tool-output phantom caret / prompt blink (VIS16/VIS31), историю промптов Up/Down и установку каретки мышью (VIS12), Codex/OpenAI connect/accounts/browser/device/key auth (VIS45), Code Mode execute inline calls/detail dialog (VIS46)
Spec: docs/goals/2026-09-21-tui-pixel-parity.md
Evidence target: evidence/T44/report.md

Full frozen R1-R6/VIS01-VIS46 and supplements remain in spec, T44_CONTRACT_AMENDMENT.md, ACCEPTANCE.json and SAFETY_REGRESSIONS.md; navigation is not reduced acceptance. Owner explicitly resumed2026-10-07: full closure, realtime todos, checked commit/push per major slice. First clean-dialogue e2120e418 qualified1720/0/11; pinned attempt005 eleven stages per side/22 stable frames, ALL22 strict comparisons DIFFERENT. Next concise typed submitted-refusal facet qualified current1721/0/11 plus strict Clippy/debug-release/release-startup; safe captured detail and pre-effect draft preserved. Receipts evidence/tui/vis42-clean-dialogue.md and vis42-request-refusal.md. Whole VIS42/V09/T44 is not PASS. Continue ordered structured tool-body/guidance/ref/capture and Generic-MCP VIS16/VIS17, remaining VIS42 styled/detail integration and every approved prompt/caret/blink/pacing/scroll/profile/model/live-selection/tabs/session/Revert/compaction/file/approval/question/DCP/child/Shell/Terminals/retry/MCP/auth slice. Consume landed backend owners without whole-task done cycles. Actual effects plus running OC2/native full styled-cell/PNG/cursor proof, not own goldens/historical PASS; disclose native differences. Four crates/one owner/immutable RAW/permissions/redaction/caps/no unknown replay; no new registry/store/poll/paid campaign/user config edits. AUTH06 deferred; original24/24 and Go13/24 unchanged. Owner-approved 2026-10-14 VIS46: Code Mode execute producer facts/live ToolCallProgress, inline nested-call rows and execute detail dialog (Code/Output/c/o/esc) with paired qualification after T59 finish; supersedes only T60 generic-card difference, NOT_RUN. Finish only after all mandatory T44 outcomes and final current-source R6/V09 gates.

Последний checkpoint этой задачи (проверить актуальность по Git):

# Direct Tab profile cycle (2026-10-16)

## Result

Owner authorized this narrow correction, commit/push and debug build after the
previous scheduling pause. Base: `0909e410661920fd5d022ba8d842e11641c093f7`.

Ordinary root-composer Tab now emits the existing selection intent for the next
visible primary profile in catalog order, including custom profiles, with wrap.
It never opens the agent picker. The existing selection owner still admits and
persists Home/session choices and restores per-profile model/variant state.
Draft/caret and panel-less transcript view state are preserved. Busy/read-only
guards and existing autocomplete, forms, dialogs, Shell and terminal key owners
remain unchanged. `/agents`, the palette and effective leader+A retain the picker;
Shift+Tab no longer opens it. Footer: `tab agents`.

No new config, API, persistence/schema, optimistic selection or error-skipping
mechanism. No provider/DCP/security changes or copied donor implementation.

## Checks

Every Cargo process used `CARGO_BUILD_JOBS=2` and CPU affinity `0,1`, including
compiler/linker descendants. Main tests used two threads; isolated retries one.
No concurrent Cargo, target cleanup or target-directory override.

- Both new regression risks were reproduced RED before the runtime change:
  missing cycle intent and panel-less wheel-animation cleanup.
- Nearest input module: 68 passed; footer goldens: 2 passed; narrow styled footer:
  1 passed; owner-routing regression: 1 passed. No failures in these checks.
- Workspace fmt check and strict workspace/all-targets Clippy: PASS.
- `cargo test --locked --workspace --no-fail-fast`: **FAILED**, 2018 passed,
  15 failed, 12 unchanged opt-in ignored cases across 46 targets.
- Of eight non-path failures, seven passed when rerun individually: shell job
  conversion, primary context/children, catalog CPU/cache, long-history wheel,
  Go account flow, inherited-selection repair and custom-provider retry.
- `lifecycle::vis31_high_rate_input_during_provider_burst` failed in both runs:
  existing 100 ms input-paint limit under provider burst. Its UTF-8 glyph scenario
  does not invoke Tab/profile selection; performance remains unqualified here.
- `cargo build --locked`: PASS, debug `target/debug/oc2`; `--version`: `oc2 0.2.0`.

Local logs: `/home/stfu/.cache/opencode-tmp/opencode/tab-agent-M1CL4x/`.
Runtime diff was reviewed after targeted success; test assertions, latency limits
and ignored-test selection were not weakened to obtain green results.

## Risks

Seven existing fixtures cannot run in this account because they hardcode missing
`/home/opencode/.cache/opencode-tmp/opencode`: auth CLI/TUI, malformed-config
diagnostics, Location-switch diagnostics, startup/discovery and terminals.
An outer TMPDIR cannot override those literals. No other-home directories or
host configuration were created/changed to work around them.

Full workspace R6 remains FAILED, not waived. Individual retry successes do not
replace a zero-failure workspace run. No whole T44/VIS/parity/READY claim; no new
paired visual or live-provider campaign was run. The owner's interactive alias
was not launched; the actual debug artifact belongs to this worktree.

## Next

Commit/push this bounded correction and report the failed workspace gate
separately from delivery/debug-build success. T44 remains open. A separate
environment/fixture portability decision and investigation of the provider-burst
latency failure are needed before repeating final R6; do not widen this bugfix or
change its acceptance criteria to conceal those blockers.


Ready (до 5): нет
Blocked: T27, T45, T57, T61

Done в журнале не означает READY всего продукта; см. GOAL.md.
