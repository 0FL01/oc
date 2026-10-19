# Backend blockers after v2.0.12 audit

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: active (integration permission resolved; T27 engineering/live remains open)
Source: owner instruction 2026-09-22: document and iteratively fix blockers/bugs;
original handoff excluded T44 (another agent owned it). Owner approval 2026-09-27
adds canonical effort ordering after RECON and permits coordinated T44 plan amendments.
Reference: upstream v2.0.12, commit `2670273ff17da96f85c5826ced57aa1b368754fa`.

## Frozen outcomes and ownership

- R1 / T47: unknown model limits do not unconditionally reject a turn. Use explicit
  native fallback budgets with a visible warning, keep metadata/UI unknown and
  discovery DISC05 unchanged. No selected variant means no variant overlay.
  Evidence: admission/selection tests and actual runtime fake-provider request.
- R2 / T43: permission resource maps retain resource semantics and cannot broaden
  central policy; agent metadata accepts relevant upstream fields without silently
  granting capabilities. Evidence: config and runtime permission regression tests.
- R3 / T46: preserve usable MCP structured/content results and sanitized error
  detail; propagate server instructions only for permitted attached servers.
  Preserve D13 degradation, cancellation, cleanup, caps and secret redaction.
  Evidence: remote/stdio adapter and runtime integration tests.
- R4 / planning + T27: fix acceptance namespaces and T43/T45 dependency deadlock;
  require final qualification after backend changes; permit discovery-selected
  models in bounded live harness without relaxing model validation or live opt-in.
  Evidence: validator negative tests, docs/journal checks, offline harness tests;
  live execution only within existing explicit bounded authorization.
- R5 / T45 plan: retain background/notices/reap, command routing, DCP child behavior,
  built-in agents and hidden/permission-filtered catalog as explicit remaining
  scope. Do not claim the whole subagent system finished by fixing its prerequisites.
- R6 / T47 / VAR01 (approved 2026-09-27; pending): normalize available variant order
  under [Canonical effort ordering](../CONTRACTS.md#canonical-effort-ordering--t47var01-approved-2026-09-27-pending).
  Known levels progress none/minimal/low/medium/high/xhigh/max; Default surrounds the
  named cycle, custom entries follow known ranks, absent/disabled levels are skipped.
  Explicit known effort outranks the name; absent effort may rank by standard name,
  explicitly unknown effort may not. Stable ties/custom order, exact aliases/IDs/wire
  and selection durability are mandatory, not alphabetical sorting or a new allowlist.
  Evidence: shared ordering fixtures, effective merge/snapshot and actual binary
  PTY/fake Responses selection/restart/refresh checks; T44 visual qualification separate.

## Original constraints and execution envelope (2026-09-22 handoff)

No changes to T44 task entry/state/leaves/spec/evidence, `crates/oc-tui`, TUI
capture/tests or `crates/oc/src/tui_cmd.rs`. Do not stage `.opencode/` or user ZIP.
No rewriting historical evidence, GOAL, baseline, gates or discovery oracle.
Unknown limits are not discovered capacities. No model-name allowlists.
No raw secrets/live responses; no widening permissions or suppressing failing tests.
MCP prompts/resources are not silently declared a non-goal or claimed delivered.

The owner explicitly paused the T44 agent and authorized independent backend work
without touching T44. Keep journal current=T44 and one-active-task invariant;
track this temporary backend execution here rather than forging a T44 transition
or a second active journal task. T47 is registered todo until the slot is released.
This is an execution exception, not evidence that tasks are finished.

Current R6 envelope: existing model/catalog helper in `oc-adapters/src/models.rs`,
`application.rs::EffectiveSelection::snapshot`, existing catalog consumers/diagnostic
lists and `oc-tui/src/picker.rs`/`app.rs`/`oc/src/tui_cmd.rs`; nearest tests/fixtures/docs.
The historical T44 exclusion above does not prohibit the approved R6 integration or
T44 plan update; keep one mutation owner and coordinate presentation under T44, not a
second active task. Shared rank helper, no competing `names.sort()` path or global JSON
sort; source discovery allowlist/validation and provider wire unchanged. No new task,
store, dependency, paid campaign, execution status or retroactive PASS.

## Canonical effort ordering

R6/VAR01 remains pending/NOT_RUN; earlier R1 evidence does not qualify ordering.
Behavioral cases:

1. Freeze nearest effective variants fixtures: permutations/subsets/disabled, reserved
   default vs named none, effort/name conflicts, absent vs unknown effort, case/whitespace,
   equal-effort aliases and unknown custom order. Keep existing zeta/alpha custom-cycle
   regression source-ordered, not alphabetical.
2. Add one stable rank helper after effective merge; reuse for owner snapshots, picker/
   cycle and applicable enabled-list diagnostics. Verify local overrides rank the final
   value, not remote input, and no consumer can put known max before known xhigh.
3. Actual binary/PTY proves picker and full Ctrl+T cycle agree, preserve model/text draft,
   shortcut sends no request and read-only/authority guards hold. The 2026-10-01 live
   model-selection clarification supersedes only blanket busy refusal: variant choice
   is a draft, captured send or blank Enter commits, next request of the same task uses
   it while prepared requests/tools retain their model/variant. Reuse TOOL12/PRM01,
   not a second switching owner or change to canonical ranks. Fake Responses proves exact
   selected effort and no overlay after Default, no inferred effort from rank/name.
4. Verify persisted identity through reorder/refresh/session reopen/process restart;
   retired/disabled choice remains visible/actionable until explicit recovery, with
   existing stale-cycle and empty/all-disabled no-op behavior intact.
5. T44 VIS09/VIS29 pairs geometry/colors/cursor using an identically canonical supplied
   order, records intentional ordering differences with an unsorted supplied fixture
   without masking or universal pixel PASS. T50 TOOL18 consumes the shared ordered
   variants view, while retaining its own provider/newest-family model grouping order.

VAR01 has only T47 as detailed owner; DISC06/T14 and UI02/T39 remain regressions, not
duplicated ownership. A03/A04/A08/A13 and relevant workspace gates remain required;
minimal T47 → T44/T50 slice prerequisites are not whole-task done-dependencies.
