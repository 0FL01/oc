use super::*;

#[tokio::test]
async fn clipboard_command_caps_deadline_cancel_and_reap() {
    let (_tx, mut cancel) = oneshot::channel();
    assert_eq!(
        command(
            "/usr/bin/printf",
            &["hello"],
            5,
            Instant::now() + Duration::from_secs(1),
            &mut cancel
        )
        .await
        .unwrap(),
        Some(b"hello".to_vec())
    );
    assert!(
        command(
            "/usr/bin/printf",
            &["oversize"],
            4,
            Instant::now() + Duration::from_secs(1),
            &mut cancel
        )
        .await
        .unwrap_err()
        .contains("limit")
    );
    assert!(
        command("/bin/cat", &[], 16, Instant::now(), &mut cancel)
            .await
            .unwrap_err()
            .contains("timed out")
    );
    let root = tempfile::tempdir().unwrap();
    let fifo = root.path().join("wait");
    let path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: path is a valid NUL-terminated filename owned by the fixture.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let pidfile = root.path().join("pid");
    let script = format!(
        "echo $$ > '{}'; exec /bin/cat < '{}'",
        pidfile.display(),
        fifo.display()
    );
    let (tx, mut cancel) = oneshot::channel();
    let args = ["-c", &script];
    let read = command(
        "/bin/sh",
        &args,
        16,
        Instant::now() + Duration::from_secs(1),
        &mut cancel,
    );
    let control = async {
        let until = Instant::now() + Duration::from_secs(1);
        let pid = loop {
            if let Ok(raw) = std::fs::read_to_string(&pidfile)
                && let Ok(pid) = raw.trim().parse::<i32>()
            {
                break pid;
            }
            assert!(Instant::now() < until, "helper not ready");
            tokio::task::yield_now().await;
        };
        tx.send(()).unwrap();
        pid
    };
    let (result, pid) = tokio::join!(read, control);
    assert!(result.unwrap_err().contains("cancelled"));
    // SAFETY: signal zero only probes the fixture's former child PID.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "helper must be reaped");
}
