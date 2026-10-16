# T51 R3 / CFG10 — frozen safe fatal diagnostics

## Coordinator qualification — 2026-09-29

The coordinator reviewed the complete R3 owner/test diff against the existing Rust
source at `fcfdd584b5ba6672df70f326ba4430602508b240`; three independent read-only
scouts checked config/trust, runtime/cleanup, and public UI/error boundaries. The
config/runtime scopes found no concrete regression. Public `ServiceDiagnostic`
fields and legacy `CoreError::Application(String)` remain caller-owned types: the
new fatal/config/service producers use schema-only fields, opaque source/identity
and typed causes; untrusted arbitrary DTO construction is not accepted as a
product error source. The owner test matrix directly checks actual consumers,
including details/copy/investigation and headless stderr. R4 retains its separate
saved-selection and real-user qualification obligations.

Two owner planning commits landed during this slice: `986e9bab0` adds T51 R4/E2E06
and `19b30a639` adds separate pending T54/VIS43. Both were docs/registry-only, so
they did not change the Rust source associated with the final dirty-source artifacts
below. Their statuses, test IDs and historical evidence were preserved.

Independent targeted checks exited 0: adapter owner `cfg10_` **5 passed** (the
intentionally panicking owned-join fixture proves non-success), actual PTY `cfg10_`
**2 passed**. Python live-envelope/code-size/progress/docs suites **47 passed**,
fmt, documentation/progress/diff checks passed, and the advisory changed-file
inventory had no >5,000-line warning. The retained current-source workspace gate
had **1,288 passed, zero failed, 10 unchanged opt-in ignored**. After the last
normal `cargo build --locked`, the coordinator directly reran native CFG10 **2**,
CFG09 **1**, UI07 **4**, recovery/startup **4**, configured-workspace **13** and
storage-lock **1**, all passed; final debug/release digests were unchanged. These
are dirty-R3 artifacts, not a clean-parent or full-product claim. The existing
bounded live campaign was untouched (8 generation / 1 search / 19 control).

## Result

Pre-change freeze at `fcfdd584b5ba6672df70f326ba4430602508b240` (tracked clean;
inherited untracked `.opencode/` untouched). R1/CFG09 and R2/UI07 are delivered.
This slice owns R3 only; qualification below is pending at this freeze.

Post-change R3/CFG10 result: the five frozen obligations below are qualified
against the actual native binary and existing owner tests. This is an R3 handoff,
not completion of T51 or A13: the independently approved R4/E2E06 saved-choice
and real-user existing/fresh-store PTY/strace qualification remains pending.

Frozen obligations before the initial actual-binary RED:

1. Loader → application → TUI/headless shares the existing `ServiceDiagnostic`
   source/field/service/stage/code/action/retryability facts. Causes come from
   typed errors and the operation that creates them, never Display matching.
2. Optional malformed definitions have visible bounded failed diagnostics while
   healthy siblings survive. A broken/unreadable config document cannot prove
   absence of mandatory policy and therefore fails closed; selected/mandatory
   definitions and trust/policy/resource admission cannot degrade to defaults.
3. Truly fatal config/trust/data-root/storage/recovery/cleanup/caps retain
   non-success with a redacted source, known schema field, cause and allowed
   action. Cancellation remains cancellation; cleanup failure cannot be success.
4. Failed security-relevant reload retains the complete previous generation,
   including policy/catalog/instructions/clients. Recovery requires explicit
   successful complete admission; no half publication or silent fallback.
5. Debug/Display/UI/details/copy/investigation drafts/stderr contain no unsafe
    source basename/path/URL/headers/env reference/expanded values/control
    sequences. Headless errors are nonzero and `--json` stdout is NDJSON only.

Observed, by obligation:

1. `config/diagnostic.rs` projects schema-only fields and opaque source IDs
   using the existing `ServiceDiagnostic` type. `composition.rs` retains
   trust/document/cap admission authority; `application.rs::SpawnDiagnostic`,
   typed worker join and `CoreError::Diagnostic` carry actual typed causes to
   `oc-tui/src/shell.rs` and the binary TUI/headless consumers. Public strict
   config assembly, legacy coarse startup category and native discovery oracle
   remain compatible; no string/regex parsing of error messages determines
   cause. Request-level MCP tags reuse the R6 diagnostic owner.
