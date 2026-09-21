# Subagent system — implementation plan (upstream v2.0.12)

Research date 2026-09-22. Upstream ref: `anomalyco/opencode` tag `v2.0.12`, tree
`2670273ff17da96f85c5826ced57aa1b368754fa`. Local repo: `/home/opencode/ai/oc`, branch
`agent/oc-rust-port`, HEAD `3ec9116`, worktree clean (only untracked `docs/goals/`).
Upstream paths below are relative to `packages/`; citations are `path:line` at that SHA.
Read-only task: no source file was modified.

## Scope note (must be resolved by the owner)

`GOAL.md:53` lists "subagents/task orchestration" as *out of this goal*, while the owner
contract `docs/goals/2026-09-21-config-compat-and-subagents.md` (R3, lines 33–38) requires
full implementation. This plan follows the newer owner contract. Before delivery, GOAL.md
must be amended (or the goal doc marked authoritative) so A12 does not claim an out-of-scope
feature; do not edit GOAL.md silently.

## 1. Upstream semantics (field → behavior)

Tool identity: the tool is named **`subagent`** (`core/src/tool/plugin/subagent.ts:17`), not
`task`. Old name survives only in permission-key migration.

| Field / event | Upstream behavior | Citation (v2.0.12) |
|---|---|---|
| input `agent` | exact agent id; unknown → `ToolFailure "Unknown agent: X"`; `mode:"primary"` → `"Agent X cannot run as a subagent"` | `tool/plugin/subagent.ts:134-137` |
| input `description` | 3–5 word label → child `title` | `subagent.ts:34,190` |
| input `prompt` | child user text, prefixed `"You are a subagent spawned by another session.\n"` for new children | `subagent.ts:208-211` |
| input `model` | `providerID/modelID[#variant]`; validated against available models; absent → `agent.model ?? parent.model` | `subagent.ts:36-39,75-98,184` |
| input `sessionID` | continue a previous child; must satisfy `existing.parentID === context.sessionID` else failure | `subagent.ts:40-43,164-167` |
| input `background` | `true` → return immediately `{status:"running"}`, notify parent later via synthetic message | `subagent.ts:44-47,200,229-232` |
| tool output | completed: text wrapped `<subagent sessionID="…" state="completed">\n…\n</subagent>`; failure/cancel: `ToolFailure` carrying sessionID; running: plain "working in the background" text | `subagent.ts:19-27,246-264` |
| result text | last completed assistant message, all `text` parts joined; empty → `SubagentCompletion.NO_TEXT` | `session/subagent-completion.ts:8-18`; `session/subagent-job.ts:44-52` |
| parent notification (bg) | `sessions.synthetic` into parent: text wrapper above with `description=…`, metadata `{source:"subagent", childID, agent, state}`; dedup key `childID:startedAt` | `subagent-completion.ts:20-44`; `subagent-job.ts:21-34` |
| depth | walks session `parentID` chain; `experimental.subagent_depth` default **1** (root=0 may spawn, child=1 may not); exceed → ToolFailure naming the config key | `subagent.ts:117-133`; `schema/src/config/experimental.ts:11-13` |
| permission | action `"subagent"`, resources/save `[agent.id]`, sessionID=parent, agent=caller; deny → `"Subagent denied: <id>"` | `subagent.ts:138-151` |
| legacy action | v1 `"task"` migrates to `"subagent"` (also `write/patch→edit`, `bash→shell`) | `core/src/v1/config/migrate.ts:117-121` |
| model visibility | tool registered normally (`codemode:false`); a `context`/`compaction`/`generate` session hook appends `Available subagents:` with `- id: description` for agents where `mode !== "primary" && !hidden && permission != deny`, sorted by id | `subagent.ts:271-298` |
| agent catalog | built-ins: `build` primary, `general`/`explore` subagent (`explore` denies `subagent` action), hidden `compaction`/`title` | `core/src/plugin/agent.ts:87-140` |
| child context | fresh history; only prompt+prefix; child gets its own agent `system` prompt and its agent permission ruleset; no parent transcript | `subagent.ts:55-62`; `config/plugin/agent.ts:115,121-123` |
| foreground join | `jobs.block({id: child.id, sessionID: parent})`; `Effect.onInterrupt` → `sessions.interrupt(child)` + `jobs.cancel(child)` | `subagent.ts:234-240` |
| cascade delete | `Session.remove` recursively removes children | `core/src/session.ts:356-359` |
| restart recovery | durable `Job.Background{kind:"subagent"}` with recovery ids; on restart re-verifies child/parent, delivers terminal outcome or resumes child once | `session/execution/restart.ts:137-204` |
| session record | `session.created` event carries `parentID`, `agent`, `model`, `title`; child inherits parent `location`, `metadata`, `permissions` | `core/src/session.ts:246-283`; `schema/src/session-event.ts:50-66` |
| listing/stats | `list({parentID})` filter exists; stats count `parent_id != null` rows as subagents; TUI "Subagents" composer tab navigates the session family | `session/store.ts:110-112`; `session/stats.ts:154-157`; `tui/src/routes/session/composer/subagents-tab.tsx:37-63` |
| command frontmatter | `subagent = command.subagent ?? command.subtask`; child branch when `subagent ?? commandAgent?.mode === "subagent"`; `subagent:false` overrides both mode and alias; child always background, `resume:false` | `config/plugin/command.ts:75,98-122`; `core/test/config/command-subagent.test.ts:109-124` |
| command inline branch | order: switch agent (if different) → switch model → prompt expanded text; model = `command.model ?? commandAgent?.model` | `config/plugin/command.ts:83-92,124-134` |
| command template | `$ARGUMENTS`, `$N`, shell `` !`cmd` `` interpolation | `config/plugin/command.ts:189-241` |
| command test proof | child `{agent:"reviewer", model:{id:"child"}, title:"Review code"}`; parent agent/model untouched and parent history empty until synthetic notice | `core/test/config/command-subagent.test.ts:88-104` |
| concurrency cap | none in core; one job per child session id, nesting bounded by `subagent_depth` | `job.ts:184-232` (registry), `subagent.ts:117-133` |

