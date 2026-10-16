# Current TUI inventory and gaps vs upstream opencode v2.0.12

Recon only, no code changes. Repo HEAD `7895f43`. Method: static read of `crates/oc-tui/src/*` (3378 lines), wiring
`crates/oc/src/{tui_cmd,bootstrap,cli}.rs`, `crates/oc-core/src/{core_app,queries}.rs`, `crates/oc-adapters/src/patch.rs`,
PTY harness `crates/oc/tests/pty_t39.rs` / `pty_t42.rs`. At write time `evidence/tui/upstream-inventory.md` did not exist,
so upstream is the task-brief component list (`packages/tui/src/**`, `packages/theme/src/tui/**`), not a local checkout
(local `/home/opencode/ai/opencode` is v1.1.60 and has no `packages/tui`).

## 1. Module map and state flow

Flow: crossterm `Event` -> `events::map_event` -> `UiEvent::{Key,Paste,Resize}` -> `tui_cmd::handle_event` ->
`TuiState::handle_key`/`handle_panel_key` -> `KeyOutcome{note,intent,consumed_input}` -> `tui_cmd::apply_outcome` ->
`PanelIntent` applied through `CoreApp` -> snapshots (`apply_*`) / worker `CoreEvent`s -> `TuiState` -> `views::render_frame`.

- `lib.rs` (31): declares modules 9-17; re-exports `render_smoke_frame`/`tui_name` 19; `truncate_utf8` 22. Dependency rule D12
  (lines 1-7): `oc-tui` sees only `oc-core` public app API + `oc-adapters` read-side (`models`,`config`,`patch`); never storage.
- `app.rs` (1446): `VIEWPORT_LINES=20` 26, `MAX_INPUT_BYTES=oc_core::session::MAX_INPUT_BYTES` 29, `CARDS_MAX=160` 31.
  `TuiStatus` 35-44 (Idle/Streaming/Cancelled/Quit); `TuiPanel` 48-65 (None/Model/Agents/Sessions/Skills/Help(topic)/Dcp/Cards);
  `PanelIntent` 69-109 (10 variants incl. ChooseModel, SwitchSession, SwitchLocation, LoadOlder/Newer, Compress); `KeyOutcome`
  114-121; `TuiState` 124-162 (app handle, session, status, panel, input, `HistoryWindow`, `live_text`, scroll, note,
  active_turn, `ModelPicker`, catalog/agents/sessions/skills/cards caches + cursors, `DcpPanelState`).
  Constructors/reset 166,209,228; `viewport()` 313-335 formats `role: text` rows and `ai: {live_text}`, returns the last
  <=20 lines with scroll; snapshot sinks 340,363,370,377,382,390; `handle_paste` 404; `handle_key` 498-561;
  `handle_enter` 563-602 (dispatch or `app.submit`, synthetic `you:` row); `run_command` 604-656; `handle_panel_key` 660-696;
  `panel_enter` 731-767; worker sinks `apply_delta` 773, `apply_finished` 785, `apply_interrupted` 797, `apply_failed` 808;
  `card_row` 838-907; test `ScriptDriver` 953-1002; unit tests 1019-1446.
- `views.rs` (334): `render_frame` 19-63; `render_test` (TestBackend) 66-80; `panel_lines` 83-153 (all panels 8 rows);
  `help_topic` 155-163; render tests 239,258,308.
- `events.rs` (125): `KeyAction` 12-31; `UiEvent` 35-43; `map_key` 49-65; `map_event` 68-75.
- `commands.rs` (152): `COMMAND_NAME_MAX=32` 9, `COMMAND_ARGS_MAX=512` 11; `CommandAction` 15-40; `dispatch` 43-73;
  `BUILTINS` 76-85 (8 names, missing `cards`); `complete` 88-94 (only used by its own tests).
- `history.rs` (506): `WINDOW_ROWS=240` 15, `WINDOW_BYTES=256KiB` 17, `CARD_PREVIEW=512` 19, `CARD_FILES=5` 21;
  `HistoryRow` 25-33; `HistoryWindow` 46-168 (reset/prepend/append, `push_synthetic` 139, `enforce` 153); `row_from_page`
  170-179; `ToolCard` 183-205; `card_from_row` 208-224; `patch_diff` 227; `preview` 263.
