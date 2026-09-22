# V04 interaction continuation (2026-09-22)

Base: `931792e`. T44 remains active. No commit/progress operation in this delegated
slice. Backend D13–D15 contracts are retained: absent variant is no overlay;
per-server MCP attach failures degrade visibly; cancellation/cleanup/caps remain
fatal; resource permissions only narrow.

## Sources and scope

Pinned original `2670273ff17da96f85c5826ced57aa1b368754fa`:
`packages/tui/src/component/dialog-model.tsx` (126–139 selection flow),
`dialog-variant.tsx` (Default and reserved `default`), `ui/dialog-select.tsx`
(weighted fuzzy filtering), `app.tsx` (session/new/variant aliases).
`bun.lock` pins fuzzysort 3.1.0. External oracle installed only under
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference`; no production JS host.

## Attempts (append-only)

1. `npm install --prefix /home/opencode/.cache/opencode-tmp/opencode/t44-reference
   --save-exact fuzzysort@3.1.0`: exit 0.
2. `cargo test --locked -p oc-tui --lib --no-fail-fast`: exit 101, 105 passed,
   2 failed. Intermediate implementation exposes two stale expectations:
   reopening sessions now reloads the genuine list (`LoadSessions`); searching
   `model` now also finds the new genuine `Switch model variant` action. Tests
    must assert those effects rather than assuming one match/cached sessions.
3. `node scripts/tui_capture/fuzzy_oracle.mjs`: exit 0; external pinned 3.1.0
   oracle produced 48 weighted/unweighted cases (Unicode, accents, words across
   keys, substring, camelcase and equal-score heap order).
4. `cargo test -p oc-tui fuzzy::tests -- --nocapture`: exit 0, 1 passed. Cargo
   resolved unicode-normalization 0.1.25 and unicode-script 0.5.8 for the Latin-only
   accent/UTF-16-compatible Rust port; lockfile updated.
5. `cargo test --locked -p oc-tui --lib --no-fail-fast`: exit 101, 107 passed,
   1 failed. Session-refresh consumed-input flag was incorrectly true before the
   first list snapshot. Corrected to use `sessions_loaded`; refresh still occurs
   on every opening, with the real application intent.
6. `cargo test --locked -p oc --test pty_t39 -- --nocapture`: exit 101,
   3 passed, 1 failed. The expanded V04 restart scenario reached Default's real
   request assertions but timed out awaiting the subsequent slow-stream request
   (#5); fixture-only prompt diagnostics added to distinguish a submission race
   from provider/script behavior. AUD29 separate-dialog conversion and AUD30
   busy-session refusal both passed in this run.
7. `cargo test --locked -p oc --test pty_t39 v04_raw_dialogs -- --nocapture`:
   exit 0, 1 passed. The earlier slow-turn timeout was intermittent; added a
   terminal-state idle wait before sequential prompt submissions, since streamed
   echo can precede the final event. No added sleeps or relaxed HTTP assertions.
8. `cargo test --locked -p oc-tui --lib --no-fail-fast`: exit 101, test compile
   failure from missing `CommandAction` imports in two new tests; imports added.
9. `cargo test --locked -p oc --test pty_t39 -- --nocapture`: exit 101,
   4 passed, 1 failed. New test assumed Ctrl+U clears the prompt; that binding is
   not implemented. The retained `/variants` draft concatenated with `/model`,
   opening Help. Corrected the fixture to use actual Backspace events, retaining
   the no-effect/draft behavior assertion rather than adding a fake shortcut.
10. `cargo test --locked -p oc-tui --lib --no-fail-fast && cargo test --locked
    -p oc --test pty_t39 -- --nocapture`: exit 101, first suite 109 passed,
    1 failed; PTY command not executed. The new current-focus test selected
    named `none` on its existing no-variant fixture, correctly producing no
    available dialog. Declared the named variant explicitly in that test.
11. Same chained command: exit 101. TUI suite 110 passed; PTY suite 4 passed,
    1 failed. The new-session test expected a session tab immediately, but the
    actual operation correctly returns Home (pinned behavior). Replaced the
    readiness marker with the real Home logo; the independent HTTP and SQLite
    session-count/history assertions remain required.
12. `cargo fmt --all && cargo test --locked -p oc --test pty_t39 v04_new_session
    -- --nocapture`: exit 0, 1 passed. Four actual new-session routes and
    `/continue`, disabled busy routes, exact wire effects and five stored sessions.
13. `cargo clippy --locked --workspace --all-targets -- -D warnings`: exit 101,
    one unnecessary unwrap in the fuzzy substring branch. Replaced with `if let`.
14. `git diff --check && git diff --stat`: exit 0.
15. `node scripts/tui_capture/capture.mjs --reference
    /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode
    --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output
    /home/opencode/ai/oc/evidence/tui/recovery-v04/followup-160x48 --columns 160
    --rows 48 --sample short --variants true`: exit 1. Both real provider contracts
    passed; session/Commands/Models captured on both. Original variant predicate
    failed: its terminal retained `/variants` autocomplete after the combined
    text+Enter input. Native variant capture succeeded. Runner now waits for the
    real draft to render before a separate Enter, preserving this failed attempt.
    All grid/PNG comparisons DIFFERENT; paint remains parent-owned and other
    unsupported actions/metadata remain visible.
16. `cargo fmt --all && cargo clippy --locked --workspace --all-targets --
    -D warnings && cargo test --locked --workspace --no-fail-fast --quiet`:
    fmt/clippy exit 0; workspace command hit the harness 120s timeout before
    completion. Three failures observed: MCP manual-compress PTY raw-byte wait
    for `cancelled`; T42 expected the former action-specific busy toast rather
    than the shared registry reason; V02 still asserted an unconditional model
    provider category. PTY T39 all 5 passed in this workspace run. Updating the
    two intentionally changed UI expectations; investigating the cancellation
    assertion with its actual lifecycle/DB checks retained.
17. Paired capture as attempt 15 with output `followup-160x48-v2`: exit 1,
    both provider contracts passed, all four states CAPTURED on both executables
    including Select variant. All grid/PNG comparisons remain DIFFERENT.
18. `cargo test --locked -p oc --test mcp_application
    v01_manual_compress_raw_pty_cancel_shutdown_and_retry -- --nocapture`:
    exit 101, same raw-byte `cancelled` wait. Earlier PID reaping, zero-provider,
    zero-turn and zero-message assertions passed. Replace only the byte-stream
    substring wait with the existing reconstructed-screen assertion: ratatui
    deltas omit unchanged letters shared with the new availability toast.
19. Paired captures as attempt 15 with output `followup-80x24`, columns 80/rows24,
    and output `followup-121x41`, columns121/rows41: both exit1, both actual
    provider contracts passed in each; Session/Commands/Models/Variants all
    CAPTURED for both normal executables. Every grid/PNG comparator DIFFERENT.

## Current qualification

Implementation and verification in progress; no VIS closure or whole-TUI parity
claim. Final commands, paired captures and exact remaining gaps will be appended.

## Additional attempts and completed checks

20. `cargo test --locked -p oc --test mcp_application
    v01_manual_compress_raw_pty_cancel_shutdown_and_retry -- --nocapture && cargo
    test --locked -p oc --test pty_t42 --test recovery_v02 -- --nocapture`: exit 0,
    respectively 1, 3 and 1 passed. MCP cancel/reap-before-release measured
    95.97232ms; the unchanged child reaping, HTTP, retry and SQLite assertions pass.
21. `cargo test --locked --workspace --no-fail-fast --quiet` with a 600s harness
    timeout: exit 0, **492 passed, 0 failed, 5 existing ignored**. No added ignores.
    This complete run precedes the final expanded busy-route assertions and the
    reserved-`default` availability edge-case correction below.
22. `cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets
    -- -D warnings && cargo build --locked && target/debug/oc --help`: exit 0.
23. Expanded raw-PTY busy coverage to `/model`, `/variants`, `/agents`, `/continue`,
    `/thinking`, `/effort`, Ctrl+X m/a/l as well as new-session routes, and asserted
    the persisted model/variant remains unchanged. `cargo fmt --all && cargo test
    --locked -p oc --test pty_t39 -- --nocapture`: exit 0, all 5 passed.
    `node --check scripts/tui_capture/capture.mjs && node --check
    scripts/tui_capture/fuzzy_oracle.mjs && python3 -m unittest discover -s scripts
    -p 'test_*.py'`: exit 0, Python 29 passed. Subsequent fmt, all-target workspace
    clippy and `git diff --check`: exit 0.
24. Final source review found an edge case: the original model flow tests whether
    *any* enabled variants are declared before its variant dialog removes the
    reserved `default` name. Corrected `has_variants` to inspect that declared
    list, and added assertions for a default-only catalog. This opens a genuine
    Default-only dialog, whose selection still means no overlay. Also normalized
    indentation of the new capture-runner statements.
25. Final `cargo fmt --all -- --check && cargo test --locked -p oc-tui --lib &&
    cargo test --locked -p oc --test pty_t39 v04_ -- --nocapture && cargo clippy
    --locked --workspace --all-targets -- -D warnings && cargo build --locked &&
    target/debug/oc --help`: exit 0 throughout; TUI 110 passed, actual V04 PTY
    2 passed. `node --check` on both changed mjs scripts and `git diff --check`:
    exit 0. This is the post-edge-case qualification, not a claim of a second full
    workspace run.

## Result at delegated handoff

This supersedes the earlier in-progress marker. One coherent interaction slice
is implemented and verified against pinned source and actual application effects.
HEAD remains `931792ef8009ae6ad024cf09c780db029b3c8ef2`; changes are uncommitted and
unstaged. No progress/goal/gate files were changed. Parent-owned dialog paint and
blank-cell foregrounds were not edited. The committed explicit-low shell fixture
and separate None/no-overlay test remain intact.

### Current capability mapping (supersedes only the corresponding historical rows)

| Action | Registry routes | Actual owner/effect and qualification |
|---|---|---|
| Select model | `/model`, `/models`, Ctrl+X m, Commands | Real `select_model` succeeds before deciding whether to replace with Select variant. Existing valid current variant closes directly; absent variant plus an enabled declared list opens the separate dialog. Escape from that dialog preserves the already applied model and prompt draft. |
| Select variant | `/variants`, `/thinking`, `/effort`, Commands | Exact active model + named variant goes through the same application selection/persistence path. Default sends None; declared `none` is distinct. Reopening focuses the current dot; clearing search restores current focus. Raw PTY proves fast→HTTP high, none→HTTP low, Default→no effort, named-none and Default SQLite persistence and restart focus. |
| New session | `/new`, `/clear`, Ctrl+X n, Commands | Existing `CoreAppHandle::create_session`, attach its empty history, return Home, preserve model/agent. Raw PTY exercises all four routes; exactly five durable sessions and each session's expected history are asserted. This closes the historical New session action gap, not tab-stack parity. |
| Switch session | `/sessions`, `/session`, `/resume`, `/continue`, Ctrl+X l, Commands | Refresh real application list, select actual history/session. `/continue` resumes the original session in the new-session PTY scenario. |
| Busy/unavailable actions | Shared registry availability for palette, slash and bindings | New/session/model/variant/agent/Location/manual-compress actions refuse before UI/application mutation, with a visible reason. No-variant catalog refuses Select variant visibly. PTY verifies no extra requests/sessions and unchanged stored model/variant; unit test verifies an outstanding submission receipt and draft survive modal Escape/disabled activation. |
| Remaining implemented native actions | Agent/skills/sidebar/help/quit/cards/Location/manual DCP compress | The same registry supplies aliases, completions, shortcut labels and lookup. Argument-taking Location remains slash-only; native DCP compress is not upstream provider compaction. Existing lifecycle suites pass. |

Unknown `/open`, `/projects`, `/project`, `/mcps` are not advertised/selectable
capabilities. The rest of `capabilities.md` remains the explicit gap/boundary map;
this additive report does not turn its unsupported operations into fake actions.

Dialog fuzzy membership/ranking is now a native source-derived fuzzysort 3.1.0
port, with its MIT notice retained in `crates/oc-tui/assets/fuzzysort-LICENSE`.
The checked oracle has 48 external original weighted/unweighted score and order
cases; Cargo runs them without Node/upstream. It covers accents, UTF-16, word
boundaries, cross-key words and non-stable equal-score heap ordering. Commands use
the original title weighting and 0.7 threshold. Models use fuzzy membership then
their available metadata order, matching the source's distinct model path.
The single configured-provider catalog has no connected-integration sections,
so Models no longer invents an unconditional provider heading. Provider/model
identities remain catalog-driven.

### Actual paired captures

| Directory under `evidence/tui/recovery-v04/` | Geometry | Actual result |
|---|---|---|
| `followup-160x48/` | 160×48 | Preserved failed initial original variant predicate; both HTTP contracts passed, other three states captured. Exit 1. |
| `followup-160x48-v2/` | 160×48 | Session, Commands, Models, Select variant CAPTURED on both normal executables; both HTTP contracts pass. Exit 1: all full-grid/PNG comparisons DIFFERENT. |
| `followup-80x24/` | 80×24 | Same four successful actual states and both HTTP contracts; exit 1, comparisons DIFFERENT. |
| `followup-121x41/` | 121×41 | Same four successful actual states and both HTTP contracts; exit 1, comparisons DIFFERENT. |

Each attempt retains normal executable identities, real PTY bytes, inputs,
provider contract facts, full styled-cell grids, PNGs, comparator outputs and
capture locks. The original SHA-256 is
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
These are pre-parent-paint captures and precede the final default-only edge case.
The existing source lock covers HEAD/tracked dirty diff plus the built binary;
it does not hash newly untracked Rust source separately. Preserve that provenance
limit rather than claiming a fully sealed uncommitted source bundle. Parent can
make the final paired capture after integrating paint and source delivery.

### Explicit remaining ownership / next steps

- **Parent / T44 V04:** integrate the separately owned paint correction and rerun
  actual paired captures in fresh directories with `--variants true`. Current
  comparator results are not VIS acceptance or whole-TUI parity.
- **T44 model DTO/application/UI follow-up:** release date, deprecated status,
  favorites/recents and connected-integrations semantics are not represented in
  the native picker contract sufficiently for the original sections/order/footer.
  Add real DTO/persistence support before rendering those actions or sections;
  model title tie-breaking is presently Rust lexical ordering, not JS locale
  collation. No hardcoded provider priority/model IDs were introduced.
- **Parent / T44 V04 and V05:** remaining modal mouse/full keymap and prompt/editor
  behavior, plus the other in-scope entries in `capabilities.md`, still need their
  own real effects and paired qualification. Current modal Escape/search/current
  focus and busy/pending preservation are qualified, not every upstream binding.
- **Owner boundary decisions:** integration/OAuth/config-authoring/sharing/service
  and other explicitly unsupported original capabilities remain as recorded in
  the historical map. No backend scope, D13–D15 behavior or permission contract
  was broadened in this interaction slice.

## Independent-review continuation (before checkpoint)

The previous handoff is superseded where it claimed a complete delivered
selection flow: review found global session leakage, missing per-model variant
preferences and frame-by-frame fuzzy preparation. Parent paint and its additional
blank-cell regression test are preserved. HEAD remains 931792e; no progress edits.

26. `cargo check --locked -p oc`: exit 0 after additive scoped selection API.
27. `cargo test --locked -p oc-tui --lib --no-fail-fast`: exit 101,
    109 passed/2 failed. Expected model intent is now distinct from explicit
    Default; a catalog with no variants correctly removes Switch model variant
    from Commands. Updated those two stale assertions; external fuzzy oracle
    still passed after prepared targets, compiled queries and linear substring
    matching. Parent paint regression also passed.

### V04 local contract extension decision

Keep legacy headless catalog/select_model/select_agent behavior compatible.
Add one typed CoreApp session-selection action carrying Current/Model/Variant/
Agent/New and Home context. The existing application worker owns resolution and
the existing SQLite prefs store owns session+agent drafts, Location+agent Home
model drafts and provider+model variant preferences. Related writes are atomic.
No transcript copy, UI persistence owner, provider adapter change or new service.
Model selection restores the current/preferred variant; explicit Variant(None)
persists a Default marker rather than falling through to a configured overlay.
Each submit/title/child inherits the rightful session selection and workspace.
New Home uses its Location/agent draft or configured fallback, not another
session's model draft. This corrects the prior global-model preservation claim.
Session snapshots remain actual accepted selections, so model→variant opening
depends on the owner's result. Validation and qualification remain in progress.

28. Full-catalog CPU gate (`cargo test --locked -p oc-tui
    v04_full_catalog_cpu_and_bounded_cache -- --nocapture`): exit 101.
    10,000 targets / 8,230,000 name bytes / 512-byte query: cold 2.382776642s
    exceeded the 2s debug gate; 100 cached frames 37.296µs. Replaced per-character
    BTree insertion for ASCII with direct buckets and omitted unused model-score
    boundary tables; kept the gate unchanged.
29. Parallel `cargo test --locked -p oc --test pty_t39 -- --nocapture`: exit 101
    at compilation (new helper accepted immutable PTY reference but sends keys).
    Corrected helper arguments to mutable; no behavioral assertion removed.

30. CPU gate repeat: exit 0. Cold queries (512,512,420,511 bytes) on all
    10,000 targets / 8,230,000 name bytes: 988.758207ms including preparation,
    152.483773ms, 4.032083ms, 205.74999ms. 100 unchanged frames: 36.746–44.128µs.
31. Full PTY repeat: exit 101, 5 passed/1 failed. New A/B/agent scenario reached
    a real owner error after changing the model of a committed session:
    `session wire history belongs to a different provider/model`. No request
    for the changed-model turn was sent. Runtime now uses its existing immutable
    message projection for a different model within the same provider, exactly
    as for changed agent behavior; it does not replay foreign-model opaque/tool
    records. Provider mismatch remains fatal. This is a necessary V04 model
    selection integration extension, not a history rewrite or adapter fallback.

32. Targeted scoped PTY: exit 0, 1 passed, and full oc-tui library: exit 0,
    112 passed (including parent paint regression and external ranking oracle).
33. Extended CPU gate to 128 distinct words that *all* match every target,
    Latin accent decomposition and non-Latin UTF-16, still 10,000 / ~8MiB.
    Exit 101: ASCII all-match 1.769936692s, accented all-match 2.336398361s
    exceeded unchanged 2s debug gate. Added bounded repeated-scalar normalization
    and compiled shared word prefixes for exact model membership; scoring path
    and admissible result set remain unchanged. Runtime cross-model projection
    test passed (exit 0); fmt + all-target workspace clippy passed (exit 0).

34. Expanded CPU gate and original ranking/membership oracle: exit 0. Debug cold
    preparation+matching: 1.007973346s; reused 512/420/511-byte queries:
    179.026191ms/5.015866ms/352.636151ms. All-word-match cold ASCII/accented/Greek:
    1.27561942s/1.308586581s/1.702556799s. 100 identical frames ≤56.725µs.
35. `cargo test --release --locked -p oc-tui
    v04_full_catalog_cpu_and_bounded_cache -- --nocapture`: exit 0. Optimized cold
    315.922338ms, subsequent query matching 33.525211ms/2.156648ms/54.810115ms;
    all-word-match cold ASCII/accented/Greek 347.487929ms/273.696691ms/297.486977ms.
    100 cached calls ≤9.955µs. Added a stricter optimized-build 500ms cold gate;
    debug remains the unchanged 2s gate, and cached100 remains 50ms. These are
    real full-capacity measurements; no model/query is truncated or dropped.

36. `cargo fmt --all && cargo test --locked --workspace --no-fail-fast --quiet`:
    exit 0, **496 passed, 0 failed, 5 existing ignored**. Includes six actual T39
    PTY tests, the new cross-model runtime wire-projection regression, 112 TUI
    tests (parent paint included), existing legacy headless/application/MCP/DCP/
    permission/lifecycle suites. No new ignores or changed mandatory gates.
37. Reviewed scoped worker/runtime/frontend/storage diff and new selection module;
    `git diff --check`: exit 0. `cargo fmt --all -- --check && cargo clippy
    --locked --workspace --all-targets -- -D warnings && cargo build --locked &&
    target/debug/oc --help`: exit 0 throughout. Optimized CPU gate repeated with
    its stricter 500ms limit: exit 0; cold 291.961593ms, subsequent queries
    29.326512ms/1.750086ms/52.004654ms; all-match ASCII/accented/Greek cold
    346.547433ms/256.826994ms/268.855758ms, cached100 ≤7.552µs.
38. Final review found that a multiword query containing an accent-only word
    normalized to empty needs rejection in the membership fast path. The isolated
    original fuzzysort 3.1.0 oracle returned `[]` for both `abc ◌́` and `◌́ abc`
    (actual input uses combining U+0301 without the dotted-circle glyph). Added
    those assertions to the existing oracle test and corrected the predicate.
    Expanded the new-session PTY to choose a model on Home, prove subsequent Home
    routes use their own persisted Location/agent draft, and inspect its DB row.
39. Post-review `cargo fmt --all && cargo test --locked -p oc-tui
    pinned_external_oracle_scores_and_order -- --nocapture && cargo test --locked
    -p oc --test pty_t39 v04_ -- --nocapture && cargo clippy --locked --workspace
    --all-targets -- -D warnings && cargo build --locked && git diff --check`:
    exit 0 throughout; oracle 1 passed, V04 raw PTY 3 passed. The complete workspace
    run in #36 precedes only these final edge-case/Home-draft assertions and the
    one-line membership guard; it is not represented as a second workspace run.

### Independent-review fixes delivered

- `crates/oc-core/src/{core_app,queries}.rs` adds the scoped typed action/API;
  old headless APIs and their persisted global keys retain their contract.
- `crates/oc-adapters/src/application_selection.rs` is the worker's metadata
  resolver over existing prefs: session+agent choices, Location+agent Home model
  drafts, provider+model variant preferences. `storage.rs` persists related rows
  atomically. `application.rs` resolves the owning session for turns, DCP context
  and title/inherited child selection; queries do not redirect another session.
- `crates/oc/src/tui_cmd.rs` uses that API at startup, model/variant/agent selection,
  new Home, session switch and Location return. `oc-tui/src/app.rs` emits a distinct
  model-selection intent so None is never used as a speculative preferred variant.
- `crates/oc-adapters/src/runtime.rs` permits a same-provider model change by
  projecting immutable public history and withholding other-model opaque/tool
  state. The new real-HTTP runtime test proves old function-call state is omitted
  for the other model, retained for return to its original lane, with all six
  original public history rows intact.
- Registry palette visibility hides Switch model variant without variants.
  Slash aliases still return the visible unavailable reason. Raw PTY asserts no
  palette result, no HTTP side effect, all busy refusals, exact session count,
  session-specific records and subsequent owning-session requests.
- `oc-tui/src/{picker,dialog,fuzzy}.rs` caches snapshot options and one prepared
  catalog/one query result. Queries compile once; bitsets reject impossible
  matches. UTF-16 occurrence indexes/shared word prefixes implement exact model
  membership; original scored ranking/heap ties remain for Commands. Linear
  substring search replaces potentially quadratic window comparison. Cache is
  replaced/reset rather than growing with user queries; unchanged frames reuse
  the same Rc. All admitted model results remain selectable.
- New raw PTY A→B→A test proves exact HTTP models/effort and inherited title
  requests, distinct session/agent drafts, current-dot **and Enter focus** after
  restart, remembered named `none` across model switch-away/back, explicit Default
  clearing preference/no overlay on return, and the unaffected B session after
  restart. SQLite confirms the scoped records and null per-model Default marker;
  scoped UI actions do not overwrite `PREF_MODEL_SELECTION`.

### Handoff boundaries

HEAD is still `931792ef8009ae6ad024cf09c780db029b3c8ef2`. No commit, staging,
progress/task/goal changes. Parent modifications to dialog paint/line styles,
blank foregrounds and the app modal regression remain intact; their tests pass.
The D13–D15 backend semantics, explicit-low shell fixture and separate no-overlay
test remain qualified by the full suite. Owner ZIP, `.opencode/` and authoring
secrets were not read or staged.

The earlier capture directories remain immutable historical results. These
selection/cache changes postdate those captures. Parent owns the final normal-
executable paired capture and source lock, now including the new
`application_selection.rs` as well as previously untracked `fuzzy.rs`. No claim of
full VIS parity: model release/status/favorites/recents/connected DTO semantics,
locale title collation, and other expressly mapped V04/V05 followups remain as
  listed above. No additional independent goal was consumed.

## Independent review: retired choices and headless precedence

Review found two blocking defects in the previous scoped-selection delivery:
retired model/variant caused startup Query failure; legacy headless selects were
masked by older session drafts. Work remains on HEAD 931792e, uncommitted; the
parent dialog paint and capture/source manifest are unchanged.

40. `cargo fmt --all && cargo check --locked -p oc`: exit 0 after introducing
    owner epoch precedence and a display-only retired snapshot.
41. `cargo fmt --all && cargo test --locked -p oc --test pty_t39 v04_retired
    -- --nocapture && ... v04_legacy`: first test exit 101, second was not run.
    Actual 80x24 PTY showed the correctly refused turn and preserved draft, but
    the test expected an unwrapped string. The terminal wraps the text across
    rows (`submit: application: selected model/variant` / `unavailable; select
    an admitted replacement or`). Corrected the grid-row predicate to assert
    `select an admitted replacement`; did not relax effects or persistence checks.

42. Targeted retired PTY repeat exit 101 (`DataRootBusy`): test opened a second
    SQLite owner while the real TUI held its data-root lock. Kept the lock
    invariant: quit after the refused submit, verify unchanged prefs and empty
    history, restart and remediate. Same test then exit 0 for both removed-model
    and disabled-variant cases; headless precedence actual HTTP/restart test exit
    0. No storage lock or production admission behavior relaxed.

43. Full `pty_t39` after pure `Current` lookup: exit 101, 7 passed/1 failed.
    Old A→B→A test expected an untouched B session to acquire a stored selection
    merely by opening it. B's real HTTP model remained correct; updated the
    assertion to require **absence** of B's pref row, matching the stronger
    no-mutation-before-explicit-acceptance contract. Subsequent TUI/Clippy chain
    did not run because the PTY target failed.

44. Corrected untouched-session assertion; `cargo fmt --all && cargo test
    --locked -p oc --test pty_t39 -- --nocapture && cargo test --locked -p
    oc-tui --lib --quiet && cargo clippy --locked --workspace --all-targets
    -- -D warnings && cargo build --locked && git diff --check`: exit 0 throughout;
    8 actual PTY, 112 TUI unit tests. Full workspace afterward exit 0,
    **498 passed, 5 existing ignored** (before extra honest-retired-variant
    presentation assertion).
45. Review found an additional presentation inconsistency: a retired named
    variant was gated on submit, but the variant dialog's Default row had a
    current dot. Modified picker current-dot and metadata to show the retired
    named variant as unavailable, with Default explicitly unselected. Added
    actual PTY assertion. First targeted compilation exit 101: moved option
    `name` before computing its footer. Reordered field initialization; no
    behavioral assertions removed. Post-change checks below supersede #44.

46. Targeted retired-model/variant raw PTY (1 passed) and picker tests (7 passed):
    exit 0 after showing unavailable named variant and no false Default current dot.
47. Post-presentation `cargo test --locked --workspace --no-fail-fast --quiet`:
    exit 101, 497 passed/1 failed/5 existing ignored. The full-capacity debug
    CPU gate failed on a Greek Unicode all-word-match cold catalog at
    2.199030483s versus 2s; earlier isolated Greek cold was 1.70s. No functional
    tests failed. The chained fmt/clippy/build/help/diff steps were not run.
    Optimized Unicode target preparation to use direct ASCII occurrence buckets
    even in mixed-script catalogs, retaining an ordered non-ASCII map. The
    10,000-model/8MiB and 512-query capacity and the timing gate were not raised.

48. Targeted cold CPU gate (debug exit 0, Greek 1.436001076s) and pinned fuzzy
    oracle exit 0; optimized-release CPU gate exit 0, Greek 313.301496ms.
49. Full workspace repeat after Unicode preparation: exit 101, 497 passed/1
    failed/5 existing ignored. `v04_raw_dialogs...` raced provider echo versus
    the application's still-pending acceptance on `/effort`: exact screen showed
    the completed echo *and* visible `turn active; action unavailable` with the
    `/effort` draft preserved. No wrong wire request or selection occurred. The
    existing test-only `wait_idle` now also watches the pending-acceptance label,
    and this route waits for idle after asserting the real low-effort request.
    The timing gate passed on this full run. Chained checks after workspace did
    not execute on failure.

50. Final `cargo fmt --all && cargo test --locked -p oc --test pty_t39
    -- --nocapture && cargo test --locked --workspace --no-fail-fast --quiet &&
    cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets
    -- -D warnings && cargo build --locked && target/debug/oc --help &&
    git diff --check`: **exit 0 throughout**; actual T39 PTY 8 passed, full
    workspace **498 passed, 0 failed, 5 existing ignored**. No ignored/disabled
    tests added; Rust 2024 workspace and normal executable remain buildable.

### Result of second independent-review correction

- `application_selection.rs` projects a retired session model or disabled/removed
  variant *without validating it during Current/open/switch*. It never rewrites
  a preference on a read. `application.rs` validates the exact model and variant
  **before** acceptance, title work, workspace permission publication or any
  provider request; rejected turns retain the editable input. A valid explicit
  Model or Variant(Default) action bypasses the stale resolution, validates the
  new value and atomically persists the remediation. The previously persisted
  global legacy model is likewise preserved as retired rather than silently
  authorizing the configured fallback.
- The picker shows an unavailable retired model by ID, keeps a retired variant
  visibly named in metadata and the variant dialog, and does not mark Default
  current before explicit acceptance. Raw PTY covers both removal and disable
  across process restart: startup/switch query succeeds, refusal sends zero HTTP
  requests and writes neither history nor prefs, replacement sends the exact
  admitted request with no stale overlay, SQLite records the explicit choice.
- A Location/provider-scoped durable selection epoch advances on the public
  legacy `CoreApp::select_model` and `select_agent` APIs. Older scoped drafts do
  not mask those accepted global headless choices, including after restart or
  for a new session with an older Home draft. A later explicit scoped action
  can establish a fresh session choice. Integrated real-HTTP tests prove
  headless model/no-overlay, explicit fast variant/high and pinned agent prompt
  on the very same previously scoped session; old scoped rows remain durable.
- Existing A→B→A selection/restart and named-none-versus-Default PTY tests, model
  oracle, parent paint tests, D13–D15 regressions and the full workspace pass.

HEAD still `931792ef8009ae6ad024cf09c780db029b3c8ef2`; all changes remain
uncommitted/unstaged. This second correction changed `crates/oc-adapters/src/
{application,application_selection}.rs`, `crates/oc-tui/src/{app,picker,fuzzy}.rs`,
`crates/oc/tests/pty_t39.rs` and this append-only evidence file. Parent's
`dialog.rs` paint/line styles and source manifest/capture ownership were not
edited here. No changes to progress/tasks/goals, .opencode or owner ZIP.
Previously paired capture comparisons remain historical and DIFFERENT; parent
owns fresh final executable captures/source locking. No VIS parity claim.

51. After the successful workspace check, extended the disabled-variant raw
    PTY path to start on a healthy session and use actual `/continue` to open the
    retired session, exercising the **switch** query as well as the removed-model
    startup query. Post-extension qualification is recorded below; #50 predates
    this added switch assertion.

52. Targeted removed-model-startup/disabled-variant-session-switch PTY exit 0.
    Full T39 PTY repeat exit 101, 7 passed/1 failed: AUD29 sent manual compress
    immediately after the visible custom-command echo while that turn's receipt
    was still active. It was correctly refused with `turn active; action
    unavailable`; no compress request was sent. Added the same reconstructed
    terminal idle check before issuing manual compress. Chained fmt/clippy/diff
    checks were not reached because PTY failed; no runtime guard was weakened.

53. Final after session-switch coverage/receipt-race regression: `cargo fmt
    --all && cargo test --locked -p oc --test pty_t39 -- --nocapture && cargo
    fmt --all -- --check && cargo clippy --locked --workspace --all-targets
    -- -D warnings && git diff --check`: exit 0 throughout, **8 actual PTY
    passed**, no ignores. The full workspace success in #50 predates only the
    added retired-session-switch PTY assertion and test-only AUD29 idle wait;
    production selection, picker and fuzzy source are identical to that full
    success. The #48 release CPU gate and fuzzy oracle remained successful.

Exact handoff: current HEAD `931792ef8009ae6ad024cf09c780db029b3c8ef2`,
no commits/staging/progress edits. Final corrected behavior uses only the
existing CoreApp, worker, SQLite prefs, runtime and TUI; model/variant choices
are visible but never sent stale or silently replaced. The full VIS comparator
and capture/source manifest remain parent-owned and not claimed here.