2. The optional namespace is admitted Markdown/inline definitions, **not** a
   partially parsed `opencode.jsonc`: a broken policy-bearing document remains
   fatal even above a complete global Deny. Healthy command/instructions and
   Deny survive malformed sibling frontmatter and 130 invalid inline commands.
   Exactly 64 definition failures display, 67 omissions are counted, and
   later entries still undergo full admission. Unsupported native DCP/provider
   metadata is visibly *ignored* with safe code, never shown as active.
3. Actual headless `--json`/PTY non-success matrix: invalid policy
   `config/invalid_config/permissions`, unreadable or malformed document
   `config/invalid_document/document`, external symlink
   `admission/trust_refused/root`, second native owner
   `storage/data_root_busy/data_root`, interrupted-operation trigger
   `recovery/recovery_failed/operations`, 16 MiB+1 source and nine enabled
   MCP entries `capacity_exceeded`, malformed/invalid SQLite preference
   `query/invalid_stored_state|storage_unavailable/selection.model`. The
   owned trigger leaves the operation `started` (no unknown-effect replay),
   the busy lock can be reacquired, and fatal cases send zero discovery and
   Responses requests. Rejected unsafe model IDs, stored-root DCP/tool queries
   and failed selection writes also retain safe typed causes; a failed write
   preserves the selected Home snapshot. Completed-but-panicked title/provider
   tasks and native worker cleanup report non-success; cancellation stays
   cancelled and MCP metadata capacity uses its actual typed limit cause.
4. Rejecting malformed project policy on `/reload` preserves the previous
   generation's Deny, model, instructions, compiled DCP and session choices.
   The next actual request still uses the old model/instructions and one DCP
   schema; the denied `read` call produces a failed tool row without reading
   the owned canary file. Two prompts + denied-read continuation = three main
   requests, one title, zero discovery. Reopening with broken policy is fatal,
   with no additional provider requests. Separate CFG09/UI07/MCP09 and
   retained-deck tests keep their independent generation/owner invariants.
5. Synthetic path basename, env-reference, credential, URL and ANSI canaries
   do not appear in the native fatal screen/stderr, DTO Display/Debug/JSON,
   Settings details/copy/investigation note or captured request bodies.
   MCP identities are always opaque (including syntactically ordinary names),
   and degradation/clear use the *admitted registry key*, not an error's
   claimed identity or a rehashed presentation string. Details/copy/unsent
   investigation retain the ordinary composer and do not submit a turn.
   Fatal `--json` stdout is empty, exit is 1; successful JSON still contains
   only NDJSON. Actual PTY fatal exit is 1, terminal is restored and emits
   `ALT_LEAVE` (declared native frame, no donor pixel-parity claim).

## Checks

Pending at freeze: first native malformed-document RED; nearest typed owner
tests; actual native fatal/optional-definition/lock/trust/recovery/reload matrix;
delivered CFG09/UI07/MCP09 regressions; policy/storage/cancel/cleanup/caps gates.
Final integration qualification: workspace fmt, strict all-target locked Clippy,
workspace tests `--locked --no-fail-fast`, Python/document checks, normal root
debug/release builds/help followed by retained native executables and hashes.

