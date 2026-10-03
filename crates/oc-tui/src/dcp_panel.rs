//! DCP context panel (UI04): context/stats snapshot from the runtime,
//! manual `/dcp-compress` request with bounded focus, and transient outcome
//! notifications that never duplicate history.
//!
//! The panel owns no authoritative counters: snapshots arrive from the
//! runtime as [`DcpContextSnapshot`], requests go back out for the runtime
//! to execute, outcomes return as transient notices.

use crate::dcp_view::format_tokens;

/// Context/stats snapshot fed by the runtime (counts only, no transcript).
pub use oc_core::queries::DcpSnapshot as DcpContextSnapshot;

/// Max focus instruction bytes (`/dcp-compress` focus is bounded).
pub const FOCUS_MAX: usize = 256;
/// Max notice bytes shown in the status area.
pub const NOTICE_MAX: usize = 120;

/// Manual compress request: bounded focus instruction for the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressRequest {
    /// Focus span description (empty = whole eligible span).
    pub focus: String,
}

/// Compress outcome reported back by the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DcpOutcome {
    /// Compression started.
    Started,
    /// Compression completed with saved tokens.
    Done {
        /// Tokens saved.
        saved_tokens: u64,
    },
    /// Compression failed visibly.
    Failed {
        /// Failure reason (no transcript contents).
        reason: String,
    },
}

/// DCP panel state: snapshot in, request out, transient outcome back.
#[derive(Debug, Clone, Default)]
pub struct DcpPanelState {
    snapshot: DcpContextSnapshot,
    pending: Option<CompressRequest>,
    outcome: Option<DcpOutcome>,
    nudge: Option<String>,
    notice: Option<String>,
}

impl DcpPanelState {
    /// Refresh the snapshot from runtime counters.
    pub fn set_snapshot(&mut self, snapshot: DcpContextSnapshot) {
        self.snapshot = snapshot;
    }

    /// Record a manual compress request; the focus becomes a bounded
    /// instruction, never executed here.
    pub fn request_compress(&mut self, focus: &str) -> Result<(), String> {
        let focus = focus.trim();
        if focus.len() > FOCUS_MAX {
            return Err(format!("focus too long (max {FOCUS_MAX} bytes)"));
        }
        self.pending = Some(CompressRequest {
            focus: focus.to_string(),
        });
        self.outcome = None;
        Ok(())
    }

    /// Pending request for the runtime to execute, if any.
    pub fn pending(&self) -> Option<&CompressRequest> {
        self.pending.as_ref()
    }

    /// Clear the pending request (runtime took it).
    pub fn take_pending(&mut self) -> Option<CompressRequest> {
        self.pending.take()
    }

    /// Report a runtime outcome: transient notice, not history.
    pub fn set_outcome(&mut self, outcome: DcpOutcome) {
        let notice = match &outcome {
            DcpOutcome::Started => "dcp: compressing…".to_string(),
            DcpOutcome::Done { saved_tokens } => {
                format!(
                    "dcp: compressed, saved {} tokens",
                    format_tokens(*saved_tokens)
                )
            }
            DcpOutcome::Failed { reason } => format!("dcp failed: {reason}"),
        };
        self.notice = Some(truncate(&notice, NOTICE_MAX));
        self.outcome = Some(outcome);
        self.pending = None;
    }

    /// Surface a runtime nudge hint (transient, never a message).
    pub fn set_nudge(&mut self, hint: Option<String>) {
        self.nudge = hint.map(|hint| truncate(&hint, NOTICE_MAX));
    }

