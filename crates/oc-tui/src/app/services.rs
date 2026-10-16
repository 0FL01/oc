//! Change-sensitive feedback from the current owned projections, never dialogue.

use super::*;
use oc_core::queries::{
    McpStatus, PluginStatus, ProviderStatus, ServiceAction, ServiceDiagnostic, ServiceKind,
};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum ServiceIssue {
    Diagnostic(ServiceDiagnostic),
    McpDiagnostic {
        server: String,
        diagnostic: ServiceDiagnostic,
    },
    Unavailable {
        kind: ServiceKind,
        service: String,
        source: String,
        status: &'static str,
    },
    Omitted(ServiceKind, usize),
}

impl TuiState {
    fn mcp_diagnostic_server(&self, service: &str) -> Option<&oc_core::queries::McpServerSnapshot> {
        self.mcp_snapshot.as_ref()?.servers.iter().find(|server| {
            server.id == service
                || server
                    .diagnostic
                    .as_ref()
                    .is_some_and(|diagnostic| diagnostic.service == service)
                || self.service_pending_issues.iter().any(|issue| {
                    matches!(issue, ServiceIssue::McpDiagnostic { server: id, diagnostic }
                        if id == &server.id && diagnostic.service == service)
                })
        })
    }

    pub(super) fn submission_note(&mut self, error: &CoreError) -> String {
        let diagnostic = match error {
            CoreError::ProviderUnavailable(diagnostic) => diagnostic,
            CoreError::Diagnostic(diagnostic)
                if matches!(
                    diagnostic.kind,
                    ServiceKind::Selection | ServiceKind::Provider
                ) =>
            {
                diagnostic
            }
            _ => {
                self.refused_submission = None;
                return format!("submit: {error}");
            }
        };
        self.refused_submission = Some(diagnostic.clone());
        let action = match diagnostic.action {
            ServiceAction::SelectModel => "/models",
            ServiceAction::SelectAgent => "/agents",
            ServiceAction::SelectVariant => "/variants",
            ServiceAction::Reauthenticate => "/accounts",
            ServiceAction::WaitForProvider => "wait /settings",
            ServiceAction::RefreshCatalog => "/reload",
            ServiceAction::RetryConnection if diagnostic.kind == ServiceKind::Mcp => "/mcps",
            ServiceAction::RetryConnection => "/reload",
            ServiceAction::RestartApplication => "restart /settings",
            _ => "/settings",
        };
        format!(
            "Request unavailable: {} · {action}",
            diagnostic.code.as_str()
        )
    }

    pub(super) fn service_issues(&self) -> Vec<ServiceIssue> {
        let mut issues = Vec::new();
        let mut add = |issue| {
            if !issues.contains(&issue) {
                issues.push(issue);
            }
        };
        for diagnostic in &self.chrome.service_diagnostics {
            if diagnostic.kind == ServiceKind::Provider
                && self.chrome.provider.as_ref().is_some_and(|provider| {
                    provider.service == diagnostic.service
                        && provider.status == ProviderStatus::Pending
                })
                || diagnostic.kind == ServiceKind::Mcp
                    && self.mcp_diagnostic_server(&diagnostic.service).is_some()
            {
                continue;
            }
            add(ServiceIssue::Diagnostic(diagnostic.clone()));
        }
        for plugin in &self.chrome.plugins.entries {
            if plugin.status != PluginStatus::Active {
                add(match &plugin.diagnostic {
                    Some(diagnostic) => ServiceIssue::Diagnostic(diagnostic.clone()),
                    None => ServiceIssue::Unavailable {
                        kind: ServiceKind::Plugin,
                        service: plugin.requested.clone(),
                        source: plugin.source.clone(),
                        status: plugin.status.as_str(),
                    },
                });
            }
        }
        if let Some(provider) = &self.chrome.provider
            && provider.status == ProviderStatus::Unavailable
        {
            add(match &provider.diagnostic {
                Some(diagnostic) => ServiceIssue::Diagnostic(diagnostic.clone()),
                None => ServiceIssue::Unavailable {
                    kind: ServiceKind::Provider,
                    service: provider.service.clone(),
                    source: provider.model.clone(),
                    status: provider.status.as_str(),
                },
            });
        }
        if let Some(selection) = &self.chrome.selection {
            add(ServiceIssue::Diagnostic(selection.diagnostic.clone()));
        }
        if let Some(snapshot) = &self.mcp_snapshot {
            for server in &snapshot.servers {
                if server.pending_action.is_none()
                    && matches!(server.status, McpStatus::Failed | McpStatus::NeedsAuth)
                {
                    add(match &server.diagnostic {
                        Some(diagnostic) => ServiceIssue::McpDiagnostic {
                            server: server.id.clone(),
                            diagnostic: diagnostic.clone(),
                        },
                        None => match self.chrome.service_diagnostics.iter().find(|diagnostic| {
                            diagnostic.kind == ServiceKind::Mcp
                                && self
                                    .mcp_diagnostic_server(&diagnostic.service)
                                    .is_some_and(|owner| owner.id == server.id)
                        }) {
                            Some(diagnostic) => ServiceIssue::McpDiagnostic {
                                server: server.id.clone(),
                                diagnostic: diagnostic.clone(),
                            },
                            None => ServiceIssue::Unavailable {
                                kind: ServiceKind::Mcp,
                                service: server.id.clone(),
                                source: snapshot.binding.location.clone(),
                                status: if server.status == McpStatus::Failed {
                                    "failed"
                                } else {
                                    "needs-auth"
                                },
                            },
                        },
                    });
                }
            }
        }
        if self.chrome.plugins.omitted_failed > 0 {
            add(ServiceIssue::Omitted(
                ServiceKind::Plugin,
                self.chrome.plugins.omitted_failed,
            ));
        }
        if self.chrome.service_diagnostics_omitted > 0 {
            add(ServiceIssue::Omitted(
                ServiceKind::Configuration,
                self.chrome.service_diagnostics_omitted,
            ));
        }
        issues
    }

