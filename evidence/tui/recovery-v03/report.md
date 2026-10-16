# T44 recovery V03 — viewport and shell geometry

## Result

Implemented the V03 slice over `a4bb361` in the working tree. No commit,
checkpoint, planning transition, or delivery operation was performed.

- Actual transcript `Rect.height` now determines the visible slice. The old
  20-line helper remains a **plain diagnostic/test projection**, not the screen
  viewport. Existing bounded history/live-part/byte budgets remain in force.
- Up/Down uses measured **wrapped rendered rows**, including scrollbox top
  padding. The per-frame maximum is cached so a burst of Up events does not
  re-render the whole active page for every key. Page/session changes invalidate
   that cache. Requested offset and draft survive resize. Review qualification
   additionally corrected Down to decrement the **displayed**, clamped offset
   after a grow, so the first Down always moves one rendered row.
- Short history starts below one scrollable padding row. This was checked
  against an actual original short-answer capture, rather than inferred from
  `stickyStart=bottom`.
- Sidebar is the original 42-cell raised surface, with no invented border
  column. Auto visibility is `available width > 120`, after the vertical rail;
  hidden config and durable child parentage suppress it. Title, measured usage,
  declared limit, and Location come from application DTOs; unavailable usage or
  limit is explicitly unknown. Location paths retain their basename when clipped.
- `cli.json`/`cli.jsonc` at the existing admitted config roots supply
  `session.sidebar: auto|hide`, `tabs.layout: horizontal|vertical`, and
   `debug.devtools: boolean`. The DTO preserves an optional explicit override
   and effective native build channel: `cfg!(debug_assertions)` maps to Local,
   otherwise Packaged. Unset follows Local=true/Packaged=false; explicit true or
   false wins. Hidden devtools consumes zero rows. Native diagnostics are
   informational, without upstream Server/Theme/Tools/Experiments actions.
- Prompt height follows wrapped/pasted lines, capped by the reference textarea
  geometry (`max(6, terminal height / 3)`); over-cap drafts show their tail and
  end cursor. Footer/status remain attached to the bottom stack. Usage-aware
  narrow footer layout was corrected from the actual 43/44-column captures.
- Bare/new launch renders centered Home; explicit empty session renders a
  session shell; accepted input transitions Home to populated session. Real
   startup/config errors now enter a distinct safe error TUI; session/history/
   catalog initialization failures enter a distinct query-error TUI. Dismissal
   exits nonzero and restores the terminal. Catalog failure is no longer silently
   replaced by an empty usable snapshot. Non-TTY launch stays an error.

Ownership: `oc-core` adds safe query fields (`HistoryPage.parent_id` and
`CatalogSnapshot.chrome`); `oc-adapters` projects durable parentage and admitted
CLI settings; `oc-tui` owns geometry/rendering/scroll state; `oc` identifies the
initial route and drives the existing runtime. Runtime policy, permissions,
provider execution, durable history ownership, keymap and Markdown code were
not changed.

## Checks

### Independent executable captures

Reference is the pinned original
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Targeted source inspection used upstream commit
`2670273ff17da96f85c5826ced57aa1b368754fa` in `upstream-v2`.
Each attempt contains actual VT bytes, styled xterm grids, PNGs, launch/profile
locks, hashes, fixture requests, and command exits. Both executables use the
same independently rendered xterm/Chromium profile. No source-derived expected
grid is called a reference.

Common runner invocation:

```text
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v03/v03-attempt-NN
```