- `picker.rs` (458): `PICKER_WINDOW=10` 17; `RefreshStatus` 21-26; `PickerState` 30-42 (Browsing/Selected/Retired);
  `ModelPicker` 53-63; `status_line` 99-112; `window` 120; `variants` 158; `pending_variant` 183; `cycle_variant` 192;
  `move_cursor` 199; `choose_id` 222; `refresh` 240; `load_persisted_raw` 254; `retired` 310.
- `dcp_panel.rs` (203): `FOCUS_MAX=256` 13, `NOTICE_MAX=120` 15; `CompressRequest` 19; `DcpOutcome` 26-39
  (Started/Done{saved_tokens}/Failed{reason}); `DcpPanelState` 43-49; `request_compress` 59; `set_outcome` 82;
  `panel_rows` 111-142 (context/blocks/pending/last/nudge rows).
- `terminal.rs` (56): `TerminalGuard` 15; `enter` 26-34 (raw mode + `EnterAlternateScreen` on stderr); idempotent
  `restore` 38-41; `install_panic_hook` 48-55 (OnceLock, restores then chains).
- `smoke.rs` (67): `render_smoke_frame` 22-44 renders `oc smoke pending=N` on a 40x5 TestBackend; unrelated to daily TUI.
- Wiring `crates/oc/src/tui_cmd.rs` (458): `run_tui` 40; `run_inner` 51-73 (TTY gate, `application::spawn`, shutdown+join);
  `drive_ui` 86-150: `enter` 87, panic probe 88-90, `CrosstermBackend(stdout)` 91, `create_session` 93, initial
  `history_page` 101-105, `catalog` 108-110, frame loop 112-146 (`event::poll(50ms)` 124, <=256 keys/frame 37/125,
  worker drain 134, DCP refresh on panel open 139-145), metrics probe 147/438-451. `handle_event` 168-194 (panel vs chat;
  paste only in chat); `apply_outcome` 197-219; `apply_intent` 221-356 (SwitchSession/Location refuse while busy 276-279,
  291-294; cards paging via `cards_before` rowid 241-253); `handle_worker_event` 358-398; `report_compress_outcome` 409-435
  (reads `savedTokens` from recorded `compress` tool ops); `interactive_ready` 160-163. `bootstrap.rs` 36 (bare `oc`) and
  61 (`oc tui`); `cli.rs` `Command::Tui{session}` 50-54.
- Core surface used: `CoreEvent` `core_app.rs:26-70` (TurnStarted/TextDelta/TurnFinished/TurnInterrupted/TurnFailed);
  `CoreApp` methods at 313,318,328,369,382,405,427,447,457,467,477,495,505,518,536. DTOs in `queries.rs`:
  `HistoryMessage:16`, `HistoryPage:27`, `ModelEntry:40`, `VariantEntry:53`, `AgentEntry:64`, `CatalogSnapshot:77`,
  `LocationSnapshot:99`, `SkillCard:112`, `DcpSnapshot:123`, `ToolOpView:142`, `ToolOpPage:163`.

## 2. Current visual inventory

- Regions (`views.rs:28-35`): one full-screen vertical `Layout` with exactly three chunks — history pane `Min(1)`, panel pane
  `Length(panel.len()+2).min(12)` (0 hides it), prompt pane `Length(3)`. No header/logo, no footer/status bar, no sidebar,
  no overlay/modal; panels are inline bordered panes, not centered dialogs.
- Widgets: only `Block`+`Borders::ALL`+`Paragraph` (`views.rs:9-14`, used 51-62). No `List`, `Table`, `Tabs`, `Clear`,
  `Gauge`, `Sparkline`, `Span`/`Line`, no cursor.
- Styles: **none**. `rg 'Color|Style|Modifier|stylize' crates/oc-tui` finds no usage; every cell renders with terminal-default
  fg/bg and default border glyphs. There are no style constants anywhere in the workspace.
- Titles/status: history pane title `format!("oc {:?}", state.status())` plus ` — {note}` and ` — {dcp notice}`
  (`views.rs:39-45`); panel title literal `"panel"` (`views.rs:57`); prompt title literal `"prompt"` (`views.rs:61`).
