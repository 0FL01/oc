use super::*;
use std::os::unix::fs::PermissionsExt;

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
