# Goal: Selected native tool parity with OC2

Status: active
Contract status means the owner-approved finish line is frozen; execution state belongs to progress/STATE.json. At the 2026-10-01 amendments T50 is active and T44 is PAUSED. Historical delivered slices do not qualify the new R2, model-dependent R1/R9 or tool-output R10 extensions.
Source: owner-approved tool RECON and plan, 2026-09-27: omit built-in websearch and Code Mode, accept the remaining proposed tool work, then update the plan and commit/push. Donor OC2 v2.0.12 at `2670273ff17da96f85c5826ced57aa1b368754fa`.
Additional source: owner request for OC2 TS edit/write for non-patch models, apply_patch for compatible GPT models, mandatory catalog removal/replacement after a user model switch, followed by detailed plan commit/push approval, 2026-10-01.
Additional source: owner requires OC2 TS parity when switching models during an active task, approves the follow-up RECON and detailed plan commit/push, 2026-10-01. This supersedes whole-turn model pinning, not immutable Location/config generation or execution authority.
Additional source: owner asks whether `opencode models` is planned for obtaining model IDs to bind agent profiles/other uses; after pinned-original comparison approves detailed work-plan changes and commit/push in the current branch, 2026-10-01. R7 adds the user CLI catalog consumer, not an automatic profile binder.
Additional source: owner requests comparison with original OC2 TS and our plans for tool-output line limits, separate filesystem logs with readable continuation, `opencode.jsonc` configuration and context-bomb protection; after read-only RECON approves the detailed work-plan amendment and commit/push in the current branch, 2026-10-01. R10/TOOL21 freezes that common pipeline; implementation is not requested by this plan delivery.
Last updated: 2026-10-01

## Objective

The selected Linux-native tool set matches the agreed OC2 capabilities through real model calls, application-owned outcomes and frontend consumers. The selected model receives apply_patch or edit/write using the exact donor predicate. A committed user model switch is accepted during busy and replaces incompatible definitions and runtime-managed guidance before the next request of the same autonomous task, without a new prompt. Picker draft is not commit; the prepared request and its tools retain their captured view. Large admitted tool text is bounded before model/history publication, retained in a registered filesystem artifact and explicitly readable in bounded pages; `tool_output` in `opencode.json/jsonc` controls line/byte previews without disabling safety caps. This is not full upstream parity: built-in websearch, Code Mode/execute, built-in browser and PDF remain excluded. Explicit MCP search/browser tools remain supported, not automatically connected or advertised.

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
  - R10 dependency supplement: shared stdout/stderr drains stream admitted text to the registered artifact before discard and retain bounded recent/tail output in RAM. Foreground, background, final notices and the existing output viewer consume the same capture identity/status; preview truncation is distinct from incomplete artifact capture and process exit. TOOL13 owns lifecycle/control proof; common configuration/artifact/continuation proof is TOOL21, not a second shell owner.
  - Primary evidence: TOOL13 actual binary with process/provider barriers proves early background return/progress/automatic notice, normal foreground waiting, default/explicit/zero timeouts, cancel/shutdown, crash/delivery and credential exclusion; reuse TOOL05/TOOL06/AUD27/AUD28 and A10 measurements, not duplicate helper-only suites.
  - Primary evidence supplement: actual-binary PTY and owner assertions prove authoritative running inventory, output while the process is held, same-PID foreground conversion, selected-job kill, final output/status after inventory removal, independent sibling progress and correct child/source routing through move/reopen/restart. T44/VIS39 reuses these facts for Shell rows/output dialog geometry and keys; TERM01/T56 interactive PTY is separate, not TOOL13 completion or a whole-task dependency.
  - Status: pending
  - Evidence:

- R3: grep and glob expose the agreed OC2 search options.
  - Source: approved search parity proposal; pinned grep/glob schemas and filesystem behavior.
  - Owner: T50, existing file/search executor and policy.
  - Acceptance: grep supports regex (default) and literal:true, path file/directory scope, include glob, caseSensitive (default true) and limit. Pin a vetted engine compatible with the donor ripgrep syntax; do not invent a custom matcher or promise unsupported regex features. glob supports path, hidden (default false), pattern and limit with donor matching/hidden/ignore behavior. Existing deterministic pagination may remain as an explicit extension, with coherent ordering/truncation diagnostics.
  - Acceptance: validate patterns/options before scanning; malformed regex and exhausted scan/result budgets are explicit outcomes. Preserve own-data-root exclusion except R10's exact registered-artifact read/search route, canonical admitted path boundaries, no-follow regular-file checks and bounded entries/bytes/time. External-directory behavior outside native trust admission remains a declared difference, not an implied access grant.
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
  - Acceptance: retain path/permission/symlink/own-data-root protections except R10's exact registered-artifact read/search route, byte/model budgets and bounded output. Successful project file/directory reads invoke the same admitted nested AGENTS lifecycle as T45/R10, with provenance and dedup; artifact reads are tool data and never discover AGENTS from the native data root. Image handling does not introduce a second instruction loader or wider filesystem trust.
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

