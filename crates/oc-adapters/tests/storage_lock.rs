//! A concurrent fork must not retain ownership after the parent drops its Db.

use std::io::{self, Read};
use std::net::Shutdown;
use std::os::fd::AsRawFd as _;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt as _;
use std::process::Command;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use oc_adapters::storage::{Db, StorageError};

struct PausedSpawn {
    socket: UnixStream,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl PausedSpawn {
    fn start() -> Self {
        let (socket, child_socket) = UnixStream::pair().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let worker = std::thread::spawn(move || {
            let fd = child_socket.as_raw_fd();
            let mut command = Command::new("/bin/true");
            // SAFETY: the post-fork closure uses only stack values and
            // async-signal-safe libc calls: no allocation, locks or unwinding.
            // The socket stays open until spawn returns. poll bounds the pause
            // even if the parent never signals; MSG_NOSIGNAL avoids SIGPIPE.
            #[allow(clippy::multiple_unsafe_ops_per_block)]
            unsafe {
                command.pre_exec(move || {
                    let byte = 1u8;
                    if libc::send(fd, (&byte as *const u8).cast(), 1, libc::MSG_NOSIGNAL) != 1 {
                        libc::_exit(91);
                    }
                    let mut event = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    if libc::poll(&mut event, 1, 10_000) != 1 {
                        libc::_exit(92);
                    }
                    Ok(())
                });
            }
            let spawned = command.spawn();
            drop(child_socket);
            let mut child = spawned?;
            // Do not rely on the executed fixture exiting to bound cleanup.
            let _ = child.kill();
            child.wait()?;
            Ok(())
        });
        Self {
            socket,
            worker: Some(worker),
        }
    }

    fn reap(&mut self) -> io::Result<()> {
        // shutdown also affects the parent's socket descriptor inherited by
        // the child; merely closing our descriptor would not wake its poll.
        let _ = self.socket.shutdown(Shutdown::Both);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("spawn worker panicked"))??;
        }
        Ok(())
    }
}

impl Drop for PausedSpawn {
    fn drop(&mut self) {
        // Assertions may unwind while Command::spawn is waiting for exec.
        let _ = self.reap();
    }
}

#[test]
fn dropped_db_releases_root_while_concurrent_child_is_before_exec() {
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_session("persisted").unwrap();
    let pause_started = Instant::now();
    let mut paused = PausedSpawn::start();
    let mut ready = [0u8];
    paused.socket.read_exact(&mut ready).unwrap();
    assert_eq!(ready, [1], "child reached pre_exec and inherited the lock");

    assert!(matches!(
        Db::open(data.path()),
        Err(StorageError::DataRootBusy)
    ));
    drop(db);
    let reopened = Db::open(data.path()).expect("parent drop must release the inherited flock");
    assert!(
        pause_started.elapsed() < Duration::from_secs(5),
        "reopen must precede the child's ten-second pre_exec deadline"
    );
    assert_eq!(reopened.list_sessions().unwrap(), ["persisted"]);
    paused.reap().expect("release and reap pre_exec child");
    assert!(matches!(
        Db::open(data.path()),
        Err(StorageError::DataRootBusy)
    ));
    drop(reopened);
    Db::open(data.path()).expect("new owner also releases its lock");
}
