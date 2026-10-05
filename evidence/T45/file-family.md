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
