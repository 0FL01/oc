# OpenCode v2.0.12 TUI — Upstream Inventory (for the Rust/ratatui port)

Source: `https://github.com/anomalyco/opencode` tag `v2.0.12`, tree SHA `2670273ff17da96f85c5826ced57aa1b368754fa`.
Verified by shallow clone; all citations are `path:line` at that tag. Renderer is SolidJS + `@opentui/core`/`@opentui/solid` (terminal renderer, not a browser DOM).
Scope note: v2.0.12 ships two UIs — the full-screen TUI in `packages/tui/src/**` (this document) and a scrollback "mini" renderer in `packages/tui/src/mini/**` (`mini/splash.ts`, `mini/footer.*`, `mini/theme.ts`), which is out of scope here.

## 1. Component inventory

### Layout / shell
| Component | Purpose / where rendered | Source |
|---|---|---|
| `run` / `App` | root provider tree; keyboard, client, theme, dialogs, toasts | `packages/tui/src/app.tsx:204`, `:467` |
| root frame | full-terminal column box, bg `theme.background.base`; row of vertical tabs + main column; overlays below | `app.tsx:1310-1386` |
| `Home` | centered logo + prompt; `home.footer` slot; form overlay | `packages/tui/src/routes/home.tsx:80-121` |
| `SessionFrame` | session pane + right pane (sidebar/terminal/panel) + resize handle | `packages/tui/src/component/session-frame.tsx:261-398` |
| `Session` | transcript scrollbox + 1-row jump status + bottom stack | `packages/tui/src/routes/session/index.tsx:1245-1424` |
| `Sidebar` | 42-col raised panel: shimmering title, `sidebar.content`/`sidebar.footer` slots | `packages/tui/src/routes/session/sidebar.tsx:20-77` |
| `SessionTabs` / `SessionTabsRail` | horizontal/vertical tab strip, tooltip, drag reorder, context menu | `packages/tui/src/component/session-tabs.tsx:497-513`, `session-tabs-rail.tsx:22` |
| `Logo` | two-tone block art, variants by width/height | `packages/tui/src/component/logo.tsx:52-77`; art in `logo.ts:1-9` |
| `DevToolsBar` | 1-row dev bar, panels Server/UI/Theme/Tools | `packages/tui/src/component/devtools-bar.tsx:232-361` |
| `Toast` | single toast, top-right, queue + hover pause | `packages/tui/src/ui/toast.tsx:45-118` |
| `StartupLoading` | centered spinner, delayed 500 ms, min 3 s display | `packages/tui/src/component/startup-loading.tsx:8-58` |
| `Reconnecting` | full-screen overlay while disconnected | `packages/tui/src/component/reconnecting.tsx:10-35` |
| `MigrationOverlay` | v1→v2 data migration progress | `packages/tui/src/component/migration-overlay.tsx:50` |
| `ErrorComponent` | crash screen with copy/GitHub issue actions | `packages/tui/src/component/error-component.tsx:113-211` |
| `PluginRouteMissing` | fallback for unknown plugin routes | `packages/tui/src/component/plugin-route-missing.tsx:7-17` |
| `PanelHost`, `TerminalPane`, `FormPrompt` | right-pane host, embedded PTY, MCP elicitation form | `component/panel-host.tsx`, `terminal-pane.tsx`, `routes/session/form.tsx` |

### Message rendering
`SessionRowView` dispatches rows: `message`, `part`, `group(reasoning)`, `group(exploration)`, `assistant-footer`, `turn-usage`, `compaction-queued` (`routes/session/index.tsx:1433-1481`). Sub-views: `SessionMessageView` (`:1676`), `SessionPartView` (`:1708`), `ReasoningPart`/`TextPart` (`component/../routes/session/message-parts.tsx:20,147`), `SessionReasoningGroupView` (`:1742`), `SessionGroupView` (`:1865`), `AssistantFooter` (`:1934`), `SessionSwitchMessageV2` (`:1986`), `SessionNoticeMessageV2` (`:2016`), `SessionSkillMessage` (`:2075`), `CompactionMessage` (`:2084`), `CompactionQueued` (`:2152`), `RevertMessage` (`:2172`), `ShellMessage` (`:2254`), `UserMessage` (`:2273`), `QueuedPromptDock` (`:2400`), `AssistantRetry` (`:2432`), `ToolPart` (`:2461`), `TurnTokenUsage` (`:1484`), `BackgroundToolHint` (`:1642`). Tool renderers: `GenericTool` `:2620`, `InlineTool` `:2688`, `BlockTool` `:2784`, `ShellDisplay` `:2884`, `Write` `:3044`, `Glob` `:3080`, `Read` `:3093`, `Grep` `:3127`, `WebFetch` `:3140`, `WebSearch` `:3148`, `Subagent` `:3173`, `ExecuteCallView` `:3230`, `Edit` `:3324`, `ApplyPatch` `:3388`, `Question` `:3508`, `Skill` `:3544`, `Diagnostics` `:3553`; dispatch set `toolDisplays = {shell,glob,read,grep,webfetch,websearch,write,edit,subagent,execute,patch,question,skill}` (`:3583-3601`).

