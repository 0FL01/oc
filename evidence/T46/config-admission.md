# T46 R6 / MCP09 — config admission and local launch

## Parent independent review and current requalification

Parent reviewed normalization, scope/cwd/credential authority, transport deadlines,
typed diagnostics and direct consumers. A concrete extra credential-domain risk
was reproduced: a credential-named product-env value not referenced by a global
provider could be withheld from inheritance, then reintroduced through a project
overlay. The existing domain regression was extended and failed (exit101); checking
the already-computed full `blocked_inherited_values` set before all explicit local
argv/cwd/env use repaired it. The regression and all17 MCP09 scenarios then passed.
Explicit local-only credentials remain admitted; shell minimal env is unchanged.

Current source includes that reviewed one-line repair and regression. Parent
fmt/strict workspace Clippy PASS; fresh full workspace **1220/0/9 unchanged opt-in
ignored**, locked workspace debug/release/help, Python5+15+14, docs/progress/diff
and advisory size checks PASS. Gate log:
`/home/opencode/.local/share/opencode/tool-output/tool_0e9949f5a001fOTO8YCedpS1Ki`.
Current debug `02d6c651eb5a1e9d37ca8655c7cb69af18d5358481c4b7d4f2f754cceeb9c269`;
release `6b11bf4492925db26b6ec5c37d9cadc45c37091055aa6b4e5e1ba7612ee8c415`.
The older artifact values below describe the agent's pre-review run, not this repair.

One parent gate initially hit unchanged AUD12 silent-stream SIGINT exit950ms
budget; test source was inspected, isolated unchanged replay passed0.52s and fresh
full workspace passed all Responses tests. No assertion/deadline/cap was changed;
the transient is recorded, not relabelled as a source fix or external blocker.
R5/R7, R4 live and media/prompts/resources remain open; no whole-T46/READY claim.

## Result

**R6/MCP09 offline qualification: PASS.** Captured 2026-09-28. Association:
base HEAD `0170891302ff184a114e663ff4af62d27f81ef53` plus the reviewed R6
worktree changes; delivery commit belongs to the parent coordinator.

The first source-derived tests reproduced the admission failure: a valid disabled
chrome entry with `environment` failed strict `McpEntry` deserialization, and the
actual binary did not reach its first PTY prompt. The normalized configuration
now loads, disabled chrome has zero launcher/browser effects, failed entries
have payload-free typed details before the first prompt, and healthy siblings
remain usable. Seventeen new `mcp09_` scenarios pass across config, composition,
stdio, remote, and actual-binary targets. Final workspace result is
**1220 passed / 0 failed / 9 existing opt-in ignored**.

### Pinned source and matrix proof

Donor: OpenCode v2.0.12, commit
`2670273ff17da96f85c5826ced57aa1b368754fa` (local donor HEAD verified).
Owners inspected: `packages/core/src/v1/config/mcp.ts`,
`v1/config/migrate.ts:202–225`, `config/normalize.ts:260–293,633–696,784–790`,
`config/plugin/mcp.ts:40–55`, `packages/schema/src/mcp.ts:7–64`,
`packages/core/src/mcp/client.ts:195–207`, and `mcp/stdio.ts:17–31,80–89`.
`fixtures/mcp09-normalization.json` records this source commit and the relevant
owners; `fixtures/mcp09-stdio.py` reports known-canary booleans and effect counters.

