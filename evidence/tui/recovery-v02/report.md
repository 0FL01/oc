# T44 recovery V02 — application metadata and durable presentation

## Result

Implemented on parent HEAD `4c1a0bc` (2026-09-22), as an uncommitted V02 slice.
Four Rust 2024 crates and application ownership are preserved. No planning,
progress, checkpoint, GOAL, baseline, dependency or production-model changes.
The pre-existing untracked recovery ZIP is untouched. Parent owns delivery.

- `oc-core::queries`: additive session title, model/provider display names,
  optional declared tariffs, and safe turn/part DTOs. Exact routing IDs remain
  separate, including IDs with multiple slashes. Unknown price is not free;
  both explicitly zero input/output tariffs are required for the `Free` label.
- Application queries project existing session metadata and durable records.
  Runtime journals pin the selected agent/model label, measured footer data,
  public reasoning summaries, and ordered references to canonical assistant
  messages and existing tool operations. No duplicate text/tool transcript is
  stored. Encrypted continuation stays in the provider journal, outside UI DTOs.
- Tool intent and its display reference are one SQLite transaction before the
  effect. Outcomes continue to use the existing durable outcome/journal boundary.
  Crash recovery projects the actual `unknown` state without inventing an output
  or repeating the tool. The new index supports turn/message anchor lookup.
- History replay uses the existing reasoning/card/footer renderers. Completed
  live turns reload the same application page. Byte accounting includes parsed
  card data; expanded parts count correctly for history scroll anchoring, and
  projecting parts no longer repeatedly clones the aggregate message text.
- Structured tool input is kept intact within the serving budget, rather than
  truncating JSON at the 2 KiB output-preview limit. A real >3 KiB `apply_patch`
  request exposed that replay bug; its restored card now retains the file/diff.
- Missing Responses usage stays unknown. A turn with one missing round's usage
  does not display a falsely complete aggregate from the other round.
- A narrow native title policy runs by default, with an optional configured
  `title` profile, through the normal Responses adapter after a completed turn.
  It has no tools, uses the configured full `provider/model#variant` (the same
  resolver as child requests) or effective selected model/variant, and writes the existing
  `sessions.title` only if absent. Provider failure/empty output leaves untitled;
  existing/child titles win. Cancellation and application queries remain serviced
  by the worker select loop while the title request runs. The ancillary request
  has a 10-second deadline, 256 output-token budget, 8-KiB user-input preview and
  100-character title limit. Canonical assistant `output_text` works without
  deltas. Invalid title model/variant configuration is rejected before acceptance.
  A malformed explicit title definition is also a composition error, rather than
  being mistaken for an absent profile and silently replaced by the default.
- `HistoryTurn.part_states` exposes durable `(turn.id, sequence)` identities,
  original order, exact status and truncation/input-omission state. The same
  bounded DTO is emitted as `TurnPresentation` at tool and terminal checkpoints.
  The application integration test compares its terminal live event exactly to
  queried replay; stale presentation events are ignored after Location changes.
- Fixed the reproduced tool → second-round reasoning reorder in live completion.
  Footer categorical agent slot is pinned by the generation, independent of a
  later catalog's ordering; failed/cancelled/incomplete/unknown remain distinct.
- Serving overflow and legacy availability are explicit DTO state and visible
  notices. Oversized structured input keeps its operation id and existing
  operation-query access; `/cards` remains the tool-card entry point. No missing
  historical reasoning or ordering is invented. Live preview eviction is marked.
- A→B→A now keeps the application's pinned config environment. The new test
  exposed `switch_target` reloading ambient process HOME instead of the explicit
  `spawn_with_env` environment; the reload now receives `composition.parent_env`.
- Final re-review: `Effective::set_agent` now resolves full
  `provider/model#variant` references with the shared child/title resolver and
  validates the variant before publication. Existing exact bare-ID profile
  aliases remain compatible. The actual PTY agent switch asserts the configured
  model, profile prompt and variant in the subsequent native provider request.
- Primary and child `TurnLane`s now carry their own generation-pinned categorical
  slot. Primary catalog entries expose that slot explicitly; all admitted
  profiles participate, including subagents. A real child-generation test verifies
  the stored child slot survives later catalog replacement/reordering.
- The `auto` marker is now explicitly capability-mapped rather than left as an
  unavailable-data TODO: [decision and source evidence](auto-capability.md).
  Native snapshots report Unsupported; only an actual Enabled snapshot renders
  `auto`. No permission widening or interactive approval backend was introduced.
