# Goal: Selected native tool parity with OC2

Status: active
Contract status means the owner-approved finish line is frozen; execution state belongs to progress/STATE.json. At the 2026-10-01 amendments T50 is active and T44 is PAUSED. Historical delivered slices do not qualify the new R2 or model-dependent R1/R9 extensions.
Source: owner-approved tool RECON and plan, 2026-09-27: omit built-in websearch and Code Mode, accept the remaining proposed tool work, then update the plan and commit/push. Donor OC2 v2.0.12 at `2670273ff17da96f85c5826ced57aa1b368754fa`.
Additional source: owner request for OC2 TS edit/write for non-patch models, apply_patch for compatible GPT models, mandatory catalog removal/replacement after a user model switch, followed by detailed plan commit/push approval, 2026-10-01.
Additional source: owner requires OC2 TS parity when switching models during an active task, approves the follow-up RECON and detailed plan commit/push, 2026-10-01. This supersedes whole-turn model pinning, not immutable Location/config generation or execution authority.
Last updated: 2026-10-01

## Objective

The selected Linux-native tool set matches the agreed OC2 capabilities through real model calls, application-owned outcomes and frontend consumers. The selected model receives apply_patch or edit/write using the exact donor predicate. A committed user model switch is accepted during busy and replaces incompatible definitions and runtime-managed guidance before the next request of the same autonomous task, without a new prompt. Picker draft is not commit; the prepared request and its tools retain their captured view. This is not full upstream parity: built-in websearch, Code Mode/execute, built-in browser and PDF remain excluded. Explicit MCP search/browser tools remain supported, not automatically connected or advertised.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: The advertised tool set and guidance reflect the approved scope.
  - Source: owner explicitly excludes websearch and Code Mode and approves the remaining tool proposal; 2026-10-01 instructions and approved RECON require model-dependent file tools, bidirectional compatibility and OC2 live-switch parity, including the draft/commit distinction and same-task request-boundary adoption.
  - Owner: T50; T45 retains profiles/prompt/subagents/DCP, T46 retains MCP parity, T44 retains visual qualification. No circular completion dependency.
  - Acceptance: canonical direct tools are read, glob, grep, the selected file family (apply_patch or edit/write), shell, webfetch, skill, question, subagent, compress, opencode_models, opencode_session_rename and opencode_session_move, subject to effective policy/config/capabilities. Do not register a built-in websearch/provider selector, Exa/Tavily integration, execute interpreter or built-in browser. Advertise configured MCP tools only from the actual admitted catalog; absence of built-in websearch is not a ban on MCP search tools.
  - Acceptance: use the exact case-sensitive donor model.id substring predicate: contains "gpt-" and neither "oss" nor "gpt-4" selects apply_patch and removes edit/write; otherwise select edit/write and remove apply_patch. Names and schemas of each tool remain stable, not provider-dependent aliases. Native apply_patch keeps patchText, not donor patch naming or hosted Responses schema. Do not infer family from provider/display name, rewrite IDs, add a model allowlist or change discovery/reasoning/protocol routing. Effective permissions may further remove tools; model matching never grants authority.
  - Acceptance: each prepared request captures one coherent selected model/variant/file-tools view for admission/budget, runtime-managed guidance, execution allowset and schema/context/cache fingerprints. Children use their own admitted selection, not the parent's family. Commit model/variant while busy and reload it before the next primary request of the same tool loop, without waiting for a new user turn. GPT → non-GPT → GPT obligatorily replaces incompatible definitions/guidance; restart/DCP/native-compaction reconstruct the latest committed view. Retry/context-rebuild boundaries also reload selection under existing finite policies. Prepared streams/tools/approval retain their original view; never cancel/restart/retarget them merely because selection changed.
  - Acceptance: TUI picker updates a session/agent-local composer draft (Location/agent before a session); it does not immediately mutate the running model. Normal message/command preparation commits the captured choice in admission order. Blank Enter in the ordinary composer of an existing authorized session commits without new user text. Reconcile by owner ack/matching events; failures/stale echoes cannot falsely commit or erase a newer draft. Selection alone emits no generation, interrupt or filesystem effects; no natural continuation means the next run uses the committed choice. Preserve existing child/read-only/Location/profile authority, not blanket busy refusal of an authorized model/variant action.
  - Acceptance: removing tools means current built-in definitions/catalog/managed instructions, not deleting/translating old calls/results, arbitrary user/profile text or raw records, nor filtering namespaced MCP suffixes. Retain completed model-neutral call/result groups and actual tool outcomes in B's compatible bounded projection, including same-task continuation; current wire_history's model-mismatch fresh lane must not lose those results. Strip incompatible opaque/checkpoint state without translating tool names or reading forgotten archive. Reject a call excluded by the snapshot of the request that issued it, before filesystem effects with matched typed result/state; do not reject A's already issued apply_patch solely because the current selection is B/edit-write. No old call is newly executed from history. Request-log/assistant attribution records actual model/variant, not mutable selection or one invented model for the entire multi-request turn.
  - Acceptance: canonical shell uses the OC2 command schema. Preserve existing bash(argv/cwd/timeout_ms) compatibility through the same executor/admission owner; normalize permission identities without bypassing legacy Deny, saved-grant scope or structural ceilings. Do not advertise two interchangeable shell tools or silently reinterpret argv as command text. Per-tool names/descriptions/schemas and effective root/child capability previews stay truthful; Explore remains read-only with its separately approved own-history compress grant.
  - Primary evidence: TOOL12 source-derived fake-provider/actual-binary root and own-model-child catalogs, real file outcomes and PTY selection. Synthetic IDs cover exact predicate/exceptions/case. Provider/tool/approval barriers prove draft-only versus commit, blank Enter/captured ordering, GPT → non-GPT → GPT inside one active task without another prompt, completion/attribution of old-request tools, refreshed next-request budgets/tools/guidance/fingerprints and compatible causal history. Retry/compaction rebuild/restart and excluded-by-request-call zero effects reuse existing invariants. See the live-switch slices below and docs/TEST_PLAN.md; PRM01/profile/VAR01 and existing A05/A06/A13/PROV04 remain regressions, not duplicate owners or a paid matrix.
  - Status: pending
  - Evidence:

