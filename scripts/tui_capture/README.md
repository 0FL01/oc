# V00 actual-executable capture tooling

This is **test-only** Python/Node tooling. It adds no production dependencies.
`bridge.py` runs each explicitly supplied executable in a real PTY with a newly
constructed environment, isolated HOME/XDG state, and a local HTTP Responses
fixture. `capture.mjs` routes every PTY byte through the same xterm.js/Chromium
frontend, saves full styled cells and PNGs, and invokes the recovery comparator.
`frontend.js` reads xterm's VT buffer; it does not draw an application screen.

## External prerequisites

Use the approved external directory (no `npm install` in this workspace):

```sh
npm install --prefix /home/opencode/.cache/opencode-tmp/opencode/t44-reference --save-exact @xterm/xterm@6.0.0 @xterm/addon-unicode11@0.9.0 playwright@1.58.2
PLAYWRIGHT_BROWSERS_PATH=/home/opencode/.cache/opencode-tmp/opencode/t44-reference/browsers /home/opencode/.cache/opencode-tmp/opencode/t44-reference/node_modules/.bin/playwright install chromium
node scripts/tui_capture/check_frontend.mjs
node scripts/tui_capture/check_capture_geometry.mjs
```

Python 3 stdlib, Pillow (PNG comparator), fontconfig and DejaVu Sans Mono must be
available. No fonts, browser, node_modules, or reference executable are vendored.
The tooling npm lock is copied into each evidence attempt. The original binary
is explicitly supplied and checked against the pinned SHA-256; the authoring
agent binary/configuration is never discovered or used.

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --build-oc true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/attempt-05
```

Output directories are immutable attempts: an existing directory is rejected.
Choose a fresh attempt name. `--tools ABSOLUTE_DIR` selects the external tooling
directory. Omitting either executable explicitly records that side as skipped;
missing pairs stay blocked. `--build-oc true` runs `cargo build --locked` and
records it; otherwise the association of an existing Rust binary with the source
commit is **not attested**. CLI exit `1` retains mismatches/failed predicates;
exit `2` denotes a runner blocker. Neither is parity success.

## What is captured

### Current prompt/paste diagnostic (VIS07), serial fresh builds

`node scripts/tui_capture/run_prompt_paste.mjs NEW_CAMPAIGN`
creates a new immutable campaign. Do not run another Cargo command until it exits.
Cold three-line chip frames wait for the actual chip and five unchanged full-grid
polls 200 ms apart before capture; settlement timing/cursor are recorded. Pending
leader frames keep their original short sampling delay. Navigation submissions
require completed main + title requests and a stable restored frame.
Each chip/default and extra/longdraft pair at 79/80/120/121×40 independently runs
`cargo build --locked`, seals source hashes before/after build/capture and uses the
explicit pinned reference plus `target/debug/oc`. Both real CLI configs set
`session.tps=false`. The existing default sequence captures the same three-line
chip normal/pending/restored and actual mouse expansion. The unchanged extra
sequence inserts the bounded Enter text at the actual middle caret and records
the full outgoing user input. Each side allows at most one main request and one
auxiliary title; the default chip sequence makes zero requests.

`--leader-paste-navigation true` is a thin exclusive alternative within
`--leader-pending`: paste a wrapped prefix, paste the same three lines twice,
capture actual expansion, three Up/three Down samples, then pending-leader Enter
and the exact raw user wire. It checks that repeat-paste expands rather than
duplicates input. It uses no renderer injection or live provider.
Pinned OC2 inserts a raw trailing space outside the paste-chip extmark and
preserves it on expansion; native sends only the original pasted input. The
fixture accepts these explicitly different exact wires and the analyzer records
the mismatch; it is not a raw-input parity waiver.
The campaign also captures a second 120×40 pair with
`--leader-paste-suffix-space true`: type an ordinary space after the first chip,
repeat the identical paste, observe two separate chips, click-expand both and
submit the exact two-paste wire with pending Enter. This distinguishes the real
suffix-space false case from the raw-end true-expansion case. Both variants stay
bounded to two local fixture requests per binary; full color/cursor frames remain.

Run `node scripts/tui_capture/analyze_prompt_paste.mjs CAMPAIGN` once to create
the immutable `prompt-paste-analysis.json`. It checks the current native/original
true-expansion contract, source association, bounded request counts and exact
raw drafts, and retains full unmasked grid/PNG/cursor comparisons including
version, footer, token and geometry differences. Behavioral verification is
separate from VIS07 PASS, which still requires every mandatory paired frame.
The historical `analyze_leader_pairs.mjs` and prior evidence remain unchanged.

### Ordinary apply_patch / original patch (VIS35), opt-in

```sh
node scripts/tui_capture/capture.mjs \
  --apply-patch true --geometry true --sidebar hide --sample short \
  --columns 121 --rows 40 --patch-view auto --patch-wrap none \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/apply-patch20260927-23
```

Use a new immutable output name. Supported sizes are 80/120/121/124/125/160×40.
Omit `--patch-view` for defaults; explicit values are `auto|unified|split`,
with `--patch-wrap word|none`. The original uses content width for its auto
breakpoint (U18 `context.width`), so terminal 121 columns alone does not prove
that its diff switches to split.

`apply_patch_fixture.py` emits ten actual ordinary-function calls, with matching
`patchText` for original `patch` and native `apply_patch`: add, empty create,
two separated update hunks, delete, move, full replacement, multi-file add,
multi-file stale preflight, stale single file and configured denial. No fake
tool results, renderer injection or transport adapter is installed.
The fixture-only original plugin admits the existing bundled U19 executor
through the standard context hook and records real execute.before/after events.
It does not replace the executor. Its input admission schema mirrors U19 Input;
the provider records and checks the actually advertised function definitions.
The pinned donor and inherited repository `.opencode/` are never modified.

Fixture setup checks the owned isolated root and seeds only a fixed path list.
Both binaries deny all other actions/resources and authorize only those exact
fixture mutation paths; `denied.txt` remains denied. Independent snapshots
record hex bytes, SHA-256, modes and presence, read-only SQLite, and original
executor metadata. Snapshots off; `/new`, reopen, `/undo`, `/redo` and actual
process restart must preserve files and tool/provider counts. Captures08 onward
also include the native `patch_effects` owner table, checked byte-for-byte
through Redo/restart. Reopen checks do not establish an OS-level read syscall
audit. Fresh attempts11 onward wait for native `Patch · arguments streaming ·
no effects yet` rather than a completed effect card. Real provider deltas are
logged, and independent pending snapshots must retain prior file bytes/modes,
durable operations and effects. The bounded SSE hold is eight seconds from
attempt15 onward; only an owned child is paused for a stable unmasked capture.
Completion must remove the pending label. Durable rows/display links are unique,
and restart must retain them with zero tool/provider re-execution. This does not
qualify an executor-held state or every transient render between samples.

Run `node scripts/tui_capture/analyze_apply_patch.mjs BASE NEW_REPORT 22` to
validate and aggregate attempts01–22. Reports also refuse overwrite. Full
styled-cell/PNG comparators include cursor and retain all failures. VIS36
accept/reject, real-model A09 authorship, and PTY postcommit partial/cancelled/
unknown outcomes remain separate gates. A stale second operation can fail in
preflight with no success prefix; the fixture does not claim otherwise.
Attempt10 aligns `session.tps=false` on both binaries. Attempts03–09 retain
the historical native default TPS display, visible in their unmasked frames.

### Session compaction (VIS34), opt-in bounded campaign

```sh
node scripts/tui_capture/capture.mjs \
  --compaction true --compaction-trigger manual \
  --geometry true --sidebar hide --sample short --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/compaction20260927-23
