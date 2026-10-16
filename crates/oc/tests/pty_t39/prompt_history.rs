use super::*;

fn wait_prompt(pty: &PtySession, lines: &[&str]) {
    let start = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if let Some(metadata) = rows.iter().rposition(|row| row.contains("Build ·"))
            && metadata > lines.len()
            // Session has one spare input row; Home does not. Stay strictly
            // inside these prompt rows rather than matching the transcript.
            && (metadata - lines.len() - 1..metadata).any(|start| {
                start + lines.len() <= metadata
                    && rows[start..start + lines.len()]
                        .iter()
                        .zip(lines)
                        .all(|(row, expected)| row.contains(expected))
                    && rows[start + lines.len()..metadata]
                        .iter()
                        .all(|row| matches!(row.trim(), "" | "┃"))
            })
        {
            return;
        }
        assert!(start.elapsed() < DEADLINE, "prompt {lines:?}: {rows:?}");
        std::thread::sleep(POLL);
    }
}

fn shared_history(fixture: &Fixture) -> Vec<String> {
    // Observe only the fixture's bounded committed value; do not acquire a
    // second application owner or bypass its live data-root write lock.
    let sql = rusqlite::Connection::open_with_flags(
        fixture.data_dir().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let raw: String = sql
        .query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB))<=4096 THEN value END FROM prefs WHERE key='tui.prompt_history.v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn wait_empty_prompt(pty: &PtySession) {
    let start = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if let Some(metadata) = rows.iter().rposition(|row| row.contains("Build ·"))
            && metadata >= 2
            && rows[metadata - 2..metadata]
                .iter()
                .all(|row| matches!(row.trim(), "" | "┃"))
        {
            return;
        }
        assert!(start.elapsed() < DEADLINE, "empty prompt: {rows:?}");
        std::thread::sleep(POLL);
    }
}

#[test]
fn vis12_actual_shared_history_home_restart_recalled_wire_and_active_owner() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    std::fs::write(project.join("note.txt"), "HISTORY_FILE_CURRENT_CANARY\n").unwrap();
    seed_session(&fixture.data_dir(), &project, "vis12-input-history", 0);
    let mut pty = PtySession::spawn(fixture.clone(), "vis12-input-history", None);
    pty.wait_visible(READY, DEADLINE);
    let first = "VIS12-HISTORY-FIRST Ω界";
    let second = "VIS12-HISTORY-SECOND\nΩ界 second-line";
    let third = "VIS12-HISTORY-THIRD @note.txt";
    for (index, text) in [first, second, third].into_iter().enumerate() {
        pty.send(format!("\x1b[200~{text}\x1b[201~").as_bytes());
        pty.send(b"\r");
        let requests = fixture.wait_requests(index + 1);
        assert_eq!(last_user_text(&requests[index]).as_deref(), Some(text));
        wait_screen_row(&pty, &user_needle(text.lines().next().unwrap()), DEADLINE);
        wait_idle(&pty);
    }
    assert_eq!(shared_history(&fixture), [first, second, third]);
    pty.send(b"\x1b[A");
    wait_prompt(&pty, &[third]);
    pty.send(b"\x1b[A");
    wait_prompt(&pty, &["VIS12-HISTORY-SECOND", "second-line"]);
    pty.send(b"\x1b[A");
    wait_prompt(&pty, &["VIS12-HISTORY-FIRST"]);
    pty.send(b"\x1b[B\x1b[B"); // raw end, then next history item
    wait_prompt(&pty, &["VIS12-HISTORY-SECOND", "second-line"]);
    pty.send(b"\x1b[B");
    wait_prompt(&pty, &[third]);
    // A resolvable textual @ref uses the existing autocomplete owner at its end.
    wait_screen_row(&pty, "┃ note.txt", DEADLINE);
    pty.send(b"\x1b[B");
    wait_prompt(&pty, &[third]);
    pty.send(b"\x1b"); // dismiss @overlay before exiting input history
    wait_screen_absent(&pty, "┃ note.txt");
    pty.send(b"\x1b[B");
    wait_empty_prompt(&pty);
    assert_eq!(fixture.wait_requests(3).len(), 3);
    // Actual shared Home, not a window seeded from another session's messages.
    pty.send(b"\x18n");
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    pty.send(b"\x1b[A");
    wait_prompt(&pty, &[third]);
    assert_eq!(
        fixture.wait_requests(3).len(),
        3,
        "recall is not submission"
    );
    pty.send(b"\r");
    let requests = fixture.wait_requests(4);
    assert_eq!(last_user_text(&requests[3]).as_deref(), Some(third));
    let users: Vec<_> = requests[3]["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["role"] == "user")
        .collect();
    assert_eq!(users.len(), 1, "new session does not copy conversation RAW");
    wait_screen_row(&pty, &user_needle(third), DEADLINE);
    wait_idle(&pty);
    assert_eq!(shared_history(&fixture), [first, second, third]);
    // Recall while a genuine stream is held cannot alter its accepted request.
    pty.send(b"vis28 held stream\r");
    let captured = fixture.wait_requests(5);
    wait_screen_row(&pty, "esc interrupt", DEADLINE);
    pty.send(b"draft \x1b[H\x1b[A");
    wait_prompt(&pty, &["vis28 held stream"]);
    assert_eq!(fixture.wait_requests(5), captured);
    assert_eq!(
        last_user_text(&captured[4]).as_deref(),
        Some("vis28 held stream")
    );
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    wait_screen_row(&pty, "answer:vis28 completed", DEADLINE);
    wait_idle(&pty);
    pty.send(b"\x03"); // clear the recalled copy, not cancel/replay the completed turn
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(fixture.wait_requests(5).len(), 5);
    let mut restarted = PtySession::spawn(fixture.clone(), "vis12-input-history", None);
    wait_screen_row(&restarted, "Build ·", DEADLINE);
    restarted.send(b"\x1b[A");
    wait_prompt(&restarted, &["vis28 held stream"]);
    restarted.send(b"\x1b[A");
    wait_prompt(&restarted, &[third]);
    assert_eq!(fixture.wait_requests(5).len(), 5);
    restarted.send(b"\x03\x03");
    assert!(restarted.wait_exit(DEADLINE).0.success() && restarted.restored());
    assert_eq!(fixture.wait_requests(5).len(), 5);
    assert!(
        captured
            .iter()
            .all(|request| !has_function_call_output(request)),
        "history does not dispatch tools"
    );
    assert!(
        captured
            .iter()
            .all(|request| !request.to_string().contains("HISTORY_FILE_CURRENT_CANARY")),
        "text-only recall cannot silently convert a literal @ref into a file read"
    );
}
