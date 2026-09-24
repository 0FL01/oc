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
a 90-line code block. The selected sample participates in the fixture hash.
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
bindings; native uses its existing Up/Down. This does not qualify keymap parity.
Native additionally checks a top-offset clamp after growing to 160×80.

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

`--tabs vertical --columns 162` and `--columns 163` check both sides of the
42-cell rail-adjusted auto-sidebar breakpoint using text **and styled blank
backgrounds**. `--devtools unset` omits the explicit override; native debug builds
then show native diagnostics, while the packaged original defaults to hidden.
Explicit `true`/`false` overrides the default in either application. These are
honest effective-channel differences, not identical-state comparisons.

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
