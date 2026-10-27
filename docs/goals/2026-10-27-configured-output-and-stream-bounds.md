# Goal: configured model limits without fragmentation-dependent stream failures

Status: complete
Source: user requests in this session: dynamic input/output/context from OpenCode config; investigate and fix the 10,000-event trap and cumulative wire accounting; preserve this plan in a goal; implement, commit, push, build.
Last updated: 2026-10-27

## Objective

Requests use the selected model's effective configured limits, and valid long responses do not fail merely because the provider sends many small events. Preserve bounded retained state, cancellation, truthful terminal/error outcomes and tool admission. Deliver checked commits to the verified origin and a release binary.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Stop at a proven external blocker and record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: use dynamic model limits, not a hardcoded default for fully described models.
  - Source: user's original llama-lain report and clarification that 64,000 is only their example.
  - Acceptance: zero requested output selects positive model output when context/output are known; partial metadata retains fallback/warnings. Input admission reserves output and the existing safety margin. Explicit title/compaction requests retain their clamp. Model switches use their own limits and receipts agree with wire.
  - Primary evidence: existing model budget, Chat roundtrip, live-switch, partial-metadata/child, binary application and VAR01 PTY tests.
  - Status: verified
  - Evidence: commit 81cdeef70; targeted tests passed before pause. Config uses 64,000/32,000 and model switches 10,000/2,000 rather than one runtime constant. Full gate not completed yet.
- R2: remove stream-event count as a generation cutoff and stop charging freed wire envelopes as retained memory.
  - Source: user's accepted detailed fix plan following the 10,000-event investigation.
  - Acceptance: Responses, Chat, Messages and Responses WS accept genuine terminals after >10,000 productive events and after the former cumulative envelope charge would exceed 32 MiB, provided retained state fits. Ignored events do not accumulate retained-state charges. Real retained state remains bounded; preserve framing, argument, request and terminal validation.
  - Primary evidence: extend nearest provider long-stream/byte-bound tests, protocol tests and synthetic WS peer; no paid generation.
  - Status: verified
  - Evidence: removed EVENT_CAP refusal and cumulative wire charge. Tests passed for 65,536 fine deltas in Chat/Messages/Responses HTTP, 20,000 WS frames, byte-bound refusal, ignored/empty events and duplicate done. Final implementation is covered by all 2434 nonignored workspace tests.
- R3: separate Responses output indices from transport event count.
  - Source: accepted plan's output_index finding.
  - Acceptance: sparse indices are not limited by EVENT_CAP; checked arithmetic rejects index overflow without panic or unbounded allocation, retaining order/identity conflict validation.
  - Primary evidence: existing reconciliation tests extended for large sparse index/overflow.
  - Status: verified
  - Evidence: output_index accepts sparse u64 positions; next_output_index uses checked_add. sparse_output_index_overflow_is_local passed (u64::MAX terminal succeeds; inferred successor refuses locally). Existing sparse reconciliation passed.
- R4: retain already published text after local provider failure as incomplete, without admitting unfinished tools or retrying local errors.
  - Source: accepted plan's partial-text error-path correction.
  - Acceptance: a reachable local stream failure after text preserves that text durably and visibly as incomplete; operation remains failed, with no tool execution/retry caused by that error.
  - Primary evidence: one nearest runtime fake-wire regression covering text, local failure, history and zero tool effects.
  - Status: verified
  - Evidence: prov09_local_rejection_after_partial_has_no_retry_intent_or_effect passed with durable incomplete text, unchanged reopened journal and zero operations/retry for local rejection. Transport continuation/opaque eligibility remains separate.
- R5: checked commit, push and release build.
  - Source: user "коммит пуш билд потом".
  - Acceptance: relevant tests plus required offline gate pass; exact scoped diff committed, ordinary fast-forward push to verified origin/master, and target/release/oc2 built. Report any unavailable check honestly.
  - Primary evidence: command exits, git status/remote hash and binary --version.
  - Status: verified
  - Evidence: fmt and strict workspace/all-target Clippy passed; all 2434 nonignored workspace tests passed across the interrupted run and its exact 29-test continuation; doctests and debug help/version passed. Release build with 16 jobs succeeded (6m45s), target/release/oc2 reports 0.2.0. Scoped diff review and git diff --check passed. Commits 81cdeef70 and c716f5516 were fast-forward pushed to verified origin/master; worktree clean before this closure update.

### Constraints

