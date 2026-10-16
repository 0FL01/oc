# T44 VIS25 — pinned autocomplete/reload follow-up (2026-09-24)

Source of this report: immutable paired diagnostic `evidence/tui/autocomplete-20260924-04/` (`capture.lock.json`, `commands.json`, `upstream/` and `oc/` autocomplete checks, full-frame styled-cell/PNG diffs and text frames). This is a **new** run; `autocomplete-report.md` describes the earlier `-03` run and is not a result for `-04`.

## Provenance and execution

- Pinned original source `2670273ff17da96f85c5826ced57aa1b368754fa`, executable `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`, SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`, version `opencode v2.0.12`.
- Native source HEAD `2ffb77b0c250f2cf8bcce22976e5198ecbad6182` (tree `6a1caafbd4381140fa444820d85ffaae4ad884eb`) **plus dirty source**, not HEAD alone: `capture.lock.json` records `dirty_diff_sha256=2ddb3751999accfb355ce7bbd40861d798060d077ae80472d500062536cc7cc0`, `source_manifest_sha256=67eac8fbf5e91f3c5bdec85560f66f4da5f0f7b08ad2da1d47c77240f8ca6c42` and `source-manifest.json` hashes the build inputs. `--build-oc true` ran `cargo build --locked` successfully before capture; the built `target/debug/oc` SHA-256 is `6eac3ffcc4457959beac7f10f5387d41757c6ecc89cb1bb290c439bf7730e792`, version `oc 0.1.0`. This attests the capture binary to this dirty-source build, unlike the earlier no-build probe.
- Exact runner invocation (recorded argv, `/usr/bin/node`):

  ```sh
  /usr/bin/node /home/opencode/ai/oc/scripts/tui_capture/capture.mjs --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --geometry true --sample tools --sidebar hide --agent-profile true --columns 120 --rows 40 --autocomplete true --output /home/opencode/ai/oc/evidence/tui/autocomplete-20260924-04
  ```

  Runner exit **1**: all eight full-frame grid and all eight PNG comparisons are `DIFFERENT` (individual comparator exit 1 / JSON `FAIL`), not a parity pass. Both application bridges exit 0 and satisfy the fake Responses provider contract; the fixture transcript includes one completed `read`. Autocomplete keystrokes cause no additional provider request (Home 0; completed session 3 on each side, 0 invalid). Common Reader/tools fixture SHA-256 `16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`; terminal/profile ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`: 120×40, truecolor, xterm.js 6.0.0, DejaVu Sans Mono 14, dark opencode theme, requested hidden sidebar. The lock explicitly calls these **diagnostic baselines only**, not frozen-clock or fully qualified visual states.

## Full-frame results

Each row is the paired original/native frame and its `NAME.grid-diff.json` / `NAME.png-diff.json` under `-04/`. Every grid checked **4,800 styled cells**, every PNG **647,040 decoded pixels** (1011×640); every cursor comparison is `cursor_differs=false`. Counts are absolute, unmasked full-frame differences, including real placeholder/version/elapsed glyphs.

| Frame (`NAME`) | Grid differing cells | PNG differing pixels | Observed state |
| --- | ---: | ---: | --- |
| `home` | 46 | 1,834 | Initial Home; real prompt example and version differ. |
| `autocomplete-home-trigger` | 709 | 20,862 | `/` opens a menu on both; inventory/paint differ. |
| `autocomplete-home-filtered` | 230 | 23,298 | `/ren`: original three entries, native two, with different rows/styles. |
| `autocomplete-home-after-tab` | 165 | 14,675 | Both execute `/reload`, clear the draft and show a reload notice; toast geometry/style, placeholder and version still differ. |
| `session-wide-completed` | 2 | 110 | Fixture turn completed; only genuine elapsed glyphs differ (grid x37–38, y10). |
| `autocomplete-session-trigger` | 1,109 | 15,141 | `/` opens differing command lists on completed session. |
| `autocomplete-session-filtered` | 809 | 82,913 | `/ren`: seven original vs three native visible options, different layout/paint. |
| `autocomplete-session-after-tab` | 2 | 110 | Both show textual `/rename`, menu closed; elapsed glyphs remain different. |

