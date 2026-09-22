# V00 capture evidence — 2026-09-22

**Actual original captures obtained; parity is not verified.** Latest complete
diagnostic run: [`attempt-05/capture.lock.json`](attempt-05/capture.lock.json).
Runner implementation and usage: [`scripts/tui_capture`](../../../scripts/tui_capture/README.md).
No production source, recovery comparator, fixture, or acceptance test was edited
by this capture work. No commit/checkpoint was made; parent owns delivery.

## Latest artifacts

| Scenario | Original PNG | Rust PNG | Grid differing cells | PNG differing pixels |
|---|---|---|---:|---:|
| Completed session/table | [reference](attempt-05/upstream/session-wide-completed.png) | [actual](attempt-05/oc/session-wide-completed.png) | 7,599 / 7,680 | 427,994 / 1,036,032 |
| Commands | [reference](attempt-05/upstream/commands-over-session.png) | [failed-state actual](attempt-05/oc/commands-over-session.png) | 7,680 / 7,680 | 1,034,330 / 1,036,032 |
| Models | [reference](attempt-05/upstream/models-over-session.png) | [failed-state actual](attempt-05/oc/models-over-session.png) | 7,680 / 7,680 | 1,034,402 / 1,036,032 |

Each PNG has adjacent `.cells.json`, `.txt`, and real `.vt` output. Full per-side
`raw.vt`, `inputs.json`, `protocol.json`, `bridge-spec.json` and stderr are saved.
Six comparator reports are at `attempt-05/*.{grid,png}-diff.json`; all exited **1**
(valid comparisons, unequal), not 2. Cursor differs in all three comparisons.
The Commands/Models Rust images deliberately retain failed actual states; they
do not represent equivalent successfully opened dialogs.

Original screenshots were visually inspected: completed Russian table, proper
fixture title and Context 6,763 / 3%, Commands overlay, and eight-model picker
with the current-model marker and Free labels are present. Rust screenshot
inspection confirms literal pipe-table lines, missing sidebar, bottom-aligned
limited transcript, raw model identifiers and devtools bar. Ctrl+P inserts `p`;
Ctrl+X then `m` leaves `pxm` instead of opening the expected dialogs.

### Protocol and provenance

* Original: `opencode v2.0.12`, executable SHA-256
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
* Original sparse source checkout HEAD independently queried in this session:
  `2670273ff17da96f85c5826ced57aa1b368754fa`, clean status.
* Downloaded original archive rehashed SHA-512:
  `fe1f885c76187572cc8545c2fe597cc7aa19a1020b08a1c8d3191b4b5cfe6d9b8949cfcabfa0f4b00179112d93f3b0d006b166270c95f1742a842b29cbc48aca`.
  It matches the integrity value supplied in the delegation; registry checking
  itself belongs to the parent.
* Rust: `oc 0.1.0`, `cargo build --locked` exit 0 on source HEAD
  `d232baa481ed6e4fc844359855d5f419f88b8a8e`; binary SHA-256
  `e36d2307361e56fc6a69d36a836a0f642c982a0b26340511add90d77ac715660`.
  Tree and dirty tracked-diff hash are in the capture lock.
* Both processes receive a newly built environment with isolated HOME/XDG and
  the same empty, public-fixture-only project. Original uses `--standalone`;
  no original background service or real credentials are used. The normal
  provider/config path is exercised for both applications.
* Original emits one valid title request and one valid transcript request;
  Rust emits one valid transcript request. Both transcript responses have hash
  `63600873471e6e72b8e7e98a14f242e32c5582668f8553d24c659147fc4ed9ed`
  (the supplied transcript with surrounding whitespace stripped).
* Safe protocol records contain actual tool names separately from the prose
  fixture: the prose mentioning write/edit/execute does not assert Rust tools.
* Common frontend: xterm.js 6.0.0 + Unicode11 addon 0.9.0,
  Playwright 1.58.2 / Chromium 145.0.7632.6. 160 × 48 cells, measured 1349 × 768
  pixels, 14px DejaVu Sans Mono, scale 1, dark theme.