- Message rendering: plain one-line strings `"{role}: {text}"` (`app.rs:317-324`), live stream as `"ai: {live_text}"`
  (`app.rs:326-328`); `Paragraph` with no wrap (`views.rs:51`) so long lines clip. No markdown, code blocks, reasoning,
  timestamps, separators, avatars, or syntax highlighting. Committed roles are `user`/`assistant` (`history.rs:170-179`),
  synthetic rows use `you`/`ai`/`""` (`app.rs:580,792,804,815`).
- Tool cards: not inline in the transcript. Only the Cards panel renders one text row per card
  (`views.rs:131-138`) from `card_row` (`app.rs:838-907`): `{name} {state} [files] -> output diff Nf +A -R (op)`, with
  per-file `+path +a -r (Nh) -> target` (`app.rs:849-878`). `apply_patch` diff is a bounded summary (counts/paths only,
  `oc-adapters/src/patch.rs:240-275`), never patch bytes; other tools list up to 5 parsed paths (`history.rs:253-261`).
- Panels (all text lines, 8-row cap `views.rs:84`, max pane height 12 `views.rs:26`, selection marker `>`):
  Model `views.rs:87-100` (header `model | <picker state>`, optional `variant: X`, 10-id window from `picker.rs:120-132`,
  trailing error); Agents `101-115` (`agents | enter selects`, `id — description [model]`); Sessions `116-123`
  (`sessions | enter resumes, esc closes`, raw ids); Skills `124-130` (`id — name: description`); Cards `131-138`;
  Help `139-145` (builtin list one-liner + 4 topics `views.rs:155-163`); Dcp `146-151` (`dcp_panel.rs:111-142`).
- DCP notice and intent errors are transient text in the history pane title, never history rows (`app.rs:430,451`,
  `views.rs:40-45`); `/dcp-compress` forces `panel=Dcp` and clears live text (`app.rs:440-448`).
- Input editor: `String` only, rendered raw in the 3-row bordered pane; Char append with budget note (`app.rs:501-513`),
  Backspace pop (`app.rs:514-517`); no cursor movement, no multiline, no input history (Up/Down are transcript scroll),
  no completion UI, no caret rendering.
- Keybindings actually implemented:
  - `events.rs:50-64`: `(Ctrl+c|Ctrl+d) => Quit`, `Esc => Cancel`, `Enter`, `Backspace`, `Up/Down/Left/Right`,
    `Char(c) if !CONTROL|ALT`; everything else (Alt chars, Tab, PgUp/PgDn, Home/End, Shift+*, F-keys) -> `None`.
  - Chat `app.rs:499-561`: `Left|Right => no-op`; `Char` append; `Backspace` pop; `Up` scroll + `LoadOlder` at top edge
    (523-524); `Down` scroll or `LoadNewer` at bottom (536); `Quit`; `Cancel` cancels turn or quits when idle (545-558);
    `Enter` submit/command (559).
  - Panel `app.rs:661-695`: `Quit`; `Cancel` closes panel (666-669); `Left|Right` cycle model variant (670-677);
    `Up` cursor move + `LoadCards` at top (678-688); `Down` cursor move; `Enter` `panel_enter` (693).
  - Ctrl-C/Ctrl-D only; no `?` help key, no Esc-to-clear-input, no Tab completion.
- Terminal modes enabled: raw + alternate screen only (`terminal.rs:26-34`). No `EnableBracketedPaste`, no
  `EnableMouseCapture`, no keyboard-enhancement protocol, although `Event::Paste` is mapped (`events.rs:71`) and a paste
  path exists (`app.rs:404-421`) — real terminals will only emit bracketed paste if enabled.
- No spinner/animation/tick; `Streaming` appears only as the `oc Streaming` title; no frame-rate or elapsed time display.
- Metrics probe is opt-in test-only (`tui_cmd.rs:33,438-451`), writes counters JSON, not a screen.

## 3. Gap table (upstream brief list)

