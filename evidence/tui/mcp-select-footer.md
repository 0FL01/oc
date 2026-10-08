# T44 — Select MCP footer focus and effective bindings

## Result and scope

The existing Select owner now paints and owns the MCP toggle action. Tab and
Shift+Tab cycle between the selected row and the available footer action; focused
submit invokes that action, not row details. Row hover/movement clears action
focus. The Search caret remains the real input owner. A shared footer rectangle
owns paint and mouse hits, including the narrow hint/action stack.

Action caption comes from the focused typed status, not loading prose. The hint
and dispatch use the same eight admitted `DialogShortcuts` values carried through
the existing config/catalog path. Remapped or disabled defaults cannot leak into
root palette, quit, agent/model cycling or composer handling. Sessions retain their
own registered Rename/Delete actions. Read-only MCP ErrorDetails stays separate.

Controls retain opaque server IDs and the captured current MCP binding. A mouse
press captures frame/action geometry and control; release revalidates all three
and must be unmodified. Drag, resize, filter or snapshot replacement invalidates
the press. No-match, pending and unavailable actions cannot execute. No resource,
permission, quarantine, coalescing, cleanup or model-request policy is weakened.

Select page movement now follows the pinned boundary-wrap rule: negative
overshoot selects the last row, positive overshoot selects the first. It is not
modulo remainder or clamp. Up/Down still wrap normally. The private startup/
Location `restore_views` future is heap-pinned at its existing ownership boundary,
preventing large inline future copies from overflowing the ordinary test-thread
stack; no larger stack, thread, task, store or timer was introduced.

This is functional qualification, **not whole VIS40/R6/V09/T44 PASS**. Full
unmasked frames remain different. Root acceptance, AUTH06 deferral and the
original24/24 / Go13/24 campaign ledgers are unchanged.

## Owners and deterministic regressions

- `oc-core/src/queries.rs::DialogShortcuts`, existing
  `config.rs::ConversationKeybinds` admission/leader resolution and
  `composition.rs` catalog projection: eight dialog command IDs, string/false
  validation, final-layer leader and disabled alternatives.
- `oc-tui/src/dialog.rs::SelectList`: logical action focus, semantic focused/disabled
  paint, title-only bold, exact footer hit rectangle and boundary page movement.
- `app/{input,mcp,live}.rs`: modal raw-key ownership, effective commands/chords,
  status/action target, current opaque binding, press retirement and modal reset.
- `app/tests/select.rs`: two scenarios cover focus/paint/caret, actual control
  values, 54-column stacked hit geometry, pending/no-match/empty inventory, hover,
  drag/resize/filter/instance changes and Ctrl/Shift/Alt modified release. Remap
  coverage retains default-key suppression, leader alternatives and unrelated
  Ctrl+Q/T/R/D ownership. Existing Sessions and Message Actions tests retain their
  Rename/Delete/copy/revert/fork behavior.
- Existing actual MCP held-request lifecycle checks retain positive queued-input
  acknowledgement, source bytes, leases, Location/reload retirement and no late
  effects. Only obsolete informational-footer expectations changed.

## Actual paired capture and source association

Each final attempt runs the rebuilt native ELF and pinned original under real
PTYs, with actual stdio peers and the same declared renderer/font/profile. No
renderer state, SQL row or result is seeded; no cells, images, version or cursor
are cropped, masked or normalized.

Final attempts share captured base `38946412c528dfb75f49f381380a1711a0534f77`,
tree `6b596adc3f4cab9c56dd54b871fc1ac5369db707`, dirty diff
`51049400f3dd1ab4d1b6db807059f762e4befbc2daa892a977822adc7faf25f7`,
source manifest
`4c12f9bd9392cf84b2e67baf1353f95d5101649c51c3df90eb65035500d43fa3`.
Each records successful `cargo build --locked` and native ELF SHA-256
`c78a6272ad483dd3cce33342c29103fa8cae771d8466065c95395f026ec7d049`.
Original source pin is `2670273ff17da96f85c5826ced57aa1b368754fa`, executable
SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

The capture runner hash is
`4adbf48597917e40345f462497d13af91768e400b025c09db64d18eb92d07254`;
footer probe hash is
`3de8fda0ecca1c99db0fd65095f7845aab3b5b33402f94a100b1420c27980bde`.
No Rust or capture-runtime edit follows these final source-associated captures.

Common invocation: jobs3/tests2, approved `TMPDIR`, `capture.mjs` with explicit
reference/native paths, `--build-oc true --geometry true --sample short
--sidebar hide --columns 120 --rows 40 --tool-preview true`. Each output directory
is fresh and immutable; each runner has a bounded 900-second process timeout.

| Final attempt | Additional flags / proof | Strict full comparisons |
| --- | --- | --- |
| `mcp-footer-default-attempt-005` | `--mcp-status true --mcp-footer default`; 27 paired stages | 54 DIFFERENT |
| `mcp-footer-remap-attempt-004` | `--mcp-status true --mcp-footer remap`; 26 paired stages | 52 DIFFERENT |
| `tool-preview-attempt-027` | Ordinary Home/body/expand/reopen/restart; 27 paired stages and two native resource views | 4 EQUAL / 50 DIFFERENT + 4 native-only entries |
| `cursor-temporal-attempt-030` | `--cursor-temporal blink --cursor-renderer webgl --cursor-case expanded` | 150 DIFFERENT / 20 unmatched temporal samples |