| Attempt | Additional arguments | Exit | Actual result |
|---|---|---:|---|
| 01 | default V00 scenarios | 1 | Original executed. Rust completed the response, but the runner's pre-V02 model-ID footer predicate timed out; fixture also misclassified the native title request. Failed Rust cells/PNG/VT retained. |
| 02 | `--geometry true` | 1 | Both actual Home/session captures obtained after adapting the fixture to the real tools-empty, 256-token title request and display-name footer. Session differs; original Home grid comparison was INVALID (cursor outside grid), PNG differs. |
| 03 | `--geometry true --matrix true --sample rows` | 1 | Both resize series captured. Original multiline-paste predicate failed because upstream displays `[Pasted ~3 lines]`, not the pasted text. Failed capture retained; Rust multiline frame exists. |
| 04 | `--geometry true --sample short` | 1 | Both short-answer frames captured. Original establishes the one-row scrollbox top padding; Rust corrected afterwards. |
| 05 | `--geometry true --matrix true --sample rows` | 1 | Both full matrix series and paste states captured. Actual paste-chip versus expanded-text states are explicitly labelled. All grid/PNG comparisons DIFFERENT. |
| 06 | `--geometry true --sample short` | 1 | Short answer placement, sidebar surface/title/Context and prompt/footer geometry match targeted measurements; full comparisons DIFFERENT. |
| 07 | `--geometry true --sample short --devtools true --sidebar hide` | 1 | Both hide sidebar and allocate the debug row; underline/footer move one row up in both. Native diagnostics intentionally has different capabilities. Full comparisons DIFFERENT. |
| 08 | `--geometry true --matrix true --sample rows` | 1 | Pre-review matrix after narrow usage footer and scroll-cache fixes. Both executables executed valid fixture requests and all captures completed. Full comparisons DIFFERENT. |
| 09 | `--geometry true --sample rows --scroll-resize true` | 1 | Both actual scroll-away/shrink/grow/Down/re-pin grids and draft/cursor checks passed. Full comparisons DIFFERENT. Before extended top-clamp correction. |
| 10 | `--geometry true --sample short --tabs vertical` | 1 | Both actual 160×48 vertical-rail routes completed with no sidebar. Full comparisons DIFFERENT. |
| 11 | native only, `--geometry true --startup-error true` | 1 | Real malformed config produced safe native preflight error VT/grid/PNG. Missing original pair BLOCKED. |
| 12-child | seeded child hook, first attempt | 1 | Both original supported imports exited 0. Original capture predicate incorrectly expected child title; actual original root-title/Subagents route timed out. Failed original grid retained; native child captured. Enclosing Cargo test exited 101. |
| 13 | `--geometry true --sample short --devtools unset` | 1 | Both completed. Native debug build shows native diagnostics; packaged original hides devtools. Effective channels differ, explicitly not equivalent-state parity. Full comparisons DIFFERENT. |
| 14-child | seeded child hook, corrected predicate | 1 | Both real child routes captured; no Context and base-colored right boundary in both. Original root-tab/Subagents composer differs from native child tab/normal composer. Full comparisons DIFFERENT. |
| 14-query | seeded foreign-Location hook, native only | 1 | Real ownership query refusal produced safe query-error VT/grid/PNG. Missing original pair BLOCKED. Enclosing Cargo test for both 14 attempts exited 0. |
| 15 | native only, extended scroll-resize | 1 | Basic scroll states passed; first Down after top-offset clamp did not move. Failed predicate/grid retained. Production Down corrected afterwards. |
| 16 | paired extended scroll-resize, final native binary | 1 | Both basic scroll/draft/cursor sequences passed. Native top→grow→Down also passed. Paired full comparisons DIFFERENT; native-only extended states BLOCKED for absent reference. |
| 17 | `--geometry true --sample short --tabs vertical --columns 162` | 1 | Both completed; effective width 120, no Context and right background `#0a0a0a`. Full comparisons DIFFERENT. |
| 18 | same, `--columns 163` | 1 | Both completed; effective width 121, Context present and right background `#141414`. Full comparisons DIFFERENT. |
| 19-child | seeded child hook, final native binary | 1 | Both supported child routes recaptured; styled sidebar suppression checks passed. Original imports both exited 0. Full comparisons DIFFERENT. |
| 19-query | seeded foreign-Location hook, final native binary | 1 | Safe query error recaptured. Original missing pair BLOCKED. Enclosing Cargo test for both 19 attempts exited 0. |
| 20 | native only, `--geometry true --startup-error true` | 1 | Safe preflight error recaptured with final native binary. Original missing pair BLOCKED. |

Every runner `--build-oc true` build exited 0. Per-comparison exits are in the
immutable attempt `commands.json` and `capture.lock.json`; DIFFERENT is exit 1,
INVALID is exit 2, and missing pairs are BLOCKED, never PASS.

`geometry-summary.json`, produced by `scripts/tui_capture/geometry.mjs`, contains
targeted measurements from the **actual styled grids**, with older observations
retained alongside final attempt 08. Coordinates are zero-based. In the paired
short-answer capture: user text y=3, answer y=6, sidebar x=118/width=42/background
`#141414`, Context y=5, usage y=6, underline y=45, footer y=46. Debug-on/hidden
sidebar moves underline/footer to y=44/45 in both.

