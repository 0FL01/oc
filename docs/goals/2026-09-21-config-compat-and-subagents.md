# Goal: Config compatibility parity + full subagent system

Status: active
Source: user instruction 2026-09-21 ("исправить баги и убрать лимиты; далее реализовать систему саб агентов полностью"), plus upstream reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (tree SHA `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-09-21

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

### Constraints

- C1: Rust 2024, modular monolith, core independent of UI, KISS/YAGNI; no Node/Bun/JS host in production.
- C2: No hardcoded production model IDs, no vendor-specific routes, no secrets/raw live responses in logs or artifacts.
- C3: Do not weaken existing contracts to gain green checks (MCP attach failure stays fatal, validation/security/accessibility/concurrency guarantees intact, no suppressed tests).
- C4: Progress engine `scripts/progress.py` is the only status source; one active task at a time.

### Non-goals

- OAuth, ChatCompletions fallback, daemon/serve/attach, Code Mode, JS/TS/WASM plugin host, cloud orchestrator.
- Unrelated refactors or audits; scope expands only on owner instruction or a diff-caused regression.

## Change Envelope

- Target: `crates/oc-adapters/src/{defs.rs,config.rs,composition.rs,runtime.rs,application.rs,tools.rs}`, `crates/oc-core/src/*`, `crates/oc-tui/src/*`, `crates/oc/src/*`, tests under `crates/*/tests/`, `docs/*`, `evidence/*`.
- Expected paths, symbols, and direct consumers: `split_frontmatter`, `parse_skill`, `load_kind`, `insert_agent`, `insert_command`, `normalize_permission`, `MAX_*` constants; subagent work touches session/tool/runtime composition and TUI rendering.
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

- 2026-09-21: Limits interpretation — remove the artificial caps that caused owner-visible warnings; keep a single 4 MiB total guard as a documented resource safety net (narrowest safe reading of "убрать лимиты"). If the owner rejects even that, remove it.
- 2026-09-21: `mode: subagent`/`all` definitions must load now (no diagnostic) even before execution lands, because the owner config uses them and requested "не блокировать".

## Checkpoint History

- 2026-09-21: contract frozen; R1–R5 recorded; implementation delegated to subagents with orchestrator verification.
- 2026-09-21: R1/R2 verified (`evidence/T43/report.md`); discovered extra owner shape `tavily-local_*` permission key and the 4 KiB `COMMAND_BYTES_CAP` invocation blocker, both fixed in-slice. Next: R3 slices 1–8.

## Completion

- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
