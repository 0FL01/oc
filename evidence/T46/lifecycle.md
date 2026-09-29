# T46 R5/MCP08 + R7/MCP10 — native lifecycle qualification

## Coordinator review and final qualification

Base remains `4d39bbfc217b5aab3f433993794a79262d7c42a6`; the coordinator
reviewed all changed production paths, new owner/scenario modules and existing
assertion migrations. Two concrete findings were resolved before delivery:

- A healthy retry of a Unicode or overlong configured server retained its old
  warning: the stored safe identity and raw clear key differed. The focused
  regression failed with exit 101. Warning bookkeeping now uses one bounded,
  idempotent safe identity, with a digest suffix only when truncation/replacement
  would alias distinct names. Short admitted identities remain byte-identical;
  raw dispatch/control identities, permissions and wire names are unchanged.
  The regression also proves two affected neighbors do not hide/clear each other.
- The first coordinator workspace run failed the existing stdio unsupported
  after-effect case. An isolated unchanged run passed, but a controlled 200-ms
  fixture catalog delay reproduced exit 101 and identified the first failed
  operation as **`glob`**, not the MCP effect. Three remaining legacy stdio
  scenarios assumed first-prompt readiness; they now explicitly observe the
  actual Connected snapshot before their intended effect/retry. The deliberate
  catalog delay remains in the unverified-result fixture. All original unknown/
  failed/completed, effect-count, reuse/reap, redaction and timeout assertions
  remain, with no product startup barrier or increased deadlines. Four stdio
  cases passed together after the correction (14.47 s).

Final current-source serial gate: workspace fmt and all-target Clippy
`-D warnings`, locked workspace tests **1232 passed / 0 failed / 9 unchanged
opt-in ignores**, locked debug/release builds and both help commands, Python
5/15/14 checks, docs/progress and diff all exited 0. Full coordinator log:
`/home/opencode/.local/share/opencode/tool-output/tool_0ea719465001t5cXUAyJFSmfPv`.
Actual immutable-snapshot/activation Core tests also passed 4/4; real binary
lifecycle 3/3 and MCP09 4/4 passed independently before the final build.

Final coordinator-built native SHA256:

- debug: `a22d5bfde71876f0fd0f4064d9b8e5fb3db1037c52b3aba8b3c8dfccc83d1a0e`
- release: `45972d228967ee22b27d8b18ae70e549d9c1c23ff3ca9c66e5de0e662b255ac6`

After those builds, the coordinator directly ran the existing
`target/debug/deps/mcp_application-c554f016ee604ae9` (single test thread, owned
TMPDIR): `lifecycle::` 3/3 (4.06 s), `mcp09_` 4/4 (2.04 s), and `v07b_stdio_`
4/4 (14.24 s), all exit 0. Both binary digests remained unchanged. Thus these
functional cases additionally qualify the final built debug artifact, not just
the preceding Cargo test-profile CLI. Checkpoint `progress/M8/T46/0002.md`
retains R4/media/prompts/resources as the next required work.

The atomic-owner command/digest table below is retained as its earlier proof,
not relabelled as the coordinator's final artifact. The additional identity
regression raises the atomic scenario total from 11 to 12. R4 live,
media/prompts/resources, T44 paired presentation and whole-goal READY remain open.

## Result

**PASS for the frozen atomic R5/R7 slice.** The existing MCP resource owner now
starts enabled connections independently before application ready, publishes
bounded typed current-Location/config-generation facts, and executes real
connect/disconnect/retry controls. Initialize **and** catalog success precede
connected. Current requests lease an immutable catalog/client view; completed
connections/removals become available at the next safe provider-request boundary.

Qualification base: `4d39bbfc217b5aab3f433993794a79262d7c42a6` plus the reviewed
working-tree source/test changes named below. HEAD stayed unchanged. No commit,
staging, progress/task status, GOAL or ACCEPTANCE mutation was performed in this
atomic slice. This is not full T46/live/READY or a paired T44 visual qualification.

Pinned donor verified at `2670273ff17da96f85c5826ced57aa1b368754fa`:

- `opencode/packages/core/src/mcp/index.ts:355–447,487–513,583–638`:
  independent initial forks, initialize/catalog before connected, per-server
  controls, scope-owned cleanup and ready tools without an all-startup barrier.