- R7: Native model lookup/session rename are available to the model, and users can list exact model references with `oc models`.
  - Source: approved native model/session tools proposal; donor opencode tool plugin. Owner-approved 2026-10-01 CLI models/profile-binding follow-up and pinned CLI models handler add the user-facing catalog consumer.
  - Owner: T50 existing catalog and application/session/storage owners; T45/R6 owns profile binding/execution, T53 owns future Go/custom catalogs/credentials. Reuse minimal qualified seams, no circular whole-task completion dependency.
  - Acceptance: opencode_models supports query/provider/all/limit/offset and returns bounded grouped catalog metadata, variants and next page with donor newest-family and own-provider **model** ordering. Variant lists consume the shared [T47/VAR01 effective effort order](../CONTRACTS.md#canonical-effort-ordering--t47var01-approved-2026-09-27-pending), not a second lexical/source-order policy. This is a minimal catalog prerequisite, not whole-T47 completion. Unknown release/family/price metadata stays unknown; no hardcoded IDs, discovery changes, provider fallback or network/auth routing. Lookup does not switch the calling session's model.
  - Acceptance: opencode_session_rename accepts title and optional sessionID (current session by default), validates nonempty trimmed title and persists/publishes the real rename. Explicit targets must be locally known and authorized by the caller's effective session/tool policy; children cannot modify arbitrary parent/sibling/foreign sessions. Read operations and session-control mutations use the common policy pipeline, never authority inferred from descriptions. These native target-access ceilings are declared donor differences.
  - Acceptance supplement (2026-10-01): `oc models` lists exact provider-qualified IDs from all enabled supported admitted catalog sources, one per stdout line in deterministic lexical reference order. It needs neither `list` nor a default/session model, retains IDs containing additional slashes and does not collapse families or inherit lookup's default page limit. Empty healthy catalog is exit 0/empty stdout; safe diagnostics stay on stderr, fatal or incomplete required dynamic catalog/output failure is nonzero. Catalog-only config/trust/credential admission and the bounded discovery oracle remain; browsing metadata is not generation readiness. Full contract, errors, source comparison and ordered slices are below.
  - Primary evidence: TOOL18 captured direct schemas/results with static/dynamic/unknown metadata, paging and model retention; actual rename/reopen/restart and deny/foreign-target/no-effect assertions. No Code Mode or second catalog/store.
  - Primary evidence supplement: TOOL18 rebuilt debug/release subprocess stdout/stderr/exit and fake-service/effect counters for catalog-only listing, no-selection/static/dynamic/multi-provider/empty/failure cases and unchanged prefs/config/history. T45/R6 uses the captured listed reference in profile requests/reopen/restart; profile semantics stay T45-owned. Existing direct-tool evidence does not qualify the new CLI extension.
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

- R10: Configurable tool-text previews retain readable filesystem artifacts without context bombs.
  - Source: owner-requested OC2 TS comparison and approved RECON plan, 2026-10-01; pinned tool-output/config/runner/shell sources below.
  - Owner: T50 common result/config/runtime/storage/file/shell owners. TOOL21 is the one new common executable scenario; TOOL13 keeps shell lifecycle, TOOL16 keeps read/media, T45 keeps prompt/DCP/child authority and T44 keeps existing visual qualification. No new task or circular whole-task dependency.
  - Acceptance: typed `tool_output.max_lines`/`max_bytes` in admitted `opencode.json/jsonc` default to 2000/51200, positive integers only; the last source defining the section replaces the section wholesale, omitted fields use defaults. Preserve provenance, atomic reload and captured running generation; model switch is not config reload. Native served-output/model/transport/resource ceilings remain independent.
  - Acceptance: every local model-facing tool text result passes the common prepared-result boundary before provider input, durable new outcome/TurnLog and UI projection. General text keeps a bounded head, shell a bounded tail; line and UTF-8 byte limits both apply, producer `truncated` metadata is not a bypass. Preserve typed control results/valid call-result graph and separately admitted media; do not truncate JSON into invalid output or turn failure into success.
  - Acceptance: oversized admitted normalized/redacted text has one cold filesystem capture under `<native-data-root>/tool-output/` and a bounded hot preview/reference with actual path, counts, read/search guidance and truthful capture state. Full means the complete admitted text, not media, secrets or bytes rejected by upstream transport/input guards. Initial artifact ceiling16 MiB and existing shared quota default2 GiB are explicit native differences; cap/quota/IO/cancel/producer loss must say incomplete, never falsely full or fall back to unbounded inline output.
  - Acceptance: the existing Db/data-root owner registers session/operation/source-generation identity and crash-safe publication; bounded streaming/read/search never materializes the artifact or duplicates full new text in SQLite, TurnLog and resident history. Shell taps real drains before the old 1 MiB-per-stream discard; no post-hoc attempt to recover discarded bytes or tool rerun after logging failure.
  - Acceptance: model `read(path,offset,limit)` and `grep` on the exact registered artifact can find text beyond preview and the old ordinary-file 1 MiB ceiling. Session/authorized-lineage access, effective Deny/trust/provenance/no-follow/regular-file and scan/page/model caps remain; no broad native-root/blob/directory access or instruction discovery. References survive allowed model changes/restart/move without auto-loading payloads.
  - Acceptance: completed artifacts expire after7 days through the same owner, with active writer/reader leases protected, explicit expired/missing continuation and crash/orphan handling. This expirable resource class does not delete raw history or ordinary referenced blobs. Old raw records remain immutable; only universal full-inline publication of new oversized outputs is superseded.
  - Primary evidence: TOOL21 rebuilt debug/release binary with bounded fake provider executes a >1 MiB shell output, captures the next request's bounded preview/path, then actual read/grep → distant sentinel → useful continuation. Nearest source-derived config/line/byte/media fixtures and fault/restart/cancel/resource receipts are described in TEST_PLAN. Reuse TOOL13/TOOL16/AUD34/LOAD02/STORE04/A05/A10, retain historical ownership/evidence; helpers/UI SQLite continuation alone cannot qualify this path.
  - Status: pending
  - Evidence:

### Constraints and non-goals

- Rust 2024, Linux rootless, existing crate DAG/error/policy/ownership levels; no service, JS/Code Mode/plugin host, cloud scheduler, second executable registry or second history/archive.
- Model-dependent file-family exposure shares one native mutation/policy owner; explicit Deny and central/parent-child narrowing, data-root/source/path/symlink/CAS protection, immutable raw history, bounded queues/output/media and no unknown-effect replay remain. Do not remove tests or change A10 baselines to qualify the extension. Exact donor file-tool predicate is the only new model-name exception; production IDs/reasoning allowlists/provider routes remain forbidden.
- Excluded: built-in websearch/provider integrations, Code Mode/execute, built-in browser, PDF, LSP, filesystem snapshots/undo, arbitrary shell terminal manager within T50. No Formatter/LSP subsystem is introduced merely because donor mutation tools call it; this slice ports the agreed file schemas/semantics under existing native boundaries. Owner-approved [T56](2026-10-01-native-session-terminals.md) narrowly adds explicit session-local interactive PTYs; it does not turn shell into a terminal tool or permit a daemon/credential inheritance. Explicit external MCP tools remain opt-in and permission-gated.
- Historical audits/PASS and existing task statuses are not rewritten as implementation evidence. Native resource/trust/permission differences are explicit; no claim of identical donor internals or unlimited resources.
- R10 narrows only full-inline publication of new oversized text, discard-before-shell-capture and the exact registered-artifact read/search exception. It does not permit native-root glob/directory reads, arbitrary blobs, tool-output execution, a new archive/daemon/credential owner, disabled media/transport caps, history rewriting or unknown-effect replay.

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

## CLI models — R7/TOOL18 (approved 2026-10-01; pending)

### Source comparison and frozen behavior

At RECON HEAD `70de7a2568bd1d900dd81da6f4517402286d8413`, native `cli.rs::Command`
contains Run/Sessions/Tui, not Models. CONFIG's `oc models list [--refresh]` is a
projected spelling, not delivered behavior. R7 direct lookup/rename landed in
`0dc85e817`; its family reduction/default 20-row page cannot implement CLI listing.
The existing `CatalogSnapshot` is selected-provider scoped, and composition currently
requires a selected model before assembling that catalog. Do not fake a selection
or use a hidden prompt/session just to make listing possible. Preserve current dirty
live-switch/R8/CODE_MAP work; this amendment is plan-only.

- Pinned OC2 `commands.ts:283–286` registers `models` with ServerParams only;
  `handlers/models.ts:12–24` resolves the server, requests the current-directory
  model list, formats `providerID/id`, uses `localeCompare`, and writes lines plus a
  final EOL only for nonempty output. `server/handlers/model.ts:11–15` returns
  `Model.available()`; `core/model.ts` filters enabled models. This is not the OC1
  `models [provider] --verbose --refresh` interface or the opencode_models tool schema.
- Native command is **`oc models`**, without a required `list`. Retain existing
  applicable global native options such as `--data-dir`; do not introduce donor
  server/standalone flags, a daemon, positional provider, refresh, verbose, JSON,
  pagination or bind/config-writing commands in this slice. No compatibility alias
  is necessary for a never-implemented `models list` spelling.
- Print the complete enabled supported admitted ID set, `provider/model-id\n` per
  line. Use full exact references as identity; names/metadata/families never replace
  them. Sort deterministically by the full reference, independently of family or
  preferred provider. Donor uses locale collation; native stable lexical ordering
  is explicit for punctuation/non-ASCII differences, not a claim of identical locale
  internals. Do not add an ICU/JS host merely for sorting. Lookup's own-provider/
  newest-family order and T47's variant effort order are different consumers and
  remain unchanged. Stream a complete admitted snapshot under existing bounds; a
  catalog/output cap must fail visibly, not silently produce a successful prefix.
- Use the same admitted global/Location config sources, precedence, enabled/disabled
  provider rules, metadata merge and discovery module as current native consumers.
  Enabled here means effective admitted catalog inclusion, not successful paid
  inference. Static/public metadata can be inspected without a generation key;
  absent credentials do not discard independently available metadata. Dynamic
  authenticated OpenProxy discovery needs its own admitted key/binding. Do not
  resolve arbitrary unselected secrets/endpoints or claim unsupported provider
  protocols are ready merely because inert metadata exists. T53's admitted public
  Go/custom sources join this read-view only through their existing owner.
  Catalog-source admission selects only required source credentials; it is distinct
  from choosing a session/default model, and never authorizes a blanket provider scan.
- Listing works when no default model is selected and does not depend on a valid
  saved session/agent/model/variant. A valid-shaped unavailable selection is neither
  silently repaired nor executed. Mandatory malformed config/policy, source trust,
  endpoint/credential authority, storage ownership when needed and metadata caps
  remain real admission failures; catalog-only operation is not wider default policy.
- The command does not submit input, create a session, select a model/profile,
  dispatch generation/title/compaction, execute tools, connect MCP/browser or run
  recovery that mutates session operations. No config/profile/prefs/history writes.
  Reuse the config/catalog owners via a narrow catalog-only loading seam, not the
  full warm application startup. Only existing admitted catalog cache effects are
  allowed if its owner already provides them; no second cache/store/credential
  file or new service. If T53 needs stored credentials, use its same native owner
  and lock/authority rules rather than import or bypass a running data-root owner.

### Stdout, errors and readiness

| Outcome | stdout | stderr / exit |
| --- | --- | --- |
| Complete healthy snapshot | Exact sorted IDs, final newline for nonempty output | Safe optional warnings only; 0 |
| Complete healthy empty snapshot | Empty | 0; not ModelRequired or a fallback selection |
| Known static/public metadata, generation key absent | Known admitted IDs | Safe readiness warning where needed; 0 if no required catalog fetch failed |
| Required authenticated catalog cannot be fetched, timeout/auth/invalid discovery | Independently known admitted IDs may still be printed | Safe cause and action; nonzero, never claim complete/fresh metadata |
| Fatal source/policy/trust/storage admission or catalog capacity failure | Do not publish a candidate as complete | Safe structured cause; nonzero |
| Output write failure / cancellation | Possibly an already written prefix | Nonzero / interrupted result under existing CLI error conventions; no false success |

These native partial-list/error semantics are explicit, not a claim of donor exit
parity. Catalog visibility never clears provider auth failure, proves connection or
authorizes an otherwise refused request. Preserve discovery's all-or-nothing merge,
last-good/failure semantics, budgets and remote ID retirement; do not silently
resurrect removed entries from local overrides. Warnings and errors contain no
secrets, raw provider/config payloads or unsafe terminal text.

### Ordered implementation and consumer closure

1. **Freeze source fixtures and catalog-only admission.** Reuse config/composition/
   models and provider_catalog owners. Separate metadata access from mandatory
   default-model resolution and generation startup, retaining complete policy and
   source validation. Support enabled static metadata and bounded admitted ludka2
   discovery before any model choice; no fabricated ID or all-provider auth scan.
2. **CLI consumer.** Add Models to `oc/src/cli.rs`, dispatch/trace label in
   `bootstrap.rs`, and a small command consumer beside headless/TUI as appropriate.
   Read one immutable provider-qualified snapshot, emit only sorted exact lines,
   report safe causes through existing diagnostics, finish/join owned catalog work
   on failure/cancel. Extract only shared metadata projection needed by CLI/TUI/
   lookup; do not expose or call the private model-tool executor as a public CLI API.
3. **TOOL18 actual-binary qualification.** Offline fixtures cover static/dynamic/
   empty/disabled/multi-provider/catalog failure without a selected default. Captured
   stdout, stderr, exits, fake-service request counts and prefs/history/config facts
   prove the contract. Unknown model metadata stays unknown, exact IDs survive
   slashes and provider collisions, and there is no 20-row/family truncation.
4. **T45/R6 binding consumer.** A listed exact reference is used in canonical
   `agents.<id>.model`, compatible legacy `agent`, and global/project Markdown
   `.opencode/agents/<id>.md`. Pinned Model.Ref separates provider at the first `/`,
   preserves remaining slashes, and admits `#variant`/structured selection and
   separate legacy variant using donor precedence. Prove actual request identity
   and profile instructions, then reopen/restart; unavailable/retired choices remain
   explicit with pre-effect refusal. Reuse T45 profile tests, not a second executor/
   selector or duplicated gate. Other existing model settings consume the same
   reference grammar under their present scope, not new routing/binding commands.
5. **T53 catalog consumer and documentation.** Extend the same snapshot with public
   Go/custom metadata after T53's minimal qualified catalog/credential seam, without
   forcing basic OpenProxy CLI to wait for full T53 or its new wires/live campaign.
   Update supported help/CONFIG/run instructions with actual implementation. Binding
   examples are instructions for users, never automatic writes to their config.

TOOL18 remains solely T50-owned; T45 profile qualification uses its existing R6/
PRM01/A03/A13 evidence and references the CLI receipt. T53 keeps GO02/GO05, T47 VAR01,
T44 separate presentation; no new task/test ID or whole-task dependency cycle.
T50 remains active, T44 PAUSED; settle the current dirty slice and preserve T55's
next-safe-handoff priority. CLI implementation and new qualification are pending/
NOT_RUN, not PASS from approval or the historical direct-tool report.

## Tool output — R10/TOOL21 (approved 2026-10-01; pending)

### RECON: original behavior and native gap

Comparison starts from native HEAD `8a4291d13457a1641529dd6fa80d97d43d22066d`
with existing live-switch/runtime/TUI/R8/CODE_MAP work dirty, not a clean runtime
qualification. Donor remains OC2 v2.0.12 at the pinned commit above. RECON was
read-only: no builds, tests, live requests, secret/config extraction or source edits.

| Surface | Pinned OC2 TS | Native source at RECON / missing work |
| --- | --- | --- |
| Common text limiter | `tool-output.ts`: 2000 lines / 50×1024 bytes, head preview, full joined text file and path marker before tool-result publication | `runtime/turn.rs` puts ordinary full String into FunctionCallOutput/TurnLog; 2048-byte report/UI preview is a different boundary |
| Configuration | root `tool_output`, optional positive `max_lines`/`max_bytes`; `Config.latest` selects the last complete section, config plugin updates limiter | `config.rs::Generation`/loader do not parse the section; documented TOML safety proposals are not implemented tool-output config |
| Shell | real combined-output stream writes a file; result keeps tail using the same configured limits; live cursor and7-day cleanup | `shell.rs::spawn_drain` keeps first1 MiB per stdout/stderr, drains/discards the rest; `ShellOutcome` cannot reconstruct lost data. Background shares the same loss |
| Continuation | filesystem path can be read with paged `read`; artifact access goes through file policy | `storage.rs::read_tool_op_output`/`read_session_tool_output` page SQLite output for application/UI only. Ordinary `files/read.rs` rejects native data root and first materializes a ≤1 MiB file |
| Storage/context guards | general limiter has TTL but no dedicated artifact quota; media excluded from text count | active-context16 MiB, request32 MiB, media/MCP/fetch/file caps and Db blob quota exist. They refuse/limit inputs but do not provide the common spill/read chain |

Original is not a perfect security contract: `metadata.truncated !== undefined`
skips its common limiter even for false, and read/grep/glob/shell set that field.
Native must preserve useful parity, **not this bypass**. Donor counts only text
parts joined with newline, preserves file parts, removes a synthetic terminal empty
line, and may keep no body for one overlong line. Donor artifact write failure is
a defect; native follows its typed failure/effect model rather than crashing or
republishing the oversized body. Media and actual transport admission stay separate.

### Frozen configuration and counting

- Only root `tool_output: {max_lines?, max_bytes?}` is added to upstream config;
  no per-tool/per-agent map, implicit file, automatic config write or executable sample
  change in this delivery. JSON/JSONC use existing admitted source order. A later
  `{max_lines: 40}` replaces an earlier `{max_bytes: 4096}`: effective40/51200,
  not40/4096. `{}` restores defaults. Missing section uses2000/51200.
- Reject zero, negative, fractional, string, overflow and invalid section shapes with
  safe source/field diagnostics; failed reload retains the whole previous generation.
  Byte values above the native64 KiB served-text ceiling cannot raise that ceiling
  and are rejected explicitly. Positive line values never disable the independent
  byte ceiling; arithmetic/counting/serialization is checked and bounded.
- Limits count UTF-8 bytes and logical lines of admitted text, not characters/tokens
  or media/base64. Join multiple text parts with newline under one shared budget;
  trailing newline does not create an extra empty line. Keep whole lines when they
  fit, clip a lone oversized line only on a UTF-8 boundary with an explicit marker.
  General results keep head, shell keeps recent tail; expose which end was kept.
- Configured limits bound preview **body**. Artifact/ref/count/status notice has its
  own bounded reserved allowance; total served text including notice is ≤64 KiB and
  still passes selected model/request admission. Tiny positive limits may yield empty
  body plus a useful bounded reference, never an unbounded marker or output fallback.
  Further native truncation reports its effective reason, not a falsely exact donor
  truncation count. Producer loss and preview loss are distinct facts.
- Published config updates affect only newly admitted turns/jobs under the existing
  safe generation boundary. An executing prepared result/background job uses captured
  limits/source generation; model/variant switch does not reread config or re-limit
  historical results. Reopened presentation uses recorded facts, not today's defaults.

### Result, artifact and access lifecycle

```text
confirmed tool execution / owned shell drain
  -> admitted normalization + redaction + bounded capture
  -> prepared result {bounded text, typed controls/media, artifact reference/status}
  -> one durable outcome + bounded TurnLog/provider projection + UI pages
  -> explicit authorized read/grep artifact -> bounded result -> next request
```

- Common preparation owns text projection for built-ins, admitted MCP text/structured
  text, webfetch and skill bodies; producer `truncated:true/false` cannot skip it.
  Preserve error category/isError/failed/partial/unknown and typed question/compress/
  session-control/mutation metadata. Bound free text around those envelopes without
  slicing serialized JSON or erasing confirmed effects. Existing media modality,
  per-part/count/wire/model budgets still apply and cannot be bypassed via a file ref.
  A saved admitted skill body is data; reading it does not execute/load another skill.
- Keep small results inline unchanged. For new oversized text retain one cold copy
  with small hot preview/reference in operation, TurnLog, provider and presentation;
  do not also retain the full payload as SQLite text, serialized log and long-lived
  String copies. Old full-inline records are not migrated/truncated/deleted. Existing
  UI continuation APIs consume legacy inline output or stream new artifacts through
  the same storage owner; no second UI archive or whole-file `read_blob` fallback.
  Artifact-backed read/search pages reuse their validated source/extent reference and
  recompute common preview caps/next cursor; do not duplicate the same cold payload
  into recursive artifacts simply because a page exceeded a lower preview setting.
- Use the existing exclusive Db/data-root owner and quota accounting, including
  reservations/temp/orphan bytes and concurrent writers. Store exact generated path,
  artifact identity, operation/session/Location/generation, admitted text byte/line
  facts and capture state. Generated opaque names are not authority. Producer streams
  normalize/redact incrementally before disk and hot publication, including secrets
  split across chunks; counts/full-capture claims refer to that admitted text.
- Initial artifact hard ceiling is16 MiB; shared existing storage quota defaults2 GiB,
  not an additional2 GiB pool. Defaults are safety choices, not measured A10 budgets
  or new `tool_output` retention/quota keys. Shell streams from launch; generic bounded
  nonstreaming results spill when oversized. Record stdout/stderr provenance and the
  combined observed capture order without inventing a total OS emission order.
- Register active captures and publish readable extents only after the matching bytes
  exist. Completed publication uses existing temp/no-follow/fsync/atomic-rename/
  directory-sync principles and durable registration. A committed result cannot
  promise a full unregistered/missing file. Live reads see a stable published extent;
  completion final-flush/path identity never duplicates the capture at Ctrl+B.
- Distinguish complete capture, producer-limited input, artifact cap/quota loss,
  IO/registration failure and interrupted capture. Where a readable prefix exists,
  return its exact extent/state; otherwise do not advertise a usable full path.
  Preview truncation alone does not mean incomplete capture. Logging trouble never
  retries a shell/MCP/mutation, falsifies known process exit/effects or claims full
  output; persist actual effects and capture failure separately. Storage/cleanup
  failures retain native non-success semantics, never an unlimited inline fallback.
- Shell artifact writes/queues and tail ring remain bounded under flood/slow disk.
  After capture cap/quota/IO failure keep draining without retention, maintain bounded
  recent output and honest loss counters/state, and preserve deadline/cancel/reap.
  Foreground/background/notice/viewer use the same stream/capture owner and source
  context. A producer that never ends cannot require unlimited disk or memory.
- `read`/`grep` admit **only an exact registered regular artifact file**, not its
  directory, all `tool-output/*`, arbitrary `blobs/*`, SQLite/WAL, configs or another
  session's guessed path. Verify session or already authorized owned lineage and
  original operation provenance plus effective current permissions/Deny; child refs,
  forked text and moved placement do not confer new authority. Keep descriptor-relative
  no-follow/type/identity checks and symlink/swap protection. Ordinary project paths,
  glob and all mutations keep the original data-root denial.
- Artifact continuation scans/reads incrementally beyond1 MiB with bounded buffers,
  line offsets/pages, huge-line clipping, explicit next cursor and scan/time/output
  budgets. No eager full-file load. Reads/searches are ordinary limited tool results;
  continuation never restores the whole cold capture to hot context. The text is
  untrusted tool data, never a discovered AGENTS/system instruction source.
- Completed artifacts expire7 days after completion through bounded owner cleanup.
  Protect active writer/reader leases; crash abandons an active capture as interrupted/
  unknown under existing recovery, never auto-restarts its producer. TTL is separate
  from ordinary referenced-blob GC: expire only this resource class, keep immutable
  raw outcome/ref and report expired/missing on explicit access. Quota pressure may
  clean already expired/unleased resources, not silently delete unexpired captures
  or normal referenced blobs. Orphan/temp accounting is crash-safe under STORE04.
- Restart, same-task model switch, allowed move, DCP and `/compact` retain bounded
  causal results/references as selected by the current projection. None automatically
  reads/reinflates an artifact or forgotten archive; expired access is actionable.

### Ordered implementation slices and qualification

1. **Freeze typed seams/fixtures.** At one reviewed code base trace each local result
   to runtime publication, add minimal prepared-result/capture descriptors through
   existing owners, and freeze donor counting/config fixtures plus a >1 MiB distant
   sentinel workload. Do not add schemas/UI paths before the storage/read route exists.
2. **Config generation.** Parse the two fields in `config.rs::Generation` with
   whole-section replacement/defaults/provenance/validation; prove atomic reload and
   captured running limits. No per-agent layer, new config daemon or secret dump.
3. **Common preparation.** Insert one limiter before provider/outcome/TurnLog cloning,
   share text/head/tail counting, reserve bounded notices and preserve typed controls/
   media/errors. Test multiple text parts, UTF-8 and producer-metadata non-bypass.
4. **Registered filesystem capture.** Extend existing `storage.rs`/blob ownership
   minimally with streaming/reservation/publication/read/expiry and session/op identity.
   Add only the needed native schema migration; preserve legacy inline reads/history
   and ordinary blob guarantees. Fault-check quota/IO/crash/orphans before advertising
   a full artifact. Common result/storage slices publish together, not a broken prefix.
5. **Shell producer integration.** Tap `shell.rs::spawn_drain` before discard, replace
   growing first-prefix vectors with bounded recent/tail state, stream through the
   same registered capture for foreground/background/live viewers/notices/Ctrl+B.
   TOOL13 barriers prove same-process control/final flush; incomplete capture remains
   separate from exit/timeout/cancel. Never rerun execution to obtain a log.
6. **Exact-artifact read/search.** Extend file admission/resource/policy and read/grep
   streaming only for registered artifacts, preserve normal data-root denial and
   no-follow/session/lineage constraints. Reuse TOOL16 media/project reads and T45
   AGENTS regressions; no generic external-directory grant or instruction loader.
7. **TOOL21 closure.** Rebuilt debug/release actual binary: large tool output → captured
   next-request bounded preview/path → real read/grep → distant sentinel → continuation;
   restart/model-switch/compact do not rehydrate or replay. Share TOOL13/TOOL16/AUD34/
   LOAD02/STORE04 resource/fault receipts and required affected/integration/final gates.
   Update current CONFIG/TOOLS_MCP/help/examples only with delivered support. No paid
   campaign or per-layer duplicate scenario; plan validation is not runtime PASS.

Scheduling: preserve and settle the current dirty T50 slice first and keep T55's
next-safe-handoff priority. When T50 reaches R10, the shared config/result/capture
seams precede shell/read integration and common qualification; they are prerequisites
for those slices, not whole T45/T44/T46/T56 completion. T44 remains PAUSED and owns
existing presentation gates separately. R10 is pending/NOT_RUN; historical T40/AUD34
and initial shell retention evidence do not prove the new filesystem/provider path.

## Change Envelope

- Reuse `crates/oc-adapters/src/{files,tools,shell,webfetch,runtime,application,composition,config,storage}.rs`, `oc-core` typed commands/queries/results, existing provider result lowering and `oc`/`oc-tui` consumers; targeted tests/fixtures/docs/evidence. Changes to internal typed media/job/move storage are allowed only for these outcomes.
- R1/R9 additionally touch `runtime.rs::builtin_tool_defs`, `runtime/turn.rs` early/per-request assembly and captured execution, `models.rs` selected ID/budget, `provider.rs` schema-based cache inputs, `runtime/context.rs::wire_history`/protection extraction and existing compaction callers. R1 live switching includes `application_selection.rs`, `application.rs` SessionSelection/SelectModel owner guards, existing storage selection/event/request-log transactions and `oc-core` typed snapshot/event identity; `oc/src/tui_cmd.rs` and `oc-tui` composer draft/capture/blank-Enter/reconciliation are direct consumers. Minimal internal identity/schema migration is allowed only if existing records cannot truthfully represent multiple request models in one turn. Use one selection/mutation/context owner, no global registry rewrite, provider-routing redesign or new store/cache.
- R9 extends `patch.rs` and `patch/{fs,effects}.rs` through a coarse private shared mutation seam, `tools.rs` registry/validation/resources/dispatch, `approval.rs` preview/grant/preimage preparation, `permissions.rs`/`config.rs` path normalization and `dcp.rs`/runtime mutation protections. Existing invocation permits/durable tool effects and core FileEffect/PatchEffects payloads may extend minimally; do not create a second executable registry, generic framework, new crate or public API solely for tests. Add substantial tests beside their owner in separate test modules and retain existing Cargo targets.
- Direct presentation consumers are `oc-tui/src/tools.rs`, `app/live.rs` and existing history projection/results; T50 supplies persisted write/edit input/results/effects, T44 extends VIS35/VIS36 actual cards/previews. Update CODE_MAP/fixture paths with the implementation only; this plan leaves unrelated dirty session_move/CODE_MAP work untouched.
- A vetted regex/search dependency or existing ripgrep integration and a bounded Markdown converter may be chosen for R3/R6; pin/provenance/compile checks are required. No dependency is added by this plan-only delivery.
- R7 CLI adds `oc/src/{cli,bootstrap}.rs` and a narrow command consumer, existing config/composition/catalog/metadata projection seams and actual-binary tests. Minimal provider-qualified read-only DTOs may extend the existing core query surface only as needed by real consumers; no full application/session startup, public API solely for tests, second catalog/store/cache, credential import or automatic config writer. T45/R6 retains binding normalization/selection ownership; T53 supplies its future catalog through the same seam.
- R10 extends existing `config.rs::Generation`/loader, `tools.rs`/result and `runtime/{turn,context}.rs` publication, `storage.rs`/`storage_shell_jobs.rs`, `shell.rs`/`shell/jobs.rs` drains, `files.rs`/`files/read.rs`/search and resource/permission preparation. Typed core/application/UI continuation facts may extend minimally for actual consumers; media/MCP/fetch/skill lowering must use the same common preparation. Internal native schema migration/streaming artifact resources are allowed only under the existing Db owner, not a new store/archive/framework/crate or public test API. Tests remain beside owners/in existing runtime/storage/binary targets; CODE_MAP updates follow implementation, not this plan.
- No private authoring config, secrets, deployment changes, paid search provisioning or executable runtime/config example edits in this plan update. Ordinary own-branch commit/push is required.

## Current Checkpoint and State

- At this plan amendment T50 is active and T44 PAUSED. The current NOW checkpoint reports the R9 executors/captured file-family slice verified and live busy switching next; Git HEAD includes initial file executors at0804b7dbb, with the live-switch/runtime/TUI/R8/CODE_MAP work still dirty. This is existing handoff evidence, not fresh qualification of that code or R10. Plan delivery does not alter execution statuses, historical leaves or evidence.
- R7 lookup/rename is committed at 0dc85e817. Initial file-tools RECON was at 3446dc3c1; that plan landed in 760d54f7d. Live-switch RECON/this clarification starts from 760d54f7d with R8 session_move/CODE_MAP/runtime/evidence dirty. Git takes precedence over the older R6 handoff. Preserve all dirty work and settle its current slice before scheduling this extension. R2 inventory/live-output/foreground-conversion and R1/R9 file-tools/live-switch extensions remain pending; follow M8 ordering without a second active task or interruption of approved T55 priority. Neither historical file-tools plan nor existing busy-refusal tests qualifies corrected same-task switching.
- Checks: TOOL12–TOOL21 have one owner T50; relevant A02/A03/A04/A05/A06/A07/A08/A10/A13 and prior scenarios are regressions, not reassigned owners. TOOL12/TOOL20 and T44 VIS35/VIS36 are separate behavioral/visual results; existing A09/live owners/flow remain. Plan validation is not runtime PASS.
- Blocker: none for this plan delivery; PDF is deliberately excluded, not a hidden pending requirement.
- CLI follow-up (2026-10-01): R7 direct lookup/rename evidence is historical delivered behavior; newly approved `oc models`/catalog-only extension is NOT_STARTED/NOT_RUN. Missing CLI/default-dependent composition and differing tool paging are source-verified gaps at RECON HEAD 70de7a256. Profile consumer belongs to T45/R6, future Go/custom catalogs to T53; dirty runtime/TUI/CODE_MAP and execution statuses remain untouched by this plan.
- Tool-output follow-up (2026-10-01): R10/TOOL21 is frozen/pending; native source gaps are recorded above at HEAD8a4291d13. This delivery changes plan/registry/generated NOW only, not runtime, executable configs, historical leaves/evidence, active T50 or PAUSED T44. Minimum next R10 action after the safe scheduled handoff is typed common-result/config/capture fixtures; missing producer capture and exact artifact read are implementation work, not an external blocker or existing PASS.

## Pinned References

All links point to the admitted OC2 v2.0.12 commit; current native restrictions are compared explicitly rather than copied from V1 docs.

- [Registration and scope](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/plugin/internal.ts#L208-L242), [direct/CodeMode snapshots](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool.ts#L225-L262).
- [Shell schema/defaults](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/shell.ts#L22-L73), [owned execution/background/notifications](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/shell.ts#L153-L278), [source shell after movement](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/shell.ts#L10-L26).
- [Grep options](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/grep.ts#L17-L38), [glob options](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/glob.ts#L17-L28), [filesystem search](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/filesystem.ts).
- [Question schema and Form lifecycle](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/question.ts#L12-L127), [typed question prompts](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/schema/src/question.ts), [actual TUI forms](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/form.tsx).
- [Read schema/media/nested AGENTS](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/read.ts#L17-L113), [read filesystem](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/read-filesystem.ts).
- [Webfetch formats/defaults/conversion](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/webfetch.ts#L12-L100), [Markdown converter](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/html-markdown.ts).
- [Models/rename/move tools](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/opencode.ts#L11-L197), [move validation/admission](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/move.ts#L74-L152), [same-session placement projection](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/projector.ts#L466-L478).
- [CLI models registration](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/cli/src/commands/commands.ts#L283-L286), [handler/reference lines/order/empty output](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/cli/src/commands/handlers/models.ts#L9-L24), [available endpoint](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/server/src/handlers/model.ts#L11-L15), [enabled model view](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/model.ts#L208-L250).
- [Exact Model.Ref grammar](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/schema/src/model.ts#L18-L40), [canonical/legacy agent normalization](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/normalize.ts#L131-L164), [Markdown model/variant precedence](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/plugin/agent.ts#L177-L211), [profile model-reference regressions](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/config/agent.test.ts#L273-L326).
- U90 — [Exact file-tools selector, context/compaction/generate hooks](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/patch.ts#L296-L309); U91 — [edit schema/matching/preparation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/edit.ts#L24-L213); U92 — [write schema/preview/create/overwrite](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool/plugin/write.ts#L23-L91).
- U93 — [Availability-conditioned write/edit guidance](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/system-prompt.ts#L9-L27); U94 — [Write presentation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/index.tsx#L3044-L3078) and [Edit presentation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/routes/session/index.tsx#L3324-L3386). IDs append to existing tui-recovery/SOURCES.json, without renumbering historical sources.
- [Provider lowering/model metadata scope](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/to-llm-message.ts#L153-L247) preserves historical call/result pairs; selector removes current tools, not old raw records. Input repair is schema repair, not cross-tool history translation.
- U95 — [Picker changes local draft](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/component/dialog-model.tsx#L126-L139); draft scope/reconciliation: [local selection](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/context/local.tsx#L263-L381). U96 — [Commit/blank Enter/captured prompt preparation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/src/component/prompt/index.tsx#L1274-L1360).
- U97 — [Busy-allowed model event/no-op](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/session.ts#L90-L102); [API forwards without busy guard](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/server/src/handlers/session.ts#L246-L253). U98 — [Boundary session/tool/model snapshot](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/context.ts#L121-L175); U99 — [Next-step/retry/compaction request preparation](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/llm.ts#L187-L247).
- U100 — [Blocked-tool switch and same-run continuation regression](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/session-runner.test.ts#L3386-L3407); overflow rebuild: [3022–3076](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/session-runner.test.ts#L3022-L3076). U101 — [TUI pending-admission/captured selection order](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/tui/test/compact-admission.test.tsx#L193-L226); U102 — [Compatible historical tool pairs/opaque metadata](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/to-llm-message.ts#L153-L256).
- [Common limiter/defaults/file/TTL and metadata skip](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/tool-output.ts#L13-L153), [runner publication boundary](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/session/runner/step.ts#L117-L129), [positive config schema](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/schema/src/config/tool-output.ts#L3-L9), [whole-section selection](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config.ts#L22-L24), [config update plugin](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/config/plugin/tool-output.ts#L9-L22).
- [Shell file/tail/live capture](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/src/shell.ts#L196-L335), [limiter/full-file/count/media/cleanup fixtures](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/tool-output.test.ts#L34-L153), [config reload fixtures](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/config/tool-output.test.ts#L17-L63), [shell limits/file fixtures](https://github.com/anomalyco/opencode/blob/2670273ff17da96f85c5826ced57aa1b368754fa/packages/core/test/tool-shell.test.ts#L1326-L1381). These R10 references do not add visual source IDs or repin the donor.

## Material Decision and Completion

- 2026-10-01: owner approves the detailed output RECON plan and commit/push in the current branch. Add R10/TOOL21 to existing T50: configurable2000-line/51200-byte common previews, registered streaming filesystem capture/read-search continuation, native16 MiB/shared2 GiB/7-day lifecycle and honest incomplete states. Supersede only full-inline publication of new oversized text, shell discard-before-capture and exact-artifact data-root read/search exclusion; preserve old raw history, permissions/redaction/media/transport/bounds/no replay and ordinary referenced blobs. TOOL13/TOOL16 and AUD34/LOAD02/STORE04 remain their original owners; no new task/store/paid campaign, T44 resume or runtime/status/PASS change. Current dirty slice and T55 priority preserved.
- 2026-10-01: owner approves detailed CLI models/profile-binding work-plan delivery and commit/push on the current branch after comparison with pinned OC2. Extend R7/TOOL18 with `oc models`, exact complete ID stdout and selection-independent catalog-only admission. Supersede only projected models list/refresh spelling, preserve lookup schemas/paging and config/policy/discovery/credential contracts. T45/R6 verifies real bindings, T53 supplies future catalogs; no new tasks/gates/store/server/config writer or execution/PASS changes. Existing dirty code and T55 handoff priority are preserved.
- 2026-10-01: follow-up owner instruction requires original OC2 switching during active work and detailed plan commit/push. Frozen boundary becomes a prepared request plus its tools, not the entire native turn. R1/TOOL12 now includes busy-allowed committed selection, local draft/captured commit/blank Enter, next-request adoption within the same task, retained compatible outcomes and real request/assistant attribution. Only model/variant busy refusal, whole-turn model pin and incompatible result-losing projection are superseded; config/Location generations, authority, policy, finite retry and no unknown-effect replay remain. T45/PRM01 and T44 VIS09/VIS29/VIS17/VIS35/VIS36 consume minimal slices; ownership/statuses/history/R8/T55 schedule unchanged, qualification pending.
- 2026-10-01: owner approves RECON plan and requires detailed work-plan commit/push in the current branch. R1 now selects apply_patch or edit/write using exact donor predicate and obligatorily refreshes next-request tools/managed guidance after a user model switch; R9/TOOL20 adds real file semantics/admission/effects. Only universal patch/no-write-edit and related model-name selector prohibitions are superseded. T45 profiles/Plan/prompt and T44 VIS35/VIS36 consume minimal slices independently. No implementation PASS/status change, new task/framework/paid campaign or historical evidence rewrite; dirty R8 preserved.
- 2026-10-01: owner approves full child-TUI/Subagents/Shell/Terminals segment. Extend R2/TOOL13 with live inventory/output, targeted kill and same-process foreground conversion; historical initial-background PASS is not this new qualification. T56 separately owns session PTYs, T44/VIS39 owns paired Shell/Terminals presentation, T45 owns child lifecycle/session control orchestration. No duplicate owner/store/framework/paid campaign; current T50/T44 states and evidence preserved.
- 2026-09-27: owner approves selected tool parity and explicit websearch/Code Mode exclusions. Add one T50 in the existing progress engine; no new tracker or T44/T45 completion cycle. Narrowly supersede permanent session-Location binding with admitted move, preserve immutable execution and safety.
- Final implementation status: pending. This document records the frozen plan, not delivery of working tools or execution PASS.
