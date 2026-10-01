//! Actual binary intent dispatcher over the real application owner and Responses wire.
use super::*;
use oc_core::approval::{ApprovalDecision, ApprovalReply, ApprovalRequest};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

struct Fixture {
    root: tempfile::TempDir,
    base: String,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(replies: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let server = std::thread::spawn(move || {
            let mut replies = replies.into_iter();
            while !done.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(_) => break,
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(at) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..at]);
                        let len = header
                            .lines()
                            .find_map(|line| {
                                line.split_once(':')
                                    .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= at + 4 + len {
                            break;
                        }
                    }
                }
                let Some(body) = replies.next() else { break };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self {
            root: tempfile::tempdir().unwrap(),
            base,
            stop,
            server: Some(server),
        }
    }
    async fn app(&self) -> (CoreApp, oc_core::core_app::WorkerGuard) {
        self.app_with_config_dir(true).await
    }
    async fn app_with_config_dir(
        &self,
        create_global: bool,
    ) -> (CoreApp, oc_core::core_app::WorkerGuard) {
        let project = self.root.path().join("project");
        let home = self.root.path().join("home");
        if create_global {
            std::fs::create_dir_all(home.join(".config/opencode")).unwrap();
        }
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("opencode.json"), serde_json::json!({"model":"fixture/gpt-fixture", "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":self.base,"apiKey":"fixture"},"models":{"gpt-fixture":{"limit":{"context":65536,"output":4096}}}}},"permission":{"bash":"ask","apply_patch":"ask","subagent":"allow"},"agent":{"title":{"disable":true},"helper":{"mode":"subagent","prompt":"Use shell.","permission":{"bash":"ask"}}}}).to_string()).unwrap();
        let (app, guard, _) = oc_adapters::application::spawn_with_env(
            &project,
            &self.root.path().join("data"),
            BTreeMap::from([
                ("HOME".into(), home.display().to_string()),
                ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
            ]),
        )
        .await
        .unwrap();
        (app, guard)
    }
}

