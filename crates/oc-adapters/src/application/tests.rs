//! Application review, file suggestion and reload regression packs.

use super::*;

#[cfg(test)]
mod review_tests {
    use super::*;
    use oc_core::queries::TerminalCopyMode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn conversation_shortcuts_follow_admitted_config_location_and_reload() {
        use oc_core::queries::ConversationShortcuts;
        let root = tempfile::tempdir().unwrap();
        let global = root.path().join("global");
        let a = root.path().join("a");
        let b = root.path().join("b");
        let data = root.path().join("data");
        for path in [&global, &a, &b, &data] {
            std::fs::create_dir_all(path).unwrap();
        }
        std::fs::write(
            global.join("opencode.json"),
            serde_json::json!({
                "model": "fixture/m", "provider": {"fixture": {
                    "options": {"baseURL": "https://example.invalid/v1", "apiKey": "dummy"},
                    "models": {"m": {}}
                }},
                "keybinds": {"leader": "ctrl+a", "session_undo": "<leader>z"}
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            global.join("opencode.jsonc"),
            r#"{"keybinds":{"leader":"ctrl+g","session_redo":"alt+r"}}"#,
        )
        .unwrap();
        std::fs::write(
            b.join("cli.jsonc"),
            r#"{"keybinds":{"leader":"ctrl+b","session.undo":"<leader>u,alt+z","session.redo":"none"}}"#,
        ).unwrap();
        let env = BTreeMap::from([(
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        )]);
        let (app, guard, _) = spawn_with_env(&a, &data, env).await.unwrap();
        let initial = app.catalog().await.unwrap();
        let global_shortcuts = ConversationShortcuts {
            leader: "ctrl+g".into(),
            undo: "ctrl+g z".into(),
            redo: "alt+r".into(),
        };
        assert_eq!(initial.chrome.conversation_shortcuts, global_shortcuts);
        let switched = app
            .switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            switched.catalog.chrome.conversation_shortcuts,
            ConversationShortcuts {
                leader: "ctrl+b".into(),
                undo: "ctrl+b u,alt+z".into(),
                redo: String::new(),
            }
        );
        assert_eq!(switched.catalog.model_id, initial.model_id);
        assert_eq!(switched.catalog.provider, initial.provider);
        assert_eq!(switched.catalog.variant, initial.variant);
        assert_eq!(switched.catalog.agent_id, initial.agent_id);
        assert_eq!(app.catalog().await.unwrap(), switched.catalog);
        // Invalid reload retains the complete previous immutable owner snapshot.
        std::fs::write(b.join("cli.jsonc"), r#"{"keybinds":{"session.redo":12}}"#).unwrap();
        assert!(app.reload_location().await.is_err());
        assert_eq!(app.catalog().await.unwrap(), switched.catalog);
        std::fs::write(
            b.join("cli.jsonc"),
            r#"{"keybinds":{"leader":"none","session.redo":"<leader>r,alt+y"}}"#,
        )
        .unwrap();
        let reloaded = app.reload_location().await.unwrap();
        assert!(reloaded.generation > switched.generation);
        assert_eq!(
            reloaded.catalog.chrome.conversation_shortcuts,
            ConversationShortcuts {
                leader: String::new(),
                undo: String::new(),
                redo: "alt+y".into(),
            }
        );
        assert_eq!(reloaded.catalog.model_id, initial.model_id);
        assert_eq!(reloaded.catalog.variant, initial.variant);
        assert_eq!(reloaded.catalog.agent_id, initial.agent_id);
        // Returning to A drops B's overrides; editing B did not mutate its old DTO.
        let returned = app
            .switch_location_home(a.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            returned.catalog.chrome.conversation_shortcuts,
            global_shortcuts
        );
        assert_eq!(
            switched.catalog.chrome.conversation_shortcuts.leader,
            "ctrl+b"
        );
        std::fs::write(global.join("opencode.jsonc"), "{}").unwrap();
        let mut config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(global.join("opencode.json")).unwrap())
                .unwrap();
        config.as_object_mut().unwrap().remove("keybinds");
        std::fs::write(global.join("opencode.json"), config.to_string()).unwrap();
        let defaults = app.reload_location().await.unwrap();
        assert_eq!(
            defaults.catalog.chrome.conversation_shortcuts,
            ConversationShortcuts::default()
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn terminal_copy_and_animations_follow_global_location_switch_and_reload() {
        let root = tempfile::tempdir().unwrap();
        let global = root.path().join("global");
        let a = root.path().join("a");
        let b = root.path().join("b");
        let data = root.path().join("data");
        for path in [&global, &a, &b, &data] {
            std::fs::create_dir_all(path).unwrap();
        }
        let model = serde_json::json!({
            "model": "fixture/m", "provider": {"fixture": {
                "options": {"baseURL": "https://example.invalid/v1", "apiKey": "dummy"},
                "models": {"m": {}}
            }}
        });
        std::fs::write(global.join("opencode.json"), model.to_string()).unwrap();
        std::fs::write(
            global.join("cli.json"),
            r#"{"keybinds":{"leader":"ctrl+g","command.palette.show":"<leader>p"}}"#,
        )
        .unwrap();
        std::fs::write(
            global.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"manual"},"animations":false,"leader_timeout":500}"#,
        )
        .unwrap();
        std::fs::write(
            b.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"select"},"animations":true,"leader":{"timeout":321}}"#,
        )
        .unwrap();
        let env = BTreeMap::from([(
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        )]);
        let (app, guard, _) = spawn_with_env(&a, &data, env).await.unwrap();
        let initial = app.catalog().await.unwrap();
        assert_eq!(initial.chrome.terminal_copy, Some(TerminalCopyMode::Manual));
        assert_eq!(initial.chrome.animations, Some(false));
        assert_eq!(initial.chrome.leader_timeout_ms(), 500);
        assert_eq!(
            initial.chrome.command_palette_shortcut.as_deref(),
            Some("ctrl+g p")
        );