* Latest fixture hash:
  `ed3822cd47c54b5102e6b6a658909cd5a87596827be5e3d598ab08ce7760f64a`.
  Common environment ID:
  `f3c1c2ec327aecf934a1fba05ce841c09ae1bb4d7f0904b12123fa1b563654bc`.

## Executed commands and attempts

Exact latest invocation (exit **1**, expected diagnostic failure):

```sh
node scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --output /home/opencode/ai/oc/evidence/tui/recovery-v00/attempt-05
```

[`attempt-05/commands.json`](attempt-05/commands.json) records the exact runner,
build, Git, font and six comparator argv, stdout/stderr and exits. Bridge exit
is 0 for both; requested process-group shutdown after capture produces original
exit 130 and Rust signal exit -15, recorded in protocol logs. These are capture
cleanup results, not graceful-shutdown qualification.

| Attempt | Invocation variation from above | Exit/result |
|---|---|---|
| 01 | output `attempt-01`, omit `--build-oc true` | 2: Playwright browser-path env set after module initialization; no captures |
| 02 | output `attempt-02`, omit `--build-oc true` | tool timeout at 120000ms: close-event race; actual partial captures retained, see `interruption.json` |
| 03 | output `attempt-03` | 1: overbroad title-request classification also returned title for transcript; diagnostic captures retained |
| 04 | output `attempt-04` | 1: all three original captures; two Rust dialog failures; all six comparator mismatches |
| 05 | as above | 1: improved durable recording, stronger completed-state predicate, post-PNG stability check; same product mismatches |

Additional executed setup/check commands:

```sh
# All exit 0:
npm install --prefix /home/opencode/.cache/opencode-tmp/opencode/t44-reference --save-exact @xterm/xterm@6.0.0 @xterm/addon-unicode11@0.9.0 playwright@1.58.2
PLAYWRIGHT_BROWSERS_PATH=/home/opencode/.cache/opencode-tmp/opencode/t44-reference/browsers /home/opencode/.cache/opencode-tmp/opencode/t44-reference/node_modules/.bin/playwright install chromium
node scripts/tui_capture/check_frontend.mjs
sha256sum /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode
sha512sum /home/opencode/.cache/opencode-tmp/opencode/t44-reference/reference.tgz
# Working directory: /home/opencode/.cache/opencode-tmp/opencode/upstream-v2
git rev-parse HEAD && git status --short
# Working directory: /home/opencode/ai/oc
git diff --check
```

The frontend check passed actual VT-buffer assertions for truecolor, styled
blank backgrounds, eight text attributes, CJK width continuation, combining
characters, cursor position/shape/visibility and DSR replies. It creates no
fake upstream capture. No full Rust workspace suite was rerun for test-only
capture scripts; the required actual Rust build was executed.

## Open conditions / exact continuation

1. **No full V00 freeze or parity claim:** elapsed duration and Rust token rate
   still use application wall clocks. Reasoning duration 6800ms is not injected;
   this fixture is text-only. Location is the recorded isolated project rather
   than `/tmp/space`, and new attempts change its name. Font fallback is unknown.
   A supported time/state stabilization mechanism is still needed for exact
   repeatable golden equality; no masks/tolerance were added.
2. Rust does not reach equivalent Commands/Models states and visibly differs
   in Session geometry/metadata/markdown. These are downstream V02–V06 changes,
   outside this test-only delegation. Reference requested sidebar/devtools
   settings are not falsely asserted as implemented Rust settings.
3. MCP attach error/stall scenarios and broader VIS01–VIS24 qualification are
   **NOT_RUN** here. This evidence does not close T44.
4. Continue with `scripts/tui_capture/README.md`, retain these immutable original
   captures, implement parent-owned product fixes, then run a fresh attempt.
   Resolve the time/location/fixture-state limitations before treating exact
   frame equality as a final acceptance condition.