### Dialogs (exact titles)
`component/command-palette.tsx:64` "Commands"; `dialog-agent.tsx:22` "Select agent"; `dialog-config.tsx:380` "Settings"; `dialog-experiments.tsx:53` "Experiments"; `dialog-integration.tsx:129` "Connect an integration"; `dialog-mcp.tsx:141` "MCP servers", `:166` `MCP server: <name>`; `dialog-model.tsx:120-124` "Select model" / provider name; `dialog-open.tsx:321` "Open"; `dialog-session-list.tsx:197` "Sessions"; `dialog-session-rename.tsx:14` "Rename session"; `dialog-skill.tsx:58` "Skills"; `dialog-stash.tsx:60` "Stash"; `dialog-theme-list.tsx:25` "Themes"; `dialog-update.tsx` "Confirm update action"; `dialog-variant.tsx:35` "Select variant"; `dialog-workspace-file-changes.tsx:76` "File Changes Found"; `dialog-workspaces.tsx:327` "Worktrees"; `dialog-worktree-name.tsx:66` placeholder "Worktree name"; `routes/session/dialog-fork.tsx:82` "Fork session"; `dialog-message.tsx:25` "Message Actions"; `dialog-timeline.tsx:41` "Timeline"; `ui/dialog-help.tsx:23` "Help"; `ui/dialog-prompt.tsx:123` placeholder "Enter text"; `ui/dialog-select.tsx:675` placeholder "Search"; `ui/dialog-export-options.tsx:121` "Markdown"/"JSON". `dialog-status.tsx:29` "No MCP servers"; `dialog-debug.tsx:28-33` rows Version/Date/Terminal/Session ID/Model; `dialog-pair.tsx` sets `setSize("large")` (`:25`).

### Status / indicators
`Spinner` braille frames `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` at 80 ms, static fallback `⋯` (`component/spinner.tsx:14-51`, `spinner-frames.ts:1`); `StatusBadge` `" Background "` (`index.tsx:2761-2770`); home footer MCP/plugin/version (`feature-plugins/home/footer.tsx:15-104`); prompt footer usage/cost/subagents (`feature-plugins/prompt/footer.tsx:34-105`); sidebar Context block (`feature-plugins/sidebar/context.tsx:20-41`); session jump status (`index.tsx:1331-1350`); `OneCellSpinner`, `tab-pulse.tsx`, `title-shimmer.tsx`, `shimmer-text.tsx`, `fade-in-text.tsx`, `retry-provider.tsx` (animation-only components).

### Input / editor
`Prompt` textarea box (`component/prompt/index.tsx:1647-1870`), `PromptMetadataRow` (`component/prompt/metadata.tsx:8-87`), `PromptInterruptStatus` (`prompt/index.tsx:108-145`), `Autocomplete` (`component/prompt/autocomplete.tsx`), `Composer` tab host with subagents/shell/terminals tabs (`routes/session/composer/index.tsx:44-60`), `PermissionPrompt` (`routes/session/permission.tsx:156-260`), `FormPrompt` (`routes/session/form.tsx`), `DialogSelect` list engine (`ui/dialog-select.tsx:103`).

## 2. Theme and colors

Default theme name `"opencode"` (`context/theme.tsx:133,155`), default mode `"dark"` (`context/theme.tsx:131`), loaded from the v2 asset (`theme/index.ts:5` imports `./assets/v2/opencode.json`); `system` falls back to `opencode` (`theme/index.ts:40`). Theme selection: `config.theme.name ?? "opencode"`; modes resolved by `selectThemeMode` (`packages/theme/src/tui/select.ts:8-17`).

