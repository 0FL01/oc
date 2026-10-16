# T45/R10 — file-tool family consumption per lane model (no product change)

R10 (spec line 132, 2026-10-01 supplement): own-model child/profile previews and
managed guidance consume T50/R1's OC2 file-tool predicate, dropping incompatible
definitions/guidance on the next request after a user model switch. T50 owns
the predicate (`runtime.rs::selected_tool_defs`) and its busy-commit/reopen
receipts (`application/live_switch_tests.rs::tool12_busy_owner_commit_reloads_next_request_without_another_prompt`,
`tests/runtime/file_mutations.rs::tool12_next_turn_and_reopen_switch_family_keep_historical_calls_verbatim`).
This atomic qualifies the T45 lane consumer only.

## Check

`tests/subagent.rs::r10_file_tool_family_follows_each_lane_model_and_next_root_request`
(fake provider, six captured requests):

- root on `gpt-parent`: tools `apply_patch` only, one guidance item naming it;
- child `own` with profile model `m`: request model `m`, tools `edit`/`write`,
  guidance `write, edit` with no `apply_patch`;
- child `inherit` without a model: request model `gpt-parent`, `apply_patch`;
- next root turn after the user switches the session model to `m`: tools
  `edit`/`write`, guidance without `apply_patch`;
- every request carries exactly one file-tool guidance item and no duplicated
  fixed lane.

`subagent` 36/0; fmt and strict Clippy PASS.

## Busy model/variant commit consumer (R10 line 135)

Reuses TOOL12's same-task A→B→A receipt
`application/live_switch_tests.rs::tool12_busy_owner_commit_reloads_next_request_without_another_prompt`
(T50 already asserts model per request, output limits, tool fingerprints,
retained call/result groups and dropped alien opaque state). Added T45 consumer
assertions on the same three requests: `context_limit`/`dcp_min_context`/
`dcp_max_context` = (100000, 40000, 55000) → (50000, 20000, 27500) → back,
i.e. native 40%/55% defaults relative to each committed model; exactly one
file-tool guidance item naming `apply_patch` → `write, edit` → `apply_patch`;
the base prompt lane is identical (no second fixed lane). Draft-only picker,
blank-Enter commit and stale/failure reconciliation remain covered by TOOL12's
TUI receipts (`oc-tui/src/app/tests/model_selection.rs::tool12_draft_only_busy_blank_commit_late_failure_keeps_newer_draft`,
`tool12_message_captures_draft_and_pending_acceptance_does_not_erase_newer_choice`).
