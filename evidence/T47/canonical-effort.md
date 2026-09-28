# T47 R6 / VAR01 — canonical effective effort ordering

## Result

**PASS for the implemented R6/VAR01 backend slice and fresh AUD41 regressions.**
This is additive qualification, not a full-product A01–A13 or T44 completion claim.

- Base: `7e594e670dc6ddf11db001fcee72a1a67ba12eca`; qualification on the reviewed
  uncommitted worktree below. HEAD stayed at that base; no implementation commit yet.
- `models::ordered_variants` is the shared borrowed, stable metadata view after
  effective merge. Known ranks are exactly `none/minimal/low/medium/high/xhigh/max`,
  followed by custom entries. Known explicit effort wins over name; absent/nullable
  effort permits standard-name rank consistently with typed snapshots. Unknown
  explicit strings, case and whitespace remain custom and retain exact wire values.
- Application snapshots retain declared disabled/reserved metadata in that order.
  Picker availability, variant options and enabled diagnostics omit disabled entries
  and the exact reserved `default`. Ctrl+T consumes the owner snapshot order with its
  existing Default/wrap/stale/no-op rules. Aliases, equal-rank ties and custom entries
  retain effective source order. No model/provider sorting, discovery input/oracle,
  merge, dependency, toolchain or capability policy changed.
- Existing declared backend allowlist validation is preserved, including a legacy
  explicit `default` metadata entry. The UI's separate Default selects `None` and
  adds no variant overlay; named `none` stays a distinct selectable overlay.

Reviewed slice paths (including the new, untracked test module):

```text
crates/oc-adapters/src/models.rs
crates/oc-adapters/src/models/tests.rs
crates/oc-adapters/src/application.rs
crates/oc-adapters/src/application/tests.rs
crates/oc-core/src/queries.rs
crates/oc-tui/src/picker.rs
crates/oc/src/tui_cmd.rs
crates/oc/tests/pty_t39.rs
crates/oc/tests/pty_t39/interaction.rs
evidence/T47/canonical-effort.md
```

The pre-existing scheduling note and generated progress/NOW/STATE/journal changes
are outside this slice and were preserved. No stage/commit/push or task-state update
was performed; inherited `.opencode/` was not inspected or edited. Existing evidence,
goldens, acceptance registries and GOAL remain unchanged.

Parent follow-up: independently reviewed all nine production/test paths and reran
`models::` (8 PASS), the effective merged snapshot/refresh case (1 PASS), all three
`var01_` actual-binary PTY cases and all three `t47_` actual-binary AUD41 cases.
Final native digest remained `d28c…`; docs/progress/diff and size checks PASS with
no >5000-line warning. Current CODE_MAP now routes to this real shared owner/test seam.

## Checks

Environment: dedicated non-root `opencode` account (uid 1003), approximately 7.5 GiB
available RAM and 177 GiB free disk before the workspace gate. Pinned `rustc 1.93.0`
and `cargo 1.93.0`. One Cargo/PTY coordinator; all heavy commands used timeout 900000 ms.
Cargo test/build/clippy and direct integration-test commands used:

```sh
env CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
  TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 <command>
```

Log directory `L = /home/opencode/.cache/opencode-tmp/opencode/`.
All successful commands below exited **0**.

| Command | Observed result | Log under L |
| --- | --- | --- |
| `cargo test --locked -p oc-adapters --lib models::` | 8 PASS, including all 5,040 known-level permutations, all 128 supported subsets, exact effort/name conflicts, stable aliases/custom zeta→alpha, missing/disabled/reserved choices, absent/nullable/unknown/case/space values and unchanged malformed discovery rejection | `t47-models-qualified.log` |
| `cargo test --locked -p oc-tui --lib picker::` | 7 PASS; existing identity/retirement/picker regressions | `t47-picker-targeted.log` |
| `cargo test --locked -p oc --bin oc variant_cycle` | 3 PASS; owner order, stale/empty/child rules, persistence and next real request | `t47-cycle-targeted.log` |
| `cargo test --locked -p oc --test pty_t39 var01_ -- --nocapture` | 3 PASS; new actual-binary scenarios | `t47-var01-pty-final.log` |
| `cargo test --locked -p oc-adapters --test runtime turns::accepted_model_switch_is_public_only_and_does_not_add_provider_requests -- --exact` | 1 PASS; unchanged legacy explicit-default/no-extra-switch scenario | `t47-allowlist-final.log` |
| `cargo fmt --all -- --check` | PASS | terminal, no output |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS | `t47-workspace-clippy-final.log` |
| `cargo test --workspace --locked` | **1,202 PASS, 0 failed, 9 existing opt-in tests ignored**; all affected crates, 39 PTY tests, 3 doc-test targets | `t47-workspace-tests-final.log` |
| `cargo build --locked` | PASS | `t47-build-locked.log` |
| `cargo build` | PASS | `t47-build.log` |
| `target/debug/oc --help` | PASS; native command help | `t47-help.log` |
| `target/debug/deps/pty_t39-5105e138ceb60f7c var01_ --nocapture --test-threads=1` | 3 PASS against the final `cargo build` binary, without Cargo rebuilding it | `t47-final-built-binary-pty.log` |
| `target/debug/deps/application-69b0e42cc6f85448 t47_ --nocapture --test-threads=1` | 3 fresh AUD41 actual-binary PASS against that same final binary | `t47-final-built-binary-aud41.log` |
| `git diff --check` | PASS; production and new test diffs reviewed | terminal, no output |

