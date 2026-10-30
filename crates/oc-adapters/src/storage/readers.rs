//! Pooled reader connections: display reads that do not wait for the writer.
//!
//! Each read runs in one WAL read transaction, so it sees one snapshot of
//! every transaction committed before it began (durable or not, like reads on
//! the writer connection) and never an uncommitted one. Only pure reads that
//! take no file lease and decide no irreversible effect belong here: a reader
//! can observe a commit whose writer has not published it yet, which `fence`
//! does not cover. Reads inside a write transaction keep using that
//! transaction.
use super::{StorageError, configure_connection, writer};
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::sync::{Condvar, Mutex};

/// Connections open lazily, so an idle owner keeps only the writer.
const READERS: usize = 4;

pub(crate) struct Readers {
    path: PathBuf,
    idle: Mutex<Idle>,
    freed: Condvar,
    #[cfg(test)]
    traced: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct Idle {
    connections: Vec<Connection>,
    opened: usize,
}

/// Returns its connection to the pool, also when the read panics.
struct Lease<'a> {
    readers: &'a Readers,
    conn: Option<Connection>,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        let mut idle = self.readers.idle.lock().expect("reader pool");
        match self.conn.take() {
            Some(conn) => idle.connections.push(conn),
            None => idle.opened -= 1,
        }
        self.readers.freed.notify_one();
    }
}

impl Readers {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            idle: Mutex::default(),
            freed: Condvar::new(),
            #[cfg(test)]
            traced: Default::default(),
        }
    }

    /// Run `read` in one read transaction on a pooled connection.
    pub(crate) fn read<T>(
        &self,
        read: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let lease = self.checkout()?;
        let conn = lease.conn.as_ref().expect("leased connection");
        // Dropping the transaction rolls it back, which ends the snapshot.
        let tx = conn.unchecked_transaction()?;
        read(&tx)
    }

    fn checkout(&self) -> Result<Lease<'_>, StorageError> {
        let mut idle = self.idle.lock().expect("reader pool");
        loop {
            if let Some(conn) = idle.connections.pop() {
                return Ok(Lease {
                    readers: self,
                    conn: Some(conn),
                });
            }
            if idle.opened < READERS {
                idle.opened += 1;
                drop(idle);
                let mut lease = Lease {
                    readers: self,
                    conn: None,
                };
                lease.conn = Some(self.open()?);
                return Ok(lease);
            }
            idle = writer::blocking(|| self.freed.wait(idle)).expect("reader pool");
        }
    }

    fn open(&self) -> Result<Connection, StorageError> {
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        configure_connection(&conn)?;
        // A pooled connection never writes, even through a misrouted helper.
        conn.pragma_update(None, "query_only", true)?;
        #[cfg(test)]
        if self.traced.load(std::sync::atomic::Ordering::SeqCst) {
            super::vm_steps::install(&conn);
        }
        Ok(conn)
    }

    /// Scaling tests count the statements of pooled connections too.
    #[cfg(test)]
    pub(crate) fn trace(&self) {
        self.traced.store(true, std::sync::atomic::Ordering::SeqCst);
        for conn in &self.idle.lock().expect("reader pool").connections {
            super::vm_steps::install(conn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Db;
    use oc_core::domain::SessionId;
    use std::sync::mpsc;
    use std::time::Duration;

    fn idle_seq(db: &Db) -> mpsc::Receiver<Option<u64>> {
        let db = db.shared_handle();
        let (sent, received) = mpsc::channel();
        std::thread::spawn(move || {
            let activity = db.session_activity(&[SessionId("s".into())]).unwrap();
            sent.send(activity[0].idle_seq).unwrap();
        });
        received
    }

    /// A display read neither waits behind a writer holding the connection nor
    /// sees its uncommitted change; a read that starts after the commit does.
    #[test]
    fn reads_do_not_wait_behind_the_writer() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.create_session("s").unwrap();
        let writer = db.conn.lock().unwrap();
        writer
            .execute_batch("BEGIN IMMEDIATE; UPDATE sessions SET idle_seq=7 WHERE id='s';")
            .unwrap();
        let before = idle_seq(&db).recv_timeout(Duration::from_secs(10));
        assert_eq!(
            before,
            Ok(None),
            "the read waited or saw an uncommitted write"
        );
        writer.execute_batch("COMMIT").unwrap();
        let after = idle_seq(&db).recv_timeout(Duration::from_secs(10));
        assert_eq!(after, Ok(Some(7)));
        drop(writer);
    }

    /// Reads beyond the pool size wait for a returned connection.
    #[test]
    fn a_read_waits_for_a_free_connection() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.create_session("s").unwrap();
        let mut leases: Vec<_> = (0..super::READERS)
            .map(|_| db.readers.checkout().unwrap())
            .collect();
        let read = idle_seq(&db);
        assert!(read.recv_timeout(Duration::from_millis(200)).is_err());
        leases.pop();
        assert_eq!(read.recv_timeout(Duration::from_secs(10)), Ok(None));
        drop(leases);
    }
}
