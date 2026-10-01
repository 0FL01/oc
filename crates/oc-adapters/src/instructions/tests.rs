use super::*;
use crate::config::Permission;
use std::collections::BTreeMap;
use std::os::unix::fs::symlink;

#[test]
fn prm01_automatic_ask_sources_are_not_opened_in_either_resource_form() {
    use std::os::fd::FromRawFd as _;
    use std::os::unix::ffi::OsStrExt as _;
    let mut observations = Vec::new();
    for absolute in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let data = temp.path().join("data");
        std::fs::create_dir_all(project.join("nested")).unwrap();
        std::fs::write(project.join("nested/file.txt"), "allowed file\n").unwrap();
        let source_path = project.join("nested/AGENTS.md");
        std::fs::write(&source_path, "ASK_SOURCE_MUST_NOT_OPEN\n").unwrap();
        let root = Root {
            path: project.clone(),
            dir: Arc::new(File::open(&project).unwrap()),
            origin: Origin::Project,
            baseline: None,
        };
        let files = crate::files::Files::new(&project, &data).unwrap();
        let permissions = BTreeMap::from([("read".into(), Permission::Allow)]);
        let source_rule = if absolute {
            source_path.to_string_lossy().into_owned()
        } else {
            "nested/AGENTS.md".into()
        };
        let rules = crate::permissions::PermissionRules::from_config(&serde_json::json!({"permission":{"read":{"*":"allow","nested/file.txt":"allow",source_rule:"ask"}}})).unwrap();
        let policy = RuntimePolicy::with_rules(&permissions, &rules).with_root(&project);
        let name = std::ffi::CString::new(source_path.as_os_str().as_bytes()).unwrap();
        // SAFETY: a new owned inotify fd, valid NUL path and IN_OPEN mask.
        let fd = unsafe { libc::inotify_init1(libc::O_NONBLOCK | libc::O_CLOEXEC) };
        assert!(fd >= 0);
        // SAFETY: fd is unique and now transferred to File ownership.
        let mut watch = unsafe { File::from_raw_fd(fd) };
        // SAFETY: fd and name remain live through this watch registration.
        assert!(unsafe { libc::inotify_add_watch(fd, name.as_ptr(), libc::IN_OPEN) } >= 0);
        assert_eq!(
            files.read("nested/file.txt", 1, 2).unwrap().lines,
            vec!["allowed file"]
        );
        let sources = after_read(
            &[root],
            &files,
            "nested/file.txt",
            false,
            1,
            &data,
            &policy,
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut buffer = [0; 4096];
        let opened = match watch.read(&mut buffer) {
            Ok(size) => size != 0,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => false,
            Err(error) => panic!("watch error: {error}"),
        };
        observations.push((
            absolute,
            opened,
            sources.iter().any(|s| s.content.is_some()),
        ));
    }
    assert_eq!(
        observations,
        vec![(false, false, false), (true, false, false)]
    );
}

#[test]
fn prm01_approved_ask_file_guard_keeps_permanent_allow_source_admission() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let data = temp.path().join("data");
    std::fs::create_dir_all(project.join("nested")).unwrap();
    std::fs::write(project.join("nested/file.txt"), "file\n").unwrap();
    std::fs::write(project.join("nested/AGENTS.md"), "permanent source Allow\n").unwrap();
    let root = Root {
        path: project.clone(),
        dir: Arc::new(File::open(&project).unwrap()),
        origin: Origin::Project,
        baseline: None,
    };
    let files = crate::files::Files::new(&project, &data).unwrap();
    let permissions = BTreeMap::from([("read".into(), Permission::Ask)]);
    for source_rule in [
        "nested/AGENTS.md".to_string(),
        project
            .join("nested/AGENTS.md")
            .to_string_lossy()
            .into_owned(),
    ] {
        let rules = crate::permissions::PermissionRules::from_config(&serde_json::json!({"permission":{"read":{"*":"ask","nested/file.txt":"ask",source_rule:"allow"}}})).unwrap();
        let policy = RuntimePolicy::with_rules(&permissions, &rules).with_root(&project);
        assert_eq!(policy.effect("read", "nested/file.txt"), Permission::Ask);
        // after_read is called only following the separate original approval.
        let sources = after_read(
            std::slice::from_ref(&root),
            &files,
            "nested/file.txt",
            false,
            1,
            &data,
            &policy,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            sources[0].content.as_deref(),
            Some("permanent source Allow\n")
        );
    }
}

#[test]
fn prm01_nested_fifo_returns_before_regular_file_check_deadline() {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let data = temp.path().join("data");
    std::fs::create_dir_all(project.join("nested")).unwrap();
    let fifo = project.join("nested/AGENTS.md");
    let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: valid owned fixture path and ordinary FIFO mode.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let root = Root {
        path: project.clone(),
        dir: Arc::new(File::open(&project).unwrap()),
        origin: Origin::Project,
        baseline: None,
    };
    let (send, recv) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let permissions = BTreeMap::from([("read".into(), Permission::Allow)]);
        let policy = RuntimePolicy::new(&permissions).with_root(&project);
        let result = read_source(
            &root,
            Path::new("nested/AGENTS.md"),
            Origin::Nested,
            1,
            &data,
            &policy,
            &AtomicBool::new(false),
        );
        send.send(result).unwrap();
    });
    let result = recv.recv_timeout(std::time::Duration::from_secs(1));
    // If the descriptor opener regresses to blocking, release only this owned
    // reader so the failing test still joins its worker and removes its FIFO.
    let release = result.is_err().then(|| {
        std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&fifo)
            .unwrap()
    });
    worker.join().unwrap();
    drop(release);
    assert_eq!(
        result.unwrap().unwrap_err(),
        "instructions are not a bounded regular file"
    );
}

