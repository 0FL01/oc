# Goal: canonical completed transcript

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete
Source: owner instructions of 2026-10-18; GOAL.md A01/A02/A08/A10
Last updated: 2026-10-19

## Objective

Finishing a primary or child response does not erase previously admitted tool
arguments or canonical assistant text from its accessible history. Remove the
aggregate turn preview policy responsible for this loss, and qualify the affected
transition against the pinned OC2 TypeScript reference.

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
- R2: visual parity for this completion/history bug family.
  - Source: owner: "нам нужен визуальный паритет с oc2 ts" and "никаких костылей".
  - Acceptance: affected populated labels, grouping/expansion, blank rows, terminal
    success/error presentation, final text and viewport/cursor agree with pinned
    TS 2.0.12 in comparable root/child states. Legitimate terminal footer/composer
    changes are not treated as crashes. No fallback label substitutes for lost data.
  - Primary evidence: existing actual-binary full styled-cell/PNG/cursor capture
    pipeline, frozen reference provenance, focused before/after/reopen episodes.
  - Status: verified
- R3: verified and delivered implementation and debug binary.
  - Source: owner: "коммит пуш, билд дебаг билда"; AGENTS.md and A01.
  - Acceptance: relevant regressions and full offline gate pass, reviewed cohesive
    commits reach verified origin/master, target/debug/oc2 is built normally.
  - Primary evidence: targeted checks, bash scripts/test.sh, Git commit/delivery
    facts and cargo build -p oc2 --bin oc2 --locked.
  - Status: verified
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