v2 token model (`packages/theme/src/tui/schema.ts:194-261`), 103 leaf color slots:
- `text.{base,muted}`; `text.action.{primary,secondary,destructive}.{base,$hovered,$focused,$pressed,$selected,$disabled}`; `text.formfield.<same 6>`; `text.feedback.{error,warning,success,info}.{base,muted}` (34)
- `background.{base}`; `background.raised.{base,high,max}`; `background.action.*`; `background.formfield`; `background.feedback.*` (32)
- `border.base`, `scrollbar.base` (2)
- `diff.text.{added,removed,context,hunkHeader}`, `diff.background.{added,removed,context}`, `diff.highlight.{added,removed}`, `diff.lineNumber.text`, `diff.lineNumber.background.{added,removed}` (12)
- `syntax.{comment,keyword,function,variable,string,number,type,operator,punctuation}` (9, `schema.ts:129-139`)
- `markdown.{text,heading,link,linkText,code,blockQuote,emphasis,strong,horizontalRule,listItem,listEnumeration,image,imageText,codeBlock}` (14, `schema.ts:144-160`)
- hues: 8 base (`gray,red,orange,yellow,green,cyan,blue,purple`) + 3 aliases (`accent,interactive,neutral`) × 9 steps 100–900 (`schema.ts:3-10`), plus `categorical` hue list.
- surface override `"@dialog"` (`schema.ts:267`) merged over base (`resolve.ts:65,81-90`).

Exact defaults, v2 `opencode` asset (`packages/tui/src/theme/assets/v2/opencode.json`): base tokens `text.base "$hue.neutral.200"`, `text.muted "$hue.neutral.400"`, `border.base "#b8b8b8"`, `scrollbar.base "#a0a0a0"`, diff light `added #1e725c / removed #c53b53 / context #7086b5`, diff bg `#d5e5d5 / #f7d8db / $hue.neutral.700`, highlight `#4db380 / #f52a65`, lineNumber `#595959`, bg `#c5d5c5 / #e7c8cb` (`:92-121`). Syntax base: `comment $hue.neutral.400, keyword $hue.accent.200, function $hue.interactive.200, variable $hue.red.200, string $hue.green.200, number $hue.accent.200, type #b0851f, operator $hue.cyan.200, punctuation $hue.neutral.200` (`:122-132`). Markdown base: `heading $hue.accent.200, link $hue.interactive.200, linkText $hue.cyan.200, code $hue.green.200, blockQuote/emphasis #b0851f, strong $hue.accent.200, horizontalRule $hue.neutral.400, listItem $hue.interactive.200, listEnumeration $hue.cyan.200, image $hue.interactive.200, imageText $hue.cyan.200, codeBlock $hue.neutral.200` (`:133-148`). Dialog surface: `background.base = $background.raised.base` (`:149-158`).
Hue scales: light `gray 100 #000000 … 800/900 #ffffff`, `red 200 #d1383d`, `green 200 #3d9a57`, `cyan 200 #318795`, `blue 200 #3b7dd8`, `purple 200 #7b5bb6`; aliases `accent→orange, interactive→blue, neutral→gray` (`:160-243`). Dark `gray 100 #ffffff, 200 #eeeeee, 300 #b5b5b5, 400 #808080, 500 #4c4c4c, 600 #1e1e1e, 700 #141414, 800 #0a0a0a, 900 #030303`; `red 200 #e06c75, orange 200 #fab283, green 200 #7fd88f, cyan 200 #56b6c2, blue 200 #5c9cf5, purple 200 #9d7cd8`; aliases `accent→purple, interactive→orange, neutral→gray` (`:245-328`). Dark token overrides: `border #484848`, `scrollbar #606060`, diff `added #4fd6be / context #828bb8`, bg `#20303b / #37222c`, highlight `#b8db87 / #e26a75`, lineNumber `#8f8f8f`, bg `#1b2b34 / #2d1f26`; syntax `number #f5a742, type #e5c07b`; markdown `blockQuote/emphasis #e5c07b, strong #f5a742`; warning text `#f5a742` (`:336-388`).