Pre-review paired viewport observations (attempt 08):

| Grid | Original visible row markers | Rust visible row markers | Sidebar |
|---|---:|---:|---|
| 80×24 | 13 | 13 | absent |
| 120×40 | 29 | 29 | absent |
| 160×48 | 37 | 37 | x=118, width=42 |
| 43×48 | 37 | 36 | absent |
| 44×48 | 37 | 36 | absent |
| 119×48 | 37 | 37 | absent |
| 120×48 | 37 | 37 | absent |
| 121×48 | 37 | 37 | x=79, width=42 |
| 120×80 | 69 | 69 | absent |
| 160×48 after shrink/grow | 37 | 37 | x=118, width=42 |

### Test/check attempt ledger

No failing test was ignored or removed. Existing self-goldens were updated for
the changed placement/debug surface and are regression checks only.

| Command / stage | Exit | Result |
|---|---:|---|
| `cargo test --locked -p oc-tui v03_ -- --nocapture`, before implementation | 101 | Both new tests failed: short transcript bottom placement and missing second pasted line. |
| Same targeted command after initial correction | 0 | 2 passed. |
| Initial `cargo build --locked` | 0 | Real binary built. |
| First full `cargo test --locked -p oc-tui -- --nocapture` | 101 | 93 passed, 5 old placement/debug-coordinate goldens failed. |
| `cargo test --locked -p oc --test recovery_v03 -- --nocapture`, attempt 1 | 101 | Test setup omitted Location ownership prefs for seeded durable sessions. Fixture corrected through the existing storage API. |
| Full oc-tui after geometry-golden updates | 0 | 99 passed. |
| Binary V03 attempt 2 | TIMEOUT | Shell tool stopped at 120 s. Test drain waited for silence despite cursor updates every frame; drain made time-bounded. No surviving test processes observed. |
| Targeted `oc-tui v03_` after expanded conditions/scroll test | 0 | 3 passed. |
| Binary V03 attempt 3 | 101 | Normal/hidden/child passed. Debug-on legacy transcript had exactly 35 visible marker rows at height 48; initial tall qualification moved to height 80, preserving the `>35` assertion. |
| Binary V03 attempt 4 | 101 | All five layout modes passed; large burst of Up timed out before frame output because each key recalculated wrapped rows. Per-frame row maximum now cached. |
| Full oc-tui after top-padding correction | 0 | 99 passed. |
| Binary V03 attempt 5 | 0 | All five modes, resize/draft, wrapped Up, Home/empty, startup error and non-TTY passed. |
| `cargo test --locked -p oc --test recovery_v02 -- --nocapture` | 0 | V02 actual-binary metadata/parts/restart regression passed. |
| Initial workspace all-target clippy | 0 | No warnings. |
| Workspace test attempt 1 | 101 | Old resize test assumed bare launch was full-width session. It now explicitly attaches an empty session; original full-width assertions retained. |
| Workspace test attempt 2 | 101 | Old bare-launch test expected an untitled-session tab. It now asserts actual Home logo plus configured model before submitting. Unused old test convenience constructor removed. |
| Workspace test attempt 3 | 101 | All preceding suites passed; oc-tui 99/100. Status-color test exposed stale cached maximum after page replacement; page/session changes now invalidate it. |
| Final targeted full oc-tui | 0 | 100 passed. |
| Final `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0 | Passed. |
| Final `cargo build --locked` | 0 | Passed. |
| `cargo fmt --all -- --check` and `git diff --check` | 0 | Passed. |
| Pre-review final `cargo test --locked --workspace` | 0 | **441 passed, 4 existing live ignores**, all doc tests passed. Includes actual PTY V03 and V02, Location/permissions/child regression, bounded-memory/soak tests. |
| Review: new hostile-config raw PTY test before correction | 101 | Existing geometry modes passed; assertion for distinct TUI failed because startup only printed stderr. No hostile sentinel leak, but no error screen. |
| Review: `cargo test --locked -p oc-tui v03_ -- --nocapture` | 0 | 4 geometry/scroll/style tests passed. |
| Review: `cargo test --locked -p oc-core v03_ -- --nocapture` | 0 | Optional override + Local/Packaged default matrix passed. |
| Review: `cargo test --locked -p oc --test recovery_v03 -- --nocapture` | 0 | Six actual PTY modes including unset debug, foreign-Location query error, hostile malformed config, Home/empty and non-TTY passed (50 s). |
| Review: `cargo test --locked -p oc --bin oc v03_ -- --nocapture` | 0 | Injected catalog-query failure reaches Query error, not a usable empty session. |
| `OC_V03_CAPTURE_OUTPUT=…/v03-attempt-12 cargo test --locked -p oc --test recovery_v03 -- --nocapture` | 101 | Actual original child route differed from capture predicate; failed artifact retained. |
| Same hook, `v03-attempt-14` | 0 | Supported original child and native child/query captures completed; raw PTY suite passed (68 s). Diagnostic runner exits remain 1, not parity success. |
| After Down correction: `cargo fmt --all` and targeted `oc-tui v03_` | 0 | Formatting and all 4 tests passed. |
| Review workspace attempt 1, 120 s shell budget | TIMEOUT | Tool terminated during MCP suite after preceding suites, including binary V03, passed. No test failure reported; no complete suite exit obtained. |
| Review final `cargo test --locked --workspace`, 300 s shell budget | 0 | **443 passed, 4 existing live ignores**, all doc tests passed. |
| Review final `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0 | Passed. |
| Review final `cargo build --locked`, `cargo fmt --all -- --check`, `git diff --check` | 0 | All passed. |
| Final-binary artifact refresh: seeded capture hook `v03-attempt-19` | 0 | Child/query capture checks plus all six actual raw PTY modes passed (70 s). |

