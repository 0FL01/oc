# Goal: Config compatibility parity + full subagent system

Status: active
Source: user instruction 2026-09-21 ("исправить баги и убрать лимиты; далее реализовать систему саб агентов полностью"), plus upstream reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (tree SHA `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-09-27

## Objective

Bare `oc` loads the owner's real `~/.config/opencode` config without spurious warnings, and the product implements the pinned OC2 subagent/profile/context system end to end, plus owner-approved Linux host context, explicit parent-message context packs and long-horizon child DCP. Donor parity and native extensions/differences are qualified separately by actual requests, effects, workspace gates and the existing bounded live run.

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
  - Source: owner instruction "реализовать систему саб агентов полностью", 2026-09-27 concurrency RECON and approval; pinned OC2 `subagent` tool (legacy `task` is a permission migration alias), session parentID, agent modes, command routing and result delivery.
  - Owner: T45 remaining backend/CLI/history slices; preserve T43's landed foreground prerequisites and coordinate existing T44 child surfaces without a completion cycle.
  - Acceptance: (a) subagent/all profiles load and run; built-in General and Explore have actual donor system/tool restrictions, default depth 1 and caller-gated spawning; model selection is explicit override → child profile → parent, while continuation retains its own selection unless explicitly switched. Primary-only/unknown profiles and foreign sessionID fail before child creation. (b) Foreground is the default; multiple independent subagent calls in one response can run concurrently, and the next parent model step waits for the batch's terminal results. (c) background:true starts independently before returning running/sessionID; it can make progress while the parent turn is still active, not only after parent completion. Parent receives an owner-generated terminal notice without polling or duplicate work.
  - Acceptance: (d) children have fresh history plus the delegated prompt, their own profile and applicable workspace context, not the parent transcript or parent agent.system; sessionID continues the child's own history. Child rows retain parent/Location linkage and are visible in history/TUI. (e) command.subagent ?? command.subtask wins over mode inference; explicit false routes inline with agent/model switch ordering, child commands route background without changing parent selection. (f) cancellation/shutdown reaps owned work; per-session scheduling, bounded in-flight/queued work and mutation safety replace a global parent-held single-flight deadlock, without serializing all children behind the parent.
  - Acceptance: (g) durable background recovery verifies parent/child identity, delivers an already committed terminal result without re-execution and uses pinned donor recovery for safely resumable work. Do not claim exactly-once execution: donor recovery is at-least-once. Native started/unknown mutation, shell or MCP operations are never auto-replayed; unresolved effects require explicit recovery. Notifications are deduplicated by durable job generation/delivery identity, including crash at delivery. (h) permissions/parent-child narrowing and R9 child DCP remain authoritative; neither background nor context transfer widens authority.
  - Primary evidence: SUB01/SUB02 scripted provider barriers prove overlapping child execution and background progress before parent completion; fake-server spawn/continuation/model/depth/deny/cancel/command/restart tests prove effects and durable outcomes, plus the existing bounded live run spawning one subagent. Time-independent ordering barriers, not fragile elapsed-time claims; rebuild/exercise the actual binary.
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

- R7: Every model request receives truthful Linux execution-environment context.
  - Source: owner-approved Linux host-environment proposal and plan diff, 2026-09-27.
  - Owner: T45 backend; reuse the existing runtime prompt assembler.
  - Acceptance: a native collector obtains hostname, OS/distribution, kernel release, machine/process architecture, process pointer width, available CPU parallelism and effective UID/GID without root, sudo, additional Linux capabilities or shell subprocesses.
  - Acceptance: the prompt includes the actual tool execution environment: working directory, workspace root, Git repository yes/no, configured shell executable and approved temporary directory. Do not infer the tool shell from $SHELL. Include the environment/date baseline and explicitly implement the missing base harness prompt fallback for an empty agent.system; custom agent.system replaces that base prompt, not the environment layer.
  - Acceptance: host/workspace context is a separate harness instruction layer; custom agent.system does not replace it. Root and child requests use the same assembler and describe their actual execution context.
  - Acceptance: collected fields are bounded and rendered deterministically, including escaping control characters and block delimiters in externally supplied values. Optional unavailable fields are omitted or marked unknown; collection failure does not prevent startup. os-release is parsed as data, never executed or sourced.
  - Acceptance: architecture and CPU parallelism describe the execution environment, not an assumed physical host. Distinguish machine and process architecture; do not equate online CPU count with available parallelism or advertise filesystem/tool access as granted.
  - Acceptance: restart and workspace switching refresh the relevant context. Each request uses one immutable snapshot; tool continuation does not accumulate duplicate environment messages. Preserve the existing immutable Location/config-generation boundary.
  - Primary evidence: collector fixtures covering available/unavailable fields, captured fake-provider root/child requests with default and custom profiles and workspace switching, plus target/release/oc exercised under a non-root account with no effective capabilities. Verify the fallback/environment/date and absence of duplicate or unselected metadata in actual requests.
  - Status: pending
  - Evidence:

- R8: Delegation attaches exact selected parent context without duplicating automatic child instructions.
  - Source: owner-approved 2026-09-27 context_message_ids and child-context/capability-preview discussion. This is a native extension; omission preserves OC2 fresh-context delegation.
  - Owner: T45, existing subagent schema/runtime/storage/provider projection; no transcript-sharing service or new history store.
  - Acceptance: optional context_message_ids selects text user/assistant messages from the calling parent session only. Runtime exposes stable message IDs with their text/provenance independently of DCP enabled/manual/permissions, using a bounded visible index rather than advertising the whole archive. Runtime-owned IDs/delimiters cannot be forged by user text. System/harness-only messages, opaque reasoning and raw tool items are not candidates; unsupported attachments/content are diagnosed rather than silently omitted.
  - Acceptance: resolve all IDs before new child creation/prompt admission against one parent conversation revision and the invoking request's cutoff. Reject unknown, cross-session, reverted/excluded or not-yet-visible messages. Explicitly referenced retained raw text covered by DCP/native compaction may be selected if it belongs to the active conversation branch; compression is not Revert. Exact text/roles/origin are preserved; repeated IDs deduplicate and output follows parent chronology, not argument order. No fuzzy extraction or regex/keyword summarization of model text.
  - Acceptance: attach an escaped, provenance-labelled quoted user-context pack alongside the current delegated task, never promote parent messages to system/developer instructions or impersonate live child assistant/tool messages. No unselected parent transcript or parent agent.system is copied. The child's own system/AGENTS/tools remain distinct. Caller-supplied task restrictions are still necessary even when tools permit broader work.
  - Acceptance: count IDs, payload bytes and model-context cost explicitly; reject an oversized pack before admission with an actionable outcome, not truncation or hidden summarizer calls. Fix the snapshot/digest and source revision durably with the child's admitted task; background, parent Revert/compaction, restart and sessionID continuation cannot change it retroactively. A continuation adds its new pack once with its new task; durable replay cannot duplicate the admission.
  - Acceptance: subagent guidance tells the orchestrator that own profile, environment, applicable AGENTS, permitted tool schemas, skill metadata and allowed MCP guidance are assembled automatically, while conversation/findings must be supplied in prompt/IDs. Skill bodies are loaded through native skill, not copied automatically; nested AGENTS may load on read. The existing subagent preview reports effective tools/restrictions and own-history compress availability from policy/config owners, including depth/config gates, not description claims. Conditional/ask/resource rules are not advertised as unconditional access; deny is never advertised as allow. Shared workspace is not a separate filesystem sandbox. Do not semantically strip exact selected user quotations merely because they mention AGENTS or earlier instructions.
  - Primary evidence: CTX01/CTX02 captured parent/child requests prove exact selected text/roles/order and absence of unselected or duplicate harness content; invalid/reverted/foreign/over-budget IDs create no child or input; DCP-off IDs, frozen background pack, restart and continuation remain valid; preview and actual deny/ask execution agree.
  - Status: pending
  - Evidence:

- R9: DCP is default-enabled for children and sustainable across long-horizon repeated compression.
  - Source: owner-approved 2026-09-27 child compress default and removal of lifetime-exhaustion behavior. No total compress-call quota was found in audited native code or pinned DCP 3.1.15; the owner's historical OC1 failure is not attributed to a specific cause without its version/error. This outcome prevents exhaustion rather than inventing a counter to remove.
  - Owner: T45 child DCP/runtime/storage, using A07/A10 and existing resource gates; OC2 native compaction remains a separate mechanism.
  - Acceptance: dcp.experimental.allowSubAgents defaults to true; explicit false disables model-facing compress/anchors/nudges/automatic DCP strategies for children only. Global enabled:false and manualMode retain their meaning; effective compress Deny and central/parent-child ceilings win. Parsing the option has actual behavior and no obsolete unsupported warning. Built-in Explore gets only a narrow own-session compress grant alongside its read-only policy; custom wildcard/explicit deny is not overridden. Preview follows the effective result.
  - Acceptance: blocks/anchors/nudges/cadence/tool projections/protections are session/generation scoped in foreground/background/root/parallel children; child compression never mutates a parent's or sibling's projection. A child can compress only closed completed ranges with intact tool-call/result/reasoning groups. Raw history is immutable and durable counters are not an admission quota.
  - Acceptance: protect the current delegation task and its attached context pack from child model-driven compress while that admitted task is pending/running. On terminal completion the task is no longer in-flight; on safe recovery reinstate its protection. Old completed tasks/packs become eligible for normal configured DCP protections and compression, so continuation does not accumulate permanent new fixed lanes. Never auto-execute a queued next task to evade protection. Current requirements remain preserved/reconstructed through native compaction as well, with irreducible context-budget failures reported explicitly.
  - Acceptance: no default session/process-lifetime successful-compression-call or block-count quota; number/age of previous compressions alone never disables compress. Repeated recompression of valid shrinking closed summaries must not inevitably exhaust nested depth or load the full historical block archive. Normalize/flatten required references or use an equivalent bounded live representation transactionally; preserve stable IDs, placeholder semantics, protected bytes, provenance and replay without mutating raw history or deleting reachable archive. Increasing recursion depth is not the solution.
  - Acceptance: load only addressed ranges/active blocks and necessary dependencies, with bounded traversal/serialization; resident/planning state does not scale linearly with inactive archive. Preserve per-call payload/range bounds, cycle/stale/cross-session checks, no-gain outcome, active-memory/model budget admission and per-turn tool-round/retry limits. A bad/no-gain compression does not spend a lifetime attempt. Recovery/compress diagnostics must not create a circular "compress to proceed" requirement whose own admitted path cannot read the bounded addressed ranges. Finite protected data/disk/model budgets are explicit failures, not arbitrary call exhaustion.
  - Acceptance: delegation/DCP guidance distinguishes work summaries from protected active task/pack; preserve facts/paths/decisions/open questions/next step and repository journal/checkpoint references, without hidden paid summarization, automatic file writes or Git commits. Compress may discard verbose closed investigation from projection, not raw records. Permission to compress is not an instruction to compress after every read.
  - Primary evidence: DCP10/DCP11 captured root/parallel child requests and SQLite assertions cover default/false/off/manual/deny/Explore, isolation and protection lifecycle. A frozen repeated compress → continue → recompress → restart workload with invariant active content compares small/large inactive archives and measures peak/retained bytes, loaded rows/dependency depth/tasks/queues. It crosses the old depth pattern without count exhaustion, preserves control facts/graph/provenance, and keeps actual-binary cleanup and A10 thresholds. Cycle count is evidence, not a product lifespan limit; one-compression E2E does not prove sustainability.
  - Status: pending
  - Evidence:

- R10: The shared prompt/instruction lifecycle follows OC2 within the native tool/trust contracts.
  - Source: owner request for the complete system/tools/AGENTS/profiles/skills/subagents assembly scheme and approval to update all discussed work, 2026-09-27; supplements R6/R7 rather than declaring their existing implementation complete.
  - Owner: T45 runtime/config/instructions; existing T44 consumers qualify UI separately.
  - Acceptance: selected nonempty agent.system replaces the base OpenCode harness prompt; empty/default profiles get the base fallback. Retain environment/date, applicable AGENTS, bounded skill metadata, permitted MCP and native DCP guidance independently. Advertise real tools through provider descriptions/JSON schemas, with truthful native apply_patch/bash guidance and no write/edit built-ins. Skill preview has pinned deterministic ordering/permission filtering; missing description or autoinvoke:false excludes automatic preview, not legitimate explicit skill use. Bodies remain absent until bounded native skill result; subagent bodies are not broadcast to parent. Model-specific donor prompt hooks are documented/checked separately; do not add hardcoded model-ID matching or vendor routes to claim parity.
  - Acceptance: initial global/project AGENTS discovery has pinned order/canonical dedup/provenance. Successful read admits applicable nested AGENTS as chronological instruction messages once; unchanged repeats do not duplicate content. Initial instruction baseline is durable; admitted changes/removals append chronological updates without retroactively rewriting old history. Reconcile applicable instructions/reminders after compaction/Revert/reopen without stale text, lost rules or duplicate fragments. Discovery respects admitted canonical Location/source boundaries and existing path policy; outside-boundary donor discovery differences are explicit, not a trust bypass.
  - Acceptance: each root/child turn pins one immutable config/Location generation and each request one context revision. Watcher changes are admitted at safe boundaries for subsequent turns, never mixed into an in-flight generation; read-derived fragments carry pinned origin/revision. No automatic skill-body attachment or CodeMode host is introduced to mimic donor features excluded by A13.
  - Primary evidence: PRM01 pinned source-derived fixtures and captured requests for Build/custom/General/Explore, initial/nested AGENTS, changed/removed files, repeated read, compaction/Revert/restart and Location A→B. Assert actual content/order/roles/provenance, policy-filtered previews, no stale instructions and truthful tools; declared native differences remain separate from donor parity evidence.
  - Status: pending
  - Evidence:

### Constraints

- C1: Rust 2024, modular monolith, core independent of UI, KISS/YAGNI; no Node/Bun/JS host in production.
- C2: No hardcoded production model IDs, no vendor-specific routes, no secrets/raw live responses in logs or artifacts.
- C3: Do not weaken contracts to gain green checks: validation/security/accessibility/concurrency guarantees stay intact, no suppressed tests. MCP per-server degradation follows D13; its fatal cancel/cleanup/caps remain. The original fatal-attach rule is historical/superseded, not a current contradiction.
- C4: Progress engine `scripts/progress.py` is the only status source; one active task at a time.
- C5: Plan-directory admission is narrowly scoped to donor-compatible `~/.opencode/plan`; no general trusted-root widening. Map donor edit permissions to native apply_patch, preserving explicit Deny, parent/child narrowing, symlink/CAS protections and the single apply_patch contract. Shell follows its own policy as in OC2; Plan is not a shell sandbox. Do not introduce write/edit built-ins or a JS plugin host. Remaining native policy/tool differences must be explicit and cannot count as full parity.
- C6: Host metadata contains only explicitly selected fields. Do not project the full process environment, machine-id, network configuration or raw /proc files. Treat hostname and os-release values as bounded data, not instructions. No automatic toolchain inventory, network probes or privilege escalation. Host metadata does not grant or bypass native tool permissions. Free RAM/disk/load and toolchain versions are measured on demand, not injected as a changing inventory on every request. Keep native tool guidance truthful; no hardcoded production model IDs or copied guidance for absent write/edit tools.
- C7: No full-parent transcript cloning, quote-to-system privilege promotion, cross-session context access, unknown-effect auto-replay, unbounded active summaries/notification queues or disabling resource checks to simulate infinite sessions. Concurrency preserves one execution owner per session and existing patch CAS/symlink protections. Native child policy narrowing is an explicit difference, not identical donor permission algebra.

### Non-goals

- OAuth, ChatCompletions fallback, daemon/serve/attach, Code Mode, JS/TS/WASM plugin host, cloud orchestrator.
- Owner-approved tool follow-up is tracked separately by [T50](2026-09-27-native-tool-parity.md): shell/search/question/read/fetch and direct model/session tools. Built-in websearch/provider integrations, built-in browser and PDF remain excluded; explicit MCP search/browser remains supported. R3/R6/R10 profile restrictions/guidance refer to the selected native tool set, not a promise to expose excluded donor tools. T45 and T50 share existing owners without a completion cycle; no new framework or status claim.
- Unrelated refactors or audits; scope expands only on owner instruction or a diff-caused regression.

## Change Envelope

- Target: `crates/oc-adapters/src/{defs.rs,config.rs,composition.rs,runtime.rs,application.rs,tools.rs}`, `crates/oc-core/src/*`, `crates/oc-tui/src/*`, `crates/oc/src/*`, tests under `crates/*/tests/`, `docs/*`, `evidence/*`.
- Expected paths, symbols, and direct consumers: `split_frontmatter`, `parse_skill`, `load_kind`, `insert_agent`, `insert_command`, `normalize_permission`, `MAX_*` constants; subagent work touches session/tool/runtime composition and TUI rendering.
- R6 adds agent definition/merge/default selection, native Build/Plan registration and reminder lifecycle, narrowly admitted Plan-directory patch operations, headless CLI selection and existing T44 picker/cycle consumers. Reuse application/session/config owners; no new agent framework or separate persistence engine.
- R7 adds a small native Linux environment collector in oc-adapters and integration into the existing application/runtime prompt assembler, including the base harness fallback and environment/date baseline. Reuse existing workspace and shell-executor metadata. No second registry, persistence engine, background service or new model-facing tool. This is a Linux extension, not a macOS/Windows implementation commitment or closure of all other OC2 instruction-lifecycle gaps.
- R3/R8–R10 extend existing session workers/job ownership, subagent schema, durable task/pack admission, conversation views, DCP block/protection queries and shared prompt/instruction projection. Internal typed snapshots/transactional storage fields may be added only for these outcomes; no donor KV clone, external scheduler, second executable registry, generic agent framework or new archive/history copy. Current GOAL.md amendments admit this scope; original audit leaves remain historical evidence.
- Allowed and forbidden artifacts: source, tests, docs, evidence notes. Forbidden: editing GOAL.md/audit gates to pass, deleting tests, adding JS runtime, committing secrets or `.local/` contents.
- User or harness budget: commits + pushes required; no attempt limit; live calls bounded.

## Current Checkpoint

- Closes: smallest unresolved T45 slice of R3/R6–R10, not already verified R1/R2.
- Smallest next action: when T45 starts, reconcile actual HEAD/foreground prerequisites, choose one scheduler or request-assembly slice from roadmap/M8.md and freeze its direct evidence. T44 remains active during this plan-only amendment.
- Expected evidence: assigned targeted scenario plus relevant crate/workspace gates; plan validators are not implementation PASS.
- Stop or replan if: a slice crosses the stated authority/trust boundary or requires unknown side-effect replay; retain historical tests and record the precise donor/native difference.

## Current State

- Resolved: R1/R2 warning fixtures remain verified by their historical evidence; T43 foreground prerequisites landed, but full R3/R6–R10 qualification is pending.
- Last relevant evidence: pinned OC2/DCP research and current code inspection; existing release/one-compression reports do not qualify the newly approved outcomes. No lifetime compress-call cap was found; nested-depth/archive-loading risks are distinct.
- Blocker: none.
- Next: T45 independently implements the ordered slices; T44 consumes owner-backed child/profile surfaces. Keep task statuses/dependencies and executed evidence unchanged in the plan-only delivery.

## Material Decisions

- 2026-09-27: Owner approved the entire subagent/context/DCP/prompt discussion. R3 concurrency/recovery replaces deferred-after-parent/serial-only proposals; R8 context packs are quoted user data, not system text; R9 child DCP defaults true with policy gates and no lifetime quota; R10 completes shared instruction assembly. Native extensions are distinct from OC2 and pinned DCP. Task-local scenarios SUB01/SUB02/CTX01/CTX02/DCP10/DCP11/PRM01 have one owner T45; existing gates/evidence are not waived or converted to PASS. Supersedes the old S5/S7/non-goals and only the relevant D03/D05/D10 clauses.

- 2026-09-27: Owner approved R7 Linux host/workspace context and the four-file plan diff. T45 owns backend/request qualification; A13 is added to its existing gates without changing task IDs, dependencies or statuses. Extended host fields are native product additions, not claims that OC2 already emits them. Implementation remains pending.

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

## Environment/context references — R7

Pinned to `2670273ff17da96f85c5826ced57aa1b368754fa`:

- [Built-in environment and date](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/instructions/builtins.ts#L22-L55) — donor environment baseline; extended Linux host fields are owner-approved native additions.
- [Agent system/base prompt and instruction baseline composition](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/model-request.ts#L73-L92) — custom system replaces the base prompt, not the remaining instruction context.
- [Base harness prompt](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/prompt/system.txt#L1-L15) — fallback reference; adapt tool guidance to the native contracts and preserve provenance if copied.

## Subagent/context/DCP references — R3/R8–R10

OC2 references are pinned to `2670273ff17da96f85c5826ced57aa1b368754fa`:

- [Foreground/background, fresh context, continuation and launch](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/subagent.ts#L29-L62), [admission/execution](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/subagent.ts#L117-L265), [filtered preview](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/subagent.ts#L271-L298).
- [Concurrent tool fibers and batch join](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/step.ts#L100-L145); [background command routing](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/plugin/command.ts#L98-L122).
- [General/Explore defaults and permissions](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/plugin/agent.ts#L94-L130); [child Location/session policy inheritance](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session.ts#L246-L279). Native parent-child narrowing remains a declared difference.
- [Background observer](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/subagent-job.ts#L20-L57), [terminal delivery](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/subagent-completion.ts#L20-L44), [at-least-once recovery](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/execution/restart.ts#L15-L20), [restart branches](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/execution/restart.ts#L137-L229).
- [Instruction baseline sources](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/context.ts#L127-L155), [AGENTS discovery](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/plugin/instruction.ts#L32-L120), [chronological instruction rendering](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/instruction-discovery.ts#L123-L149), [session instruction reconciliation/nested reads](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/instructions.ts).
- [Skill metadata instructions](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/skill/instructions.ts), [model prompt hooks](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/plugin/optimize.ts#L64-L91) — differences governed by native A13/no hardcoded model IDs, not silent full parity.

DCP reference: [range tool](https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/compress/range.ts), [range utilities](https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/compress/range-utils.ts), [block/run state](https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/compress/state.ts). Revision/version/license are recorded in `planning/baseline.lock.json` and `fixtures/dcp-compress.json` (3.1.15, AGPL-3.0-or-later). No verified lifetime counter limit is claimed. R8 context packs and R9 default-child/no-exhaustion behavior are owner-approved native additions; [docs/DCP.md](../DCP.md) records the resource/protection contract.

## Completion

- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