Resolution: hex/`transparent`/`$ref` (incl. `$hue.<name>.<step>`); refs cached, cycles rejected (`resolve.ts:224-264`); `expandTheme` fills missing states with base and feedback `muted` with base (`expand.ts:39-100`); stateful colors expose `state()` picking first of disabled/pressed/focused/selected/hovered (`resolve.ts:145-156`, order `schema.ts:15`); `increase/decrease` shift within the hue's 9-step scale (`resolve.ts:158-180`). Values become `RGBA` from `@opentui/core`; `ansiToRgba` maps 0–255 palette indices when a v1 theme uses numbers (`packages/tui/src/theme/color.ts:3-41`); `tint()` alpha-blends (`:43-47`). Syntax attributes: `generateSyntax` maps tree-sitter scopes → fg + bold/italic/underline/background (e.g. `markup.raw.inline` gets `background: theme.background.base`, `comment` italic, `keyword.type` bold+italic) (`packages/theme/src/tui/syntax.ts:10-85`).

Legacy v1 roles (still used by v1 assets and `mini`): `primary, secondary, accent, error, warning, success, info, text, textMuted, selectedListItemText, background, backgroundPanel, backgroundElement, backgroundMenu, border, borderActive, borderSubtle, diffAdded, diffRemoved, diffContext, diffHunkHeader, diffHighlightAdded/Removed, diffAddedBg/RemovedBg/ContextBg, diffLineNumber, diffAddedLineNumberBg, diffRemovedLineNumberBg, markdown*, syntax*` (`packages/theme/src/tui/v1.ts:3-58`); v1 JSON is migrated via `migrateV1` (`theme/index.ts:62`); v1 defaults `DEFAULT_THEMES` + 30 assets in `packages/tui/src/theme/assets/*.json` (v1 `opencode.json` not inspected → undetermined values).

## 3. Layout geometry

Root: `width/height = dimensions()`, `flexDirection="column"`, bg base (`app.tsx:1310-1314`). Main row `flexGrow=1, minHeight=0, flexDirection="row"` (`:1327-1335`): optional vertical `SessionTabs` (resizable, `:1337`), main column `flexGrow=1` containing optional horizontal tabs (`:1342-1343`) and route `Switch` (`:1345-1366`), then `<Slot path="app"/>`; `PaneResizeHandle` (`:1372`); `DevToolsBar` (`:1376`); `StartupLoading`, `Reconnecting`, `MigrationOverlay`, `Toast` are absolute overlays (`:1378-1385`).

Home: centered column, `paddingLeft/Right = width<44 ? 1 : 2`; spacer `height=3`; logo; `height=1`; update notice; prompt `width="100%" maxWidth={75} paddingTop={1}`; flexible spacers; footer slot (`routes/home.tsx:82-107`); form overlay `position="absolute" zIndex=2000 bottom=1 padding 2` (`:112`).

Session: content box `paddingBottom=1`, `paddingLeft/Right = width<44 ? 1 : 2` (`index.tsx:1273-1279`); scrollbox `stickyScroll stickyStart="bottom"` (`:1299-1300`); scrollbar `paddingLeft=1`, track bg `decrease(background.raised.base)`, fg `border.base` (`:1291-1297`); status row `height=1`, right-aligned (`:1331-1350`); bottom stack: `QueuedPromptDock` → `Slot session.composer.top` → `Composer` → permission/form/location-missing/`Prompt` (`:1351-1419`).

Input box: outer box `border=["left"]` with `customBorderChars {..., bottomLeft:"╹"}`; inner `paddingLeft/Right = width<44 ? 1 : 2`, `paddingTop=1`; textarea `minHeight=1`; then a 1-row `╹` + `▀` underline when prompt bg is opaque (`prompt/index.tsx:1650-1671,1736-1870`). Prompt sits in `maxWidth=75` on Home; full width in session.

Dialogs: backdrop full-screen `position="absolute" zIndex=3000`, `backgroundColor rgba(0,0,0,150)`, `paddingTop = centered ? 0 : height/4`, `alignItems="center"`; panel widths `medium 60 / large 88 / xlarge 116`, `maxWidth=width-2`, `paddingTop=1`, bg `theme.surface("dialog").background.base` (`ui/dialog.tsx:14-18,34-70`).

