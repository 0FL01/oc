use super::*;
use oc_core::queries::{
    McpAction, McpBinding, McpServerSnapshot, McpSnapshot, McpStatus, ServiceAction, ServiceCode,
    ServiceDiagnostic, ServiceKind, ServiceStage,
};

fn diagnostic(kind: ServiceKind) -> ServiceDiagnostic {
    ServiceDiagnostic {
        kind,
        service: "service-opaque-fixture".into(),
        source: "source-123/config".into(),
        field: vec!["entry".into()],
        stage: ServiceStage::Config,
        code: ServiceCode::InvalidDefinition,
        action: ServiceAction::ReviewConfiguration,
    }
}

fn mcp(status: McpStatus, diagnostic: Option<ServiceDiagnostic>, revision: u64) -> McpSnapshot {
    McpSnapshot {
        binding: McpBinding {
            location: "/fixture".into(),
            generation: 1,
            instance: 1,
        },
        revision,
        servers: vec![McpServerSnapshot {
            id: "control-fixture".into(),
            name: "configured Unicode Ω MCP".into(),
            configured_enabled: true,
            status,
            pending_action: None,
            tools: 0,
            diagnostic,
            actions: vec![McpAction::Retry],
        }],
    }
}

#[tokio::test]
async fn vis42_submission_refusal_is_brief_but_captured_details_and_draft_survive() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(2);
    let mut state = TuiState::new_home(app);
    state.handle_paste("keep the refused unsent draft");
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh { text, ack, .. }) = inbox.recv().await else {
        panic!("fresh request receipt")
    };
    assert_eq!(text, state.input());
    let mut issue = diagnostic(ServiceKind::Selection);
    issue.code = ServiceCode::ModelUnavailable;
    issue.action = ServiceAction::SelectModel;
    ack.send(Err(CoreError::ProviderUnavailable(issue.clone())))
        .unwrap();
    state.poll_submission();
    assert_eq!(
        state.note(),
        Some("Request unavailable: model_unavailable · /models")
    );
    assert_eq!(state.input(), "keep the refused unsent draft");
    assert!(state.attached_session().is_none());
    assert!(state.window.rows().is_empty());
    assert!(state.active_turn().is_none());

    // Current readiness may already have changed; details remain the actual
    // captured refusal, not a fabricated error on the new current binding.
    state.apply_catalog(snapshot());
    state.panel = TuiPanel::Settings;
    state.select.cursor = state
        .modal_options()
        .iter()
        .position(|option| option.value == "submission-refusal")
        .expect("bounded last request fact");
    let detail = state.handle_panel_key(KeyAction::Enter);
    assert!(detail.intent.is_none());
    assert_eq!(detail.note, Some(issue.to_string()));
    let modifiers = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    assert!(
        state
            .terminal_key(KeyEvent::new(KeyCode::Char('c'), modifiers))
            .is_none()
    );
    assert_eq!(state.take_copy_request(), Some(issue.to_string()));
    assert!(
        state
            .terminal_key(KeyEvent::new(KeyCode::Char('i'), modifiers))
            .is_none()
    );
    assert_eq!(state.note(), Some(issue.investigation_draft().as_str()));
    assert_eq!(state.input(), "keep the refused unsent draft");
    assert!(!state.has_pending_submission());
    state.close_panel();

    let mut next_location = snapshot();
    next_location.chrome.location = Some("/next-location".into());
    state.apply_catalog(next_location);
    state.panel = TuiPanel::Settings;
    assert!(
        state
            .modal_options()
            .iter()
            .all(|option| option.value != "submission-refusal")
    );
    state.close_panel();

    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh { ack, .. }) = inbox.recv().await else {
        panic!("explicit retry receipt")
    };
    ack.send(Err(CoreError::Application(
        "model_unavailable remains actual prose".into(),
    )))
    .unwrap();
    state.poll_submission();
    assert_eq!(
        state.note(),
        Some("submit: application: model_unavailable remains actual prose")
    );
    state.panel = TuiPanel::Settings;
    assert!(
        state
            .modal_options()
            .iter()
            .all(|option| option.value != "submission-refusal")
    );
    state.close_panel();
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh { ack, .. }) = inbox.recv().await else {
        panic!("new captured refusal receipt")
    };
    ack.send(Err(CoreError::Diagnostic(issue))).unwrap();
    state.poll_submission();
    state.panel = TuiPanel::Settings;
    assert!(
        state
            .modal_options()
            .iter()
            .any(|option| option.value == "submission-refusal")
    );
    state.close_panel();
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::SubmitFresh { ack, .. }) = inbox.recv().await else {
        panic!("accepted retry receipt")
    };
    ack.send(Ok(WorkerTurnId("accepted-retry".into()))).unwrap();
    state.poll_submission();
    assert!(state.attached_session().is_some());
    assert_eq!(state.input(), "");
    assert_eq!(state.window.rows().len(), 1);
    state.panel = TuiPanel::Settings;
    assert!(
        state
            .modal_options()
            .iter()
            .all(|option| option.value != "submission-refusal")
    );
    assert!(inbox.try_recv().is_err(), "no duplicate admission");
}

