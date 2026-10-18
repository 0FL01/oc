use super::*;

#[test]
fn srv05_fifo_registration_is_rejected_without_waiting_for_a_writer() {
    let temp = tempfile::tempdir().unwrap();
    let data = root(&temp.path().join("data")).unwrap();
    let path = registry(&data);
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: the NUL-terminated path names only this isolated fixture's FIFO.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let (result, ready) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || result.send(read(&data).is_err()).unwrap());
    let refused = ready.recv_timeout(std::time::Duration::from_secs(1));
    // Unblock the old blocking implementation on failure so this regression
    // never leaves an owned thread behind. Correct admission needs no writer.
    let unblock = refused.is_err().then(|| {
        OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap()
    });
    worker.join().unwrap();
    drop(unblock);
    assert!(refused.unwrap());
}

#[tokio::test]
async fn srv05_private_peer_registration_and_unsafe_paths_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let data = root(&temp.path().join("data")).unwrap();
    let db = crate::storage::Db::open(&data).unwrap();
    let endpoint = Endpoint::bind(&data).unwrap();
    let registration = read(&data).unwrap().unwrap();
    let stream = connect(&data, &registration).await.unwrap();
    assert_eq!(stream.peer_cred().unwrap().uid(), uid());
    drop(stream);
    let mut forged = registration.clone();
    forged.pid = forged.pid.checked_add(1).unwrap();
    assert!(
        connect(&data, &forged).await.is_err(),
        "metadata cannot spoof the actual socket peer"
    );
    fs::set_permissions(&registration.socket, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(
        connect(&data, &registration).await.is_err(),
        "permissive socket is not an admitted boundary"
    );
    fs::set_permissions(&registration.socket, fs::Permissions::from_mode(0o600)).unwrap();
    let mut foreign = registration.clone();
    foreign.uid = uid().wrapping_add(1);
    fs::write(registry(&data), serde_json::to_vec(&foreign).unwrap()).unwrap();
    assert!(
        read(&data).is_err(),
        "a registration from another UID is not authority"
    );
    fs::write(registry(&data), serde_json::to_vec(&registration).unwrap()).unwrap();
    let linked = temp.path().join("linked");
    std::os::unix::fs::symlink(&data, &linked).unwrap();
    assert!(
        root(&linked).is_err(),
        "no canonicalization bypass of final-root no-follow"
    );
    let mut incompatible = registration;
    incompatible.version = VERSION + 1;
    fs::write(registry(&data), serde_json::to_vec(&incompatible).unwrap()).unwrap();
    let error = crate::native::attach(&data, None, crate::native::Lease::Auth)
        .await
        .err()
        .expect("incompatible owner must not be restarted");
    assert_eq!(
        error.diagnostic.code,
        oc_core::queries::ServiceCode::ProtocolMismatch
    );
    assert!(matches!(
        crate::storage::Db::open(&data),
        Err(crate::storage::StorageError::DataRootBusy)
    ));
    drop(endpoint);
    drop(db);
}

#[tokio::test]
async fn srv01_election_winner_replaces_only_its_safe_stale_socket() {
    let temp = tempfile::tempdir().unwrap();
    let data = root(&temp.path().join("data")).unwrap();
    let db = crate::storage::Db::open(&data).unwrap();
    let first = Endpoint::bind(&data).unwrap();
    let saved = first.registration.clone();
    drop(first);
    // Reproduce only the on-disk endpoint left by a crash; no second Db or
    // recovery pass is opened and no live owner is killed by this unit fixture.
    let listener = std::os::unix::net::UnixListener::bind(&saved.socket).unwrap();
    fs::set_permissions(&saved.socket, fs::Permissions::from_mode(0o600)).unwrap();
    drop(listener);
    fs::write(registry(&data), serde_json::to_vec(&saved).unwrap()).unwrap();
    fs::set_permissions(registry(&data), fs::Permissions::from_mode(0o600)).unwrap();
    let elected = Endpoint::bind(&data).unwrap();
    assert_ne!(saved.instance, elected.registration.instance);
    assert!(connect(&data, &elected.registration).await.is_ok());
    assert!(matches!(
        crate::storage::Db::open(&data),
        Err(crate::storage::StorageError::DataRootBusy)
    ));
    drop(elected);
    drop(db);
    assert!(read(&data).unwrap().is_none());
}
