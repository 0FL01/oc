# Goal: Rust TUI as an OpenCode 2 plugin

Status: complete
Source: user-approved RECON plan and execution request, 2026-11-12
Last updated: 2026-11-12

## Objective

An ordinary `opencode` launch with the Rust TUI plugin present in the effective
`opencode.jsonc` configuration automatically opens the existing Rust interface
against the original TypeScript backend. Without the plugin, stock OpenCode TUI
remains available. Start with the agreed functional slice, not an unqualified
claim of complete Rust/TS parity.

The user first requested a RECON plan for OpenCode **2.2.7 or newer**, then
clarified the desired entry point: configure a plugin and continue launching
`opencode`, rather than a replacement shell command. After reviewing the plan,
the user requested: «Делай копию плана в цель и проверяй работу, а потом будем
думать как допилить контракт без потери фич из oc2 rust tui (например плавность,
поддержку высокой частоты развёртки и т.п)».

The plan below is the approved starting contract. Comprehensive feature and
performance parity is a subsequent contract discussion, not permission to
remove or simplify existing Rust rendering features now. Do not lower paint
rates, replace the renderer, or weaken existing tests to make the bridge pass.

### Approved lifecycle decision (2026-11-12)

After the stock-2.2.7 failure, the user asked which unlock is preferable in the
long term. The recommended choice was **option 2: an awaited upstream lifecycle
hook**, not a plugin-owned terminal-reset fallback. The user then instructed:
«Вноси правки в цель и приступай к реализации, жду результат; так же протестируй
сам всё если есть возможность».

This authorizes the minimal host-side lifecycle API/change and its integration
tests. Deliver a reviewable upstream candidate against the pinned source, not
an independently maintained TS fork. Keep authored patches/source overlays and
tests in this root worktree; apply/build them in an isolated throwaway upstream
test project outside it. Do not overwrite the installed OpenCode, mutate the
2.2.5 oracle, push to the upstream repository, or claim upstream acceptance.
Qualification of 2.2.7 **plus the candidate patch** is distinct from stock 2.2.7;
the unmodified stock failure remains recorded. A future official release with
the hook may become the deployment minimum; it does not exist by assertion.

The awaited contract is: external UI stops and is joined, terminal restoration
finishes while its host owner is usable, then host destruction may complete.
Normal release can instead resume stock TUI. Cleanup must be bounded, not wait
forever on a child; a notification-only `destroy` listener is insufficient.
No hook can execute after SIGKILL of its own process. Keep child-side cleanup
and IPC-loss handling without inventing an unconditional kill-proof guarantee.
The hook is lifecycle-only: it must not participate in each frame or replace
Rust rendering/scheduling. Resume the functional adapter only after the amended
R2 gate succeeds on the hook-enabled host.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and
Primary Evidence. Work on the smallest unresolved outcome. Do not add
requirements from reviews, tests, tools, speculative risks, or optional source
text. Finish when every required outcome is resolved and affected constraints
remain satisfied. Stop substantive work at a proven external blocker, an
approved budget boundary, or when no remaining in-scope action has a falsifiable
expected result; record the exact evidence and smallest unlock.

## Frozen Contract

### Required Outcomes

- R1: Save the agreed plan and maintain its actual execution state here.
  - Source: latest user request to copy the plan into a goal.
  - Acceptance: this document preserves the approved architecture, checkpoint
    gate, boundaries, results, and next action without relying on chat history.
  - Primary evidence: this file and its scoped review.
  - Status: verified
  - Evidence: initial contract created before implementation on 2026-11-12.
- R2: Config-controlled automatic plugin launch with safe terminal lifecycle.
  - Source: user clarification about `opencode.jsonc` and ordinary `opencode`;
    approved plan sections 2 and 6.
  - Acceptance: enabled starts one Rust child; absent leaves stock TUI;
    compatibility is checked before handoff; normal exit/error/reload/parent
    shutdown restore terminal state and leave no owned orphan. The hook-enabled
    host lifecycle must be qualified before bulk adapter implementation; retain
    the distinct known stock-2.2.7 result.
  - Primary evidence: isolated actual-OpenCode plugin/PTY lifecycle test using
    the exact 2.2.7 baseline plus the approved upstream candidate, initially with
    a minimal child, including the original shutdown-crash counterexample.
  - Status: verified
  - Evidence: the exact-source candidate with both lifecycle-owner patches passes
    `shutdown-crash` and 16 other cases plus an embedded seed run. Join precedes
    renderer destruction, termios/cursor and input-mode reset commands restore,
    and no owned child survives. After correcting the Sol finding about foreign
    suspensions, affected owner/static gates and the full matrix pass again; Sol
    accepts the pre-adapter gate. The actual production-plugin lifecycle case
    also passes crash, disable/re-enable and parent shutdown with restored terminal
    state and no child left behind. The distinct stock failure is preserved.