Sidebar/panes: `SESSION_SIDEBAR_WIDTH=42`, compact tabs 5, max tabs 72, content min 44/preferred 64 (`ui/layout.ts:1-16`); pane width clamp keeps ≥24 or equal half (`:19-23`); sidebar auto-shows when width−tabs > 120 (`session-frame.tsx:106-111`); narrow sidebar overlay bg `rgba(0,0,0,70)` (`:392`). Toast `absolute top=1 right=2 maxWidth=min(60,width-6)` (`ui/toast.tsx:46-52`). DevTools bar `height=1` (`devtools-bar.tsx:232`).

Resize: everything reads reactive `useTerminalDimensions()`; breakpoints 44 (narrow padding/hints), 60 (dialog footer column), 80 (footer hints), 120 (sidebar auto, diff split) — `index.tsx:1335,1277`; `ui/dialog-select.tsx:816`; `home/footer.tsx:7-13`; `index.tsx:3335`.

## 4. Message rendering

Row spacing: each row `marginTop=1` (`index.tsx:1435`). Assistant text: `paddingLeft=3`, `<markdown fg={theme.markdown.text} bg={theme.background.base} tableOptions={{style:"grid",cellPaddingX:1}} conceal={markdownMode==="rendered"} streaming=...>` (`message-parts.tsx:156-171`). Reasoning: `paddingLeft=3`, left border `┃` colored `theme.decrease(theme.background.base)`; live header spinner "Thinking" / "Thinking: <summary>"; done header "Thought: <summary> · <duration>"; hide mode collapses to `+ / -` single line; body is muted markdown (`message-parts.tsx:49-145`).

User message: left border `┃` in agent color (muted border while queued), bg `background.raised.base`, padding `1/2`, hover darkens (`index.tsx:2298-2345`); skill chips render ` skill ` with `hue.accent[200|300]` bg and name on `decrease(background.raised.base)` (`:2346-2367`); file/dir chips same style with `file`/`dir` label (`:2368-2393`). Assistant footer: `paddingLeft=3`, `<Agent>` titlecased in agent color, then ` · <model>`, ` · <duration>`, ` · <N> tok/s`, ` · interrupted` in `text.muted` (`:1963-1981`); error line `Error: <message>` in `text.feedback.error.base` (`:1957-1961`); retry `⚠ Retrying in <n>s · attempt <n> · <error>` in warning (`:2445-2455`). Switch notices `↳ Moved to <dir>` / "Switched agent to X" muted (`:1986-2013`); subagent/shell completion `↳ <Actor> finished|failed` colored info/error/warning with `!` on non-complete (`:2034-2069`); skill message `→ Skill <name>` (`:2075-2081`). Compaction: full-width top rule, spinner, "Compaction" or "Provider compaction", `· cancelled`, `· N in · N out` (`:2104-2148`); queued variant "◇ Compaction queued" (`:2152-2163`). Revert banner + "Revert"/restore actions (`:2172-2253`). Turn usage: bold "Tokens" summary, expandable per-step table (`:1484-1560`).

Tools: inline tools are one line at `paddingLeft=3` with a 2-cell icon column; pending shows spinner + label; failed uses `text.feedback.error.base`; denied adds strikethrough; click toggles error (`message-parts.tsx:176-253`, `index.tsx:2688-2759`). Block tools (shell/edit/write/patch/question) use a left `┃` border, bg `background.raised.base` (hover darkens), padding `1/2`, gap 1, optional path header with `FilePath`, spinner in header, error line in error color (`:2784-2866`). Tool lines: shell `$ <cmd>` (running: spinner, no `$`), `cd <workdir> && ` prefix, muted output collapsed to 10 lines, `[earlier output omitted]`, `Writing command…`, `Background` badge (`:2980-3037`); write `# Wrote <path>` with line numbers + syntax code, running `Preparing write…` / `Write <path>` (`:3044-3077`); glob `Glob "<pattern>" in <path> (N matches)` pending `Finding files…` (`:3080-3090`); read `Read <path>` + `↳ Loaded <path>`, pending `Reading file…` (`:3093-3124`); grep `Grep "<pattern>" in <path> (N matches)`, pending `Searching content…` (`:3127-3137`); webfetch `WebFetch <url>`, pending `Fetching from the web…` (`:3140-3145`); websearch `Web Search via <provider> "<query>"`, pending `Searching web…` (`:3148-3170`); subagent `<Agent> Subagent — <desc> · <model>` with icon `↳`/`│`/`✓`, pending `Delegating…`, `Background` badge (`:3173-3205`); execute renders tool calls (`:3230`); edit `← Edit <path>` + `PatchDiff` split/unified (`auto: width>120`) with diff token colors (`:3341-3384`); apply_patch per-file `# Created`/`← Patched`/`# Deleted`, `-N lines` for deletes, `# Patch failed` on error (`:3415-3503`); question `Asked N question(s)` / `# Questions` block with answers (`:3508-3541`); skill `Skill "<name>"` (`:3544-3550`); diagnostics `Error [line:col] <msg>` (`:3564-3576`); generic `✓/✗ <tool> <args>` + expandable `key: value` / `output:` (`:2620-2669`). Exploration grouping label `Explored — 2 reads, 1 search` / `Exploring — …` with icon `→`/`✱` (`:1889-1922`). Background hint: `Press <key> to move running work to the background` after 3 s (`:1663-1672`). Shell message errors: "Command cancelled"/"Command timed out"/"Command exited with code N" (`:2254-2260`).

