# T50 R7 supplemental atomic — `oc models`

## Coordinator qualification — 2026-10-02

Independent review covered all catalog/config/source admission and native CLI
dispatch/output paths, meaningful owner tests and the actual ELF fixture. The
selection-template counterexample was reproduced and repaired before delivery:
catalog admission checks the field's shape without resolving or selecting it,
while ordinary application admission retains its original behavior. Policy,
source trust, required catalog credentials and capacity failures stay enforced.

Fresh coordinator checks all exited0:
- Catalog owner **4**, CLI output owner **2**, actual locked-store integration **1**.
- Workspace fmt and strict all-target locked Clippy; normal locked workspace build.
- Direct normal debug and release campaigns, **31 cases + five rejected forms each**.
  Each campaign observed **13 authenticated synthetic catalog GETs**, generation
  and other GET0, disabled peer0; exact output, safe errors, cancellation, no
  selection-file/store/prefs/loader opens and unchanged fixture bytes were checked.
- Python **47**, documentation/progress structural checks and diff check.

Current full-workspace source gate is the corrected-source **1426 passed / 0 failed /
10 unchanged opt-in ignores** log `t50-cli-selection-workspace.log` below. No Rust
source changed after that gate. Documentation was corrected to remove the obsolete
claim that Models is absent from the current parser.

Source association remains BASE `1335ef7d5ec96dfb3d340e47f9561ce3122071a9` plus the
reviewed dirty implementation, not a clean-BASE binary claim. Before and after both
fresh direct campaigns the normal ELF hashes were identical:
- Debug `9a265dd04e286c13eeb842a5ecc03d5c02b3c7375d135b7d7c3a4be9578b38da`.
- Release `f16d54862c35b9d7fee583ce6a57d8fb0fe0f01c5d97bc9d8eaf5683dd7f03ac`.

All owned fixture workers/processes joined and temporary roots were removed. No
live campaign, real-user configuration, paid generation or private credentials
were accessed. T45 profile binding, T53 catalogs and T50/R10 remain separately
pending; this receipt does not claim whole-T50, T44/V09 or product readiness.

## Result

Frozen before RED/production edits at BASE `1335ef7d5ec96dfb3d340e47f9561ce3122071a9`,
branch `agent/oc-rust-port`. This receipt qualifies only catalog-only CLI TOOL18.

| Frozen obligation | Qualification / result |
| --- | --- |
| `oc models`, applicable global `--data-dir`; no extra list/refresh/provider/JSON flags | PASS both normal ELFs; five rejected extra forms each exit2 |
| Complete exact enabled supported admitted references; full-reference lexical sort; nonempty final newline; empty exit 0 | PASS exact31 static lines, including25 same-family IDs, slashes/collisions/punctuation/Unicode; empty and absent config |
| Admission before default model, saved unavailable prefs irrelevant, no store/recovery | PASS corrected file/missing-env template independence, no-default/unavailable/blocked-data-root, locked native-store integration and actual syscall proof |
| Same source order/trust/policy/filter/merge owners; static metadata without generation key | PASS shared descriptor-pinned sources/DCP/settings/config assembly; global/project JSONC/local precedence and filters |
| Only own admitted dynamic OpenProxy binding/key; no arbitrary secret/endpoint resolution | PASS13 own authenticated GETs per ELF; unsupported/static/MCP file templates inert; disabled peer0 |
| Discovery success/retirement/all-or-nothing/auth/timeout/invalid/caps; partial independently known IDs only with nonzero | PASS original19 discovery regressions; exact ELF success/retirement and failure projections; strict row/body caps |
| Fatal policy/trust/caps nonzero, no successful truncated prefix; safe typed diagnostics | PASS fatal empty stdout, exit1;100001 static rows/oversize labels/control IDs/source and secret trust |
| Output/cancel nonzero and owned work cleanup | PASS broken pipe1, GET cancel130, blocked-output cancel130 with prefix and restored shared FD flags |
| Zero generation/title/tool/MCP/browser; no config/prefs/history writes | PASS fake generation/other GET0, byte snapshots unchanged, store/prefs/loader opens0, process markers absent |
| Serial current mandatory full gates, normal builds/fingerprints/help and direct executable proof | PASS corrected-source workspace1426/0/10, Python47, fmt/Clippy/build/help/docs/progress/diff/size; normal debug31 + release31 cases; fresh association at end |

