//! Direct typed MCP projection; no endpoint, credential or inferred status.

use super::*;
use oc_core::queries::{
    McpAction, McpBinding, McpControl, McpServerSnapshot, McpSnapshot, McpStatus,
};

pub(super) struct McpDetail {
    binding: McpBinding,
    pub(super) server: String,
    pub(super) scroll: usize,
    pub(super) copied: bool,
    pub(super) copy_pending: bool,
    pub(super) pressed: Option<(Rect, crate::dialog::DialogHit)>,
}

impl TuiState {
    pub(crate) fn mcp_list_servers(&self) -> Option<&[McpServerSnapshot]> {
        if self.panel != TuiPanel::Mcps || self.mcp_detail.is_some() {
            return None;
        }
        self.mcp_snapshot
            .as_ref()
            .map(|snapshot| snapshot.servers.as_slice())
    }

    /// Live inventory only: unavailable/empty and disabled are not failures.
    pub(crate) fn mcp_status_counts(&self) -> Option<(usize, usize)> {
        let snapshot = self.mcp_snapshot.as_ref()?;
        if snapshot.servers.is_empty() {
            return None;
        }
        Some(
            snapshot
                .servers
                .iter()
                .fold((0, 0), |(connected, failed), server| {
                    (
                        connected + usize::from(server.status == McpStatus::Connected),
                        failed + usize::from(server.status == McpStatus::Failed),
                    )
                }),
        )
    }

    pub fn apply_mcp_snapshot(&mut self, snapshot: McpSnapshot) {
        // bind_events can announce the new owner's constructor before start
        // publishes its admitted inventory. Revision zero is not recovery or
        // service removal; even a genuinely empty config is published at one.
        if snapshot.revision == 0 {
            return;
        }
        if self.chrome.location.as_deref() != Some(snapshot.binding.location.as_str()) {
            return;
        }
        if let Some(previous) = &self.mcp_snapshot
            && (snapshot.binding.instance < previous.binding.instance
                || snapshot.binding.instance == previous.binding.instance
                    && snapshot.binding.generation < previous.binding.generation
                || snapshot.binding == previous.binding && snapshot.revision < previous.revision)
        {
            return;
        }
        let focus = if self.panel == TuiPanel::Mcps {
            self.modal_options()
                .get(self.select.cursor)
                .map(|option| option.value.clone())
        } else {
            None
        };
        let previous_issues = self.service_issues();
        let detail_changed = self.mcp_detail.as_ref().is_some_and(|detail| {
            let before = self
                .mcp_snapshot
                .as_ref()
                .and_then(|old| old.servers.iter().find(|row| row.id == detail.server));
            let after = snapshot.servers.iter().find(|row| row.id == detail.server);
            before.map(|row| (&row.name, &row.diagnostic))
                != after.map(|row| (&row.name, &row.diagnostic))
                || snapshot.binding != detail.binding
        });
        if detail_changed && let Some(detail) = &mut self.mcp_detail {
            if detail.copy_pending {
                self.pending_copy = None;
            }
            detail.copy_pending = false;
            detail.copied = false;
            detail.pressed = None;
        }
        self.mcp_snapshot = Some(snapshot);
        if self.mcp_detail.is_some() && self.mcp_detail_server().is_none() {
            // A replacement generation/recovery must not leave a stale detail
            // (or its copy/investigate actions) attached to another owner.
            self.mcp_detail = None;
        }
        if let Some(focus) = focus {
            let options = self.modal_options();
            if let Some(index) = options.iter().position(|option| option.value == focus) {
                self.select.cursor = index;
            } else {
                self.select.cursor = self.select.cursor.min(options.len().saturating_sub(1));
            }
        }
        self.reconcile_service_feedback(previous_issues);
    }

    pub(super) fn mcp_options(&self) -> Vec<crate::dialog::SelectOption> {
        let Some(snapshot) = &self.mcp_snapshot else {
            return Vec::new();
        };
        snapshot
            .servers
            .iter()
            .map(|server| crate::dialog::SelectOption {
                value: server.id.clone(),
                title: server.name.clone(),
                category: String::new(),
                footer: match (server.pending_action.is_some(), server.status) {
                    (true, _) | (_, McpStatus::Pending) => "Connecting …",
                    (_, McpStatus::Connected) => "Connected ✓",
                    (_, McpStatus::Disabled) => "Disabled ○",
                    (_, McpStatus::Failed) => "Failed !",
                    (_, McpStatus::NeedsAuth) => "Sign in required →",
                }
                .into(),
                current: false,
                running: false,
                destructive: false,
            })
            .collect()
    }

    pub(super) fn mcp_toggle(&mut self) -> KeyOutcome {
        if self.mcp_detail.is_some() {
            return KeyOutcome::default();
        }
        let options = self.modal_options();
        let Some(option) = options.get(self.select.cursor) else {
            return KeyOutcome::default();
        };
        let Some(snapshot) = &self.mcp_snapshot else {
            return KeyOutcome::default();
        };
        let Some(server) = snapshot
            .servers
            .iter()
            .find(|server| server.id == option.value)
        else {
            return KeyOutcome::default();
        };
        let Some(action) = server.actions.first().copied() else {
            return KeyOutcome {
                note: server.diagnostic.as_ref().map(ToString::to_string),
                ..Default::default()
            };
        };
        KeyOutcome {
            intent: Some(PanelIntent::McpControl(McpControl {
                binding: snapshot.binding.clone(),
                server: server.id.clone(),
                action,
            })),
            ..Default::default()
        }
    }