Markdown specifics come from the `<markdown>`/`<code>` renderables of `@opentui/core` (grid tables, conceal, streaming); their internal layout is not in this repo → undetermined in detail. Thinking syntax override: `generateThinkingSyntax(syntax, theme.text.muted)` (`routes/session/thinking-syntax.ts`).

## 5. Interaction

Keymap source of truth: `packages/tui/src/config/keybind.ts` (`Definitions`, `:45-299`); leader default `ctrl+x` (`:41`); leader timeout default 2000 ms (`context/keymap.tsx:137-143`); modes `base`/`global`/`modal`/`menu` (`context/keymap.tsx:41`, `ui/dialog.tsx:92`, `session-tabs.tsx:377-380`). Selected defaults (`config/keybind.ts`): exit `ctrl+c,ctrl+d,<leader>q` (`:48`); palette `ctrl+p` (`:57`); help `none` (`:58`); interrupt `escape` (`:126`); background `ctrl+b` (`:127`); compact `<leader>c` (`:128`); queue prompt `<leader>return` (`:194`); clear input `ctrl+c` (`:202`); submit `return` (`:204`); newline `shift+return,ctrl+return,alt+return,ctrl+j` (`:205`); input movement/selection/deletion vim+readline set (`:206-241`); history `up/down` (`:240-241`); session scroll `pageup/pagedown`, `ctrl+alt+u/d` half page (`:175-180`); first/last `ctrl+g,home,alt+home` / `ctrl+alt+g,end` (`:181-182`); message copy `<leader>y` (`:188`); undo/redo `<leader>u` / `<leader>r` (`:189-190`); model list `<leader>m`, cycle recent `f2/shift+f2` (`:162-164`); agent list `<leader>a`, cycle `shift+tab` (`:169-170`); variant cycle `ctrl+t` (`:172`); sessions list `<leader>l`, new `<leader>n`, open menu `ctrl+o` (`:109-111`); rename `ctrl+r`, delete `ctrl+d`, pin `ctrl+f` (`:122-123,138`); tabs `ctrl+tab/alt+down` next, `ctrl+shift+tab/alt+up` prev, `ctrl+shift+t` reopen (`:112-119`); sidebar `<leader>b` (`:95`); terminal `<leader>t`, select `<leader>down`, close `<leader>up` (`:98-100`); theme switch `none` (`:92-94`); dialog nav `up/ctrl+p`, `down/ctrl+n`, `pageup/pagedown`, `home/end`, submit `return` (`:255-261`); autocomplete `tab` complete, `escape` hide (`:270-274`); `which-key` `ctrl+alt+k` etc. (`:288-298`). `prompt.submit`/`prompt.skills` etc. default `none`.

Command palette: `ctrl+p` opens `CommandPaletteDialog`; options are every keymap command with `palette: true` (`command-palette.tsx:17-35`), `Suggested` group first when unfiltered (`:49-61`), fuzzy filter threshold 0.7, footer shows group · shortcut. Slash commands appear through the prompt autocomplete: `/` prefix lists server commands (`autocomplete.tsx:489`), `@` lists files, agents, skills (`:412-453`); completion inserts `@path/` (`:662`) or command text.