    pub(crate) fn service_issue_count(&self) -> usize {
        self.service_issues()
            .iter()
            .map(|issue| match issue {
                ServiceIssue::Omitted(_, count) => *count,
                _ => 1,
            })
            .sum()
    }

    pub(crate) fn service_pending_count(&self) -> usize {
        usize::from(self.chrome.provider.as_ref().is_some_and(|provider| {
            provider.status == ProviderStatus::Pending
                || provider.catalog_status == ProviderStatus::Pending
        })) + self.mcp_snapshot.as_ref().map_or(0, |snapshot| {
            snapshot
                .servers
                .iter()
                .filter(|server| {
                    server.status == McpStatus::Pending || server.pending_action.is_some()
                })
                .count()
        })
    }

    /// The tab-deck owner marks parked projections silent. They still consume
    /// current facts, so activating a view cannot replay an old service alert.
    pub fn set_service_feedback_visible(&mut self, visible: bool) {
        self.service_feedback_visible = visible;
        if !visible && self.service_note {
            self.note = None;
            self.toast_expiry = None;
            self.toast_down = false;
            self.service_note = false;
        }
    }

    /// Keep all turn-specific warnings, even if their text equals a service
    /// notice. Invalid provenance fails open to visibility, never hides an error.
    pub fn apply_turn_warnings(
        &mut self,
        warnings: Vec<String>,
        service_range: std::ops::Range<usize>,
    ) {
        let service_range =
            if service_range.start <= service_range.end && service_range.end <= warnings.len() {
                service_range
            } else {
                0..0
            };
        for (index, warning) in warnings.into_iter().enumerate() {
            if !service_range.contains(&index) {
                self.push_warning(&warning);
            }
        }
    }

    fn service_issue_is_pending(&self, issue: &ServiceIssue) -> bool {
        let (kind, service) = match issue {
            ServiceIssue::Diagnostic(diagnostic) => (diagnostic.kind, diagnostic.service.as_str()),
            ServiceIssue::McpDiagnostic { server, .. } => (ServiceKind::Mcp, server.as_str()),
            ServiceIssue::Unavailable { kind, service, .. } => (*kind, service.as_str()),
            ServiceIssue::Omitted(..) => return false,
        };
        match kind {
            ServiceKind::Mcp => self.mcp_snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.servers.iter().any(|server| {
                    (server.id == service
                        || server
                            .diagnostic
                            .as_ref()
                            .is_some_and(|diagnostic| diagnostic.service == service))
                        && (server.status == McpStatus::Pending || server.pending_action.is_some())
                })
            }),
            ServiceKind::Provider => self.chrome.provider.as_ref().is_some_and(|provider| {
                provider.service == service && provider.status == ProviderStatus::Pending
            }),
            _ => false,
        }
    }

    pub(super) fn reconcile_service_feedback(&mut self, mut previous: Vec<ServiceIssue>) {
        for issue in std::mem::take(&mut self.service_pending_issues) {
            if !previous.contains(&issue) {
                previous.push(issue);
            }
        }
        // A diagnostic can arrive through the catalog before the owned row.
        // Link it through typed provenance, never a mutable display label.
        for issue in &mut previous {
            if let ServiceIssue::Diagnostic(diagnostic) = issue
                && diagnostic.kind == ServiceKind::Mcp
                && let Some(server) = self.mcp_diagnostic_server(&diagnostic.service)
            {
                *issue = ServiceIssue::McpDiagnostic {
                    server: server.id.clone(),
                    diagnostic: diagnostic.clone(),
                };
            }
        }
        // At most the last cause of each *current pending* source survives.
        // No source registry or lifetime failure history is retained here.
        self.service_pending_issues = previous
            .iter()
            .filter(|issue| self.service_issue_is_pending(issue))
            .cloned()
            .collect();
        let current = self.service_issues();
        let count = self.service_issue_count();
        let summary = format!(
            "{count} service issue{} · /settings /mcps",
            if count == 1 { "" } else { "s" }
        );
        if self.service_feedback_visible && current.iter().any(|issue| !previous.contains(issue)) {
            self.push_transient_note(&summary, NoteVariant::Warning);
            self.service_note = true;
        } else if self.service_note {
            if count == 0 {
                self.note = None;
                self.toast_expiry = None;
                self.toast_down = false;
                self.service_note = false;
            } else if let Some((note, _)) = &mut self.note {
                // Recovery adjusts current counts without restarting an alert.
                *note = summary;
            }
        }
    }
}
