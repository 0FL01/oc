# Goal: canonical completed transcript

Status: active
Source: owner instructions of 2026-10-18; GOAL.md A01/A02/A08/A10
Last updated: 2026-10-19

## Objective

Finishing a primary or child response does not erase previously admitted tool
arguments or canonical assistant text from its accessible history. Remove the
aggregate turn preview policy responsible for this loss, and qualify the affected
transition against the pinned OC2 TypeScript reference.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: remove aggregate turn-content starvation, with recoverable bounded history.
  - Source: owner: "Убирай нахуй эту политику и начинай итеративную реализацию плана";
    original bug concerns primary and subagent history after completion.
  - Acceptance: admitted path/pattern and canonical ordered text/metadata remain
    accessible after completion, root/child navigation and reopen. Early previews
    or effects cannot turn later inputs into missing/empty arguments or empty final
    text. Memory bounds operate on recoverable serving windows, not destructive
    allocation of a shared turn preview budget.
  - Primary evidence: focused storage replay/window regression above the former
    64 KiB threshold; actual root and selected-child completion/reopen episodes.
  - Status: verified
  - Evidence: red late-input regression at base a8a3d28c is green; whole-part
    UTF-8 windows, normalized arguments, large text/reference and immutable replay
    regressions pass. Source-qualified canonical-05 passes all ten actual stages
    in both binaries, including the public-reasoning tail and cold reopening.
- R2: visual parity for this completion/history bug family.
  - Source: owner: "нам нужен визуальный паритет с oc2 ts" and "никаких костылей".
  - Acceptance: affected populated labels, grouping/expansion, blank rows, terminal
    success/error presentation, final text and viewport/cursor agree with pinned
    TS 2.0.12 in comparable root/child states. Legitimate terminal footer/composer
    changes are not treated as crashes. No fallback label substitutes for lost data.
  - Primary evidence: existing actual-binary full styled-cell/PNG/cursor capture
    pipeline, frozen reference provenance, focused before/after/reopen episodes.
  - Status: verified
  - Evidence: canonical-05 affected tool-row cells have zero styled-cell differences
    at seven comparable root/child stages (four complete 120-cell member rows each).
    Real error expand/recollapse, reasoning-tail traversal, 80×24 child resize and
    prompt/composer cursor checks pass. All full-frame grid/PNG comparisons remain
    unmasked and DIFFERENT: real timings/error data and the pre-existing child-input
    bootstrap difference are not rewritten or claimed as whole-product parity.
- R3: verified and delivered implementation and debug binary.
  - Source: owner: "коммит пуш, билд дебаг билда"; AGENTS.md and A01.
  - Acceptance: relevant regressions and full offline gate pass, reviewed cohesive
    commits reach verified origin/master, target/debug/oc2 is built normally.
  - Primary evidence: targeted checks, bash scripts/test.sh, Git commit/delivery
    facts and cargo build -p oc2 --bin oc2 --locked.
  - Status: in_progress
  - Evidence: full offline gate 04 passes fmt, strict Clippy, 2,105 tests (12 existing
    skips), doctests, normal debug build/help/version. Test execution 371.977 s;
    total 689.98 s includes 265 s test compilation and 42.61 s Clippy build.
    Verified origin authentication works with the execution-only owner-provided
    identity. Reviewed commit and ordinary origin/master delivery are next.

### Constraints

- Keep genuine producer per-result metadata/output limits, security validation,
  UTF-8/identity/order, bounded resource retention and native framing guards.
- RAW stays immutable; history viewing executes no tools, retries or provider
  generations and does not automatically load cold tool-output artifacts.
- One native Db owner; no storage migration, user-data/config edits, owner restart,
  lock removal, secret inspection or paid/live campaign.
- Keep canonical durable adoption: live-cache merging and replaying aggregate
  assistant text are not substitutes for canonical ordered parts.
- Existing timing/resource regression gates and pinned reference guards remain.

### Non-goals

- Resume all T44/T61, certify whole-product READY or complete unrelated parity work.
- Fix hypothetical duplicate events or unrelated cancellation/failure lifecycle
  asymmetries without an in-scope reproduction.
- New generic paging framework, persistent store, dependency or renderer.

## Change Envelope

- Target: durable transcript serving and its direct root/child history consumers.
- Expected owners: adapter storage/history/application/live snapshot; core query
  DTOs if needed; TUI history/live/window/renderer and binary history request paths;
  nearest unit/integration/PTY fixtures and existing tui_capture pipeline.
- Minimal optional cursor/page changes are allowed: current message-sequence paging
  cannot recover rows evicted inside a retained message (history.rs enforce and
  tui_cmd history paging). This is a direct R1 blocker, not a new product feature.
- Allowed artifacts: source/tests, current execution contract and factual evidence.
  No historical evidence/golden rewrites, crops/masks or relaxed baselines.

## Current Checkpoint

- Closes: R3.
- Smallest next action: commit and push the verified source, then record closeout.
- Expected evidence: current full gate, verified commits/push and target/debug/oc2.
- Stop or replan if: a necessary change crosses the envelope; record evidence first.

## Current State

