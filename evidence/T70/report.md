# T70 — OC2 exploration group parity in the TUI transcript

Status: PASS — bounded T70 correction, R1–R3 verified; one pre-existing unrelated flaky test recorded
Date: 2026-10-19
Source base: `a8a3d28c7` (master)
Branch: `fix/tui-exploration-group` (pushed to verified origin)
Plan commit: `f33adc2a8`
Reviewed implementation commit: `645784cc9`
Contract: `docs/goals/2026-10-18-tui-exploration-group.md`, R1–R3.

## Result

Cause: `exploration_kind` excluded failed/denied/cancelled rows and every
`preview_limited()` result, i.e. a Read of a large file or a Grep with many
matches. Those rows rendered standalone, each with its own blank spacer, so most
of the owner's screenshot stayed uncollapsed. OC2 (`grouping/session.ts:77-90`)
admits by tool name only.

- R1: adjacent Read/Glob/Grep/WebFetch rows collapse under one muted
  `→ Explored: N reads, M searches, K fetches` header (`⋯ Exploring:` while any
  member runs). Truncated, failed, denied, cancelled and no_gain members join;
  only crash-recovery `unknown`, permission-pending and argument-stream rows stay
  outside. Reasoning/text/other tools still split runs.
- R2: expansion key and click toggle are unchanged (first member op, collapsed by
  default). Expanded members reuse the standalone inline row without its top margin:
  no blank rows, OC2 icon (`→` Read/Skill, `✱` Glob/Grep, `%` WebFetch), muted
  color, failed members in error color with their error text. Full, indexed-visible
  and part-position renderers share the multi-line member.
- R3: standalone named inline rows show their OC2 icon and muted color;
  warning/error/strike states are unchanged; generic `✓/✗`, Question and Execute
  are unchanged.

No storage schema, migration, config or provider change.

## Checks

- `cargo nextest run -p oc-tui`: 579/579 pass. Updated contracts:
  `exploration_groups_failed_denied_truncated_but_not_unknown` (replaces the
  former never-hide test, per owner decision), expanded-group parity without blank
  rows, running/failed header status, new screenshot reproduction
  `exploration_runs_split_on_reasoning_and_count_fetches`, inline goldens and
  the truncated-Read click case in `app/tests/transcript.rs`.
- `cargo fmt --all --check` and `cargo clippy -p oc-tui --all-targets --locked -- -D warnings`: clean.
- Full offline gate `bash scripts/test.sh` with runbook disk `TMPDIR`
  (`~/.cache/oc-slot-1-tmp`): 2089/2090 pass, 12 existing skips, 379.66s nextest,
  6:25 wall. Single failure:
  `oc2::mcp_application lifecycle::mcp08_actual_retry_and_reload_location_retire_pending_scope_without_late_effects`
  (`owned child survived scope retirement`, `kill(pid,0)` still succeeds after the
  fixture's `closed` counter). It reproduces on the base without T70 code
  (`f33adc2a8`, T70 diff stashed): 3/8 isolated runs fail; 2/6 fail on the branch.
  The T70 diff touches only `crates/oc-tui` rendering. With owner approval
  (2026-10-19) this is recorded as a pre-existing defect outside T70, not hidden.
  Next engineering step: separate task on the MCP owned-child reap ordering
  (reap/wait before publishing `closed`, or probe after waitpid) in the MCP owner.
- The first gate attempt ran without `TMPDIR`; nine `crates/oc` tests failed in `/tmp`
  (`untrusted_ancestor` in `scripts/bounded_live.py`, startup path leak checks, one
  load-sensitive PTY case). All passed with the runbook TMPDIR; no code was changed for this.
- `cargo build -p oc2 --bin oc2 --locked`: debug `target/debug/oc2` built, `--version` → `oc2 0.2.0`.

## Not done

No live/paid campaign (not required). Branch not merged to master.