- R3: The first usable Rust UI slice operates on original TS sessions.
  - Source: approved plan section 4 and done criteria.
  - Acceptance: create/list/open sessions, process-local tabs, paged history,
    text/reasoning/tool presentation, own-prompt streaming, prompt admission,
    execution interrupt versus pending-input cancellation, model/agent selection,
    permission replies, and question-compatible forms work through the captured
    OpenCode client. Rust/stock views see the same saved sessions; settled live
    presentation and reopened history agree. Unsupported actions are explicit.
  - Primary evidence: isolated original TS service with a scripted loopback
    provider, exercised through the actual plugin and Rust binary under PTY.
  - Status: verified
  - Evidence: all eight isolated actual-plugin/Rust PTY cases pass before the Sol
    corrections: basic, tools, selection, history, controls, unsupported, lifecycle and
    parent-loss. They include held-stream text before settlement, distinct queue
    cancellation/interrupt, child-session form routing, foreign-Location catalog,
    opaque paging, settled Rust reopen/stock agreement and terminal/orphan cleanup.
    Sol then identified missing
    Home-agent capture, source question draft ownership and picker-rename gating;
    all three corrections now pass eight source UI/four adapter regressions and
    the affected selection/tools PTY cases. Incremental Sol review accepts source
    tree `f7358cc0759b417ff0d15f54627cd9a10101fa03`. Native integration gates
    also pass. R5 is reopened because the verified release artifact is now missing;
    this does not invalidate the recorded source tests or imply perceptual qualification.
- R4: Preserve authority, admission, failure, and native-mode boundaries.
  - Source: approved plan sections 3–5; repository AGENTS/SECURITY; latest user
    request to avoid losing Rust TUI features.
  - Acceptance: TS remains sole backend owner; no native owner/DB/provider is
    opened in bridge mode; unknown effects are not resent; unsupported forms
    are not answered; bounded failures restore stock UI without implicitly
    cancelling backend work. Native mode, existing renderer and display-rate
    behavior remain intact; no deliberate feature/performance downgrades.
  - Primary evidence: focused boundary/lifecycle regressions, native affected
    tests, and review of the final diff against these constraints.
  - Status: verified
  - Evidence: the new private Rust entry bypasses native startup and the adapter
    uses finite source-specific operations/receipts. Existing renderer/display-rate
    and frame fast paths are retained. Source admission/no-replay/viewer-release
    regressions and the full synthetic PTY matrix pass. Sol accepts the corrected
    diff. Current strict workspace/all-target Clippy, all 2749 selected native
    tests (26 configured skips), doctests and debug CLI smoke pass.
- R5: Validate and deliver the scoped implementation.
  - Source: approved plan section 6 and the original commit/push workflow request.
  - Acceptance: relevant Rust/plugin tests, scoped Clippy, the final full
    offline gate, release build, and final review pass; own changes are committed
    and pushed to verified origin; setup and limitations are documented.
  - Primary evidence: exact command outcomes, review result, release binary
    `target/release/oc2`, and commit/push confirmation.
  - Status: verified
  - Evidence: fixture build/Clippy/formatting/syntax, actual stock and candidate
    PTY probes, upstream owner tests and native build pass. Production plugin
    typecheck and the functional matrix pass. The full offline gate completes by
    resuming its failed stage after the exhaustive-match correction: formatting,
    strict Clippy, nextest, doctests and debug CLI smoke pass on the reviewed code.
    `CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2 --locked --release` passed;
    release help/version and isolated basic/lifecycle probes passed. Source review
    is closed with no blockers. Implementation commit
    `d3979661528540f91d784466f12e488dc46a7c70` was pushed normally to verified
    `git@github.com:0FL01/oc.git`, `master`. Acceptance subsequently found the entire
    root `target` absent; no alternate Cargo target or loss mechanism was established.
    The single subsequently authorized release-only rebuild passed in 14m05s.
    Its root artifact and separate preserved scratch copy are byte-identical to
    the original release hash, pass help/version, and pass the existing release
    basic/lifecycle probes again. Both paths and source provenance are recorded
    below. No broad gate or source review was rerun. Not whole-product READY.
- R6: Implement and qualify the user-selected awaited upstream lifecycle hook.
  - Source: option 2 recommendation and the user's subsequent implementation
    instruction quoted in Approved lifecycle decision above.
  - Acceptance: a minimal reviewable host-side candidate exposes a usable
    awaited terminal-handoff/shutdown boundary; registered child stop/join and
    host-owned terminal restoration precede destruction. The plugin uses that
    boundary rather than a late reset workaround; ordinary no-plugin behavior
    remains intact. No per-frame bridge or Rust scheduling downgrade is added.
  - Primary evidence: pinned-source candidate artifact, focused actual-owner
    tests and the existing isolated plugin/PTY lifecycle matrix on a candidate
    build, with stock versus patched provenance stated separately.
  - Status: verified
  - Evidence: canonical OpenCode and OpenTUI patches apply with zero fuzz and
    byte-match the tested sources. Actual Linux/x64 candidate and full native
    build pass. OpenCode owner tests pass (44 tests, 287 assertions); OpenTUI native
    suite passes (2122 pass, 9 upstream skips), including the reproduced-then-fixed
    external input-mode regression. Sol's foreign-suspension finding was reproduced
    and fixed by moving resume into the registered terminal-owner callback. Both
    affected SIGHUP tests, the rebuilt full PTY matrix and canonical static gate
    pass on the correction. Scoped Sol re-review accepts R6 and the pre-adapter
    R2 gate at patch SHA256 `1e7ac4f4414b4d8bc9e86899d1e82657e6f31bd66288a2082721b3f94ca5648d`;
    native patch SHA256 `02ca27456445b44da018f9ec1521bb249564dcd125dbf3f3c288b4d3e584159b`.

### Constraints and non-goals

