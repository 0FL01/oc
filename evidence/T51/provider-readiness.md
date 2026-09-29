# T51 R2 / UI07 — frozen provider-readiness slice

## Coordinator qualification — 2026-09-29

The coordinator reviewed all tracked production/test/documentation diffs and six
new owner/test parts, including pre-acceptance root/profile/child/title/compaction
admission, the owned catalog job, canonical binding comparison, scoped TUI/headless
queries, and checked retirement. A targeted independent read-only scope review
found no concrete introduced violation. No additional production change was needed.

During this slice the owner delivered documentation-only plan commit
`fb9b7768b5f09cb40423519f74bce57a488fb8e7` after the frozen source parent below.
Its thirteen paths contain no Rust, dependency, schema or fixture code. Those owner
changes are preserved: T53 is a separate pending task, not an expansion of R2, and
T44 remains paused. Current R2 artifacts include the same reviewed dirty Rust diff;
they are not clean-parent or implemented-T53 artifacts.

Independent current checks all exited 0: adapter owner `ui07_` **3 passed**
(including the intentionally panicking join fixture whose cleanup must refuse),
runtime `ui07_` **1 passed**, TUI `ui07_` **1 passed**, fmt, Python code-size **5**,
progress **15**, docs **14**, structural validators and diff check. The current
advisory inventory contains 273 files and no >5,000-line warning. The coordinator
checked all 42 current workspace-result summaries: **1,277 passed, 0 failed,
10 unchanged opt-in ignored**. No Rust changed after that full gate.

After the final normal root `cargo build --locked`, the coordinator directly ran
the retained native provider matrix (**4 passed**, actual native deadline unchanged),
configured-workspace target (**13 passed**), mixed-plugin case (**1 passed**) and
startup/recovery target (**3 passed**). Debug/release digests below were identical
before and after those runs. Refusals, exact wire IDs, typed read-only status,
terminal restoration, mandatory policy/trust and the R1 inventory remain verified.
No live request was made or journal reset; existing live consumption remains
8 generation / 1 search / 19 control. General fatal-source R3 remains next.

## Result

Pre-change freeze at source parent `33a57bc985124a15b38beb3e501c9b7682630350`.
Tracked worktree was clean at that freeze; inherited `.opencode/` is untouched.
Implementation and qualification were pending. Temporary sole mutation/Cargo/native-fixture coordinator;
parent owns task/progress/status/acceptance and delivery.

Frozen obligations:

1. Cold selected-provider discovery timeout/connect/auth failure or missing selected
   credential leaves local TUI/history/model picker alive and usable. Pending,
   unavailable/failed and ready are typed facts of the existing application/runtime
   owner, visible before prompt; compiled alias binding is not provider availability.
2. Configured and persisted selection remains explicit. No fallback, mock response,
   fabricated connection, stale retired model advertised as working, or successful
   refresh retaining local-override ids absent from the remote catalog.
3. Every root/profile/child request requiring unavailable credentials/model fails
   centrally before new generation/turn/effect acceptance, with typed safe actionable
   outcome; headless is nonzero. Existing authority narrowing and atomic admission hold.
4. Explicit admitted bounded refresh/retry or explicit valid selection restores ready
   state, and first/next actual wire request uses exactly that selected id. Failed
   refresh retains last complete healthy catalog/selection; empty cold result cannot
   erase configured models. Discovery metadata/precedence/publication/retry/deadline
   oracle and budgets remain unchanged; no automatic paid retry layer is added.
5. Shared existing diagnostic DTO projections expose safe source/field/service/stage/
   code/retryability/action without raw credentials, URLs, paths, control sequences or
   synthetic config/env secrets in stderr/UI/details/copy/investigation drafts.
    Invalid mandatory config/policy/trust/storage/recovery/cleanup/caps remain fatal;
    R1 inventory/admission and complete-generation rollback remain intact.

**Qualified result:** R2/UI07 and the minimal coupled provider diagnostic seam are
implemented in the dirty worktree based on the source parent above. No task/status,
progress, acceptance, stage/commit/push or general fatal-source R3 changes were made.

### Per-obligation result and ownership

