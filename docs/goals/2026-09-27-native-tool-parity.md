# Goal: Selected native tool parity with OC2

Status: active
Contract status means the owner-approved finish line is frozen, not that T50 is executing. Implementation is pending; T50 remains todo and only T44 is active in the progress engine.
Source: owner-approved tool RECON and plan, 2026-09-27: omit built-in websearch and Code Mode, accept the remaining proposed tool work, then update the plan and commit/push. Donor OC2 v2.0.12 at `2670273ff17da96f85c5826ced57aa1b368754fa`.
Last updated: 2026-09-27

## Objective

The selected Linux-native tool set matches the agreed OC2 capabilities through real model calls, application-owned outcomes and frontend consumers. This is not full upstream parity: apply_patch replaces write/edit; built-in websearch, Code Mode/execute, built-in browser and PDF remain excluded. Explicit MCP search/browser tools remain supported, not automatically connected or advertised.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: The advertised tool set and guidance reflect the approved scope.
  - Source: owner explicitly excludes websearch and Code Mode and approves the remaining tool proposal.
  - Owner: T50; T45 retains profiles/prompt/subagents/DCP, T46 retains MCP parity, T44 retains visual qualification. No circular completion dependency.
  - Acceptance: canonical direct tools are read, glob, grep, apply_patch, shell, webfetch, skill, question, subagent, compress, opencode_models, opencode_session_rename and opencode_session_move, subject to effective policy/config/capabilities. Do not register a built-in websearch/provider selector, Exa/Tavily integration, execute interpreter or built-in browser. Advertise configured MCP tools only from the actual admitted catalog; absence of built-in websearch is not a ban on MCP search tools.
  - Acceptance: canonical shell uses the OC2 command schema. Preserve existing bash(argv/cwd/timeout_ms) compatibility through the same executor/admission owner; normalize permission identities without bypassing legacy Deny, saved-grant scope or structural ceilings. Do not advertise two interchangeable shell tools or silently reinterpret argv as command text. Model-independent names/descriptions/schemas and root/child capability previews stay truthful; Explore remains read-only with its separately approved own-history compress grant.
  - Primary evidence: TOOL12 captured actual-binary root/child requests and dispatch assertions with allow/ask/deny, aliases and explicit MCP search; existing A05/A06/A13 and T45 prompt/profile regressions remain.
  - Status: pending
  - Evidence:

- R2: Linux shell supports real command and owned foreground/background lifecycles.
  - Source: approved Linux shell parity proposal; donor shell schema and job/notification implementation.
  - Owner: T50, existing shell supervisor and session/runtime ownership.
  - Acceptance: command runs in the actual selected/configured Linux shell; support workdir and timeout in milliseconds. Foreground is default with donor 120000 ms default; explicit timeout:0 disables execution timeout. background:true returns running/shellID after durable admission and actual launch; its default has no execution timeout, while an explicit timeout is honored. Zero/unset timeout never removes bounded output, queues, cancellation or teardown limits. Native trusted-workdir, minimal credential-free child environment and resource rules remain explicit differences.
  - Acceptance: background progresses while the calling session continues; exactly one durable terminal notice is delivered per job/delivery identity without model polling, even if the session is idle/busy or has moved. This is delivery deduplication, not exactly-once external execution. Capture bounded stdout/stderr/exit/signal/timeout/truncation and readable retained output; cancel/shutdown cleans owned process groups with TERM/KILL/wait/reap. Restart delivers committed results without re-execution; started/unknown shell effects are never auto-replayed or reported completed without evidence. A finished foreground response does not silently cancel admitted background work.
  - Primary evidence: TOOL13 actual binary with process/provider barriers proves early background return/progress/automatic notice, normal foreground waiting, default/explicit/zero timeouts, cancel/shutdown, crash/delivery and credential exclusion; reuse TOOL05/TOOL06/AUD27/AUD28 and A10 measurements, not duplicate helper-only suites.
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
  - Owner: T50 end-to-end typed tool/application/frontend behavior; coordinate rendering with T44 without changing VIS36 ownership.
  - Acceptance: nonempty questions, headers, options, multiple selection and automatically available free-form answer follow donor schema. Model execution waits for typed user answers; TUI presents the actual pending question and submits answers, with deterministic question order. Dismissal/cancel interrupts the relevant execution instead of inventing an answer or converting it to success. No DB transaction is held while waiting; bounded pending forms/replies are tied to operation/session/generation and reject stale/duplicate/foreign replies.
  - Acceptance: headless without a question consumer returns actionable non-success and preserves history; it does not hang indefinitely. Permission autoaccept/--auto never fabricates user answers. Effective question Deny and General/Explore restrictions remain; reopen/restart cannot duplicate an answered form or reinterpret an unanswered form as approval.
  - Primary evidence: TOOL15 scripted model calls and actual PTY answers/multiple/free-form/dismiss/cancel, headless/--auto and restart assertions; real application state/result and call/result graph, not a screenshot-only question widget.
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
  - Acceptance: opencode_models supports query/provider/all/limit/offset and returns bounded grouped catalog metadata, variants and next page with donor newest-family and own-provider ordering. Unknown release/family/price metadata stays unknown; no hardcoded IDs, discovery changes, provider fallback or network/auth routing. Lookup does not switch the calling session's model.
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
  - Acceptance: ordinary SelectLocationSession/UI Location switching still selects/creates a target-scoped session. This distinct explicit move supersedes only the permanent-binding prohibition, not immutable-turn/trust/session-access guards; histories/reopen/UI metadata reflect actual placement and original operation provenance.
  - Primary evidence: TOOL19 provider/process barriers and actual binary A→B move preserve ID/history and show source/destination requests, blocked destination-dependent batch assumptions, invalid/untrusted/stale/no-effect cases, original background completion, child placement, quarantine and crash/restart around admission/apply. AUD14 is amended narrowly; existing UI06/A13/PRM01/CTX01/SUB02 and cleanup gates remain.
  - Status: pending
  - Evidence:

