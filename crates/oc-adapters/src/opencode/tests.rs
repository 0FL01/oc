use super::*;
use tokio::io::AsyncReadExt;

fn peer() -> (Bridge, UnixStream, BufReader<UnixStream>) {
    let (reader, input) = UnixStream::pair().unwrap();
    let (writer, output) = UnixStream::pair().unwrap();
    let init = Init {
        location: "/fixture".into(),
        session: None,
        catalog: oc_core::opencode::Catalog {
            models: Vec::new(),
            agents: Vec::new(),
            agent_models: Default::default(),
            selection: Selection {
                agent: None,
                model: None,
            },
        },
    };
    (
        Bridge {
            reader: BufReader::new(reader),
            writer,
            init,
        },
        input,
        BufReader::new(output),
    )
}

async fn request(output: &mut BufReader<UnixStream>) -> serde_json::Value {
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        output.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["type"], "request");
    value
}

#[tokio::test]
async fn process_local_deck_returns_a_real_cas_revision() {
    let (bridge, _input, mut output) = peer();
    let (app, guard) = bridge.start();
    let mut before = app.tab_deck().await.unwrap();
    assert!(before.revision.is_some());
    before.sessions.push(SessionId("ses_deck".into()));
    before.active = before.sessions.first().cloned();
    let saved = app.save_tab_deck(before.clone()).await.unwrap();
    assert_eq!(saved.sessions, before.sessions);
    assert_eq!(saved.active, before.active);
    assert_ne!(saved.revision, before.revision);
    assert_eq!(
        app.save_tab_deck(before).await,
        Err(CoreError::TabDeckConflict)
    );
    assert_eq!(app.tab_deck().await.unwrap(), saved);
    app.shutdown().await.unwrap();
    guard.join_diagnostic().await.unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        output.read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    let returned: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(returned["type"], "return");
    assert_eq!(returned["session"], "ses_deck");
}

#[tokio::test]
async fn unknown_effect_is_not_resent_or_implicitly_interrupted() {
    let (bridge, mut input, mut output) = peer();
    let (app, guard) = bridge.start();
    assert!(
        app.create_session(SessionId("native-forbidden".into()))
            .await
            .is_err()
    );
    let receipt = app
        .request_opencode(Request::Prompt {
            session: SessionId("ses_original".into()),
            text: "once".into(),
            delivery: queries::InboxDelivery::Steer,
        })
        .unwrap();
    let sent = request(&mut output).await;
    assert_eq!(sent["request"]["type"], "prompt");
    input.write_all(format!("{{\"type\":\"response\",\"id\":{},\"reply\":null,\"error\":null,\"unknown\":true}}\n", sent["id"]).as_bytes()).await.unwrap();
    assert_eq!(receipt.wait().await, Err(CoreError::Shutdown));
    guard.join_diagnostic().await.unwrap();
    let mut remaining = String::new();
    output.read_to_string(&mut remaining).await.unwrap();
    let frames: Vec<serde_json::Value> = remaining
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["type"], "return");
}

#[tokio::test]
async fn viewer_release_finishes_while_an_effect_is_pending_without_interrupting_it() {
    let (bridge, _input, mut output) = peer();
    let (app, guard) = bridge.start();
    let receipt = app
        .request_opencode(Request::Prompt {
            session: SessionId("ses_original".into()),
            text: "pending".into(),
            delivery: queries::InboxDelivery::Queue,
        })
        .unwrap();
    request(&mut output).await;
    app.shutdown().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), guard.join_diagnostic())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receipt.wait().await, Err(CoreError::Shutdown));
    let mut remaining = String::new();
    output.read_to_string(&mut remaining).await.unwrap();
    let frame: serde_json::Value = serde_json::from_str(remaining.trim()).unwrap();
    assert_eq!(frame["type"], "return");
}

#[tokio::test]
async fn fragmented_frame_survives_a_cancelled_read_future() {
    let (mut bridge, mut input, _output) = peer();
    input.write_all(b"{\"type\":\"eve").await.unwrap();
    let mut buffer = Vec::new();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            read_frame(&mut bridge.reader, &mut buffer)
        )
        .await
        .is_err()
    );
    input
        .write_all(b"nt\",\"event\":{\"type\":\"refresh\",\"session\":\"ses_original\"}}\n")
        .await
        .unwrap();
    assert!(
        matches!(read_frame(&mut bridge.reader, &mut buffer).await.unwrap(), HostFrame::Event { event: Event::Refresh { session } } if session.0 == "ses_original")
    );
}