- Model limits retain compatibility values plus `context_known`/`output_known`;
  absent limits remain null in the TUI catalog projection. Real A/B fixture
  metadata distinguishes known limits from missing metadata for future V03 use.

### Source alignment and explicit assumptions

Targeted source was inspected in the supplied sparse upstream checkout at
`/home/opencode/.cache/opencode-tmp/opencode/upstream-v2`, HEAD
`2670273ff17da96f85c5826ced57aa1b368754fa`:

- `packages/tui/src/component/dialog-model.tsx`: model names and provider
  names/fallback IDs; line 214 requires declared costs before `Free`.
  OC deliberately requires both known tariffs to be zero, as V02 specifies.
- `packages/tui/src/component/prompt/metadata.tsx`: model/provider metadata.
- `packages/tui/src/component/session-tabs.tsx`: session title, tab title, then
  `Untitled session` fallback.

The sparse checkout exposes TUI/theme source, not a locally available backend
title implementation. A targeted Git-object search for backend title code timed
out at 20 s and was not repeated. The policy above is an explicit narrow OC
implementation of the owner's requested default title contract, not a claim of
upstream's entire title-agent orchestration policy.
No guessed title or screenshot-derived production metadata was introduced.

## Checks

### Final evidence

- **Latest re-review:** full `cargo test --locked --workspace --no-fail-fast`
  passed **436 tests**, with **4 unchanged live ignores**. Subsequent final
  fmt-check, workspace clippy (`-D warnings`), locked build and diff-check all
  exited **0**. This includes the native full-reference agent switch, child
  generation metadata, auto capability mapping and model-limit known flags.
- **After independent review corrections:** `cargo fmt --all && cargo test
  --locked --workspace --no-fail-fast && cargo clippy --locked --workspace
  --all-targets -- -D warnings && cargo build --locked && target/debug/oc --help
  && git diff --check`: exit **0**, **435 passed, 4 existing live ignores**.
  The final live-overflow notice and malformed-title-definition follow-ups are
  checked below; no geometry changes.
- The records below describe the earlier implementation pass and remain as
  historical evidence rather than replacing its failed attempts.
- `cargo test --locked --workspace`: exit **0**, **430 passed, 4 existing live
  ignores**. This run includes real PTYs, V01 cancellation/lifecycle regressions,
  provider/patch/store faults, subagents, soak, memory bounds, Location and DCP.
  It preceded the final scroll-count and intact-tool-input follow-ups below.
- After those follow-ups: `cargo fmt --all` → recovery/durability tests →
  `cargo test --locked -p oc-tui --lib` → workspace clippy → locked build →
  `git diff --check`: entire chain exit **0**; **2 binary integration tests and
  92 TUI tests passed**. The full workspace was not redundantly rerun afterward.
- `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets
  -- -D warnings`, `cargo build --locked`, `target/debug/oc --help`, and
  `git diff --check` also passed together earlier (exit **0**).

`crates/oc/tests/recovery_v02.rs` launches actual `oc run` and actual PTY prompt
submission with isolated
HOME/XDG/project/data and a local Responses fake. Each profile executes real
`read` and `apply_patch`, then a real title request. A fresh application owner
checks durable title, agent, model/provider names, price and usage, ordered public
reasoning, completed/failed read, and patch cards. A second actual binary starts
`oc tui --session ...` in a 120×40 PTY and exits cleanly via `/quit`.
The Changed profile starts in a PTY, submits the prompt, observes `Thinking`
before the final answer, then restarts that binary session. All profiles now
have two reasoning rounds separated by real tool cards. The fake verifies exact
function-call/result pairing, actual read contents/error and successful patch
result hashes before returning the final answer. Named/Partial use a configured
full model/variant title profile; Changed exercises the default. Titles are sent
only in canonical output, with no title delta. Invalid title profile refusal
leaves durable history unchanged. Every successful profile makes exactly three
provider requests, including its tools-empty title request.

The Named/Changed profiles mutate names, agent, title, known/unknown usage and
free/unknown price; the visible frames differ. Both compact PTY replays show
reasoning, read and patch cards, title, model and final text. The third profile
adds paid price, partially unreported usage, a real failed read, and a >3 KiB patch;
its DTO/parsed card and actual filesystem mutation are asserted, and its restarted
PTY shows title/model/final text. Every profile additionally checks A→B→A with
local model/provider overrides, retained global agent, rejected cross-Location
history, ignored old-turn reasoning/tool/completion events, and unchanged durable
presentation upon return. Secret and encrypted-reasoning sentinels are absent
from the history projection.

