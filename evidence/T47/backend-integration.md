# Backend blockers — combined integration handoff

Date: 2026-09-22. Base/current HEAD:
`15719e976030a8fdaadd843ac13556de237f559d`.
Delivery: uncommitted working-tree code/tests/docs; nothing staged, committed or
pushed in this task. This report is not full-workspace PASS or task finish.
Execution contract: `docs/goals/2026-09-22-backend-blockers.md`.

## Delivered backend behavior

- **T47 / AUD41:** unknown/partial/zero metadata uses bounded native request policy
  with a visible warning, without inserting invented limits into discovery.
  `nativeFallbackLimits` defaults to context 32768 / output 4096 and validates the
  1024 reserve. Known positive limits still constrain requests. Input, tools,
  continuation results, child and ancillary title calls are admitted. No selected
  variant means no overlay. Details/config example: `evidence/T47/report.md`.
- **T43 / AUD42:** ordered action/resource rules; unmatched resource asks, absent
  central action denies; independent central/selected-primary/child constraints
  only narrow. Rules follow selected/restored primary workspace, child inheritance
  and manual compression. Hidden/disabled metadata and catalog filtering work.
  Actual literal-star filename read/delete bypass is fixed. Details:
  `evidence/T43/backend-permissions.md`.
- **T46 / MCP07:** text, structured JSON and textual resource/link outputs survive
  safe projection. Instructions are bounded, authorized provider projection, not
  history mutation. Stdio argv/working-env redactions apply to successful results
  as well as instructions. Error categories come only from exact recognized
  structured codes / JSON-RPC codes, never guessed from arbitrary prose. Unsupported
  media remains explicit failure, not successful partial output. D13 lifetime,
  warnings, cancellation, catalog caps and reaping remain. Details:
  `evidence/T46/backend-parity.md`.
- **Planning:** T47 registered; new criteria AUD41/AUD42/MCP07 have one owner each.
  A01–A13 gate references are separately validated against GOAL headings; unknown,
  duplicate, shadowing and multiply-owned detailed IDs still fail. T43 prerequisite
  and T45 remainder ownership no longer form a done-dependency deadlock. T30 requires
  mandatory backend/live prerequisites and final-code-commit AUD38/AUD39 rerun.
- **T27 offline harness:** product config/catalog discovery, literal slash model
  IDs, explicit variants and child binary selection, isolated environment,
  resource permission preflight, new durable completed exact search verification,
  blocked/unexecuted aggregate non-success, pipe watchdog/reap and payload-free
  diagnostics. The external branch is fail-closed until the separate live envelope
  is actually enforceable. It is **not live-ready**. Details:
  `evidence/T27/backend-harness.md`.

## Review corrections

Independent read-only review and targeted failing regressions identified and fixed:

1. Selected primary permissions missing/sticky after selection and restoration.
2. Literal `*` in a filename bypassing a wildcard deny.
3. Stdio argv canary surviving successful MCP result redaction.
4. Title generation sending 256 despite an unknown-model native output cap of 64.
5. Whole-payload substring error classification inventing timeout from
   `timeout:false` and overriding structured codes with unrelated prose.

The final correction uses exact optional structured code paths, not a claim that
all MCP servers share one error schema. Unknown messages remain opaque failures.
No test was ignored/disabled to conceal these findings.

## Actual final checks

| Command | Observed result |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | exit 0 after final production edit |
| `cargo build --locked` | exit 0; `target/debug/oc` rebuilt |
| `target/debug/oc --help` | exit 0 |
| `cargo test --locked --workspace --no-fail-fast --quiet` | **exit 101: 484 passed, 3 failed, 5 ignored**; all failures are the protected T44 cases below |
| `python3 scripts/check_docs.py` | exit 0; 48 tasks, 125 detailed acceptance specifications |
| `python3 scripts/progress.py check` | exit 0, structure only |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | exit 0; 29 tests, including namespace/ownership negative cases |
| `git diff --check` | exit 0 |

The five ignored entries are four explicit external live/browser tests plus the
internal catalog-probe entrypoint. Offline harness tests explicitly run that probe
in isolated subprocesses; it is not missing offline coverage.

The final complete run includes **381 passing backend/core/binary tests** and
**103 passing TUI unit tests**. Backend runtime: 41; permissions: 8; child: 12;
remote MCP: 24; stdio: 12; adapter unit: 163; bounded harness: 9; actual-binary
application: 4 tests / nine fixtures. Actual PTY, recovery V02/V03, golden binary,
soak, cleanup, context, discovery and immutable-history suites were executed.