`experimental.allowSubAgents` is **not** a core v2.0.12 key (only `subagent_depth` exists).
It is our DCP-plugin config option; in v2.0.12 no `allowSubAgents` symbol exists anywhere
under `packages/` (verified by rg over the tag tree). Making it meaningful is therefore our
choice: gate DCP summarisation of child sessions on it (see S7).

## 2. Local code map

| Upstream behavior | Local landing point |
|---|---|
| agent mode parsing | `crates/oc-adapters/src/defs.rs:793-800` `primary_mode` rejects `subagent/all`; Markdown allowlist `:866-869`; JSON allowlist `:463-478`; `AgentDef.mode: Option<String>` `:90` |
| command fields | `CommandDef` `defs.rs:97-106` has body only; Markdown allowlist `:914-924`; JSON allowlist `:544-556` rejects `agent/subtask` explicitly |
| agent/command catalog | `Composition.agents` filters to primary `composition.rs:544-548`; `Composition.commands: BTreeMap<String,String>` `:489-493`; workspace agents `application.rs:438-449` |
| command execution | `resolve_submission` expands template only `application.rs:945-964`; executed in worker `:794-806` |
| tool schema/dispatch | `builtin_tool_defs()` `runtime.rs:183-287`; `validate_call` `tools.rs:426-457`; `execute_call` `tools.rs:408-422`; `ToolContext` `tools.rs:327-347` |
| durable tool op + turn loop | `Runtime::execute_units` `runtime.rs:1584-1644`; `run_turn_inner` `runtime.rs:1047-1168`; workspace/prompt lane `runtime.rs:777-816` |
| single-flight + cancel | `Runtime::begin_active` `runtime.rs:730-735`; worker select/`Cancel` `application.rs:807-877`; `TurnParams.cancel: &AtomicBool` `runtime.rs:599-600` |
| sessions store | `sessions(id, created_at)` `storage.rs:1794`; `create_session` `:288-308`; `list_sessions` `:384-393`; history is append-only `:314-352`; schema version `:26`, migration pattern `INSERT OR IGNORE schema_migrations` (`dcp.rs:1082`) |
| Location binding | `Runtime::create_session/open_session` `runtime.rs:840-866`; pref key from `session_location_key` |
| parent/session UI | `CoreApp::list_sessions` `oc-core/src/core_app.rs:381-386`; TUI `TuiState::apply_sessions` `oc-tui/src/app.rs:363-366`; rendering `views.rs:117-119`; wiring `oc/src/tui_cmd.rs:234-235`; headless list `oc/src/headless.rs:135-141` |
| DCP warning | `dcp_auto.rs:220-241`, test `:1035-1042`; `config.rs:655-666`; `Composition.dcp_config` `composition.rs:52` |
| permission policy | `RuntimePolicy` map-based, unlisted → deny `runtime.rs:148-173`; agent permission merge for the selected primary only `composition.rs:394-410` |
| test norm | fake SSE server + `make_harness/runtime_of/params/provider_of` `crates/oc-adapters/tests/runtime.rs:200-320` |

Existing contract to keep: `crates/oc/tests/configured_workspace.rs:506-540` (`aud17`) requires a
hard error naming `subagent` when `default_agent` points at a subagent-mode agent. Supporting
`mode: subagent` must change the diagnostic text, not the fail-closed behavior: use
`selected agent broken is subagent-only and cannot be a primary agent` (still contains both
`broken` and `subagent`).