- Work only in the current root worktree; preserve foreign `progress/` and any
  concurrent diff. One implementation/Cargo owner. Follow root AGENTS.md and
  `docs/TEST_PLAN.md`; Cargo jobs/test threads are 4, commands sequential.
- Do not edit the user's active OpenCode config, restart/kill their owner,
  inspect authoring credentials, or run their interactive client. Use examples
  and isolated fixture configurations only. Runtime fixture projects/data live
  outside the worktree under approved TMPDIR; synthetic TS tests require the
  existing kernel-enforced loopback-only isolation, not only a fake base URL.
- No independently maintained TS fork, replacement `opencode` shell wrapper, new daemon, public HTTP
  proxy, second backend/DB, storage migration, credential export, pairing, or
  independent `Service.ensure/discover`. No generic backend framework.
- The minimal upstream lifecycle candidate and public hook are explicitly
  authorized by the lifecycle decision above. This does not authorize unrelated
  upstream changes, publishing a release/PR, replacing the user's binary, or
  treating a patched build as an official stock release.
- The new TS code runs as a plugin of original OpenCode, not as an embedded JS
  host in native Rust. Default native `oc2` behavior remains unchanged.
- Preserve existing licenses/provenance. No force push, tag, release, or npm
  publication. Do not update the entire `.local/opencode` checkout.
- The first release does not promise shared/persistent Rust tabs, complete
  native DCP/auth/config/shell/PTTY control parity, all generic forms/attachments,
  seamless reconnect, zero stock startup frames, removal of the TS frontend
  process, or automatic compatibility with every future version above 2.2.7.
- Full feature/high-refresh perceptual parity is reserved for the next explicit
  contract discussion. Keep the current renderer/scheduling rather than adding
  a lower refresh cap or a replacement slow polling/render loop.

## RECON Baseline and Decisions

- Root HEAD: `8b4eb1385d69145babd30297b755677081f6c2fa`, branch `master`, tracked
  and staged diff empty; foreign `?? progress/` exists and must remain untouched.
  Origin: `git@github.com:0FL01/oc.git`.
- Local TS oracle `.local/opencode`: clean `dev` at
  `ff90ed91607cabf420a369e9009539c1aa0932af`, version 2.2.5.
- Exact upstream v2.2.7 target:
  `fedc6f208aa2bd54c1019227214568083f6380ad` in anomalyco/opencode; selected API,
  plugin, lifecycle, and schema files were checked read-only at this SHA.
  The installed binary reports v2.2.7; this alone is not binary provenance proof.
- The tag is 585 commits ahead of the local TS oracle. Synchronize the used
  interface contracts, not both runtime implementations or all upstream commits.
- Rust currently enters `native::attach` in `crates/oc/src/tui_cmd.rs::run_stages`.
  Reuse `CoreApp::channel`, `WorkerGuard`, `TuiState`, and the existing `drive_ui`.
  The UI is storage-free but not backend-agnostic.
- Public TUI context exposes `context.client`, not its URL/auth/custom transport.
  A private IPC bridge using that client is therefore the chosen no-fork route
  to the exact connected backend. No credential-reading discovery workaround.
- Stock 2.2.7 discovers TUI parts of configured server plugins. A package with
  a no-op server entrypoint and `./tui` can load from `opencode.jsonc`; TUI-only
  configuration through `cli.json` is not the requested entry point.
- Server plugin options are not reliably propagated into TUI context in 2.2.7.
  Initial switch is presence/absence in effective `plugins`, with package files
  outside auto-discovery directories. `oc2` is found through the user's PATH.
- Two independent Sol plan reviews found no additional blocking plan defects.
  Their accepted corrections distinguish part ordinals from IDs, split external
  create/prompt admission, and place a child handshake before terminal takeover.
  No executable qualification was performed during RECON/review.

## Approved Implementation Plan

### 1. Baseline and contract

Record the exact 2.2.7 source/API inputs, the first usable slice, and the native
mode boundary here. Do not resume unrelated native parity goals or overwrite
historical source pins. Download any needed isolated source/fixtures outside the
worktree without changing the oracle's refs or starting the user's service.

### 2. Mandatory lifecycle/IPC checkpoint (amended by user approval)

Create `plugins/oc2-rust-tui/` with server and TUI exports and minimal tests.
Use an async `node:child_process` child with terminal descriptors 0–2 and private
IPC descriptors 3/4, subject to actual Bun qualification. The child begins in
protocol-only mode: no terminal input/output and no native initialization.

First implement the minimal awaited host lifecycle hook (R6) at the actual
shutdown/terminal owner, with an upstream-style regression and documented API.
Keep the candidate reproducible against the exact baseline. The plugin must
register its stop/join responsibility before transferring the terminal so host
shutdown cannot bypass it. Use the owner's restoration, not a cursor-only fix.

Perform a bounded, versioned handshake in that same child. On compatibility,
use the qualified host handoff to suspend, then send `Activate`; only then may Rust enter the
terminal. Return cleanup after activation, never await the child's lifetime in
plugin `setup`. Failed handshake terminates and joins the child without changing
host terminal ownership. On later failures, stop/join the child before restoring
the host; resume only this setup's completed suspension while renderer is usable.

Normal child exit returns stock UI. Reload/disable cleanup, child crash, parent
shutdown/SIGHUP, raw mode/cursor restoration, and orphan cleanup must be tested
on the isolated candidate build based on original 2.2.7. Keep the stock failing
baseline available. A destroyed renderer must not be resumed. OpenTUI
suspend removes exit listeners; OpenCode's SIGHUP destroys the renderer before
plugin finalizers. Source inspection is not proof that this handoff is safe.

