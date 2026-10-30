//! The owner's single SQLite connection with group commit outside its mutex.
//!
//! The connection commits into the WAL with `synchronous=NORMAL`, which writes
//! the frames without an fsync. When a holder that committed releases the
//! connection, it waits — after the mutex is released — until an `fdatasync`
//! of the WAL file that started after its commits completed has finished.
//! That is the acknowledged durability of `synchronous=FULL` in WAL mode, but
//! one fsync covers every completed commit before it: while one fsync runs,
//! other holders keep committing and share the next one, and readers never
//! wait for a disk flush. The WAL is sequential, so anything durable implies
//! every earlier commit is durable.
//!
//! The commit hook runs before SQLite writes the frames, so it only marks a
//! commit as started; a commit counts as completed when its holder releases
//! the connection. A WAL fsync failure is latched: no later waiter is
//! acknowledged after it (Linux reports a writeback error only once).
use rusqlite::Connection;
use std::fs::File;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LockResult, Mutex, MutexGuard, PoisonError, TryLockError};

pub(crate) struct Conn {
    conn: Mutex<Connection>,
    // Commits started (commit hook) and commits whose holder has released
    // the connection, i.e. whose frames are fully in the WAL file.
    started: Arc<AtomicU64>,
    completed: AtomicU64,
    sync: Mutex<Sync>,
    synced: Condvar,
    wal: File,
    #[cfg(test)]
    pub(crate) syncs: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    pub(crate) before_sync: Mutex<Option<Box<dyn Fn() + Send>>>,
    #[cfg(test)]
    pub(crate) fail_sync: std::sync::atomic::AtomicBool,
    // Test populations that do not measure durability skip the WAL fsync.
    #[cfg(test)]
    pub(crate) skip_sync: std::sync::atomic::AtomicBool,
}

struct Sync {
    durable: u64,
    running: bool,
    failed: bool,
}

pub(crate) struct Guard<'a> {
    owner: &'a Conn,
    conn: Option<MutexGuard<'a, Connection>>,
    start: u64,
}

impl Deref for Guard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("held connection")
    }
}

impl DerefMut for Guard<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("held connection")
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        // Every SQLite call of this holder has returned and the mutex excluded
        // other committers: its commits are complete in the WAL file. Publish
        // that before releasing, then wait for durability outside the mutex,
        // also while unwinding (a destructor may record an unknown effect).
        let end = self.owner.started.load(Ordering::SeqCst);
        self.owner.completed.store(end, Ordering::SeqCst);
        self.conn.take();
        if end > self.start && !self.owner.is_durable(end) {
            blocking(|| self.owner.durable(end));
        }
    }
}

