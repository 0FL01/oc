# T51 R1 / CFG09 — frozen plugin-admission slice

## Coordinator qualification — 2026-09-29

The coordinator independently reviewed all tracked production/test/documentation
diffs and all four new test parts against source parent
`1dfb18456d82694bcd32dd045d3b0b69a53269ac`. The exact classifier stays before effects;
admitted root descriptors are reused instead of re-canonicalizing raw roots. The
presentation window does not stop admission, current identity exists only for a
compiled active binding, and invalid mandatory reload preserves the prior complete
generation. Shared source sanitization leaves MCP semantics unchanged. No new
registry, persistent schema, plugin host, permission grant or network owner exists.

Independent current checks all exited 0: adapters `cfg09_` **3 passed**; TUI
`cfg09_` **1 passed**; fmt; Python code-size **5**, progress **15**, docs **14**;
documentation/progress structure; diff check. The current advisory inventory has
267 files and no file over 5,000 physical lines. The coordinator checked all 42
workspace-result summaries in the retained current-source log below: **1,268
passed, 0 failed, 10 existing opt-in ignored**; no source changed after that gate.

After the last normal root `cargo build --locked`, the coordinator directly ran
the existing configured-workspace executable (**13 passed**) and exact mixed-plugin
PTY case (**1 passed**). The debug/release digests below were identical before and
after these actual-native checks. These are dirty-R1 artifacts, not clean-parent
artifacts. No live request was made; the existing live ledger remains consumed at
8 generation / 1 search / 19 control. R2 and general R3 remain open.

## Result

Pre-change freeze at source parent `1dfb18456d82694bcd32dd045d3b0b69a53269ac`.
The obligations below were recorded before RED. R1/CFG09 implementation and offline
qualification PASS on the dirty working tree described below. This is a slice report;
task/progress/acceptance, full T51/T44/GOAL qualification and delivery remain parent-owned.

Frozen obligations:

1. Exact compiled DCP and admitted-root `openproxy-models.js` aliases remain
   idempotent; authoring-only marker is ignored, never an active capability.
2. Each unknown/lookalike/version/path request becomes source-qualified typed
   failed/UnsupportedPlugin before plugin resolution, file-as-code, import,
   external loader/process or network. Healthy siblings and local runtime survive.
3. Current-generation bounded inventory carries safe opaque requested/current
   identity, typed status/module and source/field/stage/code/action; failed and
   ignored requests cannot masquerade as active. Available before prompt and after
   reload/reopen, through the existing Core catalog/TuiChrome diagnostic seam.
4. Diagnostics, Debug/UI/stderr/history and investigation/copy surfaces never
   retain raw rejected identities, URL credentials, absolute paths, controls or
   synthetic config/environment secret canaries.
5. Failed security-relevant reload keeps the previous complete generation.
   Trust/policy/storage/recovery/cleanup/caps and native mandatory DCP policy
    errors remain non-success. Genuine optional compiled setup failure may be
    isolated only at an actually safe owner boundary, without fake activation.

### Observed result against the freeze

| Obligation | Evidence and result |
| --- | --- |
| 1 — compiled aliases / marker | Exact bare/pinned/latest DCP requests share the fixed compiled revision; repeated native catalog aliases perform one discovery setup per complete load. Owner inventory deduplicates active modules and marks the authoring-only marker ignored with no current module. The mixed binary scenario executes native compression and commits one durable DCP block; each ordinary request has one compress schema and one instructions sentinel. |
| 2 — per-entry rejection before effects | Existing AUD17 is changed only to the approved success-with-failed-entry semantic. It asserts zero plugin network/loader effects while a valid Responses request succeeds. The mixed PTY fixture rejects bad versions, lookalike/native-file paths and authenticated/control-bearing URLs; exact native alias is an owned FIFO (never opened as code), rejected JS cannot create its marker, Node/Bun/npx traps remain untouched, and the fake listener accepts only admitted catalog/Responses routes. |
| 3 — truthful current generation | `CatalogSnapshot.chrome.plugins` carries typed Active/Failed/Ignored entries, opaque exact requested ids, compiled current ids only for active aliases, source/field and typed Plugin/Capability/UnsupportedPlugin/ReviewConfiguration diagnostics. Settings reads these owner facts before prompt. Reload from mixed aliases to only a rejected revision publishes no active/current module; reopen retains that state and usable durable history. Later aliases beyond 64 visible entries still activate, with honest omitted/failed counts. |
| 4 — safe projections | Config classifier and native DCP resolver Error Display/Debug contain only opaque rejected ids. Real application diagnostic Debug and actual merged PTY/stderr output/history never contain the synthetic auth/path/config/env/control canaries. Headless success keeps diagnostics on stderr. Settings details and existing note/copy surfaces receive the safe DTO rendering, never raw plugin text. |
| 5 — mandatory admission / atomic reload | Real owner invalid permission and keybind reloads fail and leave the entire previously published catalog equal. Existing trust/source/FIFO/policy/storage/cleanup/cap/recovery tests stay green, including direct final-binary configured-workspace cases. The aliases bind already-compiled native capabilities and have no independent fallible plugin initializer; no setup success is fabricated and no mandatory DCP/provider/storage error is blanket-caught. Provider readiness remains its existing separate owner. |

### API / ownership review

- `oc-core/src/queries.rs` extends existing `ServiceDiagnostic` with typed
  `ServiceKind` (old serialized diagnostics default to MCP) and `UnsupportedPlugin`,
  plus `PluginInventory`/`PluginEntry` under existing `TuiChrome`. No Core command,
  persistent schema, registry, lifecycle owner or generic framework is added.