- `opencode/packages/tui/src/component/dialog-mcp.tsx:37–175`:
  owner-derived statuses and actual controls. Native OAuth remains unsupported.
- R6 authority remains the shared config/composition normalization, substitution,
  pinned no-follow reader, resource and credential-domain admission pipeline.

### Implementation/ownership

- `crates/oc-adapters/src/runtime/mcp.rs` retains dispatch/redaction/unknown-effect
  leases; `runtime/mcp/lifecycle.rs` supervises the same generation-owned clients,
  registry, startup/relist/close jobs and bounded controls. Immutable views lease
  those clients, not a second client registry. Clean shutdown joins owned work;
  abort/join/timeout/cleanup failures never become confirmed cleanup success.
- `runtime.rs`, `runtime/turn.rs` and `application.rs` expose short snapshot reads
  through the existing CoreApp inbox/event bus. Primary requests adopt ready views
  only before building the next request. Children retain their exact parent view.
  `Runtime<'a>` still borrows the application's single `Db`; no Arc database,
  duplicate storage owner/counter/schema, daemon or generic worker framework.
- `composition.rs` freezes admitted source bytes/provenance/root descriptors for
  activation; `config.rs::activate_mcp_entry` is shared by startup and disabled
  activation. Explicit controls do not bypass inert templates or source authority.
- `oc-core/src/{queries.rs,core_app.rs}` supplies typed binding/status/action facts
  and safe diagnostic enums through the existing command/event interface.
  `oc-tui/src/app/mcp.rs` and `oc/src/tui_cmd.rs` consume them in real `/mcps`, Space
  controls and Enter details, including before the first prompt. No periodic UI
  polling, free-text/name-derived state, endpoint/env/header/error-body details or
  fictional working sign-in. An unavailable initial owner query is fatal.
- `docs/CODE_MAP.md` records these seams and nearest test packs. Substantial new
  scenarios are separate owner/test modules; public paths remain intact.

## Checks

All Cargo commands were serialized with `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`,
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
and tool timeout 900000 ms. Non-root uid 1003; the heavy-check resource sample
showed 7571 MiB available RAM and 176 GiB available disk. Only offline isolated
loopback/stdio peers and synthetic canaries were used; no paid generation, real
npx/browser, runner-auth/config extraction or dependency/toolchain upgrade.

### Obligation proof

