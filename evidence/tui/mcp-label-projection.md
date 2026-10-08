# T44 — safe configured MCP label projection (MCP08 / VIS40)

## Result and boundaries

The existing MCP owner publishes the exact effective configured map key through
`McpServerSnapshot.name` when it is presentation-safe. List/search/display sorting
and details use that label, not handshake `serverInfo.name`. Unicode and long keys
remain exact under existing source/snapshot/render bounds; no ASCII allowlist,
titlecasing or borrowed 256-byte cap is added. Unsafe labels use the existing
`server-<SHA256-prefix>` fallback before publication.

Opaque control ID, Location/generation/instance binding, diagnostic identity,
tool/permission names and provenance do not change. Controls still resolve by ID.
This is the approved existing-owner MCP08 prerequisite inside resumed T44, not a
new task, T46 status reset or whole VIS40/R6/V09/T44 PASS.

Initial protection observes existing redaction inputs, the effective stdio launch
environment and recognized sensitive fields in cached admitted sources, including
disabled/failed/unsupported definitions and inactive model/variant overlays. It
rejects controls/ANSI/bidi control channels, endpoint/path/template channels and
known protected substrings. Legacy provider `api` and model `provider.api` are
included; URL userinfo is protected in encoded and decoded form. Bearer values
retain the transport's whitespace/scheme semantics. Mixed env/file templates
protect each already-known env input without reading the inactive file.

`composition::McpActivation` reports ephemeral protected values on success,
resource-admission refusal, entry failure and partial substitution failure. Each
successfully read rooted credential value is observed before subsequent field
substitution can fail. The existing scope masks **all** current nodes before a
later publication, and never restores a masked name in that instance. Previously
issued snapshots are not retroactively erased; no growing secret cache is kept.

Label-only `source_protected_values` remains separate from the original
`source_credential_values` launch-authority inputs. The shared stdio environment
derivation is a verbatim extraction of the prior launch policy. No inactive file
read, extra spawn/network, weaker rooted/no-follow/cap check, public DTO/store,
registry/poll/framework or production dependency is introduced. Model/tool
redaction in `runtime/mcp.rs` is unchanged.

The existing TUI service-feedback owner now joins MCP diagnostic provenance to
the opaque row ID, never the mutable display name. Its existing bounded pending
cause vector preserves the same failure through pending/reload and clears on
observed recovery/Location. Labels cannot cause repeated service alerts.

## Current-source association

Final mixed008, ordinary024 and temporal027 share verified captured inputs:

- Base `779b382a9bf8942e2d678da2ee3d9512ad420cb2`,
  tree `759f00a55f5aec99a3e4c1e1474fc17100b7c2e5`.
- Dirty diff `44a6d82917528c68c8f545ddeb8ef92d6f3fe85c559f1b42e62dd2b6dd4aea82`.
- Source manifest `ad6e41372a1ea40c1980d8ea08667057c61edbbe5cfdb28a185f34f75e21d863`.
- Actual `cargo build --locked`; native ELF SHA-256
  `5a0c15bf757e537c22e97ae27e497a540edcd49e92f7201ff28f2e55e0105f5f`.
- Pinned original `2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA-256
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

Locks retain actual builds, commands, isolated fixture configuration, real input,
protocol and full styled-cell/PNG/VT/cursor association. No Rust or capture-runtime
edit follows these final captures; subsequent changes are factual docs/progress.
Git delivery is recorded after the reviewed commit/push, not inferred from hashes.

## Nearest security and actual-binary checks

Nine current `mcp08_` adapter cases pass. They cover exact ordinary Unicode/long
projection, protected-key DTO/Debug fallback, inactive/failed/unsupported source
fields, inherited values under benign env names, encoded endpoint userinfo, mixed
env/file templates, successful activation and real post-read resource refusal,
partial first-file/second-file failure, sticky masking, unchanged IDs/binding and
disabled sibling zero-spawn. The benign disabled `npm_config_offline=true` case
proves label protection does not broaden local launch credential authority.

Existing diagnostic-identity/retry tests remain hashed. Actual MCP application41
cases retain held effects, leased cleanup, reload/Location, stale controls,
filter/status refresh, restart and raw-PTY canary withholding. The extended MCP09
failed inventory uses keys equal to malformed/OAuth recognized secrets: only
opaque fallbacks are searchable, no extra spawn occurs, config bytes and the
healthy real tool roundtrip remain unchanged. T42's actual Location lifecycle and
the real soak distinguish display label from diagnostic identity, not weaker
security assertions. VIS42 unit5/0 and actual PTY1/0 verify no repeated same-cause
alert through pending/reload while genuine recovery still re-arms feedback.

## Paired real MCP presentation

`mcp-status-attempt-008` uses actual isolated original/native PTYs with healthy
stdio, disabled zero-spawn and genuinely failed initialize. No UI/SQL row is
seeded. All selections at120×40,80×24 and160×48 show exact configured
`visdisabled`, `visfailed`, `vishealthy`; real typing filters `visfailed`, Enter
opens its details, Escape returns to the filtered list and then restores the
Unicode composer draft and exact cursor/shape.

Both sides report `PASS_BEHAVIOR_ONLY` over13 paired stages, zero provider/tool/
MCP-tool/Shell counter effects and actual healthy/failed initialize receipts.
Unselected status tones and selected action foreground remain correct. Native
retains frozen source-required intrinsic Connected bold; running original lacks
it. All26 strict full-grid/PNG comparisons are **DIFFERENT**, runner exit1;
no version, label, feedback, details, elapsed field or pixel is masked.

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true --mcp-status true \
  --output /home/opencode/ai/oc/evidence/tui/MCP-LABEL-NEW-IMMUTABLE-ATTEMPT
```

