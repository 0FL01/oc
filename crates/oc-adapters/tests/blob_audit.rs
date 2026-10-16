//! AUD08: content-addressed orphan repair and durable blob ordering.
use std::fs;
use std::sync::Barrier;
use std::time::Duration;

use oc_adapters::storage::{Db, StorageError};
use rusqlite::{Connection, params};
use sha2::{Digest as _, Sha256};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn existing_content_addressed_orphan_is_readable_after_write() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    let bytes = b"valid orphan from file-to-row crash window";
    let expected = digest(bytes);
    fs::write(db.root().join("blobs").join(&expected), bytes).unwrap();

    let returned = db.write_blob(bytes).unwrap();
    assert_eq!(returned, expected);
    assert_eq!(db.read_blob(&returned).unwrap(), bytes);
}

#[test]
fn row_failure_leaves_orphan_and_retry_repairs_after_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    let db = Db::open_with_quota(&root, 8).unwrap();
    let sql = Connection::open(root.join("oc.sqlite")).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER fail_blob BEFORE INSERT ON blobs
         BEGIN SELECT RAISE(ABORT, 'injected blob row failure'); END;",
    )
    .unwrap();
    let bytes = b"12345678";
    let expected = digest(bytes);
    for _ in 0..2 {
        assert!(matches!(db.write_blob(bytes), Err(StorageError::Sqlite(_))));
        assert_eq!(fs::read(root.join("blobs").join(&expected)).unwrap(), bytes);
        assert!(matches!(
            db.read_blob(&expected),
            Err(StorageError::BlobNotFound)
        ));
    }
    // The unpublished file still consumes physical quota.
    assert!(matches!(
        db.write_blob(b"x"),
        Err(StorageError::StorageFull)
    ));
    drop(db);
    sql.execute_batch("DROP TRIGGER fail_blob").unwrap();
    let db = Db::open_with_quota(&root, 8).unwrap();
    assert_eq!(db.write_blob(bytes).unwrap(), expected);
    assert_eq!(db.read_blob(&expected).unwrap(), bytes);
    assert_eq!(db.gc_orphans(Duration::ZERO).unwrap(), 0);
    assert_eq!(db.read_blob(&expected).unwrap(), bytes);
    let (count, used): (i64, i64) = sql
        .query_row("SELECT COUNT(*), SUM(size) FROM blobs", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((count, used), (1, 8));
}

#[test]
fn orphan_adoption_and_dedup_obey_quota_without_double_counting() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open_with_quota(&temp.path().join("data"), 8).unwrap();
    let first = db.write_blob(b"1234").unwrap();
    let orphan = digest(b"5678");
    fs::write(db.root().join("blobs").join(&orphan), b"5678").unwrap();
    assert_eq!(db.write_blob(b"5678").unwrap(), orphan);
    assert_eq!(db.write_blob(b"1234").unwrap(), first);
    assert!(matches!(
        db.write_blob(b"x"),
        Err(StorageError::StorageFull)
    ));
    assert_eq!(db.gc_orphans(Duration::ZERO).unwrap(), 0);
    assert_eq!(db.read_blob(&first).unwrap(), b"1234");
    assert_eq!(db.read_blob(&orphan).unwrap(), b"5678");
}

#[test]
fn oversized_orphan_is_rejected_and_gc_recovers_quota() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open_with_quota(&temp.path().join("data"), 4).unwrap();
    let bytes = b"12345";
    let orphan = digest(bytes);
    fs::write(db.root().join("blobs").join(&orphan), bytes).unwrap();
    assert!(matches!(
        db.write_blob(bytes),
        Err(StorageError::StorageFull)
    ));
    assert!(matches!(
        db.read_blob(&orphan),
        Err(StorageError::BlobNotFound)
    ));
    assert_eq!(db.gc_orphans(Duration::ZERO).unwrap(), 1);
    assert_eq!(
        db.read_blob(&db.write_blob(b"1234").unwrap()).unwrap(),
        b"1234"
    );
}

#[test]
fn stale_metadata_is_repaired_and_corrupt_content_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open_with_quota(&temp.path().join("data"), 4).unwrap();
    let expected = db.write_blob(b"1234").unwrap();
    let sql = Connection::open(db.root().join("oc.sqlite")).unwrap();
    sql.execute("UPDATE blobs SET size = -1, path = '../outside'", [])
        .unwrap();
    assert!(matches!(db.read_blob(&expected), Err(StorageError::Io(_))));
    assert_eq!(db.write_blob(b"1234").unwrap(), expected);
    let row: (i64, String) = sql
        .query_row(
            "SELECT size, path FROM blobs WHERE digest = ?1",
            params![expected],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, (4, expected.clone()));
    assert_eq!(db.read_blob(&expected).unwrap(), b"1234");
    let path = db.root().join("blobs").join(&expected);
    fs::write(&path, b"5678").unwrap();
    assert!(matches!(db.read_blob(&expected), Err(StorageError::Io(_))));
    assert!(matches!(db.write_blob(b"1234"), Err(StorageError::Io(_))));
    // A referenced file is preserved even when corrupt.
    assert_eq!(db.gc_orphans(Duration::ZERO).unwrap(), 0);
    fs::remove_file(&path).unwrap();
    assert_eq!(db.write_blob(b"1234").unwrap(), expected);
    assert_eq!(db.read_blob(&expected).unwrap(), b"1234");
}

