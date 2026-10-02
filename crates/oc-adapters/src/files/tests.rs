use super::*;
use std::fs;
use std::os::unix::fs::symlink;

fn fixture() -> (tempfile::TempDir, Files) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    fs::create_dir_all(root.join("private")).unwrap();
    let files = Files::new(&root, &root.join("private")).unwrap();
    (tmp, files)
}

fn grep(files: &Files, path: &str, include: Option<&str>) -> Result<Vec<GrepHit>, FileToolError> {
    files.grep_search(
        &GrepOptions {
            pattern: "NEEDLE",
            path,
            include,
            literal: false,
            case_sensitive: true,
            offset: 0,
            limit: 100,
        },
        |_| true,
        None,
    )
}

#[test]
fn tool14_ignore_precedence_hidden_and_positive_override() {
    let (_tmp, files) = fixture();
    let root = &files.root;
    for name in [
        "plain.rs",
        "git.rs",
        "ignore.rs",
        "rg.rs",
        ".dot.rs",
        ".hidden/a.rs",
        ".git/a.rs",
        "private/a.rs",
        "nested/negated.rs",
        "nested/git.rs",
        "nested/rg.rs",
    ] {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "NEEDLE\n").unwrap();
    }
    fs::write(root.join(".gitignore"), "git.rs\nnegated.rs\n").unwrap();
    fs::write(root.join(".ignore"), "ignore.rs\n!git.rs\n").unwrap();
    fs::write(root.join(".rgignore"), "rg.rs\n").unwrap();
    fs::write(root.join("nested/.gitignore"), "!negated.rs\n!rg.rs\n").unwrap();
    let hits = grep(&files, ".", None).unwrap();
    assert_eq!(
        hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        [
            ".dot.rs",
            ".hidden/a.rs",
            "git.rs",
            "nested/git.rs",
            "nested/negated.rs",
            "plain.rs"
        ]
    );
    // Scoped scans retain admitted ancestor .rgignore/.ignore precedence.
    assert_eq!(
        grep(&files, "nested", None)
            .unwrap()
            .iter()
            .map(|h| h.path.as_str())
            .collect::<Vec<_>>(),
        ["nested/git.rs", "nested/negated.rs"]
    );
    let include = grep(&files, ".", Some("*.rs")).unwrap();
    assert_eq!(include.len(), 9);
    assert!(include.iter().any(|h| h.path == "rg.rs"));
    for hidden in [false, true] {
        let paths = files
            .glob_search(
                &GlobOptions {
                    pattern: "**/*.rs",
                    path: ".",
                    hidden,
                    offset: 0,
                    limit: 100,
                },
                |_| true,
                None,
            )
            .unwrap();
        assert!(
            !paths
                .iter()
                .any(|p| p.starts_with("private/") || p.starts_with(".git/"))
        );
        assert_eq!(paths.iter().any(|p| p == ".dot.rs"), hidden);
        assert!(paths.iter().any(|p| p == "rg.rs"));
    }
}