**Hard gate:** if the candidate lifecycle cannot meet observable safety, stop
before bulk adapter work and report the exact remaining blocker. The minimal
upstream hook is now approved; unrelated fork, wrapper, plugin-owned-reset
fallback, or architectural expansion is not. Test extra FDs and initial
location/session-route timing at this checkpoint too. Ordinary bare `opencode`
is the required entry point; full CLI initial-prompt/continue option forwarding
is not yet claimed.

### 3. Typed bridge and reuse of the Rust UI

- TS shim owns host lifecycle and a small whitelist of `context.client` calls.
  Bounded versioned request/response/event frames carry correlation IDs, not
  credentials. No generic RPC-for-everything gateway or second state authority.
- Add an adapter-owned worker behind existing `CoreApp::channel`, and a TUI-only
  child entry in CLI/bootstrap before native data-root/attach initialization.
  Reuse the existing UI loop and guard rather than copying them.
- Distinguish native shared workspace from external projection/source epochs.
  Implement required startup catalog/selection/deck reads and gate unsupported
  MCP/compaction/control surfaces instead of returning fake healthy snapshots.
- Keep tabs process-local initially. Local deck revisions are not server journal
  watermarks. Fresh TS session identities must have an accepted `ses` prefix;
  native `s-tui-*` identities cannot be sent unchanged.
- Keep cancellation/release responsive while requests are pending. Queue or
  frame overflow is explicit failure, never silent event loss.

### 4. Functional slice and semantic mapping

Implement R3. Historical pages and settled facts come from public message APIs;
existing public host cache may supply transient presentation but is not called
an authoritative snapshot. Subscribe before hydration, bound notifications, and
merge/replace by stable source identity rather than double-appending live text.
Full settled text replaces transient data. For already-running work, an honest
loading/partial state is allowed until the correspondence is established.

Preserve session/message/tool-call/request IDs. Text/reasoning live identity is
`assistantMessageID + part kind + ordinal`: pinned 2.2.7 counts text and reasoning
ordinals separately. Public part-start events establish transient order;
historical order is `message.content` order.
Do not invent a durable part-ID registry or equate history cursors, aggregate
event sequences, and native turn/revision fields.

External fresh creation and prompt are separate effects. Confirm create
separately in the UI state; retain the created session and draft if prompt then
fails. Never auto-delete it or report native atomic fresh rejection. Prompt ACK
means durable inbox admission, not execution started. Native `SubmitFresh`
semantics and tests remain unchanged.

Fresh create can carry initial agent/model together. Existing-session setters
and prompt are separate operations; do not claim atomic captured selection.
Display backend-resolved values, including legitimate backend plugin resolution.

Permissions and forms retain exact external request identity; native numeric UI
IDs require an explicit mapping, not invented authority. Unsupported forms remain
pending with a visible reason and safe return to stock TUI; navigate to a real
session when appropriate, never invent a session for a global/unattached form.
Do not send reply/cancel/grant merely because the bridge closes.

### 5. Fail-stop, not a new reconnect framework

The captured plugin client does not automatically follow host API replacement.
On IPC EOF/overflow, stream failure, detected host reconnect/rebind, or ambiguous
mutation result, end the bridge and return stock UI with a truthful notification.
Do not resend effects or implicitly interrupt backend work. Distinguish initial
`server.connected` from later markers; public APIs do not expose every instant
of host disconnection. Never independently discover a different backend.

### 6. Validation and delivery

Use an isolated pinned TS service with the qualified frontend lifecycle candidate
and scripted local provider under the
required loopback-only isolation. Check enabled/disabled startup, the actual
Rust binary, admission/stream/tool/permission/question/cancel, settled history
reopen, and failure/no-replay. Tests must not use the user's service, config,
credentials, sessions, or provider budget.

Planned commands (new package scripts/test targets are implementation artifacts;
none below has been executed for this goal yet):

```sh
(cd plugins/oc2-rust-tui && bun test && bun run typecheck)
CARGO_BUILD_JOBS=4 cargo test -p oc-adapters --lib --locked opencode_bridge -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo test -p oc-tui --lib --locked -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo test -p oc2 --test opencode_bridge_pty --locked -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo clippy -p oc-core -p oc-adapters -p oc-tui -p oc2 --all-targets --locked -- -D warnings
CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 bash scripts/test.sh
CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2 --locked --release
git diff --check
```

Choose relevant existing core/CLI/native regression filters during implementation;
the full offline gate runs once at final integration, not on each iteration.
Review the final diff and fix proven in-scope defects, repeating affected checks.
Update mode/setup docs and examples, then commit and push only own scoped files
to verified origin/current branch. Preserve foreign `progress/`. Report exact
commands/results, reviewed revision, commit, and `target/release/oc2`. User live
installation/visual qualification is separate; do not claim it from offline PASS.

## Change Envelope

- New plugin package/tests: `plugins/oc2-rust-tui/` (outside auto-discovery).
- Bridge owner: `crates/oc-adapters/src/opencode_bridge/` and its tests/module
  registration; existing dependencies first.
- Entry/lifecycle: `crates/oc/src/{cli,bootstrap,tui_cmd}.rs`, directly affected
  TUI loop/control modules and actual-binary integration tests.