```

Use a fresh output directory. The exclusive mode preserves all existing modes.
`manual` (default) executes three actual seeded main exchanges, a real bounded
`sleep 25` tool, `/compact`, palette Compact session and a read-only snapshot of
the single coalesced pending operation. The fixture never executes a tool itself.
It holds the real Responses summary stream after its first text delta, captures
two running frames, then completes with actual input/cache/output/reasoning usage.
Next context, failed/cancelled checkpoint preservation, clean exit/relaunch,
saved-session reopening and the next real provider request are verified.
The only seeded workspace file is `vis34-owner-approved.txt`; hashes are compared
through compaction. SQLite observations use `mode=ro`, with no direct transcript,
context or checkpoint writes. Fixture config sets snapshots off and keep.tokens=0.

`--compaction-trigger threshold` uses auto=true, context=40000/output=2048,
buffer=20000 and a 23000-input-token usage anchor on the third seeded response.
`--compaction-trigger overflow` sends exactly one actual HTTP 400
`context_length_exceeded` on the next main request, followed by normal recovery.
Both modes capture real automatic summary requests, checkpoint and rebuilt main
context; neither claims the full threshold-boundary/restart matrix.
All modes cap provider requests at 24 per side. Stream holds expire at 30 seconds;
PTY predicates have explicit timeouts. No live credentials or remote API is used.

`compaction_fixture.py` distinguishes pinned OC2's final user buildPrompt from
Rust's Developer summarizer plus JSON causal transcript. OC2 keeps tool definitions
on the summarizer request; Rust offers none. Real request input/instructions/tool
schemas are recorded rather than inferred from a generic text search. Provider-native
compaction is unqualified: no production mechanism is registered.

Every full styled 120x40 grid/PNG, VT stream, request, input, build command and
source manifest is retained. No title, duration or dynamic cell is masked.
Default running captures use animations=false; two partial-summary frames do
not qualify the animated Braille spinner's cadence or post-completion CPU/wakeups.
`--compaction-tps false` explicitly sets `session.tps=false` in both real CLI
configs (default leaves native unset and keeps original's existing false).
`--compaction-animation true` is restricted to the bounded threshold fixture:
it enables real animations on both sides, records timestamped raw PTY chunks and
live Compaction-row samples, then pauses the owned child with SIGSTOP for one
stable full running grid/PNG and resumes with SIGCONT in a finally block. Actual
phases are independently captured, never selected/aligned to match. Browser
sampling includes IPC overhead; raw chunk timestamps are not synthetic 80ms ticks
or application paint timestamps. This does not measure idle CPU/wakeups or FPS.

The campaign-specific independent checker is:
`node scripts/tui_capture/check_compaction_evidence.mjs evidence/tui/recovery-v00`.
It verifies sealed capture hashes, dimensions, safe tool boundary, coalescing,
causal pairs, immutable raw messages, usage, checkpoint failure/cancel/restart and
nonempty filesystem hashes for attempt 10, overflow attempt 08 and the observed
native usage-threshold mismatch in attempt 07. Its exit 0 means the evidence is
internally consistent, including the production failure; it is not VIS34 PASS.
See `evidence/tui/recovery-v00/compaction-report.md`.

Fresh post-fix immutable campaigns are manual 12, unchanged-threshold 13 and
overflow 14. They additionally reopen, submit a real restarted provider request,
and exercise `/undo` plus `/redo` with no extra provider/tool calls. Validate with
`node scripts/tui_capture/check_compaction_evidence.mjs evidence/tui/recovery-v00 --fresh`;
this writes `compaction-validation-fresh.json` and preserves historical validation.
`node scripts/tui_capture/analyze_compaction_frames.mjs evidence/tui/recovery-v00`
writes full unmasked cell/pixel counts and marker positions for every fresh frame.
Attempt 11 retains the failed singular/plural Undo predicate diagnostics.

Final-source campaigns are manual 15, threshold 16 and overflow 17, using the
same unchanged capture fixtures/probe as 12–14. Use `--final` instead of `--fresh`
for the evidence validator and `--final` for the frame analyzer to write separate
`*-final.json` reports. Optional `--quiet` only shortens console output; full
derivative report measurements remain intact. Final validation additionally
checks the durable current-user anchor while automatic summary is running and
actual typed failure diagnostic. No config-alias, animated-cadence or FPS claim
is inferred from canonical held-stream screenshots.

Latest production-source captures are manual 18, threshold 19 and overflow 20.
`--latest` on validator/analyzer writes separate `*-latest.json` reports.
Add `--optional` to include threshold 21 (explicit TPS false on both sides) and
22 (the same false setting plus actual animated running observation), writing
`*-latest-options.json`. Their provider fixture remains unchanged; option fields
and bridge/probe changes are declared in their own locks/specs. Exact commands
are in each immutable `commands.json` and `compaction-report.md`.
`node scripts/tui_capture/observe_compaction_timeline.mjs evidence/tui/recovery-v00`
replays 22's real chunks through the pinned xterm parser and writes derivative
`compaction-animation-observations.json`: Compaction-row glyph changes retain
actual monotonic PTY-read timestamps, including the deliberate pause gap. It
also checks that the glyph is absent from the completed row in subsequent
generation-0 output; that observation is not a scheduler wakeup/deadline gate.

### Conversation-only Revert / whole-tail Redo (VIS33)

`--revert-redo true` is an exclusive opt-in mode with paired explicit binaries,
`--geometry true --sample short --sidebar hide --columns 120 --rows 40`.
`revert_redo.mjs` admits three actual user submissions through each real PTY.
The bounded local Responses server answers identical prompts with identical text
and separately handles the application's asynchronous title request. No title
or history is inserted into a database. Fixture `opencode.json` sets
`snapshots:false`; admitted `cli.json` sets
`keybinds: {"session.redo":"<leader>r"}` on both sides (default ctrl+x r).

The probe clicks turn two → Message Actions → Revert, observes two hidden user
messages and its restored draft, captures normal/hover/selection-drag guard,
then restores the whole tail with one card click. Fresh Revert boundaries precede
shortcut, slash `/redo` and filtered palette Redo. It switches to a new Home,
reopens through `/sessions`, exits cleanly, restarts the same isolated data root,
reopens, sends PageUp/PageDown and restores via the durable card. Three-turn
paging is bounded viewport coverage, not a large-history page-load qualification.

Every DB observation uses SQLite `mode=ro`; native archive/visible user counts,
upper_seq and redo_tip are retained, as are original persisted revert JSON and
archive-derived counts. Fixture `vis33-owner-approved.txt` and both config files
are hashed before actions and at every boundary observation. No real credentials
are needed. All inputs, request text, styled cells/cursor, PNG, render geometry,
VT streams, source manifest and build result are retained. Existing attempt
directories are refused. Audit a completed attempt with:

```sh
node scripts/tui_capture/check_revert_redo_evidence.mjs evidence/tui/recovery-v00/revert-redo-20260926-07
```

Behavior PASS does not imply comparator equality: the completed run has 29 paired
full frames, all 58 unmasked grid/cursor and PNG comparisons DIFFERENT.
See `evidence/tui/recovery-v00/revert-redo-report.md` for the attempt ledger.

### Bounded variant and Shell modes

`--bounded-mode variants|shell` requires paired explicit binaries, `--geometry
true --sample short --sidebar hide --columns 120 --rows 40`, with no other
interaction mode. It creates opt-in isolated lowercase fixture profiles and
selects them from Build through the real `/agents` dialog. The selected frame
checks titlecased metadata and absence of the successful `agent: …` toast.

* `variants`: the declared `variant-dialog.json` catalog (`fast=high`,
  `none=low`) is cycled with Ctrl+T. Four actual transcript requests must carry
  effort absent → high → low → absent, plus exactly one title request. Nine full
  frames per side: home, selected profile, four completed requests, three cycle
  states.
* `shell`: two real tool calls print one short line and 40 numbered lines.
  The fixture inspects registered tool names and schemas before issuing calls;
  pinned v2.0.12 advertises `shell(command, …)`, native `bash(argv, …)`.
  Each profile permits only the exact fixture command resources. The provider
  accepts three transcript requests (two calls followed by the final answer)
  and one title request, verifies the returned fixture lines, and rejects extra
  requests. No model-generated prose is used to select a tool. Native executes
  direct `printf` argv; original executes equivalent quoted shell commands.
  Completed, hover, expanded and recollapsed frames are captured. Expansion
  resizes both real PTYs to 120×80 to show all 40 lines; recollapse restores
  120×40. Six frames per side including home and selected profile.

`bounded-checks.json`, `protocol.json`, and recorded PTY inputs distinguish
behavioral PASS from unmasked styled-grid/cursor and PNG equality. Successful
real original Shell output includes `Command exited with code 0.`; native omits
that text. This observed difference is retained. Previous attempts are immutable.
See `evidence/tui/recovery-v00/bounded-evidence-report.md` for actual runs/counts.

* 160 × 48 real PTY; xterm.js 6.0.0 / Unicode11 addon / Chromium 145.0.7632.6,
  14px DejaVu Sans Mono, device scale 1. Pixel geometry is measured from the
  terminal DOM, independently of the cell geometry supplied to both PTYs.
* Identical public fixture prompt, transcript, model IDs/display names, costs,
  limits and usage (6000 input + 763 output = 6763). The fixture accepts only
  loopback `/v1/responses`, selected model, streaming, and fixture prompt. It
  records request hashes, safe contract facts and actual registered tool names,
  never raw request prompts or credentials.
* Original uses normal `--standalone`, native
  `@opencode/ai/providers/openai/responses`, schema-v2 `providers/settings`.
  Rust uses normal `oc tui`, `@ai-sdk/openai` alias, `provider/options`.
* Original's normal title-generation request receives the fixture title; its
  exact title-agent instruction distinguishes it from the transcript request.
  Built-in models.dev catalog is disabled by the supported
  `plugins: ["-opencode.models.dev"]` directive so only the eight fixture models
  appear. This is configuration, not a replaced renderer or seeded DB.
* Bracketed paste + Enter; then Ctrl+P; Escape only if Commands actually opened;
  Ctrl+X followed by `m`. Each input and terminal-generated reply is recorded.
* Stable full-grid/cursor polling plus semantic markers, fixture completion and
  completed-turn footer precede capture. A post-PNG grid check detects changes
  during screenshot. Failed dialog predicates retain the actual screen with
  `FAILED_STATE`; they cannot count as completed dialogs.
* `.cells.json` preserves every blank cell and resolved RGB, modifiers, wide
  continuations, and cursor. `.txt` is review convenience only. `.vt` contains
  the actual bytes up to capture, `raw.vt` the full run. `protocol.json`,
  `inputs.json`, `commands.json`, and `capture.lock.json` preserve provenance.
* Each frame also has `.render.json`: float `.xterm-screen` bounds, viewport/DPR,
  measured cell sizes, independent terminal/viewport/text/row/cursor/selection
  layer CSS backgrounds and layout boxes, and any screen canvas CSS/intrinsic
  sizes. It records the exact screenshot clip and actual PNG dimensions, plus
  before/after layout readings. Two `requestAnimationFrame` callbacks precede
  each capture (including resize captures) and are recorded per frame. A changed
  layout is flagged, not masked or cropped away. The render file hash is locked
  with the cell and PNG hashes. No DOM text, canvas pixels or URLs are read by
  this geometry probe. The common environment ID hashes only the configured
  profile (including requested columns/rows), never measured per-side geometry;
  equal-size frames on both sides therefore retain the same environment ID even
  if their painted screen bounds differ.
* Optional `--refresh-before-capture true` calls xterm's full-row repaint from
  its existing VT buffer on **both** sides before every screenshot and records
  that action in `.render.json`. It does not alter the PTY or mask/crop pixels.
  Use it to diagnose stale DOM rows after resize; preserve the unrefreshed
  attempt alongside it rather than treating a repaint as an application fix.
  For DOM-rendered xterm, `.render.json` also records only the final two
  `.xterm-rows` children (row index, position, foreground/background and bounds)
  and up to four trailing element children per row (index, computed width,
  position, colors and bounds). It includes counts but never child text or
  arbitrary attributes; compare the last span's right edge with the screen's
  right edge when a styled final-column cell differs in PNG only.

For a diagnostic pair with the **same configured primary profile**, add
`--agent-profile true`. Both isolated configs then select `Reader`,
with the same fixture-only system instruction and the pinned dark-blue
`#5c9cf5` color (native categorical slot zero). The bridge refuses a
transcript request unless the configured instruction is really present in
the Responses request; the runner also checks that the selected ID appears
in the Home prompt. This opt-in does not change the ordinary absent-agent
fixture, and does not establish general permission-policy parity: original
and native schemas/effective defaults differ outside this exercised read.
Original schema: `packages/schema/src/config.ts:35-58` and
`config/agent.ts:9-21`; native definitions: `crates/oc-adapters/src/defs.rs`.

The common environment ID is SHA-256 of recursively key-sorted JSON profile;
it excludes executable identity. Fixture hash covers hashes of the entire
supplied fixture directory plus bridge source (including both config adapters
and fake-protocol implementation). Per-side launch records contain the actual
derived config and local endpoint. Per-side binary SHA, version, Rust commit/tree
and tracked dirty-diff hash are locked; runner source hashes cover uncommitted
test tooling. Parent-owned changes remain parent-owned.

`check_frontend.mjs` separately qualifies RGB, styled blank backgrounds,
bold/dim/italic/underline/blink/inverse/hidden/strike, CJK width-2/width-0 cells,
combining text, cursor hide/show/position/shape, and terminal DSR replies.
These synthetic checks are never labelled upstream captures. xterm represents
blink as one attribute (rapid/slow blink are not separately qualified); pinned
private xterm cursor/theme APIs are protected by this check. Fallback fonts are
explicitly unknown rather than guessed. The supplied captures use Cyrillic and
box drawing covered by DejaVu Sans Mono.