| R6 input | Observed normalization / effects |
| --- | --- |
| Legacy / canonical domains | One server map; canonical wins in the same document; a later same-name entry wholly replaces its predecessor. Global timeout leaves merge separately with preserved provenance. Legacy names `servers` / `timeout` follow the pinned direct-type disambiguation. Source bytes remain unchanged. |
| Local argv / remote URL / headers | Exact argv including `"two words"` and an empty argument; no shell splitting. Remote URL retains the exact configured path and existing URL policy. Header typing and case conflicts fail before I/O; healthy strict `codex_web` still initializes, catalogs, and searches. |
| Activation / malformed entries | Legacy `enabled` inversion and canonical `disabled` defaults are source-derived. Disabled entries skip substitution/credentials/resources/launch, but malformed disabled entries retain typed failed inventory. An 18-case recognized-field/capability table keeps valid siblings. Inventory is bounded; native policy and runtime catalog/server caps remain fatal. |
| cwd | Omitted uses Location; relative resolves from Location rather than the global config directory; existing absolute canonical cwd is admitted. Missing/non-directory cwd has typed admission details. Escape/symlink escape remains non-success without existing explicit `external_directory` authority. |
| environment / PATH | Admitted local launch inherits the product-process snapshot plus string-map overlay. Actual children confirm inherited and overlaid canaries and configured PATH resolution. Project-source commands withhold higher-domain credentials and benign aliases of those values; whole-entry replacement cannot erase source authority. Nested Location under the global directory does not acquire global authority. A withheld-PATH regression proves executable lookup does not reread process PATH. |
| Timeouts | Legacy numeric milliseconds affect only catalog/execution. Integer-valued JSON `1000`, `1000.0`, and `1e3` normalize identically, following the pinned JS Number semantics; fractional/non-positive/out-of-native-range inputs remain invalid. Independent global/per-server stage overlays use donor defaults 30000/30000/43200000 ms. Real stdio and loopback HTTP stage stalls observe distinct startup/catalog/execution deadlines. A 40 ms legacy operation timeout permits a 200 ms initialize through the independent startup budget. Existing cancel/cleanup budgets and caps remain in force. |
| Code Mode | Omitted / false are native direct tools (D04); true produces per-server `UnsupportedCapability`. |
| OAuth | Omitted / false use native no-OAuth behavior. True compatibility input and all approved legacy/canonical object fields produce `UnsupportedCapability`; malformed known object fields have precise typed field paths. |
| Protocol | Omitted / legacy keep native initialize negotiation up to 2025-11-25; strict `codex_web` remains exactly 2025-11-25. Auto / 2026-07-28 produce `UnsupportedProtocol`. The bounded pinned-rmcp spike and rejected-modern-response cleanup are described below. |
| Excess fields / substitutions | Non-security metadata is omitted. Recognized malformed/security/native-admission controls are explicit failures. Enabled argv/cwd/env reuse admitted substitution; `{file:}` stays relative-only/no-follow. Trust/file-boundary failures remain non-success. |

### Actual-binary observations

- Disabled chrome reaches the first `Untitled session` PTY view with
  `npm_config_offline="true"`, numeric timeout, and inert missing credential/cwd
  templates. Provider requests = 0; fake npx/chrome/chromium trap effects = 0
  before and after quit; config bytes unchanged.
- Three actual local launch cases cover global default cwd, global relative cwd,
  and project absolute admitted cwd. Each reports argv/cwd/inherited/overlay/PATH
  checks = true and spawn/initialize/catalog/call = 1/1/1/1. Higher-domain
  credential/alias presence follows source authority. Owned child PID is gone
  after shutdown. Requests, messages, tool operations, events, stdout, and stderr
  contain none of the known credential/configured-env canaries.
- Malformed disabled / OAuth object / Code Mode / auto-protocol entries have
  safe server/source/field/stage/code/action details before prompt. Failed
  launcher effects = 0, failed tools absent; healthy `codex_web` initialize,
  catalog, and search each = 1; the turn succeeds and config bytes are unchanged.
- Product PATH carrying a higher credential-domain value is withheld from a
  project command. Bare-executable lookup has trap effects = 0 and reports
  `spawn_failed`; the application still answers. The launcher uses the static
  POSIX default when admitted PATH is absent, rather than rereading host PATH.

### Capability spike / intentional differences

Pinned dependency remains `rmcp 3.4.0`. A bounded in-memory SDK spike proves
`Initialize` sends 2025-11-25, while `Discover` and `Auto` use `server/discover`
with 2026-07-28 metadata and return a typed discovery result. The SDK therefore
knows the modern protocol. The native transport owners still use the initialize
lifecycle; a qualified modern HTTP/per-request-metadata adapter path is pending.
Auto/modern config is explicitly unsupported. A stdio modern initialize response
is rejected with `ProtocolMismatch`, and its owned process is reaped; cleanup
failure remains fatal. No protocol rewrite, OAuth discovery, Code Mode interpreter,
dependency/toolchain upgrade, or alternate client registry was introduced.

## Checks

One Cargo coordinator; all Cargo commands used `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`, and
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Raw logs are in `/home/opencode/.cache/opencode-tmp/opencode/`; the table uses
their basenames. Every raw log is below 16 MiB (largest 110161 bytes). Repository
additions are source fixtures and this compact report, without a raw/pixel campaign.