Dialogs: `escape` closes (clears selection first) and `ctrl+c` closes or clears the focused input (`ui/dialog.tsx:115-156`); backdrop click closes (`:35-44`); select dialog auto-focuses a filter input (`ui/dialog-select.tsx:654-677`), rows highlight with `background.action.primary.focused`, current row marked `●` and colored `text.formfield.selected` (`:854-858`), empty states "No items available"/"No results found" (`:690-698`). Confirm dialog: left/right toggles, return confirms, buttons `Cancel`/`Confirm` titlecased (`ui/dialog-confirm.tsx:27-89`). Session list: `ctrl+f` pin, `ctrl+d` delete (title becomes `Press <key> again to confirm`), `ctrl+r` rename, `ctrl+a` all projects (`dialog-session-list.tsx:133,166,218,249,282-289`). Theme list previews on move and reverts on cancel (`dialog-theme-list.tsx:19-47`).

Editor: multiline textarea, bracketed paste normalized `\r\n`→`\n` (`prompt/index.tsx:1765-1788`), IME-safe submit double-defer (`:1759-1764`), `ctrl+v` paste (`config/keybind.ts:203`), history navigation (`:240-241`). Interrupt: first `escape` arms (`esc again to interrupt`, 5 s window), second calls `session.interrupt` (`prompt/index.tsx:515-527`, status text `:139-142`); shell mode `escape` returns to normal mode (`:985`). Dialogs push `modal` mode so global bindings pause (`ui/dialog.tsx:90-94`).

## 6. Visible strings (selection)

| String | Source |
|---|---|
| `Fix a TODO in the codebase` / `What is the tech stack of this project?` / `Fix broken tests`; shell: `ls -la`, `git status`, `pwd` | `routes/home.tsx:20-23` |
| `Thinking` / `Thinking: <summary>` / `Thought: <summary> · <duration>` | `routes/session/message-parts.tsx:120,124-141` |
| `esc interrupt` / `esc again to interrupt` | `component/prompt/index.tsx:139-142` |
| `Jump to latest ↓`, `Loading session history…` | `routes/session/index.tsx:1346,1333` |
| `Explored — …` / `Exploring — …` | `index.tsx:1899` |
| `Error: <message>`, `⚠ Retrying in Ns · attempt N · …`, ` · interrupted` | `index.tsx:1959,2450,1978` |
| `Command cancelled` / `Command timed out` / `Command exited with code N` | `index.tsx:2256-2259` |
| `Writing command…`, `[earlier output omitted]`, `Background` | `index.tsx:3001-3003,2976,3037` |
| `Preparing write…`, `Finding files…`, `Reading file…`, `Searching content…`, `Fetching from the web…`, `Searching web…`, `Delegating…`, `Asking questions…`, `Loading skill…`, `Patching`, `# Patch failed` | `index.tsx:3072,3083,3107,3130,3142,3152,3191,3536,3547,3496` |
| `Read <path>`, `↳ Loaded <path>`, `Glob "<p>" in <path> (N matches)`, `Grep …`, `WebFetch <url>`, `Web Search via <provider> "<q>"`, `Skill "<name>"` | `index.tsx:3112,3118,3084-3088,3131-3135,3143,3156-3168,3548` |
| `<Agent> Subagent — <desc> · <model>`, `Continue subagent` | `index.tsx:3203` |
| `Permission required`, `Allow once`, `Always allow`, `Reject`, `Reject permission`, `Tell OpenCode what to do differently`, `No diff provided`, `Patterns` | `routes/session/permission.tsx:204,219,325,328,82,174`; `util/permission.ts:162-168` |
| `This will always allow <action> for this project.` | `util/permission.ts:154-159` |
| `MCP server needs authentication` / `Connect "<name>" to use its tools.` / `MCP server failed: <name>` / `Run /mcps to view details.` / `Open MCP servers` | `app.tsx:544-557` |
| `Copied to clipboard`, `Restarting service…`, `Service restarted`, `Reloading configuration…`, `Configuration reloaded` | `app.tsx:582,997-1002,1014-1018` |
| `Loading plugins…`, `Finishing startup…` | `component/startup-loading.tsx:8` |
| `Your session will resume automatically.` / `Reconnecting to the server automatically.` | `component/reconnecting.tsx:34-35` |
| `Data migration failed` | `component/migration-overlay.tsx:32` |
| `An unexpected error stopped the session.`, `An unknown error occurred.`, `Copy the report and open a GitHub issue to help us fix this.` | `component/error-component.tsx:123,44,211` |
| `No preview` | `component/dialog-image-preview.tsx:63`; `prompt/index.tsx:1697` |
| `Untitled session`, `Rename`, `Copy session ID` | `component/session-tabs.tsx:1239,390,395` |
| unread markers `•` / `■`, attention `!` / `?` | `session-tabs.tsx:67-70,173-174` |
| `⊙ N MCP`, `⊙ N MCP failed`, `/mcps`, `/plugins`, `N plugins failed` | `feature-plugins/home/footer.tsx:29-45,66-70` |
| `N subagents`, `N shells`, `<ctx> · $cost`, `ctrl+p commands`, `shift+tab agents` | `feature-plugins/prompt/footer.tsx:25-32,90,103,97` |
| `Context`, `N tokens`, `N% used`, `$N spent` | `feature-plugins/sidebar/context.tsx:24-37` |
| `Sessions for <project>`, `Today`, `Pinned`, `Press <key> again to confirm` | `dialog-session-list.tsx:204,187,190,166` |
| `No items available`, `No results found` | `ui/dialog-select.tsx:690,697` |
| `Search` (filter placeholder) | `ui/dialog-select.tsx:675` |
| `Press <key> to see all available actions and commands in any context.`, `Help`, `ok`, `esc/enter` | `ui/dialog-help.tsx:31,23,41,26` |
| `+N more` (toast queue) | `ui/toast.tsx:95` |
| `Compaction`, `Provider compaction`, `· cancelled`, `◇ Compaction queued` | `index.tsx:2120-2125,2159` |
| `↳ Moved to <dir>`, `Switched agent to <Agent>` | `index.tsx:1993,2003` |
| `Patching`, `# Questions`, `Asked N question(s)`, `(no answer)` | `index.tsx:3496,3522,3537,3515` |
| `Error [line:col] <message>` | `index.tsx:3570` |
| `No MCP servers`, `Connected`, `Disabled in configuration` | `component/dialog-status.tsx:29,44,46` |
| `Debug info copied to clipboard` | `component/dialog-debug.tsx:45` |
| `Running`, `Timed out`, `Killed`, `Exited · code N` | `component/dialog-shell-output.tsx:84-87` |
| `Server`, `UI`, `Theme`, `Tools`, `Time to first draw` | `component/devtools-bar.tsx:273,320,345,361,439` |

