# V04 command capability map (2026-09-22)

Base HEAD: `1105f674e4284888e5675b0195b0df087d64b882`, T44 active.
Original source: `2670273ff17da96f85c5826ced57aa1b368754fa`, external checkout
`/home/opencode/.cache/opencode-tmp/oc-v2-src/packages/tui/src`.
This is a factual capability inventory, **not an owner-approved reduction of T44**.

## Native selectable registry

`crates/oc-tui/src/commands.rs::REGISTRY` contains only real native actions.
The palette, slash aliases and leader lookup share these action identities.
There are no selectable placeholders for unavailable original services.
Opening an empty native catalog is a genuine query, not a fabricated result.

| Original title / native action ID | Native owner and actual effect | Status / boundary |
|---|---|---|
| Show command palette / internal `OpenCommands` | `TuiState`, Ctrl+P, `/commands` opens focused shared selection overlay | Implemented; upstream does not list this action in its own palette |
| Switch session / `session.list` | `PanelIntent::LoadSessions`, `SwitchSession`; typed app query/history/session change | Implemented; Ctrl+X l, `/sessions`, `/session`, `/resume`; busy switch refused |
| Switch model / `model.list` | `LoadCatalog`, `ChooseModel` → `CoreAppHandle::select_model`, persisted exact ID/variant | Implemented; Ctrl+X m, `/model`, `/models`; busy selection refused |
| Switch agent / `agent.list` | `SelectAgent` → app registry primary-agent selection/prompt/model | Implemented; Ctrl+X a, Shift+Tab, `/agent`, `/agents`; not upstream agent cycling |
| Skills / `skill.list` | `LoadSkills` queries real skill cards, Enter uses existing native skill selection | Implemented; `/skills`; body exposure remains behind native tool permissions |
| Toggle sidebar / `session.sidebar.toggle` | Native `TuiState` sidebar setting; real layout change | Implemented, Ctrl+X b, `/sidebar`; original dynamic Show/Hide sidebar titles differ |
| Help / `help.show` | Existing native help information in shared modal | Implemented native help; original richer help contents remain a UI gap |
| Exit the app / `app.exit` | Native quit status, event-loop cleanup, terminal restoration | Implemented; Ctrl+C/Ctrl+D at root, `/quit`, `/exit`; modal Ctrl+C first clears filter/dismisses |
| Tool cards / `native.cards` (native addition) | `LoadCards` → actual paged stored tool operations and native card details | Implemented, `/cards`; not an alias for original Open diff viewer |

`Suggested` duplicates the genuine Switch session/Switch model identities; it
does not invent a separate action. The upstream Suggested New session, Connect
an integration, Open settings and Share session entries are intentionally not
selectable native commands. The resulting list is visibly different.

## Every title in the captured original Commands viewport

Evidence: `attempt-01/upstream/commands-over-session.{txt,png,cells.json}` and
the corrected native paired follow-up. Row coordinates here are zero-based.
The table accounts for all rows, including Suggested duplicates.

| Original rows | Title | Native status, action owner, boundary |
|---|---|---|
| 18, 26 | Switch session | Implemented above; native application |
| 19, 27 | New session | GAP: app `create_session` exists, but original new-tab/home/draft lifecycle has no equivalent V04 action; T44 UI owner |
| 20 | Switch model | Implemented above; native application |
| 21 | Connect an integration | UNSUPPORTED: native config/OpenProxy admission only; OAuth/integration authoring is outside GOAL; owner amendment needed |
| 22 | Open settings | UNSUPPORTED: no config-authoring UI; GOAL boundary, owner amendment needed |
| 23, 31 | Share session | UNSUPPORTED: no native sharing service/API; GOAL service boundary, owner amendment needed |
| 28 | Open session or project | GAP: separate native session picker and argument-taking `/location PATH` are real, but no original combined picker; T44 UI/application owner |
| 29 | Close tab | GAP: native single attached-session presentation, no upstream tab lifecycle; T44 UI/application owner |
| 30 | Reopen closed tab | GAP: no closed-tab stack/drafts; T44 UI/application owner |
| 32 | Rename session | GAP: no wired native rename action; title generation is not manual rename; T44 application owner |
| 33 | Jump to message | GAP: actual transcript scroll exists, no original searchable message selector; T44 UI owner |
| 34 | Fork session | GAP: T43 child sessions are not upstream historical fork; T44 application owner, preserve T43 semantics |

## Remaining original palette source inventory

Targeted sources: `app.tsx:707–1204`, `routes/session/index.tsx:812–1212`,
`component/prompt/index.tsx:259–633,873–927`, and the named feature plugins below.
Conditional titles are included even when the captured fixture does not expose
them. GAP/UNSUPPORTED entries are absent from the selectable native registry.