#[tokio::test]
async fn owner_settings_preserves_jsonc_and_missing_global_uses_admitted_location() {
    let empty = Fixture::new(vec![]);
    let (app, guard) = empty.app_with_config_dir(false).await;
    app.set_permission_mode(true).await.unwrap();
    let created: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(empty.root.path().join("project/cli.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        created.pointer("/session/permissions").unwrap(),
        "autoaccept"
    );
    assert!(!empty.root.path().join("home/.config").exists());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let fixture = Fixture::new(vec![tool("cancel-after-save-error")]);
    let project = fixture.root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cli = project.join("cli.jsonc");
    let original = "// retained comment 🦊\n{\"session\": {\"permissions\": \"prompt\", /* note */ \"tps\": false,}, \"unknown\": [1,2,], \"keybinds\": {\"leader\":\"alt+x\", \"permission.prompt.fullscreen\":\"<leader>f\", \"app.exit\":\"<leader>q,ctrl+c\"},}\n";
    std::fs::write(&cli, original).unwrap();
    let (app, guard) = fixture.app_with_config_dir(false).await;
    app.set_permission_mode(true).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(&cli).unwrap(),
        original.replacen("\"prompt\"", "\"autoaccept\"", 1)
    );
    assert!(!fixture.root.path().join("home/.config").exists());
    assert_eq!(
        app.catalog().await.unwrap().auto_accept,
        oc_core::queries::AutoAcceptState::Enabled
    );
    app.set_permission_mode(false).await.unwrap();
    assert_eq!(std::fs::read_to_string(&cli).unwrap(), original);
    let session = SessionId("pending-save".into());
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Pending fixture".into())
        .await
        .unwrap();
    let (mut state, _) = restore_initial(&app, Some(session.clone())).await.unwrap();
    let mut events = app.subscribe();
    app.submit(session.clone(), "wait".into()).await.unwrap();
    let request = asked(&mut events).await;
    refresh_approvals(&app, &mut state).await.unwrap();
    assert_eq!(state.chrome.permission_shortcuts.fullscreen, "alt+x f");
    assert!(
        state
            .terminal_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('x'),
                crossterm::event::KeyModifiers::ALT
            ))
            .is_none()
    );
    assert!(
        state
            .terminal_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('f'),
                crossterm::event::KeyModifiers::NONE
            ))
            .is_none()
    );
    assert!(state.approvals.fullscreen);
    let escape = state
        .terminal_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ))
        .unwrap();
    state.handle_key(escape).await;
    assert!(!state.approvals.fullscreen);
    std::fs::write(&cli, "{").unwrap();
    assert!(app.set_permission_mode(true).await.is_err());
    assert!(app.reload_location().await.is_err());
    assert_eq!(app.pending_approvals().await.unwrap(), vec![request]);
    assert_eq!(
        app.catalog().await.unwrap().auto_accept,
        oc_core::queries::AutoAcceptState::Disabled
    );
    app.cancel(session).await.unwrap();
    assert!(app.pending_approvals().await.unwrap().is_empty());
    assert!(!project.join("cancel-after-save-error").exists());
    std::fs::write(&cli, original).unwrap();
    // Replacement of the admitted parent cannot redirect an owner Settings write.
    let moved = fixture.root.path().join("moved-project");
    let outside = fixture.root.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("cli.jsonc"), original).unwrap();
    std::fs::rename(&project, &moved).unwrap();
    std::os::unix::fs::symlink(&outside, &project).unwrap();
    assert!(app.set_permission_mode(true).await.is_err());
    assert_eq!(
        std::fs::read_to_string(outside.join("cli.jsonc")).unwrap(),
        original
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn prepared_patch_geometry_cache_busy_add_rejection_and_palette_are_real() {
    use ratatui::{Terminal, backend::TestBackend};
    let patch = format!(
        "*** Begin Patch\n*** Add File: approval.txt\n{}*** End Patch",
        (0..30)
            .map(|n| format!("+prepared line {n}\n"))
            .collect::<String>()
    );
    let fixture = Fixture::new(vec![tool_call(
        "patch",
        "apply_patch",
        serde_json::json!({"patchText":patch}).to_string(),
    )]);
    let project = fixture.root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("cli.json"),
        r#"{"debug":{"devtools":false},"session":{"sidebar":"hide"}}"#,
    )
    .unwrap();
    let (app, guard) = fixture.app().await;
    let session = SessionId("prepared-patch".into());
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Prepared patch fixture".into())
        .await
        .unwrap();
    app.register_approval_consumer(false).await.unwrap();
    let (mut state, mut deck) = restore_initial(&app, Some(session.clone())).await.unwrap();
    let mut events = app.subscribe();
    state.handle_paste("prepare patch");
    state.handle_key(KeyAction::Enter).await;
    let request = asked(&mut events).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.has_pending_submission() {
            state.poll_submission();
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(state.is_busy());
    state.handle_paste("parked draft");
    refresh_approvals(&app, &mut state).await.unwrap();
    deck.sync_tabs(&mut state);
    state.chrome.animations = Some(false);
    state.apply_tool_argument_stream(
        &oc_core::core_app::WorkerTurnId(request.binding.turn.clone()),
        &oc_core::tool_stream::ToolStreamEvent::Pending {
            identity: oc_core::tool_stream::ToolStreamIdentity {
                round: 1,
                item_id: format!("item-{}", request.binding.call),
                call_id: request.binding.call.clone(),
            },
            name: "apply_patch".into(),
            preview: String::new(),
            truncated: false,
        },
    );
    assert_eq!(
        state
            .transcript_rows()
            .iter()
            .filter(|row| row.tool.is_some())
            .count(),
        1,
        "late broadcast announcement must not duplicate recovered owner-prepared card"
    );
    assert!(
        matches!(&request.preview, oc_core::approval::ApprovalPreview::Patch { files, .. } if !files[0].hunks.is_empty())
    );
    assert!(
        app.tool_ops_page(session.clone(), None, 32)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    assert!(!fixture.root.path().join("project/approval.txt").exists());
    for width in [79, 80, 120, 121] {
        let (indexed_cold, _) = state.visible_transcript(width - 4, width, 200);
        let full = state.rendered_transcript(width - 4, width);
        let (indexed_warm, _) = state.visible_transcript(width - 4, width, 200);
        let paint = |lines: &[oc_tui::styled::Line]| {
            let mut terminal =
                Terminal::new(TestBackend::new(width - 4, lines.len() as u16)).unwrap();
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        ratatui::widgets::Paragraph::new(
                            lines
                                .iter()
                                .cloned()
                                .map(oc_tui::styled::Line::into_ratatui)
                                .collect::<Vec<_>>(),
                        ),
                        frame.area(),
                    )
                })
                .unwrap();
            terminal.backend().buffer().clone()
        };
        assert_eq!(paint(&indexed_cold), paint(&full));
        assert_eq!(indexed_warm, indexed_cold);
        let shown = full
            .iter()
            .map(oc_tui::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            shown.contains("⋯ Patching approval.txt"),
            "action={} rows={:?} shown={shown}",
            request.action,
            state.transcript_rows()
        );
        assert!(!shown.contains("resource:") && !shown.contains("Created"));
        let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
        terminal
            .draw(|frame| oc_tui::views::render_frame(frame, &state))
            .unwrap();
        let cold = terminal.backend().buffer().clone();
        terminal
            .draw(|frame| oc_tui::views::render_frame(frame, &state))
            .unwrap();
        assert_eq!(&cold, terminal.backend().buffer());
        let theme = oc_tui::theme::Theme::dark();
        assert_eq!(
            cold[(5, 25)].symbol(),
            "△",
            "width={width}\n{}",
            oc_tui::views::render_test(&state, width, 40).join("\n")
        );
        assert_eq!(cold[(7, 25)].symbol(), "P");
        assert_eq!(cold[(7, 26)].symbol(), "→");
        assert_eq!(cold[(7, 26)].fg, theme.text_muted());
        assert_eq!(cold[(2, 24)].symbol(), "┃");
        assert_eq!(cold[(6, if width < 80 { 35 } else { 37 })].symbol(), "A");
        if width == 120 {
            assert_eq!(
                cold[(73, 37)].symbol(),
                "c",
                "fullscreen hint uses display cells"
            );
            assert_eq!(cold[(102, 37)].symbol(), "e");
        }
        assert_eq!(cold[(1, 0)].symbol(), "!");
        assert_eq!(cold[(1, 0)].fg, theme.hue("accent", 200).unwrap());
        assert_eq!(
            cold[(1, 0)].bg,
            ratatui::style::Color::Rgb(0x2c, 0x29, 0x33)
        );
        assert!(
            cold[(1, 0)]
                .modifier
                .contains(ratatui::style::Modifier::BOLD)
        );
        let rows = oc_tui::views::render_test(&state, width, 40);
        assert!(
            rows[0].contains('+'),
            "owner-backed new Home stays available while approval waits"
        );
        let scroll = state.approvals.key(KeyAction::Down);
        assert!(scroll.intent.is_none());
        let moved = oc_tui::views::render_test(&state, width, 40);
        assert_ne!(rows[28..34], moved[28..34]);
        assert_eq!(state.input(), "parked draft");
        state.approvals.key(KeyAction::Up);
    }
    state.approvals.fullscreen = true;
    let expanded = oc_tui::views::render_test(&state, 120, 40);
    assert!(expanded[3].contains("→ Edit approval.txt"));
    let mut fullscreen = Terminal::new(TestBackend::new(120, 40)).unwrap();
    fullscreen
        .draw(|frame| oc_tui::views::render_frame(frame, &state))
        .unwrap();
    assert_eq!(fullscreen.backend().buffer()[(75, 37)].symbol(), "c");
    state.approvals.key(KeyAction::Cancel); // minimize, retain queue
    state.approvals.key(KeyAction::Right);
    let always = oc_tui::views::render_test(&state, 120, 40).join("\n");
    assert!(always.contains("→ Edit approval.txt") && always.contains("- approval.txt"));
    assert!(!always.contains("- *"));
    assert_eq!(
        app.pending_approvals().await.unwrap(),
        vec![request.clone()]
    );
    apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
        .await
        .unwrap();
    assert!(state.home);
    assert!(
        state.tab_attention.contains(&0),
        "routing to Home must retain real background permission attention"
    );
    assert_eq!(
        app.pending_approvals().await.unwrap(),
        vec![request.clone()]
    );
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.input(), "parked draft");
    let rejection = state.approvals.key(KeyAction::Cancel).intent.unwrap();
    apply_intent(&app, &mut state, &mut deck, rejection)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::TurnInterrupted { .. } | CoreEvent::TurnFinished { .. } => break,
                CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    let page = app.tool_ops_page(session.clone(), None, 32).await.unwrap();
    let row = &page.rows[0];
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(row.output.as_deref().unwrap()).unwrap()["status"],
        "permission_rejected"
    );
    let card = oc_tui::history::card_from_row(row);
    let rejection_rows = oc_tui::tools::tool_block(&card, oc_tui::theme::Theme::dark(), 116);
    assert_eq!(card.files, ["approval.txt"]);
    assert!(
        rejection_rows[1]
            .plain_text()
            .contains("# Patch failed approval.txt")
    );
    assert_eq!(rejection_rows[2].plain_text().trim(), "┃");
    assert!(
        rejection_rows[3]
            .plain_text()
            .contains("The user declined this tool call")
    );
    let rejection_text = rejection_rows
        .iter()
        .map(oc_tui::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rejection_text.contains("The user declined this tool call")
            && rejection_text.contains("approval.txt")
    );
    assert!(!rejection_text.contains("permission_rejected") && !rejection_text.contains("Created"));
    assert!(!fixture.root.path().join("project/approval.txt").exists());
    let (mut state, mut deck) = restore_initial(&app, Some(session)).await.unwrap();
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("settings");
    state.handle_panel_key(KeyAction::Enter);
    assert_eq!(state.panel(), &TuiPanel::Settings);
    assert_eq!(state.modal_options()[0].footer, "prompt");
    state.handle_paste("permission");
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.panel(), &TuiPanel::Settings);
    assert_eq!(state.modal_options().len(), 2);
    assert_eq!(
        state.modal_options()[1].footer,
        app.catalog()
            .await
            .unwrap()
            .chrome
            .provider
            .unwrap()
            .to_string()
    );
    state.handle_paste("permission");
    for width in [79, 80, 120, 121] {
        let rows = oc_tui::views::render_test(&state, width, 40);
        assert!(rows[15].trim() == "Session");
        assert!(rows[16].contains("Permissions") && rows[16].contains("prompt"));
        assert!(!rows[16].contains("Session"));
        assert!(rows[13].contains("permission"));
    }
    let change = state.handle_panel_key(KeyAction::Right).intent.unwrap();
    apply_intent(&app, &mut state, &mut deck, change)
        .await
        .unwrap();
    assert_eq!(state.modal_options()[0].footer, "auto accept");
    assert!(app.catalog().await.unwrap().chrome.permissions_auto);
    assert!(oc_tui::views::render_test(&state, 80, 40)[13].contains("Search"));
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.panel(), &TuiPanel::None);
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("settings");
    state.handle_panel_key(KeyAction::Enter);
    state.handle_paste("permission");
    let source = fixture.root.path().join("home/.config/opencode/cli.json");
    let saved = std::fs::read_to_string(&source).unwrap();
    std::fs::write(&source, "{").unwrap();
    let change = state.handle_panel_key(KeyAction::Right).intent.unwrap();
    assert!(
        apply_intent(&app, &mut state, &mut deck, change)
            .await
            .is_err()
    );
    assert!(oc_tui::views::render_test(&state, 80, 40)[13].contains("permission"));
    assert_eq!(state.modal_options()[0].footer, "auto accept");
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.panel(), &TuiPanel::Settings);
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(state.panel(), &TuiPanel::None);
    std::fs::write(source, saved).unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(server) = self.server.take() {
            server.join().unwrap();
        }
    }
}