Both footer runs pass behavior at80×24,120×40,160×48: Tab/Shift+Tab, title-only
action bold, muted raised selected row, row-hover unfocus, unchanged Search caret,
effective prev/next/End/Page±10/Home, no-match disabled submit and exact Unicode
composer restoration. Default Ctrl+P/N wrap and remapped old Down/Enter/Ctrl+P
suppression are observed. Both final reference and native page selections are
last index2 after PageUp, first index0 after PageDown.

Independent real initialize role/PID receipts have this sequence on both sides:

| State/action | healthy | disabled | failed |
| --- | ---: | ---: | ---: |
| Initial / no-match / focused disconnect | 1 | 0 | 1 |
| Mouse reconnect healthy | 2 | 0 | 1 |
| Retry failed | 2 | 0 | 2 |
| Explicit disabled activation / mouse park | 2 | 1 | 2 |

All footer stages have zero provider requests, tool calls, MCP tools/call and
Shell effect counters. Disabled never starts before an explicit action. Remap
fixture uses F2/F3, Alt+U/D/H/E, F4 submit and F6/`<leader>t` toggle with Ctrl+G
leader; the same admitted effective map supplies native dispatch and footer hint.

Ordinary027 retains six provider requests (one title/five main), four effects,
three actual MCP calls and one17-byte Shell effect counter. Bounded body/guidance
facts, four presentations, artifacts and RAW operations remain unchanged through
new/reopen/clean restart; explicit read-only `/cards` retains its authorized pages.

Temporal030 retains all six owner/cadence/sampling/restoration/phantom checks.
Native raster cycles are3/3/4/3/4/4; reference3/3/4/3/3/3. Native hover/Search
hover parsed1578/1242 commands; reference Search25 commands is not a continuous
reference repaint claim. `check_cursor_temporal.py --compare-output
evidence/tui/cursor-temporal-matched-011 evidence/tui/cursor-temporal-attempt-030`
passes the144 opaque actual PNG /88,473,600 fixed-owner RGBA pixel audit. Its67
actual-phase matched pairs yield134 strict grid/PNG comparisons, **all DIFFERENT**.
Unequal phase counts never imply equality. Prior unsupported-sync/default/steady
and representative-geometry proof remains separately recorded, not a new full PASS.

## Experiments retained and current gates

Default001 used an incorrect Search-placeholder predicate after typing a query;
the real list was correctly filtered. Remap001 used literal `ctrl+g t` instead of
the original's registered `<leader>t` binding and leaked `t` into its Search; only
the isolated fixture was corrected. Default003 exposed native modulo page
movement and incorrect probe assumptions; the actual source rule corrected both
native and probe. Earlier observed reference PageUp0 is retained as that attempt's
observation, not asserted for the final runs. Pre-modifier-guard qualified attempts
remain immutable but are not the final source proof.

First broad gate exposed ordinary2MiB thread-stack overflow at startup; owned GDB
showed large nested inline futures, not Select recursion. Heap pinning the existing
restore boundary fixed the unchanged-stack targeted test. Subsequent gates caught
Sessions-owned raw Rename/Delete being blocked and stale lifecycle/footer/page
test assumptions; those owners/contract expectations were corrected without
discarding effects, privacy or cleanup checks. A late modified-release regression
was reproduced red, fixed in the release guard, and requalified with new captures.

Current `.local/t44-select-footer-gates-final5-20261008.log` records completed:

```sh
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

All PASS with jobs3/tests2/approved TMPDIR: **1745passed /0failed /11 unchanged
opt-in ignores across46 completed results**. TUI465, binary98, MCP41, adapters677,
PTYT3952, PTYT4234, runtime120 and subagents39. Actual release startup/refusal/retry
and discovery authorization/oversized/slow/absent/present checks preserve expected
exit, terminal restoration, retained owner and no Responses where forbidden.
Repo Python47/0, Node/Python syntax and source/docs/journal checks pass.

Captured padded `.txt` and raw `.vt` bytes retain intentional whitespace. Only
these exact final capture data formats are excluded from Git whitespace checking;
code/docs/JSON and every whole-grid/PNG comparator remain unchanged. Gate logs
stay ignored local artifacts; factual results and full final captures are published.

## Remaining acceptance

The source-required native Connected bold and native safe diagnostics remain
disclosed running-original differences; native version identity, renderer-edge
and all other full-frame differences are unmasked. This does not close full MCP
VIS40, prompt history/mouse caret VIS12, or the remaining frozen R1–R6/VIS01–VIS45.
Continue shared durable prompt history and ordinary prompt click-caret owners,
then all mandatory outcomes and final current-source R6/V09 gates. No task finish,
new campaign or acceptance reset is justified by this facet.