| Obligation | Result | Actual observation/assertion |
| --- | --- | --- |
| MCP10 independent startup before prompt | PASS | Real PTY first prompt with healthy, permanently held initialize, failed unsupported entry, disabled local and an actual HTTP 401 neighbor. Before user/title generation: healthy initialize/list, slow initialize=1, model requests=0. Healthy `tools/call`=1 completes while slow `tools/list`=0. Failed/slow/disabled/auth tools are absent from that request. Held transport is closed on quit. |
| MCP08 honest bounded status | PASS | Real `/mcps` shows Connected / Connecting / Failed / Disabled and typed sign-in-required unsupported. The server named `pretend_connected` returns 401 and never acquires a connected label. Configured-enabled and actual state are distinct; manually connected disabled entry still has configured-enabled=false. Safe enums/source/field/action carry failures. |
| Disabled manual activation admission | PASS | Missing `{file:program}`/token fails action with zero forks. After creating admitted files, initialize/catalog run once; cwd/activation canary booleans=true. Core tests replace a source directory yet use its pinned descriptor; a symlink in the pinned reader is refused without another fork. Lower-trust project overlay of an orphan product credential is refused while the healthy sibling stays connected. |
| Coalescing before activation/effects | PASS | With catalog held, mutate the activation program file to an invalid value, then issue two concurrent duplicate Core connects. Both acknowledge the existing pending attempt; total fork/initialize/list=1, not a fresh file read/launch. Repeated PTY Space and resize also leave totals at one. |
| Actual retry/new admitted connection | PASS | Actual missing executable yields Failed/SpawnFailed; repairing it alone does not retry. Space Retry creates exactly one initialize/catalog attempt, pending until the catalog barrier releases, then Connected. It is not a retry of an uncertain tool effect. |
| Safe current/next catalogs and guidance | PASS | Held pre-catalog request has neither dormant tool nor initialize guidance. Catalog completion does not mutate its recorded wire body; next request has both. Disconnect while a current request leases the client stays Disconnecting with closed=0; release yields actual close/reap, Disabled, and removal of tools/guidance from the next request. |
| Real responsive status/resize/cancel | PASS | `/mcps`/resize work during held initialize/catalog, held provider response and actual held remote `tools/call`. Closing the modal does not cancel that call; subsequent raw Esc sends the matching request-specific cancellation and durably records unknown. Overlapping retry reaches neither provider nor remote effect. |
| Reload/Location/stale completion | PASS | Core and actual PTY reload/Location retirement join/reap old pending children before replacement. Release an old initialize barrier afterwards: old catalog=0 and fork total remains 1. Old bindings/actions are rejected; new binding is distinct; old project inventory disappears. Controls themselves generate zero model requests. |
| Reopen/restart and config immutability | PASS | Reopen reads current Disabled after disconnect. New process rebuilds configured Disabled, no extra fork and no persisted connected/runtime-toggle label. Product leaves source bytes unchanged; deliberate fixture edits are not undone/replaced by controls. |
| MCP07 relist failure | PASS | Failed relist retains Connected/tools=1 and old catalog with safe transport diagnostic. Restored dirty flag permits the next turn's new catalog; third turn does not relist again. Total fork=1/list=3; private error sentinel never reaches provider projection. Existing AUD23 old→new→old notification assertions stay green. |
| C1 fatal caps/cancel/cleanup/quarantine | PASS | Seven actual peers ×20 tools exceed generation cap 128 while below per-server/MAX8 bounds: provider hits/history=0, all seven PIDs gone, turn/shutdown non-success. Failed/panicked supervisor join remains fatal on repeated stop after consuming its handle. Cleanup failures during retirement wake failure monitors. Existing AUD12/MCP05/AUD23 and remote unknown/no-replay cases remain green with unchanged cleanup/cancellation budgets and caps. |
| R6/MCP09 regression | PASS | All 17 R6 scenarios are included in the green workspace. Current build directly reruns four actual-binary cases: argv/cwd/inheritance/overlay/PATH/domain effects, disabled chrome zero npx/browser effects, failed inventory plus healthy sibling, and withheld credential PATH cannot re-enter resolution. Source bytes/read-only and canary redaction assertions hold. |
| Full T44 VIS19/VIS40/VIS42 paired presentation | NOT_RUN | Minimal real consumer/effects proved here; parent owns paired geometry/theme/focus/cursor/state-transition qualification. |
| R4 live; media/prompts/resource parity | NOT_RUN | Mandatory owner-live and remaining MCP parity work stays open for parent. |

New tests: **11 scenarios**: four Core/backend controls, three actual-binary
lifecycles, two runtime relist/cap risks, one fatal initial-query boundary and one
sticky supervisor-join failure scenario. The existing held-call case additionally
proves real status/resize while RPC is held, with its unknown/quarantine assertions
preserved. Source fixtures: `fixtures/mcp10-lifecycle.py` (bounded counters,
append-only total effects, known canary booleans; no env dump).

### Commands and exits

Cache prefix below means
`/home/opencode/.cache/opencode-tmp/opencode/T46-lifecycle-`.

| Command | Exit/result | Cache log |
| --- | --- | --- |
| `cargo test -p oc --test mcp_application --locked mcp10_initial_connections` before implementation | 101 / RED 0 passed, 1 failed: healthy list absent before any prompt | `initial-red.log` |
| `cargo test -p oc-adapters --lib --locked application::mcp_tests` | 0 / 4 passed | `core-controls.log` |
| `cargo test -p oc-adapters --test runtime --locked` | 0 / 93 passed | `runtime.log` |
| `cargo test -p oc --test mcp_application --locked lifecycle::` final targeted | 0 / 3 passed | `current-binary.log` |
| `cargo test -p oc --test mcp_application --locked v07b_remote_inflight_raw_esc_keeps_unknown_and_refuses_overlapping_retry -- --exact` | 0 / 1 passed | `held-tool-status.log` |
| `cargo test -p oc --test golden_binary --locked` | 0 / 1 passed | `golden.log` |
| `cargo test -p oc --test live_bounded --locked` (offline tests only) | 0 / 9 passed, 2 existing opt-in ignores | `bounded-offline.log` |
| `cargo test -p oc --test pty_t42 --locked aud38_location_switch_is_one_lifecycle -- --exact` | 0 / 1 passed | `aud38.log` |
| `cargo test -p oc-adapters --test soak --locked` | 0 / 4 passed | `soak.log` |
| `cargo test -p oc-tui --lib --locked commands::tests::completes_prefix -- --exact` | 0 / 1 passed | `command.log` |
| `cargo fmt --all -- --check` | 0 / PASS | terminal gate |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 / PASS | `clippy.log` |
| `cargo test --workspace --locked` final source | 0 / **1231 passed, 0 failed, 9 existing opt-in ignores**, including doc-tests | `workspace.log` |
| `cargo build --workspace --locked` + `target/debug/oc --help` | 0 / PASS | `debug-build.log`, `debug-help.log` |
| `cargo build --workspace --release --locked` + `target/release/oc --help` | 0 / PASS | `release-build.log`, `release-help.log` |
| Direct existing native test executable, `lifecycle::` after final build | 0 / 3 passed, 4.32 s | `built-binary.log` |
| Same executable, `mcp09_` after final build | 0 / 4 passed, 2.10 s | `built-r6.log` |
| Same executable, held remote `tools/call` status/cancel case `--exact` | 0 / 1 passed, 10.74 s | `built-held-tool.log` |
| `git diff --check` | 0 / PASS | terminal gate |