- `config.rs::classify_plugin` remains the exact pre-resolution gate; unsafe error
  identities are hashed at construction. `config/mcp.rs::safe_source_id` shares the
  existing value-free source qualifier; MCP normalization/lifecycle behavior is unchanged.
- `composition.rs` classifies each request against already-admitted canonical roots
  and publishes its complete-generation inventory. Existing `application.rs`
  snapshot/reload ownership remains atomic; its only production-file change is a
  test-module declaration (plus rustfmt ordering).
- `oc/src/tui_cmd.rs` consumes typed inventory at startup, Location adoption and
  reload. `oc-tui/src/app/input.rs` adds read-only Settings rows using the existing
  selection dialog; plugin navigation/Enter/Left/Right never changes permission mode.
- New substantial unit scenarios are in `src/{config,composition,application}/plugin_tests.rs`;
  the actual mixed scenario is `oc/tests/pty_t39/plugin_admission.rs`, inside the
  existing target. The shared PTY fake adds only a catalog-request counter.
  Existing public module paths, Cargo targets and DCP/upstream provenance remain.

## Checks

One offline Cargo command at a time, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`,
`CARGO_NET_OFFLINE=true`,
TMPDIR=`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
tool timeout=900000ms. No live/paid/browser/npm/download or real external MCP calls.

| Command / retained check | Exit / result |
| --- | --- |
| Pre-change `cargo test -p oc --test configured_workspace --locked aud17_binary_unknown_plugin_is_isolated_without_loader_side_effect -- --exact --nocapture` | RED: 101, 1 failed; actual binary exits 1 with the old app-wide/raw UnsupportedPlugin error. A prior partial exact filter selected zero tests and is not RED evidence. |
| Same AUD17 exact command after implementation | 0; 1 passed |
| `cargo test -p oc-adapters --lib --locked cfg09_ -- --nocapture` | 0; 3 passed |
| `cargo test -p oc-tui --lib --locked cfg09_ -- --nocapture` | 0; 1 passed |
| `cargo test -p oc --test pty_t39 --locked plugin_admission::cfg09_native_mixed_plugins_reload_reopen_and_effects_are_truthful -- --exact --nocapture` | 0; 1 passed after bounded fixture synchronization/fake continuation expectations were corrected; no product timeout/cap/golden changed |
| `cargo test -p oc-adapters --locked` | 0; 558 passed, 3 existing opt-in tests ignored |
| `cargo fmt --all --check` after formatting / final `git diff --check` | both 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0; no suppression |
| `cargo test --workspace --locked -- --test-threads=1` | 0; 1,268 passed, 10 existing opt-in tests ignored; includes core 26, adapters lib 336, TUI lib 418, binary unit 85, configured-workspace 13, PTY T39 40, all other existing targets/doc-tests |
| Final normal root `cargo build --locked` / `cargo build --release --locked` | both 0 |
| Final debug and release `oc --help` | both 0 |
| Direct existing `target/debug/deps/configured_workspace-8749782c82773bf5 --test-threads=1 --nocapture` after final normal builds | 0; 13 passed against the final normal debug binary |
| Direct existing `target/debug/deps/pty_t39-5105e138ceb60f7c plugin_admission::cfg09_native_mixed_plugins_reload_reopen_and_effects_are_truthful --exact --test-threads=1 --nocapture` after final normal builds | 0; 1 passed, 39 filtered against the final normal debug binary |

Workspace log is the harness-retained 1,534-line output
`/home/opencode/.local/share/opencode/tool-output/tool_0ec57d958001Ad6GhP5HPVgu3l`;
no raw live responses or campaign artifacts were created. New repo report/fixtures
are small source text. Direct checks use the existing executable targets; no Cargo
binary rebuild intervenes after the final normal debug/release build.

### Artifact association / reviewed dirty working tree

Source parent is still `1dfb18456d82694bcd32dd045d3b0b69a53269ac` on
`agent/oc-rust-port`. These artifacts include the uncommitted R1 production changes;
they are **not** clean-parent artifacts:

- `target/debug/oc`: `ccbcb4941eda46c9dcebbcee5de5080c64cb28a611481c36007a249ee88f79c1`
- `target/release/oc`: `02079a44b6b58525877b75d57a084eef3d417148f5628792dc5c8305cc77d948`

Reviewed tracked production diff: `oc-core/src/queries.rs`;
`oc-adapters/src/{config.rs,config/mcp.rs,dcp_auto.rs,composition.rs,application.rs}`;
`oc-tui/src/app/input.rs`; `oc/src/tui_cmd.rs` (all under `crates/`).
Tracked tests: `oc-tui/src/app/tests/input.rs`, `oc/tests/configured_workspace.rs`,
`oc/tests/pty_t39.rs`. Four new test parts listed in API ownership above are untracked
for parent review/staging, as is this report. Documentation: `docs/CONFIG.md` and
`docs/CODE_MAP.md`. Inherited `?? .opencode/` was never inspected or touched.
No progress/GOAL/task-spec/acceptance/status/staging/commit/push changes were made.

## Risks

The pre-change risks were the first-unknown app-wide abort, raw error identity/path,
and a module set without per-request activation state that included the ignored marker.
Those R1 risks are resolved by the tested boundary. Native DCP policy remains mandatory;
provider readiness and general fatal classification remain separate, without catches.
No unresolved R1 scope bug or real external blocker was observed. An optional compiled
initializer was not invented merely to simulate setup failure. Provider cold failures
and general fatal safe-source diagnostics are not qualified by this R1 report.

## Next

Parent reviews the dirty R1 diff/artifacts and owns staging/delivery and status updates,
then resumes R2 provider cold availability and general R3 fatal source diagnostics.
Temporary mutation/Cargo/native-fixture ownership is released on return. This report
does not close T51, T44 visual gates, or GOAL A01–A13.