#[tokio::test]
async fn vis42_mcp_failure_is_status_not_dialogue_and_pending_is_not_an_alert() {
    let mut state = fresh_state("vis42-mcp").await;
    state.chrome.location = Some("/fixture".into());
    state.handle_paste("preserve my unsent draft");
    let issue = diagnostic(ServiceKind::Mcp);
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue.clone()), 1));
    assert!(
        state.window.rows().is_empty(),
        "background failure is not a conversation row"
    );
    assert_eq!(state.note(), Some("1 service issue · /settings /mcps"));
    assert_eq!(state.input(), "preserve my unsent draft");
    assert!(!state.has_pending_submission());
    // Safe MCP facts stay inspectable/copyable without touching the composer.
    state.panel = TuiPanel::Mcps;
    assert!(state.handle_panel_key(KeyAction::Enter).intent.is_none());
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let modifiers = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('c'), modifiers)),
        None
    );
    assert_eq!(state.take_copy_request(), Some(issue.to_string()));
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('i'), modifiers)),
        None
    );
    assert_eq!(state.note(), Some(issue.investigation_draft().as_str()));
    assert_eq!(state.input(), "preserve my unsent draft");
    assert!(!state.has_pending_submission());
    state.close_panel();
    state.push_note("independent operation feedback");
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue.clone()), 2));
    assert_eq!(state.note(), Some("independent operation feedback"));
    state.apply_mcp_snapshot(mcp(McpStatus::Pending, None, 3));
    assert_eq!(state.note(), Some("independent operation feedback"));
    assert_eq!(state.service_pending_count(), 1);
    assert_eq!(state.service_issue_count(), 0);
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue.clone()), 4));
    assert_eq!(
        state.note(),
        Some("independent operation feedback"),
        "pending alone is not recovery"
    );
    assert!(state.service_pending_issues.is_empty());
    state.apply_mcp_snapshot(mcp(McpStatus::Connected, None, 5));
    assert_eq!(state.note(), Some("independent operation feedback"));
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue), 6));
    assert_eq!(state.note(), Some("1 service issue · /settings /mcps"));
    let expiry = state.toast_expiry.as_ref().unwrap().started;
    state.apply_mcp_snapshot(mcp(McpStatus::Connected, None, 7));
    assert_eq!(
        state.note(),
        None,
        "recovery clears only its own service feedback"
    );
    assert!(expiry.is_some());
    assert!(state.window.rows().is_empty());
}

#[tokio::test]
async fn vis42_parked_projection_consumes_current_facts_without_replaying_alerts() {
    let mut state = fresh_state("vis42-parked").await;
    state.chrome.location = Some("/fixture".into());
    state.set_service_feedback_visible(false);
    let issue = diagnostic(ServiceKind::Mcp);
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue.clone()), 3));
    assert_eq!(state.service_issue_count(), 1);
    assert_eq!(state.note(), None);
    state.set_service_feedback_visible(true);
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue.clone()), 4));
    assert_eq!(state.note(), None);
    let mut changed = issue.clone();
    changed.stage = ServiceStage::Query;
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(changed), 2));
    assert_eq!(
        state.note(),
        None,
        "older publication cannot introduce a new cause"
    );
    let mut foreign = mcp(McpStatus::Connected, None, 5);
    foreign.binding.location = "/other".into();
    state.apply_mcp_snapshot(foreign);
    assert_eq!(state.service_issue_count(), 1);
    state.apply_mcp_snapshot(mcp(McpStatus::Connected, None, 5));
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue), 6));
    assert!(
        state.note().is_some(),
        "failure after actual recovery alerts once"
    );
    state.set_service_feedback_visible(false);
    assert_eq!(state.note(), None);
    assert!(state.window.rows().is_empty());
}