## Known qualification boundaries

Original disables animations and devtools through supported CLI config; Rust's
effective settings may differ and are visible in the captures. Profile settings
are requested settings, not proof that Rust implements them. Applications retain
real elapsed-time/token-rate fields. Those are neither frozen nor masked, and
the requested 6800ms reasoning state is not synthesized. Both run sequentially
in one empty isolated project, with separate HOME/data; the actual location is
recorded instead of pretending it is `/tmp/space`. New attempt paths also change
the displayed location. Therefore exact repeat-run or pair parity remains open.
MCP attach error/stall and broader VIS qualification are not executed here.

Source contract anchors (pinned upstream source):
`packages/cli/src/commands/commands.ts` (`--standalone`),
`packages/schema/src/config/provider.ts`,
`packages/core/test/config/provider.test.ts` (native Responses package),
`packages/core/src/config/plugin/source.ts:113-120` (remove directive),
`packages/core/src/plugin/agent.ts` (title request),
`packages/tui/src/config/index.tsx` (theme/animations/sidebar/devtools),
`packages/cli/src/server-process.ts` (environment controls).

## V03 geometry modes

`--geometry true` captures actual Home and completed-session screens and skips
dialog inputs. `--sample short` supplies a one-line answer; `--sample rows` supplies
a 90-line code block. `--sample rows-reflow` keeps 90 ROW markers in a code fence,
with 90 ASCII characters after ROW-000 through ROW-040 so those lines wrap at
80 columns but not 160. The selected sample participates in the fixture hash.
`--matrix true` then resizes both real PTYs through 80×24,120×40,160×48,
43/44/119/120/121×48,120×80 and back to 160×48. Per-side
`geometry-checks.json` records observed row-marker counts and sidebar presence;
these targeted predicates are not full-grid parity. The final identical paste
is shown as an upstream paste chip versus Rust multiline text (V05 remains open).
`--columns`, `--rows`, `--sidebar hide`, and `--devtools true` select paired profiles.
Both sides receive explicit CLI presentation settings in admitted `cli.json`.
Native title requests now use the real tools-empty, 256-token title contract;
completed-footer predicates use the V02 model display name.

`--scroll-resize true --sample rows --geometry true` captures a single-line
draft/cursor, scroll-away, 80×24 shrink, 160×48 grow, one-row Down and bottom
re-pin in each executable. `scroll-checks.json` records actual row markers and
cursor-at-draft-end assertions. Original uses its supported Ctrl+Alt+Y/E scroll
bindings; native uses actual SGR wheel events over the transcript, since Up/Down
navigate the focused editor/history and would overwrite the draft. This does
not qualify keymap parity.
For `--sample rows`, both sides check that the first visible marker survives
shrink/grow; native additionally checks a top-offset clamp after growing to
160×80. For a width-sensitive diagnostic, use `--geometry true
--sample rows-reflow --scroll-resize true` with an initial 160×48 terminal and
a fresh output path. Each side records the first visible ROW marker and its screen y
coordinate for away, 80×24 shrink and 160×48 grow in `scroll-checks.json` and
`scroll-resize-anchors.json`. The original may retain a physical scroll offset
rather than the same semantic marker; neither side fails solely for an anchor
change. Draft/cursor checks and bottom re-pin still apply. This fixture grows
back to ROW-041 before the Down input and requires the first visible marker to
advance to ROW-042; an unchanged frame is a failed scroll. `--sample rows-reflow
--matrix true` is rejected; the native 160×80 clamp is rows-only to keep those
existing marker-count and clamp assertions.

For the completed real-read exploration group, run a **fresh** output directory:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --columns 120 --rows 40 \
  --exploration-click true --output /home/opencode/ai/oc/evidence/tui/NEW-ATTEMPT
```

This opt-in finds `→ Explored — 1 read` separately in each live styled grid,
sends SGR mouse down/up through the PTY bridge, and waits for the header to
remain with `Read fixture-note.txt` below it. A second dynamically located
click waits for the detail to disappear. Per-side `exploration-checks.json`
records predicates and coordinates; `inputs.json` records the actual PTY bytes.
`exploration-expanded` and `exploration-recollapsed` each get their own styled
grid/PNG/VT capture, with independent grid and PNG comparator reports and exit
statuses in `capture.lock.json`. Captured states do not imply frame equality:
only comparator status `EQUAL` establishes equality for its reported mode.

For a paired retained-tab interaction, use a **new** output path and the
completed real-read/profile fixture:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --tab-click true \
  --output /home/opencode/ai/oc/evidence/tui/NEW-TAB-ATTEMPT
```

After `session-wide-completed` is stably captured, `--tab-click true` requires
one visible painted ` + ` after the fixture-titled old tab in **each side's own**
row-0 styled grid. It clicks the plus (one-based SGR down/up via the real PTY),
waits for Home with the retained old tab and synthetic `+ New session` title
while the old answer disappears, then locates and clicks the old title again.
It waits for the completed old answer and add control to return before saving
`tab-added` and `tab-returned` `.cells.json`/`.txt`/`.png`/`.vt` per side.
`tab-checks.json` and `capture.lock.json` record observed coordinates,
predicates, click bytes, stage failures, and the per-side result; the normal
grid/PNG comparators record their exit statuses separately. Capture and
interaction success do not imply pixel equality. Failures keep diagnostic
frames and a nonzero exit. Existing attempt paths are rejected before launch;
use a fresh path after any failed run. This is test-only capture tooling.

To exercise the actual hovered close control, add `--tab-close true` to the
paired command above and use another **fresh** output directory. After opening
synthetic Home, the runner sends a real SGR mouse-motion event on each side,
locates that side's painted `✕`, then sends a matching down/up. It requires the
old transcript to return, the Home tab to disappear, and provider request counts
to remain unchanged. Separate `tab-close-hovered-before` and
`tab-close-closed-after` styled grids, PNGs, VT, input bytes and predicate checks
are retained. This replaces the old-tab return click only for this opt-in;
interaction success is not a whole-frame parity claim.

To exercise the keyboard close binding instead, add `--tab-close-key true` to
the paired `--tab-click true` command and use a **fresh** output directory. It
requires the same 120×40 Reader/tools profile and cannot be combined with
`--tab-restart true` or the mouse-close `--tab-close true`. After each side's
real add click reaches synthetic Home, the runner sends genuine Ctrl+X followed
by `w` through that side's PTY. It requires Home to disappear, the old transcript
and add control to return, and provider request/completion counts to remain
unchanged. `keyboard-close-after` has its own styled cells, PNG and VT per
side; `inputs.json`, `tab-close-key-checks.json` and `capture.lock.json` record
the actual input bytes, predicates, counts and result. Failed predicates cannot
yield a keyboard-close PASS. The normal full-grid/PNG comparators report equality
separately; no fixture shortcut or masking is used.

For a paired **real two-session restart**, use the same Reader/tools 120×40
command with `--tab-click true --tab-restart true` and a fresh output path
(without `--tab-close` or `--exploration-click`). After the first real read,
the runner clicks `+`, pastes/submits the fixture prompt on synthetic Home,
and requires a second, independently titled real session and a second read
tool round-trip. It clicks the old tab, captures `tab-prequit-old`, and sends
Ctrl+D through the PTY. This is the pinned original `app.exit` binding
(`packages/tui/src/config/keybind.ts:48`), also supported by native on an
idle empty composer. The bridge records `exit` code and `termination:natural`;
`stop` still forces teardown and cannot satisfy graceful-exit verification.
Only after exit code 0 does the same bridge relaunch the **same executable**
with the same HOME/XDG/project/config/server (no seeded root/import). Both
PTY generations have separate raw VT, protocol and input files and hashes
in `capture.lock.json` under `generations`; every frame has its own VT suffix,
styled cells and PNG. The prequit and restored entry, old history and other
history (via actual painted tab clicks) are compared separately for grid/PNG.
Provider transcript and title counts must remain unchanged after relaunch
and clicks. `tab-restart-checks.json` records tab order, history markers,
observed selection and provider counts independently per side. In the pinned
v2.0.12 executable, two real tabs persist but a bare startup selects a *new
synthetic Home* rather than the old tab selected before exit. Native now follows
that observed route while preserving its saved IDs; `TAB_RESTART_CHECKS_PASS`
requires Home on both sides. Any other entry route is reported separately as
`TAB_RESTART_SELECTION_DIFFERENT`; clicking old and second histories verifies
their replay without provider requests. No selection mismatch is labelled PASS, and
comparator exit 1 means differing cells/pixels rather than a fixture failure.
The fixture asserts exactly four transcript requests/completions and two title
requests/completions on each side before quit; a relaunch/click must add none.
Independently randomized Home examples and real elapsed-time fields can differ
even with the same selected route, so those frames remain diagnostic unless
their underlying visible state happens to match. No masking or fixed-clock
substitution is used to turn such a comparison into a parity claim.

For a paired **real session rename and restart**, use a new output path and
the Reader/tools profile (without any tab-click, tab-close, tab-restart,
exploration-click, variants, resize/matrix, seed-root or startup-error mode):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --rename-session true \
  --output /home/opencode/ai/oc/evidence/tui/NEW-RENAME-ATTEMPT