## 7. Porting notes (ratatui)

Expressible directly: box/flex layout, Unicode borders (`┃ ╹ ▀ ▄`), truecolor RGB (`RGBA` → `Color::Rgb`; `theme.background.base` on root), bold/italic/underline/dim/strikethrough attributes, scrollboxes with sticky bottom, dialogs/modals/toasts/tabs, per-row `marginTop=1` spacing, spinner braille frames on an 80 ms tick, diff rendering (added/removed/context tokens), syntax highlighting via a tree-sitter port. Theme v2 can be implemented as a resolved token struct + hue-step shift; the `@dialog` surface is a second resolved palette.

Not directly expressible / needs approximation: alpha compositing (`rgba(0,0,0,150)` backdrop, `tint`, `decrease` with alpha) — ratatui has no alpha, so precompute opaque colors against the base background; animations (`shimmer-text`, `tab-pulse` ignition tweens 0.22 s, `marquee`, `fade-in-text`, `title-shimmer`) — emulate with frame ticks or the static fallbacks (`⋯`, `[⋯]`, DIM); mouse interactions (click targets, drag-to-reorder tabs, pane resize, copy-on-select, right-click copy) — ratatui mouse events can approximate clicks/drags but copy-on-select needs OS clipboard integration; inline images (kitty/iTerm protocols via `<image>`, `dialog-image-preview`, `imagePreviewHeight`) — render `No preview` placeholder or omit; terminal palette detection (OSC 4/10/11 queries, `getPalette`, `theme.mode system`) — cannot be reproduced from inside ratatui reliably, default to theme `opencode` dark. Per-character logo shadow tinting is expressible but should be precomputed per theme.

Undetermined: exact internals of `@opentui/core`'s `<markdown>`, `<code>`, `<line_number>`, `<textarea>` and the terminal attribute/escape emission; v1 theme asset default values (v1 `opencode.json` not inspected); `dialog-execute.tsx`, `devtools-bar.tsx` panel bodies and `feature-plugins/system/diff-viewer.tsx` (45 KB) were not exhaustively inventoried; `Locale` formatting (durations, truncation, titlecase) details; `mini` renderer is out of scope for this inventory.