Earlier combined run hit the tool's 200-second command timeout after already
showing the same three TUI failures; it is not counted as a completed run. A scoped
`--workspace --exclude oc-tui` run passed before the final exact-code MCP correction;
it is diagnostic evidence only, not a substitute for the full final failing gate.
The final full run above completed with an adequate command timeout.

## Proven closure blocker: protected T44 fixture

`crates/oc-tui/src/shell.rs:931–965` builds a catalog with one enabled variant
`low`, but sets the selected `variant` to `None` (line 955). These tests still
expect an automatically selected `low`:

- `shell::tests::golden_screen_80x24` — assertion line 1161, expected line 1155.
- `shell::tests::golden_screen_120x40` — assertion line 1183, expected line 1179.
- `shell::tests::resize_switches_layout_at_upstream_breakpoints` — line 1194.

Actual prompt metadata is `x · a ludka2`; expected is `x · a ludka2 · low`.
Geometry/text apart from that suffix matches the two goldens. This is an intentional
backend contract change reaching an old fixture, not an unexplained render defect.
The explicit owner instruction prohibits editing any T44 source/tests/evidence.
Reintroducing implicit variant selection, faking its display, test-only backend
behavior or hiding failing tests would violate the requested behavior.

Required owner-side integration action: make the fixture explicitly select `low`
if the geometry test intends a selected variant, or expect no variant when `None`.
No such change was made here. The full gate remains red and this report is BLOCKED,
not DONE. This is the necessary missing permission for overall offline closure.

## Live prerequisite inspection — no network or generation

Read-only metadata inspection of the specifically documented product JSONC and
repository config established that required environment substitutions are present
and nonempty. No secret value or raw config is retained in evidence. Credentials
are **not** claimed missing; remote validity/current catalog were not queried.

Enabled configured MCP servers are `codex_web` and `crw`; browser remains disabled.
The configured exact model is `ludka2/ocg/muse-spark-1.3-contributor`, requiring
native discovery. Three independent issues prevent authorized live execution:

1. `OC_TEST_CONFIG` routes its whole directory. The documented product JSONC shares
   a root with excluded authoring-agent configuration. Automatic whole-root
   composition cannot be qualified within the approved read boundary. A separated,
   approved product root is required; excluded config was not read or copied.
2. Coding grants exist only in this repository's project config, not the approved
   global product source; model-only temporary fixture overlays do not inherit
   them. A bare `codex_web` grant does not authorize `codex_web__search` or its
   supported alias. Harness does not add these grants or enable disabled servers.
3. The old five-step/300-second/900-second bounds do not enforce the runbook's
   durable 24 generation requests (including retries), four short searches and
   per-mode output ceilings. This is an **unfinished harness engineering gap**, not
   a claimed external quota/credential failure. Current external preflight blocks
   before any discovery/generation/MCP; no boolean environment bypass was added.

Safe alternatives checked: ordinary product config root composition would read
excluded sources; fixture-only model override leaves insufficient permissions;
credentials and time/round limits alone do not establish campaign-wide quotas.
No paid/external request was made. No mandatory live gate has been marked PASS.

## Scope and retained plan

`git diff --exit-code` confirmed unchanged GOAL, baseline, references, core DTOs,
all `crates/oc-tui`, `tui_cmd.rs`, TUI/PTY/recovery/MCP-application tests, T44 goal,
M9 journal, T44/TUI evidence, recovery assets and repository `opencode.json`.
The T44 object in tasks/state and its current/checkpoint pointers are unchanged;
`progress/NOW.md` changes only the generated ready list. T47 is registered `todo`,
not falsely finished while T44 owns the sole active slot.

Changed categories: adapter config/models/permissions/tools/MCP/runtime/application
and their regression tests; binary application/live-harness tests; registry,
acceptance, validator/tests, decision/contract/goal docs and derived progress views.
New source files are `permissions.rs`, `mcp_result.rs` and `tests/permissions.rs`.
`serde_json/preserve_order` adds an edge to already-locked `indexmap`, no new package
or version. Production runtime remains Rust-only; provenance files are unchanged.
`.opencode/` and the owner ZIP remain untracked and untouched. No staged files.

T45 orchestration/built-ins/remaining metadata, MCP media/prompts/resource catalogs,
interactive approval, T27 full live qualification and final SHA qualification
remain explicit pending work. This backend slice does not waive or claim those
features. The source reports and this checkpoint preserve a continuable worktree
without claiming all blockers closed.