Performed (all commands use one Cargo invocation at a time,
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 CARGO_NET_OFFLINE=true`, approved
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
900000 ms cap; only isolated synthetic env and owned fake loopback transports):

- Initial actual-binary malformed-document exact RED exited 101/one failed
  (raw path and missing structured cause), then GREEN one passed. Corrupt
  saved-selection fallback, unknown-DCP warning, raw MCP server name,
  combined-generation MCP capacity, global async MCP capacity and retained
  storage-query canary each had owner-specific RED before the corresponding
  typed change. Final selection model/refused SQLite write canary RED exited
  101/one failed; after projecting both failures, exact GREEN one passed and
  both existing fork-owner cases passed. These are observed failures, not
  modified baselines or newly granted retries.
- Earlier full-workspace attempt timed out after raw MCP test-name comparisons
  left a test join waiting for a fake peer; inspected owned processes before
  retry. Later legacy raw prose/name assertions were updated only for the
  approved opaque diagnostic contract; denial, resource reaping, terminal
  results, budgets and exact wire assertions remain. One full run exposed an
  independent slow-peer scheduling assertion whose original one-second gate
  had awaited only the healthy peer; it now awaits both existing counters
  within that same deadline. A later run found the old raw retired-variant
  error oracle and one VIS31 100 ms burst outlier; the oracle now checks the
  typed field/code while retaining explicit repair/wire checks. The unchanged
  VIS31 test then passed at 165/250 Hz (maximum input-paint latency 21/48 ms)
  and passed again in the final full workspace. No cap, retry, timeout,
  ignored test, golden or historical live budget was weakened.
- Final `cargo fmt --all --check`, `git diff --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`, and
  `cargo test --workspace --locked --no-fail-fast -- --test-threads=1`:
  **exit 0, 1,288 passed / 10 existing opt-in ignored / zero failed**.
  Examples: adapters 344 lib + 95 runtime; core 28; TUI 420; actual binary
  configured workspace 13, PTY T39 46, MCP application 41, recovery startup 4,
  T42 34; all workspace targets/doc-tests passed. Final full-test harness
  output: `tool_0ee18a1cf001hNeTrgF59U3hjh` (environment-managed output,
  not a new repository fixture). DISC01–DISC10, PROV06, CFG09, UI07, MCP09,
  trust/cap/cancel/cleanup/recovery/retirement/no-effects gates included.
- Following the separate owner documentation commits, Python fixture checks
  `env PYTHONDONTWRITEBYTECODE=1 PYTHONPATH=scripts python3 -m unittest
  scripts/test_bounded_live.py scripts/test_code_size.py scripts/test_progress.py
  scripts/test_check_docs.py`: **47 passed, exit 0** (13+5+15+14); `python3
  scripts/check_docs.py`, `python3 scripts/progress.py check` and `python3
  scripts/code_size.py --base 19b30a639f3d06e9abbbb124ca7f4248692e5e99
  --changed`: exit 0, registry 54 tasks/161 acceptance items; 50 changed
  Rust files, largest 4062 physical lines, no over-5000 warning. The first
  Python run during a concurrent parent-owned planning edit reported four
  registry/state-mismatch errors; the parent independently delivered/reconciled
  that docs-only plan in `19b30a639`, and the unmodified 47-test suite passed.
- Final normal `cargo build --locked` and `cargo build --release --locked`:
  exit 0; `target/{debug,release}/oc --help`: both exit 0. After these builds,
  **direct retained** `target/debug/deps/recovery_startup-23751e4489435732`
  (4 pass), `configured_workspace-8749782c82773bf5` (13 pass),
  `pty_t39-5105e138ceb60f7c` CFG10 fatal (2 pass), CFG09 plugin (1 pass),
  UI07 provider (4 pass), and `storage_lock-c9dce5f6fb92c650` (1 pass):
  exit 0, 25 passed, zero failed, `--test-threads=1`; no intervening Cargo
  invocation changed the root binaries. Final SHA256 verified both after
  builds and after direct checks: debug
  `000051df54ecb8d102f1a12352d996c35ce44b4504bc02aa7ca7292258b85e7f`,
  release `968bf153b5633dbb0ae1fb25843620b11e15d9b76dc1a94299caf3d9d7e4651d`.
  These are R3 **dirty-source** artifacts: at build time the Git HEAD was
  docs-only `986e9bab0657a9b19f3a3a136d35d78a01230611` plus the
  reviewed R3 Rust/test diff, **not** clean-parent binaries. The subsequent
  parent-owned `19b30a639` commit changed docs/registry/journal only;
  current HEAD plus that same dirty Rust/test diff still identifies the
  compiled source. `fcfdd584b` is the last committed Rust source base.

## Risks

Mandatory config has no typed metadata-only document authority. Broken JSONC
must not be inspected heuristically to infer missing security fields. Existing
definition-only paths provide the narrow optional boundary. Preserve native
discovery oracle/deadlines, existing immutable generations and MCP owners.
T53 authentication/storage/protocol additions and T44 presentation remain their
separate owners; no live campaign/input, new dependency/schema or generic error
registry/retry/source engine belongs to this slice.
No known unresolved R3-scope defect or external blocker. The later
owner-approved T51 R4/E2E06 missing-saved-agent/variant local availability and
real-user default/fresh-store PTY/strace are **not** qualified by these
isolated CFG10 fake-fixture binaries; T54 provider retry/UI parity, T53 Go/auth
and paused T44 visual gates are separate pending work. No user config,
protected inputs, campaign journals, progress statuses, lockfile, database
schema, source limits or credentials were changed.

## Next

Parent independently reviews the R3 diff/claims and delivers if qualified;
then resumes the explicitly separate T51 R4/E2E06 slice before any T51 task
finish or A13/whole-goal claim. The later T53/T54 and paused T44 plans retain
their own owners. No coordinator stage/commit/push/progress mutation was made.
