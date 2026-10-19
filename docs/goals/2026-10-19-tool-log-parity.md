# Goal: exploration and Execute tool-log parity

Status: active
Source: owner screenshots, five independent plan audits and corrected plan accepted
by «делай копию плана в цель и итеративно реализовать и коммит пуш».
Last updated: 2026-10-19

## Objective

Adapt the missing `fix/tui-exploration-group` behavior onto native `master` and
deliver the requested exploration/Code Mode display and interactions. Preserve
newer canonical-history, permission, mouse and resource guarantees. Commit and
push verified increments to the verified origin without force.

## Execution Directive

Complete the frozen Required Outcomes using the Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks or optional source text. Stop when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: collapsed exploration and truthful direct-tool presentation.
  - Source: owner screenshots and corrected plan A.
  - Acceptance: adjacent typed Read/Glob/Grep/WebFetch, including terminal failure,
    denial, cancellation and limited results, initially hide under a muted
    `Explored:`/`Exploring:` header. First-seen categories and plurals are correct.
    Header clicks toggle original adjacent member rows; errors have independent
    disclosure. Direct WebFetch is `% WebFetch URL`, not a Code Mode fetch.
    Named inline tools have their OC2 glyph/muted style; Skill stays standalone.
    Read ranges use valid explicit recorded input, never output-prose inference.
  - Primary evidence: TUI exploration/tool/pointer regressions; paired fixture.
  - Status: in_progress
  - Evidence: coordinated native implementation passes all 13 exploration tests,
    all 591 oc-tui library tests and strict oc-tui/all-target Clippy. New mixed
    fetch/empty-part regression failed on old positional behavior before the fix.
    Paired actual-binary qualification remains R4.
- R2: actual local Code Mode search is an ordered bounded observation.
  - Source: owner Execute/search screenshot and corrected plan B.
  - Acceptance: actual global/fallback catalog search appears as a completed
    `search` child with captured parameters, interleaved in invocation order with
    MCP and genuine `fetch`. No fabricated calls, extra effects/permissions/child
    operations, execution cap, public API break or unbounded observation backlog.
  - Primary evidence: deterministic interpreter/runtime ordering, bounds,
    redaction, final-drain and durable-parent/reopen regressions.
  - Status: verified
  - Evidence: all 27 Code Mode unit tests, two runtime preview/redaction tests and
    nine Code Mode runtime integration tests pass, including one durable parent
    and reopen without replay. Deterministic outcome-first, unscheduled-consumer
    caps, eight-slot FIFO admission and exact search identity regressions pass.
    Strict oc-adapters/all-target Clippy passes; public request-only API is intact.
- R3: Execute details and input ownership match the corrected local contract.
  - Source: owner Execute screenshots and corrected plan C.
  - Acceptance: success/running/error parents open Code/Output; nested calls toggle
    parameters independently. Code/JSON character-wrap with logical-line gutters,
    original JSON text and exact bounded copy; prose word-wraps. No Execute pan.
    Failure/output/copy agree, approval input is visible, read-only paste is inert,
    and bounded captured details survive same-session source replacement without
    invented completion or replay. Esc/backdrop restore the prior draft/panel.
  - Primary evidence: Execute render/input/lifecycle/clipboard regressions and
    actual paired receiving/running/terminal/disclosure/copy/resize observations.
  - Status: pending
  - Evidence: none yet.
- R4: reproducible scoped actual-binary evidence, honestly classified.
  - Source: corrected plan D/E and TEST_PLAN paired-qualification contract.
  - Acceptance: exclusive Execute profile uses official pinned 2.0.31, verified
    TypeScript/JSON inputs, fresh owned roots and current-worktree debug binary.
    The fixture proves direct versus nested fetch, group/error/call toggles,
    original copy bytes, RAW hashes and exact provider/MCP/HTTP no-replay counts.
    Full grids/PNGs/cursors are retained unmasked. Behavior/style assertions and
    EQUAL/DIFFERENT/INVALID/BLOCKED remain separate; unexplained scoped visual
    differences are unresolved, never pixel PASS.
  - Primary evidence: bounded paired capture and immutable per-run comparisons.
  - Status: pending
  - Evidence: cached reference SHA-256 matches 2df936a20cf74af5706020342164628f9a31978c50c2b0ad306b42b614fe245d;
    source-level audit only, no runtime qualification yet.