### Constraints and non-goals

- Rust 2024, Linux rootless, existing crate DAG/error/policy/ownership levels; no service, JS/Code Mode/plugin host, cloud scheduler, second executable registry or second history/archive.
- Single native apply_patch contract, explicit Deny and central/parent-child narrowing, data-root/source/path/symlink/CAS protection, immutable raw history, bounded queues/output/media and no unknown-effect replay remain. Do not remove tests or change A10 baselines to qualify the extension.
- Excluded: write/edit built-ins, built-in websearch/provider integrations, Code Mode/execute, built-in browser, PDF, LSP, filesystem snapshots/undo, arbitrary shell terminal manager. Explicit external MCP tools remain opt-in and permission-gated.
- Historical audits/PASS and existing task statuses are not rewritten as implementation evidence. Native resource/trust/permission differences are explicit; no claim of identical donor internals or unlimited resources.

## Change Envelope

- Reuse `crates/oc-adapters/src/{files,tools,shell,webfetch,runtime,application,composition,config,storage}.rs`, `oc-core` typed commands/queries/results, existing provider result lowering and `oc`/`oc-tui` consumers; targeted tests/fixtures/docs/evidence. Changes to internal typed media/job/move storage are allowed only for these outcomes.
- A vetted regex/search dependency or existing ripgrep integration and a bounded Markdown converter may be chosen for R3/R6; pin/provenance/compile checks are required. No dependency is added by this plan-only delivery.
- No private authoring config, secrets, deployment changes, paid search provisioning or executable runtime/config example edits in this plan update. Ordinary own-branch commit/push is required.

## Current Checkpoint and State

- T50 is planned/todo, no implementation evidence. T44 remains active; existing T45/T46/T47/T49 work and statuses are unchanged.
- Next when T50 starts: choose the first minimal R1/R2 alias/schema or R3 search slice in M8, freeze its direct fixture/captured-request evidence, then implement and qualify. Do not batch all eight outcomes into one rewrite.
- Checks: new TOOL12–TOOL19 have one owner T50; relevant A02/A03/A04/A05/A06/A08/A10/A13 and prior scenarios are regressions, not reassigned owners. Plan validation is not runtime PASS.
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

## Material Decision and Completion

- 2026-09-27: owner approves selected tool parity and explicit websearch/Code Mode exclusions. Add one T50 in the existing progress engine; no new tracker or T44/T45 completion cycle. Narrowly supersede permanent session-Location binding with admitted move, preserve immutable execution and safety.
- Final implementation status: pending. This document records the frozen plan, not delivery of working tools or execution PASS.