        std::fs::write(b.join("opencode.jsonc"), r#"{"animations":"invalid"}"#).unwrap();
        let failure = app
            .switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap_err();
        assert!(matches!(
            failure,
            CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                ..
            }
        ));
        assert_eq!(app.catalog().await.unwrap(), initial);
        std::fs::write(
            b.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"select"},"animations":true,"leader":{"timeout":321}}"#,
        )
        .unwrap();
        let switched = app
            .switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            switched.catalog.chrome.terminal_copy,
            Some(TerminalCopyMode::Select)
        );
        assert_eq!(
            app.catalog().await.unwrap().chrome.terminal_copy,
            switched.catalog.chrome.terminal_copy
        );
        assert_eq!(switched.catalog.chrome.animations, Some(true));
        assert_eq!(switched.catalog.chrome.leader_timeout_ms(), 321);
        assert_eq!(app.catalog().await.unwrap().chrome.animations, Some(true));

        std::fs::write(b.join("opencode.jsonc"), r#"{"animations":null}"#).unwrap();
        let failure = app.reload_location().await.unwrap_err();
        assert!(matches!(
            failure,
            CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                ..
            }
        ));
        assert_eq!(app.catalog().await.unwrap(), switched.catalog);

        std::fs::write(
            b.join("opencode.jsonc"),
            r#"{"terminal":{"copy":"sensitive-fixture"},"animations":true}"#,
        )
        .unwrap();
        let failure = app.reload_location().await.unwrap_err();
        assert!(!failure.to_string().contains("sensitive-fixture"));
        assert_eq!(
            app.catalog().await.unwrap().chrome.terminal_copy,
            Some(TerminalCopyMode::Select)
        );
        assert_eq!(app.catalog().await.unwrap().chrome.animations, Some(true));

        std::fs::write(b.join("opencode.jsonc"), "{}").unwrap();
        let reloaded = app.reload_location().await.unwrap();
        assert!(reloaded.generation > switched.generation);
        assert_eq!(
            reloaded.catalog.chrome.terminal_copy,
            Some(TerminalCopyMode::Manual)
        );
        assert_eq!(reloaded.catalog.chrome.animations, Some(false));
        assert_eq!(reloaded.catalog.chrome.leader_timeout_ms(), 500);
        assert_eq!(
            app.catalog().await.unwrap().chrome.terminal_copy,
            reloaded.catalog.chrome.terminal_copy
        );
        let returned = app
            .switch_location_home(a.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            returned.catalog.chrome.terminal_copy,
            Some(TerminalCopyMode::Manual)
        );
        assert_eq!(returned.catalog.chrome.animations, Some(false));

        std::fs::write(global.join("opencode.jsonc"), "{}").unwrap();
        let unconfigured = app.reload_location().await.unwrap();
        assert_eq!(unconfigured.catalog.chrome.terminal_copy, None);
        assert_eq!(unconfigured.catalog.chrome.animations, None);
        assert!(unconfigured.catalog.chrome.animations_enabled());
        assert_eq!(unconfigured.catalog.chrome.leader_timeout_ms(), 2000);
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn catalog_descriptions_track_current_location_and_reload() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        let data = root.path().join("data");
        for path in [&a, &b, &data] {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut config = serde_json::json!({
            "model": "fixture/m", "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": "http://127.0.0.1:9/v1", "apiKey": "dummy"},
                "models": {"m": {}}
            }}
        });
        std::fs::write(a.join("opencode.json"), config.to_string()).unwrap();
        config["command"] = serde_json::json!({"review": {
            "template": "local review $ARGUMENTS", "description": "from B"
        }});
        std::fs::write(b.join("opencode.json"), config.to_string()).unwrap();
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&a, &data, env).await.unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "review changes [commit|branch|pr], defaults to uncommitted"
        );
        app.switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "from B"
        );
        config["command"]["review"]["description"] = "reloaded".into();
        std::fs::write(b.join("opencode.json"), config.to_string()).unwrap();
        app.reload_location().await.unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "reloaded"
        );
        app.switch_location_home(a.to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(
            app.catalog().await.unwrap().command_descriptions["review"],
            "review changes [commit|branch|pr], defaults to uncommitted"
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[test]
    fn review_replaces_only_source_placeholders_and_enforces_expansion_cap() {
        let input = "'branch name'  $ARGUMENTS  ${path}  $1";
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/upstream/v2/review.txt"
        ));
        assert_eq!(
            crate::runtime::expand_command(source, &[input.into()]).unwrap(),
            source.replace("$ARGUMENTS", input)
        );
        assert!(
            crate::runtime::expand_command(
                source,
                &["x".repeat(crate::runtime::COMMAND_BYTES_CAP)]
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn workspace_review_override_keeps_positional_expansion() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("opencode.json"),
            serde_json::json!({
                "model": "fixture/m", "provider": {"fixture": {
                    "options": {"baseURL": "https://example.invalid/v1", "apiKey": "dummy"},
                    "models": {"m": {}}
                }},
                "command": {"review": {"template": "local $1; all: $ARGUMENTS"}}
            })
            .to_string(),
        )
        .unwrap();
        let env = BTreeMap::from([(
            "XDG_CONFIG_HOME".into(),
            root.path().join("config").to_string_lossy().into_owned(),
        )]);
        let composition = composition::load_with_env(&project, env).await.unwrap();
        assert!(!composition.builtin_review);
        assert_eq!(
            resolve_submission(&composition, "/review   one  two ".into()).unwrap(),
            (
                "local one; all: one two".into(),
                Some("/review   one  two ".into())
            )
        );
        assert_eq!(
            resolve_submission(&composition, "/something else".into()).unwrap(),
            ("/something else".into(), None)
        );
    }

    #[tokio::test]
    async fn fallback_review_submits_exact_prompt_and_replays_original_invocation() {
        let root = tempfile::tempdir().expect("fixture");
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&data).expect("data");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fake provider");
        let config = serde_json::json!({
            "model": "fixture/m", "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": format!("http://{}/v1", listener.local_addr().unwrap()),
                            "apiKey": "dummy"},
                "models": {"m": {}}
            }}
        });
        std::fs::write(project.join("opencode.json"), config.to_string()).expect("config");
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env.clone())
            .await
            .expect("spawn");
        assert_eq!(app.catalog().await.expect("catalog").commands, ["review"]);
        assert_eq!(
            app.catalog().await.expect("catalog").command_descriptions["review"],
            "review changes [commit|branch|pr], defaults to uncommitted"
        );
        let composed = composition::load_with_env(&project, env.clone())
            .await
            .expect("admitted generation");
        let (default_prompt, default_invocation) =
            resolve_submission(&composed, "/review".into()).expect("default review");
        assert_eq!(default_invocation.as_deref(), Some("/review"));
        assert_eq!(
            default_prompt,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/upstream/v2/review.txt"
            ))
            .replace("$ARGUMENTS", "")
        );
        let session = SessionId("review-fallback".into());
        app.create_session(session.clone()).await.expect("session");
        let original = "/review   'branch name'  $ARGUMENTS  ${path}   ";
        let expected_input = "'branch name'  $ARGUMENTS  ${path}";
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("request");
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            let (headers_end, content_length) = loop {
                let n = stream.read(&mut chunk).await.expect("read");
                assert!(n > 0, "complete request");
                request.extend_from_slice(&chunk[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let len = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .expect("body size");
                    if request.len() >= end + 4 + len {
                        break (end, len);
                    }
                }
            };
            let body: serde_json::Value =
                serde_json::from_slice(&request[headers_end + 4..headers_end + 4 + content_length])
                    .expect("Responses JSON");
            let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"review complete\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).as_bytes()).await.expect("reply");
            body
        });
        let mut events = app.subscribe();
        app.submit(session.clone(), original.into())
            .await
            .expect("submit review");
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
                .await
                .expect("turn timeout")
                .expect("event")
            {
                CoreEvent::TurnFinished { session: id, .. } if id == session => break,
                CoreEvent::TurnFailed { error, .. } => panic!("review failed: {error}"),
                _ => {}
            }
        }
        let request = server.await.expect("fake response");
        let expected = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/upstream/v2/review.txt"
        ))
        .replace("$ARGUMENTS", expected_input);
        let actual = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item["role"] == "user")
            .filter_map(|item| item["content"].as_array())
            .flat_map(|parts| parts.iter())
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            [expected.as_str()],
            "provider receives exact expansion"
        );
        assert_eq!(
            app.read_history(session.clone()).await.unwrap()[0].text,
            original
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();

        let (reopened, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        assert_eq!(
            reopened.read_history(session).await.unwrap()[0].text,
            original
        );
        reopened.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
}

#[cfg(test)]
mod file_suggestion_tests {
    use super::*;

    #[tokio::test]
    async fn burst_coalesces_to_one_pending_walk_without_overlapping_active_walk() {
        use std::sync::atomic::AtomicUsize;
        use tokio::time::{Duration, timeout};

        let queue = Arc::new(Mutex::new(SuggestionQueue::default()));
        let epoch = Arc::new(AtomicU64::new(1));
        let active = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(AtomicUsize::new(0));
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let (first_ack, first_rx) = oneshot::channel();
        let first_active = Arc::clone(&active);
        let first_entered = Arc::clone(&entered);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    assert_eq!(first_active.fetch_add(1, Ordering::SeqCst), 0);
                    first_entered.fetch_add(1, Ordering::SeqCst);
                    let _ = started.send(());
                    release_rx.recv().unwrap();
                    first_active.fetch_sub(1, Ordering::SeqCst);
                    Ok(crate::files::FileSuggestionResult {
                        paths: vec!["first".into()],
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack: first_ack,
            },
        );
        timeout(Duration::from_secs(5), started_rx)
            .await
            .unwrap()
            .unwrap();

        let mut last_rx = None;
        for index in 0..100 {
            let (ack, rx) = oneshot::channel();
            let next_active = Arc::clone(&active);
            let next_entered = Arc::clone(&entered);
            enqueue_file_suggestion(
                &queue,
                &epoch,
                SuggestionRequest {
                    work: Box::new(move || {
                        assert_eq!(next_active.fetch_add(1, Ordering::SeqCst), 0);
                        next_entered.fetch_add(1, Ordering::SeqCst);
                        next_active.fetch_sub(1, Ordering::SeqCst);
                        Ok(crate::files::FileSuggestionResult {
                            paths: vec![index.to_string()],
                            truncated: false,
                        })
                    }),
                    location: "/a".into(),
                    generation: 1,
                    ack,
                },
            );
            if let Some(previous) = last_rx.replace(rx) {
                assert_eq!(
                    previous.await.unwrap(),
                    Err(app_error("file suggestions superseded"))
                );
            }
        }
        assert_eq!(entered.load(Ordering::SeqCst), 1);
        assert!(queue.lock().unwrap().pending.is_some());
        // Cancellation of the latest pending request must also skip the pool.
        drop(last_rx);
        let (ack, rx) = oneshot::channel();
        let last_entered = Arc::clone(&entered);
        let last_active = Arc::clone(&active);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    assert_eq!(last_active.fetch_add(1, Ordering::SeqCst), 0);
                    last_entered.fetch_add(1, Ordering::SeqCst);
                    last_active.fetch_sub(1, Ordering::SeqCst);
                    Ok(crate::files::FileSuggestionResult {
                        paths: vec!["latest".into()],
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack,
            },
        );
        release.send(()).unwrap();
        assert_eq!(
            timeout(Duration::from_secs(5), first_rx)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .paths,
            ["first"]
        );
        assert_eq!(
            timeout(Duration::from_secs(5), rx)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .paths,
            ["latest"]
        );
        assert_eq!(entered.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn canceled_pending_request_never_enters_blocking_pool() {
        use std::sync::atomic::AtomicUsize;
        use tokio::time::{Duration, timeout};

        let queue = Arc::new(Mutex::new(SuggestionQueue::default()));
        let epoch = Arc::new(AtomicU64::new(1));
        let entered = Arc::new(AtomicUsize::new(0));
        let (started, started_rx) = oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let (ack, rx) = oneshot::channel();
        let first_entered = Arc::clone(&entered);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    first_entered.fetch_add(1, Ordering::SeqCst);
                    let _ = started.send(());
                    release_rx.recv().unwrap();
                    Ok(crate::files::FileSuggestionResult {
                        paths: Vec::new(),
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack,
            },
        );
        timeout(Duration::from_secs(5), started_rx)
            .await
            .unwrap()
            .unwrap();

        let (ack, canceled) = oneshot::channel();
        let pending_entered = Arc::clone(&entered);
        enqueue_file_suggestion(
            &queue,
            &epoch,
            SuggestionRequest {
                work: Box::new(move || {
                    pending_entered.fetch_add(1, Ordering::SeqCst);
                    Ok(crate::files::FileSuggestionResult {
                        paths: Vec::new(),
                        truncated: false,
                    })
                }),
                location: "/a".into(),
                generation: 1,
                ack,
            },
        );
        drop(canceled);
        release.send(()).unwrap();
        timeout(Duration::from_secs(5), rx)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(5), async {
            while queue.lock().unwrap().running {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(entered.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn suggestions_follow_owner_across_home_switch_and_return() {
        let root = tempfile::tempdir().unwrap();
        let a = root.path().join("a");
        let b = root.path().join("b");
        let data = root.path().join("data");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let config = r#"{"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"dummy"},"models":{"m":{}}}}}"#;
        for project in [&a, &b] {
            std::fs::write(project.join("opencode.json"), config).unwrap();
        }
        std::fs::write(a.join("only-a.rs"), "a").unwrap();
        std::fs::write(b.join("only-b.rs"), "b").unwrap();
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&a, &data, env).await.unwrap();
        let first = app.file_suggestions("only".into(), 20).await.unwrap();
        assert_eq!(first.location, a.to_string_lossy());
        assert_eq!(first.paths, ["only-a.rs"]);
        assert!(!first.truncated);

        let switched = app
            .switch_location_home(b.to_string_lossy().into_owned())
            .await
            .unwrap();
        let second = app.file_suggestions("only".into(), 20).await.unwrap();
        assert_eq!(second.location, b.to_string_lossy());
        assert_eq!(second.paths, ["only-b.rs"]);
        assert!(second.generation > first.generation);
        assert_eq!(second.generation, switched.generation);

        let returned = app
            .switch_location_home(a.to_string_lossy().into_owned())
            .await
            .unwrap();
        let third = app.file_suggestions("only".into(), 20).await.unwrap();
        assert_eq!(third.location, first.location);
        assert_eq!(third.paths, first.paths);
        assert!(third.generation > second.generation);
        assert_eq!(third.generation, returned.generation);
        let reloaded = app.reload_location().await.unwrap();
        assert_eq!(reloaded.location, third.location);
        assert!(reloaded.generation > third.generation);
        assert_eq!(
            app.file_suggestions("only".into(), 20)
                .await
                .unwrap()
                .generation,
            reloaded.generation
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[test]
    fn stale_walk_is_refused_even_when_location_returns_to_same_path() {
        let epoch = AtomicU64::new(1);
        // A detached blocking walk finishes after the owner publishes A→B→A.
        epoch.fetch_add(2, Ordering::SeqCst);
        let result = file_suggestion_reply(
            &epoch,
            1,
            "/project/a".into(),
            Ok(crate::files::FileSuggestionResult {
                paths: vec!["old.rs".into()],
                truncated: false,
            }),
        );
        assert_eq!(
            result,
            Err(app_error("file suggestions belong to previous Location"))
        );
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;
    use oc_core::queries::SessionSelectionAction as Action;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn config(model: &str, base_url: &str, static_model: bool) -> String {
        let provider = if static_model { "fixture" } else { "ludka2" };
        let models = if static_model {
            serde_json::json!({(model): {}})
        } else {
            serde_json::json!({})
        };
        serde_json::json!({
            "model": format!("{provider}/{model}"),
            "provider": {(provider): {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": base_url, "apiKey": "dummy"},
                "models": models
            }}
        })
        .to_string()
    }

    fn env(data: &Path) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ])
    }

    #[test]
    fn model_switch_projection_uses_only_matching_current_catalog_name() {
        let raw = r#"{"previous":{"provider":"fixture","id":"old","variant":null},"current":{"provider":"fixture","id":"new","variant":null},"display_name":"Stale"}"#;
        let notice: ModelSwitchNotice = serde_json::from_str(raw).unwrap();
        let mut catalog = crate::models::ModelCatalog {
            provider: "fixture".into(),
            models: [("new".into(), serde_json::json!({"name": "Public\nName"}))].into(),
        };
        let projected = project_model_switch(notice.clone(), &catalog);
        assert_eq!(projected.display_name.as_deref(), Some("PublicName"));
        assert!(
            !serde_json::to_string(&projected)
                .unwrap()
                .contains("display_name")
        );
        catalog
            .models
            .insert("new".into(), serde_json::json!({"name": 17}));
        assert!(
            project_model_switch(notice.clone(), &catalog)
                .display_name
                .is_none()
        );
        catalog.models.remove("new");
        assert!(
            project_model_switch(notice.clone(), &catalog)
                .display_name
                .is_none()
        );
        catalog.provider = "foreign".into();
        catalog
            .models
            .insert("new".into(), serde_json::json!({"name": "Wrong Provider"}));
        assert!(
            project_model_switch(notice, &catalog)
                .display_name
                .is_none()
        );
    }

    #[tokio::test]
    async fn accepted_switch_projects_typed_paged_history_after_restart() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let mut config: serde_json::Value =
            serde_json::from_str(&config("old", &base, true)).unwrap();
        config["provider"]["fixture"]["models"]["new"] =
            serde_json::json!({"name": "Readable New"});
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        let server = tokio::spawn(async move {
            // First turn, its real title generation, and the second turn.
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buf = [0; 4096];
                    let size = stream.read(&mut buf).await.unwrap();
                    assert!(size > 0);
                    request.extend_from_slice(&buf[..size]);
                    if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"reply\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).as_bytes()).await.unwrap();
            }
        });
        let session = SessionId("switch-replay".into());
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        app.create_session(session.clone()).await.unwrap();
        let mut events = app.subscribe();
        let mut live_switch = None;
        for prompt in ["first", "second"] {
            if prompt == "second" {
                app.session_selection(session.clone(), false, Action::Model("new".into()))
                    .await
                    .unwrap();
                assert!(
                    app.history_page(session.clone(), None, None, 10)
                        .await
                        .unwrap()
                        .rows
                        .iter()
                        .all(|r| r.model_switch.is_none())
                );
            }
            app.submit(session.clone(), prompt.into()).await.unwrap();
            loop {
                match tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
                    .await
                    .unwrap()
                    .unwrap()
                {
                    CoreEvent::TurnStarted { model_switch, .. } => {
                        if prompt == "second" {
                            live_switch = model_switch;
                        }
                    }
                    CoreEvent::TurnFinished { .. } => break,
                    CoreEvent::TurnFailed { error, .. } => panic!("turn failed: {error}"),
                    _ => {}
                }
            }
        }
        server.await.unwrap();
        let newer = app
            .history_page(session.clone(), None, None, 1)
            .await
            .unwrap();
        let before = newer.rows[0].seq;
        let user = app
            .history_page(session.clone(), Some(before), None, 1)
            .await
            .unwrap();
        let marker = app
            .history_page(session.clone(), Some(user.rows[0].seq), None, 1)
            .await
            .unwrap();
        let switch = marker.rows[0].model_switch.as_ref().unwrap();
        assert_eq!(switch.display_name.as_deref(), Some("Readable New"));
        assert_eq!(live_switch.as_ref(), Some(switch));
        assert_eq!(
            (&switch.previous.id, &switch.current.id),
            (&"old".to_string(), &"new".to_string())
        );
        assert_eq!(marker.rows[0].text, "");
        assert_eq!(app.read_history(session.clone()).await.unwrap().len(), 4);
        let all = app
            .history_page(session.clone(), None, None, 10)
            .await
            .unwrap();
        let durable = app.read_history(session.clone()).await.unwrap();
        assert_eq!(
            all.rows
                .iter()
                .filter(|r| r.model_switch.is_none())
                .map(|r| &r.id)
                .collect::<Vec<_>>(),
            durable.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
        let forward = app
            .history_page(session.clone(), None, Some(marker.rows[0].seq), 10)
            .await
            .unwrap();
        assert_eq!(
            forward.rows,
            vec![user.rows[0].clone(), newer.rows[0].clone()]
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        config["provider"]["fixture"]["models"]["new"]["name"] = serde_json::json!("Renamed Later");
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        let (reopened, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let replay = reopened
            .history_page(session, None, None, 10)
            .await
            .unwrap();
        assert_eq!(
            replay
                .rows
                .iter()
                .map(|r| (&r.id, r.seq))
                .collect::<Vec<_>>(),
            all.rows.iter().map(|r| (&r.id, r.seq)).collect::<Vec<_>>()
        );
        assert_eq!(
            replay
                .rows
                .iter()
                .filter(|r| r.model_switch.is_some())
                .count(),
            1
        );
        assert_eq!(
            replay.rows.iter().find_map(|r| r.model_switch.as_ref()),
            Some(&ModelSwitchNotice {
                display_name: Some("Renamed Later".into()),
                ..live_switch.unwrap()
            })
        );
        reopened.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn removed_selected_primary_refuses_reload_and_keeps_old_turn_usable() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let config = |include_review: bool| {
            let mut value: serde_json::Value =
                serde_json::from_str(&config("old", &base_url, true)).unwrap();
            value["agent"] = serde_json::json!({
                "build": {"mode": "primary", "prompt": "BUILD_PRIMARY"},
                "review": {"mode": "primary", "prompt": "REVIEW_PRIMARY"}
            });
            value["default_agent"] = "build".into();
            if !include_review {
                value["agent"].as_object_mut().unwrap().remove("review");
            }
            value.to_string()
        };
        let path = project.join("opencode.json");
        std::fs::write(&path, config(true)).unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("retained-review".into());
        let active = SessionId("active-build".into());
        app.create_session(session.clone()).await.unwrap();
        app.create_session(active.clone()).await.unwrap();
        let saved = app
            .save_tab_deck(oc_core::queries::TabDeckSnapshot {
                sessions: vec![session.clone(), active.clone()],
                active: Some(active),
                ..app.tab_deck().await.unwrap()
            })
            .await
            .unwrap();
        let chosen = app
            .session_selection(session.clone(), false, Action::Agent("review".into()))
            .await
            .unwrap();
        assert_eq!(chosen.agent_id.as_deref(), Some("review"));
        let initial = app.file_suggestions("".into(), 1).await.unwrap();
        let catalog = app.catalog().await.unwrap();

        std::fs::write(&path, config(false)).unwrap();
        assert_eq!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                detail: "retained session selection unavailable in reloaded configuration".into(),
            })
        );
        assert_eq!(app.catalog().await.unwrap(), catalog);
        assert_eq!(app.tab_deck().await.unwrap(), saved);
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            initial.generation
        );
        assert_eq!(
            app.session_selection(session.clone(), false, Action::Current)
                .await
                .unwrap(),
            chosen
        );

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let n = stream.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&chunk[..n]);
                if let Some(headers_end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap();
                    if request.len() >= headers_end + 4 + content_length {
                        break;
                    }
                }
            }
            let body = String::from_utf8_lossy(&request);
            assert!(body.contains("REVIEW_PRIMARY"), "old agent prompt missing");
            assert!(body.contains("retained turn"), "old session prompt missing");
            let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
            stream
                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).as_bytes())
                .await
                .unwrap();
        });
        let mut events = app.subscribe();
        app.submit(session.clone(), "retained turn".into())
            .await
            .unwrap();
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
                .await
                .unwrap()
                .unwrap()
            {
                CoreEvent::TurnFinished { session: id, .. } if id == session => break,
                CoreEvent::TurnFailed { error, .. } => panic!("retained turn failed: {error}"),
                _ => {}
            }
        }
        server.await.unwrap();
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn retired_explicit_tab_model_is_not_silently_replaced_by_config_fallback() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let path = project.join("opencode.json");
        let config = |retired: bool| {
            let mut value: serde_json::Value =
                serde_json::from_str(&config("new", "http://127.0.0.1:9/v1", true)).unwrap();
            if !retired {
                value["provider"]["fixture"]["models"]["old"] = serde_json::json!({});
            }
            value.to_string()
        };
        std::fs::write(&path, config(false)).unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("explicit-old-model".into());
        app.create_session(session.clone()).await.unwrap();
        app.save_tab_deck(oc_core::queries::TabDeckSnapshot {
            sessions: vec![session.clone()],
            active: Some(session.clone()),
            ..app.tab_deck().await.unwrap()
        })
        .await
        .unwrap();
        app.session_selection(session.clone(), false, Action::Model("old".into()))
            .await
            .unwrap();
        let initial = app.file_suggestions("".into(), 1).await.unwrap();
        std::fs::write(&path, config(true)).unwrap();
        assert!(matches!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch {
                category: LocationSwitchFailure::Configuration,
                ..
            })
        ));
        assert_eq!(
            app.session_selection(session, false, Action::Current)
                .await
                .unwrap()
                .model_id,
            "old"
        );
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            initial.generation
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn current_reload_discovers_new_catalog_preserves_deck_and_rolls_back_failures() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let path = project.join("opencode.json");
        std::fs::write(&path, config("old", "http://127.0.0.1:9/v1", true)).unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("reload-root".into());
        app.create_session(session.clone()).await.unwrap();
        let deck = app.tab_deck().await.unwrap();
        let saved = app
            .save_tab_deck(oc_core::queries::TabDeckSnapshot {
                sessions: vec![session.clone()],
                active: Some(session.clone()),
                ..deck
            })
            .await
            .unwrap();
        let initial = app.file_suggestions("".into(), 1).await.unwrap();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for (model, count) in [("new", 1), ("new", 0)] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let n = stream.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                }
                assert!(request.starts_with(b"GET /v1/models HTTP/1.1"));
                let entries = if count == 1 {
                    vec![serde_json::json!({"id": model, "context_length": 1000})]
                } else {
                    vec![]
                };
                let body = serde_json::json!({"object":"list", "data": entries}).to_string();
                stream
                    .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes())
                    .await
                    .unwrap();
            }
        });
        std::fs::write(&path, config("new", &base_url, false)).unwrap();
        let reloaded = app.reload_location().await.unwrap();
        assert_eq!(reloaded.location, initial.location);
        assert!(reloaded.generation > initial.generation);
        assert_eq!(reloaded.catalog.model_id, "new");
        assert_eq!(reloaded.catalog.models.len(), 1);
        assert_eq!(reloaded.catalog.models[0].id, "new");
        assert_eq!(app.tab_deck().await.unwrap(), saved);
        assert_eq!(
            app.session_selection(session.clone(), false, Action::Current)
                .await
                .unwrap()
                .model_id,
            "new"
        );
        assert_eq!(
            app.probe_session(session.clone()).await.unwrap(),
            SessionProbe::Root
        );

        std::fs::write(&path, "{broken").unwrap();
        assert!(matches!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch { .. })
        ));
        assert_eq!(app.catalog().await.unwrap(), reloaded.catalog);
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            reloaded.generation
        );

        std::fs::write(&path, config("new", &base_url, false)).unwrap();
        assert!(matches!(
            app.reload_location().await,
            Err(CoreError::LocationSwitch { .. })
        ));
        server.await.unwrap();
        assert_eq!(app.catalog().await.unwrap(), reloaded.catalog);
        assert_eq!(app.tab_deck().await.unwrap(), saved);
        assert_eq!(
            app.probe_session(session).await.unwrap(),
            SessionProbe::Root
        );
        assert_eq!(
            app.file_suggestions("".into(), 1).await.unwrap().generation,
            reloaded.generation
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }

    #[tokio::test]
    async fn reload_is_refused_by_owner_during_active_turn() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        std::fs::write(
            project.join("opencode.json"),
            config("old", &base_url, true),
        )
        .unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env(&data)).await.unwrap();
        let session = SessionId("active-reload".into());
        app.submit_fresh(session.clone(), "hello".into(), None)
            .await
            .unwrap();
        assert_eq!(app.reload_location().await, Err(CoreError::TurnBusy));
        app.cancel(session).await.unwrap();
        // Drop the stalled transport so the cancelled turn can terminate.
        drop(listener);
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
}
