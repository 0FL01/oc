use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn running_rich_inbox_and_direct_shell_cross_actual_native_ipc_without_losing_media() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    let config = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    value["providers"]["fixture"]["models"][MODEL]["capabilities"] =
        serde_json::json!({"input":["text","image"],"output":["text"]});
    value["agents"]["title"] = serde_json::json!({"disabled":true});
    value["permissions"] = serde_json::json!([{"action":"shell","resource":"*","effect":"allow"}]);
    std::fs::write(&config, value.to_string()).unwrap();
    let helper = fixture.root.path().join("clipboard-bin");
    std::fs::create_dir(&helper).unwrap();
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgZGL+DwABFAEG1rmmRQAAAABJRU5ErkJggg==";
    let file = fixture.root.path().join("clipboard.b64");
    std::fs::write(&file, png).unwrap();
    let script = helper.join("wl-paste");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nexec /usr/bin/base64 -d '{}'\n", file.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    seed_session(&fixture.data_dir(), &project, "rich-busy", 0);
    let mut pty = PtySession::spawn(fixture.clone(), "rich-busy", None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "vis28 held stream");
    fixture.wait_requests(1);
    pty.send(b"!printf BUSY-SHELL > busy-shell.effect\r");
    let start = Instant::now();
    while !project.join("busy-shell.effect").exists() {
        assert!(
            start.elapsed() < DEADLINE,
            "busy shell did not execute: {:?}",
            render_screen(&pty.snapshot()).rows()
        );
        std::thread::sleep(POLL);
    }
    assert_eq!(
        fixture.wait_requests(1).len(),
        1,
        "shell must not create a model request"
    );
    let admission_ready = |pty: &mut PtySession| {
        // The pending echo precedes its receipt. This local command's shortcut
        // is enabled only after that exact admission has settled; never run it.
        pty.send(b"\x10Close tab");
        wait_screen_row(pty, "ctrl+x w", DEADLINE);
        pty.send(b"\x1b");
        dismissed(pty, "Commands");
    };
    pty.send(b"steered image ");
    pty.send(b"\x16");
    prompt_history::wait_prompt(&pty, &["steered image [Image 1]"]);
    pty.send(b"\r");
    wait_screen_row(&pty, "steered image", DEADLINE);
    admission_ready(&mut pty);
    pty.send(b"queued image \x16");
    prompt_history::wait_prompt(&pty, &["queued image [Image 1]"]);
    pty.send(b"\x18\r");
    wait_screen_row(&pty, "1 queued", DEADLINE);
    admission_ready(&mut pty);
    pty.send(b"/t39cmd steered\r");
    wait_screen_row(&pty, "/t39cmd steered", DEADLINE);
    admission_ready(&mut pty);
    pty.send(b"/t39cmd queued\x18\r");
    wait_screen_row(&pty, "2 queued", DEADLINE);
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    wait_screen_row(&pty, "echo: custom command payload for queued", DEADLINE);
    wait_idle(&pty);
    let requests = fixture.wait_requests(5);
    assert_eq!(
        requests.len(),
        5,
        "initial, image steer, command steer, image queue, command queue"
    );
    for body in requests.iter().skip(1) {
        assert!(
            body.to_string().contains(png),
            "rich history retains its admitted media"
        );
    }
    assert!(
        requests[2]
            .to_string()
            .contains("custom command payload for steered")
    );
    assert!(
        requests[4]
            .to_string()
            .contains("custom command payload for queued")
    );
    let conn = rusqlite::Connection::open_with_flags(
        pty.data_dir().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM turns WHERE session_id='rich-busy'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        3
    );
    assert_eq!(conn.query_row("SELECT count(*) FROM session_inbox WHERE session_id='rich-busy' AND message_id IS NULL",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}

#[test]
fn clipboard_images_actual_elf_png_text_wire_chip_restart_no_replay() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    let config = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    value["providers"]["fixture"]["models"][MODEL]["capabilities"] =
        serde_json::json!({"input":["text","image"],"output":["text"]});
    std::fs::write(&config, value.to_string()).unwrap();
    let helper = fixture.root.path().join("clipboard-bin");
    std::fs::create_dir(&helper).unwrap();
    // Complete 1x1 RGBA PNG with valid chunk CRCs and decoded pixels.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgZGL+DwABFAEG1rmmRQAAAABJRU5ErkJggg==";
    let file = fixture.root.path().join("clipboard.b64");
    std::fs::write(&file, png).unwrap();
    let reads = fixture.root.path().join("clipboard-reads");
    let script = helper.join("wl-paste");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexec /usr/bin/base64 -d '{}'\n",
            reads.display(),
            file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    seed_session(&fixture.data_dir(), &project, "clipboard-images", 0);
    let mut pty = PtySession::spawn(fixture.clone(), "clipboard-images", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x16");
    prompt_history::wait_prompt(&pty, &["[Image 1]"]);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\r");
    let requests = fixture.wait_requests(1);
    let image = requests[0]["input"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .find(|part| part["type"] == "input_image")
        .unwrap();
    assert_eq!(image["image_url"], format!("data:image/png;base64,{png}"));
    wait_screen_row(&pty, "clipboard", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("file") && r.contains("clipboard"))
    );
    wait_idle(&pty);
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success());
    let mut pty = PtySession::spawn(fixture.clone(), "clipboard-images", None);
    wait_screen_row(&pty, "clipboard", DEADLINE);
    assert_eq!(
        fixture.wait_requests(1).len(),
        1,
        "reopen must not replay generation"
    );
    // Nonempty bracketed paste never asks the host clipboard.
    pty.send(b"\x1b[200~continue image context\x1b[201~\r");
    let requests = fixture.wait_requests(2);
    assert!(
        requests[1].to_string().contains(png),
        "ordinary continuation retains admitted images"
    );
    wait_idle(&pty);
    assert_eq!(std::fs::read_to_string(&reads).unwrap().lines().count(), 1);
    // Same-backend text fallback, reached by an empty bracketed paste.
    std::fs::write(&script, format!("#!/bin/sh\necho \"$*\" >> '{}'\ncase \"$*\" in *image/png*) exit 1;; esac\nprintf 'clipboard text'\n", reads.display())).unwrap();
    pty.send(b"\x1b[200~\x1b[201~");
    prompt_history::wait_prompt(&pty, &["clipboard text"]);
    assert_eq!(std::fs::read_to_string(&reads).unwrap().lines().count(), 3);
    pty.send(b"\x03\x03");
    assert!(pty.wait_exit(DEADLINE).0.success());
}

