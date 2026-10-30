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
//!
//! The checkpoint (copying the WAL into the database file, with its fsyncs)
//! keeps SQLite's automatic threshold and its place under the mutex, which
//! lets the next write restart the WAL from its start (overwriting an
//! allocated file keeps `fdatasync` cheap). The committing holder runs it on
//! release instead of inside its COMMIT, as a handed-off wait like the others.
use rusqlite::Connection;
use std::cell::Cell;
use std::fs::File;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, LockResult, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::Instant;

/// SQLite's automatic checkpoint threshold.
const CHECKPOINT_PAGES: i32 = 1000;

thread_local! {
    // WAL size after the last commit on this thread: the holder's own.
    static WAL_PAGES: Cell<i32> = const { Cell::new(0) };
}

fn wal_pages(_: &rusqlite::hooks::Wal, pages: std::ffi::c_int) -> rusqlite::Result<()> {
    WAL_PAGES.set(pages);
    Ok(())
}

pub(crate) struct Conn {
    conn: Mutex<Connection>,
    // Commits started (commit hook) and commits whose holder has released
    // the connection, i.e. whose frames are fully in the WAL file.
    started: Arc<AtomicU64>,
    completed: AtomicU64,
    sync: Mutex<Sync>,
    synced: Condvar,
    wal: File,
    metrics: Metrics,
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

/// Always-on counters of the writer: `Db::storage_metrics` snapshots them,
/// and `OC_STORAGE_METRICS=1` prints them when the connection closes.
#[derive(Default)]
struct Metrics {
    locks: AtomicU64,
    contended: AtomicU64,
    wait_ns: AtomicU64,
    max_wait_ns: AtomicU64,
    hold_ns: AtomicU64,
    max_hold_ns: AtomicU64,
    runtime_holds: AtomicU64,
    max_runtime_hold_ns: AtomicU64,
    durable_waits: AtomicU64,
    durable_wait_ns: AtomicU64,
    max_durable_wait_ns: AtomicU64,
    fsyncs: AtomicU64,
    fsync_ns: AtomicU64,
    stalled_waits: AtomicU64,
    checkpoints: AtomicU64,
    checkpoint_ns: AtomicU64,
    max_checkpoint_ns: AtomicU64,
    wal_pages: AtomicU64,
    wal_frames: AtomicU64,
}

/// Snapshot of the writer counters; times in microseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageMetrics {
    pub locks: u64,
    /// Lock requests that found the connection held and waited.
    pub contended: u64,
    pub wait_us: u64,
    pub max_wait_us: u64,
    pub hold_us: u64,
    pub max_hold_us: u64,
    /// Holds on a tokio runtime thread, which run SQL on that thread.
    pub runtime_holds: u64,
    pub max_runtime_hold_us: u64,
    pub commits: u64,
    /// Releases that waited for a WAL fsync covering their commits.
    pub durable_waits: u64,
    pub durable_wait_us: u64,
    pub max_durable_wait_us: u64,
    pub fsyncs: u64,
    pub fsync_us: u64,
    /// Pages committed into the WAL: the database's write volume.
    pub wal_frames: u64,
    /// WAL checkpoints run by releasing holders.
    pub checkpoints: u64,
    pub checkpoint_us: u64,
    pub max_checkpoint_us: u64,
    /// Waits on a runtime worker that could not hand its tasks off
    /// (a current-thread runtime); the owner's runtime has none.
    pub stalled_waits: u64,
}

