//! Composer, key/panel commands and top-level pointer dispatch.

use super::*;

/// Mark a command as fully handled, or request its snapshot when the view
/// has never loaded one.
fn open_snapshot(outcome: &mut KeyOutcome, loaded: bool, intent: PanelIntent) {
    if loaded {
        outcome.consumed_input = true;
    } else {
        outcome.intent = Some(intent);
    }
}

fn clamp_cursor(cursor: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = cursor as isize + delta;
    next.clamp(0, len as isize - 1) as usize
}

impl TuiState {
    /// Open panel, if any.
    pub fn panel(&self) -> &TuiPanel {
        &self.panel
    }

    /// Safe options from actual snapshots. Filtering never changes runtime selection.
    pub fn modal_options(&self) -> std::rc::Rc<Vec<crate::dialog::SelectOption>> {
        use crate::dialog::SelectOption;
        let item =
            |value: String, title: String, category: &str, footer: String, current| SelectOption {
                value,
                title,
                category: category.into(),
                footer,
                current,
                running: false,
                destructive: false,
            };
        let options = match &self.panel {
            TuiPanel::MessageActions { .. } => [
                ("jump", "Jump to", "view message in session"),
                ("revert", "Revert", "undo messages and restore prompt"),
                ("copy", "Copy", "message text to clipboard"),
                ("fork", "Fork", "create a new session"),
            ]
            .into_iter()
            .map(|(value, title, description)| {
                item(value.into(), title.into(), "", description.into(), false)
            })
            .collect(),
            TuiPanel::Commands => {
                let mut options = Vec::new();
                // Use the last rendered terminal width and the same rail allocation
                // and auto breakpoint as shell::shell_regions/session_main.
                let sidebar_visible = !self.home
                    && self.parent_id.is_none()
                    && !self.chrome.sidebar_hidden
                    && self.viewport.get().is_some_and(|view| {
                        let session = crate::layout::configured_shell_regions(
                            Rect::new(0, 0, view.terminal_width, view.height),
                            self.chrome.devtools_visible(),
                            self.chrome.vertical_tabs_width,
                        )
                        .session;
                        crate::layout::sidebar_auto(session.width)
                    });
                if self.select.query.is_empty() {
                    options.extend(
                        crate::commands::REGISTRY
                            .iter()
                            .filter(|c| {
                                c.id == "model.list"
                                    || ((c.id == "session.list" || c.id == "session.new")
                                        && !self.home)
                            })
                            .map(|c| {
                                item(
                                    c.id.into(),
                                    c.title.into(),
                                    "Suggested",
                                    self.command_footer(c),
                                    false,
                                )
                            }),
                    );
                }
                options.extend(
                    crate::commands::REGISTRY
                        .iter()
                        .filter(|c| {
                            c.registered(self.chrome.dcp.commands_enabled)
                                && (!c.action.is_terminal() || self.chrome.session_terminal)
                                && c.in_palette(
                                    self.picker.as_ref().is_some_and(|p| p.has_variants()),
                                )
                                && (c.action != CommandAction::CloseTab || !self.tabs.is_empty())
                                && (!matches!(c.action, CommandAction::RenameSession { .. })
                                    || self.command_unavailable(&c.action).is_none())
                        })
                        .map(|c| {
                            item(
                                c.id.into(),
                                if c.id == "session.toggle.thinking" && self.thinking_expanded {
                                    "Collapse thinking".into()
                                } else if c.id == "session.sidebar.toggle" {
                                    if sidebar_visible {
                                        "Hide sidebar".into()
                                    } else {
                                        "Show sidebar".into()
                                    }
                                } else {
                                    c.title.into()
                                },
                                c.group,
                                self.command_footer(c),
                                false,
                            )
                        }),
                );
                options
            }
            TuiPanel::Model => {
                return self.select.filter_for(
                    self.picker
                        .as_ref()
                        .map(|p| p.options())
                        .unwrap_or_default(),
                    &self.panel,
                );
            }
            TuiPanel::Settings => {
                let mut options = vec![item(
                    "permissions".into(),
                    "Permissions".into(),
                    "Session",
                    if self.chrome.permissions_auto {
                        "auto accept"
                    } else {
                        "prompt"
                    }
                    .into(),
                    false,
                )];
                if let Some(provider) = &self.chrome.provider {
                    options.push(item(
                        "provider".into(),
                        format!("Provider request — {}", provider.status.as_str()),
                        "Services",
                        provider.to_string(),
                        false,
                    ));
                }
                options.extend(self.chrome.plugins.entries.iter().enumerate().map(
                    |(index, plugin)| {
                        item(
                            format!("plugin:{index}"),
                            format!("{} — {}", plugin.label(), plugin.status.as_str()),
                            "Compiled plugins",
                            plugin.to_string(),
                            false,
                        )
                    },
                ));
                if self.chrome.plugins.omitted > 0 {
                    options.push(item(
                        "plugin:omitted".into(),
                        format!("{} additional plugin requests", self.chrome.plugins.omitted),
                        "Compiled plugins",
                        format!(
                            "Bounded display; {} failed/unsupported_plugin; active modules: {:?}",
                            self.chrome.plugins.omitted_failed, self.chrome.plugins.active_modules
                        ),
                        false,
                    ));
                }
                options.extend(
                    self.chrome
                        .service_diagnostics
                        .iter()
                        .enumerate()
                        .filter_map(|(index, diagnostic)| {
                            use oc_core::queries::ServiceKind;
                            matches!(
                                diagnostic.kind,
                                ServiceKind::Configuration
                                    | ServiceKind::Definition
                                    | ServiceKind::Storage
                                    | ServiceKind::Runtime
                            )
                            .then(|| {
                                item(
                                    format!("diagnostic:{index}"),
                                    format!(
                                        "{} — {}",
                                        diagnostic.code.as_str(),
                                        if diagnostic.code
                                            == oc_core::queries::ServiceCode::IgnoredSetting
                                        {
                                            "ignored"
                                        } else {
                                            "failed"
                                        }
                                    ),
                                    "Configuration diagnostics",
                                    diagnostic.to_string(),
                                    false,
                                )
                            })
                        }),
                );
                if self.chrome.service_diagnostics_omitted > 0 {
                    options.push(item(
                        "diagnostic:omitted".into(),
                        format!(
                            "{} additional diagnostics",
                            self.chrome.service_diagnostics_omitted
                        ),
                        "Configuration diagnostics",
                        "Bounded presentation; effective admitted configuration is unchanged"
                            .into(),
                        false,
                    ));
                }
                options
            }
            TuiPanel::Variant => self
                .picker
                .as_ref()
                .map(|p| p.variant_options())
                .unwrap_or_default(),
            TuiPanel::Agents => {
                let mut options = Vec::new();
                if let Some(saved) = self.chrome.selection.as_ref().filter(|issue| {
                    issue.diagnostic.code == oc_core::queries::ServiceCode::AgentUnavailable
                }) {
                    options.push(item(
                        "selection:unavailable".into(),
                        format!("Saved agent {} — unavailable", saved.requested),
                        "Agents",
                        saved.diagnostic.to_string(),
                        true,
                    ));
                }
                options.extend(self.agents.iter().map(|a| {
                    item(
                        a.id.clone(),
                        a.id.clone(),
                        "Agents",
                        a.description.clone(),
                        self.active_agent.as_ref() == Some(&a.id),
                    )
                }));
                options
            }
            TuiPanel::Sessions => self
                .sessions
                .iter()
                .map(|s| {
                    let entry = self.session_entries.iter().find(|entry| &entry.id.0 == s);
                    let mut option = item(
                        s.clone(),
                        entry.map_or_else(
                            || "New session — metadata unavailable".into(),
                            |entry| entry.title.clone(),
                        ),
                        entry.map_or("Update time unavailable", |entry| &entry.date_group),
                        entry
                            .and_then(|entry| entry.worktree.clone())
                            .unwrap_or_default(),
                        self.attached_session().is_some_and(|id| s == &id.0),
                    );
                    if self.session_delete_confirm.as_deref() == Some(s) {
                        option.title = "Press ctrl+d again to confirm".into();
                        option.destructive = true;
                    }
                    option.running = entry.is_some_and(|entry| entry.running);
                    option
                })
                .collect(),
            TuiPanel::Skills => self
                .skills
                .iter()
                .map(|s| {
                    item(
                        s.id.clone(),
                        format!("{} — {}", s.id, s.name),
                        "Skills",
                        s.description.clone(),
                        false,
                    )
                })
                .collect(),
            TuiPanel::Rename | TuiPanel::Accounts => Vec::new(),
            TuiPanel::Mcps => self.mcp_options(),
            TuiPanel::None => Vec::new(),
            _ => crate::views::panel_lines(self)
                .into_iter()
                .enumerate()
                .map(|(i, s)| item(i.to_string(), s, "", String::new(), false))
                .collect(),
        };
        self.select
            .filter_for(std::rc::Rc::new(options), &self.panel)
    }