| Upstream component/behavior | Status | Our evidence | Needed |
|---|---|---|---|
| Theme package / palette roles (text, muted, primary, error/warning/success, panel, border, diff add/remove, user/assistant) | missing | no `Color`/`Style` anywhere; only default widgets `views.rs:9-14,51-62` | new `theme` module with role constants + applied `Style`s; golden palette test |
| Layout regions (header/logo, stream, editor, status/footer, overlays) | partial | fixed 3-chunk stack `views.rs:28-35`; no header/footer/overlay | redesigned layout (regions + centered `Clear` dialogs) with size matrix tests |
| Text messages | partial | `role: text` lines `app.rs:317-324`; clip, no wrap `views.rs:51` | styled message blocks, wrapping (`unicode-width`), role alignment |
| Markdown (headings/lists/code/inline code) | missing | absent (no markdown dep in `crates/oc-tui/Cargo.toml`) | markdown renderer or vendor-neutral parser; code-block styling |
| Reasoning/thinking blocks | missing | absent; `CoreEvent` has no reasoning delta `core_app.rs:26-70` | core event + DTO field, collapsible styled block |
| Inline tool cards | partial | Cards panel only `views.rs:131-138`; row builder `app.rs:838-907` | inline per-tool cards with state colors, expand/collapse |
| Diffs | partial | bounded `DiffSummary` row `app.rs:842-879`, `patch.rs:240-275` | unified +/- diff rendering inline in transcript/cards |
| Status bar | partial | pane title `oc {status:?}` + notes `views.rs:39-45` | persistent status/footer (model, agent, context, keys, busy) |
| Input editor | partial | raw `String`, pop only `app.rs:501-517`, `views.rs:60-62` | cursor, multiline, history, completion popup, key hints |
| Command palette | missing | `commands.rs:88` `complete()` unused; slash table only 43-73 | palette overlay with fuzzy filter, preview, key nav |
| Session list dialog | present (text) | `TuiPanel::Sessions` `app.rs:56`, `views.rs:116-123` | richer rows (title/time/count), rename/delete, current-session mark |
| Model dialog | present (text) | `views.rs:87-100`, `picker.rs` | upstream dialog chrome, per-model metadata, search |
| Agent dialog | present (text) | `app.rs:54`, `views.rs:101-115` | dialog chrome, mode grouping, current marker |
| Help/keys dialog | partial | `/help [topic]` `views.rs:139-145`, 4 topics 155-163 | full keymap table, `?` binding, searchable help |
| Error details | missing | errors become note/`(error: …)` `app.rs:430,808-816` | scrollable error dialog (stack, copy) |
| Toasts | partial | single `note` in title `app.rs:451`, `views.rs:40-42` | toast stack with severity/expiry |
| Spinner/loading | missing | only `Streaming` string `views.rs:39` | animated spinner/busy affordance per turn |
| Devtools bar | missing | absent | decide not-applicable (dev-only) or implement |
| `dialog-config/debug/experiments/integration/mcp/open/pair/session-rename/shell-output` | missing | no panels/commands; `BUILTINS` 8 entries `commands.rs:76-85`; `/location` is the only open-like flow `commands.rs:60` | one dialog slice per upstream component or documented deviation |
| `dialog-image-preview` | not-applicable | image input rejected `bootstrap.rs:43-48` | none while text-only profile stands |

## 4. Test infrastructure and what golden snapshots need

- `pty_t39.rs` gives: isolated `Fixture` (tempdir HOME/XDG, fake config, scripted Responses peer) 29-158; `script()` 167;
  `openpty_pair(cols,rows)` with real `winsize` 337-363; `PtySession::{spawn,spawn_bad_stdout}` 383/402, reader thread 434,
  `snapshot` 453, `send` 461, `wait_visible[_after]` 466-495 (CSI-stripped, whitespace-insensitive byte needle),
  `resize` via TIOCSWINSZ 497-513, `restored()` asserting slave termios `ICANON|ECHO` 516-524, `wait_exit` 526-548, and
  `ALT_LEAVE` (`\x1b[?1049l`) proof 22/879.
- Screen reconstruction: `Screen` grid 562-602 + `render_screen` 605-719 parse CUP/moves/`K`/`J`/save-restore/OSC/CR-LF into
  `Vec<Vec<char>>` capped 200x400; helpers `wait_screen_row` 722, `submit()` which asserts `you: {text}` on the grid 775,
  persistence seeds 783/788/806. Used by aud29 818, aud30 932, aud31 997. `pty_t42.rs:520-700` duplicates this parser and
  aud38 962-1000 asserts diff rows and absence of `*** Begin Patch` on the grid.