The existing real SIGKILL durability test now opens a fresh application and asserts
the unknown tool, absent output, pinned model label and unknown transcript card.
`storage::tests::turn_tool_reference_and_intent_are_atomic` injects an UPDATE
failure and verifies rollback of the intent before successful retry.

### Command history, including failures

All Cargo failures below exited **101**; successful commands exited **0**.
Repeated shorthand `recovery` means
`cargo test --locked -p oc --test recovery_v02 -- --nocapture`.

1. Initial `git status --short && git rev-parse --short HEAD`: 0; base HEAD and
   pre-existing ZIP verified. Subsequent Git status/diff/stat/rev-parse reviews: 0.
2. Recovery `--no-run`: 101, initial red DTO contract (new types/fields absent);
   also corrected the fixture's `TuiState::new` argument.
3. `cargo check --locked --workspace --all-targets`: 101, missing new title arg
   and additive DTO literal fields; after updating callsites: 0.
4. Recovery: three successive failures at the real read-card assertion while
   constructing the fixture (wrong `filePath` instead of `path`, then singular
   `permission` instead of `permissions`, with one diagnostic rerun). Corrected
   the fake/config; no runtime permission weakening.
5. Recovery: 101, test expected expanded public reasoning text although the
   renderer shows a collapsed `Thought:` title. Exact reasoning remains asserted
   in the DTO; visible assertion was corrected to the existing renderer contract.
6. Recovery: 101, unreported usage incorrectly became `(0,0)`; fixed provider
   parsing. Recovery then passed: 1 test / two profiles.
7. `cargo test --locked -p oc-tui --lib`: 101 (88 passed, 3 old label assertions
   failed). Updated exact footer/picker expectations to real configured names.
8. `cargo test --locked -p oc --test durability -- --nocapture`: 101, crash replay
   lost the configured model name. Pinned labels in the initial journal.
9. `cargo fmt --all && cargo test --locked -p oc --test recovery_v02 --test
   durability -- --nocapture`: 0, 2 passed.
10. Workspace clippy: 101, PTY unsafe-block safety comment needed to immediately
    precede the block inside `assert_eq!`; fixed the comment placement.
11. `cargo test --locked -p oc-tui --lib && cargo test --locked -p oc-adapters
    --test runtime`: 0, 91 + 35 passed.
12. Upstream `git ls-tree` and `git rev-parse` commands: 0. Targeted
    `git grep -n 'ensureTitle\|async function.*[Tt]itle\|generateTitle' HEAD --
    packages/core/src/session packages/server/src/session`: tool timeout at 20 s;
    no shell exit code supplied. No broad retry/download was performed.
13. Recovery after adding partial-round usage/paid-price checks: 101, aggregate
    incorrectly retained `(321,17)` after an unreported second round. Fixed.
14. `cargo test --locked -p oc-adapters --lib
    turn_tool_reference_and_intent_are_atomic`: 101, intentional compile-red
    before the transactional method existed. After implementation, the combined
    fmt + binary recovery/durability + this test chain: 0 (2 + 1 passed).
15. Fmt + recovery with A→B→A: 101, ambient config environment replaced the pinned
    environment. Fixed reload ownership. Next run: 101, B fixture provider
    replacement lacked its own options (provider replacement is the existing
    composition contract). Supplied isolated fake options. Next run: 0.
16. Workspace clippy: 0. First `cargo test --locked --workspace`: 101 at
    `aud29_pty_panels_change_runtime_state`, which expected the old bare model ID
    in the model row. Updated to exact `T39 alt · fixture`; wire ID assertions stay.
17. Next workspace test: 101 at `aud38_location_switch_is_one_lifecycle`.
    The failure's decoded screen contained the exact refusal, but raw incremental
    VT bytes split it. Switched that assertion to the existing decoded-screen
    `wait_screen_row`, preserving the exact required refusal text.
18. Next workspace test: 0, 430 passed / 4 ignored. Fmt-check + clippy + build +
    binary help + diff-check chain: 0.
19. `cargo test --locked -p oc-tui --lib
    expanded_parts_count_as_rendered_rows_for_scroll_anchoring`: 101 (returned
    message count 1 instead of rendered parts+footer 3). Fixed projection count.
20. Recovery after adding actual patch replay: 101, fixture used `patch_text`
    instead of `patchText`; corrected it. Next recovery: 101, >2 KiB structured
    input was truncated and lost parsed patch metadata. Fixed bounded intact JSON.