| Command | Exit / result | Cache log |
| --- | --- | --- |
| Initial `cargo test -p oc-adapters --lib --locked mcp09_` | 101; expected RED, 0/1: unknown `environment` | `T46-r6-config-red.log` |
| Initial `cargo test -p oc --test mcp_application --locked mcp09_` | 101; expected RED, 0/1: first prompt blocked by config | `T46-r6-binary-red.log` |
| Final-review `cargo test -p oc-adapters --lib --locked mcp09_positive_integer_json_spellings_follow_pinned_number_semantics` | 101; expected RED, 0/1: donor-valid integer-valued decimal rejected | `T46-r6-number-red.log` |
| `cargo test -p oc-adapters --lib --test mcp_stdio --test mcp_remote --locked mcp09_` | 0; 8 lib + 4 stdio + 1 remote passed | `T46-r6-admission.log` |
| `cargo test -p oc --test mcp_application --locked mcp09_` | 0; 4 passed | `T46-r6-binary.log` |
| `cargo test -p oc-adapters --test mcp_stdio --test mcp_remote --test runtime --locked` | 0; stdio 16/0/1 ignored, remote 25/0/1 ignored, runtime 91/0 | `T46-r6-affected.log` |
| `cargo fmt --all -- --check` | 0 | direct command |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | `T46-r6-clippy.log` |
| Final `cargo test --workspace --locked` | 0; **1220/0/9 existing ignored**, including actual-binary 32/0 and T39 PTY 39/0 | `T46-r6-workspace.log` |
| `cargo build --locked` and `target/debug/oc --help` | 0 / 0 | `T46-r6-debug-build.log`, `T46-r6-debug-help.log` |
| `cargo build --release --locked` and `target/release/oc --help` | 0 / 0 | `T46-r6-release-build.log`, `T46-r6-release-help.log` |
| `git diff --check` | 0 | direct command |

Current native artifacts, associated with the base HEAD and R6 dirty worktree:

- Debug SHA256: `d9ae52cf842a31c283b27dc009f2ed443ad509cd8c44143a65cb89719d457359`.
- Release SHA256: `92289190ed9e30c6373ff77930abf59a1dbb02e095725adebe59a8ee8adb99c8`.

Existing tests changed deliberately for the amended local-MCP inheritance
contract and typed missing-credential diagnostics. Tests/caps/deadlines/baselines
were not weakened, and the nine existing opt-in ignores were preserved. The
workspace run covers existing CFG02/CFG04, AUD22/AUD23, MCP04/MCP05, ordinary-shell
minimal-env, cancellation, quarantine, reaping, storage, and resource regressions.

## Risks

- One preceding workspace run failed the unchanged T39 S07 active cursor equality
  assertion while active view and requests agreed: small `(33,5)` vs large
  `(0,15)`. The test stabilizes the view and takes a separate cursor snapshot;
  this suggests a capture race, but root cause is not established. The unchanged
  isolated exact test passed (exit 0), and the final full workspace rerun passed
  all 39 T39 scenarios. Failure retained in
  `T46-r6-workspace-cursor-failure.log`; isolated diagnosis in
  `T46-r6-s07-diagnosis.log`. No assertion or baseline was changed.
- Credential-domain admission uses conservative known-value matching across
  already-admitted sources and product env; it never reads runner-auth/config.
- Runtime MCP attach is still lazy/sequential until R5/R7. This evidence qualifies
  R6 admission and effects; async startup/status/control, paired visuals, whole
  T46/live qualification, and product READY require their remaining gates.

## Next

1. Parent reviews/delivers the R6 implementation and this evidence. Inherited
   scheduling/progress work and the immutable T44 leaf were preserved. No
   staging/commit/push/task-status/GOAL/ACCEPTANCE mutations were performed.
2. Continue R5/MCP08 and R7/MCP10 through the existing MCP owner: genuine typed
   lifecycle/control, independent pre-prompt startup, safe catalog publication,
   late-generation/cancel/shutdown effects; then paired T44 VIS19/VIS40 checks.
3. Continue required T46 R4 owner-live and media/prompts/resource parity, independent
   T51/T45/T50 work, and the parent's full T44/V09 plus GOAL A01–A13 completion.
   This atomic evidence does not close those outcomes.