    pub(super) fn mcp_enter(&mut self) -> KeyOutcome {
        let options = self.modal_options();
        let Some(snapshot) = &self.mcp_snapshot else {
            return KeyOutcome::default();
        };
        let Some(server) = options.get(self.select.cursor).and_then(|option| {
            snapshot
                .servers
                .iter()
                .find(|server| server.id == option.value)
        }) else {
            return KeyOutcome::default();
        };
        // Enter is not a connection action, nor a dismissal for healthy,
        // disabled or pending rows. Native unsupported-auth diagnostics remain
        // honest safe details, never a pretend OAuth flow.
        if matches!(server.status, McpStatus::Failed | McpStatus::NeedsAuth)
            && server.diagnostic.is_some()
        {
            self.mcp_detail = Some(McpDetail {
                binding: snapshot.binding.clone(),
                server: server.id.clone(),
                scroll: 0,
                copied: false,
                copy_pending: false,
                pressed: None,
            });
            self.mouse_down = None;
        }
        KeyOutcome::default()
    }

    pub(crate) fn mcp_detail_server(&self) -> Option<&McpServerSnapshot> {
        if self.panel != TuiPanel::Mcps {
            return None;
        }
        let detail = self.mcp_detail.as_ref()?;
        let snapshot = self.mcp_snapshot.as_ref()?;
        if snapshot.binding != detail.binding {
            return None;
        }
        snapshot
            .servers
            .iter()
            .find(|server| server.id == detail.server && server.diagnostic.is_some())
    }

    pub(crate) fn mcp_detail_scroll(&self) -> usize {
        self.mcp_detail.as_ref().map_or(0, |detail| detail.scroll)
    }

    pub(crate) fn mcp_detail_copied(&self) -> bool {
        self.mcp_detail.as_ref().is_some_and(|detail| detail.copied)
    }

    pub(super) fn mcp_detail_key(&mut self, action: KeyAction) -> KeyOutcome {
        let layout = crate::dialog::mcp_detail_layout(self, self.detail_area());
        let start = self
            .mcp_detail_scroll()
            .min(layout.count.saturating_sub(layout.body.height as usize));
        if let Some(detail) = &mut self.mcp_detail {
            let end = layout.count.saturating_sub(layout.body.height as usize);
            detail.scroll = match action {
                KeyAction::Up => start.saturating_sub(1),
                KeyAction::Down => (start + 1).min(end),
                KeyAction::PageUp => start.saturating_sub(20),
                KeyAction::PageDown => (start + 20).min(end),
                KeyAction::Home | KeyAction::CtrlA => 0,
                KeyAction::End => end,
                _ => start,
            };
        }
        match action {
            KeyAction::Cancel | KeyAction::Interrupt => {
                self.mcp_detail = None;
                self.mouse_down = None;
            }
            KeyAction::Char('c') => {
                if let Some(server) = self.mcp_detail_server() {
                    let text = format!(
                        "MCP server: {}\nError: {}",
                        server.name,
                        server.diagnostic.as_ref().expect("detail diagnostic")
                    );
                    match self.copy_message_text(text) {
                        Ok(()) => {
                            if let Some(detail) = &mut self.mcp_detail {
                                detail.copy_pending = true;
                                detail.copied = false;
                            }
                        }
                        Err(error) => self.push_transient_note(&error, NoteVariant::Error),
                    }
                }
            }
            KeyAction::Char('i') => {
                if self.linked_child().is_some() {
                    return KeyOutcome {
                        note: Some("linked child is read-only".into()),
                        ..Default::default()
                    };
                }
                if let Some(server) = self.mcp_detail_server() {
                    let draft = format!(
                        "MCP server: {}\n{}",
                        server.name,
                        server
                            .diagnostic
                            .as_ref()
                            .expect("detail diagnostic")
                            .investigation_draft()
                    );
                    if draft.len() > MAX_INPUT_BYTES {
                        return KeyOutcome {
                            note: Some("diagnostic exceeds composer input limit".into()),
                            ..Default::default()
                        };
                    }
                    self.close_panel();
                    self.restore_prompt(draft);
                }
            }
            _ => {}
        }
        KeyOutcome::default()
    }

    pub fn mcp_footer(&self) -> &str {
        if self.mcp_detail.is_some() {
            return "esc back · c copy details · i investigate · ↑/↓ scroll";
        }
        let options = self.modal_options();
        let server = options.get(self.select.cursor).and_then(|option| {
            self.mcp_snapshot
                .as_ref()?
                .servers
                .iter()
                .find(|server| server.id == option.value)
        });
        match server.and_then(|server| server.actions.first()) {
            Some(McpAction::Connect) => "space connect · enter details",
            Some(McpAction::Disconnect) => "space disconnect · enter details",
            Some(McpAction::Retry) => "space retry · enter details",
            None if server.is_some_and(|server| {
                server.diagnostic.as_ref().is_some_and(|diagnostic| {
                    diagnostic.action == oc_core::queries::ServiceAction::SignInUnsupported
                })
            }) =>
            {
                "enter safe details · OAuth sign-in unsupported"
            }
            None if server.is_some_and(|server| server.pending_action.is_some()) => {
                "connection pending · enter details"
            }
            None if server.is_some_and(|server| server.diagnostic.is_some()) => {
                "enter safe details · review configuration"
            }
            None if server.is_none() => "no configured MCP servers",
            None => "controls unavailable · enter safe details",
        }
    }
}