1. The TUI's existing application owner starts after complete local config/policy,
   data-root/recovery and MCP-owner admission; one optional read-only catalog job
   finishes at its safe boundary. `ProviderReadiness` reports actual pending,
   unavailable, failed and ready facts. Local history, fork, model picker, Settings
   and unsent drafts remain usable during missing-key/connect/auth/native-deadline
   failures. Compiled DCP binding remains active independently of provider readiness.
2. Exact configured/profile/persisted choices stay explicit. The pending-profile
   owner test preserves an unknown selected ID, removes a retired local override
   after a successful remote-only catalog, and keeps the persisted retired choice
   unavailable until explicit replacement. Old snapshots remain immutable; no
   provider/model fallback, invented metadata or mock generation response is added.
3. `Runtime::admit_provider` is the shared pre-MCP/pre-turn/effect guard for root,
   fresh-root, profile and child execution; application title/manual-compaction
   admission and runtime compaction delivery use the same fact. Missing declared
   credentials cannot be bypassed by a standalone runtime's valid request config.
   Refused requests create no accepted turn, title request or tool effect. Headless
   refuses before a new root for an unavailable default; an explicit stored root
   may instead use its admitted scoped choice. Ready defaults preserve the existing
   detailed foreign-Location admission; runtime checks the actual scoped request.
4. Explicit reload or valid model selection repairs readiness and the first/next
   captured wire ID is exact. Failed refresh retains the prior complete generation,
   catalog, selection and policies, publishing only the latest safe attempt fact.
   Known 401/403 for the same effective wire binding centrally refuses requests;
   URL trailing-slash/configured reserved-header spelling cannot evade that fact.
   Native `discovery.rs`, negotiation/merge/removal/source authority, 15s attempt /
   30s total budget and retry delays are unchanged. Parallel reload while the cold
   job is pending is explicitly refused rather than creating another retry budget.
5. Existing `ServiceDiagnostic` gains only provider kind, models-list stage and
   necessary codes/actions; `TuiChrome.provider`, typed `CoreError::ProviderUnavailable`
   and payload-free `ProviderChanged` carry owner facts to scoped query consumers.
   Service/model identities are opaque SHA-256 qualifiers; source uses the existing
   safe source qualifier. Display/Debug diagnostics, read-only details, stderr and
   TTY canaries are bounded and do not retain endpoints, remote error bodies,
   headers, credentials, private paths or controls. Mandatory malformed transport,
   policy/trust/storage/recovery/cleanup/caps retain non-success and rollback.

Production owners: `composition/provider_readiness.rs` holds one admitted provider's
ephemeral facts; `application/provider_catalog.rs` owns its single optional job;
`provider.rs::same_request_binding` compares canonical actual URL/header maps;
runtime owns request admission; Core owns the DTO/error/event contract; binary
TUI/headless read scoped owner queries. No second registry/store, new dependency,
DB schema, retry framework or test-only public API was introduced. Public runtime
constructors retain legacy budget/model-validation defaults; low-level public config
assembly still rejects missing selected credentials. New substantial tests are
separate owner parts under the existing library/integration Cargo roots.

## Checks

Planned RED/GREEN: closest owner admission/cold-load tests and an actual-binary
fake discovery/Responses fixture with barriers/errors, durable history, missing
credential, retained prompt, explicit alternative selection and admitted repair/
refresh, exact outbound id and zero pre-refusal generation/tool effects.
Reuse DISC01–DISC10/PROV06/UI02/UI05, retirement/generation/cancel/recovery and
R1/DCP/MCP/source-policy gates. New test parts stay under existing Cargo targets.