- Runtime values follow effective selected model/config snapshots; no llama-lain or 64K special case.
- Retain cancellation/fairness, bounded bytes/state, genuine terminal and identity/JSON validation, captured request/tool ownership, no unknown-effect replay.
- Do not weaken permissions, path/security policy, the bounded-live 2048 guard, or retry eligibility for local errors.
- One mutation/Cargo owner; preserve unrelated concurrent changes. No user config/data/schema changes or service restart/lock deletion.
- Latest user concurrency instruction supersedes the temporary two-thread ceiling: use all 16 available cores for compilation/tests, retaining existing exclusive/serial test groups. Do not persist a global/project scheduler change.

### Non-goals

- No new dependency, config knobs for maxEvents, generic resource framework, storage migration or retry engine.
- No broad provider/status compatibility audit, unrelated cleanup, model benchmark or paid-live qualification.
- Do not remove separate WS checkpoint optimization limits just because they also use the number 10,000.

## Change Envelope

- Target: model budget (already committed), shared provider framing/retained-state accounting, direct protocol decoders/WS consumers, Responses index reconciliation and runtime failure projection.
- Expected paths: crates/oc-adapters/src/provider.rs, provider/{chat,messages,websocket}.rs and nearest tests; runtime/turn.rs and nearest runtime integration test; affected provider contract docs and this goal.
- Allowed artifacts: minimal Rust edits/regressions and contract documentation. No new persistent state, public/generic API, processes/services or dependencies.
- Verification: narrow tests first; one full offline gate before acceptance; release build. Follow docs/TEST_PLAN.md resource/live limits.

## Current Checkpoint

- Closed: R1-R5 verified. No further substantive work or audit is authorized by this goal.
- Delivery: 81cdeef70 (dynamic configured model limits), c716f5516 (retained-state stream bounds and incomplete local-failure text) pushed to origin/master; release binary built and checked.

## Current State

- Starting HEAD: 81cdeef70cd659f6f64e5003d2be61dff8fd865a; master tracks origin/master (verified git@github.com:0FL01/oc.git); clean worktree at resume.
- Non-root; memory/disk preflight passed. One Cargo owner; no user service changes.
- R1 targeted checks passed: model budget, Chat roundtrip, busy model switch, runtime partial limits, child fallback, binary application (4 tests), VAR01 PTY.
- R2-R4 implemented and verified. Messages text appends in place rather than cloning the entire growing block on every tiny delta.
- Current gate passed fmt and strict workspace/all-target Clippy. A subsequent CARGO_INCREMENTAL=0 build linked successfully (the prior incremental build had undefined LLVM symbols after interruption; no artifacts removed). All-core nextest run 9f3f4856-dda5-4b42-aa16-88bc3cb96e7a passed 2405 tests before the tool's 600-second total deadline (compilation plus 240.003 seconds of tests) sent SIGTERM. Five running tests were terminated and 24 nonignored tests had not started; 18 existing ignored tests remain excluded. No assertion failures. Compared structured `nextest list` JSON with JUnit passed cases to identify exactly 29 remaining tests; do not rerun the other 2405.
- Continuation f1a42306-c922-4a25-a07a-b76665060f80 ran exactly those 29 tests: 29 PASS in 130.316s. Together the two runs cover all 2434 nonignored tests on unchanged runtime code. Doctests (no examples), debug build/help/version and release build/version passed. Runtime diff reviewed once; no unrelated edits, dependency or schema changes.
- Release artifact: target/release/oc2, 105304712 bytes, version oc2 0.2.0.
- Blocker: none.
- Delivered: ordinary push c86990eb2..c716f5516 to origin/master succeeded. The temporary large-index test failure was an invalid assumed ordering in a newly extended test; restored the existing prefix case and tested explicit large indices/checked overflow separately.

## Material Decisions

- 2026-10-27: do not raise 10,000 or derive an event count from max_tokens. Event fragmentation is not model capacity or resident memory. Replace the cumulative wire-envelope charge with accounting of state actually kept by the parser/collector, retaining an explicit byte budget (not a claim of exact RSS).
- 2026-10-27: separate sparse-index arithmetic from event count; local resource errors remain nonretryable. Already displayed text survives those errors but unfinished calls do not execute.

## Completion

- Resolved outcomes: R1-R5 verified.
- Commands and artifacts: cargo fmt --all -- --check; workspace/all-target strict Clippy; nextest workspace (2405 PASS + exact 29 PASS continuation, 18 pre-existing ignored); cargo test --workspace --locked --doc --no-fail-fast; debug build/help/version; cargo build -p oc2 --bin oc2 --locked --release -j 16; release --version; git diff --check.
- Constraint and diff-scope check: passed for reviewed runtime/test/documentation diff; no user owner restart, dependency, schema or user config/data changes.
- Final status: complete. No paid-live generation or user owner restart was performed; resource byte limits remain explicit failures, not a promise of unlimited output.