#[test]
fn read_propagates_sqlite_failure() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    let expected = db.write_blob(b"bytes").unwrap();
    let sql = Connection::open(db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("DROP TABLE blobs").unwrap();
    assert!(matches!(
        db.read_blob(&expected),
        Err(StorageError::Sqlite(_))
    ));
    assert!(matches!(
        db.gc_orphans(Duration::ZERO),
        Err(StorageError::Sqlite(_))
    ));
    assert!(db.root().join("blobs").join(expected).exists());
}

#[test]
fn gc_respects_grace_references_and_non_regular_entries() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    let referenced = db.write_blob(b"keep").unwrap();
    let dir = db.root().join("blobs");
    let orphan = dir.join(digest(b"orphan"));
    fs::write(&orphan, b"orphan").unwrap();
    let stale_temp = dir.join(".tmp-crashed");
    fs::write(&stale_temp, b"partial").unwrap();
    let nested = dir.join(".tmp-directory");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("keep"), b"keep").unwrap();
    let outside = temp.path().join("outside");
    fs::write(&outside, b"outside").unwrap();
    let link = dir.join(".tmp-link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert_eq!(db.gc_orphans(Duration::from_secs(3600)).unwrap(), 0);
    assert_eq!(db.gc_orphans(Duration::ZERO).unwrap(), 2);
    assert!(!orphan.exists());
    assert!(!stale_temp.exists());
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&outside).unwrap(), b"outside");
    assert_eq!(fs::read(nested.join("keep")).unwrap(), b"keep");
    assert_eq!(db.read_blob(&referenced).unwrap(), b"keep");
}

#[test]
fn abandoned_temp_does_not_block_retry_and_counts_toward_quota() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open_with_quota(&temp.path().join("data"), 8).unwrap();
    let expected = digest(b"1234");
    // Legacy deterministic temp name must not block the same process/digest.
    let stale =
        db.root()
            .join("blobs")
            .join(format!(".tmp-{}-{}", std::process::id(), &expected[..16]));
    fs::write(&stale, b"part").unwrap();
    assert_eq!(db.write_blob(b"1234").unwrap(), expected);
    assert!(matches!(
        db.write_blob(b"x"),
        Err(StorageError::StorageFull)
    ));
    assert_eq!(db.gc_orphans(Duration::ZERO).unwrap(), 1);
    assert_eq!(
        db.read_blob(&db.write_blob(b"5678").unwrap()).unwrap(),
        b"5678"
    );
}

#[test]
fn concurrent_writes_do_not_overrun_quota() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open_with_quota(&temp.path().join("data"), 8).unwrap();
    let start = Barrier::new(2);
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            start.wait();
            db.write_blob(b"aaaaa")
        });
        let b = scope.spawn(|| {
            start.wait();
            db.write_blob(b"bbbbb")
        });
        let results = [a.join().unwrap(), b.join().unwrap()];
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|r| matches!(r, Err(StorageError::StorageFull)))
                .count(),
            1
        );
    });
}

#[test]
fn concurrent_gc_preserves_published_blobs_and_active_temps() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    let start = Barrier::new(2);
    std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            start.wait();
            for byte in 0..32 {
                let bytes = vec![byte; 16 * 1024];
                let hash = db.write_blob(&bytes).unwrap();
                assert_eq!(db.read_blob(&hash).unwrap(), bytes);
            }
        });
        let gc = scope.spawn(|| {
            start.wait();
            for _ in 0..32 {
                assert_eq!(db.gc_orphans(Duration::ZERO).unwrap(), 0);
            }
        });
        writer.join().unwrap();
        gc.join().unwrap();
    });
    assert_eq!(fs::read_dir(db.root().join("blobs")).unwrap().count(), 32);
}

#[test]
fn partial_temp_write_failure_is_cleaned_up() {
    const CHILD: &str = "OC_BLOB_WRITE_FAULT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "partial_temp_write_failure_is_cleaned_up",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    // Isolate process-global resource limits from all other tests.
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    let mut old = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: old is a valid writable rlimit; this is an isolated child process.
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut old) }, 0);
    let limit = libc::rlimit {
        rlim_cur: 4,
        rlim_max: old.rlim_max,
    };
    assert_ne!(
        // SAFETY: SIG_IGN is a valid signal disposition, isolated to this subprocess.
        unsafe { libc::signal(libc::SIGXFSZ, libc::SIG_IGN) },
        libc::SIG_ERR
    );
    // SAFETY: limit points to an initialized rlimit and preserves the hard limit.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &limit) }, 0);
    let result = db.write_blob(b"partial write fault");
    // SAFETY: restore the saved valid limit before further test activity.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &old) }, 0);
    assert!(matches!(result, Err(StorageError::Io(_))));
    assert_eq!(fs::read_dir(db.root().join("blobs")).unwrap().count(), 0);
    let hash = db.write_blob(b"partial write fault").unwrap();
    assert_eq!(db.read_blob(&hash).unwrap(), b"partial write fault");
}