### VAR01 identity, wire and effects

- Nearest application reload fixture verifies final local effort/disable overrides
  **after discovery merge**, raw disabled/reserved snapshot facts, absent effort,
  stable custom-order refresh and exact `fast` identity/`low` effort. Fresh workspace
  result: `application::tests::reload_tests::canonical_effort_snapshot_ranks_effective_overrides_after_discovery_and_refresh` PASS.
- Real PTY keyboard traversal in both session and Home completes
  `Default → none → minimal → fast(low) → low(no effort) → medium → high → xhigh → max → zeta → alpha → Default`.
  Independent picker Home/down/Enter indexing selects the same identities at every
  step. Model and unsent draft stay exact; cycle/picker traversal sends **0 main
  Responses requests**. New-session/session-reopen also preserves the parked draft.
- Next accepted fake Responses requests carry exact alias effort `low`, omit
  reasoning for name-ranked `low` and for Default, carry literal `none` for named
  none, and preserve custom effort `" deep "` without trimming. Configured output
  cap 128 and cache-key option survive all overlays/Default. Six explicit main
  submissions produce exactly six captured main requests. The held-stream barrier
  verifies busy Ctrl+T does not submit/change choice or discard the next draft.
- Missing and all-disabled/reserved-only catalogs are actual-binary no-ops. Read-only
  child uses a genuinely enabled `low` catalog, so a missing guard cannot hide behind
  an empty catalog. Shortcut preserves draft/model, yields read-only feedback, sends
  zero main requests and leaves durable history/parent selection unchanged. All
  tested native exits succeed and restore terminal state.
- Actual `ludka2` GET discovery plus local merge/reload shifts `fast` from picker
  index 2 to 4 while the exact alias remains selected. Reopening the picker, session
  navigation, OS-process restart and scoped persisted record all retain `fast`.
- Successful actual refresh removes, then disables, independently selected Home
  `fast`. Metadata remains `fast (unavailable)` and picker has retirement diagnostics,
  not a current Default. Submission fails with canonically ordered enabled choices,
  preserving the draft and sending zero main requests. Explicit Ctrl+T recovers to
  Default; the final accepted request then has no reasoning overlay. Existing retained
  session/generation reload rejection remains intact; no silent request fallback.
- Fresh full regression includes DISC01–DISC10/DISC06 merge allowlist/publication,
  AUD18 static high-context preservation, exact selection/retired choices, missing/
  zero/partial unknown budgets with input/output preflight, assembled title admission,
  separate title model without variant overlay, child fallback warnings, profile/child
  authority and immutable-generation boundaries. Legacy AUD41 report was not used as
  current PASS evidence. No authenticated/live/paid API or new package was used.

### Binary association

SHA256 verified before/after direct final-binary qualification:

```text
d28c457f67cdf276e199955580a60ffc21f0ebb3256f56dd9c9046a153ad67da  target/debug/oc
3a9c2b25e5aebc847332ed26014b154d2b1d009e6e5634025218a832cd9968cf  target/debug/deps/pty_t39-5105e138ceb60f7c
fcc667178a21d2837767bc6f7ceaf6febe93d21687677a4048f21f2dc05eef20  target/debug/deps/application-69b0e42cc6f85448
```

The workspace-test build had native digest
`7b54991e00c4e727f624e5f6dafbca4d6c84e888782fafade95772a0c0890162`.
`cargo build --locked` rebuilt the dev-profile TUI/native binary, so the final
published-path digest differs. Direct existing integration harnesses then qualified
the final `d28c…` binary; it was not replaced by another Cargo invocation afterward.

### Diagnosed intermediate failures

Initial new PTY fixtures had incorrect expectations for existing diagnostic wording,
unpersisted no-op preference, explicitly cleared draft and a history suffix covered
by the startup notice. Corrected the fixtures using actual behavior and processed-key/
generation barriers; no sleeps, timeout increases, ignored tests or relaxed caps.
Logs: `t47-var01-pty-first.log`, `t47-var01-pty-qualified.log`; final runs above PASS.
The first workspace gate exposed my accidental rejection of legacy explicit `default`;
restored declared allowlist validation while retaining filtered canonical diagnostics.
Existing failing test stayed unchanged. Log: `t47-workspace-tests.log`; corrected
targeted and full gates above PASS. A short `--exact` filter initially matched zero
tests (`t47-allowlist-qualified.log`); it is not counted as qualification, and the
fully qualified `turns::…` test was subsequently run and passed.

## Risks

- No unresolved failure in this frozen behavioral slice. Discovery malformed-data
  rejection and immutable retained-session reload guards remain authoritative.
- Intentional approved donor difference: native known effort ranks precede stable
  custom entries; pinned OC2 retains declared order for the same unsorted input.
  This report provides behavior/wire/durability qualification, not paired geometry,
  colors, styled cells, PNG or cursor evidence for VIS09/VIS29.
- `opencode_models` / T50 TOOL18 is still a pending consumer; this helper does not
  claim that tool was implemented. No full GOAL, all-T44/V09 or product READY claim.

## Next

Return sole mutation/Cargo/PTY ownership to the parent for independent review and
verification, implementation commit and factual T47 task closure when its complete
contract is satisfied. Then use this shared ordered view and evidence for the separate
T44 VIS09/VIS29 paired presentation qualification and T50 TOOL18 consumer. Continue
the remaining full T44/V09 and GOAL A01–A13 campaign; scheduling pause is a prerequisite
handoff, not an external blocker or completion status.