#[test]
fn prm01_initial_canonical_alias_is_admitted_inside_pinned_root_only() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("actual.md"), "canonical initial").unwrap();
    symlink("actual.md", project.join("AGENTS.md")).unwrap();
    let mut root = Root {
        path: project.clone(),
        dir: Arc::new(File::open(&project).unwrap()),
        origin: Origin::Project,
        baseline: None,
    };
    let permissions = BTreeMap::from([("read".into(), Permission::Allow)]);
    let policy = RuntimePolicy::new(&permissions).with_root(&project);
    let cancel = AtomicBool::new(false);
    let source = read_source(
        &root,
        Path::new("actual.md"),
        Origin::Project,
        3,
        &temp.path().join("data"),
        &policy,
        &cancel,
    )
    .unwrap();
    assert_eq!(source.path, project.join("actual.md").to_string_lossy());
    assert_eq!(source.content.as_deref(), Some("canonical initial"));
    root.baseline = Some(Arc::new(source));
    std::fs::write(project.join("actual.md"), "unadmitted generation").unwrap();
    assert_eq!(
        root.baseline.as_ref().unwrap().content.as_deref(),
        Some("canonical initial")
    );
    std::fs::remove_file(project.join("AGENTS.md")).unwrap();
    assert!(
        read_source(
            &root,
            Path::new("AGENTS.md"),
            Origin::Nested,
            3,
            &temp.path().join("data"),
            &policy,
            &cancel
        )
        .unwrap()
        .content
        .is_none()
    );
    let external = temp.path().join("external.md");
    std::fs::write(&external, "must not escape").unwrap();
    symlink(&external, project.join("AGENTS.md")).unwrap();
    assert!(
        read_source(
            &root,
            Path::new("AGENTS.md"),
            Origin::Nested,
            3,
            &temp.path().join("data"),
            &policy,
            &cancel
        )
        .unwrap()
        .content
        .is_none()
    );
}

#[test]
fn prm01_future_directory_hook_and_source_guards_share_admitted_descriptors() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let data = project.join("data");
    std::fs::create_dir_all(project.join("one/two")).unwrap();
    std::fs::create_dir(&data).unwrap();
    std::fs::write(project.join("one/AGENTS.md"), "outer").unwrap();
    std::fs::write(project.join("one/two/AGENTS.md"), "inner").unwrap();
    std::fs::write(project.join("one/two/file"), "file").unwrap();
    let root = Root {
        path: project.clone(),
        dir: Arc::new(File::open(&project).unwrap()),
        origin: Origin::Project,
        baseline: None,
    };
    let files = crate::files::Files::new(&project, &data).unwrap();
    let permissions = BTreeMap::from([("read".into(), Permission::Allow)]);
    let policy = RuntimePolicy::new(&permissions).with_root(&project);
    let cancel = AtomicBool::new(false);
    let sources = after_read(
        std::slice::from_ref(&root),
        &files,
        "one/two",
        true,
        7,
        &data,
        &policy,
        &cancel,
    )
    .unwrap();
    assert_eq!(
        sources
            .iter()
            .map(|s| s.content.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["inner", "outer"]
    );
    assert_eq!(
        sources[0].digest.as_deref(),
        Some(format!("{:x}", Sha256::digest(b"inner")).as_str())
    );
    let file_sources = after_read(
        std::slice::from_ref(&root),
        &files,
        "one/two/file",
        false,
        7,
        &data,
        &policy,
        &cancel,
    )
    .unwrap();
    assert_eq!(sources, file_sources);
    let missing = BTreeMap::new();
    let no_authority = RuntimePolicy::new(&missing).with_root(&project);
    assert!(
        after_read(
            std::slice::from_ref(&root),
            &files,
            "one/two/file",
            false,
            7,
            &data,
            &no_authority,
            &cancel
        )
        .is_err()
    );
    assert!(
        read_source(
            &root,
            Path::new("one/two/AGENTS.md"),
            Origin::Nested,
            7,
            &data,
            &no_authority,
            &cancel
        )
        .unwrap()
        .content
        .is_none()
    );
    let rules = crate::permissions::PermissionRules::from_config(&serde_json::json!({"permission":{"read":{"*":"allow",project.join("one/two/AGENTS.md").to_string_lossy():"deny"}}})).unwrap();
    let denied = RuntimePolicy::with_rules(&permissions, &rules).with_root(&project);
    let sources = after_read(
        std::slice::from_ref(&root),
        &files,
        "one/two/file",
        false,
        7,
        &data,
        &denied,
        &cancel,
    )
    .unwrap();
    assert_eq!(sources.iter().filter(|s| s.content.is_some()).count(), 1);
    std::fs::remove_file(project.join("one/two/AGENTS.md")).unwrap();
    let external = temp.path().join("external");
    std::fs::write(&external, "EXTERNAL_MUST_NOT_BE_READ").unwrap();
    symlink(&external, project.join("one/two/AGENTS.md")).unwrap();
    let sources = after_read(
        std::slice::from_ref(&root),
        &files,
        "one/two/file",
        false,
        7,
        &data,
        &policy,
        &cancel,
    )
    .unwrap();
    assert_eq!(sources.iter().filter(|s| s.content.is_some()).count(), 1);
    assert_eq!(sources[1].content.as_deref(), Some("outer"));
    assert!(
        after_read(
            std::slice::from_ref(&root),
            &files,
            "data",
            true,
            7,
            &data,
            &policy,
            &cancel
        )
        .is_err()
    );
    cancel.store(true, Ordering::Release);
    assert!(after_read(&[root], &files, "one/two", true, 7, &data, &policy, &cancel).is_err());
}