    /// Current transient notice for the status area, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Clear the transient notice (next user action).
    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    /// Bounded panel rows for the view.
    pub fn panel_rows(&self) -> Vec<String> {
        let snapshot = &self.snapshot;
        let mut rows = vec![
            format!(
                "context {} tokens, {} turns since compress",
                if snapshot.estimated_tokens_available {
                    format_tokens(snapshot.estimated_tokens)
                } else {
                    "unavailable".into()
                },
                snapshot.turns_since_compress
            ),
            format!(
                "blocks {} | compressions {} | nudges {} | prunes {}",
                snapshot.blocks, snapshot.compressions, snapshot.nudges, snapshot.prunes
            ),
        ];
        if let Some(facts) = &snapshot.reminders {
            rows.push(format!(
                "reminders min {} | max {} tokens | summary buffer {}",
                format_tokens(facts.min_context),
                format_tokens(facts.max_context),
                facts.summary_buffer
            ));
            rows.push(format!(
                "model {} | context {} tokens ({})",
                facts.model_key,
                format_tokens(facts.model_context),
                if facts.context_from_fallback {
                    "native fallback cap"
                } else {
                    "model metadata"
                }
            ));
            if let Some(warning) = &facts.budget_warning {
                rows.push(format!("budget: {warning}"));
            }
        }
        if let Some(accounting) = &snapshot.accounting {
            rows.push(format!(
                "removed {} | active summary {} | net saved {} tokens",
                format_tokens(accounting.gross_removed),
                format_tokens(accounting.active_summary),
                format_tokens(accounting.net_saved)
            ));
        }
        let method = snapshot
            .accounting
            .as_ref()
            .map_or(snapshot.estimate_method, |accounting| accounting.method);
        let method = match method {
            oc_core::dcp_view::DcpEstimateMethod::Utf16RoundQuarterFallback => {
                "UTF-16 length / 4, half-up fallback"
            }
        };
        rows.push(format!(
            "estimate: {method}{}",
            if snapshot
                .accounting
                .as_ref()
                .is_some_and(|accounting| !accounting.complete)
            {
                " (legacy accounting incomplete)"
            } else {
                ""
            }
        ));
        match &self.pending {
            Some(request) if request.focus.is_empty() => {
                rows.push("pending: compress eligible span".to_string());
            }
            Some(request) => rows.push(format!("pending focus: {}", request.focus)),
            None => rows.push("no pending request".to_string()),
        }
        match &self.outcome {
            Some(DcpOutcome::Done { saved_tokens }) => {
                rows.push(format!(
                    "last: saved {} tokens",
                    format_tokens(*saved_tokens)
                ));
            }
            Some(DcpOutcome::Failed { reason }) => rows.push(format!("last failed: {reason}")),
            Some(DcpOutcome::Started) => rows.push("last: started".to_string()),
            None => {}
        }
        if let Some(nudge) = &self.nudge {
            rows.push(format!("nudge: {nudge}"));
        }
        rows
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::{DcpContextSnapshot, DcpOutcome, DcpPanelState, FOCUS_MAX};

    fn state() -> DcpPanelState {
        let mut panel = DcpPanelState::default();
        panel.set_snapshot(DcpContextSnapshot {
            estimated_tokens_available: true,
            estimate_method: Default::default(),
            accounting: None,
            estimated_tokens: 900,
            max_context: 1000,
            reminders: None,
            turns_since_compress: 3,
            blocks: 2,
            compressions: 1,
            nudges: 4,
            prunes: 0,
        });
        panel
    }

    #[test]
    fn request_outcome_cycle() {
        let mut panel = state();
        panel.request_compress("draft span").expect("request");
        assert_eq!(panel.pending().expect("pending").focus, "draft span");
        panel.set_outcome(DcpOutcome::Done { saved_tokens: 400 });
        assert!(panel.pending().is_none());
        let notice = panel.notice().expect("notice");
        assert!(notice.contains("400"), "{notice}");
        let rows = panel.panel_rows();
        assert!(
            rows.iter().any(|row| row.contains("context 900 tokens")),
            "{rows:?}"
        );
        assert!(rows.iter().any(|row| row.contains("saved 400")), "{rows:?}");
    }

    #[test]
    fn focus_is_bounded() {
        let mut panel = state();
        let error = panel
            .request_compress(&"x".repeat(FOCUS_MAX + 1))
            .expect_err("too long");
        assert!(error.contains("too long"), "{error}");
        assert!(panel.pending().is_none());
    }

    #[test]
    fn nudge_is_transient_text_only() {
        let mut panel = state();
        panel.set_nudge(Some("context high; compress a closed span".to_string()));
        let rows = panel.panel_rows();
        assert!(rows.iter().any(|row| row.contains("nudge:")), "{rows:?}");
        panel.clear_notice();
        assert!(panel.notice().is_none());
    }

    #[test]
    fn unavailable_context_and_precise_accounting_use_owner_metadata_and_shared_labels() {
        use oc_core::dcp_view::DcpAccounting;
        let mut panel = DcpPanelState::default();
        panel.set_snapshot(DcpContextSnapshot {
            max_context: 1_000_000,
            accounting: Some(DcpAccounting {
                gross_removed: 999_950,
                active_summary: 842,
                net_saved: 999_108,
                complete: false,
                ..Default::default()
            }),
            ..Default::default()
        });
        let text = panel.panel_rows().join("\n");
        assert!(text.contains("context unavailable tokens"));
        assert!(text.contains("removed 1M | active summary 842 | net saved 999.1K tokens"));
        assert!(
            text.contains("UTF-16 length / 4, half-up fallback (legacy accounting incomplete)")
        );
        assert_eq!(
            panel.snapshot.accounting.as_ref().unwrap().gross_removed,
            999_950,
            "formatting preserves raw u64 accounting"
        );
        panel.set_outcome(DcpOutcome::Done {
            saved_tokens: 999_950,
        });
        assert_eq!(panel.notice(), Some("dcp: compressed, saved 1M tokens"));
        panel.set_nudge(Some("中文".repeat(50)));
        assert!(
            panel
                .panel_rows()
                .iter()
                .any(|row| row.starts_with("nudge: 中文"))
        );
    }
}
