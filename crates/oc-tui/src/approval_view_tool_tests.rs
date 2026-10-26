use super::*;
use oc_core::{approval::ApprovalBinding, core_app::CoreApp, domain::SessionId};

fn request() -> ApprovalRequest {
    ApprovalRequest {
        id: 7,
        binding: ApprovalBinding {
            session: "root".into(),
            turn: "turn".into(),
            call: "call-mcp".into(),
            operation: "operation".into(),
            input_digest: "digest".into(),
            location: "location".into(),
            generation: 3,
            agent: None,
            agent_digest: None,
        },
        project: "project".into(),
        action: "vis36__echo".into(),
        resources: vec!["*".into()],
        save_patterns: vec!["*".into()],
        preview: ApprovalPreview::Tool,
    }
}

#[tokio::test]
async fn generic_tool_body_uses_actual_action_in_inline_and_fullscreen() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, SessionId("root".into()));
    let request = request();
    assert_eq!(
        presentation_title(&request),
        Some(("⚙", "Call tool vis36__echo".into()))
    );
    state.approvals.reconcile(vec![(request, true)]);
    for width in [79, 121] {
        for fullscreen in [false, true] {
            state.approvals.fullscreen = fullscreen;
            let rows = crate::views::render_test(&state, width, 40);
            assert!(rows.iter().any(|row| row.contains("Tool: vis36__echo")));
            assert!(
                !rows
                    .iter()
                    .any(|row| row.contains("Resource: *") || row.contains("Tool: *"))
            );
            assert_eq!(
                body_lines(&state, width, width),
                vec![Line::styled(
                    "Tool: vis36__echo",
                    Style::default().fg(Theme::dark().text_muted())
                )]
            );
        }
    }
}

#[tokio::test]
async fn external_directory_shows_directory_and_patterns_in_inline_and_fullscreen() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, SessionId("root".into()));
    for (scope, directory) in [("/outside/*", "/outside"), ("/*", "/")] {
        let mut request = request();
        request.action = "external_directory".into();
        request.resources = vec![scope.into()];
        request.save_patterns.clear();
        request.preview = ApprovalPreview::Resource {
            values: request.resources.clone(),
        };
        let title = format!("Access external directory {directory}");
        assert_eq!(presentation_title(&request), Some(("←", title.clone())));
        state.approvals.reconcile(vec![(request, true)]);
        for fullscreen in [false, true] {
            state.approvals.fullscreen = fullscreen;
            let rows = crate::views::render_test(&state, 79, 40);
            assert!(rows.iter().any(|row| row.contains(&title)));
            assert!(rows.iter().any(|row| row.contains("Patterns")));
            assert!(rows.iter().any(|row| row.contains(&format!("- {scope}"))));
            assert!(
                !rows
                    .iter()
                    .any(|row| row.contains("Call tool") || row.contains("Tool:"))
            );
            assert_eq!(state.approvals.active().unwrap().resources, [scope]);
        }
    }
}

#[tokio::test]
async fn generic_tool_always_keeps_raw_scope_and_exact_reply_binding() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, SessionId("root".into()));
    let request = request();
    state.approvals.reconcile(vec![(request.clone(), true)]);
    state.approvals.selected = state
        .approvals
        .options()
        .iter()
        .position(|(_, decision)| *decision == ApprovalDecision::Always)
        .unwrap();
    assert_eq!(
        body_lines(&state, 121, 121),
        vec![Line::styled(
            "This will always allow vis36__echo for this project.",
            Style::default().fg(Theme::dark().text_muted())
        )]
    );
    let Some(PanelIntent::ReplyApproval(reply)) = state.approvals.key(KeyAction::Enter).intent
    else {
        panic!("exact reply intent");
    };
    assert_eq!(
        reply,
        ApprovalReply {
            id: request.id,
            binding: request.binding.clone(),
            decision: ApprovalDecision::Always
        }
    );
    assert_eq!(state.approvals.active(), Some(&request));
    assert_eq!(state.approvals.active().unwrap().resources, ["*"]);
    assert_eq!(state.approvals.active().unwrap().save_patterns, ["*"]);
}
