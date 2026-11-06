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
  - Status: verified
  - Evidence: strict modern remote tests 3/3 PASS (0.02s); MCP config tests 9/9 PASS.
    `cargo test -p oc2 --test mcp_application --locked mcp_modern -- --test-threads=4`
    PASS 2/2 (0.50s): ordered inline media in the next provider request and native cancellation/quarantine.
- R2: Legacy compatibility and captured lifecycle/effect boundaries remain intact.
  - Source: latest user decision and approved plan contract.
  - Acceptance: omission/`legacy` keeps initialize with zero discovery probes;
    stdio remains legacy-only; `auto` stays unsupported. Modern cancellation emits
    no unsupported client notification and never replays a dispatched call.
    Existing auth, exact URL, permissions, leases, cleanup and quarantine are preserved.
  - Primary evidence: existing legacy tests and focused modern cancellation/application assertions.
  - Status: verified
  - Evidence: pinned mismatch/no-fallback and modern cancellation/no-wire-notification
    tests PASS; legacy defaults/config admission PASS. Native actual-binary interruption
    proves durable unknown, HTTP closure and zero overlapping retry. The full nextest run
    passed all MCP remote/stdio/application cases, including the retained legacy media/cancel tests.
- R3: Document, verify, commit/push and build the implementation.
  - Source: approved plan step 3 and latest execution instruction.
  - Acceptance: config manual describes both modes and image execution timeout;
    scoped tests/Clippy and one full offline gate run; release artifact exists at
    `target/release/oc2`; own commits reach verified `origin/master` by normal push.
    Actual failures and unverified live behavior are reported without overstating PASS.
  - Primary evidence: command results, reviewed diff, release artifact and remote Git SHA.
  - Status: in_progress
  - Evidence: goal committed as `3e23b2312`; config manual/contracts updated;
    `python3 scripts/check_docs.py`, scoped rustfmt and `git diff --check` PASS.
    `cargo clippy -p oc-adapters -p oc2 --all-targets --locked -- -D warnings` PASS.
    One full gate: fmt/workspace Clippy PASS; nextest 2721/2723 PASS, 2 FAIL,
    26 skipped (491.241s after cold test build 11m55s). Not a wholly green gate.
    Exact DNS retest without proxy env PASS (0.05s); exact isolated background-child
    retest PASS (3.29s), no source/assertion/deadline changes. Remaining workspace
    doctest check PASS (zero examples), debug build/help/version PASS; release build
    PASS (7m04s). `target/release/oc2`, version0.2.0, SHA256
    `8cc8dc08f56265f4d9d19586911f36f8cc21707949f1201f7f74e085ea7a3c32`.

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
- Closes: R3 (Git delivery only).
- Smallest next action: commit the reviewed runtime/tests/docs and push normally to verified origin/master.
- Expected evidence: remote Git SHA contains this implementation and the goal commit.
- Stop or replan if: an unexpected worktree/remote change prevents normal delivery.

## Current State
- Resolved: protocol admission and existing SDK lifecycle implemented; strict wire/config evidence PASS.
- Last relevant evidence: 3 modern transport tests and 9 MCP config tests PASS;
  2 modern native application tests and scoped strict Clippy PASS. Runtime/test/doc
  diff reviewed against the frozen scope; no dependency, storage or stdio runtime changes.
- Full-gate failures: unchanged `webfetch::format_tests::tool17_dns_is_inside_total_deadline`
  got HTTP502 instead of Deadline: its prepared reqwest client allows inherited system
  proxies, bypassing the injected HeldDns resolver. Unchanged
  `background_children::background_admission_caps_before_child_prompt_or_effect` hit
  `foreground_children.rs:337`'s five-second provider barrier watchdog under the full run.
- Slow-case diagnosis: unchanged `storage::fork::tests::w8_fork_copies_history_above_old_row_and_byte_budgets`
  passed in103.554s; its setup individually appends5000 small rows and32×600KiB rows
  to durable databases before forking (`storage_fork.rs:1745`). No MCP path is used;
  the expensive durable fixture setup is outside this goal. No workload/threshold weakened.
- Retests: DNS test with proxy environment removed PASS; background-child test
  isolated PASS. Original full-gate failures remain reported, not relabeled green.
- Blocker: none; all implementation/build evidence complete, Git delivery pending.
- Next: commit/push own files, confirm remote SHA, close this goal. No live qualification claimed.

## Material Decisions
- 2026-11-06: latest user decision supersedes the intermediate modern-only hard cut.
  Two explicit remote modes; no auto/fallback; stdio compatibility stays unchanged.

## Checkpoint History
- 2026-11-06: contract frozen before implementation; no live effects authorized or performed.
- 2026-11-06: strict modern wire/config checkpoint PASS. Rich-media and cancellation
  fixtures reuse existing legacy application assertions; no stdio runtime or storage change.
- 2026-11-06: actual-binary media and cancellation checkpoint PASS 2/2; scoped Clippy
  PASS. Frozen diff scope reviewed; full offline gate/release pipeline started once.
- 2026-11-06: full nextest finished2721/2723 PASS with two unrelated failures above.
  All MCP cases PASS. Only those failures are retested; the full gate is not repeated.
- 2026-11-06: exact failure retests PASS without source changes; remaining doctest,
  debug build/help/version and release build PASS. Release SHA recorded; own diff
  remains inside the frozen envelope, foreign test script untouched.

## Completion
- R1/R2 verified; implementation/checks/release complete. R3 awaits normal Git delivery.
