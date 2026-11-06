# Goal: OpenProxy modern remote MCP with legacy compatibility

Status: active
Source: user-approved RECON plan; “тогда оставим два варианта, для совместимости”; “Делай копию план в цель и итеративно реализвоать и коммит пуш билд”.
Last updated: 2026-11-06

## Objective
Native oc2 connects to the current OpenProxy `/v1/mcp` API using explicitly selected
MCP `2026-07-28`, while existing remote/stdio legacy configurations keep their behavior.
Baseline: oc2 `c47ba77f2`; upstream `/home/stfu/ai/agent-frameworks/roxy` `ae6780b8`,
breaking protocol commit `dbfcdbc855478baebe97e0a7cbad86b83039cfed`.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied. Stop at a
proven external blocker or approved budget boundary; record the smallest unlock.

## Frozen Contract

### Required Outcomes
- R1: Explicit modern remote config reaches the existing SDK discovery/call path.
  - Source: approved plan steps 1–2.
  - Acceptance: `protocol: "2026-07-28"` admits remote entries, starts `server/discover`,
    and sends required per-request metadata and matching HTTP headers. Search and
    inline image results use the existing registry/result owners, including complete
    results and tool/JSON-RPC failures. No fallback to initialize.
  - Primary evidence: strict OpenProxy-like HTTP fixture plus one native application roundtrip.
  - Status: pending
  - Evidence: none yet.
- R2: Legacy compatibility and captured lifecycle/effect boundaries remain intact.
  - Source: latest user decision and approved plan contract.
  - Acceptance: omission/`legacy` keeps initialize with zero discovery probes;
    stdio remains legacy-only; `auto` stays unsupported. Modern cancellation emits
    no unsupported client notification and never replays a dispatched call.
    Existing auth, exact URL, permissions, leases, cleanup and quarantine are preserved.
  - Primary evidence: existing legacy tests and focused modern cancellation/application assertions.
  - Status: pending
  - Evidence: none yet.
- R3: Document, verify, commit/push and build the implementation.
  - Source: approved plan step 3 and latest execution instruction.
  - Acceptance: config manual describes both modes and image execution timeout;
    scoped tests/Clippy and one full offline gate run; release artifact exists at
    `target/release/oc2`; own commits reach verified `origin/master` by normal push.
    Actual failures and unverified live behavior are reported without overstating PASS.
  - Primary evidence: command results, reviewed diff, release artifact and remote Git SHA.
  - Status: pending
  - Evidence: none yet.

### Constraints / Non-goals
- Default stays legacy. Protocol selection uses config, never server/model/URL inference.
- Reuse pinned rmcp 3.4.0; no new dependency, transport stack, retry owner, service,
  persistent state or migration. HTTP and stdio remain separate transports.
- No modern stdio, automatic probing/fallback, image `output_path` workflow or JS host.
- No live calls, deployment, user config mutation, owner restart/kill or lock removal.
- Preserve foreign `scripts/test_test_runner.py`; one mutation/Cargo owner, four build/test threads.

## Approved Plan
1. Admit modern remote protocol, carry it into connection settings and validate the
   negotiated version against the selected lifecycle; retain legacy defaults and guards.
2. Select SDK `Initialize` or pinned `Discover` in the existing remote client. Keep
   shared catalogs, permissions, rich results and cleanup; verify strict wire headers,
   metadata, errors, images and cancellation without replay.
3. Extend nearest fixtures/application test, document both modes, run scoped checks,
   one full offline gate and release build, then commit/push the completed work.

## Change Envelope
- Runtime: `crates/oc-adapters/src/{config.rs,config/mcp.rs,mcp_remote.rs}` and direct
  connection consumers only where required to carry protocol settings.
- Tests: nearest config/remote tests, connection-config literals and
  `crates/oc/tests/mcp_application*` fixtures. Keep stdio runtime unchanged.
- Docs: this goal and existing MCP/config manual contracts where behavior changes.
- Git: normal commits on current `master`; origin verified as `git@github.com:0FL01/oc.git`.

## Current Checkpoint
- Closes: R1–R2.
- Smallest next action: implement protocol admission/SDK lifecycle and strict wire regression.
- Expected evidence: modern remote tests pass; legacy config and remote/stdio tests retain behavior.
- Stop or replan if: rmcp cannot satisfy the observed upstream contract without expanding the envelope.

## Current State
- Resolved: read-only RECON; upstream hard-cut and installed SDK support confirmed.
- Last relevant evidence: worktree has no tracked diff; only foreign untracked test script.
- Blocker: none.
- Next: implement the first checkpoint.

## Material Decisions
- 2026-11-06: latest user decision supersedes the intermediate modern-only hard cut.
  Two explicit remote modes; no auto/fallback; stdio compatibility stays unchanged.

## Checkpoint History
- 2026-11-06: contract frozen before implementation; no live effects authorized or performed.

## Completion
- Pending implementation, checks, release build and Git delivery.