Authority: GOAL.md:483–521; spec:209–324; TEST_PLAN CLI models section.
Pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`, commands.ts:283–286,
handlers/models.ts:12–24, server/handlers/model.ts:11–15. Native lexical rather
than locale/ICU sorting is declared. No new protocols/auth/models.dev or store.

## Checks

### Reopened coordinator finding — selection template independence

Parent review identified that catalog's shared `admit_settings` call still resolved
the unrelated default `model` string via `substitute(..., trusted=false)`. A healthy
static catalog with `model: "{file:must-not-read}"` therefore failed `trust_refused`.
This contradicts GOAL493–505's selection-independent listing; not a relaxation of
source/provider credential trust. Reopened sole mutation/Cargo authority on the same
BASE + preserved DIRTY slice. All prior1426 gates/normal hashes below are historical
until the fresh qualification recorded at the end of this receipt.

Before production changes, extended the existing static test with file and missing
env selection templates while retaining the unavailable case. Required sequence:
real RED → shape-only catalog settings → GREEN → fresh full gates and normal ELF
fixtures. Ordinary application selection resolution must remain exactly intact;
malformed `model`/known fields and source/policy/provider/catalog admission stay fatal.

RED: existing normal `target/debug/oc models` in owned cache cwd with synthetic
HOME, cleared environment: exit2, empty stdout, `unrecognized subcommand 'models'`.
Inherited normal debug hash `1e106ce25bb926a05f859987eb114ea7426604e68f7f545e6768972ff1b07843`.

Development attempt `t50-cli-dev-native.log` exit1: unchanged-fixture assertion
identified bootstrap's default trace creation under synthetic HOME. Diagnosis:
`trace::init_default` creates/truncates HOME state. Fixed by withholding ordinary
startup trace initialization for the read-only Models command; fixture assertion
retained. Owned TemporaryDirectory cleanup completed even on failure.

Development `t50-cli-locked-state.log` exit101: integration root module lookup
expected `tests/models_cli.rs`; diagnosed Rust integration-root resolution and
added explicit `#[path = "configured_workspace/models_cli.rs"]`. Existing Cargo
target retained; no assertion or runtime change.

Pre-output-cancellation full gate was PASS1426/0/10 (`t50-cli-workspace.log`).
Final review identified a synchronous-output cancellation risk after Tokio's
SIGINT handler was installed. Replaced blocking output with owned nonblocking
descriptors, bounded async waits/yields and one cancellation scope through GET,
diagnostics and output. Descriptor flags restore on exit; no detached tasks.
Added actual undrained-pipe SIGINT/prefix/non-success/flags-restored proof.
Current full gates are rerun after this production change below.

`t50-cli-clippy-final.log` exit101: the second fcntl unsafe block required its own
adjacent safety comment after formatting. Split the flag-read failure check and
documented the flag-update block; no lint allowances or assertion changes.

First final ELF attempt `t50-cli-native-debug.log` exit1: blocked-output fixture
used a 250ms sleep before SIGINT, which could interrupt synchronous config admission
before output/signal registration rather than the intended blocked-output state.
Replaced this timing assumption with a bounded OS write-readiness barrier proving
the undrained pipe filled. Exit130/safe stderr/prefix and flag-restoration assertions
remain exact. Production/ELFs unchanged; owned child group reaped and fixture removed.

Preflight: non-root uid1003; origin verified `git@github.com:0FL01/oc.git`;
HEAD matches BASE; only inherited untracked `.opencode/` (never inspected).
Disk178GiB free/MemAvailable5478MiB. Cargo coordinator exclusive; jobs3,
threads1, offline, owned disk TMPDIR, timeout900000. New log aggregate <1MiB.

