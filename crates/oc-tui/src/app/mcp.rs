//! Direct typed MCP projection; no endpoint, credential or inferred status.

use super::*;
use oc_core::queries::{McpAction, McpControl, McpSnapshot, McpStatus};

impl TuiState {
    pub fn apply_mcp_snapshot(&mut self, snapshot: McpSnapshot) {
        if self.chrome.location.as_deref() != Some(snapshot.binding.location.as_str()) {
            return;
        }
        if let Some(previous) = &self.mcp_snapshot
            && (snapshot.binding.instance < previous.binding.instance
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
        let warnings: Vec<_> = snapshot
            .servers
            .iter()
            .filter_map(|server| {
                let diagnostic = server.diagnostic.as_ref()?;
                let previous = self
                    .mcp_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.servers.iter().find(|old| old.id == server.id))
                    .and_then(|old| old.diagnostic.as_ref());
                (previous != Some(diagnostic)).then(|| diagnostic.to_string())
            })
            .collect();
        self.mcp_snapshot = Some(snapshot);
        if let Some(focus) = focus {
            let options = self.modal_options();
            if let Some(index) = options.iter().position(|option| option.value == focus) {
                self.select.cursor = index;
            } else {
                self.select.cursor = self.select.cursor.min(options.len().saturating_sub(1));
            }
        }
        for warning in warnings {
            self.push_warning(&warning);
        }
    }

    pub(super) fn mcp_options(&self) -> Vec<crate::dialog::SelectOption> {
        let Some(snapshot) = &self.mcp_snapshot else {
            return Vec::new();
        };
        if let Some(id) = &self.mcp_detail {
            let Some(server) = snapshot.servers.iter().find(|server| &server.id == id) else {
                return Vec::new();
            };
            let lines = match &server.diagnostic {
                Some(diagnostic) => vec![
                    format!("Server: {}", server.name),
                    format!("Stage: {}", diagnostic.stage.as_str()),
                    format!("Code: {}", diagnostic.code.as_str()),
                    format!("Source: {}", diagnostic.source),
                    format!("Field: {}", diagnostic.field.join(".")),
                    diagnostic.to_string(),
                ],
                None => vec![
                    format!("Server: {}", server.name),
                    format!("Tools: {}", server.tools),
                ],
            };
            return lines
                .into_iter()
                .enumerate()
                .map(|(index, title)| crate::dialog::SelectOption {
                    value: index.to_string(),
                    title,
                    category: String::new(),
                    footer: String::new(),
                    current: false,
                    running: false,
                    destructive: false,
                })
                .collect();
        }
        snapshot
            .servers
            .iter()
            .map(|server| crate::dialog::SelectOption {
                value: server.id.clone(),
                title: server.name.clone(),
                category: String::new(),
                footer: match (server.pending_action, server.status) {
                    (Some(McpAction::Disconnect), _) => "Disconnecting …",
                    (_, McpStatus::Pending) => "Connecting …",
                    (_, McpStatus::Connected) => "Connected ✓",
                    (_, McpStatus::Disabled) => "Disabled ○",
                    (_, McpStatus::Failed) => "Failed !",
                    (_, McpStatus::NeedsAuth) => "Sign in required (unsupported)",
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
        if self.mcp_detail.take().is_none() {
            self.mcp_detail = self
                .modal_options()
                .get(self.select.cursor)
                .map(|option| option.value.clone());
        }
        self.select.reset();
        KeyOutcome::default()
    }

    pub fn mcp_footer(&self) -> &str {
        if self.mcp_detail.is_some() {
            return "enter/esc back · safe details";
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