- R5: tested integration and ordinary committed/pushed delivery.
  - Source: owner implementation/commit/push instruction and TEST_PLAN.
  - Acceptance: relevant checks and full offline gate pass on final code; debug
    binary is target/debug/oc2; reviewed scoped commits reach verified origin.
    No uncommitted implementation or known diff-created blocker remains.
  - Primary evidence: actual command exits, clean diff/status and remote tip.
  - Status: pending
  - Evidence: clean master 7b66d433; origin git@github.com:0FL01/oc.git.

### Constraints and Non-goals

- One mutation/Cargo owner, native UID/data-root/Db authority and private UDS.
  RAW, captured authority, cancellation/cleanup and no unknown-effect replay stay.
- Pending permission, argument-stream and unknown rows stay explicit standalone
  boundaries. Exploring lasts until actual terminal outcomes, not a later row.
  These native semantics are intentional exceptions to current TS edge behavior.
- Keep first-source-row positional ownership and bounded resident history/live
  windows. No pending partition, new group-header anchors, per-member range
  redesign, complete-history lookbehind or persistent group registry.
- No arbitrary-tool grouping, WebSearch registration, recursive tree, activity/
  verbosity mode, public observer API, migration, new dependency/config or global
  execution cap. No fabricated Loaded notices or output-prose/name heuristics.
- No shared dialog-stack/selection framework, artifact loads or whole-conversation
  cache. No all-grammar qualification, unrelated T44 work or whole-product READY.
- Fixture-only effects; no paid/live campaign, authoring credentials/config/env,
  user owner restart/kill/lock removal, generic /tmp, sudo or force push.

## Change Envelope / Copied Corrected Plan

### A — Exploration and direct inline tools (one coordinated increment)

- Owner: oc-tui messages.rs/tools.rs and direct Read constructors/consumers.
- Add typed WebFetch eligibility, fetches, colon headers, first-seen ordering;
  atomically extend its member error suppression and independent click route.
- A small render-local descriptor holds source extent, original member indices
  and first operation. Share membership across full/indexed/measurement paths.
- Only structurally whitespace-only assistant content without metadata/notices is
  transparent. Preserve first-row height ownership; skipped/subsequent rows zero.
- Named glyphs: Read/Skill arrow, Glob/Grep star, direct WebFetch percent; ordinary
  text muted, terminal glyph spacer plain, native pending/spinner captions kept.
  Typed Skill denial is muted/struck without acquiring exploration error hiding.
- Parse Read range fields together: invalid explicit field invalidates the suffix;
  positive integers/checked endpoint, no imposed file-specific cap or prose parse.
- Extend existing renderer/style/range/geometry/mouse tests, not a new framework.

### B — Local search observation

- Owner: adapters codemode.rs/codemode/host.rs/runtime/code_mode.rs and tests.
- Preserve public Execution::start/requests/run request-only scheduling. Private
  observed execution reuses private thread/cancel/join ownership and a transport
  enum, not forwarding tasks or a second metadata stream.
- Observe actual search using its already-converted input after successful local
  execution; no new reply/await/admission. Preserve FIFO invocation order and
  eight-slot external admission; handler construction stays at admission.
- First 16 combined starts plus one overflow marker; later searches still execute
  without new payloads. Whole supplied scalar fields fit the existing input cap,
  preserve order/types and limitation flags, then undergo runtime redaction.
- After outcome discard effect work, join and drain metadata only. Do not execute
  unawaited calls or invent completion for retained unconfirmed Running entries.

### C — Execute details

- Owner: tools/execute.rs, app/execute.rs, execute_view.rs and narrow input/render/
  lifecycle consumers; include retained snapshot in memory accounting.