| Original command title(s) | Native owner/status | Boundary / missing semantics |
|---|---|---|
| MCP servers | T44/T45 application UI, GAP | Native MCP tools execute, but this management dialog is not wired; no OAuth pretending |
| Variant cycle; Switch model variant | Native model adapter, PARTIAL | Declared enabled variants work through Models ←/→, exact normal requests verified; separate original variant dialog/hotkey absent |
| View status | T44 UI, GAP | Native debug chrome is informational, not the original service status dialog |
| Update OpenCode | Owner, UNSUPPORTED | Publishing/auto-update outside GOAL |
| Pair device | Owner, UNSUPPORTED | Native pairing/remote service outside GOAL |
| Restart service | Owner, UNSUPPORTED | In-process native app has no original serve/attach service |
| Reload configuration | T44 application, GAP | Supported config loading/Location switching do not establish an original reload action |
| View debug info | T44 UI, GAP | Native Local/Packaged debug bar exists; original debug-info dialog absent |
| Switch theme | T44 UI, GAP | Pinned dark theme only; no original theme picker |
| Open docs | T44 UI, GAP | No native external-browser docs action |
| Toggle console | T44 UI, GAP | No original renderer console |
| Compact session | Owner/application, UNSUPPORTED equivalent | Native `/dcp-compress [focus]` is a real DCP operation, not original provider compaction; histories stay immutable |
| Unshare session | Owner, UNSUPPORTED | Sharing service absent |
| Undo previous message; Redo | Owner, UNSUPPORTED | Snapshot/undo outside GOAL; no fake transcript deletion |
| Copy last assistant message | T44 terminal integration, GAP | No clipboard/OSC52 action |
| Copy session ID | T44 terminal integration, GAP | No clipboard/OSC52 action |
| Copy session transcript | T44 terminal integration, GAP | No clipboard/OSC52 action |
| Export session transcript | T44 application/UI, GAP | No wired export command |
| Toggle subagent picker | T44 UI/T43 owner, GAP | Real T43 child execution/persistence preserved; original child-picker UI not wired |
| View queued prompts | T44 application/UI, GAP | No original queued-prompt steering lifecycle |
| Change working directory | Native application, PARTIAL | `/location PATH` works through existing permissions/generation switch; original `/cd` interactive picker not implemented |
| Queue prompt | T44 application/V05, GAP | No original queue UI |
| Remove editor context | T44 V05, GAP | No original editor context state |
| View image attachments | T44 V05, GAP | No original attachment inspection overlay |
| Open editor | T44 V05, GAP | External editor launch/restore not implemented |
| Manage workspaces | Owner/application, UNSUPPORTED equivalent | `/location` is not worktree creation/move management |
| Stash prompt; Stash pop; Stash list | T44 V05, GAP | Overlay draft preservation implemented; stash stack is separate |
| Ask a side question (`feature-plugins/prompt/btw.tsx`) | Owner/T43 boundary, GAP | Not silently mapped to a child agent or normal turn |
| Usage statistics (`feature-plugins/system/stats.tsx`) | T44 UI, GAP | DTO sidebar usage is real; aggregate statistics dialog absent |
| Open diff viewer (`feature-plugins/system/diff-viewer.tsx`) | T44 V06/UI, GAP | Native tool diff cards remain; no full source/hunk/review workflow |
| Plugins (`feature-plugins/system/plugins.tsx`) | Owner, UNSUPPORTED | No JS production host; admitted native plugin aliases remain config-only |
| Open storybook; Storybook: each registered story (`feature-plugins/system/storybook/index.tsx`) | T44 dev tooling, GAP | Debug plugin routes not production commands in native |
| Workspace-defined command titles (`routes/session/index.tsx:1200`) | Native application/T43, PARTIAL | Existing slash/template admission preserved; not added to V04 palette because invocation arguments and current-session command semantics need a separate UI contract |

## Original key-only commands (palette undefined)

These are recorded to avoid mistaking hidden entries for missing captured rows.

| Original titles | Native status / owner / boundary |
|---|---|
| Switch to session in quick slot 1…9; Switch to tab 1…10 | GAP, T44 tab/draft UI |
| Next tab; Previous tab; Next unread tab; Previous unread tab | GAP, T44 tab lifecycle |
| Model cycle; Model cycle reverse; Favorite cycle; Favorite cycle reverse | GAP, T44 model recents/favorites UI; genuine picker selection exists |
| Agent cycle; Agent cycle reverse | GAP, T44; Shift+Tab opens genuine selector rather than cycling |
| Switch to light mode; Switch to dark mode; Lock theme mode; Unlock theme mode | GAP, T44 theme configuration |
| Toggle debug panel | GAP, T44 renderer tooling; native debug info is not the original debug overlay |
| Suspend terminal | GAP, T44 terminal lifecycle |
| Enable/Disable terminal title; Enable/Disable animations | GAP, T44 runtime settings |
| Enable/Disable file context; Enable/Disable paste summary | GAP, T44 V05 |
| Enable/Disable diff wrapping | GAP, T44 V06 |
| Page up/down; Line up/down; Half page up/down; First/Last message | PARTIAL, T44 V03/V05; real rendered-row scroll exists, full keymap parity not claimed |
| Show/Hide sidebar (dynamic title) | PARTIAL, native Ctrl+X b toggle exists; original persistence/title variants differ |
| Toggle session scrollbar; Show tool calls individually; Group related tool calls | GAP, T44 transcript UI |
| Jump to last user message; Next/Previous message; Next/Previous user message | GAP, T44 transcript navigation |
| Background blocking tools | GAP, T43/application boundary; do not detach existing work silently |
| Go to parent session | GAP, T44 UI; parent relation comes from real T43 DTOs |
| Clear prompt; Submit prompt; Paste; Interrupt session | PARTIAL, native paths exist; complete multiline editor/key parity is V05 |
| Previous/Next prompt history; Shell mode; Exit shell mode | GAP, T44 V05 |

## Decision required for full parity

The owner must resolve original sharing/integrations/OAuth, settings authoring,
service pairing/restart/attach, updater, snapshots/undo, workspace management and
plugin-host controls against GOAL's explicit exclusions before full functional
parity can be asserted. An unavailable action cannot become selectable merely
because the original displays it. In-scope UI gaps still require implementation
and qualification; this map does not waive any VIS gate.

Native autoaccept remains unsupported as documented by V02. No autoaccept
permission bypass, service-attach shim, provider-family fallback or production
JS dependency was added. T43 and T45 architecture/permission ownership remains
unchanged. Modal search uses token-substring matching, not upstream fuzzysort;
favorites/recents, integration footer, mouse actions and variant-subdialog parity
remain visible gaps.