- R2: Linux shell supports real command and owned foreground/background lifecycles.
  - Source: approved Linux shell parity proposal; donor shell schema and job/notification implementation.
  - Owner: T50, existing shell supervisor and session/runtime ownership.
  - Acceptance: command runs in the actual selected/configured Linux shell; support workdir and timeout in milliseconds. Foreground is default with donor 120000 ms default; explicit timeout:0 disables execution timeout. background:true returns running/shellID after durable admission and actual launch; its default has no execution timeout, while an explicit timeout is honored. Zero/unset timeout never removes bounded output, queues, cancellation or teardown limits. Native trusted-workdir, minimal credential-free child environment and resource rules remain explicit differences.
  - Acceptance: background progresses while the calling session continues; exactly one durable terminal notice is delivered per job/delivery identity without model polling, even if the session is idle/busy or has moved. This is delivery deduplication, not exactly-once external execution. Capture bounded stdout/stderr/exit/signal/timeout/truncation and readable retained output; cancel/shutdown cleans owned process groups with TERM/KILL/wait/reap. Restart delivers committed results without re-execution; started/unknown shell effects are never auto-replayed or reported completed without evidence. A finished foreground response does not silently cancel admitted background work.
  - Acceptance supplement (2026-10-01): authoritative bounded list/status and live output cursor/snapshot/events identify actual active jobs for their original source session/Location/operation/generation. An initial immutable running tool result is not current job state. The Shell composer lists running jobs only; an already opened output viewer retains its original job identity through list removal and reads the final flush. Bounded live recent-output/retained reads work before completion, not only after a terminal outcome. Reuse the supervisor's actual drains; do not infer liveness from a transcript string or create a permanent UI poller.
  - Acceptance supplement: Ctrl+D kill targets the selected shell only, including child-origin work; it is not CancelTurn on an unrelated/current parent. Ctrl+B converts the already admitted foreground shell to owned background with the same process/operation/output provenance, releases its blocking wait according to the shared application control contract and retains durable terminal notice/delivery. Repeated controls and conversion-versus-completion/cancel races cannot spawn again, duplicate result/delivery or lose cleanup. T45 aggregates child/shell session-background controls through existing owners, not a competing shell manager. Parent move/view navigation preserves original execution context.
  - Primary evidence: TOOL13 actual binary with process/provider barriers proves early background return/progress/automatic notice, normal foreground waiting, default/explicit/zero timeouts, cancel/shutdown, crash/delivery and credential exclusion; reuse TOOL05/TOOL06/AUD27/AUD28 and A10 measurements, not duplicate helper-only suites.
  - Primary evidence supplement: actual-binary PTY and owner assertions prove authoritative running inventory, output while the process is held, same-PID foreground conversion, selected-job kill, final output/status after inventory removal, independent sibling progress and correct child/source routing through move/reopen/restart. T44/VIS39 reuses these facts for Shell rows/output dialog geometry and keys; TERM01/T56 interactive PTY is separate, not TOOL13 completion or a whole-task dependency.
  - Status: pending
  - Evidence:

