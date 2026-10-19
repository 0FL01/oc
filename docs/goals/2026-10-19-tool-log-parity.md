# Goal: exploration and Execute tool-log parity

Status: complete
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
  - Status: verified
  - Evidence: coordinated native implementation passes all 13 exploration tests,
    all 591 oc-tui library tests and strict oc-tui/all-target Clippy. New mixed
    fetch/empty-part regression failed on old positional behavior before the fix.
    The pinned actual-binary a04 fixture also passes collapsed/expanded group,
    independent failed-member disclosure and direct/nested fetch style assertions.
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
  - Status: verified
  - Evidence: all 598 oc-tui library tests, including 12 Execute regressions,
    pass in the final workspace gate. These include
    including failed-parent mouse routing, Unicode/source-preserving wrapping,
    exact copy, independent fetch/search disclosures, source eviction/resync,
    visible approval ownership and read-only paste/backdrop containment. Strict
    workspace/all-target Clippy passes. Paired a05 proves live/copy/resize behavior,
    matching modal geometry/colors/footer and the corrected transcript-preview
    continuation inset. Successful modal cells match the reference at all sizes.
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
  - Status: verified
  - Evidence: `evidence/tui/tool-log-parity-20261019-a05` retains 30 stages per
    binary, both PASS_BEHAVIOR_ONLY, exact five-request/six-operation/three-MCP/
    two-GET graph, settled RAW hashes, no replay and natural generation 0/1 exits.
    The fixed 2.0.31 binary and bundled TS/pinned JSON inputs are verified. Full
    comparisons retain 13 EQUAL/17 DIFFERENT grids and 3 EQUAL/27 DIFFERENT PNGs
    (exit 1), not pixel PASS. Receiving, Running and successful modal cells match,
    including 80x24/160x48. Remaining differences are actual elapsed/diagnostic
    data, intentional explicit argument streams, chrome/cursor metadata, painted
    whitespace FG and a two-pixel right raster edge even with identical grids.
    No comparator normalization, crop or mask is used.
- R5: tested integration and ordinary committed/pushed delivery.
  - Source: owner implementation/commit/push instruction and TEST_PLAN.
  - Acceptance: relevant checks and full offline gate pass on final code; debug
    binary is target/debug/oc2; reviewed scoped commits reach verified origin.
    No uncommitted implementation or known diff-created blocker remains.
  - Primary evidence: actual command exits, clean diff/status and remote tip.
  - Status: verified
  - Evidence: final `bash scripts/test.sh` passes: strict workspace/all-target
    Clippy, 2132 nextest tests (12 pre-existing skips), doctests, debug build and
    help/version. Runtime test phase is 381.493s. Verified increments through
    d07dc4ced are pushed to git@github.com:0FL01/oc.git master; clean status and
    matching remote tip verified after that delivery. No force or history rewrite.

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

- Closed: R1–R5. No remaining implementation checkpoint.
- Delivery: reviewed increments through d07dc4ced are pushed; this final closure
  is documentation-only and follows the same ordinary commit/push policy.

## Current State

- Resolved: R1–R5 behavior/style, native lifecycle, ordered bounded observations,
  full offline gate and committed/pushed implementation. No schema/dependency change.
- Last relevant evidence: 12 Execute regressions, six Execute fixture/cache tests,
  four existing tool-preview fixture tests and 25 comparator tests pass. Paired
  a05 passes 30 behavior/style stages per side; full comparison outcomes are
  retained separately rather than advertised as pixel PASS.
- Blocker: none established. The exact JSON WASM, narrow cache/source provenance
  and effective owned browser/font/temp setup are verified.
- Next: none; the frozen slice is complete. Other product qualifications remain
  separate, not an automatically resumed backlog.

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
- 2026-10-19: R3's actual failed-parent mouse regression failed before the fix.
  All 11 Execute tests now pass; full oc-tui has 597 passing tests (8.33s) and
  strict Clippy passes (32.47s). Clippy identified a complex pointer tuple and
  test-only borrow/clone issues; a named local press record and scoped test
  borrows fixed them without suppressions. No paired visual PASS claimed.
- 2026-10-19: paired a02 passed behavior but exposed modal inset, raised band and
  footer differences. A failing surface regression drove the narrow correction;
  all 12 Execute tests pass. a03 exposed a probe selector that incorrectly assumed
  no transcript text to the left of the wide modal; paint-coordinate ownership
  fixed it. a04 passes both 30-stage probes, preserving all nonzero full-frame
  differences. Its transcript-error wrapping inset remains the next in-scope fix.
- 2026-10-19: a05 verifies that preview continuation fix and all 30 stages per side;
  exact effects/RAW/copy/no-replay assertions pass. All successful modal cells
  match, while full grid/PNG outcomes remain honestly separate as recorded above.
- 2026-10-19: the first full gate had one native-move progress failure after a
  34m18s cold workspace compilation (two active rustc processes; adequate memory/
  disk, not a stuck test). The unchanged case passes isolated in 2.192s, then the
  unchanged 10-slot full profile passes all 2132 tests in 381.493s. Its failure
  cause was not established; no automatic retries, deadlines, assertions, workload or test
  configuration were changed. The only >30s case is the documented discovery
  deadline exception (32.474s, below its owner-approved 40s bound).
- 2026-10-19: final reviewed code/fixture increment d07dc4ced pushed normally;
  clean worktree and exact origin/master tip verified. Frozen closure check passes.

## Completion

- Resolved outcomes: R1–R5 verified with the evidence above.
- Commands: `bash scripts/test.sh` exits 0 (2132 nextest tests, doctests, strict
  Clippy/fmt, debug build/help/version); six Execute fixture/cache and four
  tool-preview fixture Python tests pass; 25 comparator tests pass; Node syntax,
  docs source baselines and `git diff --check` pass.
- Actual-binary artifacts: local immutable
  `evidence/tui/tool-log-parity-20261019-a05`, source/build attestation and 30
  stages per side. Comparison exit 1 and all 16 EQUAL/44 DIFFERENT outcomes are
  retained, not converted into pixel PASS.
- Constraints/scope: no schema, dependency, public API or execution-cap change;
  no user owner restart, paid/live effects, secret/config exposure or force push.
  All source/test changes are within the frozen envelope and pushed through
  d07dc4ced. Final closure delivery is documentation-only.
- Final status: complete for this frozen behavior/style slice; no full-frame
  pixel, whole-T44, release-native or whole-product PASS claimed.