- Resolved: read-only diagnosis/provenance and plan audit; implementation authorized.
- Last relevant evidence: full offline gate 04 passes all 2,105 tests, fmt, strict
  Clippy, doctests and debug build/help/version. Canonical-05 completes all ten
  stages in both binaries: 86,565 public bytes per
  source, exact native hashes, nine real tools/seven HTTP requests per origin and
  unchanged settled RAW after cold reopen. Seven tool-row comparisons each have
  zero differences. Full-frame comparisons remain DIFFERENT, not global pixel PASS.
- Blocker: none. Owner-authorized key use resolves the HTTPS credential prerequisite;
  the key contents are never inspected or printed.
- Gate checkpoint: 371.977 s test execution / 689.98 s total, not a warmed gate:
  test compilation takes 265 s and Clippy 42.61 s. Only UI07 exceeds 30 s (32.500 s),
  within its already approved 40 s measurement contract. No thresholds changed.
- Next: verified implementation delivery and factual task closeout.

## Material Decisions

- 2026-10-18: aggregate64KiB introduced in a4bb3612 as a bounded-preview shortcut;
  72fc7b62 later coupled it to a legitimate per-result guard. Remove the aggregate
  policy, not the per-result guard. Multipass loss prioritization and cosmetic
  omission labels were rejected by the owner-directed audit.
- 2026-10-18: bounded correction T70 leaves paused task ownership unchanged.
- 2026-10-18: use existing history owner and optional canonical part cursors on
  History and launch-fenced ReadChild. Original aggregate message readers keep
  their contracts. Transcript reads do not duplicate the archive-sized aggregate
  answer alongside canonical parts. A page target stops before a whole part and
  returns its continuation; a larger producer-admitted single part is served alone.
  No partial JSON, content-priority passes, extra store or schema migration.
- 2026-10-18: private native wire version changes from 2 to 3 for window semantics;
  do not silently talk to an older owner. No running owner is restarted or killed.
- 2026-10-18: canonical checkpoints replace covered live hints. A bounded live-slot
  index records original part sequences; rendering overlays by turn/sequence, never
  by matching repeated prose. A newer canonical read wins over queued older hints.
  The canonical cache/cursors remain intact and completion still adopts durable history.
- 2026-10-18: the same display policy also clipped public reasoning to 16 KiB while
  constructing runtime references. Remove that clip, not the generation/RAW execution
  guards. One large admitted reference can be served alone with continuation.
- 2026-10-18: actual pinned captures prove failed Read grouping, search-root labels,
  glyph/colors and collapsed error interaction differ. Correct those observed seams;
  do not rewrite literal errors or normalize real timing/child-bootstrap differences
  in the unmasked full-frame comparison.
- 2026-10-18: canonical argument JSON may grow when parsed wire numbers normalize.
  Its bounded encoding envelope is distinct from the unchanged raw argument admission
  limit; the focused admitted-arguments regression proves both sizes and exact replay.
- 2026-10-18: R1 also requires the preserved public text to be reachable when expanded.
  Reuse the existing indexed Markdown renderer for reasoning bodies instead of moving
  their old producer truncation into an unpageable UI preview. This is the same R1/R2
  correction, not a new renderer or broader parity campaign.
- 2026-10-18: full-gate fixture corrections keep protocol admission fail-closed and
  the MCP shutdown/reap assertion intact. Observe real reap within the existing
  readiness deadline, as the existing owner test specifies; no timeout or
  production shutdown change. The 681.06 s first run included 280 s compilation
  and 382.123 s tests; its only test above 30 s was the approved UI07 window
  (32.731 s, within 40 s), not a new slow-test exception.

## Checkpoint History

- 2026-10-19: R1 continuation review found a partial turn must stay at the page
  edge: earlier messages before its missing prefix can make edge cursors skip or
  misorder the prefix. Verify both directions through the application owner.
- 2026-10-19: the correctly Location-bound owner regression reproduces four rows
  instead of the contiguous three-row tail. Root/child pages now stop at partial
  turns; forward message traversal starts at part zero. The original aggregate
  API and RAW stay unchanged. This case, UTF-8 continuation and the unchanged
  clipboard deadline/cancel/reap regression pass 3/3 in 2.547 s.
- 2026-10-19: full gate 03 has 2,103 passes and one existing clipboard test failure
  (expired deadline races immediate helper EOF). Permit the smallest correction in
  clipboard/read.rs; keep the deadline/cancel/reap test and all bounds unchanged.
- 2026-10-19: HTTPS push preflight has no credential and no gh client. The owner
  subsequently authorizes ~/.ssh/gl for execution-only Git authentication to the
  exact configured origin. Keyed ls-remote succeeds; no key/config contents are read
  and no persistent Git configuration is changed.

- 2026-10-18: contract frozen before implementation; no checks claimed yet.
- 2026-10-18: red producer regression and actual failing native completion obtained;
  TS passes the controlled root/child/cold-reopen graph. Initial interrupted Cargo
  compile left an incremental linker failure; a non-incremental rebuild passed
  linking and reached the expected failing assertion. Target/dev ELF were not cleaned.

## Completion

- R1/R2 and the final source/checks are verified. Pending R3 Git delivery/closeout.
