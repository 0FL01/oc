# T44 — typed live MCP status in Home footer

Date: 2026-10-08. Functional slice qualified; whole T44/R6/V09 remains ACTIVE,
not a whole VIS or pixel-parity PASS. No external blocker, paid generation, user
configuration change, acceptance/baseline change or task finish.

## Existing owners and implementation

- `app/mcp.rs::mcp_status_counts` derives connected/failed counts from the already
  admitted current `McpSnapshot`. No inventory means no item. Failed count takes
  precedence; Pending, Disabled and NeedsAuth are not invented failures. Existing
  Location/instance/generation/revision filtering is unchanged.
- `shell.rs` renders the pinned semantic status mark, base count and muted `/mcps`
  command. Footer mounts at width44/height12, command at width64; below16 rows its
  padding disappears. An occupied narrow footer contributes its real layout row.
  The native package version stays truthful rather than impersonating OC2.
- The exact painted item is the shared mouse target. Hover does not open it;
  actual click uses the existing `OpenMcps`/`LoadMcps` path. Modal, modifier,
  drag/selection and stale frame-size guards keep unrelated input inert. Opening
  is not a connection action, model call or configuration write. The existing
  modal controls still own explicit disconnect/connect/retry.
- No new registry, schema, service, poll, timer, provider projection or history
  rewrite. Permissions, credentials, bounded inventory and resource owners stay
  with their existing contracts.

Two focused tests in `shell/tests/home_mcp.rs` cover live status/count precedence,
semantic cell colors, boundaries, empty/constructor/foreign inventory, exact hit
extent, modal ownership, drag and exact composer draft/caret restoration. The
existing short-height version-row expectation was corrected from bottom−2 to
bottom−1 using the pinned source and actual64×12 capture, not to hide a failure.

## Actual paired current-source proof

Final normal attempt: `tool-preview-attempt-017/`. Command:

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true \
  --output /home/opencode/ai/oc/evidence/tui/tool-preview-attempt-017
```

Both sides report `PASS_BEHAVIOR_ONLY`:27 paired full styled-grid/PNG/VT/cursor
stages, plus two explicitly native-only bounded `/cards` detail/paging frames.
Actual stdio initialization fails via JSON-RPC in the controlled isolated peer;
the lifecycle audit records it. Connected → disabled → failed → recovered status,
real click/modal controls, Unicode draft/caret restoration and widths44/63/64/43
plus64×12 are captured. The Home sequence makes **zero provider requests**.

The subsequent four ordinary effects still use six requests (one title/five main),
three actual MCP calls and one real Shell counter effect (17bytes/one line).
Reopen/restart leave those calls, Shell counter/hash, tool operations, native four
bounded presentation events, capture descriptors and artifact hashes unchanged.
The native read-only details/paging have no underlying composer caret. There is
no seeded renderer, database write/import, permission bypass or output recovery.

Whole-frame strict comparison: **3 EQUAL,51 DIFFERENT**, plus4
`NATIVE_ONLY_RESOURCE_DETAILS` entries. Runner exit1 is diagnostic, not a green
visual gate. At63×24 the entire styled grid, PNG and cursor match. At44×24 the
entire grid/cursor match; the PNG has41 different pixels on x370,y240–280, an
unresolved right-edge raster difference. No cells/pixels/cursor/version are
masked. At120×40 the draft/restored Home frame differences include the genuine
native0.1.0 versus pinned2.0.12 identity; failed Home also retains the native
compact service-issue footer. At43×24 broader Home geometry is still different.
At64×12 pinned footer spaces overpaint underlying prompt-footer text; the actual
unmasked capture and native readable item both remain evidence, not equal claims.

The separately rebuilt current-source temporal regression is
`cursor-temporal-attempt-022/`:120×40, expanded real MCP card, WebGL, blink enabled,
synchronized publication. Native six-state cycles are4/4/3/4/4/4; pinned original
3/3/4/4/4/4. Both preserve measured same-owner cadence, sample-gap bounds, final
caret, Search/composer restoration and zero transient wrong-owner cursor states.
Native hover records2462 commands, Search hover1367; original Search records25
despite sustained input, not a claim of continuous original repaint. No tool
replay.144 actual full temporal PNGs audited:88,473,600 pixels and fixed-owner
RGBA/opacity checks. `cursor-temporal-matched-003/report.json` contains66 matched
actual raster-phase pairs/132 strict comparisons, all DIFFERENT. The runner has
148 DIFFERENT/24 unmatched phase samples. Functional blink proof is not pixel PASS.

Both final attempts associate the same locked build and source manifest:

- captured base `bfbe229652407b0fc7e32463b41ef22aa043eaf9`;
- dirty diff `942fe059a4e1c88677da8b08a198c878170a5475b8f56ecb8b2c95471af85344`;
- source manifest `0c912fd5393e52fa49e2de198057a3dfffe7ea002870be00ba58b9aa8ae792b0`;
- native ELF `0d784b22c54504c6647f2abd84faab0e4ae9a47fb8ae8a0039d75793c042eb37`;
- pinned donor commit `2670273ff17da96f85c5826ced57aa1b368754fa`, ELF
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

## Experiments and current-source checks

- First targeted test exposed gap cells inheriting short prompt foreground;
  the actual item now explicitly paints its own base canvas style. Two tests PASS.
- Attempt014 qualified native behavior but original short64×12 failed the old
  contiguous-label predicate. Actual source/frame established the last-row footer
  and transparent-space overpaint. Geometry corrected; probe records that genuine
  reference condition rather than claiming readable contiguous original text.
- Attempt015 qualified both sides and revealed full63×24 equality; failure/retry
  coverage was then added using the real peer, not a synthetic UI status.
- Attempt016 qualified original; native failed a strict capture resample while
  the independent five-second alert expired. Final017 explicitly waits for natural
  toast settling before the settled live-footer capture. No image normalization.

Final log `.local/t44-home-mcp-gates-20261008.log`: workspace fmt check, locked
strict workspace/all-target Clippy, **1733passed/0failed/11 unchanged opt-in
ignores across46 completed records**, locked and ordinary debug builds, locked
release build (2m10s), both help commands, actual release startup and discovery
checks PASS. Includes461 TUI,97 binary,52 PTY T39,41 MCP,670 adapter,120 runtime,
39 subagent tests, existing terminal restore/idle/fairness/memory/security cases.
Live campaign ignores are not exercised or reset. The startup probes preserve
expected failure exits/terminal restoration and discovery-only request behavior.

Node syntax/Python compile, repo Python/docs/journal and source diff checks are
recorded with delivery. Captured padded `.txt` and raw `.vt` retain exact bytes;
they alone are excluded from Git whitespace checking for the final attempt paths,
never from visual comparison or source/code/docs/JSON validation.

## Remaining mandatory scope

Continue unresolved Home43 geometry/right-edge raster/short overpaint, MCP modal
styled details and every frozen R1–R6/VIS01–VIS45 outcome. Whole T44 and final
R6/V09 require their own full current-source closure. AUTH06 remains deferred;
original24/24 and Go13/24 ledgers are unchanged. Unrelated `.opencode/` remains
unread/untouched; no paid generation, credential or authoring-agent config edits.