impl std::fmt::Display for StorageMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "locks={} contended={} wait_us={} max_wait_us={} hold_us={} max_hold_us={} runtime_holds={} max_runtime_hold_us={} commits={} durable_waits={} durable_wait_us={} max_durable_wait_us={} fsyncs={} fsync_us={} wal_frames={} checkpoints={} checkpoint_us={} max_checkpoint_us={} stalled_waits={}",
            self.locks,
            self.contended,
            self.wait_us,
            self.max_wait_us,
            self.hold_us,
            self.max_hold_us,
            self.runtime_holds,
            self.max_runtime_hold_us,
            self.commits,
            self.durable_waits,
            self.durable_wait_us,
            self.max_durable_wait_us,
            self.fsyncs,
            self.fsync_us,
            self.wal_frames,
            self.checkpoints,
            self.checkpoint_us,
            self.max_checkpoint_us,
            self.stalled_waits,
        )
    }
}

fn add(total: &AtomicU64, max: &AtomicU64, since: Instant) {
    let ns = u64::try_from(since.elapsed().as_nanos()).unwrap_or(u64::MAX);
    total.fetch_add(ns, Ordering::Relaxed);
    max.fetch_max(ns, Ordering::Relaxed);
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
    acquired: Instant,
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
        // Holds count the holder's own SQL, not the handed-off checkpoint.
        let metrics = &self.owner.metrics;
        add(&metrics.hold_ns, &metrics.max_hold_ns, self.acquired);
        if tokio::runtime::Handle::try_current().is_ok() {
            metrics.runtime_holds.fetch_add(1, Ordering::Relaxed);
            let ns = u64::try_from(self.acquired.elapsed().as_nanos()).unwrap_or(u64::MAX);
            metrics.max_runtime_hold_ns.fetch_max(ns, Ordering::Relaxed);
        }
        let pages = WAL_PAGES.replace(0);
        if pages > 0 {
            let pages = pages as u64;
            let before = metrics.wal_pages.swap(pages, Ordering::Relaxed);
            // A smaller WAL after a checkpoint restarted it from the start.
            let frames = if pages >= before {
                pages - before
            } else {
                pages
            };
            metrics.wal_frames.fetch_add(frames, Ordering::Relaxed);
        }
        if pages >= CHECKPOINT_PAGES
            && let Some(conn) = self.conn.as_ref()
            && conn.is_autocommit()
        {
            let since = Instant::now();
            self.owner.count_stall();
            if blocking(|| conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE)")).is_ok() {
                metrics.checkpoints.fetch_add(1, Ordering::Relaxed);
                add(&metrics.checkpoint_ns, &metrics.max_checkpoint_ns, since);
            }
        }
        let end = self.owner.started.load(Ordering::SeqCst);
        self.owner.completed.store(end, Ordering::SeqCst);
        self.conn.take();
        if end > self.start && !self.owner.is_durable(end) {
            let since = Instant::now();
            self.owner.count_stall();
            blocking(|| self.owner.durable(end));
            metrics.durable_waits.fetch_add(1, Ordering::Relaxed);
            add(
                &metrics.durable_wait_ns,
                &metrics.max_durable_wait_ns,
                since,
            );
        }
    }
}