- Typed boundary/presentation: relevant `crates/oc-core/src/core_app.rs`, query,
  admission and identity DTOs; `crates/oc-tui/src/app*` and affected menus/controls.
- Necessary package/lock updates, scoped documentation/examples/provenance and
  this goal. Do not rewrite renderer/syntax/layout or native runtime/storage.
- Minimal upstream candidate artifacts and reproduction/build instructions under
  `plugins/oc2-rust-tui/upstream/`. Expected upstream owners include the TUI app
  shutdown/terminal lifecycle, public plugin TUI context/host adapter and directly
  related tests/docs. If the actual owner is in OpenTUI, record the exact pinned
  dependency owner and smallest required change before editing. This is approval
  for the lifecycle hook, not a general renderer redesign or new server API.
- Proven necessary dependency owner expansion (2026-11-12, R6): OpenTUI 0.8.1,
  npm `gitHead` `9a09079c0c7ad0fb2d55404f66a5c453204328dd`, native terminal
  resume in `packages/native/src/session.zig` and ANSI/input-mode helpers plus
  the nearest native owner regression. Existing tracked-state restoration does
  not clear external child modes 1004/1015. Add only resume-time normalization
  before re-enabling host features; build its actual native library and embed it
  in the isolated candidate. Keep a separate pinned patch/provenance under the
  same upstream artifact directory. No renderer redesign or per-frame work.
- Apply candidate artifacts to a disposable pinned source test project under
  approved TMPDIR for checks/build/PTy runs. It is not a new primary worktree or
  a mutation of `.local/opencode`. Preserve provenance and applicable upstream
  AGENTS/testing rules; do not hand-edit generated clients.
- Additional services, auth layers, persistent stores/migrations, public APIs,
  arbitrary backend frameworks, upstream patches beyond R6, or broader parity require
  explicit scope approval, not an agent's speculative recommendation.

## Current Checkpoint

- Resolved: R1–R6. The single authorized restoration and scoped artifact checks
  succeeded. Both the normal Cargo output and a separate preserved release copy
  are available with the same verified hash and recorded source provenance.
- No remaining implementation/verification checkpoint. This factual goal/README
  closeout records delivery; no source change, further broad gate, source review,
  installation or parity work is needed.

## Current State

- Resolved: R1–R6. The eight-case synthetic functional matrix, subsequent targeted
  corrections, native gates and historical release artifact checks passed. Implementation
  `d3979661528540f91d784466f12e488dc46a7c70` is pushed to verified `origin/master`;
  documentation closeout `ea180863f985cbe7651de5de259b6de2a888e3a8` was also pushed.
  The release artifact was subsequently restored and preserved as recorded below;
  not whole-product READY.
- Current Sol corrections: capture the actual Home composer-agent for fresh
  create (including `/new`), keep question drafts local to each source view, and
  refuse unsupported picker rename/delete before opening/pending state. The prior
  binary reproduces the missing Plan on the second session and the lost question
  draft. Eight source UI regressions and four adapter regressions now pass. The
  rebuilt selection case `oc2-functional-rc1s7wxq` verifies Plan on fresh create;
  tools case `oc2-functional-cvf5zw2p` verifies an already-pending question, retained
  partial answers across a neighboring tab, and the exact final `{q0:B,q1:A}`.
  Both restore termios/cursor and leave no Rust orphan. Incremental Sol review
  accepts all three corrections at source tree `f7358cc0759b417ff0d15f54627cd9a10101fa03`.
- Full offline gate first stopped before test execution on two exhaustive native
  test event matches. Explicit unexpected-source panic arms preserve those tests;
  the remaining `scripts/test.sh` stages were resumed on the corrected/reviewed
  code using the same Cargo/Rustup and cleared-XDG environment. `cargo fmt --all
  -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings`
  pass (Clippy 13.87s). With `CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4`, nextest
  `run --workspace --locked --no-fail-fast` passes all 2749 selected tests across
  48 binaries, with 26 configured skips, in 292.004s. The preceding cold test
  compilation took 19m28s, not a hung test body; preflight had over 8 GiB available
  RAM and 93 GiB free disk. `cargo test --workspace --locked --doc --no-fail-fast
  -- --test-threads=4` passes (no doctests); debug build/help/version pass
  (`oc2 0.2.0`). New unit modules
  live at owner/tests.rs with unchanged discovery names; no assertions or workloads
  were removed.
- Historical release evidence: `CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2 --locked
  --release` passed after 15m50s cold dependency compilation. At verification time,
  `target/release/oc2` passed `--help` and `--version` (`oc2 0.2.0`), SHA256
  `393680cea4d60ede9a51126b42e7daa93bc6e07be722e65638bb5fd8e4ec1ba8`.
  The existing functional harness with `--oc2 target/release/oc2` passed `basic`
  and `lifecycle`; artifacts are `/home/opencode/.cache/opencode-tmp/oc2-functional-rwlo3xcu`
  and `oc2-functional-ye64xmx_`. Both restore termios/cursor with no owned orphan;
  basic also checks settled Rust reopen and stock history, and lifecycle checks
  actual child crash/re-enable/disable/parent shutdown. These are synthetic
  qualification, not live/visual/high-Hz measurements.