#[tokio::test]
async fn owner_prepared_url_wraps_header_and_body_at_actual_available_width() {
    use ratatui::{Terminal, backend::TestBackend};
    let url = format!(
        "https://example.invalid/vis36/never-fetch?wrapped={}",
        "word-".repeat(24)
    );
    let fixture = Fixture::new(vec![tool_call(
        "url",
        "webfetch",
        serde_json::json!({"url":url}).to_string(),
    )]);
    let project = fixture.root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("opencode.jsonc"),
        r#"{"permission":{"webfetch":"ask"}}"#,
    )
    .unwrap();
    std::fs::write(
        project.join("cli.json"),
        r#"{"debug":{"devtools":false},"session":{"sidebar":"hide"}}"#,
    )
    .unwrap();
    let (app, guard) = fixture.app().await;
    let session = SessionId("prepared-url".into());
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Prepared URL fixture".into())
        .await
        .unwrap();
    app.register_approval_consumer(false).await.unwrap();
    let (mut state, mut deck) = restore_initial(&app, Some(session.clone())).await.unwrap();
    let mut events = app.subscribe();
    state.handle_paste("prepare url");
    state.handle_key(KeyAction::Enter).await;
    let request = asked(&mut events).await;
    assert!(
        matches!(&request.preview, oc_core::approval::ApprovalPreview::Resource { values } if values.as_slice() == std::slice::from_ref(&url))
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while state.has_pending_submission() {
            state.poll_submission();
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    state.handle_paste("untouched composer");
    refresh_approvals(&app, &mut state).await.unwrap();
    deck.sync_tabs(&mut state);
    for width in [79, 80, 120, 121] {
        let mut terminal = Terminal::new(TestBackend::new(width, 40)).unwrap();
        terminal
            .draw(|frame| oc_tui::views::render_frame(frame, &state))
            .unwrap();
        let cold = terminal.backend().buffer().clone();
        terminal
            .draw(|frame| oc_tui::views::render_frame(frame, &state))
            .unwrap();
        assert_eq!(&cold, terminal.backend().buffer());
        let rows = oc_tui::views::render_test(&state, width, 40);
        let header = rows
            .iter()
            .position(|row| row.contains("Permission required"))
            .unwrap();
        let title_rows = if width < 120 { 3 } else { 2 };
        let title = (header + 1..header + 1 + title_rows)
            .map(|y| {
                (9..width - 5)
                    .map(|x| cold[(x, y as u16)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<String>();
        assert!(
            title.replace(' ', "").contains(&url),
            "width={width}\n{}",
            rows.join("\n")
        );
        assert!(rows[header + title_rows + 2].contains("URL: https://example.invalid"));
        if width == 80 {
            assert_eq!(header, 27);
            assert_eq!(cold[(7, 28)].symbol(), "%");
        }
        let (cold_indexed, _) = state.visible_transcript(width - 4, width, 200);
        let full = state.rendered_transcript(width - 4, width);
        let (warm_indexed, _) = state.visible_transcript(width - 4, width, 200);
        let paint = |lines: &[oc_tui::styled::Line]| {
            let mut terminal =
                Terminal::new(TestBackend::new(width - 4, lines.len() as u16)).unwrap();
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        ratatui::widgets::Paragraph::new(
                            lines
                                .iter()
                                .cloned()
                                .map(oc_tui::styled::Line::into_ratatui)
                                .collect::<Vec<_>>(),
                        ),
                        frame.area(),
                    )
                })
                .unwrap();
            terminal.backend().buffer().clone()
        };
        assert_eq!(paint(&cold_indexed), paint(&full));
        assert_eq!(paint(&warm_indexed), paint(&full));
        assert_eq!(state.input(), "untouched composer");
        assert_eq!(
            app.pending_approvals().await.unwrap(),
            vec![request.clone()]
        );
        assert!(
            app.tool_ops_page(session.clone(), None, 32)
                .await
                .unwrap()
                .rows
                .is_empty()
        );
    }
    app.cancel(session).await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
fn event(value: serde_json::Value) -> String {
    format!("data: {value}\n\n")
}
fn tool(marker: &str) -> String {
    let input = serde_json::json!({"argv":["/usr/bin/touch",marker]}).to_string();
    tool_call(marker, "bash", input)
}
fn tool_call(marker: &str, name: &str, input: String) -> String {
    event(
        serde_json::json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":format!("item-{marker}"),"call_id":marker,"name":name,"arguments":""}}),
    ) + &event(
        serde_json::json!({"type":"response.function_call_arguments.delta","item_id":format!("item-{marker}"),"output_index":0,"delta":input}),
    ) + &event(
        serde_json::json!({"type":"response.function_call_arguments.done","item_id":format!("item-{marker}"),"output_index":0,"arguments":input}),
    ) + &event(
        serde_json::json!({"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":format!("item-{marker}"),"call_id":marker,"name":name,"arguments":input,"status":"completed"}}),
    ) + &event(
        serde_json::json!({"type":"response.completed","response":{"id":"r","status":"completed","usage":{"input_tokens":1,"output_tokens":1}}}),
    )
}

#[tokio::test]
async fn real_child_pending_routes_to_root_tab_with_feedback_and_actual_binding() {
    let fixture = Fixture::new(vec![tool_call("delegate", "subagent", serde_json::json!({"agent":"helper","description":"Child fixture","prompt":"Run shell"}).to_string()), tool("child-marker"), finished(), finished()]);
    let (app, guard) = fixture.app().await;
    let root = SessionId("root".into());
    app.create_session(root.clone()).await.unwrap();
    app.rename_session(root.clone(), "Family fixture".into())
        .await
        .unwrap();
    app.register_approval_consumer(false).await.unwrap();
    let (mut state, mut deck) = restore_initial(&app, Some(root.clone())).await.unwrap();
    let mut events = app.subscribe();
    app.submit(root.clone(), "delegate".into()).await.unwrap();
    let request = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::PermissionAsked(request) => break request,
                CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_ne!(request.binding.session, root.0);
    refresh_approvals(&app, &mut state).await.unwrap();
    deck.sync_tabs(&mut state);
    assert_eq!(state.approvals.active(), Some(&request));
    assert!(state.tab_attention.contains(&0));
    let child = SessionId(request.binding.session.clone());
    let mut child_view = TuiState::new(app.clone(), child.clone());
    let page = app
        .history_page(child.clone(), None, None, 1)
        .await
        .unwrap();
    child_view.attach_page(&page);
    refresh_approvals(&app, &mut child_view).await.unwrap();
    assert!(child_view.approvals.active().is_none());
    let mut other = TuiState::new(app.clone(), SessionId("other".into()));
    app.create_session(SessionId("other".into())).await.unwrap();
    refresh_approvals(&app, &mut other).await.unwrap();
    assert!(other.approvals.active().is_none());
    state.handle_key(KeyAction::Left).await;
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    state.handle_paste("Please use a narrower command");
    let reply = state.handle_key(KeyAction::Enter).await.intent.unwrap();
    let PanelIntent::ReplyApproval(binding) = &reply else {
        panic!("reply")
    };
    assert_eq!(binding.binding.session, child.0);
    assert_eq!(
        binding.decision,
        ApprovalDecision::Reject {
            feedback: Some("Please use a narrower command".into())
        }
    );
    apply_intent(&app, &mut state, &mut deck, reply)
        .await
        .unwrap();
    complete(&mut events).await;
    assert!(!fixture.root.path().join("project/child-marker").exists());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
fn finished() -> String {
    event(serde_json::json!({"type":"response.output_text.delta","delta":"done"}))
        + &event(
            serde_json::json!({"type":"response.completed","response":{"id":"r","status":"completed","usage":{"input_tokens":1,"output_tokens":1}}}),
        )
}
async fn asked(events: &mut tokio::sync::broadcast::Receiver<CoreEvent>) -> ApprovalRequest {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::PermissionAsked(r) => break r,
                CoreEvent::ToolCallStarted { .. } => panic!("intent before approval"),
                CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap()
}
async fn complete(events: &mut tokio::sync::broadcast::Receiver<CoreEvent>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::TurnFinished { .. } => break,
                CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn real_binary_permission_intent_error_ack_cancel_and_pending_auto() {
    let fixture = Fixture::new(vec![
        tool("once-marker"),
        finished(),
        tool("cancel-marker"),
        tool("pending-auto"),
        finished(),
        tool("new-auto"),
        finished(),
    ]);
    let (app, guard) = fixture.app().await;
    let session = SessionId("root".into());
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Permission fixture".into())
        .await
        .unwrap();
    app.register_approval_consumer(false).await.unwrap();
    let mut events = app.subscribe();
    let (mut state, mut deck) = restore_initial(&app, Some(session.clone())).await.unwrap();
    state.handle_paste("composer 🦊 is retained");
    app.submit(session.clone(), "tool".into()).await.unwrap();
    let request = asked(&mut events).await;
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &session,
        CoreEvent::PermissionAsked(request.clone()),
    )
    .await
    .unwrap();
    assert_eq!(state.approvals.active(), Some(&request));
    assert!(!fixture.root.path().join("project/once-marker").exists());
    let db = rusqlite::Connection::open(fixture.root.path().join("data/oc.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM tool_operations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let mut binding = request.binding.clone();
    binding.generation += 1;
    assert!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::ReplyApproval(ApprovalReply {
                id: request.id,
                binding,
                decision: ApprovalDecision::Once
            })
        )
        .await
        .is_err()
    );
    assert!(state.approvals.error.is_some());
    assert_eq!(state.approvals.active(), Some(&request));
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )),
    )
    .await
    .unwrap();
    complete(&mut events).await;
    assert!(fixture.root.path().join("project/once-marker").exists());
    assert!(state.approvals.active().is_none());
    assert_eq!(state.input(), "composer 🦊 is retained");
    app.submit(session.clone(), "cancel".into()).await.unwrap();
    let _ = asked(&mut events).await;
    app.cancel(session.clone()).await.unwrap();
    refresh_approvals(&app, &mut state).await.unwrap();
    assert!(state.approvals.active().is_none());
    assert!(!fixture.root.path().join("project/cancel-marker").exists());
    // Drain cancellation before the next turn's permission event.
    app.submit(session.clone(), "auto pending".into())
        .await
        .unwrap();
    let _ = asked(&mut events).await;
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SetPermissionMode { auto_once: true },
    )
    .await
    .unwrap();
    complete(&mut events).await;
    assert!(app.pending_approvals().await.unwrap().is_empty());
    assert!(fixture.root.path().join("project/pending-auto").exists());
    app.submit(session, "new auto".into()).await.unwrap();
    complete(&mut events).await;
    assert!(fixture.root.path().join("project/new-auto").exists());
    assert!(
        std::fs::read_to_string(fixture.root.path().join("home/.config/opencode/cli.json"))
            .unwrap()
            .contains("autoaccept")
    );
    deck.cli_auto = true;
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SetPermissionMode { auto_once: false },
    )
    .await
    .unwrap();
    assert_eq!(
        app.catalog().await.unwrap().auto_accept,
        oc_core::queries::AutoAcceptState::Enabled
    );
    let cli = fixture.root.path().join("home/.config/opencode/cli.json");
    let saved = std::fs::read_to_string(&cli).unwrap();
    assert!(saved.contains("prompt"));
    std::fs::write(&cli, "{").unwrap();
    assert!(app.reload_location().await.is_err());
    assert_eq!(
        app.catalog().await.unwrap().auto_accept,
        oc_core::queries::AutoAcceptState::Enabled
    );
    assert!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::SetPermissionMode { auto_once: false }
        )
        .await
        .is_err()
    );
    assert_eq!(state.input(), "composer 🦊 is retained");
    std::fs::write(&cli, &saved).unwrap();
    let snapshot = app.reload_location().await.unwrap();
    assert!(!snapshot.catalog.chrome.permissions_auto);
    finish_reload(&app, &mut state, &mut deck, snapshot, None)
        .await
        .unwrap();
    deck.sync_tabs(&mut state);
    assert_eq!(
        state.auto_accept,
        oc_core::queries::AutoAcceptState::Enabled
    );
    assert_eq!(state.input(), "composer 🦊 is retained");
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard) = fixture.app().await;
    assert!(!app.catalog().await.unwrap().chrome.permissions_auto);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