impl Conn {
    /// `conn` is the open WAL database; `wal` is its `-wal` file.
    pub(crate) fn new(conn: Connection, wal: File) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let started = Arc::new(AtomicU64::new(0));
        let counter = started.clone();
        conn.commit_hook(Some(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            false
        }))?;
        Ok(Self {
            conn: Mutex::new(conn),
            started,
            completed: AtomicU64::new(0),
            sync: Mutex::new(Sync {
                durable: 0,
                running: false,
                failed: false,
            }),
            synced: Condvar::new(),
            wal,
            #[cfg(test)]
            syncs: std::sync::atomic::AtomicUsize::new(0),
            #[cfg(test)]
            before_sync: Mutex::new(None),
            #[cfg(test)]
            fail_sync: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            skip_sync: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Exclusive access; a holder that committed returns from its release
    /// only once those commits are durable.
    pub(crate) fn lock(&self) -> LockResult<Guard<'_>> {
        let guard = |conn| Guard {
            owner: self,
            conn: Some(conn),
            start: self.started.load(Ordering::SeqCst),
        };
        let locked = match self.conn.try_lock() {
            Ok(conn) => Ok(conn),
            Err(TryLockError::Poisoned(poisoned)) => Err(poisoned),
            Err(TryLockError::WouldBlock) => blocking(|| self.conn.lock()),
        };
        match locked {
            Ok(conn) => Ok(guard(conn)),
            Err(poisoned) => Err(PoisonError::new(guard(poisoned.into_inner()))),
        }
    }

    /// Make every completed commit durable before an irreversible action
    /// decided from database state (for example deleting a file whose row
    /// another holder removed but has not yet been acknowledged).
    pub(crate) fn fence(&self) {
        let target = self.completed.load(Ordering::SeqCst);
        if !self.is_durable(target) {
            blocking(|| self.durable(target));
        }
    }

    fn is_durable(&self, target: u64) -> bool {
        let sync = self.sync.lock().expect("wal sync");
        !sync.failed && sync.durable >= target
    }

    /// True while another holder owns the connection (tests only).
    #[cfg(test)]
    pub(crate) fn is_held(&self) -> bool {
        matches!(self.conn.try_lock(), Err(TryLockError::WouldBlock))
    }

    /// Wait until commit `target` is covered by a finished WAL fsync,
    /// running that fsync if none is in flight.
    fn durable(&self, target: u64) {
        let mut sync = self.sync.lock().expect("wal sync");
        loop {
            if sync.failed {
                drop(sync);
                fatal("an earlier WAL fsync failed");
            }
            #[cfg(test)]
            if self.skip_sync.load(Ordering::SeqCst) {
                sync.durable = sync.durable.max(target);
            }
            if sync.durable >= target {
                return;
            }
            if sync.running {
                sync = self.synced.wait(sync).expect("wal sync");
                continue;
            }
            sync.running = true;
            drop(sync);
            #[cfg(test)]
            if let Some(hook) = self.before_sync.lock().expect("sync hook").as_ref() {
                hook();
            }
            // Completed commits are fully in the WAL file before this fsync.
            let upto = self.completed.load(Ordering::SeqCst);
            #[cfg(test)]
            self.syncs.fetch_add(1, Ordering::SeqCst);
            #[cfg(test)]
            let result = if self.fail_sync.load(Ordering::SeqCst) {
                Err(std::io::Error::other("injected writeback failure"))
            } else {
                self.wal.sync_data()
            };
            #[cfg(not(test))]
            let result = self.wal.sync_data();
            sync = self.sync.lock().expect("wal sync");
            sync.running = false;
            match result {
                Ok(()) => sync.durable = sync.durable.max(upto),
                Err(_) => sync.failed = true,
            }
            self.synced.notify_all();
        }
    }
}

/// Waiting on the connection or a WAL fsync from a tokio worker of the
/// owner's small multi-thread runtime hands that worker's other tasks to the
/// runtime instead of stalling them; other contexts simply block.
pub(super) fn blocking<T>(wait: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(wait)
        }
        _ => wait(),
    }
}