- Prior acceptance discrepancy: both sessions found the root `target` absent.
  Cargo metadata still selected `/home/opencode/ai/oc/target`, target overrides
  were unset, and no target configuration, active hook or matching cached release
  explained another location. Historical verification logs remained genuine, but
  did not establish later availability. No responsible actor or deletion mechanism
  was identified; do not describe the loss as a proven Cargo relocation or a fixed
  cleanup mechanism. The read-only investigation and superseded stop are retained
  in history and commit `3aa571a0fef6a300ade1612ca3517650f266cda9`.
- Authorized restoration: preflight confirmed UID 1003, only foreign `progress/`,
  unchanged accepted source, no Cargo/compiler process, over 8 GiB available RAM
  and over 126 GiB free disk. Exactly one `CARGO_BUILD_JOBS=4 cargo build -p oc2
  --bin oc2 --locked --release` in `/home/opencode/ai/oc` passed in 14m05s. Neither
  target override, clean/deletion, source change nor broad gate rerun was used.
  Build HEAD was `3aa571a0fef6a300ade1612ca3517650f266cda9`; implementation source
  remains `d3979661528540f91d784466f12e488dc46a7c70`, accepted semantic tree
  `f7358cc0759b417ff0d15f54627cd9a10101fa03`. Differences from that tree are only
  this goal and the plugin README.
- Current release artifacts (separate files, not an installation):
  `/home/opencode/ai/oc/target/release/oc2` and
  `/home/opencode/.cache/opencode-tmp/opencode/rust-tui-plugin-v227/artifacts/oc2`.
  Both pass `--help` and `--version` (`oc2 0.2.0`) and have SHA256
  `393680cea4d60ede9a51126b42e7daa93bc6e07be722e65638bb5fd8e4ec1ba8`, matching
  the original release. The copy was created exclusively in a new owned directory,
  byte-verified and fsynced without overwriting any unrelated artifact or deleting
  the original. `artifacts/provenance.json` beside it records both absolute paths,
  source/review identifiers, build command, version, hash and verification time.
- Fresh scoped verification used the preserved exact release copy with the existing
  `functional.py --opencode <pinned candidate> --oc2 <preserved copy> --screen
  <existing decoder> --case basic` and `--case lifecycle`; both pass. New artifacts
  are `/home/opencode/.cache/opencode-tmp/oc2-functional-g4wd70o5` and
  `oc2-functional-2vbk6qky`. They restore termios/cursor and leave no owned orphan;
  basic also verifies settled Rust/stock history, and lifecycle exercises actual
  child crash/re-enable/disable/parent shutdown. No disappearance recurred during
  these checks. Prior native gate/source review remain current and were not rerun.
- Final source review closes at the accepted semantic tree `f7358cc0759b417ff0d15f54627cd9a10101fa03`:
  all three findings remain fixed, native gate evidence is checked, and no blocking
  defect remains. Subsequent changes only record these results in the goal/README;
  they do not require repeating already-current behavioral gates.
- Current owner checks: `CARGO_BUILD_JOBS=4 cargo test -p oc-tui --lib
  app::opencode::tests --locked -- --test-threads=4` passes eight tests, including
  Home-agent capture and unsupported picker rename. Adapter `--lib opencode::tests`
  passes four transport/deck tests, including unknown-effect/no-interrupt, priority
  viewer release, fragmented reads and the saved session in the final return frame.
  Test bodies take 0.00s/0.01s; the latest cold compilation takes 3m55s/7m06s.
  The recorded debug build completed in 1m02s. External `plugin-check` project
  `bun run typecheck` passes (191 files); runtime uses the already-hosted Effect
  schema, not an unavailable runtime `@opencode/client` import or a new dependency.
- The fixture owns its rootless loopback namespace, drops capabilities, then
  enters bwrap. Product processes have nonzero UID, zero capabilities and no
  external interface/route. Runtime data/logs remain outside the worktree. Local
  deck CAS, actual opened Location and return route are qualified; no native Db
  or independent backend discovery is used.
- The held-stream probe exposed a real identity collision: the first reasoning
  and first text parts both have public ordinal 0 in pinned
  `session/runner/publish-llm-event.ts::fragments`. A number-only key merged text
  into hidden reasoning. The corrected transient source projection uses typed
  identities and public starts, not a fabricated durable part registry or a
  renderer change. The already-active unfenced-delta restriction is preserved.
- Full matrix before the incremental review corrections: `for case in basic tools selection history controls
  unsupported lifecycle parent-loss; do python3 plugins/oc2-rust-tui/test/functional.py
  --opencode <candidate-ELF> --oc2 target/debug/oc2 --screen <fixture-screen-ELF>
  --case "$case" || exit; done` passes all eight cases. Artifacts under
  `/home/opencode/.cache/opencode-tmp/`: `oc2-functional-nf3x63yt`, `rup6_li_`,
  `fxpllrru`, `3gbw1nz8`, `hr2avwam`, `0fwsktby`, `4cqut9_e`, `sft2q0x5`
  (the latter seven use the same `oc2-functional-` prefix). Controls prove text
  visible during held generation, queue cancellation without interrupt, and the
  existing two-Esc guard. Parent loss proves viewer cleanup without implicit
  backend interruption. Basic proves the same settled history in reopened Rust
  and resumed stock UI. Native bounded screen decoding is text-only evidence,
  not styled/perceptual/high-Hz qualification.
- Stock provenance: official v2.2.7 linux-x64 archive downloaded outside worktree;
  GitHub release digest
  `89a6cee227b54bc011179c528a5e63176032462b1a5e21723b616801cddc2ca7`
  verified. Its ELF SHA256
  `589253d36810f21217edd51545cd435b328cdc3cf4f451fbe0cce6f88afcf5e1`
  matches the installed binary. Stock ordinary/negative probes pass, but the
  shutdown-crash baseline below fails. It has not been replaced with a candidate.
