# T44 recovery V02 checkpoint

## Result

Base `4c1a0bc`. Existing turn journal now stores safe ordered display references,
public reasoning and pinned metadata; no second transcript. Application projects
bounded parts with stable sequence/status and explicit omissions/legacy availability.
Tool intents and their display reference are atomic. Actual title provider request,
display names/provider names/known prices/limits and measured usage reach TUI.
Primary and child agent colors are generation-pinned. Full model/variant agent
selection works through existing runtime resolution.

Independent review led to fixes for default/canonical title generation, invalid
title profiles, part order/identity/status, preview omissions, actual result-graph
assertions and raw PTY live/restart coverage. Parent reviewed DTO/application,
storage/provider/tools and crash-test diffs and verified capability mapping.

## Checks

- Parent `cargo test --locked -p oc --test recovery_v02 -- --nocapture`: exit 0
  twice, one integration test each; latest includes actual PTY agent selection,
  native provider effects, persisted title and live/replay parts.
- Parent `cargo fmt --all -- --check`, `git diff --check`: exit 0.
- Implementation final workspace: 436 passed, 4 existing live ignores; clippy,
  locked build and fmt passed. Every earlier failure retained in report.md.
- Paired visual comparison and final live qualification not run in this slice.

## Risks

Native autoaccept is Unsupported, not inferred from Allow tool rules (see
auto-capability.md). It remains a full behavioral parity capability gap.
Title generation adds a real tools-empty request and at most 10 s ancillary work;
failure stays untitled and cancellation remains owner-routed. Legacy journals
cannot recover never-recorded reasoning/order and are visibly text-only.
Part/byte serving limits are visible, not silently complete. Long wrapped patches
exposed the pre-existing viewport/scroll defect assigned to V03. V07 MCP in-flight
call cancellation, config admission and docs registry gate remain open.

## Next

V03 full-height viewport/sidebar/title/conditional devtools/dynamic prompt geometry,
with wide and breakpoint actual frames; then V04–V09. No R3/R4/VIS/full T44 gate
closed. User ZIP remains unstaged, no historical report rewritten.