### Initial source/executable association (historical; correction supersedes it)

**DIRTY source = BASE + reviewed changes**, not clean HEAD qualification. No
stage/commit/push or status/planning/spec mutation. Production source was frozen
before the final full gates/builds; subsequent changes are this receipt and the
offline harness's blocked-output barrier. Inherited `.opencode/` remains untouched.
No Cargo ran between the final normal builds, before fingerprints, help and direct
executable qualification, and after fingerprints.

| Normal ELF | SHA256 before AND after both final direct campaigns |
| --- | --- |
| `/home/opencode/ai/oc/target/debug/oc` | `74a28edb066389db66bc944baaed20c832ad860a8710ead7cf93088069e9f97b` |
| `/home/opencode/ai/oc/target/release/oc` | `53665adbc4ad76ba593a48d96a35f45511a0fe3c343260e1363536c192d7fe09` |

### Initial managed logs and commands (historical)

All new raw logs are under the exact managed directory
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`.
Commands below run at `/home/opencode/ai/oc`, through the serial bounded runner
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-check.py`.
Cargo environment: `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 CARGO_NET_OFFLINE=true`
and `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Timeout900000ms; no dependency/toolchain/lockfile/target-dir changes.

| Command / current result | Exit | Exact log path |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-fmt-final2.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-clippy-final2.log` |
| `cargo test --workspace --locked --no-run` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-precompile-final.log` |
| `cargo test --workspace --locked --no-fail-fast`:1426 passed/0 failed/10 existing ignores | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-workspace-final.log` |
| `cargo build --workspace --locked` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-debug-build.log` |
| `cargo build --workspace --release --locked` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-release-build.log` |
| `target/debug/oc --help` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-debug-help.log` |
| `target/release/oc --help` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-release-help.log` |
| `python3 evidence/T50/cli_models.py --binary target/debug/oc`:29 cases +5 rejected forms | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-native-debug2.log` |
| `python3 evidence/T50/cli_models.py --binary target/release/oc`:29 cases +5 rejected forms | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-native-release.log` |
| `sha256sum target/debug/oc target/release/oc` before proofs | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-fingerprints-before.log` |
| same fingerprint command after proofs | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-fingerprints-after.log` |
| `python3 -m unittest discover -s scripts -p 'test_*.py'`:47 (bounded_live13/code_size5/progress15/check_docs14) | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-python.log` |
| `python3 scripts/check_docs.py`:56 tasks/166 specifications | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-docs-final.log` |
| `python3 scripts/progress.py check` (read-only journal validation) | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-progress-final.log` |
| `git diff --check` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-diff-final.log` |
| `python3 scripts/code_size.py --base 1335ef7d5ec96dfb3d340e47f9561ce3122071a9 --changed`:13 code files,7848 lines, no changed file >5000 | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-size-final.log` |
| Directed `cargo test -p oc-adapters --locked --lib composition:: -- --nocapture`:26 | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-owner.log` |
| Directed `cargo test -p oc-adapters --locked --lib discovery::`:19 | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-discovery.log` |
| Directed `cargo test -p oc --locked --bin oc models_cmd::`:2 | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-cli-owner2.log` |
| Directed `cargo test -p oc --locked --test configured_workspace models_cli::`:1 | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-locked-state2.log` |

Earlier attempts retain distinct names, never overwritten: `t50-cli-dev-native.log`
(1), `t50-cli-locked-state.log` (101), `t50-cli-clippy-final.log` (101),
`t50-cli-native-debug.log` (1), with diagnoses above. Development success logs
`t50-cli-dev-native2.log` (24 cases) and `t50-cli-dev-native3.log` (29 cases) are
historical intermediate ELF receipts, not the final fingerprint association.

### Initial full workspace target summaries (historical)

All failures0; all measured/filtered0. Existing ignores remain exactly10.
The full per-test names, exits and elapsed summaries are retained in the current
full log. No tests/assertions/caps/deadlines/goldens were disabled or loosened.

| Crate / target | Passed | Ignored |
| --- | ---: | ---: |
| oc / bin unit | 87 | 0 |
| oc / application | 4 | 0 |
| oc / approval_binary | 1 | 0 |
| oc / configured_workspace | 14 | 0 |
| oc / dcp_runtime | 3 | 0 |
| oc / durability | 1 | 0 |
| oc / golden_binary | 1 | 0 |
| oc / live_bounded | 15 | 3 |
| oc / mcp_application | 41 | 0 |
| oc / memory_bounds | 1 | 0 |
| oc / pty | 16 | 0 |
| oc / pty_t39 | 48 | 0 |
| oc / pty_t42 | 34 | 0 |
| oc / recovery_startup | 4 | 0 |
| oc / recovery_v02 | 1 | 0 |
| oc / recovery_v03 | 1 | 0 |
| oc / responses | 4 | 0 |
| oc / t49_live_smoke | 1 | 4 |
| oc / tui_render_alloc | 1 | 0 |
| oc-adapters / lib unit | 436 | 0 |
| oc-adapters / blob_audit | 11 | 0 |
| oc-adapters / context_bounds | 3 | 0 |
| oc-adapters / dcp_atomic | 4 | 0 |
| oc-adapters / e2e_live | 0 | 1 |
| oc-adapters / e2e_offline | 3 | 0 |
| oc-adapters / mcp_remote | 27 | 1 |
| oc-adapters / mcp_stdio | 21 | 1 |
| oc-adapters / patch_audit | 10 | 0 |
| oc-adapters / patch_effects | 4 | 0 |
| oc-adapters / permissions | 8 | 0 |
| oc-adapters / runtime | 119 | 0 |
| oc-adapters / session_rename | 3 | 0 |
| oc-adapters / shell_watchdog | 2 | 0 |
| oc-adapters / soak | 4 | 0 |
| oc-adapters / storage_lock | 1 | 0 |
| oc-adapters / subagent | 14 | 0 |
| oc-adapters / tab_deck | 13 | 0 |
| oc-core / lib unit | 32 | 0 |
| oc-tui / lib unit | 433 | 0 |
| oc-adapters / doc | 0 | 0 |
| oc-core / doc | 0 | 0 |
| oc-tui / doc | 0 | 0 |
| **Total** | **1426** | **10** |

### Initial direct effects / cleanup (historical)

Each final ELF:29 reported cases, exits0×11/1×16/130×2; plus5 rejected forms
exit2. Catalog GET counts: success1/auth1/strict-invalid1/syntax4/rows-cap1/
body-cap1/control1/timeout2/cancel1 =13; all13 authenticate only with that
synthetic source's key. Disabled peer0; generation/title/compaction/other GET0.
Exact31 static lines; success retires the configured-only ID. Auth/invalid/timeout
failures print only known independent/configured IDs with incomplete/error stderr
and exit1; fatal controls/policy/trust/metadata caps print no prefix. Syscall case
confirms store/prefs/loader opens0; fixture byte snapshots and execution markers
confirm no configuration/history/prefs mutation or MCP/browser/plugin launch.
The native locked-store integration additionally seeds real owner-managed history,
unavailable prefs and a pending turn, holds its Db open, and proves unchanged bytes.

All CLI TempDirs/syscall files removed, non-daemon HTTP workers joined, owned child
groups reaped, no Cargo fixture target trees retained. The two workspace runs left
six known legacy `tui_workspace` unit roots (PIDs3976471/4004810), identified by
producer/source and serial gate timestamp windows, then removed by exact owned
paths only; `t50-cli-cleanup.log` exit0 records all six full paths. Foreign older
cache fixtures were untouched. New raw logs measured327482 bytes before cleanup
receipt,330417 bytes after final read-only checks, far below1MiB and16MiB per log.

### Changed paths (all intended, unstaged)

- `README.md`
- `docs/CODE_MAP.md`
- `docs/CONFIG.md`
- `crates/oc/src/cli.rs`
- `crates/oc/src/bootstrap.rs`
- `crates/oc/src/main.rs`
- `crates/oc/src/models_cmd.rs`
- `crates/oc/src/models_cmd/tests.rs`
- `crates/oc-adapters/src/composition.rs`
- `crates/oc-adapters/src/composition/catalog.rs`
- `crates/oc-adapters/src/composition/catalog/tests.rs`
- `crates/oc-adapters/src/config.rs`
- `crates/oc-adapters/src/models.rs`
- `crates/oc-adapters/src/models/lookup.rs`
- `crates/oc/tests/configured_workspace.rs`
- `crates/oc/tests/configured_workspace/models_cli.rs`
- `evidence/T50/cli-models.md`
- `evidence/T50/cli_models.py`

## Risks

Full T50/project readiness remains open. T44 remains paused. Catalog visibility
does not establish generation readiness. T53 Go/custom catalogs and T45 profile
grammar/binding consumer are explicit separate remainders.

## Next

Parent reviews this unstaged atomic and handles checkpoint/delivery. No blocked
frozen CLI outcome remains. T53 Go/custom source integration and T45 listed-ID
profile consumer remain separate; this is not full T50/T44/project READY.
Temporary mutation/Cargo/owned-offline-fixture ownership is released on return.

## Selection-template correction — fresh qualification

RED: `t50-cli-selection-red.log` exit101,0 passed/1 failed/0 ignored. The existing
static catalog test's file-template variant failed at `load_catalog_with_env` with
typed `Admission/TrustRefused`, field `file`, before any provider discovery. The
missing/unavailable selection variant had passed; this was a real uncovered source
dependence, not a proposed schema/credential-policy relaxation.

Minimal fix: shared `admit_settings(..., resolve_selection)` validates the same
known field shapes in both modes. Ordinary application passes `true`, retaining
exact substitution/selection/default-agent semantics. Catalog passes `false` and
does not substitute or choose default model/agent. No ignored/fake selection prefix
or additional reader. Source/DCP/settings/policy/provider/catalog admission remains
shared and mandatory; malformed `model` and plugin types still fail.

GREEN: `t50-cli-selection-green.log` exit0,4 passed/0 failed/0 ignored. Existing
static test retains unavailable/no-key/inert connections/>20 IDs and adds file and
missing-env selection templates. It verifies no catalog selection is captured and
ordinary admission still rejects file selection (`TrustRefused`) and expands a
missing env to the same empty selection while preserving default agent.

Normal-ELF harness adds two exact-output file/missing-env cases. Its saved-state
syscall case now uses both saved and default selection file templates and forbids
any `must-not-read` open, alongside store/prefs/loader0 and unchanged bytes. Prior
29-case/hashes and1426 full gates are historical; fresh full qualification follows.

### Fresh corrected-source Result / Checks / Risks / Next

**Result: PASS**, same frozen catalog-only slice; review counterexample resolved.
Fresh source association is still **DIRTY BASE1335 + preserved initial CLI slice +
selection-template correction**, not clean HEAD. Ordinary selection admission passes
`true`; catalog shape-only admission passes `false`. No production change after
fresh fmt/Clippy/full workspace gates. No Cargo between fresh normal builds,
fingerprints before, both helps/direct campaigns and fingerprints after.

| Corrected normal ELF | SHA256 before AND after fresh direct proofs |
| --- | --- |
| `/home/opencode/ai/oc/target/debug/oc` | `9a265dd04e286c13eeb842a5ecc03d5c02b3c7375d135b7d7c3a4be9578b38da` |
| `/home/opencode/ai/oc/target/release/oc` | `f16d54862c35b9d7fee583ce6a57d8fb0fe0f01c5d97bc9d8eaf5683dd7f03ac` |

Fresh Cargo environment/serial coordinator/timeouts/owned cache are unchanged from
the explicitly recorded initial managed directory. Full exact current log paths:

| Fresh command / summary | Exit | Exact log path |
| --- | --- | --- |
| Static-template RED, exact directed test:0 passed/1 failed/0 ignored, typed TrustRefused | 101 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-red.log` |
| `cargo test -p oc-adapters --locked --lib composition::catalog::`:4 passed/0 failed/0 ignored | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-green.log` |
| `cargo fmt --all -- --check` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-fmt.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-clippy.log` |
| `cargo test --workspace --locked --no-run` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-precompile.log` |
| `cargo test --workspace --locked --no-fail-fast`:1426 passed/0 failed/10 existing ignores | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-workspace.log` |
| `python3 -m unittest discover -s scripts -p 'test_*.py'`:47 (13/5/15/14) | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-python.log` |
| `cargo build --workspace --locked` normal debug | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-debug-build.log` |
| `cargo build --workspace --release --locked` normal release | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-release-build.log` |
| `target/debug/oc --help` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-debug-help.log` |
| `target/release/oc --help` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-release-help.log` |
| `python3 evidence/T50/cli_models.py --binary target/debug/oc`:31 cases +5 rejected forms | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-native-debug.log` |
| `python3 evidence/T50/cli_models.py --binary target/release/oc`:31 cases +5 rejected forms | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-native-release.log` |
| `sha256sum target/debug/oc target/release/oc` before fresh proofs | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-fingerprints-before.log` |
| same fingerprint command after fresh proofs | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-fingerprints-after.log` |
| Exact owned legacy workspace-fixture cleanup | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-cleanup.log` |
| `python3 scripts/check_docs.py`:56 tasks/166 specifications | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-docs.log` |
| `python3 scripts/progress.py check` read-only | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-progress.log` |
| `git diff --check` | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-diff.log` |
| `python3 scripts/code_size.py --base 1335ef7d5ec96dfb3d340e47f9561ce3122071a9 --changed`:13 code files/7883 lines, no changed file >5000 | 0 | `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/t50-cli-selection-size.log` |

Fresh full workspace's42 target summaries have exactly the same pass/ignore counts
as the initial table above, including bin87, configured_workspace14, adapters436,
runtime119, core32, tui433; all failed/measured/filtered0. The current full log
retains every target, test name and elapsed summary. Counts remain1426 because the
existing static scenario was extended, not duplicated into a new target/test count.
Malformed-model, policy, source and dynamic-secret trust, provider/catalog caps and
all prior no-default/unavailable cases remain GREEN. No disabled tests, lint
allowances, relaxed assertions/caps/deadlines/goldens or schema changes.

Each fresh ELF:31 reported cases, exits0×13/1×16/130×2; five extra forms still
exit2. Both file and missing-env default templates print the exact same31 static
IDs with no generation/default/session resolution. An existing selection-file
sentinel is never opened: the syscall proof uses saved AND default file templates
and observes selection-file/store/prefs/loader opens0. Bytes/markers unchanged.
Dynamic counts remain13 catalog GETs/13 own auth, generation/title/other GET0,
disabled-peer0; output/GET cancellation and flag restoration remain qualified.

Only these five paths changed during the reopened correction:
`crates/oc-adapters/src/composition.rs`,
`crates/oc-adapters/src/composition/catalog.rs`,
`crates/oc-adapters/src/composition/catalog/tests.rs`,
`evidence/T50/cli_models.py`, `evidence/T50/cli-models.md`.
The entire initial18-path slice remains preserved/unstaged; no other writer work
was overwritten, no acceptance/planning/progress/spec/GOAL or credential changes.

Fresh CLI TempDirs/syscall files removed, non-daemon peer workers joined, own child
groups reaped. Fresh workspace's three legacy `tui_workspace` roots, PID4051907,
identified within this run's precompile→workspace window and exact known producer,
were removed by exact paths only (full paths in fresh cleanup log). No fixture
target tree retained, foreign earlier roots untouched. All new logs, including
initial history, total499254 bytes after fresh cleanup/read-only checks; each is
≤130909 bytes, comfortably below16MiB/per-log and1MiB aggregate.

Risks/Next: T53 Go/custom source integration and T45 listed-ID profile consumer,
R10/T44/full READY remain separate. Parent performs the next full review and owns
checkpoint/delivery. Temporary mutation/Cargo/owned-fixture authority is released
on return; no blocked frozen correction outcome remains.
