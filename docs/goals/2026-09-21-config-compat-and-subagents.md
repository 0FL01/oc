# Goal: Config compatibility parity + full subagent system

Status: active
Source: user instruction 2026-09-21 ("исправить баги и убрать лимиты; далее реализовать систему саб агентов полностью"), plus upstream reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (tree SHA `2670273ff17da96f85c5826ced57aa1b368754fa`).
Last updated: 2026-10-02

## Objective

Bare `oc` loads the owner's real `~/.config/opencode` config without spurious warnings, and the product implements the pinned OC2 subagent/profile/context system end to end, plus owner-approved Linux host context, explicit parent-message context packs and long-horizon DCP with percentage/total-active defaults. The 2026-09-30 hot/cold clarification requires indefinitely repeatable replacement/forgetting of hot context, optional user `.md`/Git memory and compatible manual `/compact`, not lossless accumulation of past summaries. Donor parity and native extensions/differences are qualified separately by actual requests, effects, workspace gates and the existing bounded live run.

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

- R3: Subagent system fully implemented per upstream v2.0.12 with the approved native MCP defaults.
  - Source: owner instruction "реализовать систему саб агентов полностью", 2026-09-27 concurrency RECON and approval; pinned OC2 `subagent` tool (legacy `task` is a permission migration alias), session parentID, agent modes, command routing and result delivery.
  - Owner: T45 remaining backend/CLI/history slices and typed child lifecycle/projections; preserve T43's landed foreground prerequisites. T44 owns VIS39 transcript/composer/indicator/colors/animations qualification, after the minimal R3 slice, without all-T45 completion or a reverse dependency.
  - Acceptance: (a) subagent/all profiles load and run; General and Explore are registered built-in subagents without custom definitions, with real descriptions/system/native-tool restrictions, default depth 1 and caller-gated spawning. The approved 2026-10-01 MCP Allow default below replaces only donor Explore's blanket MCP denial, not native file/shell/question/delegation restrictions or explicit policy. Model selection is explicit override → child profile → parent, while continuation retains its own selection unless explicitly switched. Primary-only/unknown profiles and foreign sessionID fail before child creation. (b) Foreground is the default; multiple independent subagent calls in one response can run concurrently, and the next parent model step waits for the batch's terminal results. (c) background:true starts independently before returning running/sessionID; it can make progress while the parent turn is still active, not only after parent completion. Parent receives an owner-generated terminal notice without polling or duplicate work.
  - Acceptance: (d) children have fresh history plus the delegated prompt, their own profile and applicable workspace context, not the parent transcript or parent agent.system; sessionID continues the child's own history. Child rows retain parent/Location linkage and are visible in history/TUI. (e) command.subagent ?? command.subtask wins over mode inference; explicit false routes inline with agent/model switch ordering, child commands route background without changing parent selection. (f) cancellation/shutdown reaps owned work; per-session scheduling, bounded in-flight/queued work and mutation safety replace a global parent-held single-flight deadlock, without serializing all children behind the parent.
  - Acceptance: (g) durable background recovery verifies parent/child identity, delivers an already committed terminal result without re-execution and uses pinned donor recovery for safely resumable work. Do not claim exactly-once execution: donor recovery is at-least-once. Native started/unknown mutation, shell or MCP operations are never auto-replayed; unresolved effects require explicit recovery. Notifications are deduplicated by durable job generation/delivery identity, including crash at delivery. (h) permissions/parent-child narrowing and R9 child DCP remain authoritative; neither background nor context transfer widens authority.
  - Acceptance: (i) project call/operation, parent/child/session/generation, complete validated input/model/continuation, immutable background launch metadata, current child state, typed permission/outcome and bounded family status/navigation facts from existing owners. Durable synthetic completion retains source=subagent/childID/agent/state/description/result and delivery identity; one child then a batch can reach parent continuation without UI polling or duplicate work. Ctrl+B actually backgrounds owned foreground jobs and admits genuine control context, not a TUI flag. Child prose is not runtime failure. View restoration reconciles current status/notices without reexecuting committed work or unknown effects; safe unfinished-job recovery remains (g). Keep native closed-response validation before admission, queue/query bounds and existing store; T44 VIS39 consumes these facts and independently qualifies presentation.
  - Acceptance supplement (2026-10-01): supply real child-scoped accepted/text/reasoning/tool/terminal events plus bounded authoritative family/current-state reads, not only committed child history or no-op callbacks. TUI can open the actual child while the parent awaits it, reconcile history/live deltas once and independently target interrupt to a selected running child. Current idle/completed/cancelled/error/unknown differs from immutable launch/tool status. Controls validate family/source-session/job generation; stale/foreign requests cannot cancel a parent/sibling or a different operation. Repeated/racing background/interrupt/terminal transitions settle the same admitted work once, with no kill/restart/reexecution disguised as Ctrl+B.
  - Acceptance supplement: narrowly replace existing blanket child SwitchSession/navigation refusal with owner-backed linked-child/parent/family navigation and control. Preserve the child guard against arbitrary new root-like turns, profile/model mutation and conversation Undo; opening a child does not grant root-tab authority or silently select a new Location. Root deck/draft/focus and pinned execution context survive round trips. Pending descendant permission/question routing keeps the existing real binding and parent attention owner; navigation/close/hide/filter alone never approves, answers or cancels work.
  - Primary evidence: SUB01/SUB02 scripted provider barriers prove overlapping child execution and background progress before parent completion; fake-server spawn/continuation/model/depth/deny/cancel/command/restart tests prove effects and durable outcomes, plus the existing bounded live run spawning one subagent. Include actual Ctrl+B conversion, launch/current-state distinction, structured completion metadata and parent continuation/delivery dedup; reuse this actual-binary/protocol/SQLite evidence for T44 VIS39, without claiming visual PASS. Time-independent ordering barriers, not fragile elapsed-time claims; rebuild/exercise the actual binary.
  - Primary evidence supplement: rebuilt actual-binary PTY opens a linked live child before parent completion, observes its real shell/text/reasoning events, returns to the parent and interrupts precisely one selected child with another still running. Assert no duplicate history/deltas/notice/parent continuation, correct source Location after parent move and stale/late/restart safety. T50 supplies its own TOOL13 shell conversion/list/live-output seam; T56 supplies TERM01 PTYs. T45 does not absorb those owners or wait for complete T44 visual qualification.
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
  - Acceptance supplement (owner-approved 2026-10-01 built-ins): register Build/Plan as primary and General/Explore as subagent before configured transforms; no custom-file prerequisite or primary-only exposure of subagents. Qualify the actual Plan policy/reminder lifecycle and common default MCP authority described below, not merely a custom profile named plan or a picker label. Explicit central/parent/profile restrictions remain authoritative; the requested MCP default also applies to Plan, without redefining it as a shell/MCP sandbox.
  - Acceptance supplement (owner-approved 2026-10-01): consume a real listed reference from [T50/R7 `oc models`](2026-09-27-native-tool-parity.md#cli-models--r7tool18-approved-2026-10-01-pending) in canonical `agents.<id>.model`, legacy `agent` normalization, and Markdown global/project agent/agents roots. Pinned grammar uses the first `/` as provider separator, preserves further slashes in model ID, and admits `provider/model#variant` or structured selection. Separate legacy variant joins a string model only if it has no embedded `#`; embedded/structured native choice wins, per donor importer/migration, not current parser convenience. Listing does not write/bind profiles, choose defaults or widen permissions. Missing/retired model or disabled variant remains explicit, with generation/tool refusal before effects and no provider/model fallback.
  - Agent-cycle clarification (owner-approved 2026-09-30): supply the smallest real ordered primary catalog/selection slice for [T44/VIS06/VIS10/VIS17](../../tui-recovery/T44_CONTRACT_AMENDMENT.md#agent-cycle-keybindings--уточнение-2026-09-30). Current primary_capable() alone does not exclude hidden profiles; automatic cycle/picker eligibility must omit hidden, disabled and subagent-only definitions while explicit addressing retains its separate rules. Reuse session/Home selection validation and model/variant persistence; never change an in-flight profile or silently repair a saved unavailable choice. Existing config composition admits canonical/legacy agent list/forward/reverse bindings for the same immutable Location generation, not a new keymap/config/profile store. T44 supplies direct-cycle input and paired visual evidence after explicit resume; no all-T45 completion dependency or new acceptance ID.
  - Primary evidence: pinned TS fixtures for normalized definitions/merge/default selection plus captured fake-provider requests, independently verified tool effects and session/restart assertions; T44 paired picker/prompt captures prove UI parity separately. Rebuild and exercise target/release/oc. Existing workspace/live gates remain mandatory.
  - Primary evidence supplement: reuse TOOL18's actual CLI stdout receipt, bind its synthetic exact ID into JSON canonical/legacy and Markdown fixtures through the real config loader, select the profile and capture actual provider/model/variant plus profile body in a fake-provider request. Verify reopen/restart and unavailable pre-effect refusal, including a model ID with additional slashes and colliding IDs in admitted providers. Profile semantics remain T45/R6/PRM01/A03/A13-owned; T50 only owns listing, not a duplicate binding test owner or all-T45 completion dependency. New scenario pending/NOT_RUN.
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

- R9: DCP has owner-approved defaults, is default-enabled for children and supports infinite hot-context renewal through intentional forgetting, with optional cold memory and compatible manual `/compact`.
  - Source: owner-approved 2026-09-27 child compress default and removal of lifetime-exhaustion behavior. No total compress-call quota was found in audited native code or pinned DCP 3.1.15; the owner's historical OC1 failure is not attributed to a specific cause without its version/error. This outcome prevents exhaustion rather than inventing a counter to remove.
  - Owner: T45 shared DCP config/runtime/model-budget facts and child DCP/storage, using A07/A10 and existing resource gates; OC2 native compaction remains a separate mechanism.
  - Source supplement: owner-approved 2026-09-27 minContextLimit="40%", maxContextLimit="55%", summaryBuffer=false and long-horizon/config discussion. Pinned DCP 3.1.15 retains 50000/100000/true; new defaults are native policy, not donor parity or a baseline revision change.
  - Source supplement: owner instruction 2026-09-30: hot path is the model context window and must allow endlessly repeated compression at the cost of throwing away useless context; cold path in `.md` files/Git history is fully optional and chosen by the user. Owner approved recording the revised plan, commit and push; implementation/qualification remain pending. Canonical detailed contract: [infinite hot / optional cold](../DCP.md#infinite-hot-context--optional-cold-path--t45r9dcp11-pending).
  - Acceptance: omitted fields use native 40%/55%/false in root and eligible child sessions, not merely a supplied local/example override; preserve other defaults, integer/percent/partial/exact per-model overrides and read-only admitted dcp.json/jsonc/inline config precedence. Runtime and existing panel share canonical provider/model identity, positive model-budget context/fallback and effective min≤max validation, keeping discovery metadata/admission unchanged. Default upper nudges count total active context including summaries; explicit true subtracts active summaries only for upper-reminder accounting. Preserve strict-above-max escalation, cadence/iteration/reset, disabled/manual/Deny and protections; 55% is not an enforced input ceiling or hidden summarizer. Full semantics, config surface and minimal typed facts/evidence are in [DCP12 contract](../DCP.md#approved-percentage-defaults--t45r9dcp12-pending).
  - Acceptance: dcp.experimental.allowSubAgents defaults to true; explicit false disables model-facing compress/anchors/nudges/automatic DCP strategies for children only. Global enabled:false and manualMode retain their meaning; effective compress Deny and central/parent-child ceilings win. Parsing the option has actual behavior and no obsolete unsupported warning. Built-in Explore gets only a narrow own-session compress grant alongside its read-only policy; custom wildcard/explicit deny is not overridden. Preview follows the effective result.
  - Acceptance: blocks/anchors/nudges/cadence/tool projections/protections are session/generation scoped in foreground/background/root/parallel children; child compression never mutates a parent's or sibling's projection. Compress replaces only closed completed ranges; retained tool-call/result/reasoning groups stay intact and forgotten closed groups leave hot context as whole groups. In-flight batches are not split. Raw history is immutable and durable counters are not an admission quota.
  - Acceptance: protect the current delegation task and its attached context pack from child model-driven compress while that admitted task is pending/running. On terminal completion the task is no longer in-flight; on safe recovery reinstate its protection. Old completed tasks/packs become eligible for normal configured DCP protections and compression, so continuation does not accumulate permanent new fixed lanes. Never auto-execute a queued next task to evade protection. Current requirements remain preserved/reconstructed through native compaction as well, with irreducible context-budget failures reported explicitly.
  - Acceptance: every admitted compression replaces the selected hot range with a standalone working summary, rather than adding a summary plus an obligation to retain the old range. Fully covered old blocks are consumed even when the replacement omits their content/placeholders; omission intentionally forgets their unprotected content. Explicit authored references resolve faithfully during preparation, without persistent live dependency on an aging chain. Recompression can rewrite or forget prior summaries; flatten alone is insufficient. Stable IDs/provenance belong to durable history, not mandatory provider input. Retain raw history and existing conversation-only Undo/Redo/fork semantics; no raw-record purge or second history store.
  - Acceptance: obsolete closed tool outputs, reasoning/media/opaque groups and runtime-added task/pack protection do not survive solely through compressed ancestry. Preserve selected working facts, current requirements and currently effective explicit user protections, but do not blindly append historical protected suffixes. Terminal completion releases extra runtime task/pack protection; safe recovery reinstates protection for the actual unfinished task. A user policy override is not silently bypassed by forgetting.
  - Acceptance: cold path in user `.md`/Git is entirely optional. Compression, restart and continuation work with no journal/checkpoint files or Git history; no automatic writes/commits, dependency on cold references, hidden archival recall or RAG service. Existing native persistence/UI history is separate from model memory. Only explicit admitted read/context selection or conversation Undo can deliberately reintroduce historical data; ordinary restart, DCP and `/compact` restore the last committed hot state without resurrecting forgotten facts.
  - Acceptance: manual `/compact` consolidates the current checkpoint, selected working facts and eligible history into one replacement checkpoint plus fresh tail, with no chain of mandatory old checkpoints or resurrection from raw records. Subsequent DCP advertises resolvable anchors, uses fresh reminders and measures before/after with identical unchanged checkpoint/fixed/current lanes. Reused tool IDs retain correct occurrence identity through compaction/restart/Undo/Redo; no tool replay. Straddling-block progress is qualified before changing boundary semantics.
  - Acceptance: no default session/process-lifetime successful-compression-call or block-count quota; number/age/depth of previous compressions, historical covered member count and unrelated tool marks never disable an otherwise valid shrinking hot replacement. Planning/projection/measurement/commit operate on bounded active/addressed descriptors and selected payload, not all covered IDs or the full archive. Release inactive live objects/marks; keep durable provenance outside the hot dependency graph. Increasing caps, eager whole-archive normalization and moving unlimited copying into a new store are not solutions.
  - Acceptance: recovery remains reachable when ordinary model-turn admission overflows. Select floor/revision/eligible closed ranges by metadata before content reads; forgetting a whole eligible closed group does not require materializing its huge historical payload. Count actual wire cost without constructing an overflowing full-before copy. `/dcp-compress` is a model turn, so API-only `run_compress` evidence cannot prove the user's escape path; qualify actual `/compact` and same-session continuation. Distinguish measured recovery/partial progress/no-gain/irreducible current-state outcomes; no circular unavailable-compress instruction or hidden multi-request summarizer loop. Invalid/no-gain attempts do not consume lifetime capacity; a wider range or smaller replacement remains selectable. Keep per-call/cycle/stale/cross-session/permissions/current-model/turn guards as operation safety, not retention obligations for forgotten history.
  - Acceptance: delegation/DCP guidance preserves only selected useful facts/paths/current decisions/open questions/next step, and optional cold references when the user chose them. Obsolete requirements and pointless investigation can be forgotten entirely; retain a concise disproof reason only when it remains useful. Do not infer this choice by regex/keywords over free LLM text. Permission to compress is not an instruction to compress after every read, and unsupported forced truncation is not an admitted forgetting operation.
  - Primary evidence: DCP10/DCP11 captured root/parallel child requests and SQLite assertions cover default/false/off/manual/deny/Explore, isolation and protection lifecycle. Freeze one mixed compress → continue → recompress → compact → continue → compress → restart → compact → continue workload with kept control facts and deliberately forgotten sentinels. Forgotten summary/tool/media/protection content is absent from actual subsequent wire and resident hot state, including after compaction/restart; explicit Undo/read is tested separately. With no cold files/Git requirement, the actual rebuilt binary continues the task, crosses old depth/4096-covered-row patterns and handles recoverable host/model overflow. Compare equal hot states over small/large inactive block/member/mark archives; measure loaded rows/bytes/depth/peak/retained RAM/tasks/queues and DB/WAL/I/O amplification. Keep A10 thresholds and cleanup; a finite cycle count is evidence, never a product lifespan or proof from one-compression E2E.
  - Primary evidence supplement: DCP12 no-file/default/override/buffer/boundary/cadence fixtures, then rebuilt actual-binary fake-provider reminders and owner snapshots for known/missing/zero/partial model metadata, source precedence, restart and safe-boundary Location generations. Reuse DCP05/DCP07/DCP10/DCP11/AUD41 and existing gates; T44 presentation is separate, no all-T45 dependency or new paid campaign. New qualification remains pending; earlier T19/T36/T39 evidence is unchanged.
  - Acceptance supplement (2026-10-02 host-config RECON): exact DCP3.2.0 plugin spelling has a separate T51/CFG09/CFG10 source-delta/provenance/admission follow-up. R9/DCP10–DCP12 still qualify actual configured allowSubAgents:true/false, child isolation/protections/strategies and thresholds; current parser warning/ignore is not support. Keep compiled3.1.15 baseline/AGPL truthful until qualified derivative changes, no claim of full3.2.0 parity from accepting the name.
  - Status: pending
  - Evidence:

- R10: The shared prompt/instruction lifecycle follows OC2 within the native tool/trust contracts.
  - Source: owner request for the complete system/tools/AGENTS/profiles/skills/subagents assembly scheme and approval to update all discussed work, 2026-09-27; supplements R6/R7 rather than declaring their existing implementation complete.
  - Owner: T45 runtime/config/instructions; existing T44 consumers qualify UI separately.
  - Acceptance: selected nonempty agent.system replaces the base OpenCode harness prompt; empty/default profiles get the base fallback. Retain environment/date, applicable AGENTS, bounded skill metadata, permitted MCP and native DCP guidance independently. Advertise real tools through provider descriptions/JSON schemas with truthful selected-family apply_patch OR edit/write and shell/legacy-bash guidance. Owner-approved 2026-10-01 T50/R1/R9 supplies the exact OC2 file-tool predicate; own-model child/profile/Plan previews and managed base/custom guidance consume its effective view, obligatorily dropping incompatible definitions/guidance on the next request after user model switch and after restart/DCP/compact reconstruction. Raw user/profile/history text is not scrubbed or translated. Skill preview has pinned deterministic ordering/permission filtering; missing description or autoinvoke:false excludes automatic preview, not legitimate explicit skill use. Bodies remain absent until bounded native skill result; subagent bodies are not broadcast to parent. Other model-specific prompt hooks stay separately documented/checked; no production model allowlist, reasoning-name inference or vendor routes to claim parity. TOOL12/TOOL20 remain T50-owned; PRM01 tests this assembler consumer, not a second selector.
  - Acceptance: initial global/project AGENTS discovery has pinned order/canonical dedup/provenance. Successful read admits applicable nested AGENTS as chronological instruction messages once; unchanged repeats do not duplicate content. Initial instruction baseline is durable; admitted changes/removals append chronological updates without retroactively rewriting old history. Reconcile applicable instructions/reminders after compaction/Revert/reopen without stale text, lost rules or duplicate fragments. Discovery respects admitted canonical Location/source boundaries and existing path policy; outside-boundary donor discovery differences are explicit, not a trust bypass.
  - Acceptance: each root/child turn pins one immutable config/Location generation and each request one context revision. Watcher changes are admitted at safe boundaries for subsequent turns, never mixed into an in-flight generation; read-derived fragments carry pinned origin/revision. No automatic skill-body attachment or CodeMode host is introduced to mimic donor features excluded by A13.
  - Acceptance (live-selection clarification 2026-10-01): consume T50/R1/TOOL12's busy-allowed committed model/variant at the next request of the same task, not only a new user turn. Composer picker remains a session/agent (pre-session Location/agent) local draft; captured message/command preparation commits in order, blank Enter in an existing ordinary composer commits without new text, and matching owner ack/events reconcile it without erasing a newer draft on stale/failure paths. Preserve child/read-only/profile authority and config/Location/agent generation; no blanket busy refusal of an otherwise authorized model/variant action. Current prepared request/stream/tools/approval retain actual model/view; capability/managed prompt revisions change only for the next request. Retained ordinary outcomes/call-result groups survive compatible model projection, alien opaque state does not. Selection label versus actual assistant/request attribution is distinct. No new request from draft/commit alone after final completion and no tool replay. T47/VAR01 still owns ordering; PRM01 consumes this behavior rather than owning the switch/executor matrix.
  - Primary evidence: PRM01 pinned source-derived fixtures and captured requests for Build/custom/General/Explore, initial/nested AGENTS, changed/removed files, repeated read, compaction/Revert/restart and Location A→B. Assert actual content/order/roles/provenance, policy-filtered previews, no stale instructions and truthful tools; declared native differences remain separate from donor parity evidence.
  - Primary evidence supplement: reuse TOOL12's provider/tool-barrier same-task A→B→A and real draft/blank-Enter/captured-commit receipts. Assert the next root/own-model-child base/custom prompt, schemas/guidance, model-relative DCP budget and capability view agree, without duplicate fixed lanes, lost retained outcomes, foreign child changes or old-request reattribution. Model choice does not reload config or widen Plan/grants. T44 VIS09/VIS29/VIS17/VIS35/VIS36 paired presentation remains separate and PAUSED.
  - Acceptance/evidence supplement (2026-10-02): consume T53/GO03's ordered chronological system/effort seam, distinct from initial system and top-level variant selection. PRM01 captures actual root/child instruction order with Responses developer updates, Chat escaped in-place user-text fallback and Messages native updates only when explicitly supported, otherwise the same lower-authority fallback. Unsupported or final-marker-mismatched effort history strips markers and uses captured current effort; supported matching history retains first marker.previous as top-level baseline and lowers updates, including reset/default, in position. Reuse GO03/GO04 wire/reopen receipts rather than own a second protocol/history parser; no guessed model allowlist or lost completed tool outcomes.
  - Status: pending
  - Evidence:

### Built-in profiles / default MCP access — approved 2026-10-01, pending

Source: owner asks for built-in explore/general and default access to all MCP for web
search/etc, asks to include built-in Plan, then approves detailed plan commit/push.
This supplements R3/R6/R8/R10; it is not a new R-ID/task or proof of implementation.

**Source-derived RECON snapshot (HEAD `d6e4d549c`, dirty T50 worktree at inspection):**
composition registers only `builtin_build`; defs has special constraints for loaded
general/explore but does not create them. Custom `plan` is an ordinary profile, with
no Plan lifecycle, and CLI has no `run --agent`. Unmatched central action authority
denies; attached MCP schemas are appended without the built-in catalog policy filter
at both initial and follow-up preparation, while guidance/execution do check policy.
These are read-only findings, not executed RED/PASS. T46 is done for its own MCP
transport/lifecycle scope; it does not qualify these profile/default-policy outcomes.

#### Profile contract

| ID / default mode | Required behavior without custom definitions |
| --- | --- |
| `build` / primary | Preserve existing native registration and default selection; general execution follows effective configured policy, not a new blanket tool Allow. |
| `plan` / primary | Real Plan edit policy, enter/leave reminders and context reconciliation; question remains permitted only within effective authority. |
| `general` / subagent | General-purpose description and base harness fallback when no custom system; no question or nested subagent, native session-control ceilings retained. |
| `explore` / subagent | Adapted donor search-specialist system/quick–medium–very-thorough guidance; native read/glob/grep/webfetch, no shell/file mutation/question/nested delegation, separately admitted own-history compress. |

Seed these profiles once in the existing definition owner before global/Location
JSON/Markdown merges. Preserve field-wise supplied overrides, model/variant/digest,
hidden/disabled and explicit mode semantics under R6; a configured omitted field must
not accidentally erase the built-in mode/system/policy. Automatic primary picker/cycle
excludes subagent/hidden/disabled; subagent catalog excludes primary/hidden/disabled
and caller-denied profiles. Explicit hidden addressing follows existing separate rules;
disabled/unknown/primary-only child targets fail before child/input effects.
Child context still uses its own profile, not the parent's system/transcript.

#### MCP authority and effective views

- Default product authority is **Allow for every registered tool** in the current
  configured/enabled/admitted/connected MCP catalog, including root Build/Plan and
  children General/Explore. No per-tool user grant is needed in an otherwise valid
  admitted default configuration. This changes only the old MCP Ask/missing-authority
  default; unknown/unregistered tools and unrelated missing actions still fail closed.
- Add that explicit native baseline in the shared permission owner before root/child
  effective intersection. A child-only Allow cannot fix missing central authority.
  Existing explicit central/parent/profile wildcard or per-tool Deny/Ask/resource
  rules and legacy tools:false remain constraints; a new default never erases them.
  Parent effective restrictions remain ceilings during continuation/recovery.
- Explore's built-in default deny layer must contain scoped exceptions for actual
  registered MCP actions, rather than leave them under donor blanket Deny. Keep this
  builtin-default exception separate from user constraints; custom wildcard Deny is
  not reordered/overwritten. No hardcoded server/tool list, guessed search suffix,
  readOnly/destructive annotation classifier or MCP allowlist restricted to web tools.
- Use the registry's collision-safe wire identity (`server__tool`) and existing
  compatible permission alias (`server_tool`) consistently for catalog visibility,
  delegation previews, initialize guidance and invocation/resource admission.
  Explicit whole-action Deny removes schemas from new requests; resource-dependent
  capabilities remain conditional. Ask may be advertised only truthfully and must
  use the real permission consumer; headless without one fails before tools/call.
  Denied/Ask-only tools do not acquire unconditional server instructions.
- Reuse T46 clients/lifecycle and the captured request lease. Pending/failed/disabled
  services supply no new usable tools/guidance; profiles never enable/connect a
  disabled server. New/relisted/disconnected entries reconcile at safe request
  boundaries; prepared requests retain their captured view and actual lease checks.
  Restart reconstructs the admitted catalog, not grants or uncertain tool effects.
- MCP Allow covers all registered MCP tools, not a claim that they are read-only.
  Explore/Plan native filesystem policies and their planning/research guidance remain;
  shell/MCP have separate actual policies, as in the existing non-sandbox contract.
  Do not silently add an effect classifier or a new external side-effect sandbox.

#### Plan behavior, not just registration

Plan native mutations use T50's canonical permission identity for both apply_patch
and edit/write. Deny ordinary files and narrowly admit the donor-compatible
`~/.opencode/plan` exception within existing trust/external-directory/no-follow/
preimage/data-root checks; no whole-HOME grant. Reminders forbid creating/updating
plan files unless explicitly requested, forbid delegating forbidden file changes,
and retain Plan until the user selects another agent. Do not parse free user text by
keywords to guess authorization or create plan files automatically.

Creation/selection into Plan emits the enter reminder; leaving emits the leave
reminder, repeated same-profile selection does not duplicate it. Request assembly
reconciles missing/stale reminders after DCP/native compact/Revert/reopen while
preserving chronological immutable raw records. Custom system overrides cannot erase
the independent Plan reminder/policy layers. Ordinary implementation prose, MCP use,
model/variant switch or --auto does not switch agents or bypass native mutation Deny.
Supply `run --agent plan` through R6's existing selection-before-prompt owner.

#### Ownership and evidence

T45 owns registration/merge/eligibility, default MCP authority/effective views,
Plan reminders/selection and CTX02/PRM01 qualification. Reuse SUB01/SUB02 for actual
child launch/continuation/narrowing, DCP10 for Explore own-history compression and
T43/AUD42 for resource rules; do not duplicate their full matrices. T50 keeps file
selector/executors, T46 transports/catalog lifecycle, T44 VIS06/VIS10/VIS17/VIS26/
VIS39 presentation after explicit resume. Minimal qualified seams, not whole-task
completion dependencies or reopening completed T46 solely for this new default.

Minimum primary evidence: no custom profiles → real Build/Plan/General/Explore
catalog/eligibility and captured root/child requests; both children and Plan perform
fake-MCP search plus an arbitrary non-web tool without MCP overrides; explicit
central/parent/profile Deny/Ask/aliases agree across schemas/preview/guidance/effects.
Then actual-binary Build → Plan → Build, native mutation zero effects outside the
plan directory, explicitly requested plan-file effect, compact/Revert/restart and
headless --agent proof. Use deterministic fake HTTP/stdio/provider counters and
existing actual-binary harnesses, no new paid/browser campaign. Full methodology:
[TEST_PLAN](../TEST_PLAN.md#built-in-profiles--default-mcp-access--t45-approved-2026-10-01-pending).
New scenarios remain pending/NOT_RUN until rebuilt binary/effect evidence and the
existing affected-crate/workspace gates; historical PASS is not sufficient.

### Constraints

- C1: Rust 2024, modular monolith, core independent of UI, KISS/YAGNI; no Node/Bun/JS host in production.
- C2: No hardcoded production model IDs, no vendor-specific routes, no secrets/raw live responses in logs or artifacts.
- C3: Do not weaken contracts to gain green checks: validation/security/accessibility/concurrency guarantees stay intact, no suppressed tests. MCP per-server degradation follows D13; its fatal cancel/cleanup/caps remain. The original fatal-attach rule is historical/superseded, not a current contradiction.
- C4: Progress engine `scripts/progress.py` is the only status source; one active task at a time.
- C5: Plan-directory admission is narrowly scoped to donor-compatible `~/.opencode/plan`; no general trusted-root widening. Map donor edit permissions to the shared native apply_patch mutation identity for the selected apply_patch OR edit/write family, preserving explicit Deny, parent/child narrowing, symlink/CAS/approved-preimage and data-root protections. Model selection cannot widen Plan path admission/grants. Shell follows its own policy as in OC2; Plan is not a shell sandbox. Only T50/R1/R9's file-tool exception supersedes the single-patch/no-write-edit clause; no JS plugin host or other model/reasoning/provider routing. Remaining native policy/tool differences must be explicit and cannot count as full parity.
- C6: Host metadata contains only explicitly selected fields. Do not project the full process environment, machine-id, network configuration or raw /proc files. Treat hostname and os-release values as bounded data, not instructions. No automatic toolchain inventory, network probes or privilege escalation. Host metadata does not grant or bypass native tool permissions. Free RAM/disk/load and toolchain versions are measured on demand, not injected as a changing inventory on every request. Keep native tool guidance truthful; no hardcoded production model IDs or copied guidance for absent write/edit tools.
- C7: No full-parent transcript cloning, quote-to-system privilege promotion, cross-session context access, unknown-effect auto-replay, unbounded active summaries/notification queues or disabling resource checks to simulate infinite sessions. Concurrency preserves one execution owner per session and existing patch CAS/symlink protections. Native child policy narrowing is an explicit difference, not identical donor permission algebra.

### Non-goals

- OAuth, ChatCompletions fallback, daemon/serve/attach, Code Mode, JS/TS/WASM plugin host, cloud orchestrator.
- Owner-approved tool follow-up is tracked separately by [T50](2026-09-27-native-tool-parity.md): shell/search/question/read/fetch and direct model/session tools. Built-in websearch/provider integrations, built-in browser and PDF remain excluded; explicit MCP search/browser remains supported. R3/R6/R10 profile restrictions/guidance refer to the selected native tool set, not a promise to expose excluded donor tools. T45 and T50 share existing owners without a completion cycle; no new framework or status claim.
- Unrelated refactors or audits; scope expands only on owner instruction or a diff-caused regression.

## Change Envelope

- Target: `crates/oc-adapters/src/{defs.rs,config.rs,composition.rs,runtime.rs,application.rs,tools.rs}`, `crates/oc-core/src/*`, `crates/oc-tui/src/*`, `crates/oc/src/*`, tests under `crates/*/tests/`, `docs/*`, `evidence/*`.
- Expected paths, symbols, and direct consumers: `split_frontmatter`, `parse_skill`, `load_kind`, `insert_agent`, `insert_command`, `normalize_permission`, `MAX_*` constants; subagent work touches session/tool/runtime composition and TUI rendering.
- R6 adds agent definition/merge/default selection, native Build/Plan registration and reminder lifecycle, narrowly admitted Plan-directory mutations under T50's selected family, headless CLI selection and existing T44 picker/cycle consumers. Reuse application/session/config owners; no new agent framework or separate persistence engine. T50 owns executors and selector; T45 consumes their minimal effective view without whole-T50 completion dependency.
- R6's 2026-10-01 CLI-reference consumer touches existing defs/config normalization, composition and scoped application/model selection only as required to preserve the pinned provider/model/variant grammar and precedence. Use TOOL18's same catalog fixture/receipt and existing profile/recovery tests. Other admitted global/command/child model settings retain their existing owners and routing contracts; no new binding CLI, config writer, credential scan or whole-T53 prerequisite.
- R3/R6/R8/R10's 2026-10-01 built-ins/MCP amendment touches `defs.rs` builtin constructors/merge, `composition.rs` seeding/default authority, `permissions.rs` and `runtime.rs::RuntimePolicy` identity/visibility, both initial/follow-up `runtime/turn.rs` catalogs, `runtime/mcp.rs` guidance, existing application selection/reminder/context owners and `oc/src/{cli,headless}.rs`. Reuse existing TUI catalog consumers and tests; no agent registry/store, effect classifier, new MCP connection owner, new model tool or user-config writer.
- R7 adds a small native Linux environment collector in oc-adapters and integration into the existing application/runtime prompt assembler, including the base harness fallback and environment/date baseline. Reuse existing workspace and shell-executor metadata. No second registry, persistence engine, background service or new model-facing tool. This is a Linux extension, not a macOS/Windows implementation commitment or closure of all other OC2 instruction-lifecycle gaps.
- R3/R8–R10 extend existing session workers/job ownership, subagent schema, durable task/pack admission, conversation views, DCP block/protection queries and shared prompt/instruction projection. Internal typed snapshots/transactional storage fields may be added only for these outcomes; no donor KV clone, external scheduler, second executable registry, generic agent framework or new archive/history copy. Current GOAL.md amendments admit this scope; original audit leaves remain historical evidence.
- R9/DCP12 additionally touches existing `dcp_auto.rs` defaults/resolution, `composition.rs` config loading, `models.rs` budget reuse and runtime/application/query/panel threshold consumers. Reuse one effective model/config owner, minimal typed fields and bounded active queries; no new policy engine, config writer or lifetime counter. Executable examples/help change with actual implementation, not this plan-only approval.
- R10 live-selection consumer additionally touches existing `application_selection.rs`/typed selection acknowledgement, `oc/src/tui_cmd.rs` and `oc-tui` composer model/variant draft/captured submit/blank-Enter/reconciliation, and shared prompt/capability/history identity projections. T50 owns the minimal busy-commit/per-request backend slice and TOOL12; reuse it without a second owner/store or whole-task completion dependency. Do not reopen T49 or rewrite its historical attribution evidence: preserve actual request identity through targeted regressions in the current slice. Future provider-qualified T53 binding is consumed through existing admitted APIs, not redesigned here.
- R9/DCP11 hot/cold clarification touches existing `dcp.rs`, `runtime/context.rs`, `runtime/turn.rs`, `runtime_compaction.rs`, `storage_dcp_view.rs`, compression/checkpoint commits and their existing context-version/fork consumers. Minimal persisted descriptors/flags may change only to represent admitted replacement/protection/occurrence identity faithfully. Reuse existing range schema/owners/tests; no new forget tool, cold exporter, automatic Git work, raw-history deletion, generic memory service or separate archive. Detailed slices and evidence are in docs/DCP.md and roadmap/M8.md.
- Allowed and forbidden artifacts: source, tests, docs, evidence notes. Forbidden: editing GOAL.md/audit gates to pass, deleting tests, adding JS runtime, committing secrets or `.local/` contents.
- User or harness budget: commits + pushes required; no attempt limit; live calls bounded.

## Current Checkpoint

- Built-ins/MCP follow-up (owner-approved 2026-10-01): after safe handoff, first freeze a no-custom-profile fixture for missing Plan/General/Explore and absent central MCP authority. Implement the shared builtin/default policy view, then captured schema/preview/effects and Plan lifecycle using the ordered M8 slices. Existing T50 dirty work, T55 scheduling priority, T44 PAUSED and all task statuses are unchanged; this delivery is plan-only.
- CLI/profile follow-up (owner-approved 2026-10-01): after T50's minimum `oc models` catalog consumer, qualify one exact listed reference through R6 JSON/Markdown profile selection/request/reopen. Basic loader/profile normalization may proceed with shared fixtures before the CLI lands, but final binding evidence must cite the real CLI receipt. T50 active, T44 PAUSED, current dirty work and T55 safe-handoff priority are unchanged; no historical profile PASS qualifies this new end-to-end scenario.
- Closes: smallest unresolved T45 slice of R3/R6–R10, not already verified R1/R2.
- Smallest next action: after a safe scheduling handoff to T45, reconcile actual HEAD/foreground prerequisites and choose one unresolved slice from roadmap/M8.md. For R9/DCP11, first freeze kept/forgotten control facts and reproduce replacement resurrection; do not start with cap increases or a full archive rewrite. This plan-only amendment does not switch the active task (T50 at approval) or resume PAUSED T44.
- Expected evidence: assigned targeted scenario plus relevant crate/workspace gates; plan validators are not implementation PASS.
- Stop or replan if: a slice crosses the stated authority/trust boundary or requires unknown side-effect replay; retain historical tests and record the precise donor/native difference.

## Current State

- Resolved: R1/R2 warning fixtures remain verified by their historical evidence; T43 foreground prerequisites landed, but full R3/R6–R10 qualification is pending.
- Last relevant evidence: pinned OC2/DCP research and read-only code inspection at b88dc0fe8; existing release/one-compression reports do not qualify the newly approved outcomes. No lifetime compress-call cap was found. Recursive summaries, covered-ID materialization, retained closed tool wire, inherited protection, asymmetric checkpoint measurement, stale reminders and full-before recovery are source-derived risks, not executed RED/PASS evidence. The 2026-09-30 amendment changes the target from mere flattening to intentional hot forgetting.
- Blocker: none.
- Next: T45 independently implements the ordered slices; T44 consumes owner-backed child/profile surfaces. Keep task statuses/dependencies and executed evidence unchanged in the plan-only delivery.

## Material Decisions

- 2026-10-02: owner approves host OC1 config RECON plan corrections; pinned OC2 remains the oracle. R9 consumes separately qualified DCP3.2.0 admission/provenance under T51, not a false parser-only child-DCP PASS. R10/PRM01 consumes chronological system/effort and T50 same-task captured/next-request semantics from T53's finite protocol seam. Default MCP Allow retains explicit last-match wildcard rules inside a ruleset and central/parent/profile intersection; host Allow exceptions do not erase another authority's Deny. New qualification pending, task statuses/baselines/history/T44 PAUSED unchanged.

- 2026-10-01: owner approves expanded built-in Plan/General/Explore/default-all-MCP plan and commit/push after RECON. Superseded only donor Explore MCP denial and old default MCP Ask/missing registered-action grant; explicit central/parent/profile Ask/Deny, native file/shell/question/delegation restrictions and non-sandbox/security invariants remain. All registered MCP tools, not a web-name allowlist, are covered. T45 owns CTX02/PRM01 additions, T46/T50/T44 retain their existing scopes; new implementation/qualification pending, no status/PASS/baseline changes.
- 2026-10-01: owner requires original OC2 model switching while work is active and approves detailed plan commit/push after follow-up RECON. R10/PRM01 consumes T50/R1's local draft versus committed selection, blank Enter/captured send ordering and next-request adoption inside the same task. Superseded only blanket model/variant busy refusal and whole-turn model pin, not config/Location generations, child/profile/read-only authority, policy/preimage, retry/quarantine or no replay. U95–U102 append source proof; T47 ordering/T44 visual ownership and all execution statuses/evidence remain unchanged, implementation pending.
- 2026-10-01: owner approves detailed file-tools plan commit/push after RECON. T50/R1/R9 owns exact OC2 model-dependent apply_patch OR edit/write selection, mandatory next-request switch and real mutation semantics. R6/R10/PRM01 consumes this shared view for root/own-model child, Plan, base/custom guidance and capability previews; only old single-patch/no-write-edit/model-name-selector clauses are superseded. No new T45 executor or model routing, policy widening, task/status change or historical PASS rewrite; T44 stays PAUSED and separately owns VIS35/VIS36.
- 2026-10-01: owner approved full child-TUI/Subagents/Shell/Terminals visual and interactive plan, explicitly including Terminals. R3 now supplies ordinary live child events, authoritative family/status and independent controls; only blanket navigation/control refusal is superseded, not child turn/profile/model/Undo authority. T50/TOOL13 retains shell jobs, new T56/TERM01 owns native session PTYs and T44/VIS39 owns paired visual qualification. No completion cycle or new T45 store/framework; active T50, PAUSED T44, historical evidence and R1/R2 statuses unchanged.

- 2026-09-30: Owner rejected history-preserving framing of infinite context and approved recording hot-context replacement/forgetting with entirely optional user `.md`/Git cold memory and compatible manual `/compact`. Supersedes only R9/DCP11 clauses that would make old summary contents/tool groups/protected ancestry mandatory hot dependencies. Retained explicit references/current protections remain faithful; raw history, durable execution safety and conversation-only Undo are not deleted. New detailed evidence checks deliberate absence/no resurrection as well as selected-fact preservation. No new task/status/paid campaign; plan-only commit/push is not implementation PASS.

- 2026-09-27: Owner approved native DCP omitted-field defaults 40%/55%/summaryBuffer=false for total-active long-horizon reminders; frequency5/iteration15/soft and other defaults retained. DCP12 has sole owner T45/R9; optional dcp.json/jsonc and explicit overrides remain. Shared canonical model-budget resolution fixes the panel's raw-ID/context=0 discrepancy; upper nudge criterion is distinct from hard model/input admission. R9/previous scenarios/statuses/evidence remain pending or historically factual; no baseline/source/default-profile rewriting to claim implementation PASS.

- 2026-09-27: owner approved T44 VIS39 full delegation visual/interactive parity. R3 supplies its minimal typed lifecycle/projection/control slice and SUB01/SUB02 evidence; T44 owns paired cells/PNG/temporal qualification. Launch badge and live child state are separate, completed prose is not failure, native response-close-before-admission remains. No new task/store/framework or whole-task completion dependency; statuses and earlier evidence unchanged.

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
- [Actual foreground-to-background conversion](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session.ts#L425-L440); typed consumer and full visual contract: [T44 VIS39 amendment](../../tui-recovery/T44_CONTRACT_AMENDMENT.md#subagent-delegation-parity--vis39), U35–U49 + U02 in SOURCES.json. SUB01/SUB02 remain solely T45-owned.
- [Instruction baseline sources](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/context.ts#L127-L155), [AGENTS discovery](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/plugin/instruction.ts#L32-L120), [chronological instruction rendering](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/instruction-discovery.ts#L123-L149), [session instruction reconciliation/nested reads](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/instructions.ts).
- [Skill metadata instructions](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/skill/instructions.ts), [model prompt hooks](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/plugin/optimize.ts#L64-L91) — differences governed by native A13/no hardcoded model IDs, not silent full parity.

DCP reference: [range tool](https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/compress/range.ts), [range utilities](https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/compress/range-utils.ts), [block/run state](https://github.com/Tarquinen/opencode-dynamic-context-pruning/blob/11f6517780a502512a3467645074be447cb0369e/lib/compress/state.ts). Revision/version/license are recorded in `planning/baseline.lock.json` and `fixtures/dcp-compress.json` (3.1.15, AGPL-3.0-or-later). No verified lifetime counter limit is claimed. R8 context packs and R9 default-child/no-exhaustion behavior are owner-approved native additions; [docs/DCP.md](../DCP.md) records the resource/protection contract.

DCP defaults/config reference [D4](../SOURCES.md#dcp) pins lib/config.ts at the same revision. Its 50000/100000/summaryBuffer=true differ from the approved native R9/DCP12 target; config read/default semantics are source evidence, not new runtime PASS.

## Completion

- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