## 3. New/changed types and functions

- `defs.rs`: `enum AgentMode { Primary, Subagent, All }` (replace `Option<String>`);
  `CommandDef { agent: Option<String>, model: Option<ModelRef>, subagent: Option<bool>, subtask: Option<bool> }`
  and `CommandDef::is_subagent(agent_catalog) -> bool` implementing `subagent ?? subtask` +
  agent-mode fallback; `ModelRef` parse `provider/model[#variant]`.
- `Composition`: keep all agents (`agents` unfiltered); add `commands: BTreeMap<String, CommandDef>`;
  add `subagent_depth: u32` read from `experimental.subagent_depth` (default 1) in `config::assemble`
  or a sibling native key reader.
- `storage.rs`: migration v2 `ALTER TABLE sessions ADD COLUMN parent_id/agent/model/title TEXT`;
  `SessionMeta`, `create_child_session`, `session_meta`, `children_of`, `list_session_meta`
  (parent-first, id-sorted); never touch `messages`.
- `tools.rs`: `SubagentRequest`, `SubagentOutcome`, `trait SubagentRunner { fn spawn(...) -> BoxFuture<'_, Result<SubagentOutcome, ToolError>>; }`,
  `ToolContext.subagent: Option<&dyn SubagentRunner>`; `validate_call` arm; `execute_call` arm
  (permission check already happens in `execute_units`, `runtime.rs:1629-1636`).
- `runtime.rs`: `TurnParams { system_prompt: Option<String>, agent_id/digest: Option<String>,
  child: bool, subagent: Option<&dyn SubagentRunner>, depth: u32 }`;
  `Runtime::spawn_subagent(...)` → nested `run_turn_inner` (no `begin_active`, child cancel flag OR-ed
  with parent flag), child policy = parent generation permissions refined by child agent rules;
  `builtin_tool_defs(subagents_enabled)` + turn-time description augmentation with `Available subagents:`.
- `application.rs`: `CommandRoute::{Inline{agent,model}, Child{agent,model}}`;
  `resolve_submission -> (CommandRoute, String, Option<String>)`;
  bounded `PendingChild` queue drained after the parent turn; synthetic admission helper.
- `oc-core/queries.rs`: `SessionListEntry { id, parent_id, agent, title }`; `TuiState::apply_sessions`
  accepts it; `oc/src/tui_cmd.rs` marks children (e.g. `└─ <agent> <title>`).
- `dcp_auto.rs`: `DcpConfig.allow_subagents: bool` (default false), warning removed;
  `dcp_available(session, meta)` gate.

## 4. Ordered slices (each keeps workspace gates green and is committed)

**S1 Config admission (compat slice dependency).** Accept `mode: subagent|all` in both Markdown and
JSON agent paths; parse command `agent/model/subagent/subtask` into `CommandDef` (Markdown frontmatter
+ JSON); keep `subtask` as deprecated alias; keep AUD17 fail-closed with the reworded diagnostic.
Tests: `defs.rs` units (mode round-trip, command fields, unknown mode still error) + existing
`configured_workspace.rs` suite. Assumption: a concurrent compat slice may already accept some of this;
re-check `git log -1 -- crates/oc-adapters/src/defs.rs` before editing to avoid a conflicting revert.
Note: `docs/DECISIONS.md:25` and `docs/CONFIG.md:31-33` currently document subagents as out of scope;
update them in this slice.

