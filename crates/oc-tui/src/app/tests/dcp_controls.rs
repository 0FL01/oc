use super::*;
use crate::commands::CommandAction;

#[tokio::test]
async fn dcp12_commands_unregistered_completion_palette_dispatch_and_restored() {
    let mut state = fresh_state("controls").await;
    let mut catalog = snapshot();
    catalog.chrome.dcp.commands_enabled = false;
    state.apply_catalog(catalog);
    for action in [
        CommandAction::OpenDcp,
        CommandAction::DcpCompress {
            focus: String::new(),
        },
    ] {
        assert_eq!(
            state.command_unavailable(&action),
            Some("DCP commands disabled")
        );
        let outcome = state.run_command(action);
        assert!(outcome.intent.is_none());
        assert_eq!(outcome.note.as_deref(), Some("DCP commands disabled"));
    }
    state.run_command(CommandAction::OpenCommands);
    assert!(
        state
            .modal_options()
            .iter()
            .all(|option| !option.title.contains("DCP"))
    );
    state.close_panel();
    type_text(&mut state, "/d").await;
    assert!(
        state
            .slash_options()
            .unwrap()
            .iter()
            .all(|option| !option.name.starts_with("dcp"))
    );
    state.input.clear();
    state.editor.cursor = 0;
    state.apply_catalog(snapshot());
    type_text(&mut state, "/d").await;
    assert!(
        state
            .slash_options()
            .unwrap()
            .iter()
            .any(|option| option.name == "dcp")
    );
    assert!(
        state
            .slash_options()
            .unwrap()
            .iter()
            .any(|option| option.name == "dcp-compress")
    );
    let mut snapshot = oc_core::queries::DcpSnapshot::default();
    snapshot.availability.manual_refusal = Some(oc_core::dcp_view::DcpUnavailable::CompressOff);
    state.apply_dcp_snapshot(snapshot);
    let outcome = state.run_command(CommandAction::DcpCompress {
        focus: String::new(),
    });
    assert!(outcome.intent.is_none());
    assert_eq!(outcome.note.as_deref(), Some("compression disabled"));
    assert!(state.request_compress(String::new()).is_err());
    state.panel = super::super::TuiPanel::Dcp;
    let panel = state.handle_panel_key(KeyAction::Enter);
    assert!(panel.intent.is_none());
    assert_eq!(panel.note.as_deref(), Some("compression disabled"));
    assert_eq!(state.status(), &TuiStatus::Idle);
    let mut changed_profile = super::snapshot();
    changed_profile.agent_id = Some("replacement-profile".into());
    state.apply_catalog(changed_profile);
    assert_eq!(
        state.command_unavailable(&CommandAction::DcpCompress {
            focus: String::new()
        }),
        None,
        "a previous profile's query must not veto owner admission"
    );
}