#[test]
fn tool14_gitignore_requires_repository_and_explicit_file_bypasses_ignores() {
    let (_tmp, files) = fixture();
    fs::write(files.root.join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(files.root.join("ignored.rs"), "NEEDLE\n").unwrap();
    assert_eq!(grep(&files, ".", None).unwrap().len(), 1);
    fs::create_dir(files.root.join(".git")).unwrap();
    assert!(grep(&files, ".", None).unwrap().is_empty());
    assert_eq!(grep(&files, "ignored.rs", None).unwrap().len(), 1);
}

#[test]
fn tool14_invalid_patterns_before_scan_and_resource_outcomes() {
    let (tmp, files) = fixture();
    let missing = Files::new(&tmp.path().join("missing"), &tmp.path().join("data")).unwrap();
    for pattern in ["[", "(?=secret)", "(a)\\1", "secret\0"] {
        let error = missing.grep(pattern, false, 0, 1).unwrap_err();
        assert!(matches!(error, FileToolError::InvalidPattern(_)));
        assert!(!error.to_string().contains("secret"));
    }
    assert!(matches!(
        missing.glob("[", 0, 1),
        Err(FileToolError::InvalidPattern(_))
    ));
    fs::write(
        files.root.join("large"),
        vec![b'x'; GREP_FILE_BYTES_CAP as usize + 1],
    )
    .unwrap();
    assert_eq!(grep(&files, ".", None), Err(FileToolError::BudgetExhausted));
    let mut budget = Budget::new(None);
    budget.start = std::time::Instant::now() - std::time::Duration::from_secs(31);
    assert_eq!(budget.check(), Err(FileToolError::BudgetExhausted));
}

#[test]
fn tool14_search_concurrent_directory_swap_never_reads_escape() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let (tmp, files) = fixture();
    let slot = files.root.join("slot");
    let parked = files.root.join("parked");
    let outside = tmp.path().join("outside");
    fs::create_dir(&slot).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(slot.join("safe.rs"), "NEEDLE\n").unwrap();
    fs::write(outside.join("escape.rs"), "NEEDLE ESCAPE\n").unwrap();
    fs::write(files.data_root.join("secret.rs"), "NEEDLE SECRET\n").unwrap();
    let data = files.data_root.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let worker = std::thread::spawn(move || {
        for index in 0..300 {
            if done.load(Ordering::Relaxed) {
                break;
            }
            fs::rename(&slot, &parked).unwrap();
            symlink(if index % 2 == 0 { &outside } else { &data }, &slot).unwrap();
            std::thread::yield_now();
            fs::remove_file(&slot).unwrap();
            fs::rename(&parked, &slot).unwrap();
        }
    });
    for _ in 0..100 {
        if let Ok(hits) = grep(&files, ".", None) {
            assert!(
                hits.iter()
                    .all(|h| !h.text.contains("ESCAPE") && !h.text.contains("SECRET"))
            );
        }
    }
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
}