#[tokio::test]
async fn vis42_completion_provenance_keeps_identical_turn_warning_text() {
    let mut state = fresh_state("vis42-warnings").await;
    state.apply_turn_warnings(
        vec![
            "same sanitized text".into(),
            "same sanitized text".into(),
            "turn guidance".into(),
        ],
        1..2,
    );
    let rows = state.window.rows();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].text.contains("same sanitized text"));
    assert!(rows[1].text.contains("turn guidance"));
    state.apply_turn_warnings(vec!["visible with invalid provenance".into()], 0..2);
    assert_eq!(state.window.rows().len(), 3);

    // Completion routing remains pinned even with a concurrent child/old turn.
    let (app, _inbox, events) = CoreApp::channel(1);
    let mut driver = ScriptDriver::attach(&app);
    let mut state = TuiState::new(app, sid("vis42-owned-warning"));
    let owned = WorkerTurnId("owned-turn".into());
    state.active_turn = Some(owned.clone());
    state.status = TuiStatus::Streaming;
    for (turn, warning) in [
        (WorkerTurnId("foreign-turn".into()), "foreign warning"),
        (owned, "owned turn warning"),
    ] {
        events
            .send(CoreEvent::TurnFailed {
                session: sid("vis42-owned-warning"),
                turn,
                error: CoreError::Application("safe terminal reason".into()),
                warnings: vec![warning.into(), "background service warning".into()],
                service_warning_range: 1..2,
            })
            .unwrap();
    }
    assert_eq!(
        driver
            .pump_until_idle(&mut state, Duration::from_millis(100))
            .await,
        PumpOutcome::Closed
    );
    let rows = state.window.rows();
    assert!(
        rows.iter()
            .any(|row| row.text.contains("owned turn warning"))
    );
    assert!(rows.iter().all(|row| !row.text.contains("foreign warning")
        && !row.text.contains("background service warning")));
}

#[tokio::test]
async fn vis42_catalog_and_mcp_share_typed_change_detection_without_a_history_ledger() {
    let mut state = fresh_state("vis42-catalog").await;
    let issue = diagnostic(ServiceKind::Mcp);
    let mut catalog = snapshot();
    catalog.chrome.location = Some("/fixture".into());
    catalog.chrome.service_diagnostics.push(issue.clone());
    state.apply_catalog(catalog.clone());
    assert_eq!(state.note(), Some("1 service issue · /settings /mcps"));
    let expiry = state.toast_expiry.as_ref().unwrap().started;
    state.apply_mcp_snapshot(mcp(McpStatus::Failed, Some(issue), 1));
    assert_eq!(state.toast_expiry.as_ref().unwrap().started, expiry);
    state.push_transient_note("independent operation feedback", NoteVariant::Warning);
    let mut pending = mcp(McpStatus::Pending, None, 2);
    pending.servers[0].name = "masked-after-activation".into();
    state.apply_mcp_snapshot(pending);
    assert_eq!(state.note(), Some("independent operation feedback"));
    assert_eq!(state.service_pending_count(), 1);
    assert_eq!(state.service_issue_count(), 0);
    state.apply_catalog(catalog.clone());
    assert_eq!(state.note(), Some("independent operation feedback"));
    state.apply_mcp_snapshot(mcp(
        McpStatus::Failed,
        Some(catalog.chrome.service_diagnostics[0].clone()),
        3,
    ));
    assert_eq!(state.note(), Some("independent operation feedback"));
    assert!(state.service_pending_issues.is_empty());
    assert_eq!(state.service_issue_count(), 1);
    let expiry = state.toast_expiry.as_ref().unwrap().started;
    // Selection Location epoch and a rebuilt runtime's MCP generation are
    // different owners/clocks. Republished catalog epochs do not clear MCP.
    catalog.chrome.selection_generation = 73;
    state.apply_catalog(catalog.clone());
    assert_eq!(state.toast_expiry.as_ref().unwrap().started, expiry);
    let mut bootstrap = mcp(McpStatus::Pending, None, 0);
    bootstrap.binding.instance = 2;
    bootstrap.servers.clear();
    state.apply_mcp_snapshot(bootstrap);
    assert_eq!(
        state.service_issue_count(),
        1,
        "unpublished owner is not recovery"
    );
    state.push_note("independent operation feedback");
    state.apply_catalog(catalog.clone());
    assert_eq!(state.note(), Some("independent operation feedback"));
    catalog.chrome.service_diagnostics[0].stage = ServiceStage::Query;
    state.apply_catalog(catalog.clone());
    assert_eq!(
        state.note(),
        Some("independent operation feedback"),
        "current MCP facts supersede the overlapping catalog diagnostic"
    );
    catalog
        .chrome
        .service_diagnostics
        .push(diagnostic(ServiceKind::Definition));
    state.apply_catalog(catalog);
    assert_eq!(state.note(), Some("2 service issues · /settings /mcps"));
    assert!(state.window.rows().is_empty());
}