All `cargo fmt --all` formatting runs exited 0. Geometry-analysis invocations
exit 0 on successful artifact analysis; this is not a parity exit status.

`recovery_v03.rs` + `support/geometry.py` launches the actual `CARGO_BIN_EXE_oc`
in isolated PTYs. It verifies 80/120/160 and 43/44/119/120/121 shrink/grow,
normal/hidden/child/debug/vertical modes, multiline draft retention, >35 visible
markers on a tall terminal, and Up exposing the beginning of a single long
wrapped message at 120×80. Child identity is genuine durable parent metadata.
Separate styled Rust tests assert sidebar background boundary and DTO usage.

### Review evidence and native capability mapping

The actual paired scroll sequence in attempt 16 records the following marker
ranges in **both** executables. Every state retains `scroll-draft` with the visible
cursor at its end. Coordinates are zero-based; all are read from actual xterm
cells, not a Rust helper or source-derived expected frame.

| State | Grid | Visible markers | Cursor |
|---|---|---|---|
| Pinned draft | 160×48 | 053–089 | (17,42) |
| Scroll away | 160×48 | 041–079 | (17,42) |
| Shrink | 80×24 | 065–079 | (17,18) |
| Grow | 160×48 | 041–079 | (17,42) |
| One Down | 160×48 | 042–080 | (17,42) |
| Re-pin | 160×48 | 053–089 | (17,42) |

Native extended top→160×80→Down exposes 000–065 then 000–066, retaining
cursor (17,74). The original uses its supported Ctrl+Alt+Y/E scroll bindings;
native uses existing Up/Down. These measurements qualify scroll geometry, not
keymap equivalence. See each side's `scroll-checks.json`, inputs, raw VT, styled
cells and PNG. Attempt 16's formatting check ran concurrently with capture, but
its actual binary hash equals subsequent cleanly built attempts 17–20:
`9512d7c0698768885889ee1c203d30e1a19310d2d49cfb3ebbfb4143adb810b3`.

| Capability / failure | Native mapping and evidence | Reference boundary |
|---|---|---|
| Preflight config/runtime failure | Separate static safe reason and restart/config/data guidance, no raw config/error interpolation. Hostile sentinel absent in PTY; actual final frame `v03-attempt-20/oc/preflight-error.*`. | Native in-process runtime has no service attach; no invented attach route or paired-equivalence claim. |
| Session/history/catalog initialization failure | Separate Query reason. Actual foreign-Location refusal in `v03-attempt-19-query/oc/session-foreign.*`; catalog-specific unit negative ensures failure propagates. Dismissal exits nonzero. | Query ownership failure is not upstream service connection failure. |
| Devtools | Explicit `Option<bool>` + build-channel DTO. Unset Local shows an informational Native runtime/UI row; unset Packaged hides. Explicit true/false wins. Both channel matrices unit tested; unset/true/false actual debug-binary PTYs tested. | Original `debug.devtools ?? channel === local` condition mapped honestly. Packaged original hides by default in attempt 13. No Server/Theme/Tools/Experiments controls or release-binary default capture claimed. |
| Child sidebar | Genuine native durable parent metadata, normal `--session`; final actual grid in `v03-attempt-19-child/oc/`. | Original parent and child imported through supported `session import --standalone`, then normal `--session`. Payloads/import commands/exits retained. Both right blank surfaces are base-colored, sidebar absent. |
| Vertical rail | Final paired 162/163 grids and per-side `chrome-checks.json` in attempts 17/18 verify subtraction of 42 before `>120`. | Normal `cli.json` tabs.layout configuration, no renderer replacement. |