```

Both PTYs complete the real read/tool/title fixture first (two completed
transcript requests and one completed title request each). Each then receives
Ctrl+R to open its own `Rename session` dialog with the fixture title prefilled.
The runner uses Home, Shift+End and typed keyboard input to replace the field
with `Paired renamed session`; it captures the prefilled and edited dialogs,
submits Return, and requires the renamed painted tab and original transcript.
Ctrl+D must naturally exit code 0 before the same bridge relaunches the same
binary with the same isolated HOME/XDG/project/config/provider. On the restored
Home, it finds that side's renamed tab, clicks it with real PTY mouse input and
requires the persisted title and transcript. `rename-checks.json`,
`inputs.json`, per-generation raw VT/protocol/input files and
`capture.lock.json` retain predicates and provider counts. Rename/relaunch/
history navigation must issue no additional provider request. Styled grids,
PNGs, VT and ordinary unmasked grid/PNG comparator reports are preserved for
`rename-prefilled`, `rename-edited`, `rename-after`, `rename-restored-home` and
`rename-restored-session`; an interaction PASS does not mean frame equality.

To qualify genuine bare `/rename` title regeneration, add
`--regenerate-title true` to the paired rename command above with a **new**
output directory. This opt-in keeps the completed explicit rename and its
captures, then types `/rename` and Return through each real PTY. The fixture
serves a distinct `Regenerated fixture title` only on the second title-provider
request in this mode; ordinary samples and the two-session restart fixture
keep their original title responses. The original shows slash autocomplete;
the runner dismisses only that suggestion with Escape before submitting Return,
as in the pinned upstream command test. `rename-checks.json` requires a unique
painted regenerated title, disappearance of the manual and initial titles,
exactly one additional completed title request with the expected response hash,
no extra transcript requests and no invalid requests. The new
`rename-regenerated` frame and `rename-regenerated-restored-home` /
`rename-regenerated-restored-session` frames retain full unmasked styled grids,
PNG, VT, per-generation protocol and input evidence. Restart must exit
naturally, reuse the same isolated root and provider, and add **no** requests;
the restored tab click must reveal the durable transcript and regenerated
title. A failed predicate retains its diagnostic frame and failed status.
Full-grid and PNG comparisons remain separate: genuine elapsed-time digits,
versions and randomized Home examples cannot establish parity.

`--tabs vertical --columns 162` and `--columns 163` check both sides of the
42-cell rail-adjusted auto-sidebar breakpoint using text **and styled blank
backgrounds**. `--devtools unset` omits the explicit override; native debug builds
then show native diagnostics, while the packaged original defaults to hidden.
Explicit `true`/`false` overrides the default in either application. These are
honest effective-channel differences, not identical-state comparisons.

For the completed Reader/tools sidebar palette probe, run two **fresh** attempts
at 160×48 with `--geometry true --sample tools --columns 160 --rows 48
--sidebar-palette true --build-oc true` and both explicit binaries above: use
`--sidebar hide` for initially hidden and `--sidebar auto` for initially visible.
The runner asserts the initial state independently from painted `Context` and
the styled right-edge cell, sends real PTY Ctrl+P and types `sidebar`, captures
`sidebar-palette-search` styled cells/PNG/VT, and records the actual visible
`Show sidebar`, `Hide sidebar`, `Toggle sidebar`, or absence in each side's
`sidebar-palette-checks.json`. If exactly one action is visible it sends Return,
requires the sidebar state to invert, and captures `sidebar-palette-after`.
An absent action remains `ACTION_ABSENT`, without inventing a label or
pressing Return; unequal labels are reported as observations, not parity.
`capture.lock.json` records side-specific outcomes and ordinary grid/PNG
comparisons independently. An existing output path is never overwritten.

## R4/V06 same-session two-turn spacing probe

Use the pinned original and an **already built** native binary, with a fresh
output directory (this command does not run Cargo):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --two-turn true \
  --output /home/opencode/ai/oc/evidence/tui/two-turn-NEW-ATTEMPT
```

This test-only opt-in uses the same 120×40 Reader/tools isolated fake Responses
profile for both real PTYs. It submits the fixture question and then the distinct
`Second same-session spacing check?` **in the same live session**, without a tab
switch or restart. The bridge requires the second prompt on the second turn and
serves `GEOMETRY-TURN-TWO: tool read completed.` after a second genuine `read`
round-trip; the runner requires four valid completed transcript requests (two
per turn), one title request, and no extra or invalid requests per side.
`session-wide-completed` (after the first turn/title) and
`session-two-turn-completed` each retain complete `.cells.json`, `.png`, `.vt`,
`.render.json` and `.txt`. `two-turn-checks.json` records actual styled-cell
rows between the first assistant attribution and the next user's painted text,
including their full cells, the second user block's top row, answer→footer and
footer→user row counts, and request counts. The existing comparator produces
unmasked full-grid and PNG diff reports for **both** stages; a passed per-side
turn/spacing observation does not mean the whole frames match. Elapsed-time
digits, path/version differences, and any resulting scroll remain visible.
Existing output paths are refused; failed predicates retain diagnostic frames
and exit nonzero. The source/binary association of an existing native executable
is not attested without a separate build.

## VIS15 public reasoning click diagnostic

After rebuilding the native binary, use **both** explicit binaries and a new
immutable output directory (the runner refuses an existing attempt):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample reasoning --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --reasoning-click true \
  --output /home/opencode/ai/oc/evidence/tui/NEW-REASONING-ATTEMPT