#[tokio::test]
async fn tool14_active_scan_cancel_refuses_partial_and_joins_before_next_query() {
    use crate::config::{Generation, Permission};
    use crate::dcp_auto::DcpConfig;
    use crate::models::ModelCatalog;
    use crate::patch::ProtectedGlobs;
    use crate::provider::{InputItem, ResponsesConfig};
    use crate::runtime::{Runtime, TurnParams, TurnStatus};
    use crate::storage::Db;
    use crate::tools::ToolRoots;
    use std::collections::BTreeMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    struct Peer {
        stop: Arc<AtomicBool>,
        task: Option<std::thread::JoinHandle<()>>,
    }
    impl Drop for Peer {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(task) = self.task.take() {
                let _ = task.join();
            }
        }
    }
    for (name, phase) in [("grep", "hit"), ("glob", "candidate"), ("read", "read")] {
        let (tmp, mut files) = fixture();
        // Exercise the same joined worker/token bridge on an external invocation;
        // internal active-scan guards use this identical owner and checkpoints.
        let cache = tmp.path().join("cache");
        fs::create_dir(&cache).unwrap();
        fs::write(cache.join("a.rs"), "NEEDLE\nNEEDLE\n").unwrap();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let barrier = Arc::new(ScanBarrier {
            phase,
            entered: Mutex::new(Some(entered_tx)),
            cancel_seen: Mutex::new(Some(cancel_tx)),
            release: Mutex::new(release_rx),
            released: AtomicBool::new(false),
        });
        files.scan_barrier = Some(barrier.clone());
        let project = files.root.clone();
        let data = files.data_root.clone();
        let db = Db::open(&data).unwrap();
        let runtime = Runtime::new(
            &db,
            "work",
            Generation {
                permissions: ["read", "grep", "glob", "shell", "external_directory"]
                    .into_iter()
                    .map(|n| (n.into(), Permission::Allow))
                    .collect(),
                ..Default::default()
            },
            ProtectedGlobs { patterns: vec![] },
            files,
            crate::shell::Shell::new(&project).unwrap(),
            BTreeMap::new(),
            ToolRoots {
                project: project.clone(),
                data,
            },
            None,
            false,
            DcpConfig::default(),
        )
        .unwrap();
        runtime.create_session("s").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let captured = requests.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let call = |id: &str, tool: &str, args: serde_json::Value| serde_json::json!({"type":"function_call","id":id,"call_id":id,"name":tool,"arguments":args.to_string(),"status":"completed"});
        let args = if name == "read" {
            serde_json::json!({"path":cache.join("a.rs")})
        } else {
            serde_json::json!({"pattern":if name=="grep" {"NEEDLE"} else {"*.rs"},"path":cache})
        };
        let script = vec![
            vec![
                call("active", name, args.clone()),
                call(
                    "must-not-run",
                    "shell",
                    serde_json::json!({"command":"touch forbidden"}),
                ),
            ],
            vec![call("next", name, args)],
            vec![
                serde_json::json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}),
            ],
        ];
        let server = std::thread::spawn(move || {
            for output in script {
                let (stream, _) = loop {
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    match listener.accept() {
                        Ok(pair) => break pair,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::park_timeout(Duration::from_millis(1))
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':')
                        && key.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                captured
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&body).unwrap());
                let response = format!(
                    "data: {}\n\n",
                    serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output}})
                );
                write!(reader.get_mut(),"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{response}").unwrap();
            }
        });
        let mut peer = Peer {
            stop: stopping.clone(),
            task: Some(server),
        };
        let catalog = ModelCatalog {
            provider: "test".into(),
            models: [(
                "m".into(),
                serde_json::json!({"limit":{"context":1_000_000,"output":100_000}}),
            )]
            .into_iter()
            .collect(),
        };
        let provider = ResponsesConfig {
            headers: BTreeMap::new(),
            set_cache_key: true,
            base_url: base,
            api_key: "fixture".into(),
            timeout: Some(false),
            chunk_timeout_ms: 5000,
            connect_timeout: Duration::from_secs(2),
            allow_private: true,
        };
        let cancel = AtomicBool::new(false);
        let params = || TurnParams {
            session: "s".into(),
            prompt: "Search fixture".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 1000,
            provider: provider.clone(),
            cancel: &cancel,
        };
        let mut finishes = Vec::new();
        let (report, ()) = tokio::join!(
            runtime.run_turn_with_tool_events(
                params(),
                |_| {},
                |_, _| {},
                |_, _| {},
                |_, event| {
                    if let crate::runtime::ToolCallEvent::Finished { name, state, .. } = event {
                        finishes.push((name.clone(), state.clone()));
                    }
                }
            ),
            async {
                entered_rx.await.unwrap();
                cancel.store(true, Ordering::Release);
                // Release only after the actual scan guard observed cancellation.
                // This also proves the current-thread actor/timer keeps polling.
                let _ = cancel_rx.await;
                release_tx.send(()).unwrap();
            }
        );
        // Stop/join the fake even on a RED assertion; no orphan test peer.
        if !barrier.released.load(Ordering::Acquire) {
            stopping.store(true, Ordering::Release);
            peer.task.take().unwrap().join().unwrap();
            panic!(
                "{name}: traversal held after partial result but async cancel/controller could not run"
            );
        }
        let report = report.unwrap();
        assert_eq!(report.status, TurnStatus::Cancelled);
        assert_eq!(report.calls[0].state, "cancelled");
        assert_eq!(report.calls[0].output, "error: cancelled");
        assert_eq!(
            finishes,
            [
                (name.to_string(), "cancelled".to_string()),
                ("shell".to_string(), "cancelled".to_string())
            ]
        );
        assert!(!project.join("forbidden").exists());
        let (_, raw) = db.turn_result(&report.turn_id).unwrap();
        let log = crate::tools::TurnLog::from_json(&serde_json::from_str(&raw.unwrap()).unwrap())
            .unwrap();
        assert!(log.input.iter().any(|item|matches!(item,InputItem::FunctionCallOutput{call_id,output} if call_id=="active"&&output=="error: cancelled")));
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "no provider continuation/replay after active cancellation"
        );
        cancel.store(false, Ordering::Release);
        let next = runtime.run_turn(params()).await.unwrap();
        stopping.store(true, Ordering::Release);
        peer.task.take().unwrap().join().unwrap();
        assert_eq!(next.status, TurnStatus::Completed);
        assert_eq!(next.calls.len(), 1);
        assert_eq!(next.calls[0].state, "completed");
        assert_eq!(requests.lock().unwrap().len(), 3);
        assert!(!project.join("forbidden").exists());
    }
}