Ordinary `tool-preview-attempt-024` behavior-qualifies all27 paired stages and two
native resource frames: Home genuine failure/retry with zero Home model requests,
bounded literal body/guidance separation, MCP/error/Shell expansion, exact draft/
caret, read-only `/cards` paging, resize, new/reopen/same-root restart. Six provider
requests, four effects, three MCP calls, the single17-byte Shell counter effect,
artifact hashes and native RAW operations/resources/four presentation facts stay
unchanged through viewing/restart. Full results:4 EQUAL/50 DIFFERENT plus4 explicitly
native-only resource entries, exit1, not whole parity.

Temporal `cursor-temporal-attempt-027` uses120×40 expanded MCP/WebGL/blink/supported
sync. Six actual composer/Search states qualify unchanged sampling and same-owner
idle cadence: native cycles3/3/4/4/3/4, original3/3/4/4/4/4; no unsynchronized visible
phantom, exact owner/draft/caret restoration and no tool replay. Hover/Search-hover
parse2473/1342 native commands and1656/25 original commands; continuous original
Search repaint is not inferred from input alone. The actual fixed-owner RGBA
audit of144 full opaque PNGs/88,473,600 pixels passes in matched008.71 actual-phase
pairs yield142 strict full comparisons ALL DIFFERENT; runner158 DIFFERENT/4 unmatched
phase entries are not inferred equality. Prior fallback/steady/default/geometry
controls remain published, not newly claimed whole VIS31 PASS.

## Gates and experiments

Current serial jobs3/test threads2/approved cache TMPDIR chain passes:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo build --locked
cargo build
cargo build --locked --release
target/debug/oc --help
target/release/oc --help
python3 crates/oc/tests/support/startup.py target/release/oc
python3 crates/oc/tests/support/discovery_startup.py target/release/oc
```

`.local/t44-mcp-label-gates-final5-20261008.log`:46 completed results,1741 passed/
0 failed/11 unchanged opt-in ignores; adapters677 (101.05s), TUI462 (28.21s), release
3m17s. Release startup covers normal/error terminal restoration, retained-owner
refusal/retry and corrupt-headless no effects; discovery unauthorized/forbidden/
oversized/slow/absent/present has expected GET/no Responses/exit/restoration.
Node syntax, Python repository47/0 and docs/progress/source checks accompany delivery.

Earlier chains exposed the real mutable-name service join bug, then obsolete T42
display-hash and soak identity assumptions. Each was fixed at its owner with
preserved effect/resource/security assertions and targeted actual verification.
A later UI07 unsent-reload timing failure passed targeted then unchanged full-chain
retry; no threshold or test was disabled. Final review found omitted raw legacy
endpoint aliases: the extended source-protection test was RED before the minimal
collector fix, then all9 MCP08 cases and final5 full gates passed. Earlier mixed006/
007, ordinary023 and temporal026 remain immutable pre-final source qualification;
they are not substituted for final008/024/027.

Only exact final captures' padded `.txt`/raw `.vt` are exempt from Git whitespace
checking, not visual comparison, source/docs/JSON checks or immutable byte retention.
No secrets, .opencode edits, user-config writes or paid calls are part of delivery.
AUTH06 deferred; original24/24 and Go13/24 ledgers unchanged. Every remaining frozen
R1–R6/VIS01–VIS45 and final current-source R6/V09 remains mandatory/open.