```

This test-only mode excludes the other interaction, resize, variant and seeded
routes. The bridge's real Responses stream supplies public
`**Inspecting**\n\nPublic summary only.` plus an opaque encrypted marker that must
never paint; the visible answer is `GEOMETRY-SHORT: public reasoning completed.`
After the completed-session frame, each side's actual styled cells must show
exactly one `+ Thought: Inspecting` (pinned hide-mode group in
`opencode/packages/tui/src/routes/session/index.tsx:1780-1814`), with no public
body. The runner uses that side's cell coordinates to send a left-button SGR
press and release separately through its real PTY, waits for the unique
`- Thought` header and `Public summary only.` beneath it, then locates the
expanded header anew, clicks and requires the body to disappear. Duplicate or
overpainted headers/bodies fail rather than choosing a convenient match.
`reasoning-{collapsed,expanded,recollapsed}` each save full styled cells, PNG,
VT, render geometry and cursor. `reasoning-click-checks.json`, `inputs.json`
and `capture.lock.json` retain actual header styles/coordinates, click bytes,
typed predicates, provider request/completion counts and failures. The normal
unmasked full-grid and PNG comparators run for all paired stages; a successful
click does not assert whole-frame equality. A failed attempt remains available
at its original path; retry only under a fresh name.

To diagnose the pinned original's release-only reasoning handler against the
native PTY, add `--reasoning-release-only true` to the paired command above and
choose a different, fresh `--output` path. It requires `--reasoning-click true`
and the same Reader/reasoning 120×40 profile. This opt-in sends **only** SGR
left-button UP (`ESC[<0;column;rowm`) at each side's own uniquely painted
header for both collapsed → expanded and expanded → recollapsed; it sends no
mouse DOWN. The ordinary `--reasoning-click true` path still sends separate
press and release events. The same unique-header, public-body visibility,
opaque-content and unchanged completed transcript/title request checks apply
to every stage. `reasoning-click-checks.json` records the input mode, target
and actual release bytes; `inputs.json` records each PTY input. Captured stages
retain full immutable styled cells/PNG/VT/render data and the existing unmasked
whole-grid and PNG comparisons. If either side does not transition, its failed
predicate and `failure-diagnostic` frame remain in the attempt; no interaction
or VIS15 parity PASS is implied by the other side's successful transition.

For the independent **two adjacent public reasoning items** VIS15 probe, use
both real executables, a fresh attempt path, and the Reader 120×40 profile:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample reasoning-steps --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --reasoning-steps true \
  --output /home/opencode/ai/oc/evidence/tui/reasoning-steps-NEW-ATTEMPT
```

This test-only opt-in is exclusive with other interaction, resize, variant and
seed modes. The fixture serves one completed Responses output: two sequential
reasoning items with separate IDs and output indices, each with a public
summary part, delta, part.done and item.done, then a short assistant message and
response.completed. The first item is `**Inspecting**` /
`First public step marker.`; the second is `**Verifying**` /
`Second public step marker.`. Opaque
encrypted content is never an expected visible body. Each side must actually
issue one valid completed transcript and one valid completed title request,
without a tool round-trip or invalid request; request hashes and safe contract
facts are in `protocol.json`. The pinned original groups adjacent reasoning
parts into one completed `+ Thought: Verifying · 2 steps`; expansion paints
both distinct titles and bodies in order (pinned
`opencode/packages/tui/src/routes/session/grouping/session.ts` and
`opencode/packages/tui/src/routes/session/index.tsx`). The runner records the **actual**
per-side painted header, step number, styles/cells, marker positions, provider
counts and real one-based SGR click bytes in `reasoning-steps-checks.json` and
`inputs.json`. It captures `home`, `reasoning-steps-collapsed`,
`reasoning-steps-expanded` and `reasoning-steps-recollapsed` as full styled
grids, PNG, VT and render geometry, comparing every
available paired stage unmasked in grid and PNG modes. If the native groups
differently, its collapsed and `failure-diagnostic` frames and failed predicates
are kept; no missing expansion is manufactured or reported as PASS. A successful
per-side interaction alone does not establish whole-frame equality. This
command uses existing binaries and does not invoke Cargo.

## VIS42 clean-dialogue service transitions

Use both pinned executables, the existing Reader tools profile, and a fresh
immutable attempt directory:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --clean-services true \
  --output /home/opencode/ai/oc/evidence/tui/VIS42-NEW-ATTEMPT
```

This test-only exclusive mode creates isolated per-side Home/config/project
roots, an owned failing local plugin, native ignored legacy-compaction fields,
and one actual local Python MCP peer. Both configurations use the admitted
startup/catalog/execution timeout object. The first genuine read/result/answer
completes while MCP initializes; fixture controls release the real failed
initialize, then repair the owned configuration and release a healthy initialize
and catalog. No live model API, user configuration, or credential is used.
The native cannot execute the foreign plugin and reports its typed unsupported
inventory rather than loading JavaScript. Services mode intentionally uses its
own config file, not `OPENCODE_CONFIG_CONTENT`, so real `/reload` adopts repairs.

`services-checks.json` records actual pending, answer, brief failure notification,
MCP list/details/back, unchanged reload, recovery and second-answer stages.
`inputs.json`, `protocol.json`, and the owned peer's bounded PID/method/phase
snapshots retain the dispatch/effect witnesses. Reload waits for a **new actual
initializer**, not an ephemeral success toast that a later failure can replace.
Every shot briefly pauses the owned application process group through the
existing acknowledged pause/drain/resume bridge, records exactly the post-ACK
real full grid and its signature, and resumes in `finally`. This stabilizes
transient PNG capture without selecting a closest frame, inventing clock digits,
masking cells, cropping, or painting a component reference; cleanup resumes and
reaps an interrupted paused group as on the existing scanner route.
The stage predicate observes the first real matching frame before that freeze;
it does not wait for unrelated toast/caret clocks to settle while an initializer
deadline is running. The post-ACK predicate and complete before/after signature
checks still must hold. Per-shot wait/pause timings and peer timestamps diagnose
this interaction without raising startup, catalog, execution or capture bounds.

Native sanitized service identities/brief aggregate status, cause-sensitive
unchanged reload without re-alert, and the compact outside-message live-preview
indicator are declared before comparison. The short answer does **not** exercise
preview eviction; that remains a separate bounded behavioral proof. The pinned
original can re-alert the same MCP failure after pending. Actual elapsed digits,
wording, footer inventories and detail layout differences remain in the full
styled-cell/PNG/cursor comparisons. `OBSERVED` means both real transitions were
recorded, not pixel-parity PASS. Missing/failed/unstable stages and nonzero strict
comparator exits stay with the failed attempt; none are promoted to VIS42/V09
PASS or hidden by the native's successful functional checks.

## VIS16/VIS17 bounded Generic/MCP and Shell presentation

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true \
  --output /home/opencode/ai/oc/evidence/tui/TOOL-PREVIEW-NEW-IMMUTABLE-ATTEMPT
```

Allow a bounded 600-second runner timeout. This exclusive test-only mode uses
the existing bridge/frontend/comparator, isolated per-side roots, local fake
Responses and a real stdio MCP peer. No live generation or user configuration is
used. Four ordinary tool effects are two consecutive large MCP results, real
80-line Shell output and a genuine MCP `isError` result; one title plus five main
requests are validated. The authorized Shell also appends one fixed line to its
per-side isolated project counter; read-only snapshots require exactly one effect
and unchanged bytes/hash through view changes/restart, independently of provider
call counts. Both sides use 12 lines/1024 bytes output preparation.
Payload includes literal `[Part preview truncated]`,
`[output preview truncated; full result retained]` and `[truncated]`: their
presence is required where the retained body is expanded, not globally forbidden.

Before those effects, `home_mcp.mjs` exercises the live Home footer: click to the
existing `/mcps` modal, real disconnect, failed initialization and retry/recovery,
then width44/63/64/43 and short64×12 resize boundaries. The Unicode draft and
composer caret are restored; these controls must make zero provider requests.
The isolated peer's `fail`/`recover` phase file affects real `initialize` replies,
not renderer state, and its lifecycle record proves the genuine failure.
Failure-footer capture waits for the existing toast to expire naturally; it does
not mask the alert or alter its clock. Twenty-seven paired stages now include the
thirteen Home additions plus the fourteen original tool-presentation stages.
The short original footer overpaints prompt-footer spaces; this and the true
product-version identities remain in the full comparisons, not normalized away.

The primary ordinary profile remains unrefreshed DOM. For an independent mature
renderer control, ordinary tool-preview also admits `--cursor-renderer webgl`
using the same exact external test-only addon described below. Both binaries run
on that profile; WebGL's measured integer cell geometry differs from DOM's
fractional geometry, and its equality is not substituted for a DOM result.
`--refresh-before-capture true` is an explicit ordinary diagnostic only: the
profile and render record disclose a repaint of the unchanged VT buffer. It is
rejected with cursor-temporal qualification, where forced repaint would disturb
the measured blink state.

`readCaptureGeometry` records at most eighty actual DOM rows and each row's last
four child boxes/styles. At most four retained buffer cells beyond the viewport
are observed as color-mode/color/bold facts only, never glyphs or text. This
diagnoses xterm DOM painting of retained off-viewport styled tails after shrink;
it does not clear, extend or recolor either application's buffer. Current43/44
Home grids/cursors agree, while DOM's last fractional PNG column still differs.
The full unmasked DOM, refreshed diagnostic and WebGL results are retained in
`evidence/tui/home-geometry.md`; no edge crop, CSS fix or comparison mask is used.

The actual default collapse, hover, two MCP expand/recollapse sequences, failure
detail, Shell expand/resize/recollapse, `/new`/history reopen and clean same-root
restart produce fourteen further paired full styled-grid/PNG/VT/cursor stages. Hover and
clicks retain the real draft/caret at 120×80 before returning to 120×40. Ordinary
expansion must not reveal the discarded distant-end marker, display generated
model guidance as body, execute a tool again or change recorded operation facts.
The read-only audit checks bounded native presentation events and identical tool
records, capture descriptors/artifact hashes and MCP call records through replay.

Two additional **native-only** full frames exercise the existing `/cards` detail
owner at 120×20: visible typed preview/capture/reference facts, then an explicit
240-byte next page after viewing all preceding rows. This viewer may read a
registered capture or recorded RAW through the existing authorized owner; it is
not ordinary Generic/MCP expansion or a promise of full-result availability.
The resource-detail frames have no donor `/cards` counterpart and are labelled
`NATIVE_ONLY_RESOURCE_DETAILS`, not a paired visual PASS. Their reference remains
a recorded fact, not a readability guarantee.

`tool-preview-checks.json`, `protocol.json`, `inputs.json`, snapshots and
`capture.lock.json` retain actual predicates, cursor/counts and source/binary
association. Native MCP wire names, safe failure normalization, bounded 2048-byte
presentation, compact viewing status and capture details are disclosed; elapsed
digits, redacted fixture data and all other differences remain unmasked. Existing
redaction is never bypassed to force fixture text equality. `PASS_BEHAVIOR_ONLY`
does not qualify VIS16/VIS17 pixels, prompt blink, all history paging/capture
faults, release behavior or T44/V09. Missing/failed attempts stay diagnostic.

## VIS40 mixed MCP modal status

`--tool-preview true --mcp-status true` exclusively selects a zero-model diagnostic
instead of the ordinary Home/body or temporal sequence. It configures three real
MCP entries: healthy stdio, disabled (not started), and genuine failed initialize.
The original's canonical disabled field and native admitted field are distinct;
no renderer state, SQL row or result is seeded. Real SGR Home clicks, arrows and
resize select every status at120×40,80×24 and160×48. Every selection must show the
exact safe configured names `visdisabled`, `visfailed`, `vishealthy`, distinct from
the handshake name/argv and known inherited protected values. Real search for
`visfailed` and Enter open a dedicated read-only detail (no Search/hardware caret).
Arrow/Page/Home/End input checks the short available body, not overflow. Keyboard
and mouse Copy each require actual OSC52 delivery and copied feedback; mouse Back
restores the originating filter before exact Unicode draft/caret restoration.
Explicit keyboard and mouse investigation produce an unsent diagnostic draft in
native, never an automatic model/tool action. Eighteen full paired frames are saved
unmasked; the ordinary tool-effect fixture retains its original server key.

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

`mcp_status.mjs` observes typed-result styled cells: unselected semantic tones,
selected action fg/bg override, intrinsic Connected bold separately. Native must
retain frozen source-required bold; the running original's absent bold is recorded,
not treated as native failure or equality. Zero provider/tool/MCP-call/Shell effects
and real healthy/failed initialization receipts are required. Copy decodes the real
OSC52 wire, not a toast; it does not prove desktop/SSH terminal clipboard acceptance
(`NOT_VERIFIED_BY_PTY`). Native uses the safe owner diagnostic, not the peer's raw
exception. The pinned Home route can close a repeated identical investigation
after user clear without reinjecting the draft; the full observed reference frame
and false reference draft field are retained, never imitated or accepted in native.
This bounded slice does not qualify every pending/auth/control/footer interaction
or whole VIS40 outcome. Unsafe
label protection and activation/refusal are separately checked at the existing
config/runtime/application and actual-binary owners; the paired probe does not
seed masked rows or read inactive credential files. The current36 full-grid/PNG
comparisons remain DIFFERENT; `PASS_BEHAVIOR_ONLY` is
not pixel PASS. Separate ordinary and temporal runs qualify their regressions.

## VIS40 Select MCP footer focus and effective remaps

Add `--mcp-footer default|remap` to the exclusive ordinary
`--tool-preview true --mcp-status true` mode. This selects `mcp_footer.mjs` instead
of the status/details sequence. Real three-peer inventory and initializer role/PID
receipts prove focused-submit disconnect, footer-mouse reconnect, failed retry,
explicit disabled activation and park. Initial healthy/disabled/failed counts are
1/0/1, final2/1/2; disabled never starts before the explicit action. All stages have
zero provider requests, tool calls, MCP tools/call and Shell effect counters.

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true --mcp-status true --mcp-footer remap \
  --output /home/opencode/ai/oc/evidence/tui/MCP-FOOTER-NEW-IMMUTABLE-ATTEMPT
```

Tab/Shift+Tab focus, title-only action bold, selected-row muted/raised treatment,
row-hover unfocus and unchanged Search caret are captured at80×24,120×40,160×48.
Real filtering/no-match submit, prev/next/End/Page±10/Home and exact Unicode composer
restoration are required. Remap fixture admits F2/F3, Alt+U/D/H/E, F4 submit and
F6/`<leader>t` toggle with Ctrl+G leader; native dispatch and hints share the effective
projection. Old Down/Enter/Ctrl+P remain inert. Default additionally checks Ctrl+P/N
wrap. Page movement records actual selected indexes and requires native source
boundary-wrap policy, never modulo/clamp or a normalized reference frame.

Final default27/remap26 paired stages yield54/52 strict full-grid/PNG comparisons,
all DIFFERENT. The full frames, cursor, real controls and source-associated build
are retained; no crop/mask/version substitution or renderer-state seeding. Ordinary
body/reopen and temporal blink regressions run separately. Receipt:
`evidence/tui/mcp-select-footer.md`. `PASS_BEHAVIOR_ONLY` is not whole VIS40/T44 or
pixel PASS; pending/auth/security/fault and remaining frozen outcomes retain their
separate mandatory acceptance.

## VIS12 ordinary prompt mouse click-caret

`--tool-preview true --prompt-caret true` exclusively selects `prompt_caret.mjs`
instead of MCP/status/body/temporal probes. Both actual isolated binaries start
without MCP entries. Real SGR Left Down/Up and typed `X` prove Home and session
insertion, second wide cell, combining/ZWJ start, trailing blank, wrap gap, blank
line,80-column resize, chip neighbor/expansion priority and modal backdrop ownership.
Hover does not own the caret. Twenty-five full paired stages stay unmasked.

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true --prompt-caret true \
  --output /home/opencode/ai/oc/evidence/tui/PROMPT-CARET-NEW-IMMUTABLE-ATTEMPT
```

There are zero model/tool effects until one explicit Enter. The fixture accepts
exactly `Проведи RECXON, жду план` as the sole user request, plus one title request.
Native SQLite is read-only audited for that exact user row before/after real
`/new`, saved-session reopen and clean same-root restart. The original's distinct
RAW schema is not decoded; its real wire and saved-dialog/restart frames are
separate evidence. No tool/MCP/Shell calls occur. Full emoji bytes are tested at
the native editor owner: the common Unicode11 raster may lose the second ZWJ
component, so the paired probe verifies insertion prefixes and retains the actual
unmodified full raster, not a normalized emoji. Current50 whole-grid/PNG
comparisons are DIFFERENT. Receipt: `evidence/tui/prompt-click-caret.md`.
This does not qualify shared durable newest-50 input history or whole VIS12/T44;
ordinary tool-output and temporal blink regressions run separately.

## VIS12 shared durable input history

`--tool-preview true --prompt-history default|remap` exclusively selects
`prompt_history.mjs`/`prompt_history_fixture.py`, without MCP or tool execution.
Actual accepted First/Second/Third inputs include multiline, Unicode and literal
`@note.txt`; native read-only SQL checks the shared global list and unchanged user
rows. Real arrows/visual boundaries/edited refusal, new-Home add-tab, fourth exact
recalled fresh-session request, clean same-root restart and local `/mcps` admission
are captured without editor-state seeding or RAW writes. The canary file body must
not enter any request. Four main requests plus two titles occur; navigation has
no provider effect before explicit Enter. The existing `pty_t39` target separately
qualifies active held-stream ownership and three-input durable acceptance.

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 80 --rows 24 \
  --tool-preview true --prompt-history default \
  --output /home/opencode/ai/oc/evidence/tui/HISTORY-NEW-IMMUTABLE-ATTEMPT
```

Run `remap` separately at120×40: admitted F2/F3 and registered leader Ctrl+G p
replace default history arrows, while physical visual movement still wins first.
Current default25 paired stages/50 full grid+PNG comparisons and remap28/56 are
ALL DIFFERENT, not pixel PASS. Original refuses an arbitrary nonempty scratch
draft and forwards to empty; actual refusal and reference-only real clear are
recorded. Native must preserve its declared unfinished-draft return instead.
No original RAW/native-SQL claim is made. Normal tool and temporal regressions
remain separate. Receipt: `evidence/tui/prompt-input-history.md`; structured user
Shell admission/full parts/Mini and remaining VIS12/T44 outcomes are not inferred
from this text-history proof.

## VIS12 structured user-Shell follow-up

The exclusive `--tool-preview true --user-shell true` episode admits80×24 or120×40.
Use the history command above with `--user-shell true` instead of `--prompt-history`
and a fresh immutable output. `user_shell.mjs` exercises actual `!`/Esc/exact Shell
Enter/recall/explicit second Enter/relaunch, not model tools. Its ordinary process
reads only owned native SQLite URI modeRO BEFORE effect, asserting committed exact
history, started NULL-turn intent and zero model turns; each side has exactly2
explicit effects, zero Responses/title/MCP and no restart replay. Original uses
actual `session.shell`; no original RAW decoding. Native text-only recalled mode
differs from original; full parts/mode/Mini are explicitly outside frozen VIS12.
Historical admission pairs have11 stages/ALL22 strict grid+PNG differences;
receipt `evidence/tui/user-shell-admission.md`. Historical typed presentation pairs
have12 stages/ALL24 differences, adding a genuine bounded running phase before
effect and checking exactly one/two typed command/output blocks after explicit
Enter, without RAW admission/notices or a generated successful-exit line. Use
`--build-oc true` for current-source association; receipt
`evidence/tui/user-shell-presentation.md`. This is behavior qualification, not
whole pixel/T44 PASS. Historical tab-title/padding pairs00280×24/003120×40 retain12
stages each: completed, recalled-normal and second-completed are exact full grid
AND PNG matches (6 EQUAL/18 DIFFERENT comparisons per geometry). Tab-only
`New session` survives title-less refresh/restart; real durable titles win.
Running/composer/Home differences remain unmasked. Receipt:
`evidence/tui/user-shell-tab-title.md`. Normal tool and temporal regressions remain
independent; this does not mark the whole Shell episode or T44 pixel PASS.

Historical live-footer pairs00680×24/007120×40 add the real `↓ 1 shell` inventory
indicator, retire it after completion, and retain12 stages each. Running,
completed, recalled-normal and second-completed match full grids AND PNGs/cursor
(8 EQUAL/16 DIFFERENT comparisons per geometry). The finite genuine process hold
is six seconds for the first command and three later, leaving time for the actual
running screenshot and read-only pre-effect witness; it is not a product timer.
Zero output during this hold does not qualify live streamed stdout. Receipt:
`evidence/tui/user-shell-live-footer.md`; remaining Home/mode/input/restart and
combined lower-composer differences stay open and unmasked.

Historical Shell-mode pairs00480×24/005120×40 preserve12 stages and add the
mode-specific `esc exit shell mode` footer and bounded Home example. Session
Shell prompts have no placeholder. Five stage pairs (running, completed,
recalled-normal, explicitly entered recalled-mode, second-completed) match full
grids/PNGs/cursor:10 EQUAL/14 DIFFERENT per geometry. Typed-input frames retain six
actual-version cells; empty mode also retains independently selected examples.
Neither real versions nor random selections are forced/masked. Receipt:
`evidence/tui/user-shell-mode.md`. This does not expand the frozen recalled
parts/mode/Mini exclusion or qualify live stdout/combined VIS39/full T44 parity.

Current live-output pairs00180×24/002120×40 preserve12 stages and five exact full
grid/PNG/cursor pairs (10 EQUAL/14 DIFFERENT per geometry), now including real
partial stdout AND stderr while the owned process is still running before effect.
The ordinary first command prints stdout, then stderr after0.2s, and holds nine
seconds/three later for genuine screenshot/read-only-witness margin. Both
executors, two explicit effects, zero Responses/title/MCP and restart-no-replay
checks remain real. The capture owner's bounded typed projection is not parsed
from transport labels; model tools and RAW remain unchanged. Receipt:
`evidence/tui/user-shell-live-output.md`. Remaining version/example/Home/restart
differences stay visible; combined VIS39 and full T44 are not qualified by this.

## VIS16/VIS31 real caret ordering and temporal blink

The exclusive `--tool-preview true --cursor-temporal blink|steady|default` mode
uses the same bounded real MCP/Shell effects, then measures six real input states:
composer idle, continuous hover, restored composer, command Search idle, continuous
Search hover, and exact composer restoration. It does not run the ordinary mode's
fourteen reopen/restart stages. A separate ordinary tool-preview run qualifies
those stages and read-only `/cards` cursor ownership after the terminal change.

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true --cursor-temporal blink --cursor-renderer webgl \
  --cursor-case expanded \
  --output /home/opencode/ai/oc/evidence/tui/CURSOR-NEW-IMMUTABLE-ATTEMPT
```

Run each profile separately with a bounded 600-second runner timeout. Admitted
geometries are 80×24, 120×40 and 160×48; `--cursor-case collapsed|expanded` selects
the real card state. `--cursor-sync unsupported` simulates an unsupported frontend
by consuming only private mode 2026 in the mature parser, leaving every other
command intact. This is test-only capability simulation, not output rewriting in
the observer or a product fallback timer. Native always emits the advisory envelope
and retains safe Hide/MoveTo/Show ordering without it. The original fixture uses
its admitted explicit cursor style/blinking configuration; default retains its
terminal-default configuration. Native inherits the actual frontend defaults.

WebGL uses exact test-only `@xterm/addon-webgl` 0.19.0 installed independently under
`/home/opencode/.cache/opencode-tmp/opencode/cursor-renderer`. It is not a native
runtime dependency or a modification of the existing reference installation.
The runner checks the version, hashes the loaded bundle and external package lock,
and copies that lock into the immutable receipt. DOM remains available as a
diagnostic control: observed hover blink starvation is not accepted as PASS.

`cursor_frontend.js` observes the actual mature parser after each original command
and samples the real fixed input-owner raster. It never rewrites commands, patches
CSS phase or paints a substitute cursor. Independent 20-ms SGR hover injection
continues while each 5.6-second state collects at most 1,200 raster samples,
50,000 command observations and twelve full opaque renderer-canvas PNG/grid/cursor
snapshots. Temporal pictures are timestamped, explicitly unsettled actual renderer
frames, not settled/atomic screenshot claims. Blink requires at least three full
raster cycles, cadence relative to the same owner's measured idle profile and
maximum sampling gap no larger than one quarter of its idle cycle. Steady/default
controls require zero observed cycles. Draft, final caret and no tool replay are
checked independently of raster phase.

```sh
python3 scripts/tui_capture/check_cursor_temporal.py \
  --compare-output evidence/tui/NEW-IMMUTABLE-CURSOR-COMPARISONS \
  evidence/tui/cursor-temporal-attempt-018 \
  evidence/tui/cursor-temporal-attempt-019 \
  evidence/tui/cursor-temporal-attempt-020 \
  evidence/tui/cursor-temporal-attempt-021
```

The audit validates rebuilt Rust input identity, every full PNG's dimensions,
opacity and recorded fixed-owner raw pixel, then compares full styled grids/PNGs
at observed matching visible/hidden phases. It does not normalize images, crop,
mask, rewrite immutable metadata or infer equality from unmatched phase counts.
`QUALIFIED_CURSOR_BEHAVIOR_ONLY` is not VIS16/VIS31/T44 visual PASS: the current
matched full-frame comparisons all remain DIFFERENT. Failed attempts and genuine
DOM/frontend observation limits remain recorded in the factual receipt.

`--startup-error true` with only `--oc` captures a real malformed-config native
preflight error. For supported child routes and real Location query failures,
`OC_V03_CAPTURE_OUTPUT=/absolute/fresh-attempt-prefix cargo test --locked -p oc
--test recovery_v03 -- --nocapture` optionally seeds native storage through its
storage API, imports original parent/child sessions using the original supported
`session import --standalone` command, and saves `-child` and `-query` attempts.
The normal Cargo test has no Node/browser dependency. These modes preserve raw
VT, styled grids, commands/import results, failures and nonzero comparison exits.
The native in-process startup error is not the original service-attach route.
Original child composer/root-tab behavior also remains different. No near-limit
PTY paste or editor/paste-chip equivalence is claimed by V03; that is V05 work.

## VIS25 slash-autocomplete diagnostic

Run with both pinned executables and a **new** immutable output directory:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --autocomplete true \
  --output /home/opencode/ai/oc/evidence/tui/NEW-AUTOCOMPLETE-ATTEMPT
```

The opt-in sends `/`, `ren`, and **Tab only** through each real PTY on Home,
clears the resulting draft with Backspace, completes a real fixture-backed
read turn, and repeats the three keystrokes in the session. It never sends
Return to submit the slash draft. Home's available commands may differ from
session commands: Tab can execute the actual highlighted argument-free action
(the pinned original selects `/reload` on Home with this query). This is
recorded, not replaced with a synthetic `/rename` completion. Each stage
saves full styled cells, PNG, VT, cursor, actual menu rows and draft, provider
request counts, `autocomplete-checks.json` predicates and normal unmasked
grid/PNG comparator results. The typed-query predicate applies only before
Tab; after Tab the runner records the actual draft, caret and reload notice
without expecting either outcome.
Comparator inequality still exits 1 and leaves VIS25 unverified. Failed
attempts, including runner-predicate failures, remain separate immutable paths.

For a separate **keyboard** probe, use a fresh path and replace
`--autocomplete true` with `--autocomplete-keys true` (exclusive with
`--autocomplete true` and `--mention true`):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --autocomplete-keys true \
  --output /home/opencode/ai/oc/evidence/tui/NEW-AUTOCOMPLETE-KEYS-ATTEMPT
```

On Home and again after the fixture-backed read/title completes, this opt-in
types `/reload`, captures the actual suggestion overlay, sends Enter, and
requires a visible `Configuration reloaded` success, an empty draft and no
provider request. After capturing this success frame, it waits for the actual
five-second reload toast to disappear with the draft empty, menu hidden and no
provider request, recording `reload-notice-expired` predicates for both routes.
Only then does it type `/ren`, capture the overlay, send Escape,
require the `/ren` draft to remain with the suggestion menu hidden, and clear
the draft with Backspace before submitting the fixture or leaving the session.
Add `--autocomplete-keys-rename true` to also type `/rename` in the completed
session, capture before/after Enter, check the inserted `/rename ` by its
cursor position and no new provider request, and clear that draft. The optional
flag requires `--autocomplete-keys true`.

For the paired selection-movement probe, add `--autocomplete-keys-move true`
to that command (also works alongside `--autocomplete-keys-rename true`) and
choose another fresh output directory. After typing `/ren` on **each** route,
`before-esc` is the captured initial selection; the runner requires more than
one painted option and a distinct styled highlight on the first row. It sends
Up, Ctrl+P, Down, Ctrl+N individually through each real PTY (`ESC [ A`, `0x10`,
`ESC [ B`, `0x0e`; pinned `packages/tui/src/config/keybind.ts:270-271`).
`movement-up`, `movement-ctrl-p`, `movement-down`, `movement-ctrl-n` each save
full styled cells/PNG/VT. The runner samples the actual slash-label cell's
background on every option row, identifies the highlighted option by the
initial focused background, and asserts wrap from first to last and last to
first, as well as each intermediate move. It requires unchanged `/ren` draft,
option inventory and provider request/completion counts. `autocomplete-keys-checks.json`
records per-side labels, sampled foreground/background, selected option/index,
expected index and predicates; `capture.lock.json` records outcomes. The
ordinary comparator checks **all** paired movement frames in grid and PNG modes,
without masking differences. Escape and optional rename still run afterward.
Styled selection can pass on both sides while whole-frame VIS25 parity remains
DIFFERENT; a missing or ambiguous highlight fails the interaction.

Each `autocomplete-keys-{home,session}-{before-enter,after-enter,before-esc,after-esc}`
frame (plus optional rename frames) retains its full styled grid, PNG, VT and
ordinary whole-frame grid/PNG comparison. `autocomplete-keys-checks.json` and
`capture.lock.json` record actual per-side drafts, menu rows, cursor, baseline
and provider counts, predicates and result. Home and session menu inventories
are observations, not asserted equal; neither frames nor upstream inputs are
masked or synthesized. A failed predicate is a failed diagnostic, and a passed
interaction does not claim full-frame parity.

For the independent R5/V05 **Ctrl+C** probe, use a fresh output directory and
the same paired Reader/tools 120×40 profile, replacing `--autocomplete-keys true`
with `--ctrl-c true` (exclusive with other interaction/resize/variant modes):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --ctrl-c true \
  --output /home/opencode/ai/oc/evidence/tui/ctrl-c-NEW-ATTEMPT
```

Each real PTY receives a visible literal draft prefix and a bracketed three-line
paste on Home; `ctrl-c-home-draft-before` and `ctrl-c-home-root-cleared` capture
the painted prefix/chip before Ctrl+C and the empty prompt after it. Only then
does the runner submit the normal fixture prompt for the real read/title session.
On that completed session it repeats the multiline draft, opens Commands with
Ctrl+P, types a query and sends Ctrl+C to the focused modal. The resulting
query-clear **or** dismissal must preserve the root draft; if still open, Escape
closes the modal. A second root Ctrl+C clears the draft without terminating the
PTY; a third, on the empty root prompt, must produce the bridge's natural exit
event with code 0 and actual post-key VT alternate-screen-leave/cursor-show
bytes. The probe never presses Enter on its draft or modal query.
`ctrl-c-{home,session}-draft-before`,
`ctrl-c-session-modal-query-before`, `ctrl-c-session-modal-after` and
`ctrl-c-{home,session}-root-cleared` retain full unmasked styled cells/PNG/VT;
`ctrl-c-checks.json`, `inputs.json` and `protocol.json` retain actual input bytes,
observed modal outcome, provider baseline/current counts and natural exit event.
Each paired frame goes through the existing unmasked full-grid and PNG comparators;
their exit 1 remains DIFFERENT even when both interaction probes pass. Failed
attempt directories remain immutable. Supply a rebuilt native binary to test
native changes; this invocation does not run Cargo.

## VIS26 file-mention diagnostic

Once the native binary has been rebuilt with VIS26, run both pinned executables
with a **new** immutable output directory (without `--autocomplete true`):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --mention true \
  --output /home/opencode/ai/oc/evidence/tui/mention-20260924-01
```

If that output path already exists, choose the next unused suffix; the runner
refuses to overwrite an attempt. The bridge seeds `fixture-note.txt` in the
isolated Location for the tools sample. On each side, the probe types `@`, then
`fixture`, then **Tab only** on Home, clears the draft with Backspace before
the normal fixture-backed turn, and repeats/clears the probe in the completed
session. It never submits the mention draft. `mention-checks.json` records the
actual prompt, cursor, nearby painted file rows, fixture suggestion, inserted
relative mention (if any), provider counts, and stage predicates independently
for original and native. A provider request during the probe fails that side.

Every trigger/filtered/after-Tab state saves full styled cells, PNG and VT.
`capture.lock.json` retains per-side outcomes and the existing **whole-frame**
grid/PNG comparator exit codes. Exit 1 means differing frames or failed
predicates, not parity. Record an executed run and its actual comparator results
in `evidence/tui/mention-report.md`; do not write capture evidence or a PASS
claim for a binary that predates VIS26.

## VIS27 paired mouse selection/copy diagnostic

Use an existing **rebuilt** native binary and the pinned original, with a fresh
output path (do not pass `--build-oc true` unless you explicitly want the runner
to build it):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --selection-copy true \
  --output /home/opencode/ai/oc/evidence/tui/selection-copy-NEW-ATTEMPT
```

`--selection-copy true` is test-only and requires exactly this paired Reader/tools
120×40 completed-session profile without other interaction, variant or resize
modes. Each side locates its own **unique painted** `GEOMETRY` in the real tool
answer, compares the word's styled cells against the completed baseline, and
sends two then three SGR left-button press/release pairs at an interior word
cell via that side's PTY. There is **no motion event**. The runner observes the
styled highlight and the actual `Copied to clipboard` toast in the VT grid,
captures `selection-double` and `selection-triple` (full unmasked cells, PNG,
VT and render geometry), and waits for observable toast disappearance before
the second gesture. A triple-click also requires styled highlight outside the
word on the same line. `selection-copy-checks.json`, `inputs.json` and
`capture.lock.json` retain coordinates, exact mouse bytes (base64), observed
cell styles, predicate outcomes, provider counts and whether an OSC 52 **prefix**
was seen in PTY output. The prefix count does not decode payloads or prove a
system clipboard write. The existing raw VT evidence is the unmodified PTY
stream; use the fixture-only isolated root for this probe, not private content.
No system clipboard is read or asserted: terminal OSC 52 transport, xterm's
clipboard handling, and the desktop/system clipboard are separate boundaries.

The original OpenTUI copy-on-select handler requires `isDragging` on mouse
release (`opencode/packages/tui/src/util/selection.ts`). Bare SGR clicks can
select without producing that event; a missing toast/highlight is recorded as
`FAILED_OBSERVATION`, with its actual diagnostic frame, never substituted with
a synthetic success. Both sides still run, and the normal **whole-grid and PNG**
comparators run for both stages. Any differing frame or failed predicate keeps
exit code 1; interaction feedback does not imply pixel parity. Existing output
paths are refused, including failed attempts.

For the **121×40 visible-sidebar toast overlap** diagnostic, use a separate fresh
output directory and replace `--sidebar hide --columns 120 --selection-copy true`
above with `--sidebar auto --columns 121 --toast-overlap true`. Keep the paired
Reader/tools profile, `--rows 40 --geometry true --sample tools` and
`--agent-profile true`, and both explicit binaries. This mode rejects
`--build-oc true` and other interaction/resize/seed modes; it uses an existing
binary without invoking Cargo. Each real PTY completes the read and title
fixture (two transcript requests and one title request), then sends Ctrl+R,
replaces the prefilled title through the real Rename session dialog with a long
`OVERLAPTITLE-` title and confirms with Return.

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar auto --agent-profile true \
  --columns 121 --rows 40 --toast-overlap true \
  --output /home/opencode/ai/oc/evidence/tui/toast-overlap-NEW-ATTEMPT
```

The title must appear in the painted sidebar and on the toast's future
bottom-padding row. The runner saves
`toast-overlap-before` (full styled cells/PNG/VT/render) and finds that side's
unique painted `GEOMETRY` in the transcript for two SGR left-button down/up
pairs without pointer movement. It saves `toast-overlap-toast` after observing
the actual `Copied to clipboard` feedback, selection styling and an OSC 52
introducer. `toast-overlap-checks.json` records the toast's observed border
rectangle, the **underlying title cells inside that exact rectangle**, the
painted bottom-padding cells, predicates that padding contains only raised-bg
spaces, mouse bytes and unchanged provider counts. Failure remains a failed
observation, with the actual frames retained. The normal comparator checks
the **entire** paired grid and PNG for both stages without masks; per-side
overlap predicates do not imply frame equality. `capture.lock.json` records
both outcomes and comparator exits. The OSC 52 prefix and toast only establish
the PTY transport attempt and on-screen feedback; the external/system clipboard
destination is **NOT_VERIFIED_BY_PTY** and needs a separate destination test.
Use a rebuilt native binary for claims about a source fix; an existing binary
has no attested source association in this run.

## VIS28 paired temporal running-footer diagnostic

Run the pinned original and an **already built** native binary with a fresh
immutable output path for each setting (do not pass `--build-oc true`):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --scanner true --scanner-animation true --scanner-cancel false \
  --output /home/opencode/ai/oc/evidence/tui/scanner-animated-NEW-ATTEMPT

node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --scanner true --scanner-animation false --scanner-cancel false \
  --output /home/opencode/ai/oc/evidence/tui/scanner-fallback-NEW-ATTEMPT
```

`--scanner true` requires explicit `--scanner-animation true|false` and
`--scanner-cancel true|false`. For interruption, use either command above with
`--scanner-cancel true` and a **new** output path, e.g.
`scanner-animated-cancel-NEW-ATTEMPT` or `scanner-fallback-cancel-NEW-ATTEMPT`.
The mode requires the
paired Reader/tools 120×40 profile above, without other interaction, matrix,
resize or seed modes. Both isolated configs receive the same explicit animation
value (original CLI config and native product config). Ordinary modes retain
their previous animation settings and fast-completing fixture. In this mode the
bridge streams a real `response.created` event on the **first** transcript
request, holds the remainder of that HTTP Responses stream open for at most
60 seconds, and records the held/release/resume events. The runner releases it
explicitly even if observation fails; the genuine read-tool roundtrip and title
then finish when cancel is false. For cancel true, the runner sends real PTY
Escape **while the first stream is still held**. It verifies the actual painted
`esc interrupt` hint on each side before sending input (and its painted
`esc again to interrupt` prompt after an upstream Escape). The pinned original
uses an interrupt counter and a five-second reset window; the runner allows
up to three Escapes within that window, based on observed running state.
The native may cancel after one. After each Escape the
runner checks actual frames for the indicator disappearing while the HTTP
stream remains held, records every input/check and raw provider counts, and
sends no further Escape once canceled (an idle Escape could exit the app).
If still running after three Escapes it fails and releases the stream. After the
footer disappears it pauses the owned PTY child for the screenshot, so an
independently animating tab cannot make the interrupted full-grid capture unstable.
It captures
`scanner-interrupted` with full grid/PNG, checks the painted indicator disappears, the submitted
prompt remains visible, and no read completion or second transcript request occurs. After
releasing the held handler it checks the HTTP handler terminates (completed
write or disconnect is recorded), the answer remains absent and no indicator
reappears. Cancellation does not require completed read/title counts. A bridge
stop also releases the handler. This is a bounded fake
provider, not a permanently stalled server or synthetic terminal frame.

Each side independently finds its painted `esc interrupt` hint and reads the
adjacent eight VT styled cells. Running polls read only the real xterm footer
rows (including all eight styled indicator cells); each paused screenshot still
records the full unmasked styled grid. The animated route observes changing block
symbols and foreground colors, detects forward travel, end fading (the brief
all-dot interval may fall between browser samples), reverse travel by its
opposite color gradient, and start fading in that **observed order**;
it never selects a frame by elapsed wall-clock time. Upon each candidate the
runner explicitly asks the bridge to SIGSTOP the owned child process group,
waits for an ACK (after draining prior PTY output), and resamples the painted
phase. ACK latency may advance the exact frame: an independently classified
paused frame is captured only if it still belongs to the requested stage.
The paused frame's own exact styled signature and complete grid must remain
unchanged through the PNG. Other phases are recorded as failed pause attempts;
the runner continues across observed cycles in stage order and always requests SIGCONT in a finally
block, and the bridge resumes before wait/termination even on stop/EOF/errors.
The HTTP fixture keeps running independently of PTY suspension; neither side's
TUI state or fixture is fabricated. It retains timestamped running samples,
pause/ACK/resample/restore observations (including full indicator cells), stage
predicates and provider counts in `scanner-checks.json`, plus `scanner-forward`, `scanner-end-hold`,
`scanner-reverse`, `scanner-start-hold` and `scanner-completed` full styled grids,
PNG/render geometry, PTY bytes and input/protocol records. The animation-off
route requires a painted `[⋯]` beside the hint and captures `scanner-fallback`
and `scanner-completed`. The footer must disappear after the explicit release
and completed read. Screenshot capture checks the paused indicator and *whole
styled grid* before/after PNG; a changed grid is `UNSTABLE_CAPTURE`, never
silently equated. The scanner probe has a 35-second observation window (8
seconds for animation-off), then releases the stream. A provider-side 60-second
timeout bounds interruption of the runner. Sparse browser samples can miss
transitions; end fading must have a changed lead color or uniformly colored
all-dot row, while start fading requires two distinct observed all-dot colors
after reverse. A uniform all-dot row by itself cannot label either hold.
For animation-on stages, the original's **paused** eight styled cells establish
each reference glyph pattern and colors. Native pauses only candidates with
the same glyph pattern. A non-exact candidate is saved under a unique immutable
`scanner-<stage>-candidate-N` name, including its entire unmasked grid/PNG;
the first exact styled-cell match is captured directly as canonical
`scanner-<stage>` on the paused PTY, with the same scenario field as the reference.
Existing canonical artifacts are never overwritten, and completed candidate files
are never renamed or edited to pass comparison. Pause attempts retain capture name
and phase match metadata. Within the existing probe deadline the closest RGB
candidate is selected if no exact match appears; an exact match wins immediately.
`scanner-checks.json` records the
reference cells, native candidate cells, RGB distance and selected scenario;
`capture.lock.json` links each stage comparison to that selected file. Different
glyph patterns cannot be called the same phase: if none is obtained the stage
is `UNMATCHED_PHASE`, with no stage comparator or PASS. A nearest-color frame
with different styled cells also reports `UNMATCHED_PHASE` rather than PASS;
its full-grid and PNG diagnostic comparators still run and record their own
results. The same rule applies to the fallback `[⋯]` cells.
Missing stages and timeouts are failures retained in the attempt,
not substituted by elapsed-time matching.

The existing comparator runs **both full unmasked styled-grid and PNG** diffs
for every matched stage present on both sides; compare matched glyph candidates,
not only phase names or unsynchronized wall clocks of sequential PTYs. Real elapsed/status digits and
all other cells are included. Passing the per-side motion/fallback predicates
does not establish pixel parity: inspect `capture.lock.json` comparator exit
codes and stage diff reports separately. Independently animated tab cells at y=0
remain in the full grid and a mismatch there is `DIFFERENT`, even when the footer
matches exactly. Existing output directories are
rejected, even after failure. Animation-off alone cannot qualify VIS28.

## VIS09 actual Models navigation and selected-model wire check

Run against the pinned v2.0.12 executable and a source-built native
binary, with a new immutable attempt directory (120×40; `--columns 160
--rows 48` is also supported):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample tools --sidebar hide --agent-profile true \
  --columns 120 --rows 40 --models-interaction true \
  --output /home/opencode/ai/oc/evidence/tui/models-NEW-ATTEMPT
```

This opt-in excludes other interaction/variant/resize/seed modes. Both sides
receive the same eight normal models plus twelve
configured `ZZ Scroll 00`–`ZZ Scroll 11` entries, only in this mode. The
fixture's Responses server checks the actual first model for the first real
`read` round-trip and the selected `fixture-scroll-11` model for **both**
second-turn requests, including the returned read content. There is no named
variant in this catalog; the request-key observation is recorded separately.
The normal fixture and existing capture modes keep their original catalog.

After one completed Reader read and title, the runner opens the real Models
dialog with Ctrl+X, `m`, captures its first styled grid and PNG, records the
current-model marker and the independently styled initial focus, then sends
16 real Down keys. It requires a newly configured model to appear and the
first option to scroll away before capturing `models-scrolled`. It types
`ZZ Scroll 11`, captures the query and painted matching option as
`models-filtered`, presses Return, captures the resulting same-session
composer as `models-selected`, and submits `Second same-session model check?`.
Whether the current-model marker remains visible after scrolling is recorded
per side and compared as a scroll-policy observation, not a prerequisite for
filtering or selection. A visible marker must still label the original model; the
filtered target must be distinct from the current-model marker even if that
marker remains painted elsewhere in the dialog.
`models-second-turn` requires the original answer, new answer, retained tab
and raw valid completed provider requests on the selected model. Every
stage is saved as complete `.cells.json`, `.png`, `.vt`, `.render.json`, `.txt`;
`models-interaction-checks.json`, `inputs.json`, `protocol.json` and
`capture.lock.json` record painted rows/styles, request hashes/model IDs,
actual inputs, predicates and failures. Existing whole-frame grid and PNG
comparators run unmasked on each paired capture and exit 1 on differences.
`View all integrations` is recorded as the upstream action when painted;
no integration action is fabricated. Per-side interaction PASS does not
assert VIS09 parity, nor does an existing binary attest its source commit.

## V04 variant and search follow-up

`--variants true --sample short` adds the explicit fixture in
`tui-recovery/fixtures/variant-dialog.json` to the active model using each
application's native config schema, then captures `/variants` after the model
dialog. Default and declared `none` must both appear. This does not submit a
variant-bearing turn; the Rust raw PTY integration test qualifies actual HTTP
and durable selection effects, including restart. Run separate fresh attempts
with `--columns 160 --rows 48`, `--columns 80 --rows 24` and
`--columns 121 --rows 41` for paired dialog geometry.

`fuzzy_oracle.mjs` is a test-only generator using external `fuzzysort@3.1.0`
(the pinned original's dependency). Its JSON output is checked in at
`crates/oc-tui/assets/fuzzysort-oracle.json`. Cargo tests compare native scores
and heap ordering without Node; the Rust port retains the upstream MIT notice.

## VIS31/VIS32 bounded wheel and scheduling measurements

```sh
node scripts/tui_capture/capture.mjs \
  --bounded-mode wheel --geometry true --sample short --sidebar hide \
  --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/NEW-HIGH-REFRESH-ATTEMPT
node scripts/tui_capture/summarize_wheel.mjs \
  evidence/tui/recovery-v00/NEW-HIGH-REFRESH-ATTEMPT
```

Allow a bounded 300-second runner timeout. Existing paths and summary filenames
are refused. This mode runs actual authorized short/40-line `printf` function
calls on both applications, verifies returned outputs, expands the Shell card,
and sends actual SGR single/burst/reversed/edge wheel sequences with a multiline
editor draft at requested 165/250 Hz. A 32-entry catalog extension exercises
list-modal scrolling. Two real Responses streams cover detached and sticky
viewports; the sticky stream also receives 32 unique UTF-8 glyphs at each rate.
Paced input runs in the PTY bridge independently of browser sampling.

`protocol.json` preserves monotonic input timestamps and real function-call
results. `output-timeline.jsonl` preserves timestamped raw PTY chunks.
`wheel-checks.json` records endpoints, cursor, coarse frontend observations,
raw first/last terminal writes and exact UTF-8 input-to-output samples.
Frontend observations include browser/IPC overhead and are **not** millisecond
latency measurements. First/last wheel writes are diagnostic bounds: during
active streaming they also include scanner/provider output. Negative last-write
offsets at an edge mean the view finished changing before remaining ticks.
These fields are not configured FPS or physical display-refresh measurements.

Four settled one-second windows use `/proc/<pid>/stat` for actual process CPU
and `/proc/<pid>/status` for main-thread context switches. Context switches do
not enumerate every worker-thread wakeup. The native binary opts into
`OC_TUI_TEST_METRICS` and exits naturally to seal `scheduler.json`. Instrumented
draws include terminal writes; output totals are not changed-frame counts.
The renderer's relative first-draw time lacks an external monotonic epoch, so
this campaign cannot directly align every idle sample to scheduler frames.

Compare all normal/settled grids and PNGs without masks. Native's active
16.667ms wheel adaptation is an explicit temporal difference from original's
immediate default three-row endpoint, not evidence of MacOS acceleration or
momentum parity. `MEASURED` means the campaign completed; frame differences,
unstable active-animation PNGs, missing samples and anchor failures remain
visible and do not qualify VIS31/VIS32. History paging requires a separate
large-history scenario. This command does not invoke Cargo.