- Known stock defect: final `--case shutdown-crash` returns 1, `cursor left hidden`.
  Current artifact: `/home/opencode/.cache/opencode-tmp/oc2-plugin-pbwep3df/`.
  Sequence: renderer.destroy → cleanup.begin(destroyed=true) → child.crash →
  child.exit(7) → cleanup.end. `terminal_restored=true` is termios only;
  `cursor_shown=false`. The simplified failure injects no late terminal writes:
  the same non-unwinding failure as the ordinary crash probe occurs during
  parent shutdown instead of via the keyboard. Even `renderer.once("destroy",
  stop)` cannot join before destruction or resume the destroyed renderer.
- Prior Sol review confirmed that stock blocker and found no remaining defects
  in the original qualification artifacts. It is not review of the new candidate.
- Candidate provenance and complete reproduction commands are in
  `plugins/oc2-rust-tui/upstream/README.md`. Source/toolchain archives were verified;
  disposable sources are under `/home/opencode/.cache/opencode-tmp/opencode/rust-tui-plugin-v227/`.
  The tested TUI-only dev ELF is `source/packages/cli/dist/cli-linux-x64/bin/opencode`,
  SHA256 `a33a0538a700e72889255b50890fc0e369839f06ed101ada56b3c67adfadf1f0`.
  Its rebuilt OpenTUI library SHA256 is
  `174e44b1120dc3ff2a7e2a05099879c04236de31dda5643900086cd0192af874`.
  It is not an official release or qualified Web UI build.
- Current PTY evidence: the existing `lifecycle.py --opencode <candidate-ELF>
  --child <fixture-ELF> --candidate --case <case>` passes `shutdown-crash`, `normal`,
  `crash`, `disabled`, `disable`, `reload`, `sighup`, `slow-sighup`, `sigterm`,
  `sigint`, `sigkill`, `resize`, `missing`, `bad-hello`, `silent`, `session`, `stuck`
  (session also runs seed). Original fault and assertions are retained. Primary
  artifact `/home/opencode/.cache/opencode-tmp/oc2-plugin-hx7c80n8/` records
  host.shutdown → cleanup.begin(destroyed=false) → child.crash → child.exit(7) →
  cleanup.end(destroyed=false) → resume → renderer.destroy, restored termios/cursor/alternate
  screen, explicit child-mode reset commands and no orphan. Command audit is not
  a universal emulator-state model. The bounded stuck-child artifact is
  `/home/opencode/.cache/opencode-tmp/oc2-plugin-9udodavo/` (SIGKILL then join before
  destroy). Parent SIGKILL separately exercises child IPC-loss cleanup.
- Ordinary return/crash/negative startup retain actual stock Ctrl+P → Commands
  keyboard/paint probes. Extra FDs, captured-client location, an actual initial
  session route and suspended resize isolation pass. No generation was performed.
  The child reuses TerminalGuard, not the full Rust UI or a native backend.
- Owner checks: isolated `bun test --timeout 30000 test/app-lifecycle.test.tsx
  test/plugin-keymap.test.tsx` from pinned `packages/tui` passed 44 tests/287
  assertions in 32.14s before the review correction. The new foreign-suspension
  regression failed first (unexpected resume call); after correction, the affected
  `bun test --timeout 30000 test/app-lifecycle.test.tsx -t SIGHUP` passes 2 tests/9
  assertions in 4.80s. `bun run test:native -j4` from pinned OpenTUI `packages/core`
  passes 2122 tests, 9 upstream skips (native runtime 2m); the focused resume
  regression previously failed without the 8-line owner fix and passes with it.
  OpenTUI root `bun run build` passes with unchanged ABI (259 symbols/93 records).
  Scoped TUI/plugin typechecks and canonical root `bun run check` pass: 34 tasks,
  SDK docs 0 problems, lint 0 errors (550 warnings). The first canonical attempt
  failed because system Node 20 cannot load oxlint's TS plugins; rerun with the
  verified disposable Node 24.15.0, matching upstream's Node 24 setup, passes.
- Initial candidate Sol review found one P2: app-level resume inferred ownership
  from a globally suspended renderer, which could reacquire the built-in editor's
  terminal. The correction removes that generic resume. A registered handoff
  callback now stops/joins and awaits the host renderer only for its own completed
  suspension; no plugin ANSI reset or editor lifecycle rewrite was introduced.
  The original ordering/failure regression still passes. Scoped Sol re-review
  accepts the corrected candidate and removes P2; no remaining blocking findings.
- Fixture checks: latest `CARGO_BUILD_JOBS=4 cargo build --offline --manifest-path
  <scratch>/Cargo.toml` and scoped Clippy pass (6.64s/1.62s warm); Rust formatting,
  Python syntax and whitespace pass. Canonical patches apply with zero fuzz to
  pristine pinned files and byte-match all 7 OpenCode/3 OpenTUI tested files.
- User configuration/service, credentials, oracle refs, Rust renderer/frame-rate
  features and foreign `progress/` remain untouched.

## Checkpoint History

