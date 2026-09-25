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