#[test]
fn running_queue_survives_interruption_and_reopen_until_explicit_idle_promotion() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "parked-queue", 0);
    let mut pty = PtySession::spawn(fixture.clone(), "parked-queue", None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "vis28 held stream");
    fixture.wait_requests(1);
    pty.send(b"PARKED_RESUME\x18\r");
    wait_screen_row(&pty, "1 queued", DEADLINE);
    pty.send(b"CONTINUE_STEER\r");
    wait_screen_row(&pty, "CONTINUE_STEER", DEADLINE);
    prompt_history::wait_prompt(&pty, &[]);
    pty.send(b"\x1b");
    wait_screen_row(&pty, "esc again to interrupt", DEADLINE);
    pty.send(b"\x1b");
    assert_eq!(
        last_user_text(&fixture.wait_requests(2)[1]).as_deref(),
        Some("CONTINUE_STEER")
    );
    wait_screen_row(&pty, "echo: CONTINUE_STEER", DEADLINE);
    wait_idle(&pty);
    assert_eq!(fixture.wait_requests(2).len(), 2);
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success());
    let mut pty = PtySession::spawn(fixture.clone(), "parked-queue", None);
    wait_screen_row(&pty, "1 queued", DEADLINE);
    assert_eq!(fixture.wait_requests(2).len(), 2);
    pty.send(b"\r");
    wait_screen_row(&pty, "echo: PARKED_RESUME", DEADLINE);
    wait_idle(&pty);
    assert_eq!(fixture.wait_requests(3).len(), 3);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}