Final affected totals in the workspace: native MCP application 35/0; native T39
PTY 39/0; native T42 PTY 34/0; adapter lib 321/0; runtime 93/0; remote 25/0 with one
existing opt-in ignore; stdio 16/0 with one existing opt-in ignore; soak 4/0;
core 26/0; TUI 417/0. No ignore increase, thresholds/caps/deadlines/GOAL change or
disabled assertion was used to obtain green.

### Current binary/source association

Final built artifacts from the unchanged base + reviewed source above:

- `target/debug/oc`: SHA256
  `d860c43687c0f25e4d237b100e7f89e5dc48012f2feb47558c293da82eed3eee`.
- `target/release/oc`: SHA256
  `334cff43c038bbb8105e33e58322c3b5e7d3b9da706370752ba3c379f06329e0`.

Both help commands passed. After the final workspace build, executed existing
`target/debug/deps/mcp_application-c554f016ee604ae9` directly (RUST_TEST_THREADS=1,
owned TMPDIR), avoiding Cargo replacing the built CLI with its test-profile
artifact. Its fixture uses `CARGO_BIN_EXE_oc` = `target/debug/oc`. The three
lifecycle, four MCP09 and held-call status/cancel cases therefore exercised the
debug artifact above; both hashes matched before and after those executions.
Release qualification here is build/help; the counters above are debug-binary
functional proof.

## Risks / intentional differences

- Native no-OAuth remains honest: actual unauthorized becomes typed auth-required
  with unsupported sign-in action/details. No OAuth discovery/flow is promised.
  R6 direct Code Mode difference and UnsupportedProtocol auto/modern paths remain.
- Control acknowledgement means admitted/coalesced action, not eventual network
  success. Pending work and immutable request leases can delay a real disconnect;
  Disabled is published only after confirmed close. Cleanup uncertainty is fatal.
- R7 intentionally supersedes lazy first-turn fixture assumptions. Existing local/
  remote pending-admission tests now hold a notified catalog refresh after actual
  initialization; original no-acceptance/history/effect, duplicate-submit/editor,
  Esc, exact cancellation/reap budgets and quarantine assertions remain. Scripted
  golden/dry-run models request only advertised MCP tools, using a permitted read
  round if startup is still pending. AUD38/soak synchronize on actual typed
  connected/failed facts; the former unsupported `/mcps` assertion now requires
  its genuine approved command. These are explicit contract migrations, not a
  new product startup barrier or weakened acceptance.
- Initial red and intermediate failures are preserved in bounded cache logs
  (`binary-diagnosis*.log`, `workspace-*-fixture-failure.log`), then diagnosed as
  lazy-fixture assumptions or corrected implementation/query/latch issues. Final
  full workspace is green. Largest owned lifecycle raw log is 111228 bytes, well
  below 16 MiB per-log; no paired/raw campaign was added to repository evidence.

## Next

Parent reviews and delivers this working-tree atomic slice, then continues
mandatory R4 owner-live, MCP media/prompts/resources and T44 paired presentation /
full V09 plus GOAL A01–A13. T45/T50/T51 remain independent continuation work. This
report does not finish T46, remove T44 PAUSED or assert whole-goal READY.
