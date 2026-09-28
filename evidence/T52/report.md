# T52 — coarse code slices qualification

## Result

**PASS: R0–R7 / ARCH01–ARCH05.** Structural plan is implemented and qualified;
this is not T44/V09 or product READY. Latest owner instruction explicitly
requested applying `.local/oc-code-slices-plan.patch` and continuing the changed
plan. T44 was temporarily scheduling-blocked, not marked failed/done.

Git base: `ddb6ae2f0cba18c4aa161159899228a7343398eb`.
Implementation is already committed through
`9570160fdd4f5d89716456dc1d02b2981632c028`; Rust source last changed in
`c75465eaf`. Separate reviewed slices:

| Slice | Implementation / result | Direct evidence |
| --- | --- | --- |
| R0 | `700a87895`: exact planning patch, scheduling, actual-host baseline, advisory helper | scheduling.md, baseline.md, test-inventory-before.json |
| R1 | `eb6e7e109`: owner unit suites + shared fixtures | same nonempty app/messages/shell/binary targets; final exact inventory |
| R2 | `04b0467e8`: TuiState facade + four parts | R2.md, frame-comparison.json, actual PTY/allocation/unit tests |
| R3 | `8c64d021f`: Runtime facade + turn/context/MCP + unit packs | R3.md; runtime/subagent/DCP/MCP/recovery gates |
| R4 | `62e875416`: storage/application test packs | R4.md; same storage/application/transaction/recovery targets |
| R5 | `c75465eaf`: same runtime/pty_t39 targets, thematic scenario parts | R5.md; 91 runtime + 35 serialized actual-PTY cases |
| R6 | `9570160fd`: current map and live source consumers | R6.md; nonempty exact filters, missing-coverage checks |
| R7 | full current-source qualification, exact list/ignored bijection, scoped review | this report; inventories, actual command logs below |

All eight original >5000 Rust files are addressed, and application tests are
also separate. Root sizes are physical lines, not exact production SLOC:

| Owner / existing target root | Base | Current root | New responsibility/test parts |
| --- | ---: | ---: | --- |
| oc-tui/src/app.rs | 17604 | 1308 | input 2931, transcript 1680, tabs 1229, live 1666; shared/thematic tests |
| oc/src/tui_cmd.rs | 8502 | 3255 | tests fixture 71, routing 1821, lifecycle 3347 |
| oc-adapters/tests/runtime.rs | 7930 | 954 | turns 3302, context 1696, tool_lifecycle 1994 |
| oc-tui/src/messages.rs | 7408 | 3670 | tests 3746 |
| oc-adapters/src/runtime.rs | 6189 | 1548 | turn 2359, context 1041, mcp 843, tests 439 |
| oc-tui/src/shell.rs | 5561 | 2207 | tests 3350 |
| oc/tests/pty_t39.rs | 5496 | 2128 | interaction 2093, lifecycle 1286 |
| oc-adapters/src/storage.rs | 5177 | 3502 | tests 1674 |
| oc-adapters/src/application.rs | 4553 | 3272 | nested test packs 1298 |

Current owned inventory: **229 files, 167410 physical lines, 6718486 UTF-8
bytes, zero >5000 warnings**. R0 host inventory included the two newly added
helper files: 204 files / 167279 lines / 6801544 bytes. These are not archive
counts or a claim about exact production LOC/token savings. Git-base sizes and
new/deleted paths are in size-inventory-after.json; base read from Git, not from
a copied tree. There are 25 new Rust parts and two dev-tool files, not hundreds
of microfiles. Remaining >5000 exception: **none**. Small inline bound_preview
case stays local with its reason in CODE_MAP; cfg probes stay in their owner.

## Checks

### ARCH acceptance

| ID | Result | Qualification |
| --- | --- | --- |
| ARCH01 | PASS | Real coarse owner/test slices, warning-only line/byte helper and edge/Git/symlink/error tests; no leftover >5000 file |
| ARCH02 | PASS | Exact target + full-name + ignored-set bijection for all 927 affected Rust identities; same Cargo topology/fixtures |
| ARCH03 | PASS | Scoped ownership/API/representation/body review plus real wire/storage/PTY/allocation/render and final workspace gates |
| ARCH04 | PASS | Current AGENTS/ARCHITECTURE/CODE_MAP; selective tab/DCP navigation with actually nonempty test filters |
| ARCH05 | PASS | Separate Git-traceable moves, original base, current implementation, immutable old evidence, one active task, factual closeout |

### Test identity, not counts alone

Identical Cargo list options/environment before/after:
`cargo test <target selectors> --locked -- --list`, then
`cargo test <target selectors> --locked -- --ignored --list`.
The full raw lists and explicit per-name mapping are in
test-inventory-{before,after,comparison}.json. No old identity or ignored status
was lost, duplicated or inferred from matching totals.

| Target selector | Before = after | Ignored before = after |
| --- | ---: | ---: |
| -p oc-tui --lib | 414 | 0 |
| -p oc-adapters --lib | 304 | 0 |
| -p oc --bin oc | 83 | 0 |
| -p oc-adapters --test runtime | 91 | 0 |
| -p oc --test pty_t39 | 35 | 0 |