- Unit render: `views::render_test` (`views.rs:66-80`) draws on `TestBackend` and returns symbols only; used at 248/267/331.
- Missing for golden snapshots: (a) a style-capturing render helper (fg/bg/modifiers per cell) — `render_test` drops styles;
  (b) a deterministic state fixture that does not require `CoreApp::spawn` + `std::mem::forget(guard)` (`app.rs:1057-1062`);
  (c) pinned size (TestBackend sizes are explicit, PTY uses `openpty_pair(80,24)`) and pinned session id
  (`s-tui-{nanos}` `tui_cmd.rs:60,453` must not leak into goldens); (d) SGR-aware PTY emulation (current `Screen` ignores
  colors and wide-char widths, handles only CSI subset, no scroll regions/`?25l`); (e) golden storage convention (no
  `insta`/snap files today); (f) a screen-dump probe analogous to `OC_TUI_TEST_METRICS` (`tui_cmd.rs:438-451`) if PTY goldens
  are chosen.

## 5. Risks / constraints for a parity effort

1. **Plain `Vec<String>` view contract.** `views::panel_lines`, `DcpPanelState::panel_rows`, `ModelPicker::window` and
   `TuiState::viewport` all return text (`views.rs:83-153`, `dcp_panel.rs:111`, `picker.rs:120`, `app.rs:313`) and their unit
   tests assert exact strings. Any styled/spanned rendering forces an API change through `app.rs`, `picker.rs`, `dcp_panel.rs`
   and every panel test — the largest structural cost of parity.
2. **Missing core/provider data.** `HistoryMessage`/`HistoryPage` carry only `role+text` (`queries.rs:16-36`) and `CoreEvent`
   has no reasoning/usage/tool-call/status events (`core_app.rs:26-70`). Reasoning blocks, live usage/context counters, and
   inline tool cards need `oc-core`/adapter changes, crossing the UI/core boundary (AGENTS.md invariant; A04 already expects
   reasoning metadata, so the data may exist upstream of `CoreEvent` but is not projected to the UI).
3. **Terminal restore contracts are audited.** `TerminalGuard`+panic hook (`terminal.rs:15-55`) and PTY assertions
   (`pty_t39.rs:516-524,879-880,960-961,989`) must stay green; adding bracketed paste/mouse/keyboard-enhancement must be
   paired with disable-on-restore. Note `enter()` writes to stderr while the backend draws to stdout (`terminal.rs:29`,
   `tui_cmd.rs:91`) — keep the broken-stdout error path (aud30) intact.
4. **Snapshot flakiness.** The binary redraws on a 50 ms poll (`tui_cmd.rs:124`), streams asynchronously, mixes pane titles
   with transient notes, and PTY screen parsing is a partial emulator without SGR/wide-char/scroll-region support
   (`pty_t39.rs:605-719`). Golden tests should render `TuiState` on `TestBackend` with pinned size/state; PTY goldens need a
   proper VT emulator to avoid false diffs.
5. **Bounded-state invariants must survive richer rendering.** Row/byte caps (`history.rs:15-21`), input budget
   (`app.rs:26-31`), cards cap `CARDS_MAX=160`, and the T39 metrics assertions (`pty_t39.rs:1035-1056`) constrain any change
   that keeps spans/styled segments in state instead of strings.
6. **DCP flow coupling.** Manual compress sets `panel=Dcp`, clears input/live text (`app.rs:440-448`), the loop refreshes the
   snapshot on open (`tui_cmd.rs:139-145`), and outcomes come from recorded `compress` ops (`tui_cmd.rs:409-435`); UI04
   behavior and its PTY test (`pty_t39.rs:860-866`) must be preserved by any dialog/stack redesign.
7. **Scope tension.** `GOAL.md` A08 explicitly says "Pixel parity не требуется", while `docs/goals/2026-09-21-tui-pixel-parity.md`
   declares pixel parity active (R2-R5). Slices must reconcile the two or record an architectural decision per AGENTS.md.