## Direct PTY observations

- **Home `/` → `/ren` → Tab:** original `/ren` shows `/reload` (y17), `/review` (y18), `/btw` (y19), with no `/rename`; native shows `/reload` (y18) and `/dcp-compress` (y19), also no `/rename`. Tab in this run **executes `/reload` on both sides**, rather than inserting a rename candidate. `upstream/autocomplete-home-after-tab.txt` paints `Configuration reloaded` in a compact top-right `┃ … x ┃` toast; `oc/autocomplete-home-after-tab.txt` paints the same notice in a wider, more leftward `│ … │` box without the `x`. Both menus close, draft clears to a real placeholder, Home stays sessionless, and neither sends a model request for this action. The earlier report's native Home `/rename ` outcome belongs to its different executable/dirty-source attempt, not this one.
- **Completed session `/` → `/ren` → Tab:** original `/ren` shows `/rename`, `/reload`, `/review`, `/share`, `/fork`, `/export`, `/btw` (y26–32); native shows `/rename`, `/reload`, `/dcp-compress` (y30–32). Tab on each side closes the menu and inserts the **textual `/rename` completion** into the draft (caret x13,y34); it does **not** execute a rename or create a model request. The paired `.txt` frames render `/rename` without a visible trailing-space glyph; this observation does not assert an Enter outcome.
- On `/`, original Home starts `/agents`, `/btw`, `/cd`, `/clear`, `/connect` and session adds `/compact`, `/copy`; native starts `/agent`, `/agents`, `/cards`, `/clear`, `/close-tab`, then `/commands` and `/dcp-compress`. These are real command-inventory differences (including missing native `/review` and `/btw` in the filtered lists), not a claim that unsupported upstream actions should be fabricated. Original menu labels use wider command/description spacing; native Home `/` menu text visibly intersects the logo in rows 14–17. Filtered-session different pixels include repositioned rows and styling, not just text.
- Initial/after-reload original Home placeholder: `Ask anything… "What is the tech stack of this project?"`; native: `Ask anything… "Fix broken tests"`. Bottom-right real versions are `2.0.12` vs `0.1.0`. Completed-session footer shows original `132ms` vs native `202ms`; the 2-cell/110-pixel session baseline and after-Tab difference is this unfrozen elapsed value. No placeholder, version or wall-clock masking was applied, and a region match would not convert any `FAIL` to PASS.

## Checks and status

Relevant owner-backed real PTY checks in `crates/oc/tests/pty_t42.rs`: `vis25_reload_rebuilds_real_config_on_home_and_session_without_losing_identity` tests failed config rollback/draft retention then Home `/ren` Tab → actual reload and refreshed model (no Home root), plus attached-session identity; `reload_refuses_busy_turn_and_refreshes_parked_tab_after_owner_success` tests busy-turn refusal and parked-tab catalog refresh. Both are `ok` in the supplied full workspace gate output. The same output records adapter reload tests, TUI `home_ren_filter_selects_real_reload_without_stealing_workspace_ren` and `reload_refresh_invalidates_generation_options_without_erasing_draft` as `ok`.

Full workspace gate output: `/home/opencode/.local/share/opencode/tool-output/tool_0d4acfc0e001Zz0WSA18o463mv`. All reported test suites finish with **0 failed** (including `oc` PTY T42 33 passed, `oc-adapters` unit 208 passed, `oc-tui` unit 254 passed; opt-in live tests remain ignored). Following the tests the output shows successful workspace check and build, `OK: documentation/registry/examples/journal structure only`, and `OK: check (journal structure only, not product acceptance)`. These green implementation gates do not override the sixteen failed frame comparators.

**VIS25 / T44: OPEN.** The capture covers `/` trigger, `/ren` filter and Tab on Home and session, not VIS25's remaining navigation/Enter/Esc/empty-filter cases; every full-frame comparison is still different. Continue from the pinned `-04` observations and owner-backed behavior without asserting pixel parity from textual completion or successful reload alone.
