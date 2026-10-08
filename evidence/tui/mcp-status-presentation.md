# T44 — typed MCP modal status presentation

## Result and scope

The existing MCP snapshot/list owner now presents the frozen VIS40 status contract:
`Connecting …` for pending state or any pending action (including disconnect),
`Connected ✓`, `Disabled ○`, `Failed !` and `Sign in required →`. Native unsupported
OAuth remains honest in the existing details/actions; no fictitious sign-in is
enabled by changing the label.

`app/mcp.rs::mcp_list_servers` lends the current bounded typed inventory only to
the MCP list, not its details or other panels. `dialog.rs` resolves status by
opaque option/server ID: unselected status uses the dialog success/error/warning/
muted token; selected foreground/background keep the existing action override.
Connected's intrinsic bold is independent of foreground and selection. No status
is inferred from label prose, and unrelated Select footers keep their old style.
Actions, binding/generation checks, search caret, titles and geometry are unchanged.
There is no new store, registry, polling, public DTO or production dependency.

Pinned source is `opencode/packages/tui/src/component/{dialog-mcp,dialog-select}.tsx`,
commit2670273ff17da96f85c5826ced57aa1b368754fa; frozen amendment1259–1390 requires
Connected bold. The **running original loses that bold** on selected and unselected
rows, despite its source declaration. Native preserves the frozen source contract;
this is disclosed as a genuine comparison difference, not pixel equality. Selected
status foreground already matched the original and was not incorrectly recolored.

This is a functional slice, **not whole VIS40/T44/R6/V09 PASS**. Safe configured
label projection (MCP08), full modal controls/details and all other frozen outcomes
remain independent mandatory work.

## Current-source association

Code/evidence `2a42652006857ac244d1cd32a7dc83a01c722ff2` is PUSHED by ordinary
fast-forward to verified `origin/agent/oc-rust-port`; checkpoint0091 remains
immutable. No whole-task finish accompanies delivery. Final mixed005, ordinary022
and temporal025 share:

- Captured base `aff2d8f97702fb00b3c6d20b00f1d73e593bf879`;
  tree `5b772271a04fb01168c88d141816f83ff32a84f3`.
- Dirty diff `2a66b011d3de54d05213cbe8b801cfe2bf70180413afe054e2aa04d7dacd0303`.
- Source manifest `ba15479f3f66a5993d914e43f4e9d7506161a9ea3a8bf8d8ed572bb18f6d40f3`.
- Actual `cargo build --locked`; native ELF SHA-256
  `0fbcb007f64b5c4c58171ba618e2c64b83d4ed0deec101f55b73ab7a208190f9`.
- Original executable SHA-256
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

Locks retain actual commands, inputs, provider/MCP protocol, profile and source
association. No Rust or capture-runtime changes follow the final captures;
subsequent changes are documentation/progress only.

## Actual mixed MCP inventory

`mcp-status-attempt-005` runs isolated original/native PTYs with one healthy real
stdio peer, one disabled configured peer (never started) and one genuinely failed
initialize. The original fixture uses its canonical `disabled:true`; native uses
its admitted `enabled:false`. No UI state, SQL row or tool result is seeded.

The probe preserves Unicode draft `preserve MCP Ω界`, opens the actual Home MCP
item by SGR mouse input, selects all three rows at120×40,80×24 and160×48, then
closes at120×40. Eleven full paired stages retain cells, PNG, VT and cursor.
Both sides report `PASS_BEHAVIOR_ONLY`, semantic tones true, exact composer cursor/
draft restoration, zero provider requests/tool effects/MCP tool calls/Shell counters,
and actual healthy/failed initialize receipts. Native intrinsic Connected bold is
true; original is false. Default dark unselected green/red/muted and selected
action foreground/background are asserted from actual styled cells, not text.