21. Fmt + recovery/durability chain: 101, durable test passed, recovery's large
    patch pushed reasoning outside the fixed viewport. Repeated with 120×80: same
    failure. Repeated through normal Up scrolling: same failure. Later chained
    TUI/clippy/build commands were not reached on these runs. This is the existing
    wrapped-row viewport defect assigned to V03; see Risks. Kept two compact
    visible-replay fixtures and an additional large-patch DTO/card fixture rather
    than changing geometry or claiming that defect was fixed.
22. Final fmt + recovery/durability + TUI-library + workspace clippy + locked
    build + diff-check chain: 0; 2 binary integration + 92 TUI tests passed.
23. After writing this report, fmt-check + diff-check + status + diff-stat +
    HEAD verification: 0; HEAD remains `4c1a0bc`, with no progress/planning changes.
24. Review red tests: `cargo test --locked -p oc-tui v02_multiround -- --nocapture`
    exited 101, reproducing second reasoning inserted before the first tool round.
    Recovery exited 101, missing third request for a full provider/model#variant
    title profile. Both tests were added before those corrections.
25. Three `cargo check --locked --workspace --all-targets` attempts exited 101:
    new event match arms, canonical-output type, workspace presentation field and
    SQLite integer conversion; then remaining event/test metadata literals; then
    a test's `Option<&str>` tool-turn argument. Compiler issues corrected.
26. `cargo test --locked -p oc-tui v02_multiround -- --nocapture && cargo test
    --locked -p oc-adapters --lib v02_bounded -- --nocapture && recovery`: 0,
    one test in each target passed. Fmt + recovery after strict graph and actual
    PTY-submit assertions: 0.
27. Fmt + recovery/durability + application-event test + TUI + clippy chain:
    101 at durability's old fake (it never answered the new default title request).
    Added the exact tools-empty title request/response. Repeated chain: 0,
    2 binary tests + 1 application-event test + 93 TUI tests and workspace clippy.
28. `cargo test --locked --workspace`: 101 at configured_workspace (4 old fake
    timeouts; 2 preflight refusals passed). Added explicit bounded title responses.
    Fmt + workspace: 101 at 2 DCP fake timeouts. Added title-aware completion for
    seed and new-session turns. Next fmt + workspace: 101 at 2 remaining new-session
    DCP completions (manual/crash restart); updated those specific peers as well.
29. Fmt + `cargo test --locked --workspace --no-fail-fast`: 101, six targets:
    golden_binary script consumed title as next main turn; mcp_application had
    seven old request-count expectations; pty had an old count and interrupted
    label; pty_t39 had two timing/request-index failures; pty_t42 had one timing
    failure; responses waited without answering title. All other targets passed,
    including DCP, runtime, storage, bounded soak, 155 adapters and 94 TUI tests.
30. Fmt + six affected binary targets `--no-fail-fast`: 101; golden/pty/responses
    passed, MCP had one second-session total-count mismatch, T39 two handoff races,
    T42 one handoff race. Strict ancillary title peer preserves raw capture,
    validates shape and avoids consuming main scripts. Exact total request counts
    include titles in MCP/PTY/golden. T39/T42 main-turn accessors explicitly filter
    titles from retained raw capture; initial submission waits for the actual
    persisted-title frame, not a preterminal answer delta.
31. Fmt + MCP/T39/T42/recovery `--no-fail-fast` then TUI/clippy chain: 101,
    MCP12 and recovery1 passed; one T39 and one T42 fake panicked on BrokenPipe
    when /quit legitimately cancelled the ancillary title request. Request-shape
    assertions remain strict; the peer now tolerates that disconnect as other
    streaming fixture responders already do. Later chain commands were not run.
32. Final full chain listed above: 0, 435 passed, 4 unchanged live ignores;
    fmt, clippy, locked build, binary help, diff check all passed.
33. `git status --short && git diff --stat && git rev-parse --short HEAD`: 0;
    HEAD remains 4c1a0bc. Broader test-file changes account for the genuine default
    title request, not altered gates or hidden failures.
34. After adding a visible live-overflow notice and extending the existing flood
    test: fmt + `cargo test --locked -p oc-tui --lib` + recovery + workspace clippy
    + locked build + fmt-check + diff-check: 0, 95 TUI + 1 recovery tests passed.
35. Final review added a malformed explicit title-definition regression before
    correction. Recovery: 101, invalid mode was diagnosed but silently replaced
    by the default. Composition now refuses that selected invalid title profile.
36. Fmt + `cargo test --locked -p oc --test recovery_v02 --test configured_workspace`
    + `cargo test --locked -p oc-adapters --lib composition::tests` + workspace
    clippy + locked build + fmt-check + diff-check: 0, 7 binary + 9 composition
    tests passed. Full workspace success in item 32 predates only the two narrow
    follow-ups verified in items 34–36; no blanket post-follow-up rerun claimed.