/// A commit already returned but its durability is unknown: never acknowledge
/// it. A second panic while unwinding would abort anyway; say why first.
fn fatal(reason: &str) -> ! {
    let message =
        format!("storage durability lost: {reason}; committed writes may not survive power loss");
    if std::thread::panicking() {
        eprintln!("{message}");
        std::process::abort();
    }
    panic!("{message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn open(dir: &std::path::Path) -> Arc<Conn> {
        let conn = Connection::open(dir.join("w.sqlite")).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute_batch("CREATE TABLE t(v INTEGER UNIQUE)")
            .unwrap();
        let wal = File::open(dir.join("w.sqlite-wal")).unwrap();
        Arc::new(Conn::new(conn, wal).unwrap())
    }

    fn insert(conn: &Conn, v: i64) {
        conn.lock()
            .unwrap()
            .execute("INSERT INTO t VALUES(?1)", [v])
            .unwrap();
    }

    /// Writers that complete while an fsync is being prepared share it: eight
    /// acknowledged writes, one fsync.
    #[test]
    fn commits_completed_before_an_fsync_share_it() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path());
        let base = conn.started.load(Ordering::SeqCst);
        let first = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let gate = conn.clone();
        *conn.before_sync.lock().unwrap() = Some(Box::new(move || {
            // Hold the first fsync until the other seven writers committed.
            if first.swap(false, Ordering::SeqCst) {
                let deadline = Instant::now() + Duration::from_secs(10);
                while gate.completed.load(Ordering::SeqCst) < base + 8 {
                    assert!(Instant::now() < deadline, "writers did not commit");
                    std::thread::yield_now();
                }
            }
        }));
        let leader = {
            let conn = conn.clone();
            std::thread::spawn(move || insert(&conn, 0))
        };
        while conn.started.load(Ordering::SeqCst) == base {
            std::thread::yield_now();
        }
        let followers: Vec<_> = (1..8)
            .map(|v| {
                let conn = conn.clone();
                std::thread::spawn(move || insert(&conn, v))
            })
            .collect();
        leader.join().unwrap();
        for follower in followers {
            follower.join().unwrap();
        }
        assert_eq!(conn.syncs.load(Ordering::SeqCst), 1);
        assert_eq!(conn.sync.lock().unwrap().durable, base + 8);
    }

    /// Readers and holders that did not commit never wait for an fsync, and
    /// the connection is available to them while an fsync runs.
    #[test]
    fn reads_do_not_wait_for_a_running_fsync() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path());
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let entered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (gate, inside) = (release.clone(), entered.clone());
        *conn.before_sync.lock().unwrap() = Some(Box::new(move || {
            inside.store(true, Ordering::SeqCst);
            while !gate.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
        }));
        let writer = {
            let conn = conn.clone();
            std::thread::spawn(move || insert(&conn, 1))
        };
        while !entered.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        // The fsync is held: the committed row is readable and no read syncs.
        let count: i64 = conn
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(
            !writer.is_finished(),
            "a write is acknowledged only after fsync"
        );
        release.store(true, Ordering::SeqCst);
        writer.join().unwrap();
        assert_eq!(conn.syncs.load(Ordering::SeqCst), 1);
    }

    /// Only commits whose holder released the connection are covered: a
    /// commit started by a holder that still owns the connection may not be
    /// in the WAL file yet, so that holder runs its own later fsync.
    #[test]
    fn an_fsync_covers_released_commits_only() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path());
        let committed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let first = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (gate, once) = (committed.clone(), first.clone());
        *conn.before_sync.lock().unwrap() = Some(Box::new(move || {
            if once.swap(false, Ordering::SeqCst) {
                let deadline = Instant::now() + Duration::from_secs(10);
                while !gate.load(Ordering::SeqCst) {
                    assert!(Instant::now() < deadline, "holder did not commit");
                    std::thread::yield_now();
                }
            }
        }));
        let writer = {
            let conn = conn.clone();
            std::thread::spawn(move || insert(&conn, 0))
        };
        while conn.completed.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }
        let holder = conn.lock().unwrap();
        holder.execute("INSERT INTO t VALUES(1)", []).unwrap();
        committed.store(true, Ordering::SeqCst);
        writer.join().unwrap();
        assert_eq!(conn.syncs.load(Ordering::SeqCst), 1);
        assert_eq!(conn.sync.lock().unwrap().durable, 1);
        drop(holder);
        assert_eq!(
            conn.syncs.load(Ordering::SeqCst),
            2,
            "the holder ran its own fsync"
        );
        assert_eq!(conn.sync.lock().unwrap().durable, 2);
    }

    /// One failed WAL fsync is never followed by an acknowledgement, even if
    /// a later fsync would report success.
    #[test]
    fn a_failed_fsync_is_latched() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path());
        conn.fail_sync.store(true, Ordering::SeqCst);
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| insert(&conn, 1)));
        assert!(failed.is_err());
        conn.fail_sync.store(false, Ordering::SeqCst);
        let later = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| insert(&conn, 2)));
        assert!(later.is_err(), "no acknowledgement after a lost writeback");
        assert_eq!(conn.syncs.load(Ordering::SeqCst), 1);
    }

    /// A task waiting for the connection on a tokio worker does not stall the
    /// other tasks of that worker: with one worker thread, a timer task still
    /// completes while the connection is held elsewhere.
    #[test]
    fn waiting_for_the_connection_does_not_stall_the_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let held = conn.lock().unwrap();
        let ticked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        runtime.block_on(async {
            let waiter = {
                let conn = conn.clone();
                tokio::spawn(async move {
                    insert(&conn, 1);
                })
            };
            tokio::task::yield_now().await;
            let tick = {
                let ticked = ticked.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    ticked.store(true, Ordering::SeqCst);
                })
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            while !ticked.load(Ordering::SeqCst) {
                assert!(
                    Instant::now() < deadline,
                    "the worker stalled behind the connection"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            drop(held);
            waiter.await.unwrap();
            tick.await.unwrap();
        });
        assert_eq!(conn.syncs.load(Ordering::SeqCst), 1);
    }
}