impl Conn {
    /// `conn` is the open WAL database; `wal` is its `-wal` file.
    pub(crate) fn new(conn: Connection, wal: File) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // Replaces SQLite's automatic checkpoint (itself a WAL hook).
        conn.wal_hook(Some(wal_pages));
        let metrics = Metrics::default();
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
            metrics,
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
            acquired: Instant::now(),
        };
        self.metrics.locks.fetch_add(1, Ordering::Relaxed);
        let locked = match self.conn.try_lock() {
            Ok(conn) => Ok(conn),
            Err(TryLockError::Poisoned(poisoned)) => Err(poisoned),
            Err(TryLockError::WouldBlock) => {
                let since = Instant::now();
                self.metrics.contended.fetch_add(1, Ordering::Relaxed);
                self.count_stall();
                let locked = blocking(|| self.conn.lock());
                add(&self.metrics.wait_ns, &self.metrics.max_wait_ns, since);
                locked
            }
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

    /// Commits started on this connection: unchanged means no write since.
    pub(crate) fn commits(&self) -> u64 {
        self.started.load(Ordering::SeqCst)
    }

    fn count_stall(&self) {
        if tokio::runtime::Handle::try_current().is_ok_and(|handle| {
            handle.runtime_flavor() != tokio::runtime::RuntimeFlavor::MultiThread
        }) {
            self.metrics.stalled_waits.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn metrics(&self) -> StorageMetrics {
        let m = &self.metrics;
        let us = |ns: &AtomicU64| ns.load(Ordering::Relaxed) / 1000;
        let n = |count: &AtomicU64| count.load(Ordering::Relaxed);
        StorageMetrics {
            locks: n(&m.locks),
            contended: n(&m.contended),
            wait_us: us(&m.wait_ns),
            max_wait_us: us(&m.max_wait_ns),
            hold_us: us(&m.hold_ns),
            max_hold_us: us(&m.max_hold_ns),
            runtime_holds: n(&m.runtime_holds),
            max_runtime_hold_us: us(&m.max_runtime_hold_ns),
            commits: self.started.load(Ordering::SeqCst),
            durable_waits: n(&m.durable_waits),
            durable_wait_us: us(&m.durable_wait_ns),
            max_durable_wait_us: us(&m.max_durable_wait_ns),
            fsyncs: n(&m.fsyncs),
            fsync_us: us(&m.fsync_ns),
            wal_frames: n(&m.wal_frames),
            checkpoints: n(&m.checkpoints),
            checkpoint_us: us(&m.checkpoint_ns),
            max_checkpoint_us: us(&m.max_checkpoint_ns),
            stalled_waits: n(&m.stalled_waits),
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
            let since = Instant::now();
            #[cfg(test)]
            let result = if self.fail_sync.load(Ordering::SeqCst) {
                Err(std::io::Error::other("injected writeback failure"))
            } else {
                self.wal.sync_data()
            };
            #[cfg(not(test))]
            let result = self.wal.sync_data();
            self.metrics.fsyncs.fetch_add(1, Ordering::Relaxed);
            let ns = u64::try_from(since.elapsed().as_nanos()).unwrap_or(u64::MAX);
            self.metrics.fsync_ns.fetch_add(ns, Ordering::Relaxed);
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

impl Drop for Conn {
    fn drop(&mut self) {
        if std::env::var_os("OC_STORAGE_METRICS").is_some() {
            eprintln!("oc storage metrics: {}", self.metrics());
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
        let metrics = conn.metrics();
        assert_eq!(
            (metrics.contended, metrics.stalled_waits, metrics.commits),
            (1, 0, 1)
        );
        assert_eq!((metrics.fsyncs, metrics.durable_waits), (1, 1));
        assert!(metrics.runtime_holds >= 1 && metrics.max_wait_us > 0);
    }

    /// The releasing holder checkpoints at SQLite's threshold, and the WAL
    /// restarts instead of growing.
    #[test]
    fn a_releasing_holder_checkpoints_the_wal() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path());
        conn.skip_sync.store(true, Ordering::SeqCst);
        for _ in 0..3000 {
            conn.lock()
                .unwrap()
                .execute_batch("INSERT INTO t VALUES(randomblob(4096))")
                .unwrap();
        }
        assert!(conn.metrics().checkpoints >= 2, "{}", conn.metrics());
        let wal = std::fs::metadata(dir.path().join("w.sqlite-wal"))
            .unwrap()
            .len();
        assert!(wal < 2 * 1100 * 4200, "the WAL grew to {wal} bytes");
    }

    /// A current-thread runtime cannot hand off: such waits (here for the
    /// connection, then for the fsync of the commit) are counted.
    #[test]
    fn waits_without_a_handoff_are_counted() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(dir.path());
        let held = conn.lock().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| runtime.block_on(async { insert(&conn, 1) }));
            while conn.metrics().contended == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
            drop(held);
        });
        assert_eq!(conn.metrics().stalled_waits, 2);
    }
}