One Cargo command at a time: jobs=3, test threads=1, offline,
TMPDIR=`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
tool timeout=900000ms. Final workspace fmt/strict all-target locked Clippy/tests
`--no-fail-fast`, normal root debug/release builds/help, then direct retained native
executable checks against recorded dirty-tree binary digests.

### RED/GREEN and regression review

- Initial exact owner missing-credential case: RED exit 101, one failed (old cold
  spawn rejection); GREEN. Introduced-risk RED cases then covered pre-effect manual
  compaction, local history fork, configured-model acceptance while native auth was
  pending, same-effective-binding auth rejection, unsafe unavailable-ID footer,
  unexpected completed catalog-worker panic cleanup, and headless stored selection.
  All are green in the final workspace and native runs.
- Existing foreign-Location configured/golden binary assertions exposed a new
  preflight diagnostic regression (RED); restored the original ready-default
  Location gate without changing either assertion. Both targets are green.
- Superseded assertions are limited to old app-wide cold credential/discovery
  rejection, the new legitimate Settings provider row, typed retired-model refusal,
  and failed-refresh latest-attempt diagnostics. Old trust/side-effect/retirement,
  selection, generation and permissions assertions remain. R1's exact discovery
  count now waits for the actual discovered picker row before the first prompt.
- New PTY synchronization asserts reconstructed persistent owner status instead of
  raw CSI deltas or a replaceable reload toast, and waits for the actual picker
  title before selecting. Existing PTY deadlines/caps/ignored/goldens are unchanged.
- Earlier full runs were non-success and are not counted as PASS: approved old
  cold-start assertions and newly introduced issues were diagnosed/fixed; unchanged
  VIS38/S07 and sub-second SIGINT checks had isolated timing failures, then passed
  both nearest reruns and final full qualification. No thresholds or test assertions
  were weakened. Harness logs retain those failed runs, including
  `tool_0ecc6fe0c0016Po1FFILluYI0c` and `tool_0ed060396001AiFyQE15nISwnS`.

### Final commands and results

All commands ran in `/home/opencode/ai/oc`, non-root UID 1003. Cargo environment:
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 CARGO_NET_OFFLINE=true`, the TMPDIR above,
one Cargo command at a time, 900000ms tool timeout. Every final command below exited 0.