    pub fn command_unavailable(&self, action: &CommandAction) -> Option<&'static str> {
        if action.is_terminal() {
            if !self.chrome.session_terminal {
                return Some("session terminals unavailable on this platform");
            }
            if self.attached_session().is_none() {
                return Some("no session yet");
            }
        }
        if self.linked_child().is_some()
            && matches!(
                action,
                CommandAction::NewSession
                    | CommandAction::CloseTab
                    | CommandAction::OpenSessions
                    | CommandAction::OpenAgents
                    | CommandAction::OpenModelPicker
                    | CommandAction::OpenVariants
                    | CommandAction::UndoConversation
                    | CommandAction::RedoConversation
                    | CommandAction::RenameSession { .. }
                    | CommandAction::SwitchLocation { .. }
                    | CommandAction::ReloadConfiguration
                    | CommandAction::CompactSession
                    | CommandAction::DcpCompress { .. }
                    | CommandAction::OpenPermissions
            )
        {
            return Some("linked child is read-only");
        }
        if !crate::commands::spec(action).registered(self.chrome.dcp.commands_enabled) {
            return Some("DCP commands disabled");
        }
        if matches!(action, CommandAction::DcpCompress { .. })
            && let Some(reason) = self.dcp.manual_refusal()
        {
            return Some(reason.reason());
        }
        if matches!(
            action,
            CommandAction::UndoConversation | CommandAction::RedoConversation
        ) {
            if self.session.is_none() {
                return Some("no session yet");
            }
            if self.parent_id.is_some() {
                return Some("child session is read-only");
            }
            if self.status == TuiStatus::PendingSubmission {
                return Some("submission pending");
            }
            if let Some((undo, redo)) = self.conversation_available {
                if *action == CommandAction::UndoConversation && !undo {
                    return Some("nothing to undo");
                }
                if *action == CommandAction::RedoConversation && !redo {
                    return Some("nothing to redo");
                }
            }
        }
        if *action == CommandAction::CloseTab {
            let (tabs, index, _) = self.tab_presentation();
            if tabs.is_empty() || (index >= tabs.len() && !self.home) {
                return Some("no tab to close");
            }
            if tabs.get(index).is_some_and(|tab| tab.busy) {
                return Some("tab busy; action unavailable");
            }
        }
        if matches!(action, CommandAction::RenameSession { .. }) {
            if self.home || self.session.is_none() {
                return Some("no session yet");
            }
            if self.parent_id.is_some() {
                return Some("child session is read-only");
            }
            if self.tabs.get(self.active_tab).is_some_and(|tab| tab.busy) {
                return Some("tab busy; action unavailable");
            }
        }
        if self.session.is_none()
            && matches!(
                action,
                CommandAction::OpenCards
                    | CommandAction::OpenDcp
                    | CommandAction::DcpCompress { .. }
                    | CommandAction::CompactSession
            )
        {
            return Some("no session yet");
        }
        crate::commands::spec(action).unavailable(
            self.is_busy(),
            self.picker.as_ref().is_some_and(|p| p.has_variants()),
        )
    }

    pub(super) fn command_footer(&self, command: &crate::commands::CommandSpec) -> String {
        self.command_unavailable(&command.action)
            .map(str::to_string)
            .unwrap_or_else(|| match command.action {
                CommandAction::UndoConversation => self.conversation_shortcut(true),
                CommandAction::RedoConversation => self.conversation_shortcut(false),
                _ => self.command_shortcuts(command).join(" "),
            })
    }

    /// The Commands hints and builtin chord resolver use the same effective
    /// leader projection. Direct keys and explicit owner overrides stay literal.
    fn command_shortcuts(&self, command: &crate::commands::CommandSpec) -> Vec<String> {
        if let Some(index) = command.action.terminal_binding() {
            return self.chrome.terminal_shortcuts.bindings[index]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
        }
        let override_binding = match command.action {
            CommandAction::UndoConversation => Some(self.conversation_shortcut(true)),
            CommandAction::RedoConversation => Some(self.conversation_shortcut(false)),
            CommandAction::OpenCommands => self.chrome.command_palette_shortcut.clone(),
            _ => None,
        };
        if let Some(binding) = override_binding {
            return binding
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty() && !key.eq_ignore_ascii_case("none"))
                .map(str::to_owned)
                .collect();
        }
        command
            .shortcuts
            .iter()
            .flat_map(|shortcut| {
                if let Some(suffix) = shortcut.strip_prefix("ctrl+x ") {
                    self.chrome
                        .conversation_shortcuts
                        .leader
                        .split(',')
                        .map(str::trim)
                        .filter(|leader| !leader.is_empty() && !leader.eq_ignore_ascii_case("none"))
                        .map(|leader| format!("{leader} {suffix}"))
                        .collect::<Vec<_>>()
                } else {
                    vec![(*shortcut).to_string()]
                }
            })
            .collect()
    }

    /// Effective owner-configured shortcuts; Some("") disables a binding.
    pub fn set_conversation_shortcuts(&mut self, undo: Option<String>, redo: Option<String>) {
        let previous = [
            self.conversation_shortcut(true),
            self.conversation_shortcut(false),
        ];
        self.conversation_bindings = [undo, redo];
        if previous
            != [
                self.conversation_shortcut(true),
                self.conversation_shortcut(false),
            ]
        {
            self.leader = None;
            self.clear_transcript_selection();
        }
    }

    pub(super) fn conversation_shortcut(&self, undo: bool) -> String {
        self.conversation_bindings[usize::from(!undo)]
            .clone()
            .unwrap_or_else(|| {
                crate::commands::spec(&if undo {
                    CommandAction::UndoConversation
                } else {
                    CommandAction::RedoConversation
                })
                .shortcuts
                .join(" ")
            })
    }

    /// Resolve configured direct keys before the generic editor mapping.
    /// Modal focus keeps ownership; leader sequences are resolved in handle_key.
    pub fn conversation_key(&mut self, event: crossterm::event::KeyEvent) -> Option<KeyAction> {
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
        if event.kind != KeyEventKind::Press {
            return None;
        }
        let binding = crate::events::binding(event)?;
        if self
            .leader_deadline()
            .is_some_and(|until| Instant::now() >= until)
        {
            self.leader = None;
        }
        let chord = self
            .leader
            .map(|_| format!("{} {binding}", self.leader_key));
        if self.chrome.session_terminal
            && self.panel == TuiPanel::None
            && let Some(index) = self
                .chrome
                .terminal_shortcuts
                .bindings
                .iter()
                .position(|s| {
                    s.split(',').any(|b| {
                        !b.is_empty()
                            && b.eq_ignore_ascii_case(chord.as_deref().unwrap_or(&binding))
                    })
                })
        {
            self.leader = None;
            return Some(
                [
                    KeyAction::TerminalFocusLeft,
                    KeyAction::TerminalFocusRight,
                    KeyAction::TerminalSelect,
                    KeyAction::TerminalToggle,
                    KeyAction::TerminalClose,
                ][index]
                    .clone(),
            );
        }
        let action = [true, false]
            .into_iter()
            .find(|undo| {
                self.panel == TuiPanel::None
                    && self.conversation_shortcut(*undo).split(',').any(|value| {
                        value
                            .trim()
                            .eq_ignore_ascii_case(chord.as_deref().unwrap_or(&binding))
                    })
            })
            .map(|undo| {
                if undo {
                    KeyAction::UndoConversation
                } else {
                    KeyAction::RedoConversation
                }
            });
        if action.is_some() {
            self.leader = None;
            return action;
        }
        if self.panel == TuiPanel::None
            && self
                .chrome
                .command_palette_shortcut
                .as_ref()
                .is_some_and(|shortcut| {
                    shortcut.split(',').any(|value| {
                        value
                            .trim()
                            .eq_ignore_ascii_case(chord.as_deref().unwrap_or(&binding))
                    })
                })
        {
            self.leader = None;
            return Some(KeyAction::Commands);
        }
        if self.leader.is_some() {
            let printable = match event.code {
                KeyCode::Char(value)
                    if !event
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    Some(value)
                }
                _ => None,
            };
            return Some(KeyAction::SequenceKey(binding, printable));
        }
        if self
            .chrome
            .conversation_shortcuts
            .leader
            .split(',')
            .any(|value| value.trim().eq_ignore_ascii_case(&binding))
            || [true, false].into_iter().any(|undo| {
                self.conversation_shortcut(undo).split(',').any(|value| {
                    value
                        .trim()
                        .split_once(' ')
                        .is_some_and(|(prefix, _)| prefix.eq_ignore_ascii_case(&binding))
                })
            })
        {
            self.leader_key = binding;
            return Some(KeyAction::Leader);
        }
        None
    }

    pub fn terminal_key(&mut self, event: crossterm::event::KeyEvent) -> Option<KeyAction> {
        if self.terminal_focused()
            && let Some(action) = self.conversation_key(event)
        {
            return Some(action);
        }
        if self.approvals.active().is_some() {
            return self
                .approvals
                .terminal_key(event, &self.chrome.permission_shortcuts);
        }
        use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
        if self.questions.active().is_some() {
            return self.questions.terminal_key(event);
        }
        if let Some(action) = self.conversation_key(event) {
            return Some(action);
        }
        if self.panel == TuiPanel::None
            && event.kind == KeyEventKind::Press
            && event.modifiers == KeyModifiers::CONTROL
        {
            if event.code == KeyCode::Char('s') {
                return Some(KeyAction::Shells);
            }
            if event.code == KeyCode::Char('g') {
                return Some(KeyAction::Children);
            }
            if (self.shells.open || self.children.open || self.linked_child().is_some())
                && event.code == KeyCode::Char('b')
            {
                return Some(KeyAction::ShellBackground);
            }
        }
        if self.panel == TuiPanel::Settings
            && event.kind == KeyEventKind::Press
            && event.modifiers == (KeyModifiers::CONTROL | KeyModifiers::SHIFT)
            && matches!(event.code, KeyCode::Char('c' | 'C' | 'i' | 'I'))
            && let Some(diagnostic) = self.selected_diagnostic()
        {
            if matches!(event.code, KeyCode::Char('c' | 'C')) {
                if let Err(error) = self.copy_message_text(diagnostic.to_string()) {
                    self.push_note(&error);
                }
            } else {
                // The draft is a read-only detail, never injected into the ordinary composer.
                self.push_note(&diagnostic.investigation_draft());
            }
            return None;
        }
        crate::events::map_key(event).filter(|action| {
            *action != KeyAction::Leader
                && !(*action == KeyAction::Commands
                    && self.panel == TuiPanel::None
                    && self.chrome.command_palette_shortcut.is_some())
        })
    }

    pub(crate) fn terminal_leader_bypass(&mut self, event: crossterm::event::KeyEvent) -> bool {
        if self
            .leader_deadline()
            .is_some_and(|until| Instant::now() >= until)
        {
            self.leader = None;
        }
        self.leader.is_some()
            || crate::events::binding(event).is_some_and(|binding| {
                self.chrome
                    .conversation_shortcuts
                    .leader
                    .split(',')
                    .any(|value| !value.is_empty() && value.eq_ignore_ascii_case(&binding))
            })
    }

    fn selected_diagnostic(&self) -> Option<oc_core::queries::ServiceDiagnostic> {
        let options = self.modal_options();
        let value = &options.get(self.select.cursor)?.value;
        if value == "provider" {
            return self.chrome.provider.as_ref()?.diagnostic.clone();
        }
        if let Some(index) = value
            .strip_prefix("plugin:")
            .and_then(|index| index.parse::<usize>().ok())
        {
            return self.chrome.plugins.entries.get(index)?.diagnostic.clone();
        }
        let index = value.strip_prefix("diagnostic:")?.parse::<usize>().ok()?;
        self.chrome.service_diagnostics.get(index).cloned()
    }

    /// Called only after a successful application selection. The original applies
    /// the model before replacing its dialog; Escape here never rolls it back.
    pub fn model_choice_applied(&mut self, snapshot: CatalogSnapshot) {
        let selecting_model = self.panel == TuiPanel::Model;
        self.apply_catalog(snapshot);
        if selecting_model
            && self.picker.as_ref().is_some_and(|p| {
                p.has_variants() && p.selection().is_some_and(|s| s.variant.is_none())
            })
        {
            self.open_variants();
        } else {
            self.close_panel();
        }
    }

    /// Owner ACK returns Settings to its unfiltered main list.
    pub fn permission_mode_applied(&mut self) {
        if self.panel == TuiPanel::Settings {
            self.select.reset();
        }
    }

    pub(super) fn open_variants(&mut self) {
        self.panel = TuiPanel::Variant;
        self.tab_view.get_mut().reset_hover();
        self.close_hold = None;
        self.last_mouse = None;
        // A press belongs to the dialog where it began, not the replacement.
        self.mouse_down = None;
        self.select.reset();
        self.toast_down = false;
        self.select.cursor = self
            .modal_options()
            .iter()
            .position(|o| o.current)
            .unwrap_or(0);
        // DialogSelect centers its current option after the first layout, not
        // only when that option falls outside the viewport.
        self.select.follow_selection();
    }

    fn changed_modal_query(&mut self) {
        self.session_delete_confirm = None;
        self.select.changed_query();
        if self.panel == TuiPanel::Variant && self.select.query.is_empty() {
            self.select.cursor = self
                .modal_options()
                .iter()
                .position(|o| o.current)
                .unwrap_or(0);
            self.select.follow_selection();
        }
        self.sync_modal_cursor();
    }

    pub(super) fn sync_modal_cursor(&mut self) {
        let options = self.modal_options();
        self.select.cursor = self.select.cursor.min(options.len().saturating_sub(1));
        let Some(option) = options.get(self.select.cursor) else {
            return;
        };
        match self.panel {
            TuiPanel::Model => {
                if let Some(p) = &mut self.picker {
                    p.focus_id(&option.value);
                }
            }
            TuiPanel::Agents => {
                self.agents_cursor = self
                    .agents
                    .iter()
                    .position(|a| a.id == option.value)
                    .unwrap_or(usize::MAX)
            }
            TuiPanel::Sessions => {
                self.sessions_cursor = self
                    .sessions
                    .iter()
                    .position(|s| s == &option.value)
                    .unwrap_or(0)
            }
            TuiPanel::Skills => {
                self.skills_cursor = self
                    .skills
                    .iter()
                    .position(|s| s.id == option.value)
                    .unwrap_or(0)
            }
            _ => {}
        }
    }

    /// Current input buffer.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Prompt layout and the insertion caret share the same grapheme/cell model.
    pub fn prompt_layout(&self, width: usize) -> (Vec<crate::editor::PromptRow>, (usize, usize)) {
        self.editor.layout(&self.input, width)
    }

    pub(crate) fn clear_prompt_paint(&self) {
        self.painted_prompt.borrow_mut().take();
    }

    pub(crate) fn observe_prompt_paint(
        &self,
        frame: Rect,
        input: Rect,
        top: usize,
        rows: &[crate::editor::PromptRow],
    ) {
        self.prompt_width.set(Some(input.width as usize));
        if self.panel != TuiPanel::None || self.approvals.active().is_some() {
            return;
        }
        let toast = crate::shell::toast_rect(self, frame);
        let mut chips = Vec::new();
        for (index, row) in rows
            .iter()
            .skip(top)
            .take(input.height as usize)
            .enumerate()
        {
            for &(from, to, start) in &row.chip_hits {
                // Clip to both the viewport and final toast overpaint. The map
                // contains painted intervals only, never virtual separator cells.
                let mut run = None;
                let y = input.y + index as u16;
                for column in from..to.min(input.width as usize) {
                    let x = input.x + column as u16;
                    if toast.is_some_and(|rect| rect.contains((x, y).into())) {
                        if let Some(a) = run.take() {
                            chips.push((Rect::new(a, y, x - a, 1), start));
                        }
                    } else {
                        run.get_or_insert(x);
                    }
                }
                if let Some(a) = run {
                    chips.push((
                        Rect::new(a, y, input.x + to.min(input.width as usize) as u16 - a, 1),
                        start,
                    ));
                }
            }
        }
        *self.painted_prompt.borrow_mut() = Some(PaintedPrompt {
            frame,
            main: crate::shell::prompt_main(self, frame),
            home: self.home,
            session: self.session.clone(),
            revision: self.input_revision,
            generation: self.generation,
            cursor: self.editor.cursor,
            anchor: self.editor.anchor,
            chips,
        });
    }

    fn prompt_vertical(&mut self, down: bool, select: bool) -> bool {
        // Keyboard input uses the last painted width even after draft edits;
        // after a resize, the next paint establishes the new width.
        let width = self.prompt_width.get().unwrap_or(usize::MAX);
        let edge = if down { self.input.len() } else { 0 };
        if !select && self.editor.cursor == edge {
            return false;
        }
        if self
            .editor
            .vertical_wrapped(&self.input, width, down, select)
        {
            return true;
        }
        // At a visual edge, the donor first moves to the absolute raw edge;
        // only the next unselected arrow is owned by prompt history.
        if !select && self.editor.cursor != edge {
            self.editor.move_to(edge, false);
            return true;
        }
        false
    }

    /// Only the focused prompt, not a dialog or a dismissed revision, owns the overlay.
    pub(crate) fn slash_options(&self) -> Option<Vec<crate::autocomplete::SlashOption>> {
        if self.panel != TuiPanel::None || self.slash_dismissed == Some(self.input_revision) {
            return None;
        }
        let filter = crate::autocomplete::query(&self.input, self.editor.cursor)?;
        // Home does not register the session-only rename action. Inventory
        // padding must be measured after that route exclusion, before search.
        Some(
            crate::autocomplete::options(
                filter,
                &self.commands,
                &self.command_descriptions,
                self.home,
                self.chrome.dcp.commands_enabled,
                self.chrome.session_terminal,
            )
            .into_iter()
            .collect(),
        )
    }

    /// Keep selection and activation aligned when caret movement or a catalog
    /// refresh shrinks the filtered list without an intervening text edit.
    pub(crate) fn slash_selected(&self, count: usize) -> usize {
        self.slash_selected.min(count.saturating_sub(1))
    }

    pub(super) fn clear_mentions(&mut self) {
        self.mention_result = None;
        self.mention_dismissed = None;
        self.mention_owner_epoch = None;
        self.mention_selected = 0;
    }

    /// A parked view retains its draft, but its filesystem snapshot is no
    /// longer current when the route becomes active again.
    pub fn invalidate_file_suggestions(&mut self) {
        self.generation += 1;
        self.clear_mentions();
    }

    /// A successful owner reload invalidates Location-generation UI snapshots
    /// even when the canonical path and prompt text did not change.
    pub fn refresh_configuration(&mut self, catalog: CatalogSnapshot) {
        self.close_panel();
        self.invalidate_file_suggestions();
        self.slash_selected = 0;
        self.slash_dismissed = None;
        self.sessions.clear();
        self.sessions_loaded = false;
        self.skills.clear();
        self.skills_loaded = false;
        self.dcp = DcpPanelState::default();
        self.apply_catalog(catalog);
    }

    /// No storage or filesystem access: the binary asks the owner after the
    /// input burst, then delivers the bounded snapshot using this exact key.
    pub fn mention_request(&self) -> Option<MentionRequest> {
        if self.panel != TuiPanel::None || self.editor.selected().is_some() {
            return None;
        }
        let location = self.chrome.location.as_ref()?.clone();
        let (start, query) = crate::autocomplete::mention(&self.input, self.editor.cursor)?;
        let request = MentionRequest {
            query: query.into(),
            location,
            view_id: self.view_id,
            generation: self.generation,
            revision: self.input_revision,
            caret: self.editor.cursor,
            start,
        };
        (self.mention_dismissed.as_ref() != Some(&request)).then_some(request)
    }

    /// Reject late results from another edit, caret, route, or owner epoch.
    pub fn apply_file_suggestions(
        &mut self,
        request: MentionRequest,
        result: FileSuggestionsSnapshot,
    ) -> bool {
        if self.mention_request().as_ref() != Some(&request)
            || self.chrome.location.as_deref() != Some(result.location.as_str())
            || self
                .mention_owner_epoch
                .is_some_and(|epoch| epoch != result.generation)
        {
            return false;
        }
        self.mention_owner_epoch = Some(result.generation);
        self.mention_selected = 0;
        self.mention_result = Some((request, result));
        true
    }

    /// A zero-match snapshot is loaded too; do not query again each frame.
    pub fn mention_loaded(&self, request: &MentionRequest) -> bool {
        self.mention_result
            .as_ref()
            .is_some_and(|(key, _)| key == request)
    }

    pub(crate) fn mention_options(&self) -> Option<&FileSuggestionsSnapshot> {
        let request = self.mention_request()?;
        self.mention_result
            .as_ref()
            .filter(|(key, snapshot)| {
                *key == request
                    && snapshot.location == request.location
                    && self.mention_owner_epoch == Some(snapshot.generation)
            })
            .map(|(_, snapshot)| snapshot)
    }

    pub(crate) fn mention_selected(&self, count: usize) -> usize {
        self.mention_selected.min(count.saturating_sub(1))
    }

    fn select_mention(&mut self) {
        let Some((start, caret, path)) = self.mention_options().and_then(|options| {
            let path = options
                .paths
                .get(self.mention_selected(options.paths.len()))?;
            Some((
                self.mention_request()?.start,
                self.editor.cursor,
                path.clone(),
            ))
        }) else {
            return;
        };
        // An owner path is Location-relative. It is inserted as ordinary text;
        // no structured part or implicit read is created.
        if path.starts_with('/') || path.split('/').any(|part| part == "..") {
            return;
        }
        // Pinned autocomplete.tsx:164-175 inserts a separator at the caret,
        // except when the following text already supplies whitespace.
        let separator = self
            .input
            .get(caret..)
            .and_then(|after| after.chars().next())
            .is_some_and(char::is_whitespace);
        let replacement = format!("@{path}{}", if separator { "" } else { " " });
        if self.input.len() - (caret - start) + replacement.len() > MAX_INPUT_BYTES {
            return;
        }
        self.editor.move_to(start, false);
        if self.editor.cursor != start {
            // A pasted chip is an atomic editor range; never expand a mention
            // selection across hidden pasted text.
            self.editor.move_to(caret, false);
            return;
        }
        self.editor.move_to(caret, true);
        if self.editor.selected() != Some((start, caret)) {
            self.editor.move_to(caret, false);
            return;
        }
        if self
            .editor
            .replace(&mut self.input, &replacement, MAX_INPUT_BYTES)
            > 0
        {
            self.editor
                .mark_file_mention(start, start + 1 + path.len(), &self.input);
            self.input_revision += 1;
            self.mention_selected = 0;
            self.mention_result = None;
            self.mention_dismissed = self.mention_request();
        }
    }

    fn replace_slash(&mut self, name: &str, trailing_space: bool) {
        let cursor = self.editor.cursor;
        self.editor.move_to(0, false);
        self.editor.move_to(cursor, true);
        let replacement = format!("/{name}{}", if trailing_space { " " } else { "" });
        if self
            .editor
            .replace(&mut self.input, &replacement, MAX_INPUT_BYTES)
            > 0
        {
            self.input_revision += 1;
        }
        self.slash_selected = 0;
        self.slash_dismissed = Some(self.input_revision);
    }

    async fn select_slash(&mut self, enter: bool) -> KeyOutcome {
        let Some(options) = self.slash_options() else {
            return KeyOutcome::default();
        };
        let selected = self.slash_selected(options.len());
        let Some(option) = options.into_iter().nth(selected) else {
            return KeyOutcome::default();
        };
        if (!enter && option.action != Some(CommandAction::ReloadConfiguration))
            || option.arguments
            || option.action.is_none()
        {
            self.replace_slash(&option.name, true);
            return KeyOutcome::default();
        }
        let action = option.action.expect("argument-free built-in");
        if matches!(
            action,
            CommandAction::NewSession
                | CommandAction::CloseTab
                | CommandAction::CompactSession
                | CommandAction::ReloadConfiguration
                | CommandAction::UndoConversation
                | CommandAction::RedoConversation
        ) {
            // The binary clears these drafts only after its owner accepts the
            // intent. An optimistic removal would lose `/new` on refusal.
            if self.input != format!("/{}", option.name) {
                self.replace_slash(&option.name, false);
            }
            return self.handle_enter().await;
        }
        // The existing owner path checks availability and returns actual intents;
        // a refused command leaves the editable slash text untouched.
        let result = self.run_command(action);
        if result.note.is_none() {
            let cursor = self.editor.cursor;
            self.editor.move_to(0, false);
            self.editor.move_to(cursor, true);
            if self.editor.delete(&mut self.input, true, false) {
                self.input_revision += 1;
            }
            self.slash_selected = 0;
        }
        result
    }

    /// Close any open panel (chat view).
    pub fn close_panel(&mut self) {
        self.clear_accounts();
        let was_open = self.panel != TuiPanel::None;
        self.wheel_motion = None;
        self.clear_transcript_selection();
        self.panel = TuiPanel::None;
        self.rename_input.clear();
        self.rename_editor.clear();
        self.rename_pending = None;
        self.rename_selected = None;
        self.session_delete_confirm = None;
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.mouse_down = None;
        self.tab_down = None;
        if was_open {
            self.tab_view.get_mut().reset_hover();
            self.last_mouse = None;
        }
        self.close_hold = None;
        self.exploration_down = None;
        self.reasoning_down = None;
        self.select.reset();
    }

    /// The active dialog exclusively owns search/cursor input; when it is
    /// replaced (Model → Variant) the former owner is destroyed, and closing
    /// the replacement restores the original prompt draft, selection and caret.
    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> KeyOutcome {
        if self.panel == TuiPanel::None
            && let Some(outcome) = self.terminal_mouse(event, area)
        {
            return outcome;
        }
        if self.panel == TuiPanel::None {
            self.prepare_tabs(area, Instant::now());
            if matches!(
                event.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) && self.tab_wheel_hit(area, event.column, event.row)
            {
                self.tab_scroll
                    .set(if event.kind == MouseEventKind::ScrollUp {
                        self.tab_scroll.get().saturating_sub(1)
                    } else {
                        self.tab_scroll
                            .get()
                            .saturating_add(1)
                            .min(self.tabs.len().saturating_sub(1))
                    });
                self.last_mouse = Some((event.column, event.row, area));
                self.tab_down = None;
                self.prepare_tabs(area, Instant::now());
                return KeyOutcome::default();
            }
            if crate::shell::tab_strip(self, area)
                .and_then(|strip| strip.hit_test(event.column, event.row))
                .is_none()
            {
                let view = self.tab_view.get_mut();
                view.hovered = None;
                view.leave = Some(Instant::now());
            }
        }
        if self.approvals.active().is_some() {
            if !crate::shell::tab_region(self, area).contains((event.column, event.row).into()) {
                self.tab_down = None;
                return self.approvals.mouse(event, area);
            }
            self.approvals.cancel_pointer();
        }
        if self.questions.active().is_some()
            && !crate::shell::tab_region(self, area).contains((event.column, event.row).into())
        {
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::None && self.children.open {
            return self.children.mouse(event);
        }
        use crate::dialog::DialogHit;
        if matches!(
            event.kind,
            MouseEventKind::Down(_) | MouseEventKind::Drag(_)
        ) {
            self.message_down = None;
        }
        let pointer_down = self.reasoning_pointer_down;
        match event.kind {
            MouseEventKind::Down(_) | MouseEventKind::Drag(_) => {
                self.reasoning_pointer_down = true;
            }
            MouseEventKind::Up(_) => self.reasoning_pointer_down = false,
            _ => {}
        }
        self.last_mouse = (self.panel == TuiPanel::None).then_some((event.column, event.row, area));
        if self.close_hold.as_ref().is_some_and(|hold| {
            hold.area != area
                || hold.until <= Instant::now()
                || event.row != hold.strip.tabs.first().map_or(u16::MAX, |tab| tab.rect.y)
                || !area.contains((event.column, event.row).into())
        }) || matches!(event.kind, MouseEventKind::Down(_))
        {
            self.close_hold = None;
        }
        if self.panel == TuiPanel::None {
            let toast = crate::shell::toast_rect(self, area);
            let toast_hit =
                toast.is_some_and(|rect| rect.contains((event.column, event.row).into()));
            self.set_toast_hover(toast_hit, Instant::now());
            // Only the painted close cell is an activation target. The terminal
            // does not expose selection state, so text drags must remain inert.
            let close_hit = toast
                .is_some_and(|rect| event.column == rect.right() - 4 && event.row == rect.y + 1);
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    self.toast_down = close_hit && event.modifiers.is_empty();
                    if self.toast_down {
                        self.selection_gesture = false;
                        self.click = None;
                        self.tab_down = None;
                        self.exploration_down = None;
                        self.reasoning_down = None;
                        return KeyOutcome::default();
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    if std::mem::take(&mut self.toast_down)
                        && close_hit
                        && event.modifiers.is_empty()
                    {
                        self.note = None;
                        self.toast_expiry = None;
                        return KeyOutcome::default();
                    }
                }
                MouseEventKind::Drag(_) => self.toast_down = false,
                _ => {}
            }
        } else {
            self.toast_down = false;
            self.set_toast_hover(false, Instant::now());
        }
        // These surfaces are drawn after the transcript. Their entire painted
        // rectangles own the press and release, including blank fill cells.
        if self.panel == TuiPanel::None
            && matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && self.transcript_overpainted(area, event.column, event.row)
        {
            self.selection_gesture = false;
            self.click = None;
            self.reasoning_down = None;
            self.exploration_down = None;
            self.tab_down = None;
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::None {
            if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
                && !self.transcript_overpainted(area, event.column, event.row)
            {
                let start = self
                    .painted_prompt
                    .borrow()
                    .as_ref()
                    .filter(|p| {
                        p.frame == area
                            && p.main == crate::shell::prompt_main(self, area)
                            && p.home == self.home
                            && p.session == self.session
                            && p.revision == self.input_revision
                            && p.generation == self.generation
                            && p.cursor == self.editor.cursor
                            && p.anchor == self.editor.anchor
                    })
                    .and_then(|p| {
                        p.chips
                            .iter()
                            .find(|(rect, _)| rect.contains((event.column, event.row).into()))
                            .map(|&(_, start)| start)
                    });
                if let Some(start) = start
                    && self.editor.expand_chip(&self.input, start)
                {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.mention_selected = 0;
                    self.selection_gesture = false;
                    self.click = None;
                    self.tab_down = None;
                    self.exploration_down = None;
                    self.reasoning_down = None;
                    self.clear_prompt_paint();
                    return KeyOutcome::default();
                }
            }
            self.handle_transcript_selection(event, area);
            if matches!(event.kind, MouseEventKind::Up(MouseButton::Left))
                && self.transcript_overpainted(area, event.column, event.row)
            {
                self.reasoning_down = None;
                self.exploration_down = None;
                self.tab_down = None;
                return KeyOutcome::default();
            }
            // A drag started on a header belongs to text selection; it must
            // never activate the header on release, even if it returns there.
            if matches!(event.kind, MouseEventKind::Drag(_)) {
                self.reverted_down = None;
                self.message_down = None;
                self.exploration_down = None;
                self.reasoning_down = None;
                self.tab_down = None;
            }
            match event.kind {
                MouseEventKind::Moved => {
                    self.enter_tab_at(area, event.column, event.row, Instant::now());
                    self.exploration_down = None;
                    self.reasoning_down = None;
                }
                MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                    self.reverted_down = self
                        .painted_user_message_target_at(area, event.column, event.row)
                        .filter(|target| target.reverted)
                        .and_then(|_| self.reverted.clone())
                        .map(|boundary| (boundary, self.paint_generation.get(), area));
                    self.message_down = self
                        .painted_user_message_target_at(area, event.column, event.row)
                        .and_then(|target| target.message_id)
                        .map(|id| ((*id).clone(), self.paint_generation.get(), area));
                    self.tab_down = self.tab_hit(area, event.column, event.row);
                    self.exploration_down = self
                        .exploration_hit(area, event.column, event.row)
                        .map(|op| (op, event.column, event.row));
                    self.reasoning_down =
                        self.reasoning_hit(area, event.column, event.row).map(|id| {
                            let rect = crate::shell::transcript_area(self, area);
                            let (_, total, scroll) = self.visible_transcript_at_viewport(
                                rect.width,
                                area.width,
                                rect.height,
                            );
                            (
                                id,
                                event.column,
                                event.row,
                                area,
                                total,
                                scroll,
                                self.scroll,
                            )
                        });
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let reverted_pressed = self.reverted_down.take();
                    let message_pressed = self.message_down.take();
                    let pressed_tab = self.tab_down.take();
                    let pressed = self.exploration_down.take();
                    let exploration_pressed = pressed.is_some();
                    let reasoning_pressed = self.reasoning_down.take();
                    // Resolve a user click only against the last painted,
                    // still-current block. A selection/drag owns its release.
                    if event.modifiers.is_empty()
                        && self
                            .user_message_target_at(area, event.column, event.row)
                            .is_some_and(|target| target.reverted)
                        && self.reverted.as_ref().is_some_and(|boundary| {
                            reverted_pressed.as_ref()
                                == Some(&(boundary.clone(), self.paint_generation.get(), area))
                        })
                    {
                        return self.run_command(CommandAction::RedoConversation);
                    }
                    if event.modifiers.is_empty()
                        && let Some(target) =
                            self.user_message_target_at(area, event.column, event.row)
                        && !target.reverted
                        && let Some(message) = target.message_id
                        && message_pressed
                            == Some(((*message).clone(), self.paint_generation.get(), area))
                    {
                        self.select.reset();
                        self.panel = TuiPanel::MessageActions {
                            message: (*message).clone(),
                            seq: target.seq,
                        };
                        return KeyOutcome::default();
                    }
                    if event.modifiers.is_empty()
                        && let Some(tab) = pressed_tab
                        && self.tab_hit(area, event.column, event.row) == Some(tab)
                    {
                        return KeyOutcome {
                            intent: Some(match tab {
                                TabPress::Add => PanelIntent::NewSession,
                                TabPress::Tab(index) => PanelIntent::ActivateTab { index },
                                TabPress::Close(index) => PanelIntent::CloseTab { index },
                            }),
                            ..KeyOutcome::default()
                        };
                    }
                    if event.modifiers.is_empty()
                        && self.click.is_none_or(|click| click.count == 1)
                        && let Some((op, x, y)) = pressed
                        && (x, y) == (event.column, event.row)
                        && self
                            .exploration_hit(area, event.column, event.row)
                            .as_deref()
                            == Some(&op)
                    {
                        let rect = crate::shell::transcript_area(self, area);
                        let height = rect.height as usize;
                        let (_, before, displayed) = self.visible_transcript_at_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                        );
                        // Keep the clicked header at its painted row by anchoring
                        // the first visible row, rather than the bottom offset.
                        let first = before.saturating_sub(height).saturating_sub(displayed);
                        let rows = self.transcript_rows();
                        self.exploration_expanded.retain(|id| {
                            rows.iter()
                                .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == id))
                        });
                        if !self.exploration_expanded.insert(op.clone()) {
                            self.exploration_expanded.remove(&op);
                        }
                        let (_, after) =
                            self.visible_transcript(rect.width, area.width, rect.height);
                        self.scroll = after.saturating_sub(height).saturating_sub(first);
                        self.observe_transcript_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                            after,
                            self.scroll,
                        );
                    }
                    // The pinned original toggles onMouseUp without a down.
                    // Admit that path only for a currently painted header; a
                    // consumed/stale press or selection gesture stays inert.
                    let release_only = if reasoning_pressed.is_none()
                        && pressed_tab.is_none()
                        && !exploration_pressed
                        && !pointer_down
                        && event.modifiers.is_empty()
                    {
                        let rect = crate::shell::transcript_area(self, area);
                        let (rows, total, scroll) = self.visible_transcript_at_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                        );
                        self.painted_transcript
                            .borrow()
                            .as_ref()
                            .filter(|painted| {
                                painted.area == rect
                                    && painted.total == total
                                    && painted.scroll == scroll
                                    && painted.rows == rows
                            })
                            .and_then(|_| self.reasoning_hit(area, event.column, event.row))
                            .map(|id| {
                                (
                                    id,
                                    event.column,
                                    event.row,
                                    area,
                                    total,
                                    scroll,
                                    self.scroll,
                                )
                            })
                    } else {
                        None
                    };
                    if event.modifiers.is_empty()
                        && self.click.is_none_or(|click| click.count == 1)
                        && matches!(self.selection_text(), Ok(None))
                        && let Some((id, x, y, painted, total, scroll, requested)) =
                            reasoning_pressed.or(release_only)
                        && painted == area
                        && requested == self.scroll
                        && (x, y) == (event.column, event.row)
                        && self.reasoning_hit(area, x, y) == Some(id)
                    {
                        let rect = crate::shell::transcript_area(self, area);
                        let (_, now, displayed) = self.visible_transcript_at_viewport(
                            rect.width,
                            area.width,
                            rect.height,
                        );
                        if (now, displayed) == (total, scroll) {
                            let first = now
                                .saturating_sub(rect.height as usize)
                                .saturating_sub(displayed);
                            self.prune_reasoning();
                            if !self.reasoning_expanded.insert(id) {
                                self.reasoning_expanded.remove(&id);
                            }
                            let (_, after) =
                                self.visible_transcript(rect.width, area.width, rect.height);
                            self.scroll = after
                                .saturating_sub(rect.height as usize)
                                .saturating_sub(first);
                            self.observe_transcript_viewport(
                                rect.width,
                                area.width,
                                rect.height,
                                after,
                                self.scroll,
                            );
                        }
                    }
                }
                MouseEventKind::Drag(_)
                | MouseEventKind::Down(_)
                | MouseEventKind::Up(_)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown => {
                    self.exploration_down = None;
                    self.reasoning_down = None;
                    self.tab_down = None;
                    if matches!(event.kind, MouseEventKind::Drag(_)) {
                        self.tab_view.get_mut().reset_hover();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.reasoning_down = None;
        self.tab_down = None;
        self.tab_view.get_mut().reset_hover();
        self.close_hold = None;
        if self.panel == TuiPanel::Accounts {
            return self.accounts_mouse(event, area);
        }
        if self.panel == TuiPanel::Rename {
            let rect = crate::dialog::rename_geometry(area);
            let hit = if !rect.contains((event.column, event.row).into()) {
                DialogHit::Backdrop
            } else if event.row == rect.y + 1
                && event.column >= rect.right().saturating_sub(7)
                && event.column < rect.right().saturating_sub(4)
            {
                DialogHit::Close
            } else {
                DialogHit::Surface
            };
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => self.mouse_down = Some(hit),
                MouseEventKind::Up(MouseButton::Left) => {
                    let pressed = self.mouse_down.take();
                    if self.rename_pending.is_none()
                        && ((pressed == Some(DialogHit::Backdrop) && hit == DialogHit::Backdrop)
                            || (pressed == Some(DialogHit::Close) && hit == DialogHit::Close))
                    {
                        self.close_panel();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::Cards && self.card_output.is_some() {
            let (rect, _, _) = crate::dialog::card_geometry(area);
            let inside = rect.contains((event.column, event.row).into());
            match event.kind {
                MouseEventKind::ScrollUp if inside => {
                    for _ in 0..3 {
                        self.handle_panel_key(KeyAction::Up);
                    }
                }
                MouseEventKind::ScrollDown if inside => {
                    for _ in 0..3 {
                        self.handle_panel_key(KeyAction::Down);
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    self.mouse_down = Some(if inside {
                        DialogHit::Surface
                    } else {
                        DialogHit::Backdrop
                    });
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    let hit = if inside {
                        DialogHit::Surface
                    } else {
                        DialogHit::Backdrop
                    };
                    if self.mouse_down.take() == Some(DialogHit::Backdrop)
                        && hit == DialogHit::Backdrop
                    {
                        self.close_panel();
                    }
                }
                _ => {}
            }
            return KeyOutcome::default();
        }
        let options = self.modal_options();
        let size = crate::dialog::size_for(&self.panel);
        let hit = self
            .select
            .hit(area, size, &options, event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.mouse_down = Some(hit);
                if let DialogHit::Option(index) = hit {
                    if self.select.cursor != index {
                        self.session_delete_confirm = None;
                    }
                    self.select.cursor = index;
                    self.sync_modal_cursor();
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(MouseButton::Left) => {
                if let DialogHit::Option(index) = hit
                    && self.select.cursor != index
                {
                    self.session_delete_confirm = None;
                    self.select.cursor = index;
                    self.sync_modal_cursor();
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if hit != DialogHit::Backdrop {
                    self.select.scroll_rows(
                        if event.kind == MouseEventKind::ScrollUp {
                            -3
                        } else {
                            3
                        },
                        area,
                        size,
                        &options,
                    );
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self.mouse_down.take();
                if pressed == Some(hit) {
                    match hit {
                        DialogHit::Backdrop | DialogHit::Close => self.close_panel(),
                        DialogHit::Option(index)
                            if !matches!(
                                self.panel,
                                TuiPanel::Dcp | TuiPanel::Cards | TuiPanel::Help(_)
                            ) =>
                        {
                            self.select.cursor = index;
                            self.sync_modal_cursor();
                            return self.panel_enter();
                        }
                        DialogHit::Option(_)
                        | DialogHit::Search
                        | DialogHit::Surface
                        | DialogHit::List => {}
                    }
                }
            }
            _ => {}
        }
        KeyOutcome::default()
    }

    pub fn session_search(&self) -> String {
        self.select.query.clone()
    }

    pub fn sessions_all_projects(&self) -> bool {
        self.sessions_all_projects
    }

    pub fn session_entries(&self) -> &[oc_core::queries::SessionListEntry] {
        &self.session_entries
    }

    pub fn sessions_title(&self) -> String {
        if !self.sessions_all_projects
            && let Some(name) = &self.session_project_name
        {
            return format!("Sessions for {name}");
        }
        "Sessions".into()
    }

    pub fn session_scope_update(&mut self) -> Option<bool> {
        self.session_scope_pending.take()
    }

    pub fn selected_session_rename(&self) -> Option<&str> {
        self.rename_selected.as_deref()
    }

    /// Handle a bracketed paste as one bounded event (never per-char).
    pub fn handle_paste(&mut self, text: &str) -> KeyOutcome {
        if self.panel == TuiPanel::Accounts {
            if self.approvals.active().is_some() || self.questions.active().is_some() {
                self.close_panel();
                return KeyOutcome::default();
            }
            return self.paste_accounts(text);
        }
        if self.approvals.active().is_some() {
            self.approvals.paste(text);
            return KeyOutcome::default();
        }
        if self.questions.active().is_some() {
            self.questions.paste(text);
            return KeyOutcome::default();
        }
        use unicode_segmentation::UnicodeSegmentation as _;
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::None && (self.shells.open || self.terminals.open) {
            return KeyOutcome::default();
        }
        if self.panel != TuiPanel::None {
            if self.panel == TuiPanel::Rename {
                return self.paste_rename(text);
            }
            let room = 512_usize.saturating_sub(self.select.query.len());
            let mut kept = 0;
            for grapheme in text.graphemes(true) {
                if kept + grapheme.len() > room {
                    break;
                }
                if !grapheme.chars().any(char::is_control) {
                    self.select.query.push_str(grapheme);
                    kept += grapheme.len();
                }
            }
            self.changed_modal_query();
            return KeyOutcome {
                note: (text.len() > kept).then(|| "modal search truncated at 512 bytes".into()),
                intent: (self.panel == TuiPanel::Sessions).then_some(PanelIntent::LoadSessions),
                ..KeyOutcome::default()
            };
        }
        let mut clean = String::with_capacity(text.len().min(MAX_INPUT_BYTES));
        let mut exceeded = false;
        for grapheme in text.graphemes(true) {
            let safe = match grapheme {
                "\r\n" | "\r" => "\n",
                "\t" => " ",
                _ if grapheme.chars().any(char::is_control) && grapheme != "\n" => continue,
                _ => grapheme,
            };
            if clean.len() + safe.len() > MAX_INPUT_BYTES {
                exceeded = true;
                break;
            }
            clean.push_str(safe);
        }
        let chip_count = self.editor.chip_count();
        let paste = if exceeded {
            self.editor
                .paste_clipped(&mut self.input, &clean, MAX_INPUT_BYTES)
        } else {
            self.editor.paste(&mut self.input, &clean, MAX_INPUT_BYTES)
        };
        if paste.expanded {
            self.input_revision += 1;
            self.slash_selected = 0;
            self.mention_selected = 0;
            return KeyOutcome::default();
        }
        let dropped = text.len().saturating_sub(paste.inserted);
        if paste.inserted > 0 {
            self.input_revision += 1;
            self.slash_selected = 0;
            self.mention_selected = 0;
        }
        let mut notes = Vec::new();
        if exceeded || clean.len() > paste.inserted + paste.trimmed {
            notes.push(format!(
                "paste truncated: {dropped} bytes dropped at the {MAX_INPUT_BYTES} byte input limit"
            ));
        } else if text.len() > clean.len() {
            notes.push(format!(
                "paste filtered: {} control/newline-normalization bytes",
                text.len() - clean.len()
            ));
        }
        if paste.trimmed > 0 {
            notes.push(format!(
                "paste chip trimmed: {} surrounding whitespace bytes removed",
                paste.trimmed
            ));
        }
        if paste.inserted > 0
            && chip_count == crate::editor::MAX_PASTE_CHIPS
            && crate::editor::chip_worthy(&clean[..paste.inserted + paste.trimmed]).is_some()
        {
            notes
                .push("paste display chip limit reached; full pasted text remains in draft".into());
        }
        KeyOutcome {
            note: (!notes.is_empty()).then(|| notes.join("; ")),
            ..KeyOutcome::default()
        }
    }

    /// Current modal value, distinct from the prompt draft and its caret.
    pub fn rename_title(&self) -> Option<&str> {
        (self.panel == TuiPanel::Rename).then_some(self.rename_input.as_str())
    }

    pub(crate) fn rename_cursor(&self) -> usize {
        self.rename_editor.cursor
    }

    fn paste_rename(&mut self, text: &str) -> KeyOutcome {
        use unicode_segmentation::UnicodeSegmentation as _;
        if self.rename_pending.is_some() {
            return KeyOutcome::default();
        }
        let mut clean = String::new();
        let mut clipped = false;
        for grapheme in text.graphemes(true) {
            let safe = match grapheme {
                "\r\n" | "\n" | "\r" | "\t" => " ",
                _ if grapheme.chars().any(char::is_control) => continue,
                _ => grapheme,
            };
            if clean.len() + safe.len() > MAX_SESSION_TITLE_BYTES {
                clipped = true;
                break;
            }
            clean.push_str(safe);
        }
        let inserted =
            self.rename_editor
                .replace(&mut self.rename_input, &clean, MAX_SESSION_TITLE_BYTES);
        KeyOutcome {
            note: (clipped || inserted < clean.len()).then(|| self.rename_limit_note()),
            ..KeyOutcome::default()
        }
    }

    fn rename_limit_note(&self) -> String {
        if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
            format!(
                "session title exceeds {MAX_SESSION_TITLE_BYTES} bytes; shorten it or select all to replace"
            )
        } else {
            format!("session title truncated at {MAX_SESSION_TITLE_BYTES} bytes")
        }
    }

    /// Model under the picker cursor; the active variant is preserved when
    /// the cursor still points at the selected model.
    pub fn picker_selection(&self) -> Option<(String, Option<String>)> {
        let picker = self.picker.as_ref()?;
        let id = picker.cursor_id()?;
        let variant = picker
            .selection()
            .filter(|selection| selection.id == id)
            .and_then(|selection| selection.variant.as_ref())
            .map(|variant| variant.name.clone());
        Some((id, variant))
    }

    /// Effective model id and variant name from the catalog snapshot (the
    /// resolved selection, never the picker cursor).
    pub fn active_model_label(&self) -> Option<(String, Option<String>)> {
        let picker = self.picker.as_ref()?;
        if let crate::picker::PickerState::Retired { wanted, .. } = picker.state() {
            let identity = self
                .chrome
                .provider
                .as_ref()
                .map_or(wanted.as_str(), |provider| provider.model.as_str());
            return Some((format!("{identity} (unavailable)"), None));
        }
        let selection = picker.selection()?;
        Some((
            selection
                .entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&selection.id)
                .to_string(),
            selection
                .variant
                .as_ref()
                .map(|variant| variant.name.clone())
                .or_else(|| {
                    picker
                        .retired_variant_label()
                        .map(|name| format!("{name} (unavailable)"))
                }),
        ))
    }

    /// Provider id of the loaded catalog, if any.
    pub fn active_provider(&self) -> Option<&str> {
        let picker = self.picker.as_ref()?;
        Some(
            picker
                .selection()
                .and_then(|s| s.entry.get("provider_name"))
                .and_then(|v| v.as_str())
                .unwrap_or(picker.provider()),
        )
    }

    /// Effective agent id from the catalog snapshot, if any.
    pub fn active_agent(&self) -> Option<&str> {
        self.active_agent.as_deref()
    }

    /// True when the input names a workspace command (template expanded by
    /// the application, never by the view).
    pub fn is_workspace_command(&self, text: &str) -> bool {
        let Some(rest) = text.strip_prefix('/') else {
            return false;
        };
        let name = rest.split_whitespace().next().unwrap_or_default();
        self.commands.iter().any(|id| id == name)
    }

    /// Agent id under the Agents cursor, if any.
    pub fn selected_agent(&self) -> Option<String> {
        self.agents
            .get(self.agents_cursor)
            .map(|agent| agent.id.clone())
    }

    /// Sessions cursor position.
    pub fn sessions_cursor(&self) -> usize {
        self.sessions_cursor
    }

    // ---- key handling ---------------------------------------------------

    /// Handle one key action. The returned [`KeyOutcome`] tells the binary
    /// whether to display a note, apply an intent, or treat the input as
    /// consumed.
    pub async fn handle_key(&mut self, action: KeyAction) -> KeyOutcome {
        let terminal_command = match action {
            KeyAction::TerminalFocusLeft => Some(CommandAction::FocusSessionPane),
            KeyAction::TerminalFocusRight => Some(CommandAction::FocusTerminalPane),
            KeyAction::TerminalSelect => Some(CommandAction::SelectTerminal),
            KeyAction::TerminalToggle => Some(CommandAction::ToggleTerminal),
            KeyAction::TerminalClose => Some(CommandAction::CloseTerminal),
            _ => None,
        };
        if let Some(command) = terminal_command {
            return self.run_command(command);
        }
        if self.panel == TuiPanel::Accounts {
            if self.approvals.active().is_some() || self.questions.active().is_some() {
                self.close_panel();
                return KeyOutcome::default();
            }
            return self.accounts_key(action);
        }
        let mut action = action;
        if self.approvals.active().is_some()
            && !(self.terminal_focused()
                && matches!(action, KeyAction::Leader | KeyAction::SequenceKey(..)))
        {
            return self.approvals.key(action);
        }
        if self.questions.active().is_some()
            && !(self.terminal_focused()
                && matches!(action, KeyAction::Leader | KeyAction::SequenceKey(..)))
        {
            return self.questions.key(action);
        }
        self.poll_submission();
        if self
            .leader_deadline()
            .is_some_and(|until| Instant::now() >= until)
        {
            self.leader = None;
        }
        if action == KeyAction::Quit {
            self.leader = None;
        }
        if self.leader.take().is_some() {
            let key = match &action {
                KeyAction::SequenceKey(key, _) => key.clone(),
                KeyAction::Char(key) => key.to_string(),
                KeyAction::Cancel => "esc".into(),
                KeyAction::Backspace => "backspace".into(),
                _ => String::new(),
            };
            let binding = format!("{} {key}", self.leader_key);
            let command = if self.panel == TuiPanel::None {
                [
                    CommandAction::UndoConversation,
                    CommandAction::RedoConversation,
                ]
                .into_iter()
                .find(|action| {
                    self.conversation_shortcut(*action == CommandAction::UndoConversation)
                        .split(',')
                        .any(|value| value.trim().eq_ignore_ascii_case(&binding))
                })
                .or_else(|| {
                    self.chrome
                        .command_palette_shortcut
                        .as_ref()
                        .filter(|shortcut| {
                            shortcut
                                .split(',')
                                .any(|value| value.trim().eq_ignore_ascii_case(&binding))
                        })
                        .map(|_| CommandAction::OpenCommands)
                })
                .or_else(|| {
                    crate::commands::REGISTRY
                        .iter()
                        .filter(|c| {
                            !matches!(
                                c.action,
                                CommandAction::UndoConversation | CommandAction::RedoConversation
                            )
                        })
                        .find(|c| {
                            self.command_shortcuts(c)
                                .iter()
                                .any(|shortcut| shortcut.eq_ignore_ascii_case(&binding))
                        })
                        .map(|c| c.action.clone())
                })
            } else {
                None
            };
            if let Some(command) = command {
                return self.run_command(command);
            }
            // Actual pinned original: sequence miss clears pending, then an
            // unmatched printable reaches the focused composer (wzord oracle).
            action = match action {
                KeyAction::SequenceKey(_, Some(value)) | KeyAction::Char(value) => {
                    KeyAction::Char(value)
                }
                KeyAction::SequenceKey(ref key, _) if key == "enter" => KeyAction::Enter,
                KeyAction::Enter => KeyAction::Enter,
                _ => return KeyOutcome::default(),
            };
        }
        if self.panel == TuiPanel::None && action == KeyAction::Shells {
            self.children.open = false;
            self.close_terminal_composer();
        }
        if self.panel == TuiPanel::None && action == KeyAction::Children {
            self.shells.open = false;
            self.close_terminal_composer();
        }
        if self.panel == TuiPanel::None && self.terminals.open && action != KeyAction::Leader {
            return self.terminal_composer_key(action);
        }
        if self.panel == TuiPanel::None && (self.shells.open || action == KeyAction::Shells) {
            return self.shells.key(action);
        }
        if self.panel == TuiPanel::None
            && (self.children.open
                || action == KeyAction::Children
                || (self.linked_child().is_some()
                    && matches!(
                        action,
                        KeyAction::Cancel | KeyAction::Interrupt | KeyAction::ShellBackground
                    )))
        {
            return self.children.key(action);
        }
        if self.panel != TuiPanel::None {
            if action == KeyAction::Leader {
                self.leader = Some(Instant::now());
                return KeyOutcome::default();
            }
            return self.handle_panel_key(action);
        }
        let action = if action == KeyAction::CtrlA {
            KeyAction::Home
        } else {
            action
        };
        if let Some(options) = self.slash_options() {
            match action {
                KeyAction::Up | KeyAction::Commands => {
                    if !options.is_empty() {
                        self.slash_selected = (self.slash_selected(options.len()) + options.len()
                            - 1)
                            % options.len();
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Down => {
                    if !options.is_empty() {
                        self.slash_selected =
                            (self.slash_selected(options.len()) + 1) % options.len();
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Tab => return self.select_slash(false).await,
                KeyAction::Enter => return self.select_slash(true).await,
                KeyAction::Cancel => {
                    self.slash_dismissed = Some(self.input_revision);
                    return KeyOutcome::default();
                }
                KeyAction::Interrupt => {
                    // autocomplete.tsx:744-759: prompt.clear hides the command
                    // menu and removes only the trigger-to-caret token.
                    let caret = self.editor.cursor;
                    self.editor.move_to(0, false);
                    self.editor.move_to(caret, true);
                    if self.editor.delete(&mut self.input, true, false) {
                        self.input_revision += 1;
                    }
                    self.slash_selected = 0;
                    self.slash_dismissed = Some(self.input_revision);
                    self.leader = None;
                    return KeyOutcome::default();
                }
                _ => {}
            }
        }
        if let Some(options) = self.mention_options() {
            let count = options.paths.len();
            match action {
                KeyAction::Up | KeyAction::Commands => {
                    if count > 0 {
                        self.mention_selected = (self.mention_selected(count) + count - 1) % count;
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Down => {
                    if count > 0 {
                        self.mention_selected = (self.mention_selected(count) + 1) % count;
                    }
                    return KeyOutcome::default();
                }
                KeyAction::Tab => {
                    self.select_mention();
                    return KeyOutcome::default();
                }
                KeyAction::Cancel => {
                    self.mention_dismissed = self.mention_request();
                    return KeyOutcome::default();
                }
                KeyAction::Interrupt => {
                    // Reference autocomplete hides without deleting @query.
                    self.mention_dismissed = self.mention_request();
                    self.leader = None;
                    return KeyOutcome::default();
                }
                _ => {}
            }
        } else if let Some(request) = self.mention_request() {
            match action {
                KeyAction::Cancel | KeyAction::Interrupt => {
                    self.mention_dismissed = Some(request);
                    self.leader = None;
                    return KeyOutcome::default();
                }
                KeyAction::Up | KeyAction::Down | KeyAction::Commands | KeyAction::Tab => {
                    return KeyOutcome::default();
                }
                _ => {}
            }
        }
        if matches!(
            action,
            KeyAction::Cancel
                | KeyAction::Interrupt
                | KeyAction::Quit
                | KeyAction::Commands
                | KeyAction::Agents
                | KeyAction::CycleVariant
                | KeyAction::Rename
                | KeyAction::UndoConversation
                | KeyAction::RedoConversation
        ) {
            self.leader = None;
        }
        match action {
            KeyAction::Commands => self.run_command(CommandAction::OpenCommands),
            KeyAction::TerminalFocusLeft
            | KeyAction::TerminalFocusRight
            | KeyAction::TerminalSelect
            | KeyAction::TerminalToggle
            | KeyAction::TerminalClose => unreachable!("handled before global focus"),
            KeyAction::UndoConversation => self.run_command(CommandAction::UndoConversation),
            KeyAction::RedoConversation => self.run_command(CommandAction::RedoConversation),
            KeyAction::Agents => self.run_command(CommandAction::OpenAgents),
            KeyAction::CycleVariant => KeyOutcome {
                intent: Some(PanelIntent::CycleVariant),
                ..Default::default()
            },
            KeyAction::Rename => self.run_command(CommandAction::RenameSession { title: None }),
            KeyAction::Leader => {
                self.leader = Some(Instant::now());
                KeyOutcome::default()
            }
            KeyAction::SequenceKey(_, _) => KeyOutcome::default(),
            KeyAction::Left
            | KeyAction::Right
            | KeyAction::WordLeft
            | KeyAction::WordRight
            | KeyAction::SelectLeft
            | KeyAction::SelectRight
            | KeyAction::SelectWordLeft
            | KeyAction::SelectWordRight => {
                let right = matches!(
                    action,
                    KeyAction::Right
                        | KeyAction::WordRight
                        | KeyAction::SelectRight
                        | KeyAction::SelectWordRight
                );
                let word = matches!(
                    action,
                    KeyAction::WordLeft
                        | KeyAction::WordRight
                        | KeyAction::SelectWordLeft
                        | KeyAction::SelectWordRight
                );
                let select = matches!(
                    action,
                    KeyAction::SelectLeft
                        | KeyAction::SelectRight
                        | KeyAction::SelectWordLeft
                        | KeyAction::SelectWordRight
                );
                self.editor.horizontal(&self.input, right, word, select);
                KeyOutcome::default()
            }
            KeyAction::Home | KeyAction::CtrlA | KeyAction::End => {
                self.editor
                    .line_edge(&self.input, action == KeyAction::End, false);
                KeyOutcome::default()
            }
            KeyAction::SelectHome | KeyAction::SelectEnd => {
                self.editor.move_to(
                    if action == KeyAction::SelectHome {
                        0
                    } else {
                        self.input.len()
                    },
                    true,
                );
                KeyOutcome::default()
            }
            KeyAction::SelectUp | KeyAction::SelectDown => {
                self.prompt_vertical(action == KeyAction::SelectDown, true);
                KeyOutcome::default()
            }
            KeyAction::PageUp | KeyAction::PageDown => KeyOutcome::default(),
            KeyAction::Char(c) => {
                if self
                    .editor
                    .replace(&mut self.input, &c.to_string(), MAX_INPUT_BYTES)
                    > 0
                {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.mention_selected = 0;
                    KeyOutcome::default()
                } else {
                    KeyOutcome {
                        note: Some(format!(
                            "input limit {MAX_INPUT_BYTES} bytes reached; the key was not added"
                        )),
                        ..KeyOutcome::default()
                    }
                }
            }
            KeyAction::Backspace => {
                if self.editor.delete(&mut self.input, true, false) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.mention_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::DeleteOrQuit if self.input.is_empty() && !self.is_busy() => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Delete
            | KeyAction::DeleteOrQuit
            | KeyAction::WordBackspace
            | KeyAction::WordDelete => {
                if self.editor.delete(
                    &mut self.input,
                    action == KeyAction::WordBackspace,
                    action != KeyAction::Delete,
                ) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Newline => {
                if self.editor.replace(&mut self.input, "\n", MAX_INPUT_BYTES) > 0 {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Undo | KeyAction::Redo => {
                if self.editor.undo(&mut self.input, action == KeyAction::Redo) {
                    self.input_revision += 1;
                    self.slash_selected = 0;
                }
                KeyOutcome::default()
            }
            KeyAction::Up => {
                if self.prompt_vertical(false, false) {
                    return KeyOutcome::default();
                }
                if self.recall_history(true) {
                    return KeyOutcome::default();
                }
                self.scroll_transcript(true)
            }
            KeyAction::Down => {
                if self.prompt_vertical(true, false) {
                    return KeyOutcome::default();
                }
                if self.recall_history(false) {
                    return KeyOutcome::default();
                }
                self.scroll_transcript(false)
            }
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Interrupt => {
                if self.input.is_empty() {
                    self.status = TuiStatus::Quit;
                } else {
                    self.input.clear();
                    self.editor.clear();
                    self.input_revision += 1;
                    self.slash_selected = 0;
                    self.slash_dismissed = None;
                    self.clear_mentions();
                }
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                if self.is_busy() {
                    // Pending submission cancellation retains its existing
                    // safety semantics. Once a turn is accepted, the focused
                    // prompt follows the original's two-Esc interrupt guard.
                    if self.active_turn.is_some()
                        || self.compactions.iter().any(crate::compaction::active)
                    {
                        let now = Instant::now();
                        if self.interrupt_armed_until.is_none_or(|until| now >= until) {
                            self.interrupt_armed_until = now.checked_add(Duration::from_secs(5));
                            return KeyOutcome::default();
                        }
                        self.interrupt_armed_until = None;
                    }
                    let session = self
                        .pending
                        .as_ref()
                        .map(|p| &p.session)
                        .or(self.session.as_ref())
                        .cloned();
                    if let Some(pending) = &mut self.pending {
                        pending.cancelling = true;
                    }
                    let Some(session) = session else {
                        return KeyOutcome {
                            note: Some("no session yet".into()),
                            ..KeyOutcome::default()
                        };
                    };
                    let compaction = self.compactions.iter().any(crate::compaction::active);
                    if compaction {
                        if let Err(error) = self.app.cancel_compaction(session.clone()).await {
                            return KeyOutcome {
                                note: Some(format!("cancel: {error}")),
                                ..KeyOutcome::default()
                            };
                        }
                        if self.active_turn.is_none() && self.pending.is_none() {
                            return KeyOutcome::default();
                        }
                    }
                    match self.app.cancel(session).await {
                        Ok(()) => KeyOutcome::default(),
                        Err(error) => KeyOutcome {
                            note: Some(format!("cancel: {error}")),
                            ..KeyOutcome::default()
                        },
                    }
                } else {
                    if let Some(session) = self.session.clone() {
                        match self.app.cancel(session).await {
                            Ok(()) => {
                                return KeyOutcome {
                                    note: Some("background shell cancellation requested".into()),
                                    ..KeyOutcome::default()
                                };
                            }
                            Err(CoreError::TurnNotActive) => {}
                            Err(error) => {
                                return KeyOutcome {
                                    note: Some(format!("cancel: {error}")),
                                    ..KeyOutcome::default()
                                };
                            }
                        }
                    }
                    self.status = TuiStatus::Quit;
                    KeyOutcome::default()
                }
            }
            KeyAction::Enter => self.handle_enter().await,
            KeyAction::Tab => KeyOutcome::default(),
            KeyAction::Shells | KeyAction::ShellBackground | KeyAction::Children => {
                KeyOutcome::default()
            }
        }
    }

    pub(super) fn recall_history(&mut self, previous: bool) -> bool {
        let entries: Vec<String> = self
            .window
            .rows()
            .iter()
            .filter(|row| row.role == "user")
            .map(|row| row.text.clone())
            .collect();
        let changed = self.editor.recall(&mut self.input, previous, entries);
        if changed {
            self.editor
                .move_to(if previous { 0 } else { self.input.len() }, false);
            self.input_revision += 1;
        }
        changed
    }

    async fn handle_enter(&mut self) -> KeyOutcome {
        if self.status == TuiStatus::Quit {
            return KeyOutcome::default();
        }
        if self.input.trim().is_empty() {
            return match self.commit_composer_model() {
                Ok(()) => KeyOutcome::default(),
                Err(error) => KeyOutcome {
                    note: Some(format!("model commit: {error}")),
                    ..Default::default()
                },
            };
        }
        if self.pending.is_some() {
            if let Some(action @ (CommandAction::OpenModelPicker | CommandAction::OpenVariants)) =
                dispatch(self.input.trim())
            {
                let outcome = self.run_command(action);
                if outcome.note.is_none() {
                    self.input.clear();
                    self.editor.clear();
                    self.input_revision += 1;
                }
                return outcome;
            }
            if let Some(action) = dispatch(self.input.trim())
                && let Some(reason) = self.command_unavailable(&action)
            {
                return KeyOutcome {
                    note: Some(reason.into()),
                    ..KeyOutcome::default()
                };
            }
            return KeyOutcome {
                note: Some("submission pending; Esc to cancel".into()),
                ..KeyOutcome::default()
            };
        }
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return KeyOutcome::default();
        }
        if let Some(action) = dispatch(&text) {
            // Workspace commands reach the application, which owns their
            // templates; the built-in table only routes known commands.
            if !matches!(action, CommandAction::Help(None)) || !self.is_workspace_command(&text) {
                if matches!(action, CommandAction::RenameSession { title: None }) {
                    if let Some(reason) = self.command_unavailable(&action) {
                        return KeyOutcome {
                            note: Some(reason.into()),
                            ..KeyOutcome::default()
                        };
                    }
                    if self.regenerate_pending.is_some() {
                        return KeyOutcome {
                            note: Some("title generation pending".into()),
                            ..KeyOutcome::default()
                        };
                    }
                    self.regenerate_pending = Some(self.input_revision);
                    return KeyOutcome {
                        intent: Some(PanelIntent::RegenerateTitle),
                        ..KeyOutcome::default()
                    };
                }
                let outcome = self.run_command(action);
                if outcome.consumed_input
                    || matches!(
                        self.panel,
                        TuiPanel::Model
                            | TuiPanel::Variant
                            | TuiPanel::Agents
                            | TuiPanel::Sessions
                            | TuiPanel::Skills
                            | TuiPanel::Commands
                            | TuiPanel::Cards
                            | TuiPanel::Help(_)
                    )
                {
                    self.input.clear();
                    self.editor.clear();
                    self.input_revision += 1;
                }
                return outcome;
            }
        }
        if self.active_turn.is_some() {
            return KeyOutcome {
                note: Some("turn busy".into()),
                ..KeyOutcome::default()
            };
        }
        // Capture now; owner preparation commits only after earlier admissions.
        let fresh = self.session.is_none();
        let session = self.session.clone().unwrap_or_else(fresh_session_id);
        let result = if fresh {
            let selection =
                self.captured_model_commit()
                    .map(|commit| oc_core::core_app::FreshSelection {
                        binding: Some(commit.binding.clone()),
                        agent_id: commit.binding.agent_id,
                        model_id: commit.model_id,
                        variant: commit.variant,
                    });
            self.app
                .request_submit_fresh(session.clone(), text, selection)
        } else {
            self.app
                .request_submit_selected(session.clone(), text, self.captured_model_commit())
        };
        match result {
            Ok(receipt) => {
                self.begin_submission(receipt, session, fresh, false);
                KeyOutcome::default()
            }
            Err(error) => KeyOutcome {
                note: Some(format!("submit: {error}")),
                ..KeyOutcome::default()
            },
        }
    }

    pub(super) fn run_command(&mut self, action: CommandAction) -> KeyOutcome {
        if let Some(reason) = self.command_unavailable(&action) {
            return KeyOutcome {
                note: Some(reason.into()),
                ..KeyOutcome::default()
            };
        }
        if action.is_terminal() {
            let mut outcome = self.terminal_command(action);
            outcome.consumed_input = self.input.trim().starts_with('/');
            return outcome;
        }
        if matches!(
            action,
            CommandAction::UndoConversation | CommandAction::RedoConversation
        ) {
            return KeyOutcome {
                intent: Some(PanelIntent::ChangeConversation {
                    action: if action == CommandAction::UndoConversation {
                        oc_core::queries::ConversationAction::Undo
                    } else {
                        oc_core::queries::ConversationAction::Redo
                    },
                }),
                ..KeyOutcome::default()
            };
        }
        if action == CommandAction::CloseTab {
            let (_, index, _) = self.tab_presentation();
            // The binary applies the close and replaces the view on success.
            // A refused owner action must leave the dialog and draft intact.
            return KeyOutcome {
                intent: Some(PanelIntent::CloseTab { index }),
                ..KeyOutcome::default()
            };
        }
        if action == CommandAction::CompactSession {
            // Retain palette search/cursor and composer until owner admission.
            return KeyOutcome {
                intent: Some(PanelIntent::CompactSession),
                ..KeyOutcome::default()
            };
        }
        if self.regenerate_pending.is_some()
            && matches!(action, CommandAction::RenameSession { title: None })
        {
            return KeyOutcome {
                note: Some("title generation pending".into()),
                ..KeyOutcome::default()
            };
        }
        if let CommandAction::RenameSession { title: Some(title) } = &action {
            if self.rename_direct_pending.is_some() {
                return KeyOutcome {
                    note: Some("session rename pending".into()),
                    ..KeyOutcome::default()
                };
            }
            let Some(title) = oc_core::core_app::normalized_session_title(title) else {
                return KeyOutcome {
                    note: Some(format!(
                        "session title must be 1–{MAX_SESSION_TITLE_BYTES} bytes of visible text"
                    )),
                    ..KeyOutcome::default()
                };
            };
            let title = title.to_string();
            self.rename_direct_pending = Some((title.clone(), self.input_revision));
            return KeyOutcome {
                intent: Some(PanelIntent::RenameSessionDirect { title }),
                ..KeyOutcome::default()
            };
        }
        self.clear_accounts();
        self.select.reset();
        self.mouse_down = None;
        self.tab_down = None;
        self.tab_view.get_mut().reset_hover();
        self.close_hold = None;
        self.last_mouse = None;
        self.leader = None;
        let mut outcome = KeyOutcome::default();
        match action {
            CommandAction::CreateTerminal
            | CommandAction::SelectTerminal
            | CommandAction::ToggleTerminal
            | CommandAction::CloseTerminal
            | CommandAction::FocusSessionPane
            | CommandAction::FocusTerminalPane => {
                unreachable!("terminal command returned before modal reset")
            }
            CommandAction::OpenConnect => return self.open_accounts(true),
            CommandAction::OpenAccounts => return self.open_accounts(false),
            CommandAction::OpenSettings => self.panel = TuiPanel::Settings,
            CommandAction::OpenPermissions => self.panel = TuiPanel::Settings,
            CommandAction::UndoConversation | CommandAction::RedoConversation => {
                unreachable!("returned before modal reset")
            }
            CommandAction::OpenCommands => {
                self.panel = TuiPanel::Commands;
            }
            CommandAction::ToggleSidebar => {
                self.chrome.sidebar_hidden = !self.chrome.sidebar_hidden;
                self.panel = TuiPanel::None;
                outcome.consumed_input = true;
            }
            CommandAction::ToggleThinking => {
                self.thinking_expanded = !self.thinking_expanded;
                self.panel = TuiPanel::None;
                outcome.consumed_input = true;
            }
            CommandAction::Quit => {
                self.status = TuiStatus::Quit;
                outcome.consumed_input = true;
            }
            CommandAction::OpenModelPicker => {
                self.panel = TuiPanel::Model;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenVariants => self.open_variants(),
            CommandAction::NewSession => {
                outcome.intent = Some(PanelIntent::NewSession);
            }
            CommandAction::ReloadConfiguration => {
                outcome.intent = Some(PanelIntent::ReloadConfiguration);
            }
            CommandAction::RenameSession { title: None } => {
                self.panel = TuiPanel::Rename;
                // Generated titles contain at most 100 Unicode scalar values (<=400
                // UTF-8 bytes). Keep the whole title, even when the owner would
                // reject it as a replacement, so Enter cannot submit a prefix.
                // An unexpectedly larger title stays only in session_title, not
                // in the editor's undo history or an unbounded modal copy.
                const MAX_RENAME_PREFILL_BYTES: usize = 100 * 4;
                self.rename_input = self
                    .session_title
                    .as_ref()
                    .filter(|title| title.len() <= MAX_RENAME_PREFILL_BYTES)
                    .cloned()
                    .unwrap_or_default();
                if self
                    .session_title
                    .as_ref()
                    .is_some_and(|title| title.len() > MAX_RENAME_PREFILL_BYTES)
                {
                    outcome.note =
                        Some("existing title too long to prefill; type a replacement".into());
                } else if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
                    outcome.note = Some(self.rename_limit_note());
                }
                self.rename_editor.clear();
                self.rename_editor.cursor = self.rename_input.len();
                self.rename_pending = None;
            }
            CommandAction::RenameSession { title: Some(_) } => {
                unreachable!("direct rename is returned before modal reset")
            }
            CommandAction::CloseTab => unreachable!("close is returned before modal reset"),
            CommandAction::OpenAgents => {
                self.panel = TuiPanel::Agents;
                open_snapshot(&mut outcome, self.catalog_loaded, PanelIntent::LoadCatalog);
            }
            CommandAction::OpenSessions => {
                self.panel = TuiPanel::Sessions;
                // Sessions can be created by the application since the last opening.
                outcome.intent = Some(PanelIntent::LoadSessions);
                outcome.consumed_input = self.sessions_loaded;
            }
            CommandAction::OpenSkills => {
                self.panel = TuiPanel::Skills;
                open_snapshot(&mut outcome, self.skills_loaded, PanelIntent::LoadSkills);
            }
            CommandAction::OpenMcps => {
                self.panel = TuiPanel::Mcps;
                self.mcp_detail = None;
                outcome.intent = Some(PanelIntent::LoadMcps);
                outcome.consumed_input = true;
            }
            CommandAction::OpenCards => {
                self.panel = TuiPanel::Cards;
                self.card_output = None;
                self.card_scroll = 0;
                self.card_seen.set(0);
                open_snapshot(&mut outcome, self.cards_loaded, PanelIntent::LoadCards);
            }
            CommandAction::SwitchLocation { path } => {
                if path.is_empty() {
                    outcome.note = Some("usage: /location <project-path>".to_string());
                    outcome.consumed_input = true;
                } else {
                    outcome.intent = Some(PanelIntent::SwitchLocation { path });
                }
            }
            CommandAction::Help(topic) => {
                self.panel = TuiPanel::Help(topic);
                outcome.consumed_input = true;
            }
            CommandAction::OpenDcp => {
                self.panel = TuiPanel::Dcp;
                outcome.consumed_input = true;
            }
            CommandAction::DcpCompress { focus } => {
                self.panel = TuiPanel::Dcp;
                outcome.intent = Some(PanelIntent::Compress { focus });
            }
            CommandAction::CompactSession => {
                unreachable!("compaction is returned before modal reset");
            }
        }
        self.sync_modal_cursor();
        outcome
    }

    /// Panel navigation: Up/Down move the panel cursor, Enter chooses,
    /// Esc closes; text and paste belong to the focused modal search.
    pub fn handle_panel_key(&mut self, action: KeyAction) -> KeyOutcome {
        if self.panel == TuiPanel::Accounts {
            if self.approvals.active().is_some() || self.questions.active().is_some() {
                self.close_panel();
                return KeyOutcome::default();
            }
            return self.accounts_key(action);
        }
        if self.approvals.active().is_some() {
            return self.approvals.key(action);
        }
        if self.questions.active().is_some() {
            return self.questions.key(action);
        }
        if self.panel == TuiPanel::Settings
            && action == KeyAction::Cancel
            && !self.select.query.is_empty()
        {
            self.select.reset();
            self.changed_modal_query();
            return KeyOutcome::default();
        }
        if self.panel == TuiPanel::Settings
            && matches!(action, KeyAction::Left | KeyAction::Right)
            && self
                .modal_options()
                .get(self.select.cursor)
                .is_some_and(|option| option.value == "permissions")
        {
            return KeyOutcome {
                intent: Some(PanelIntent::SetPermissionMode {
                    auto_once: !self.chrome.permissions_auto,
                }),
                ..Default::default()
            };
        }
        if self.panel == TuiPanel::Sessions {
            if matches!(action, KeyAction::Rename | KeyAction::DeleteOrQuit) {
                let Some(id) = self.sessions.get(self.sessions_cursor).cloned() else {
                    return KeyOutcome::default();
                };
                if action == KeyAction::DeleteOrQuit {
                    if self.session_delete_confirm.as_ref() == Some(&id) {
                        self.session_delete_confirm = None;
                        return KeyOutcome {
                            intent: Some(PanelIntent::DeleteSelectedSession { id }),
                            ..KeyOutcome::default()
                        };
                    }
                    self.session_delete_confirm = Some(id);
                } else {
                    self.rename_input = self
                        .session_entries
                        .iter()
                        .find(|entry| entry.id.0 == id)
                        .map(|entry| entry.title.clone())
                        .unwrap_or_default();
                    self.rename_editor.clear();
                    self.rename_editor.cursor = self.rename_input.len();
                    self.rename_pending = None;
                    self.rename_selected = Some(id);
                    self.session_delete_confirm = None;
                    self.panel = TuiPanel::Rename;
                }
                return KeyOutcome::default();
            }
            // Matches donor onMove; search/scope changes also retire intent.
            self.session_delete_confirm = None;
        }
        if self.panel == TuiPanel::Sessions && action == KeyAction::CtrlA {
            self.sessions_all_projects = !self.sessions_all_projects;
            self.session_scope_pending = Some(self.sessions_all_projects);
            self.select.changed_query();
            return KeyOutcome {
                intent: Some(PanelIntent::LoadSessions),
                ..KeyOutcome::default()
            };
        }
        let action = if action == KeyAction::CtrlA {
            KeyAction::Home
        } else {
            action
        };
        if self.panel == TuiPanel::Rename {
            return self.handle_rename_key(action);
        }
        if self.panel == TuiPanel::Mcps {
            if action == KeyAction::Char(' ') {
                return self.mcp_toggle();
            }
            if action == KeyAction::Cancel && self.mcp_detail.take().is_some() {
                self.select.reset();
                return KeyOutcome::default();
            }
        }
        if self.panel == TuiPanel::Cards && self.card_output.is_some() {
            let (start, height, count) = crate::views::card_window(self);
            self.card_scroll = start;
            match action {
                KeyAction::Up => self.card_scroll = start.saturating_sub(1),
                KeyAction::Down => self.card_scroll = (start + 1).min(count.saturating_sub(height)),
                KeyAction::PageUp => self.card_scroll = start.saturating_sub(height),
                KeyAction::PageDown => {
                    self.card_scroll = (start + height).min(count.saturating_sub(height))
                }
                KeyAction::Home => self.card_scroll = 0,
                KeyAction::End => self.card_scroll = count.saturating_sub(height),
                KeyAction::Enter
                    if height > 0 && start + height >= count && self.card_seen.get() >= count =>
                {
                    return self.panel_enter();
                }
                KeyAction::Cancel => {
                    self.card_output = None;
                    self.card_scroll = 0;
                    self.select.reset();
                }
                KeyAction::Quit | KeyAction::Interrupt => self.status = TuiStatus::Quit,
                _ => {}
            }
            return KeyOutcome::default();
        }
        match action {
            KeyAction::Char(c) => {
                if self.select.query.len() + c.len_utf8() <= 512 {
                    self.select.query.push(c);
                }
                self.changed_modal_query();
                return KeyOutcome {
                    intent: (self.panel == TuiPanel::Sessions).then_some(PanelIntent::LoadSessions),
                    ..KeyOutcome::default()
                };
            }
            KeyAction::Backspace => {
                self.select.query.pop();
                self.changed_modal_query();
                return KeyOutcome {
                    intent: (self.panel == TuiPanel::Sessions).then_some(PanelIntent::LoadSessions),
                    ..KeyOutcome::default()
                };
            }
            KeyAction::Interrupt
                if matches!(
                    self.panel,
                    TuiPanel::Dcp | TuiPanel::Cards | TuiPanel::Help(_)
                ) =>
            {
                // Existing informational-panel shutdown binding (including
                // the post-compression DCP panel); Select dialogs retain their
                // own clear-filter/dismiss behavior.
                self.status = TuiStatus::Quit;
                return KeyOutcome::default();
            }
            KeyAction::Interrupt => {
                if self.select.query.is_empty() {
                    self.close_panel();
                } else {
                    self.select.reset();
                    self.changed_modal_query();
                    if self.panel == TuiPanel::Sessions {
                        return KeyOutcome {
                            intent: Some(PanelIntent::LoadSessions),
                            ..KeyOutcome::default()
                        };
                    }
                }
                return KeyOutcome::default();
            }
            KeyAction::Commands
            | KeyAction::Up
            | KeyAction::Down
            | KeyAction::PageUp
            | KeyAction::PageDown
            | KeyAction::Home
            | KeyAction::End
                if matches!(
                    self.panel,
                    TuiPanel::Commands
                        | TuiPanel::Settings
                        | TuiPanel::Model
                        | TuiPanel::Variant
                        | TuiPanel::Agents
                        | TuiPanel::Sessions
                        | TuiPanel::Skills
                        | TuiPanel::Mcps
                        | TuiPanel::MessageActions { .. }
                ) =>
            {
                let count = self.modal_options().len();
                match action {
                    KeyAction::Home => self.select.cursor = 0,
                    KeyAction::End => self.select.cursor = count.saturating_sub(1),
                    _ => self.select.move_by(
                        match action {
                            KeyAction::Commands | KeyAction::Up => -1,
                            KeyAction::PageUp => -10,
                            KeyAction::PageDown => 10,
                            _ => 1,
                        },
                        count,
                    ),
                }
                self.sync_modal_cursor();
                self.select.follow_selection();
                return KeyOutcome::default();
            }
            KeyAction::Enter if self.modal_options().is_empty() => return KeyOutcome::default(),
            _ => {}
        }
        match action {
            KeyAction::Quit => {
                self.status = TuiStatus::Quit;
                KeyOutcome::default()
            }
            KeyAction::Cancel => {
                if self.panel == TuiPanel::Cards && self.card_output.is_some() {
                    self.card_output = None;
                    self.select.reset();
                } else {
                    self.close_panel();
                }
                KeyOutcome::default()
            }
            KeyAction::Left | KeyAction::Right => KeyOutcome::default(),
            KeyAction::Up => {
                self.move_panel_cursor(-1);
                let intent = (self.panel == TuiPanel::Cards
                    && self.cards_has_older
                    && self.cards_cursor == 0)
                    .then_some(PanelIntent::LoadCards);
                KeyOutcome {
                    intent,
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Down => {
                self.move_panel_cursor(1);
                KeyOutcome::default()
            }
            KeyAction::Enter => self.panel_enter(),
            _ => KeyOutcome::default(),
        }
    }

    fn handle_rename_key(&mut self, action: KeyAction) -> KeyOutcome {
        if action == KeyAction::Cancel {
            if self.rename_pending.is_none() {
                self.close_panel();
            }
            return KeyOutcome::default();
        }
        if self.rename_pending.is_some() {
            return KeyOutcome::default();
        }
        if action == KeyAction::Interrupt {
            if self.rename_input.is_empty() {
                self.close_panel();
            } else {
                self.rename_input.clear();
                self.rename_editor.clear();
            }
            return KeyOutcome::default();
        }
        match action {
            KeyAction::Enter => {
                if self.rename_selected.is_none()
                    && let Some(reason) =
                        self.command_unavailable(&CommandAction::RenameSession { title: None })
                {
                    return KeyOutcome {
                        note: Some(reason.into()),
                        ..KeyOutcome::default()
                    };
                }
                if self.rename_input.trim().is_empty() {
                    return KeyOutcome::default();
                }
                if self.rename_input.len() > MAX_SESSION_TITLE_BYTES {
                    if self.session_title.as_deref() == Some(self.rename_input.as_str()) {
                        self.close_panel();
                        return KeyOutcome::default();
                    }
                    return KeyOutcome {
                        note: Some(self.rename_limit_note()),
                        ..KeyOutcome::default()
                    };
                }
                let title = self.rename_input.trim().to_string();
                self.rename_pending = Some(title.clone());
                KeyOutcome {
                    intent: Some(match &self.rename_selected {
                        Some(id) => PanelIntent::RenameSelectedSession {
                            id: id.clone(),
                            title,
                        },
                        None => PanelIntent::RenameSession { title },
                    }),
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Char(c) if !c.is_control() => {
                let inserted = self.rename_editor.replace(
                    &mut self.rename_input,
                    &c.to_string(),
                    MAX_SESSION_TITLE_BYTES,
                );
                KeyOutcome {
                    note: (inserted == 0).then(|| self.rename_limit_note()),
                    ..KeyOutcome::default()
                }
            }
            KeyAction::Backspace | KeyAction::WordBackspace => {
                self.rename_editor.delete(
                    &mut self.rename_input,
                    true,
                    action == KeyAction::WordBackspace,
                );
                KeyOutcome::default()
            }
            KeyAction::Delete | KeyAction::DeleteOrQuit | KeyAction::WordDelete => {
                self.rename_editor.delete(
                    &mut self.rename_input,
                    false,
                    action == KeyAction::WordDelete,
                );
                KeyOutcome::default()
            }
            KeyAction::Left
            | KeyAction::Right
            | KeyAction::WordLeft
            | KeyAction::WordRight
            | KeyAction::SelectLeft
            | KeyAction::SelectRight
            | KeyAction::SelectWordLeft
            | KeyAction::SelectWordRight => {
                self.rename_editor.horizontal(
                    &self.rename_input,
                    matches!(
                        action,
                        KeyAction::Right
                            | KeyAction::WordRight
                            | KeyAction::SelectRight
                            | KeyAction::SelectWordRight
                    ),
                    matches!(
                        action,
                        KeyAction::WordLeft
                            | KeyAction::WordRight
                            | KeyAction::SelectWordLeft
                            | KeyAction::SelectWordRight
                    ),
                    matches!(
                        action,
                        KeyAction::SelectLeft
                            | KeyAction::SelectRight
                            | KeyAction::SelectWordLeft
                            | KeyAction::SelectWordRight
                    ),
                );
                KeyOutcome::default()
            }
            KeyAction::Home | KeyAction::End | KeyAction::SelectHome | KeyAction::SelectEnd => {
                self.rename_editor.move_to(
                    if matches!(action, KeyAction::End | KeyAction::SelectEnd) {
                        self.rename_input.len()
                    } else {
                        0
                    },
                    matches!(action, KeyAction::SelectHome | KeyAction::SelectEnd),
                );
                KeyOutcome::default()
            }
            KeyAction::Undo | KeyAction::Redo => {
                self.rename_editor
                    .undo(&mut self.rename_input, action == KeyAction::Redo);
                KeyOutcome::default()
            }
            _ => KeyOutcome::default(),
        }
    }

    fn move_panel_cursor(&mut self, delta: isize) {
        match self.panel {
            TuiPanel::Model => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.move_cursor(delta);
                }
            }
            TuiPanel::Agents => {
                self.agents_cursor = clamp_cursor(self.agents_cursor, delta, self.agents.len());
            }
            TuiPanel::Sessions => {
                self.sessions_cursor =
                    clamp_cursor(self.sessions_cursor, delta, self.sessions.len());
            }
            TuiPanel::Skills => {
                self.skills_cursor = clamp_cursor(self.skills_cursor, delta, self.skills.len());
            }
            TuiPanel::Cards => {
                self.cards_cursor = clamp_cursor(self.cards_cursor, delta, self.cards.len());
            }
            _ => {}
        }
    }

    pub(super) fn panel_enter(&mut self) -> KeyOutcome {
        let mut outcome = KeyOutcome::default();
        match self.panel.clone() {
            TuiPanel::MessageActions { message, seq } => {
                if !self
                    .window
                    .rows()
                    .iter()
                    .any(|row| row.role == "user" && row.message_id.as_deref() == Some(&message))
                {
                    outcome.note =
                        Some("message is no longer in the current history window".into());
                    return outcome;
                }
                let options = self.modal_options();
                match options
                    .get(self.select.cursor)
                    .map(|option| option.value.as_str())
                {
                    Some("jump") => self.close_panel(),
                    Some("revert") => {
                        outcome.intent = Some(PanelIntent::ChangeConversation {
                            action: oc_core::queries::ConversationAction::Revert { message },
                        })
                    }
                    Some("copy") => {
                        outcome.intent = Some(PanelIntent::CopyMessage { message, seq })
                    }
                    Some("fork") => outcome.intent = Some(PanelIntent::ForkMessage { message }),
                    _ => {}
                }
            }
            TuiPanel::Commands => {
                let options = self.modal_options();
                if let Some(option) = options.get(self.select.cursor)
                    && let Some(command) = crate::commands::REGISTRY
                        .iter()
                        .find(|c| c.id == option.value)
                {
                    return self.run_command(command.action.clone());
                }
            }
            TuiPanel::Rename => return self.handle_rename_key(KeyAction::Enter),
            TuiPanel::Accounts => return self.accounts_key(KeyAction::Enter),
            TuiPanel::Agents if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenAgents)
                    .map(str::to_string)
            }
            TuiPanel::Sessions if self.is_busy() => {
                outcome.note = self
                    .command_unavailable(&CommandAction::OpenSessions)
                    .map(str::to_string)
            }
            TuiPanel::Model => match self.picker_selection() {
                Some((id, _)) => {
                    outcome.intent = Some(PanelIntent::SelectModel { id });
                }
                None => outcome.note = Some("no model selected".to_string()),
            },
            TuiPanel::Variant => {
                if let Some(option) = self.modal_options().get(self.select.cursor)
                    && let Some(selection) = self.picker.as_ref().and_then(|p| p.selection())
                {
                    outcome.intent = Some(PanelIntent::ChooseModel {
                        id: selection.id.clone(),
                        variant: (option.value != "default").then(|| option.value.clone()),
                    });
                }
            }
            TuiPanel::Settings => {
                if let Some(option) = self.modal_options().get(self.select.cursor) {
                    if option.value == "permissions" {
                        outcome.intent = Some(PanelIntent::SetPermissionMode {
                            auto_once: !self.chrome.permissions_auto,
                        });
                    } else {
                        outcome.note = Some(option.footer.clone());
                    }
                }
            }
            TuiPanel::Agents => match self.selected_agent() {
                Some(id) => outcome.intent = Some(PanelIntent::SelectAgent { id }),
                None => outcome.note = Some("no agent selected".to_string()),
            },
            TuiPanel::Sessions => match self.sessions.get(self.sessions_cursor) {
                Some(id) if SessionId::new(id.clone()).is_some() => {
                    outcome.intent = Some(PanelIntent::SwitchSession { id: id.clone() });
                }
                Some(_) => outcome.note = Some("bad session id".to_string()),
                None => outcome.note = Some("empty session list".to_string()),
            },
            TuiPanel::Skills => {
                self.panel = TuiPanel::None;
            }
            TuiPanel::Mcps => return self.mcp_enter(),
            TuiPanel::Cards => {
                if self.session.is_none() {
                    outcome.note = Some("no session yet".into());
                    return outcome;
                }
                if let Some(detail) = &self.card_output {
                    if let Some(offset) = detail.page.next_offset {
                        outcome.intent = Some(PanelIntent::LoadCardOutput {
                            op: detail.op.clone(),
                            offset: offset as usize,
                        });
                    } else {
                        self.card_output = None;
                    }
                } else if let Some(op) = self.card_ops.get(self.cards_cursor) {
                    outcome.intent = Some(PanelIntent::LoadCardOutput {
                        op: op.clone(),
                        offset: 0,
                    });
                }
            }
            TuiPanel::Dcp => {
                if !self.chrome.dcp.commands_enabled {
                    return self.run_command(CommandAction::OpenDcp);
                }
                if let Some(reason) = self.dcp.manual_refusal() {
                    outcome.note = Some(reason.reason().into());
                    return outcome;
                }
                if self.session.is_some() {
                    outcome.intent = Some(PanelIntent::Compress {
                        focus: String::new(),
                    });
                } else {
                    outcome.note = Some("no session yet".into());
                }
            }
            TuiPanel::Help(_) | TuiPanel::None => {
                self.panel = TuiPanel::None;
            }
        }
        outcome
    }
}