Only planned module components changed: app::tests → input/transcript/lifecycle
children, and app::tabs::tests for the 17 tab cases (private animation state
stays private); tui_cmd::tests → routing/lifecycle; Runtime packs gain ::tests;
three application packs gain ::tests; runtime/pty_t39 scenarios gain their
thematic prefixes. Existing approval_lifecycle and tui_cmd::approval_tests paths
and all unchanged identities remain. Added Python helper tests: five methods,
accounted separately, no new Rust cases/targets.

### Ownership / API / invariants review

Reviewed per owner and commit, not one monolithic moved-block dump. Supplemental
base/current comparison preserved 1451 functions with literal bytes and all
outer attributes, and 238 type/field/variant/constant/derive/cfg definitions
(including the unchanged tool_stream descendant). This is evidence alongside
scoped diff and runtime gates, not an assertion that token matching alone proves
semantic equivalence. Allowed differences reviewed explicitly: seven anchored
include_str paths to the same untouched fixtures; rustfmt whitespace/optional
trailing call commas; one named storage test's redundant sole-expression closure
block; minimal pub(super) at actual owner-private seams. No assertion, golden,
timeout, ignored status or resource threshold edits.

Four packages, Cargo manifests/lock/toolchain and oc-core are unchanged. Existing
D12 read-side remains exactly that exception. Public ScriptDriver/PumpOutcome
are still non-cfg-test at their original app paths; explicit App/Runtime exports
preserve callers. No new public test probe, holder/trait/crate, include! monolith
or duplicated fixture/counter was introduced. TuiState remains the sole UI-state
owner. Runtime still borrows &Db; published/active state and counter stay single.
Lock guards/ActiveLease/MCP lease drop and await scopes, response close before
admission, durable intent/outcome/child effects and cancellation/shutdown order
remain inside the same unchanged bodies. SQL/DTO/wire/blob/key/digest formats,
generation publication, title jobs, tab CAS, bounds and raw-history immutability
were not rewritten. Existing compaction/tool_stream/storage parts retain paths.

Live capture consumers now include moved parts. Historical absent coverage is
reported null/incomplete/mismatched rather than backfilled or accepted. No old
checkpoint/capture lock/report/manifest was modified. No new SHA256 refactor
manifest/gate was created; existing product/capture digests remain intact.

### Actual commands / exits

Pinned host/toolchain; non-root dedicated worktree. Cargo serialized, jobs=3,
RUST_TEST_THREADS=1, owned disk TMPDIR
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Full final command log:
`/home/opencode/.local/share/opencode/tool-output/tool_0e793caf3001M6pq49SbRxoPYK`.
All commands below exited 0 after the final Rust/tooling edits:

```text
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo build --workspace --release --locked
target/debug/oc --help
target/release/oc --help
python3 scripts/test_code_size.py
python3 scripts/test_progress.py
python3 scripts/test_check_docs.py
python3 scripts/check_docs.py
python3 scripts/progress.py check
node scripts/tui_capture/check_frontend.mjs
node scripts/tui_capture/check_capture_geometry.mjs
node --check scripts/tui_capture/summarize_prompt_paste.mjs
git diff --check
```

Workspace: **1189 Rust cases PASS, zero failures, nine existing opt-in live
cases ignored unchanged**. Python: 5 + 15 + 14 PASS. Same current-source explicit
`cargo fmt --all -- --check` and
`cargo test -p oc --locked --test pty_t39 -- --test-threads=1` also exit 0;
the latter ran all 35 cases in 120.24 seconds. Applicable staged/scoped Git
diff checks, current-source inventory/bijection and representation/body review,
Python consumer AST syntax and bounded missing/full association smoke exit 0.
Nearest slice checks are recorded in R2–R6 and repeated in workspace qualification.

UI before/after R2: existing page(3000) fixture, fixed real session/tab state,
animations off, no masks. Full 80x24 / 120x40 / 160x48 grids: **14400 raw styled
cells, xterm grids and three cursors equal; three PNGs byte-identical**. Saved
frame-fixture sources, raw frames and frame-comparison.json document the method.
The before source is clean R1; after is the subsequently committed R2 diff, not
mislabelled as a then-existing new HEAD. UI production is unchanged since R2.
These are bounded in-memory regression frames plus separately green actual PTY
tests, not new original/native T44 parity captures or full VIS38/V09.

## Risks

R0's initial 120-second tool timeout interrupted the long test campaign; inspected
process state and reran the complete unchanged suite with an adequate tool timeout,
PASS. R3's first mcp_application run hit one existing shared two-second UI deadline;
isolated and complete unchanged reruns, and final whole workspace all PASS. No
timeouts/assertions were weakened or failure suppressed; details in baseline/R3.

No paid/live/API/Docker requirement was introduced by structural work. Archive-only
missing large raw captures remain NOT_AVAILABLE_IN_ARCHIVE as specified, not new
PASS. Preexisting untracked `.opencode/` was never read/edited/staged. Product
requirements and the incomplete T44/VIS38/V09 matrix remain open. Delivery is a
separate Git operation after report/finish; this report does not preclaim push.

## Next

Finish T52 with this factual report and a short closeout note, commit journal/
evidence and push the verified own branch normally. Then explicitly start T44
through progress.py and continue its previously unfinished feature/qualification
slice under the owner's resumed updated plan. Do not mark T44 or A01–A13 done
from ARCH PASS.