37. Re-review red tests, added before corrections:
    `cargo test --locked -p oc --test pty_t39 aud29_pty_panels_change_runtime_state
    -- --nocapture`: 101, actual agent switch never succeeded with a valid full
    provider/model#variant reference. In parallel,
    `cargo test --locked -p oc-adapters --test subagent
    spawn_returns_child_text_and_persists_fresh_child_row -- --nocapture`: 101,
    child journal color was None instead of its generation slot 1.
38. `cargo check --locked --workspace --all-targets`: 101, fixture catalog/agent
    literals needed new explicit auto capability and color-index fields. Updated
    the fixture literals; one patch context mismatch applied no changes.
39. Fmt + `cargo test --locked -p oc --test pty_t39 --test recovery_v02` +
    adapters subagent suite + TUI library suite + workspace clippy + locked build:
    101 at clippy's needless borrow in the new marker test. Preceding tests passed
    (4 binary + 10 subagent + 96 TUI); build was not reached.
40. After adding limit-known flags: fmt + full workspace `--no-fail-fast` +
    clippy/build/check chain: 101 at the same test-only needless borrow (an earlier
    patch with repeated file sections did not retain that correction). Full
    workspace itself exited 0: 436 passed, 4 live ignores. Applied the borrow fix
    in a single-file patch; no production behavior or test assertion changed.
41. `cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets
    -- -D warnings && cargo build --locked && git diff --check && git diff
    --name-only -- GOAL.md progress planning && git rev-parse --short HEAD`: 0.
    No contract/progress/planning diff; HEAD remains 4c1a0bc. No delivery action.

Several patch applications failed context matching (including reordered sections
and differing test imports); they applied no changes
and was reapplied against current text. An exploratory read of `SAFETY.md` failed;
the actual required file `SAFETY_REGRESSIONS.md` was then read successfully.

## Risks

- Upstream interactive session autoaccept remains an unsupported native backend
  capability, explicitly represented and documented in the linked mapping
  decision. Allowed tools do not cause a false `auto` marker. This is the permitted
  V02 capability mapping, not a claim that permission-request/reply support exists.
- **V03 remains open:** a long wrapped patch can fill the fixed 20-rendered-line
  viewport; increasing terminal height or Up scrolling did not reveal earlier
  reasoning in the exploratory fixture. V02 DTOs and parsed rows retain those
  parts. Compact actual-binary replay is qualified, wide/long visual parity is not.
- Title generation adds a real provider request by default and can delay the final
  terminal event by up to its 10-second ancillary budget. Failure/empty output
  stays untitled; a later completed turn may retry. Configuration errors are loud.
  User input is bounded to 8 KiB for that title-only request; main prompt is intact.
- Legacy journals without presentation references remain text-only. Newly stored
  safe metadata is not retroactively invented for old history. Tool records still
  exist through the established operation queries. A visible legacy text-only
  availability marker distinguishes missing historical data from an empty answer.
- Serving is bounded: up to 240 parts and roughly 64 KiB of projected turn content,
  public reasoning capped at 16 KiB per block, existing 2 KiB output previews with
  size/truncation metadata. Input JSON too large for the remaining turn budget is
  omitted instead of cut into invalid JSON; operation ID/outcome remain available.
   Complete text/tool records remain durable. Full part-level pagination is not
   introduced; omissions/truncation are explicit in the DTO and rendered notices.
   Public reasoning beyond 16 KiB is not retained, and is marked truncated.
- Reasoning is journaled at existing generation/tool/terminal boundaries, not
  fsynced per delta. A crash within an unfinished stream can lose uncheckpointed
  reasoning/text. Unknown tool outcomes are covered by the real crash test.
- The title fake, failed read and crash test cover real runtime data paths; no live
  credential use or paired upstream screenshot qualification was performed. Four
  pre-existing live/real-server tests retain their declared ignores.

## Next

No concrete unresolved data-plumbing defect from either V02 re-review remains known.
Autoaccept's backend capability is explicitly Unsupported as decided above.
The documented bounded-preview, checkpoint-durability and V03 viewport limits
remain; they are not presented as full transcript/visual qualification.

Parent review the uncommitted V02 diff and documented title/serving limits, then own commit
and delivery. Continue the prescribed V03 viewport/geometry slice, including the
long wrapped-patch case recorded above, then remaining V04–V09 and paired visual
qualification. No claim of full T44, S06/S08 or product readiness is made here.