- All typed parent headers open details; failed transcript preview stays static
  under the existing four-logical-line plus character budget.
- Character-wrap highlighted Code/JSON, shared blank continuation gutters, no
  pan; keep original JSON and word-wrapped prose/logs/warnings.
- Share truthful failure/output/copy selection, including bounded safe error
  preview fallback; keep async clipboard acknowledgement and limited notices.
- Suspend Execute paint during approval ownership and invalidate stale presses.
  Keep one bounded last-admitted display snapshot on same-session source loss,
  prefer current facts and show unavailable source without invented completion.
- Read-only paste is inert; captured Esc/backdrop close restores panel/draft.

### D — Existing capture profile and oracle

- Owner: scripts/tui_capture Execute fixture/probe/guards/bootstrap and narrow
  README/T44 contract updates; old seals and general 2.0.12 pin stay unchanged.
- Official 2.0.31 release 22728d67461260d56bb9796b9b8f36e1de47202e,
  source checkout bc5ebf00a4c169ece10f276e6ce02eaf18c326e6. Record narrow source
  equivalence/manifests, not whole-tree equality. Establish only TS/JSON cache
  inputs and effective owned browser/font/temp setup before qualification.
- Reuse compound SSE pattern: four direct calls plus successful Execute, then
  failed Execute; five provider requests, six top-level operations, four success
  children, one failed child, three MCP effects and two GETs. One direct Read
  fails to prove separate member-error disclosure. Validate exact result IDs.
- Route immediate/held GETs in existing fixture handler. Additional inspections
  occur after releasing held fetch; retain production/fixture deadlines.
- Hash settled RAW and include HTTP in reopen/restart no-replay comparisons.
  Bind native capture to this worktree debug binary and attest source/fresh roots.

### E — Acceptance and delivery

- A/B/C can be reviewed independently; WebFetch grouping/style/error routing land
  together. D consumes their final behavior; one mutation owner throughout.
- Run targeted TUI/Code Mode tests, fixture/comparator/docs checks, diff review,
  full scripts/test.sh and bounded paired actual-binary capture sequentially.
- Scoped behavior/style observations are not pixel PASS. Keep comparator results
  and exit statuses unchanged; diagnose every unexplained scoped difference.
- Commit/push verified increments to origin/master without force. Other pending
  product qualifications in GOAL.md remain untouched.

## Current Checkpoint

- Closes: R3 (R1 native checks and R2 complete; paired acceptance remains R4).
- Smallest next action: character-wrapped Execute details for all parent states,
  consistent bounded output/copy and narrow approval/source-loss/input ownership.
- Expected evidence: existing and extended Execute render/input/lifecycle/copy
  regressions, then full oc-tui tests and strict affected-crate Clippy.

## Current State

- Resolved: R1 native rendering/range/pointer checks pushed as 83343ecf6; R2
  private bounded observations pass native checks. No schema/dependency change.
- Last relevant evidence: 27 Code Mode unit + two runtime preview + nine runtime
  integration tests; strict oc-adapters/all-target Clippy; diff check clean.
- Blocker: none established. R4 needs the exact pinned JSON WASM download and
  narrow cache bootstrap; existing browser/font/xterm and bundled TS are present.
- Next: R3, then R4 against the same final binary.

## Checkpoint History

- 2026-10-19: accepted audited plan copied here; no source implementation yet.
- 2026-10-19: R1 native increment passed targeted/full-crate checks; no visual
  PASS claimed. Initial full test compilation exceeded the 120s shell watchdog;
  no surviving Cargo process, completed with a 600s compilation allowance. Actual
  targeted tests took 0.01s and full crate 6.30s, no test deadline was changed.
- 2026-10-19: R2 observed search closes the real missing nested row. The existing
  literal-MCP-path integration failed before implementation because search was
  absent, then passed with its original result and exact new observation. A test
  fixture initially used reserved Function.call syntax; changing that fixture to
  fixture.echo preserved the interpreter contract. All relevant checks pass.

## Completion

- Pending R1–R5. No live/visual/full-product PASS claimed.