| Command | Actual result |
| --- | --- |
| `cargo fmt --all --check`; `git diff --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS, no suppressions |
| `cargo test -p oc-adapters --lib --locked -- --test-threads=1` (affected pass before final worker test) | 338 passed |
| `cargo test -p oc-adapters --test runtime --locked -- --test-threads=1` | 95 passed |
| `cargo test -p oc-adapters --lib --locked ui07_ -- --test-threads=1` | 3 passed, including owned worker-panic cleanup |
| `cargo test -p oc-tui --locked ui07_ -- --test-threads=1` | 1 passed |
| `cargo test --workspace --locked --no-fail-fast -- --test-threads=1` | **1,277 passed; 10 unchanged opt-in ignores; zero failures** |
| `cargo build --locked`; `cargo build --release --locked` | final normal root debug/release builds PASS |
| `target/debug/oc --help`; `target/release/oc --help` | both PASS |

Final full log is the harness-owned
`/home/opencode/.local/share/opencode/tool-output/tool_0ed159480001fdK0QQ015wcq15`
(1,542 lines). Full qualification includes DISC01–10/PROV06/UI02/UI05, native
Core/path/trust/policy/retirement/generation/cancel/recovery/DCP/MCP gates; binary
unit 85, adapter unit 339, runtime integration 95, Core 26, TUI 419, configured
workspace 13 and PTY T39 44. `live_bounded` ran only its offline/fake checks:
15 passed/3 existing opt-in or internal ignores. No real/live campaign was invoked.

After the final normal builds, the retained native executables were run directly
with `--test-threads=1` and the same offline/TMPDIR context, without intervening Cargo:

| Retained executable / filter | Actual result |
| --- | --- |
| `target/debug/deps/configured_workspace-8749782c82773bf5` | 13 passed |
| `target/debug/deps/pty_t39-5105e138ceb60f7c provider_readiness::` | 4 passed / 40 filtered |
| same PTY executable, exact `plugin_admission::cfg09_native_mixed_plugins_reload_reopen_and_effects_are_truthful` | 1 passed / 43 filtered |
| `target/debug/deps/recovery_startup-23751e4489435732` | 3 passed, including all existing startup/discovery subcases |

### Actual UI07 effect counters

`crates/oc/tests/pty_t39/provider_readiness.rs` extends only the existing owned
fixture with typed GET status/barrier controls. All child environments are synthetic;
fake GET/Responses routes validate actual method/path/auth. The final direct matrix:

| Scenario | Discovery GET | Main Responses | Title Responses | Exact request identity / refusal |
| --- | ---: | ---: | ---: | --- |
| Stored history → missing key → admitted repair/reopen → scoped headless with unavailable config default | 3 | 3 | 1 | all main requests `ALT_MODEL`; missing-key prompt/headless refusal adds zero |
| Cold 401 → admitted remote-ID repair → same effective binding 403 failed refresh → repair → headless 401 | 5 | 2 | 1 | first/next `DYNAMIC`; every auth refusal adds zero |
| Native 30s discovery deadline with local pending UI → explicit configured alternative | 2 | 1 | 1 | `ALT_MODEL`; both held GET clients close; pending refusal adds zero |
| Owned closed-loopback connection, unsafe unavailable selected ID | 0 at fake peer | 0 | 0 | explicit failed owner state, live picker, retained refused draft, no generation |

All captured main/title JSON excludes synthetic config/environment canaries, all
PTY exits restore terminal state and emit ALT_LEAVE, and error TTY/stderr contains
no raw canary/auth URL/path/controls. Missing/auth headless refusal is nonzero with
empty stdout; existing JSON-output checks remain NDJSON-only. R1's directly rerun
mixed-alias test also proves actual discovery/DCP effects and reload/reopen inventory.

### Artifact association / reviewed dirty tree

These are **not clean-parent artifacts**. Source association is parent
`33a57bc985124a15b38beb3e501c9b7682630350` plus the reviewed R2/UI07 dirty code/test
diff and live documentation/report paths. No stage/commit was performed.

- `target/debug/oc` SHA-256: `b595eb222de0aaf581b628e2690d1de7db24ad31fabfab9c1a900bcf209a629d`
- `target/release/oc` SHA-256: `313e9f088ceac0745ab211bcdea79a895280e837c168fa59f275c6e9984eaeba`

Both hashes were measured immediately after normal builds and repeated after all
direct native checks, unchanged. Retained integration executables embed the normal
debug root path; no Cargo root-binary rebuild occurred between those measurements.

Reviewed mutation paths: adapters `application.rs`, `application/tests.rs`,
`application/provider_tests.rs`, `application/provider_catalog.rs` and its `tests.rs`,
`composition.rs`, `composition/provider_readiness.rs`, `config.rs`, `provider.rs`,
`runtime.rs`, `runtime/turn.rs`, `runtime_compaction.rs`, existing runtime test root /
turns part plus `tests/runtime/provider_readiness.rs`; Core `core_app.rs`, `queries.rs`,
`session.rs`; TUI `app/input.rs`, `app/live.rs`, existing input tests; binary
`approval_tests.rs`, `headless.rs`, `tui_cmd.rs`; existing offline live-bounded helper,
PTY root/interaction/lifecycle/plugin part plus provider part, startup/discovery
Python support; `docs/CODE_MAP.md`, `docs/CONFIG.md`, and this report. The inherited
`.opencode/` was never inspected or modified. No large report/capture/log artifacts
were added to the repo, and foreign/historical data was not deleted or overwritten.

## Risks

No unresolved R2/UI07 scope bug or external blocker is known after final qualification.
`ready` means provider-layer request admission (credential + exact model metadata,
first native attempt no longer pending), not Connected/network health or permission/
cap success. A non-auth catalog failure may leave a configured/last-healthy known
model request-admissible; the actual deadline fixture proves its Responses route.
The unchanged native discovery error type combines connect/deadline as `Network`;
the shared safe code is `connection_failed`, without decoding exception strings.
Embedded/headless spawn still awaits the existing bounded attempt; TUI alone starts
its local owner asynchronously. General fatal source/field/stage classification,
optional-document policy analysis and T44/VIS42 presentation remain separate work.

## Next

Temporary mutation/Cargo/native-fixture ownership is released on return. Parent
independently reviews/verifies/delivers this dirty slice, then resumes general fatal
storage/trust/config source R3. This report does not close T51, T44 or GOAL A01–A13.