Original child import contract was inspected in the pinned full source at
`/home/opencode/.cache/opencode-tmp/oc-v2-src`: CLI
`commands/handlers/session/import.ts`, schema `session-transfer.ts`/`session.ts`,
and core `session/transfer.ts`. Original `--help`, `session --help`, and
`session import --help` checks exited 0. Import creates the parent first and
retains parentID through the supported transfer API. The original child route
actually shows the root tab title and Subagents composer; the runner now waits
for that real route, retaining attempt 12's failed earlier assumption. Original
and native agent/model/footer/composer contents are not represented as equal.

## Risks / remaining gaps

- **No VIS03/VIS04/VIS05/VIS07 or full T44 parity gate is closed.** Exact paired
  cell/PNG comparisons remain DIFFERENT. Targeted geometry agreement is not a
  whole-screen parity claim.
- Narrow 43/44 captures show one fewer Rust transcript marker because the
  existing message footer wraps differently; message rendering remains V06.
  The prompt footer's narrow usage/shortcut condition itself was corrected and
  recaptured in attempt 08.
- Original paste is a collapsed paste chip; native pasted text expands the
  composer. This slice implements geometry over the existing input string;
  paste chips, editor navigation/history/graphemes/selection remain V05.
  **Near-limit (1 MiB budget) PTY paste was not qualified in V03.** Existing unit
  budget tests and three-line PTY paste are not near-limit or editor equivalence
  evidence. V05 must qualify actual bounded near-limit paste, draft and cursor.
- Home logo/composer geometry is present, but rotating placeholder text,
  upstream default Build profile, multiple-session tab strip/add button,
  interactive rail resizing and original debug actions are not reproduced.
  Native displays its actual package version and actual configured agent/model,
  rather than fabricating the original's values.
- Safe native startup/query error TUI and non-TTY behavior are tested on the
  actual executable. Original service attach remains unsupported by the native
  in-process architecture; no connection-error route equivalence is claimed.
  Child sidebar now has paired original/native evidence, but upstream root-tab
  and Subagents composer/navigation behavior remains a capability/visual gap.
- Context uses the measured V02 usage currently available in the bounded
  window and the effective catalog limit. It does not invent missing cache/
  reasoning token components or accumulated historical dollar cost. Switching
  to a different model can change the denominator; fully generation-pinned
  historical Context/cost semantics require an extended upstream-equivalent DTO.
- Vertical rail accounting has final paired styled evidence at adjusted widths
  120/121; child suppression has final paired styled evidence via supported
  original import. No claim of full multi-tab/navigation parity.
- Reference elapsed times, token rates, version, and isolated paths remain real
  and unmasked. Attempts 06/07 precede the final narrow-footer/cache-invalidating
  fixes; attempt 08 contains the pre-review matrix. Review final-binary captures
  are attempts 16–20. Explicit devtools=false in the geometry captures preserves
  their requested geometry despite the corrected native Local default.

## Next

Parent review the implementation and immutable captures, then own the requested
checkpoint/commit/delivery. Prioritize V04 dialogs, V05 editor/keymap/paste-chip
behavior and V06 message/footer rendering before whole-screen qualification.
Use attempt 16 for paired scroll/draft/cursor, 17/18 for vertical breakpoints,
19-child for child suppression, 19-query/20 for safe native error states, and
13 for honest unset-channel behavior. Attempt 08 remains the pre-review width
matrix, 06/07 the short-answer and debug/hidden-sidebar comparisons. Retain all
earlier failed attempts. The pre-existing user ZIP remains outside this slice.