**S2 Child session persistence.** Storage migration v2 (idempotent, versioned), `SessionMeta`,
Location inheritance (child uses parent's Location pref), title/agent/model columns. Tests: storage
units — reopen applies v2 once; child row visible via `children_of`; parent messages byte-identical
(immutability); `list_sessions` order unchanged for legacy callers.

**S3 Catalog/command route plumbing.** `Composition` carries all agents and full `CommandDef`;
`subagent_depth`; `Effective::apply_route(Inline)` performs upstream order (agent switch, then model
switch); `CatalogSnapshot` exposes subagent-capable ids. Tests: composition unit + application
unit (`resolve_submission` inline/child classification table, `subagent:false` override).

**S4 Foreground `subagent` tool (model-facing).** Tool def, validation, permission action
`subagent` (deny path already generic), nested turn with depth limit (default 1), unknown/primary
agent failures, `sessionID` continuation guarded by `parent_id`, completed output wrapped
`<subagent sessionID="…" state="completed">…</subagent>`, error/cancel strings carrying sessionID,
result = last completed assistant text with `NO_TEXT` fallback. Tests: new
`crates/oc-adapters/tests/subagent.rs` on the existing fake-SSE harness: spawn returns child text and
persists child rows; nested spawn at depth 1 fails with the depth message; `deny subagent` yields
`error: denied subagent` and **no** child row; cancel mid-child-stream marks the child turn
`cancelled`/`interrupted` and the parent tool output `error: cancelled`; completion detail absent
from parent context (fresh history).

**S5 Background + completion notices + reap.** `background:true` admits the child to a bounded
worker queue, returns the running text; after the parent turn reaches terminal state the worker runs
the child (nested, no lease), then admits exactly one synthetic
`<subagent sessionID state description>` message to the parent and wakes a parent turn. On
shutdown/cancel the queue is drained: no pending child turns after `WorkerGuard::join`
(A02). Tests: queued child runs after parent; synthetic appears once with metadata
`{source:"subagent", childID, agent, state}`; shutdown with a queued/running child leaves no
orphan turn and child status is terminal; restart path marks in-flight children `interrupted`
without duplicate notice.

**S6 Command subagent routing.** Child branch: `sessions.create(parent_id, title=description??id,
agent=route.agent??parent.agent, model=route.model??child-agent.model??parent.model)`, prompt with
prefix, run to terminal, synthetic notice + parent resume; always `resume:false` for the child's
own wake. Inline branch per S3 ordering. Tests mirror upstream
`command-subagent.test.ts`: subagent-true fixture (JSON + legacy Markdown) leaves parent
agent/model/history untouched and creates one child with the right title/agent/model; the
`subagent:false` fixture produces zero children, parent agent+model switched, expanded text
with a shell interpolation.

**S7 DCP `allowSubAgents`.** Remove the warning; `DcpConfig.allow_subagents` (default false).
When false, child-session turns run with DCP compression disabled (no `compress` tool, no nudges);
when true, children behave like root sessions. Tests: `dcp_auto` config unit (no warning, default
false, parse true); runtime test that a parent turn still advertises `compress` while a child turn
does not with false, and does with true. Update `examples/dcp.jsonc` comment.

**S8 TUI/history surfaces.** `list_session_meta` query, indented child rows with agent/title in the
sessions panel, selection opens the child session, and the sessions panel refreshes after a child
finishes. Tests: `views.rs` render unit with a parent/child pair; `oc sessions` stdout stays ids-only
(headless test `sessions_list_stdout_ids_only`); optional PTY smoke for navigation.

## 5. Top risks and mitigations

1. **Single-flight lease deadlock.** `Runtime` holds one `AtomicBool` lease per Location
   (`runtime.rs:626-654`); a child turn calling the public `run_turn*` would return `TurnActive`.
   Mitigation: `spawn_subagent` must call `run_turn_inner` directly, never `begin_active`; it must
   OR the child cancel flag with the parent's `params.cancel` so one Cancel stops both; a unit test
   asserts `runtime.turn_active()` is true throughout a nested child and that cancel terminates the
   child within the same turn.
2. **Cancellation/reaping orphans (A02).** Child turns share the worker; background children are the
   only detached work. Mitigation: bounded queue owned by the worker loop; terminal statuses only
   (`cancelled`/`interrupted`/`failed`); worker shutdown drains the queue before `WorkerGuard::join`;
   restart writes `interrupted` for non-terminal children; tests assert zero pending children and
   child turn status after shutdown/crash simulation.
3. **Permission/authority widening.** Today agent rules are merged into the *generation* for the
   selected primary only (`composition.rs:394-410`) and `RuntimePolicy` denies unlisted tools.
   A child must not inherit the caller's broad permissions beyond its own agent ruleset. Mitigation:
   build a per-child `RuntimePolicy` from parent generation ∩ child agent rules (explore's deny-all
   profile must actually block bash/patch), and a test where the child's `subagent`/`bash` rule is
   denied while the parent is allowed. Secondary risk in the same area: config compat slice landing
   concurrently may already parse `mode`/`subagent` fields — re-check before S1.

## 6. Acceptance mapping

- R3(a) S1/S3, (b) S4/S5, (c) S2/S8, (d) S6, (e) S4, (f) S7, (g) S5; A02 reaping in S5.
- Primary evidence: fake-server integration tests + one bounded live run spawning one subagent on the
  configured OpenProxy provider; commands and outputs recorded under `evidence/subagents/`.

## 7. Explicit non-goals / declared differences

No concurrent foreground subagents (worker is single-flight); background children start only after the
current parent turn reaches terminal state rather than truly concurrently; no upstream `Job` KV replay
of notifications across restarts (terminal-only replay); no `question` tool/UI for subagent approvals;
no Code Mode exposure (`codemode:false` upstream is moot here — direct exposure only).
