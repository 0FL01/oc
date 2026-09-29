# T51 — qualified startup fault isolation

## Result

**R1–R4 verified; CFG09/CFG10/UI07/E2E06 PASS.** Current source implementation
commits: `33a57bc985124a15b38beb3e501c9b7682630350` (plugins),
`fcfdd584b5ba6672df70f326ba4430602508b240` (provider readiness),
`5dc0fa5eb59de3b0dc66cbdaecb7d7cc885e9353` (typed fatal diagnostics),
`bb527947a41dc209c3c62bbe8d53277592ae0f2a` (saved choices), and
`cbc04cca5` (real-user trace qualification).

| Outcome | Result | Direct evidence |
| --- | --- | --- |
| R1 / CFG09 | PASS | [plugin-admission.md](plugin-admission.md): exact compiled aliases, truthful failed requested capability, no JS resolver/execution; healthy siblings and reload/reopen |
| R2 / UI07 | PASS | [provider-readiness.md](provider-readiness.md): cold missing credential, connect/auth/timeout, local Home/history/selectors, strict pre-effect refusal, admitted recovery, exact selected request |
| R3 / CFG10 | PASS | [fatal-diagnostics.md](fatal-diagnostics.md): typed source/field/stage/code/action; fatal trust/policy/storage/recovery/cleanup/caps; atomic failed reload; safe TUI/details/headless |
| R4A / E2E06 fixture | PASS | [inherited-fixture.md](inherited-fixture.md): active/parked/Home saved missing primary, blocked pin, retired model/variant, unchanged preferences/deck, zero refusal effects, explicit scoped repair and durable recovery |
| R4B / E2E06 existing store | PASS | [real-user.md](real-user.md): bare retained release binary, current inherited HOME/XDG/PATH/Location, actual existing native lock, pre-prompt views, natural exit and no owned descendants |
| R4B / E2E06 fresh store | PASS | Same binary/environment/config sources plus new private --data-dir; views work, native history/preferences remain empty, no browser enabling or generation |

No unresolved assigned outcome or known introduced blocker. This is **not**
T44/V09 pixel qualification or full product READY. T44 remains PAUSED under the
current owner amendments; T45/T50/T53/T54 and final A01–A13 remain separate work.

## Checks

- Final compiled R4A source: `cargo fmt --all --check`, strict locked workspace
  all-target Clippy `-D warnings`, `cargo test --workspace --locked --no-fail-fast
  -- --test-threads=1`: exits 0, **1297 passed / 0 failed / 10 unchanged opt-in
  ignored**. See final managed log `tool_0ef05618f001w1GDHQc9BYJCnZ` and the
  exact artifact association/failure record in inherited-fixture.md.
- `cargo build --locked`, `cargo build --release --locked`, both native helps:
  exits 0. Debug `8d3b36e2e39affdc5fff18c160e1e61404eeb9e06351dbe78f1281d7738adfd1`;
  release `9c56cb0ce2f67de8c01411032547b0c630b29aeb7af1e19ab3111ee7209e7af4`.
  These retained normal artifacts match the delivered Rust at bb527947; hashes
  remained unchanged through final direct native and real-user checks.
- Post-build direct native matrix: **28 PASS** (R4A two, VAR01 one, plugin one,
  provider four, fatal two, configured-workspace thirteen, recovery four, lock one).
  Parent independent owner five and native R4A two PASS under prescribed TMPDIR.
- Real-user harness final pair: both exits 0, 120×40 PTY, 180s watchdog, trace
  payloads excluded and exec argv/env raw. Existing native counts and complete
  preference digest identical before launch / after startup / after exit;
  fresh store empty. Existing and fresh locks acquired/released exactly once,
  owned descendants zero, termios and alternate screen restored. No generation
  prompt, search or tool effect; two actual connected MCPs and one disabled.
- Parent independently re-audited `T51-real-user-_1of_3b_` metadata: same global
  then Location actual config opens, admitted .opencode root probed (no third
  JSON source invented), no successful JS/TS source open or nonthread clone.
  Actual network startup is disclosed, not mislabelled zero network.
- Python bounded-live/code-size/progress/docs suites **47 PASS**, check_docs,
  progress structure, fmt and diff checks exits 0. No soft 5k code warning.
  New opt-in harness changes no Rust compiled input. Private traces 662046 and
  664594 bytes; all its runs total 6277181 bytes, no truncation or raw payload in Git.

## Risks

Real-user history was viewed through its nonempty list without selecting a saved
tab or changing a user's choices. Restored active/parked histories, exact request
identities, scoped repair and durable recovery are proved separately by the real
native fake-service fixture. Trace connections are metadata, not generation
counts; no provider/config/history payload was inspected for real-user proof.
Genuinely malformed mandatory definitions/policy remain fatal. Broken documents
cannot be skipped to broaden authority. Unknown effects are never replayed.

No dependency/toolchain/schema/store/daemon/framework or JS host was added.
Ordinary product admission, permission Deny, source roots, redaction, discovery
oracle/budgets, request generation, MCP quarantine and cancellation remain intact.
Failed attempts and the corrected saved-pin classification are retained in phase
reports, not counted as PASS. R1 has no artificial compiled-module initializer to
claim tested; actual admitted modules and rejected identities are covered.

## Next

Close T51 through the journal using this already committed implementation and
profile evidence. Continue independently ready T45/T50/T53/T54 in ordered slices;
do not resume paused T44 without the required explicit resume. Remaining mandatory
live tests reuse the existing 8-generation/1-search/19-control campaign, without
reset or replenishment. Full GOAL completion and evidence/FINAL.md remain pending.