- R3: grep and glob expose the agreed OC2 search options.
  - Source: approved search parity proposal; pinned grep/glob schemas and filesystem behavior.
  - Owner: T50, existing file/search executor and policy.
  - Acceptance: grep supports regex (default) and literal:true, path file/directory scope, include glob, caseSensitive (default true) and limit. Pin a vetted engine compatible with the donor ripgrep syntax; do not invent a custom matcher or promise unsupported regex features. glob supports path, hidden (default false), pattern and limit with donor matching/hidden/ignore behavior. Existing deterministic pagination may remain as an explicit extension, with coherent ordering/truncation diagnostics.
  - Acceptance: validate patterns/options before scanning; malformed regex and exhausted scan/result budgets are explicit outcomes. Preserve own-data-root exclusion, canonical admitted path boundaries, no-follow regular-file checks and bounded entries/bytes/time. External-directory behavior outside native trust admission remains a declared difference, not an implied access grant.
  - Primary evidence: TOOL14 pinned source-derived fixtures for regex/literal/case/path/include/hidden/ignore/options plus actual provider schemas/calls; TOOL01 and path/permission regressions remain authoritative.
  - Status: pending
  - Evidence:

- R4: question is a real application-owned user-question tool.
  - Source: approved question proposal; donor Question/Form tool, distinct from permission approval.
  - Owner: T50 end-to-end typed tool/application/frontend behavior; T44/VIS37 owns exact FormPrompt/card presentation and paired visual qualification, distinct from VIS36 permission approval.
  - Acceptance: nonempty questions, headers, options, multiple selection and automatically available free-form answer follow donor schema. Model execution waits for typed user answers; TUI presents the actual pending question and submits answers, with deterministic question order. Dismissal/cancel interrupts the relevant execution instead of inventing an answer or converting it to success. No DB transaction is held while waiting; bounded pending forms/replies are tied to operation/session/generation and reject stale/duplicate/foreign replies.
  - Acceptance: headless without a question consumer returns actionable non-success and preserves history; it does not hang indefinitely. Permission autoaccept/--auto never fabricates user answers. Effective question Deny and General/Explore restrictions remain; reopen/restart cannot duplicate an answered form or reinterpret an unanswered form as approval.
  - Primary evidence: TOOL15 scripted model calls and actual PTY answers/multiple/free-form/dismiss/cancel, headless/--auto and restart assertions; real application state/result and call/result graph, not a screenshot-only question widget.
  - Delivery order (owner-approved 2026-09-27): first deliver this minimal backend/real answer-consumer slice and prove ordered answers in the next provider request (TOOL15); then execute [T44/VIS37](../../tui-recovery/T44_CONTRACT_AMENDMENT.md#question-ui-parity--vis37) FormPrompt/transcript/replay and full paired styled-cell/PNG qualification using pinned U31–U33. Share the same fixture/runtime evidence, keep behavior and visual results separate; neither task depends on completion of the whole other task. Independent T44 work remains ready and only one journal task is active.
  - Status: pending
  - Evidence:

- R5: read supports text, directory listings and images with honest modality handling.
  - Source: approved read proposal; PDF remains outside the selected scope.
  - Owner: T50 file/result/provider boundary; T45 retains nested AGENTS instruction ownership.
  - Acceptance: path plus 1-based offset/limit reads text with line references or paged directory entries; donor default limit is 2000. Images supported by the selected Responses model are validated and delivered as real image content, not a success string containing only a path/base64 dump. Unsupported model/file modality, PDF, invalid images and over-budget content are actionable outcomes. Do not silently substitute text for unsupported media.
  - Acceptance: retain path/permission/symlink/own-data-root protections, byte/model budgets and bounded output. Successful file/directory reads invoke the same admitted nested AGENTS lifecycle as T45/R10, with provenance and dedup; image handling does not introduce a second instruction loader or wider filesystem trust.
  - Primary evidence: TOOL16 actual binary text/directory pagination and captured image continuation, malformed/PDF/unsupported/over-budget cases; reuse PROV05/PRM01 rather than claiming user-image input tests already prove tool-image output.
  - Status: pending
  - Evidence:

- R6: webfetch supports the agreed formats and timeout semantics.
  - Source: approved fetch proposal; pinned webfetch schema/conversion.
  - Owner: T50 existing HTTP/conversion executor.
  - Acceptance: read-only GET with format:text|markdown|html, default markdown, and timeout in seconds (default 30, maximum 120). Convert HTML according to requested format, preserve readable Unicode and useful structure, and return truthful original/final URL, status/content type and format metadata. No websearch integration, JS rendering/browser, upload or inherited cookies/auth.
  - Acceptance: one total deadline covers DNS/hops/body/conversion; preserve actual-dial/redirect SSRF protection, response/output caps, credential exclusion and explicit test-only loopback admission. Requested format never bypasses egress/security or creates an unbounded serialization path.
  - Primary evidence: TOOL17 fake HTTP and actual model calls for all formats/defaults/timeout/redirects/Unicode; TOOL07/TOOL08/AUD25/AUD26 remain mandatory.
  - Status: pending
  - Evidence:

- R7: Native model lookup and session rename are available directly to the model.
  - Source: approved native model/session tools proposal; donor opencode tool plugin.
  - Owner: T50 existing catalog and application/session/storage owners.
  - Acceptance: opencode_models supports query/provider/all/limit/offset and returns bounded grouped catalog metadata, variants and next page with donor newest-family and own-provider **model** ordering. Variant lists consume the shared [T47/VAR01 effective effort order](../CONTRACTS.md#canonical-effort-ordering--t47var01-approved-2026-09-27-pending), not a second lexical/source-order policy. This is a minimal catalog prerequisite, not whole-T47 completion. Unknown release/family/price metadata stays unknown; no hardcoded IDs, discovery changes, provider fallback or network/auth routing. Lookup does not switch the calling session's model.
  - Acceptance: opencode_session_rename accepts title and optional sessionID (current session by default), validates nonempty trimmed title and persists/publishes the real rename. Explicit targets must be locally known and authorized by the caller's effective session/tool policy; children cannot modify arbitrary parent/sibling/foreign sessions. Read operations and session-control mutations use the common policy pipeline, never authority inferred from descriptions. These native target-access ceilings are declared donor differences.
  - Primary evidence: TOOL18 captured direct schemas/results with static/dynamic/unknown metadata, paging and model retention; actual rename/reopen/restart and deny/foreign-target/no-effect assertions. No Code Mode or second catalog/store.
  - Status: pending
  - Evidence:

- R8: session_move durably changes the same session's Location at a safe boundary.
  - Source: approved session_move proposal and explicit supersession of permanent session-to-Location binding; pinned donor move/admission/projector.
  - Owner: T50 existing application/session/config-generation/storage owners; T45 reconciles its shared instruction/profile consumers.
  - Acceptance: opencode_session_move accepts directory and optional sessionID (current by default); resolve relative paths/~ against the targeted session context, validate existence/type, destination trust and complete target generation before acceptance. Foreign/unauthorized/stale targets and failed target admission leave placement unchanged. Persist one move identity/outcome; return an honest admitted/pending versus applied status, not an early false success.
  - Acceptance: preserve sessionID/history/DCP and apply the placement transition durably at a safe boundary. Never retarget an in-flight request/tool or mix Location/config/agent generations inside a native turn: the source turn reaches a durable terminal boundary under its original pinned generation; any destination model request belongs to a distinct subsequent turn with fresh generation/causality context, not a resumed source turn with replaced configuration. Destination-dependent calls in the same captured batch keep their source context or fail explicitly; they cannot assume the move already applied. Clear old project-local catalogs/rules and refresh actual environment/AGENTS/skills/MCP under the new admitted scope without resetting raw history or retaining invalid opaque continuation.
  - Acceptance: operations and running background jobs/children retain their original execution Location/generation and authority; a parent move does not silently migrate children or rewrite their task/pack provenance. Subsequent continuation respects actual child placement/lineage and explicit target admission. No move creates permission grants or drops sticky unknown-effect MCP quarantine. Restart deduplicates an already committed move and preserves pending safe-boundary intent without replaying shell/MCP/mutations; failed cleanup is never hidden.
  - Acceptance: ordinary SelectLocationSession/UI Location switching still selects/creates a target-scoped session. This distinct explicit move supersedes only the permanent-binding prohibition, not immutable Location/config-generation, trust or session-access guards; histories/reopen/UI metadata reflect actual placement and original operation provenance. R1's same-generation model switch is not permission for an in-flight Location/config move.
  - Primary evidence: TOOL19 provider/process barriers and actual binary A→B move preserve ID/history and show source/destination requests, blocked destination-dependent batch assumptions, invalid/untrusted/stale/no-effect cases, original background completion, child placement, quarantine and crash/restart around admission/apply. AUD14 is amended narrowly; existing UI06/A13/PRM01/CTX01/SUB02 and cleanup gates remain.
  - Status: pending
  - Evidence:

- R9: Native edit/write provide the donor file contracts through the shared mutation owner.
  - Source: owner instruction and detailed RECON-plan approval, 2026-10-01; pinned core tool/plugin/edit.ts, write.ts and FileMutation helpers. This narrowly supersedes the old no-write/edit clause; apply_patch behavior itself is unchanged.
  - Owner: T50 real tools/admission/results and minimal frontend data consumer; T45 consumes selected family for profiles/Plan/prompt, T44 owns VIS35 cards/paired visuals and VIS36 approval presentation, not a second executor.
  - Acceptance: write input is {path:string,content:string}; create a missing file, create missing parents and overwrite an existing admitted regular text file, including zero-byte content. Preserve supplied text/newline semantics and donor BOM handling; do not implement overwrite using patch Add or synthetic patch strings. Report actual target/resource/existed/operation and bounded confirmed effects. Existing ordinary mode/no-follow/staging/fsync/path/preimage safeguards apply, not unrestricted host paths.
  - Acceptance: edit input is {path:string,oldString:string,newString:string,replaceAll?:boolean}; target must exist. Reject empty oldString, identical old/new, no match and ambiguous multiple matches unless replaceAll=true. Empty newString deletes matching text. Match in donor precedence: exact nonoverlapping occurrences → typography-normalized occurrences → trailing-whitespace-normalized line matches. Preserve actual newline boundaries, normalize old/new for target CRLF and preserve BOM. Return actual replacements and bounded before/after files/diff metadata. Do not relax strict apply_patch matching or invent a regex replacement schema.
  - Acceptance: reuse existing directory-relative no-follow mutation primitives, per-path prepared bytes/digests, approved-preimage revalidation and bounded file/arguments/result/plan budgets through one internal owner. Extend resource extraction/home normalization/approval grants and own-data-root protections to actual edit/write path. Shared legacy_key mutation identity keeps explicit Deny/Ask/Allow, saved-grant scopes and central/primary/child ceilings; changing model or tool name cannot create a grant. Donor permission identity is edit, native canonical identity remains apply_patch as an explicit compatibility difference.
  - Acceptance: prepare actual before/after preview before Ask, revalidate after the user wait and immediately before commit; a changed identity/bytes/absence preimage fails with no unapproved write. Persist exact-call intent before effects and truthful outcome/confirmed effects afterwards. Failure/partial/unknown/cancel/storage boundaries and restart never duplicate or reexecute mutations. No separate preceding read-tool invocation is required: mutation preparation reads its own bounded preimage under the mutation policy, as in OC2.
  - Acceptance: DCP and configured mutation protections check edit/write's parsed path, not oldString/newString/content as pseudo-paths; patch retains all source/destination affected_paths. Preserve protected tool/content/dedup/purge behavior, immutable raw history and bounded hot projection without old-schema resurrection. Persist result-derived presentation for live/reopen/restart; never reconstruct historical effects from present-day files or mutate through the renderer.
  - Primary evidence: TOOL20 source-derived edit/write fixtures prove schema/output, exact/normalized/line matching precedence, unique/all/no-match/ambiguity/empty-identical-old/empty-new, Unicode/CRLF/final-newline/BOM and write create/overwrite/empty/parents. Actual rebuilt binary/fake provider verifies bytes, durable matched call/results, confirmed diffs, Allow/Ask/Deny, changed approval preimage, path/data-root/protections and crash/reopen/no replay. Reuse TOOL02–TOOL04/TOOL09/TOOL10/AUD04–AUD06/AUD16/AUD42 and A10 without duplicating their entire matrices at every layer. VIS35/VIS36 visual results remain separate.
  - Status: pending
  - Evidence:

### Constraints and non-goals

- Rust 2024, Linux rootless, existing crate DAG/error/policy/ownership levels; no service, JS/Code Mode/plugin host, cloud scheduler, second executable registry or second history/archive.
- Model-dependent file-family exposure shares one native mutation/policy owner; explicit Deny and central/parent-child narrowing, data-root/source/path/symlink/CAS protection, immutable raw history, bounded queues/output/media and no unknown-effect replay remain. Do not remove tests or change A10 baselines to qualify the extension. Exact donor file-tool predicate is the only new model-name exception; production IDs/reasoning allowlists/provider routes remain forbidden.
- Excluded: built-in websearch/provider integrations, Code Mode/execute, built-in browser, PDF, LSP, filesystem snapshots/undo, arbitrary shell terminal manager within T50. No Formatter/LSP subsystem is introduced merely because donor mutation tools call it; this slice ports the agreed file schemas/semantics under existing native boundaries. Owner-approved [T56](2026-10-01-native-session-terminals.md) narrowly adds explicit session-local interactive PTYs; it does not turn shell into a terminal tool or permit a daemon/credential inheritance. Explicit external MCP tools remain opt-in and permission-gated.
- Historical audits/PASS and existing task statuses are not rewritten as implementation evidence. Native resource/trust/permission differences are explicit; no claim of identical donor internals or unlimited resources.

## Live model switching — R1/TOOL12 (approved 2026-10-01; pending)

### Frozen behavior and source

- U95/U96: choosing B in `/models` changes the local composer draft only; ordinary
  submission commits its captured selection. Blank Enter commits in an existing
  ordinary composer before optional queued-input promotion, not as a synthetic prompt.
  Per-session/agent draft and owner commit are distinct, including variant choice.
- U97: `Session.switchModel` accepts a changed model while busy and publishes the
  durable selection event; unchanged provider/model/effective variant is a no-op.
  It does not interrupt, wake an otherwise idle session or itself execute a tool.
- U98/U99: session/model/tool state is reloaded at request preparation, including
  tool-driven next steps and admitted retry/compaction rebuilds. The prepared attempt
  owns its stream/tools/model identity. A switch after its snapshot waits for the
  next preparation; it is not a racy edit to the already prepared request.
- U100 directly proves switch while a tool is blocked, then continuation on the
  replacement model in the same run with no new prompt. U101 proves captured TUI
  commit order behind earlier pending admission. U102 retains ordinary tool pairs
  while withholding incompatible model/provider-bound metadata.

```text
picker B -> local draft only
commit B -> durable session selection while request A/tool A is active
settle A stream + issued tools under snapshot A -> prepare next request from B
request B -> compatible B tools/guidance/budget/history -> same task continues
```

### Ordered implementation slices

1. **Selection owner and real UI commit.** T50 supplies the minimal existing
   application/session acknowledgement/typed committed selection needed by TOOL12;
   T45/R10 consumes it for composer draft/captured submission/blank Enter. Split
   authorized model/variant actions from busy guards for profile/config/Location/
   child/read-only operations. Preserve availability/trust checks, scopes and failure
   feedback. Commit choice, not a deferred-until-idle command or a new model-facing
   tool. Exact same choice is a no-op; unmatched/late events cannot replace a newer draft.
2. **Request selection and coherent rebuild.** Replace whole-turn model capture
   with per-request resolution through the existing session selection owner. Keep
   config/Location/agent generation pinned. Resolve exact model/variant and admitted
   provider binding before the next dispatch; recompute known/fallback input/output
   limits, DCP model thresholds, tool schemas/managed lanes, history compatibility,
   request estimates and fingerprints together. T47 ordering/budget rules and T53
   provider-qualified selection/credential binding remain existing owners; consume
   minimal slices without inventing endpoint heuristics or all-task dependencies.
3. **Captured execution and history identity.** Lease the request's tool view and
   model identity through its stream and settled tool batch, including approval wait.
   Use that allowset for calls, not the latest preference. Common permission/preimage
   checks still apply. Extend existing request/assistant log and presentation fields
   minimally for actual identity if the current single-turn model field is insufficient;
   do not rewrite old A receipts/footers as B. Reproject retained closed call/results
   for B without incompatible opaque state or loss of outcomes. Keep raw/journal
   causality, bounded hot state, result dedup/quarantine and no tool replay; no new store.
4. **Actual-binary behavioral closure.** First keep A's stream/tool or Ask blocked,
   choose B without commit and prove no owner switch, then commit while busy and
   prove A settles once while the next request is B. Exercise B→A before a later
   natural continuation. Independently compare sent identities/tools/guidance/budgets/
   fingerprints, file bytes/results/paired history and actual assistant attribution.
   Add retry/compact rebuild and restart of committed selection within existing
   policies, not an expanded retry allowance or replay of unknown effects. A terminal
   answer plus selection commit produces no extra request. The independent child's
   admitted view stays isolated; this is not permission to edit an unauthorized child.
5. **Separate consumer/visual closure.** PRM01 proves shared prompt selection and
   current tool guidance, reusing TOOL12 switch receipts rather than its whole matrix.
   After explicit T44 resume, VIS09/VIS29/VIS17 show picker/composer draft versus
   actual A/B request attribution, VIS35 old/new file-family cards and VIS36 retained
   approval identity, with full paired styled-cell/PNG/cursor frames. Renderer-only
   goldens cannot prove live switching. TOOL20 filesystem semantics remain separate.

## Change Envelope

- Reuse `crates/oc-adapters/src/{files,tools,shell,webfetch,runtime,application,composition,config,storage}.rs`, `oc-core` typed commands/queries/results, existing provider result lowering and `oc`/`oc-tui` consumers; targeted tests/fixtures/docs/evidence. Changes to internal typed media/job/move storage are allowed only for these outcomes.
- R1/R9 additionally touch `runtime.rs::builtin_tool_defs`, `runtime/turn.rs` early/per-request assembly and captured execution, `models.rs` selected ID/budget, `provider.rs` schema-based cache inputs, `runtime/context.rs::wire_history`/protection extraction and existing compaction callers. R1 live switching includes `application_selection.rs`, `application.rs` SessionSelection/SelectModel owner guards, existing storage selection/event/request-log transactions and `oc-core` typed snapshot/event identity; `oc/src/tui_cmd.rs` and `oc-tui` composer draft/capture/blank-Enter/reconciliation are direct consumers. Minimal internal identity/schema migration is allowed only if existing records cannot truthfully represent multiple request models in one turn. Use one selection/mutation/context owner, no global registry rewrite, provider-routing redesign or new store/cache.
- R9 extends `patch.rs` and `patch/{fs,effects}.rs` through a coarse private shared mutation seam, `tools.rs` registry/validation/resources/dispatch, `approval.rs` preview/grant/preimage preparation, `permissions.rs`/`config.rs` path normalization and `dcp.rs`/runtime mutation protections. Existing invocation permits/durable tool effects and core FileEffect/PatchEffects payloads may extend minimally; do not create a second executable registry, generic framework, new crate or public API solely for tests. Add substantial tests beside their owner in separate test modules and retain existing Cargo targets.
- Direct presentation consumers are `oc-tui/src/tools.rs`, `app/live.rs` and existing history projection/results; T50 supplies persisted write/edit input/results/effects, T44 extends VIS35/VIS36 actual cards/previews. Update CODE_MAP/fixture paths with the implementation only; this plan leaves unrelated dirty session_move/CODE_MAP work untouched.
- A vetted regex/search dependency or existing ripgrep integration and a bounded Markdown converter may be chosen for R3/R6; pin/provenance/compile checks are required. No dependency is added by this plan-only delivery.
- No private authoring config, secrets, deployment changes, paid search provisioning or executable runtime/config example edits in this plan update. Ordinary own-branch commit/push is required.

## Current Checkpoint and State

- At this plan amendment T50 is active; its latest checkpoint qualifies R6/TOOL17 and directs R7 lookup/rename then R8 move (verify actual Git/NOW). This patch does not alter those execution statuses or historical evidence. T44 remains PAUSED.
- R7 lookup/rename is committed at 0dc85e817. Initial file-tools RECON was at 3446dc3c1; that plan landed in 760d54f7d. Live-switch RECON/this clarification starts from 760d54f7d with R8 session_move/CODE_MAP/runtime/evidence dirty. Git takes precedence over the older R6 handoff. Preserve all dirty work and settle its current slice before scheduling this extension. R2 inventory/live-output/foreground-conversion and R1/R9 file-tools/live-switch extensions remain pending; follow M8 ordering without a second active task or interruption of approved T55 priority. Neither historical file-tools plan nor existing busy-refusal tests qualifies corrected same-task switching.
- Checks: TOOL12–TOOL20 have one owner T50; relevant A02/A03/A04/A05/A06/A07/A08/A10/A13 and prior scenarios are regressions, not reassigned owners. TOOL12/TOOL20 and T44 VIS35/VIS36 are separate behavioral/visual results; existing A09/live owners/flow remain. Plan validation is not runtime PASS.
- Blocker: none for this plan delivery; PDF is deliberately excluded, not a hidden pending requirement.

## Pinned References

All links point to the admitted OC2 v2.0.12 commit; current native restrictions are compared explicitly rather than copied from V1 docs.

- [Registration and scope](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/plugin/internal.ts#L208-L242), [direct/CodeMode snapshots](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool.ts#L225-L262).
- [Shell schema/defaults](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/shell.ts#L22-L73), [owned execution/background/notifications](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/shell.ts#L153-L278), [source shell after movement](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/shell.ts#L10-L26).
- [Grep options](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/grep.ts#L17-L38), [glob options](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/glob.ts#L17-L28), [filesystem search](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/filesystem.ts).
- [Question schema and Form lifecycle](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/question.ts#L12-L127), [typed question prompts](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/schema/src/question.ts), [actual TUI forms](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/form.tsx).
- [Read schema/media/nested AGENTS](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/read.ts#L17-L113), [read filesystem](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/read-filesystem.ts).
- [Webfetch formats/defaults/conversion](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/webfetch.ts#L12-L100), [Markdown converter](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/html-markdown.ts).
- [Models/rename/move tools](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/opencode.ts#L11-L197), [move validation/admission](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/move.ts#L74-L152), [same-session placement projection](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/projector.ts#L466-L478).
- U90 — [Exact file-tools selector, context/compaction/generate hooks](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/patch.ts#L296-L309); U91 — [edit schema/matching/preparation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/edit.ts#L24-L213); U92 — [write schema/preview/create/overwrite](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/write.ts#L23-L91).
- U93 — [Availability-conditioned write/edit guidance](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/system-prompt.ts#L9-L27); U94 — [Write presentation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/index.tsx#L3044-L3078) and [Edit presentation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/index.tsx#L3324-L3386). IDs append to existing tui-recovery/SOURCES.json, without renumbering historical sources.
- [Provider lowering/model metadata scope](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/to-llm-message.ts#L153-L247) preserves historical call/result pairs; selector removes current tools, not old raw records. Input repair is schema repair, not cross-tool history translation.
- U95 — [Picker changes local draft](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/component/dialog-model.tsx#L126-L139); draft scope/reconciliation: [local selection](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/context/local.tsx#L263-L381). U96 — [Commit/blank Enter/captured prompt preparation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/component/prompt/index.tsx#L1274-L1360).
- U97 — [Busy-allowed model event/no-op](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/session.ts#L90-L102); [API forwards without busy guard](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/server/src/handlers/session.ts#L246-L253). U98 — [Boundary session/tool/model snapshot](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/context.ts#L121-L175); U99 — [Next-step/retry/compaction request preparation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/llm.ts#L187-L247).
- U100 — [Blocked-tool switch and same-run continuation regression](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/session-runner.test.ts#L3386-L3407); overflow rebuild: [3022–3076](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/session-runner.test.ts#L3022-L3076). U101 — [TUI pending-admission/captured selection order](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/test/compact-admission.test.tsx#L193-L226); U102 — [Compatible historical tool pairs/opaque metadata](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/to-llm-message.ts#L153-L256).

## Material Decision and Completion

- 2026-10-01: follow-up owner instruction requires original OC2 switching during active work and detailed plan commit/push. Frozen boundary becomes a prepared request plus its tools, not the entire native turn. R1/TOOL12 now includes busy-allowed committed selection, local draft/captured commit/blank Enter, next-request adoption within the same task, retained compatible outcomes and real request/assistant attribution. Only model/variant busy refusal, whole-turn model pin and incompatible result-losing projection are superseded; config/Location generations, authority, policy, finite retry and no unknown-effect replay remain. T45/PRM01 and T44 VIS09/VIS29/VIS17/VIS35/VIS36 consume minimal slices; ownership/statuses/history/R8/T55 schedule unchanged, qualification pending.
- 2026-10-01: owner approves RECON plan and requires detailed work-plan commit/push in the current branch. R1 now selects apply_patch or edit/write using exact donor predicate and obligatorily refreshes next-request tools/managed guidance after a user model switch; R9/TOOL20 adds real file semantics/admission/effects. Only universal patch/no-write-edit and related model-name selector prohibitions are superseded. T45 profiles/Plan/prompt and T44 VIS35/VIS36 consume minimal slices independently. No implementation PASS/status change, new task/framework/paid campaign or historical evidence rewrite; dirty R8 preserved.
- 2026-10-01: owner approves full child-TUI/Subagents/Shell/Terminals segment. Extend R2/TOOL13 with live inventory/output, targeted kill and same-process foreground conversion; historical initial-background PASS is not this new qualification. T56 separately owns session PTYs, T44/VIS39 owns paired Shell/Terminals presentation, T45 owns child lifecycle/session control orchestration. No duplicate owner/store/framework/paid campaign; current T50/T44 states and evidence preserved.
- 2026-09-27: owner approves selected tool parity and explicit websearch/Code Mode exclusions. Add one T50 in the existing progress engine; no new tracker or T44/T45 completion cycle. Narrowly supersede permanent session-Location binding with admitted move, preserve immutable execution and safety.
- Final implementation status: pending. This document records the frozen plan, not delivery of working tools or execution PASS.
