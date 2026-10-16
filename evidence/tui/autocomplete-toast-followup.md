# T44 VIS25 — autocomplete toast follow-up (2026-09-24)

Immutable paired attempt: [`autocomplete-20260924-05/`](autocomplete-20260924-05/) (`capture.lock.json`, `commands.json`, `source-manifest.json`, both PTY text/cells/PNG frames and autocomplete checks, eight full-frame `*.grid-diff.json` + eight `*.png-diff.json`). This is the `-05` result, not a revision of earlier reports/captures.

**Provenance.** Pinned original `2670273ff17da96f85c5826ced57aa1b368754fa`, `opencode v2.0.12`, executable SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`; native HEAD `9e7ee6fdeeb6519a5d44e7747c2e6714a5aefbda` **plus dirty source** (`dirty_diff_sha256=677e2d1e06be6e5c545dffe466e8e0c2b31e2f20d3899cb96f3ea069b34509d9`, source manifest SHA-256 `2ae6f0e2c8898870d33d1c03fedcd62eb1ddae8e55e1f345bb1caf70396f6848`). Runner invoked `--build-oc true`: `cargo build --locked` exit 0; built `target/debug/oc` SHA-256 `3935ed0ea24b7d40d2922938a9a61f33bff7f8ec241995b590b36ce0faf87579`, version `oc 0.1.0`. Shared fixture SHA-256 `16a8757ab6b87d86b9db94e5b5beda3c1117cfa15997522339b86479524bc143`; 120×40 truecolor, dark opencode, hidden sidebar, xterm.js 6.0.0/DejaVu Sans Mono 14, profile ID `75fcd82f7fb47e6511f42bd40133631799e28aba9b0fbcb04b19c5d0860e638e`. Both bridges exit 0 with valid fake Responses contract (Home 0 requests, completed session 3 each, 0 invalid; no autocomplete-triggered request). Lock qualification is `DIAGNOSTIC_BASELINES_ONLY` with real wall clock, no masks.

**Unmasked full frames.** Each row corresponds to `NAME.grid-diff.json` and `NAME.png-diff.json` in the attempt. All eight grid and all eight PNG statuses are **FAIL/DIFFERENT**, comparator exit 1; runner exit **1**. Every grid checks 4,800 styled cells and agrees on cursor; every PNG checks 647,040 decoded pixels (1011×640).

| NAME | Different cells | Different pixels |
| --- | ---: | ---: |
| `home` | 46 | 1,834 |
| `autocomplete-home-trigger` | 709 | 20,862 |
| `autocomplete-home-filtered` | 230 | 23,298 |
| `autocomplete-home-after-tab` | 46 | 1,834 |
| `session-wide-completed` | 2 | 131 |
| `autocomplete-session-trigger` | 1,109 | 15,162 |
| `autocomplete-session-filtered` | 809 | 82,934 |
| `autocomplete-session-after-tab` | 2 | 131 |

**Observed effect.** Home `/` → `/ren` → Tab selects and executes `/reload` on both sides, closes the menu and clears the draft without creating a session or provider request. The paired `autocomplete-home-after-tab.txt` frames show the **same toast body, borders, close `x`, position and layout** (`┃  Configuration reloaded  x  ┃`, y1–3); the styled-cell diff has *zero differing toast cells* and the PNG difference bbox begins at y338, below the toast. Thus this region matches the pinned original **EXACTLY**. Whole-frame Home still fails with 46 cells / 1,834 pixels: original random placeholder `What is the tech stack of this project?` versus native `Fix broken tests`, plus the native's own version `0.1.0` versus `2.0.12`. No region match is promoted to a full-frame PASS.

Trigger/filter remain **DIFFERENT** in inventory and styling: on Home `/ren` original shows `/reload`, `/review`, `/btw` (y17–19), native `/reload`, `/dcp-compress` (y18–19); `/` menus also have different first entries/spacing and native intersects the logo. On completed session `/ren` original lists seven (`/rename`, `/reload`, `/review`, `/share`, `/fork`, `/export`, `/btw`) vs native three (`/rename`, `/reload`, `/dcp-compress`), with different position/paint. Tab inserts textual `/rename` on both session drafts, closes the menu and makes no request; `session-wide-completed` and `autocomplete-session-after-tab` differ only at elapsed-time glyphs x38–39,y10 (`143ms` vs `154ms`), 2 cells / 131 pixels each. Enter, Esc, navigation and empty-filter behavior are not established by these captures.

**Code/gates.** Current dirty changes include `crates/oc-tui/src/{app,shell,views}.rs`, `crates/oc/src/tui_cmd.rs`, `crates/oc/tests/support/startup.py`: typed info/success/warning/error toast variant, content-width/wrap/theme/border/close rendering, reload success/error notices, timed expiry (info 30s, others 5s) paused on hover, and close click requiring same-cell unmodified press/release (drag/resize cannot close it). Startup PTY warning check reconstructs sparse repaint rows and requires both warning clauses; no notice text or gate suppressed. Initial full gate output `tool_0d4d745de001UjwcVFkh09k5oi` **FAIL**: known intermittent `pty_t39::v05_raw_unicode_multiline_focus_and_one_durable_submit` timed out waiting for cursor `(17, 11)` (21/22 in that target; other suites including TUI 257 passed). Targeted rerun **PASS**; subsequent full gate `tool_0d4dd4d8a001bgnJCvJk1RiSX4` **PASS, 0 failed** (PTY target 22/22, TUI 257/257; fmt/clippy/build/docs/progress checks completed). No test, threshold, comparison or failure was suppressed. `git diff --check` PASS.

**VIS25 / T44: OPEN.** Toast-region exactness and green implementation gates do not qualify the still-different full frames or remaining VIS25 interactions.