- 2026-11-12: R1 — saved approved plan and execution boundaries. R2–R5 pending.
- 2026-11-12: R2 checkpoint — reproducible isolated stock-2.2.7 composite-plugin
  PTY fixture added; ordinary, negative startup, context and minimal Rust-child
  probes pass. Shutdown/error fault exposes missing final terminal reset;
  earliest public destroy-listener attempt also fails. Sol confirms R2 blocker.
- 2026-11-12: User selected the recommended long-term awaited upstream hook and
  authorized implementation/testing. Added R6, reopened execution, retained the
  stock failure and all original functional outcomes; no fallback downgrade.
- 2026-11-12: R6 candidate builds; owner ordering regression passes. Actual PTY
  now restores cursor/raw/alternate-screen after the original fault but reveals
  missing external input-mode normalization in OpenTUI 0.8.1. Expanded only to
  that exact native resume owner and its compiled artifact; R3 remains gated.
- 2026-11-12: Rebuilt exact native dependency normalizes external modes at resume;
  the unchanged shutdown-crash fault and all 16 other candidate cases plus seed
  pass. Owner package/native and canonical static gates pass. Finish candidate
  Sol review, then proceed to R3; no production adapter is claimed yet.
- 2026-11-12: Sol identified foreign-suspension ownership regression; reproduced
  it, removed app-level resume and restored through the handoff owner's awaited
  callback. Rebuilt candidate passes both affected owner tests, all 17 PTY cases
  plus seed and canonical static check. Same reviewer re-review is next.
- 2026-11-12: Scoped Sol re-review accepts R6/pre-adapter R2 after the ownership
  fix. Resume R3 implementation under the existing approved contract. This is not
  acceptance of production-plugin integration, the full Rust UI or product readiness.
- 2026-11-12: The actual plugin and Rust UI pass all eight isolated functional
  cases. A real held stream exposed separate text/reasoning ordinal namespaces;
  typed transient identities and canonical ordered hydration fix it without a
  renderer or scheduling change. Production lifecycle and source authority pass.
- 2026-11-12: Sol found three uncovered R3 defects: Home-agent capture, question
  draft ownership and unsupported picker rename. Minimal owner corrections pass
  eight UI/four adapter tests and the extended selection/tools PTY cases. Same
  reviewer accepts the incremental diff; finish R4/R5 native gates and delivery.
- 2026-11-12: R4 native integration passes: strict workspace/all-target Clippy,
  nextest 2749 pass/26 configured skips, doctests and debug CLI smoke. The initial
  full gate was resumed at its failed stage, not rerun wholesale. Release checks
  and authorized delivery remain.
- 2026-11-12: The release build, help/version and actual release-binary basic and
  lifecycle probes pass. Final semantic source review is closed without blockers;
  only the authorized scoped commit/push and delivery closeout remain.
- 2026-11-12: R5 delivery — conventional implementation commit
  `d3979661528540f91d784466f12e488dc46a7c70` pushed normally from the verified
  baseline to `origin/master`, followed by the documentation closeout. Reviewed
  source code did not change; artifact availability was not rechecked at commit time.
- 2026-11-12: Acceptance found the promised root release artifact missing despite
  preserved successful build/hash/PTY logs. Read-only investigation confirms the
  default target path is absent and finds no matching cached release or configured
  relocation. Cause is not established. Reopen R5 and mark this goal blocked;
  preserve historical checks and stop affected artifact mutation. Correct stale
  Current State statements; no production code or tests change.
- 2026-11-12: Scoped acceptance clarified that unidentified disappearance alone
  does not prohibit a safe in-scope rebuild. Authorized one standard release
  restoration plus a second verified scratch copy, existing basic/lifecycle
  smokes and factual documentation only; no broad gate or source review rerun.
- 2026-11-12: R5 restoration — that one release build passes in 14m05s, producing
  the original SHA256 at the normal root path. A separate owned scratch copy and
  provenance record preserve the exact binary; both help/version and fresh
  release basic/lifecycle probes pass. Close R5 without source changes or a broad
  rerun. The unexplained prior disappearance is not claimed to be diagnosed.

## Completion

- Resolved outcomes: R1–R6 verified. The single authorized restoration re-establishes
  release availability with two verified copies and fresh scoped checks.
- Delivery: `d3979661528540f91d784466f12e488dc46a7c70`, pushed to verified
  `origin/master`; final source review accepts the semantic code at tree
  `f7358cc0759b417ff0d15f54627cd9a10101fa03`, with only factual docs afterward.
- Current artifacts: `/home/opencode/ai/oc/target/release/oc2` and
  `/home/opencode/.cache/opencode-tmp/opencode/rust-tui-plugin-v227/artifacts/oc2`.
  Both are verified as `oc2 0.2.0`, SHA256
  `393680cea4d60ede9a51126b42e7daa93bc6e07be722e65638bb5fd8e4ec1ba8`;
  native gate 2749 passed/26 configured skips and actual release basic/lifecycle
  probes passed, including the fresh preserved-copy probes above. Source provenance
  is recorded beside the copy and in Current State. Exact commands, investigation
  and upstream/functional evidence are above; no current artifact absence remains.
- Constraint/diff closure: only the 54 authorized source/test/docs/plugin paths
  were delivered; foreign `progress/` is untouched. No Rust dependency, storage
  migration, native backend startup in source mode, installed host/config/service
  mutation, or renderer/pacing downgrade was introduced.
- Limit: upstream acceptance, automatic future-version compatibility, complete
  feature/high-Hz parity and live readiness are not claimed by synthetic qualification.