All22 strict whole-grid/PNG comparisons are **DIFFERENT**; runner exit1 is diagnostic,
not a green parity gate. Opaque native names, intrinsic-bold discrepancy, safe
service feedback and truthful native version remain fully unmasked.

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true --mcp-status true \
  --output /home/opencode/ai/oc/evidence/tui/MCP-STATUS-NEW-IMMUTABLE-ATTEMPT
```

Earlier001 used the wrong original disabled field and records a real additional
connected row, not a product failure. Corrected002 is the pre-fix unselected-tone
RED.003/004 qualified progressively one/three geometries before the final source
association. These attempts remain immutable; no comparator or donor was patched.

## Existing body/cursor regressions

`tool-preview-attempt-022` retains all27 paired ordinary stages and two native-only
resource frames. Both sides behavior-qualify: genuine Home failure/retry, zero
Home model requests, bounded MCP/error/Shell expansion, literal markers, Unicode
draft/caret, read-only `/cards` paging, resize, `/new`, reopen and clean same-root
restart. Six total requests, four tool effects, three real MCP calls, one17-byte
Shell counter line, artifact hashes and native read-only operations/resources/
four presentation facts remain unchanged through the viewing/replay sequence.
Full comparisons:4 EQUAL/50 DIFFERENT +4 native-only resource entries, exit1.

`cursor-temporal-attempt-025`,120×40 expanded MCP/WebGL/blink/supported sync, has
six actual composer/Search states: both `QUALIFIED_CURSOR_BEHAVIOR_ONLY`.
Native cycles3/3/3/4/4/4; original3/4/4/4/4/4. Every state meets the unchanged
same-owner idle cadence and sampling bound, final caret/draft/restoration and zero
visible unsynchronized phantom commands; real effects/durable facts show no replay.
Native hover/Search-hover parse2381/1302 commands; original1647/25. Continuous
original Search repaint is not inferred from its mouse input alone.

The audit of144 full opaque actual PNGs/88,473,600 pixels and fixed-owner RGBA
passes in `cursor-temporal-matched-006/report.json`.67 actual-phase matched pairs
yield134 strict full comparisons, ALL DIFFERENT; runner150 DIFFERENT/20 unmatched
phase entries are not inferred equality. Prior geometry/fallback/steady controls
remain in `cursor-frame-presentation.md`, not newly claimed whole VIS31 PASS.

Temporal024 honestly fails native sampling/cadence: one540.4-ms RAF gap exceeds
the unchanged quarter-idle-period bound, distorting the apparent hover period.
Its final caret/no-phantom facts do not qualify it. The same-source additional025
measurement passes without changing production, observer or acceptance criteria.
Failed024 and the refused matched005 audit remain recorded, not external blockers.

## Gates actually executed

Serial jobs3/test threads2/approved cache TMPDIR, current source:

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

ALL PASS; `.local/t44-mcp-status-gates-final-20261008.log` records46 completed
results,1734 passed/0 failed/11 unchanged opt-in ignores, TUI462 (26.25s), release
2m07s. Actual startup covers terminal restoration, retained-owner safe refusal/
retry and corrupt-headless no effects; discovery unauthorized/forbidden/oversized/
slow/absent/present has expected GET/no Responses/exit/restoration.
The first chain exposed the obsolete actual-binary expectation `Disconnecting`;
only that expectation was changed to the frozen common pending label `Connecting`.
All held lease/counter/config/secret/restart assertions remain. Targeted lifecycle
1/0, typed TUI matrix1/0, Node syntax, Python repository47/0 and source checks PASS.
Documentation/progress structure and source diff checks PASS with delivery.
No test/threshold is disabled.

Captured padded `.txt` and raw `.vt` remain immutable data: only their exact final
capture paths are excluded from Git whitespace checking, never visual comparison.
AUTH06 remains deferred; original24/24 and Go13/24 ledgers are unchanged. Whole
frozen R1–R6/VIS01–VIS45 and final current-source R6/V09 remain open.
