# Goal: Config compatibility parity + full subagent system

Status: active
Source: user instruction 2026-09-21 ("исправить баги и убрать лимиты; далее реализовать систему саб агентов полностью"), plus upstream reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (tree SHA `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-09-27

## Objective

Bare `oc` loads the owner's real `~/.config/opencode` config without spurious warnings, and the product implements the upstream v2.0.12 subagent system end to end (subagent-mode agents, task spawning, child sessions, command subtask/agent routing, permissions, result delivery), verified by workspace gates and a live run.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: Markdown config loading matches upstream v2.0.12 semantics for the 7 owner-visible warnings.
  - Source: owner report of warnings from `target/release/oc` + upstream tag v2.0.12 sources.
  - Acceptance: with the owner config, no warning is emitted for: (a) commented `#model:` lines, (b) `permission.external_directory` glob maps, (c) skill unknown frontmatter fields (`license`, `compatibility`), (d) skill dirs without `SKILL.md` (silent skip), (e) `command.agent`/`subtask` frontmatter, (f) `agent: file too large` on a 30 KiB agent, (g) `command: file too large` on a 41 KiB command. Definitions that upstream loads must load; nothing that upstream accepts may be rejected.
  - Primary evidence: targeted tests over fixtures mirroring the owner shapes + a real `target/release/oc run` start showing zero warnings for those paths.
  - Status: verified
  - Evidence: `evidence/T43/report.md`; owner config start (real HOME) emits zero config diagnostics; fixtures in `defs::tests::commented_frontmatter_and_glob_permissions_follow_upstream`, `defs::tests::large_agent_and_command_bodies_load_within_the_global_budget`, `config::tests::skill_frontmatter_comments_and_optional_metadata`.

- R2: Artificial size limits removed (upstream has none).
  - Source: owner instruction "убрать лимиты"; upstream v2.0.12 has no per-file/frontmatter/description caps (only unrelated 256 KiB for API-managed instruction entries).
  - Acceptance: no size-based diagnostic for agent/command/skill files, frontmatter, description, name, or body within realistic configs; at most one generous total guard remains (recorded assumption: `MAX_TOTAL_BYTES` raised to 4 MiB) and it never fires on the owner config.
  - Primary evidence: tests with oversized-but-realistic fixtures + the owner-config live start.
  - Status: verified
  - Evidence: `evidence/T43/report.md`; `MAX_DEF_BODY`/`MAX_SKILL_FILE`/frontmatter/description caps removed, `MAX_TOTAL_BYTES = 4 MiB`, `COMMAND_BYTES_CAP`/`SKILL_BODY_CAP = 1 MiB`; 41 KiB command expands (`crates/oc-adapters/tests/runtime.rs::command_expansion_is_single_bounded_pass`).

- R3: Subagent system fully implemented per upstream v2.0.12.
  - Source: owner instruction "реализовать систему саб агентов полностью"; upstream v2.0.12 (`task` tool, session parentID, agent modes, command `subagent`/`subtask`, permission action, result delivery).
  - Acceptance: (a) `mode: subagent` / `mode: all` agents load and are runnable; (b) the model can spawn a subagent task and receive its result in the parent session; (c) child sessions are persisted with parent linkage and visible in history/TUI; (d) command frontmatter `agent`/`model`/`subagent`/`subtask` executes with upstream semantics (`subtask:false` inline with agent selected, subagent mode/`true` → child session); (e) permissions gate task spawning (`task`/`subagent` action, deny blocks); (f) DCP `experimental.allowSubAgents` is honored without a warning once supported; (g) cancellation and failure paths reap child sessions.
  - Primary evidence: fake-server integration tests for spawn/result/cancel + permission denial, plus a bounded live run spawning one subagent on the configured provider.
  - Status: pending
  - Evidence:

- R4: Every slice is committed and pushed to `origin agent/oc-rust-port`.
  - Source: owner instruction "коммиты пуши делай".
  - Acceptance: `git status` clean, each slice has a commit, branch pushed.
  - Primary evidence: `git log --oneline`, `git status`, push output.
  - Status: pending
  - Evidence:

- R5: Workspace gates stay green after each slice.
  - Source: repo AGENTS.md / GOAL.md A01.
  - Acceptance: `cargo fmt --all` clean, `cargo clippy --locked --workspace --all-targets -- -D warnings` exit 0, `cargo test --locked --workspace --no-fail-fast` 0 failures, `cargo build --locked --release` succeeds.
  - Primary evidence: command output captured per slice.
  - Status: pending
  - Evidence:

- R6: Primary-agent profiles match pinned OC2 v2.0.12.
  - Source: owner-approved 2026-09-27 RECON/plan for Build, Plan and custom Markdown profiles; pinned references below.
  - Owner: T45 backend/CLI; T44 consumes the same owner-backed catalog/selection under VIS06/VIS10/VIS17. No second task-state engine or circular completion dependency.
  - Acceptance: built-in Build and Plan, configured overrides and default_agent selection/fallback follow OC2. Build is the initial default unless a valid configured default wins. Plan edit restrictions, explicit-user-request plan-directory exception, enter/leave reminders and context reconciliation after compaction/Revert work through real runtime operations, including reopen. Switching agents, not an implementation request in the prompt, leaves Plan.
  - Acceptance: custom Markdown discovery supports global/project agent/agents and compatibility mode/modes roots, pinned recursion/ID derivation/source order and native/legacy frontmatter. Absent mode defaults to primary for a new profile; primary/subagent/all eligibility is preserved. Later definitions merge supplied fields rather than replacing the whole profile; permissions follow pinned ordered composition within the preserved native authority boundaries.
  - Acceptance: system/body instructions, model/variant, request settings/headers/body, steps, color, mode/hidden/disabled have actual execution/presentation semantics, not silent acceptance. Native and compatibility forms are checked against the donor importer/migration rather than guessed from V1 docs. Hidden/disabled/subagent-only profiles follow donor picker/cycle/default eligibility; explicit addressing is checked separately.
  - Acceptance: headless run --agent selects/switches the profile before prompting; omission preserves an existing session's profile. Session selection, instructions and model/variant remain truthful through switch/reopen/reload, without silent substitution of Build.
  - Primary evidence: pinned TS fixtures for normalized definitions/merge/default selection plus captured fake-provider requests, independently verified tool effects and session/restart assertions; T44 paired picker/prompt captures prove UI parity separately. Rebuild and exercise target/release/oc. Existing workspace/live gates remain mandatory.
  - Status: pending
  - Evidence:

### Constraints

- C1: Rust 2024, modular monolith, core independent of UI, KISS/YAGNI; no Node/Bun/JS host in production.
- C2: No hardcoded production model IDs, no vendor-specific routes, no secrets/raw live responses in logs or artifacts.
- C3: Do not weaken existing contracts to gain green checks (MCP attach failure stays fatal, validation/security/accessibility/concurrency guarantees intact, no suppressed tests).
- C4: Progress engine `scripts/progress.py` is the only status source; one active task at a time.
- C5: Plan-directory admission is narrowly scoped to donor-compatible `~/.opencode/plan`; no general trusted-root widening. Map donor edit permissions to native apply_patch, preserving explicit Deny, parent/child narrowing, symlink/CAS protections and the single apply_patch contract. Shell follows its own policy as in OC2; Plan is not a shell sandbox. Do not introduce write/edit built-ins or a JS plugin host. Remaining native policy/tool differences must be explicit and cannot count as full parity.

### Non-goals

- OAuth, ChatCompletions fallback, daemon/serve/attach, Code Mode, JS/TS/WASM plugin host, cloud orchestrator.
- Unrelated refactors or audits; scope expands only on owner instruction or a diff-caused regression.

## Change Envelope

- Target: `crates/oc-adapters/src/{defs.rs,config.rs,composition.rs,runtime.rs,application.rs,tools.rs}`, `crates/oc-core/src/*`, `crates/oc-tui/src/*`, `crates/oc/src/*`, tests under `crates/*/tests/`, `docs/*`, `evidence/*`.
- Expected paths, symbols, and direct consumers: `split_frontmatter`, `parse_skill`, `load_kind`, `insert_agent`, `insert_command`, `normalize_permission`, `MAX_*` constants; subagent work touches session/tool/runtime composition and TUI rendering.
- R6 adds agent definition/merge/default selection, native Build/Plan registration and reminder lifecycle, narrowly admitted Plan-directory patch operations, headless CLI selection and existing T44 picker/cycle consumers. Reuse application/session/config owners; no new agent framework or separate persistence engine.
- Allowed and forbidden artifacts: source, tests, docs, evidence notes. Forbidden: editing GOAL.md/audit gates to pass, deleting tests, adding JS runtime, committing secrets or `.local/` contents.
- User or harness budget: commits + pushes required; no attempt limit; live calls bounded.

## Current Checkpoint

- Closes: R1, R2
- Smallest next action: implement the config-compat fixes and limit removal in `defs.rs`/`config.rs` with fixture tests.
- Expected evidence: targeted tests green + owner-config start without the 7 warnings; workspace gates green.
- Stop or replan if: upstream semantics contradict the owner's config intent or a fix breaks an existing audited contract test.

## Current State

- Resolved: owner-visible diagnosis and upstream research (documented in conversation and in this contract).
- Last relevant evidence: upstream v2.0.12 research (no size limits; skill schema `name?/description?/metadata?`, unknown fields ignored, dirs without SKILL.md skipped; permission scalar-or-map; commands accept `agent`/`model`/`subagent`/`subtask`; js-yaml comments; gray-matter parse behavior). Local code map with file:line references.
- Blocker: none.
- Next: R1+R2 implementation slice (delegated), then R3 research + implementation slices, committing/pushing each.

## Material Decisions

- 2026-09-27: Owner approved R6 primary-profile parity and pinned references. T45 owns backend/CLI; T44 owns UI qualification. Implementation and acceptance remain pending; T43's historical verified warning fixtures do not prove full profile parity.

- 2026-09-22: D13 in `docs/DECISIONS.md` supersedes only C3's historical fatal-MCP-attach clause; cancellation/cleanup/caps and all security invariants remain. D15 and `evidence/T43/backend-permissions.md` restore ordered resource semantics, including selected primary and child narrowing.
- 2026-09-22: T43 owns R1/R2 + foreground slices 1–4 and permission/config prerequisites. T45 owns remaining R3 slices 5–8, built-in agents and outstanding typed metadata semantics. T45 no longer depends on T43 being marked done: foreground code already landed in `7895f43`; no circular completion requirement. Full R3 remains pending, not waived. Current backend work does not modify T44 or claim TUI closure.

- 2026-09-21: Limits interpretation — remove the artificial caps that caused owner-visible warnings; keep a single 4 MiB total guard as a documented resource safety net (narrowest safe reading of "убрать лимиты"). If the owner rejects even that, remove it.
- 2026-09-21: `mode: subagent`/`all` definitions must load now (no diagnostic) even before execution lands, because the owner config uses them and requested "не блокировать".

## Checkpoint History

- 2026-09-21: contract frozen; R1–R5 recorded; implementation delegated to subagents with orchestrator verification.
- 2026-09-21: R1/R2 verified (`evidence/T43/report.md`); discovered extra owner shape `tavily-local_*` permission key and the 4 KiB `COMMAND_BYTES_CAP` invocation blocker, both fixed in-slice. Next: R3 slices 1–8.

## Primary-profile references — R6

All references are pinned to `2670273ff17da96f85c5826ced57aa1b368754fa`.
IDs are registered in [SOURCES.json](../../tui-recovery/SOURCES.json).

- U25 — [Agent defaults](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/schema/src/agent.ts#L38-L53); [Build registration](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/plugin/agent.ts#L86-L92).
- U26 — [Plan policy, reminders, switches and reconciliation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/plugin/plan.ts#L14-L120).
- U27 — [Agent merge](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/plugin/agent.ts#L88-L124), [Markdown importer](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/plugin/agent.ts#L164-L211), [source discovery](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/discovery.ts#L23-L83), [V1 field migration](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/v1/config/migrate.ts#L141-L163).
- U28 — [Default selection and fallback](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/agent.ts#L92-L133).
- U29 — [Picker eligibility and cycle](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/context/local.tsx#L56-L123); [picker](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/component/dialog-agent.tsx#L6-L29); [keybindings](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/config/keybind.ts#L169-L171).
- U30 — [Headless target selection](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/cli/src/session-target.ts#L42-L85); [switch before prompt](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/cli/src/run/noninteractive.ts#L644-L648).

## Completion

- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
