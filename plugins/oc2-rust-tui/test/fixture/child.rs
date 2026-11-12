//! Lifecycle-only Rust child, linked to the existing oc2 TerminalGuard.
use std::io::{BufRead as _, Write as _};
use std::os::fd::{AsRawFd as _, FromRawFd as _};
use std::sync::atomic::{AtomicBool, Ordering};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

fn record(event: &str) {
    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(std::env::var_os("OC2_LIFECYCLE_LOG").expect("fixture log"))
        .expect("open fixture log");
    log.write_all(format!("{{\"event\":\"{event}\"}}\n").as_bytes())
        .expect("write fixture log");
}

fn main() {
    // SAFETY: these fixed inherited descriptors are owned by this private role;
    // the handlers touch only a lock-free atomic and are installed before tty use.
    let (input, mut output) = unsafe {
        libc::signal(libc::SIGTERM, stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, stop as *const () as libc::sighandler_t);
        (std::fs::File::from_raw_fd(3), std::fs::File::from_raw_fd(4))
    };
    let mode = std::env::var("OC2_LIFECYCLE_MODE").unwrap_or_default();
    if mode != "silent" {
        let version = if mode == "bad-hello" { 2 } else { 1 };
        if writeln!(
            output,
            "{{\"type\":\"hello\",\"version\":{version},\"pid\":{}}}",
            std::process::id()
        )
        .is_err()
        {
            return;
        }
    }
    let mut frame = String::new();
    let mut reader = std::io::BufReader::new(input);
    if reader.read_line(&mut frame).unwrap_or(0) == 0
        || serde_json::from_str::<serde_json::Value>(&frame).ok()
            != Some(serde_json::json!({ "type": "activate", "version": 1 }))
    {
        return;
    }
    let terminal = oc_tui::terminal::enter().expect("terminal");
    eprint!("\x1b[?25lOC2 RUST LIFECYCLE CHILD (q=return, c=crash)\r\n");
    record("child.ready");
    let mut input = std::io::stdin();
    while !STOP.load(Ordering::Relaxed) || mode == "stuck" {
        let mut poll = [
            libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: reader.get_ref().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: poll borrows two initialized pollfds for this call only.
        if unsafe { libc::poll(poll.as_mut_ptr(), 2, 50) } > 0 {
            if poll[1].revents != 0 {
                break;
            }
            if poll[0].revents == 0 {
                continue;
            }
            let mut key = [0];
            if std::io::Read::read(&mut input, &mut key).unwrap_or(0) == 0 || key[0] == b'q' {
                break;
            }
            if key[0] == b'c' {
                std::process::exit(7);
            }
        }
    }
    if mode == "shutdown-crash" {
        // The same non-unwinding failure as the ordinary crash probe, but
        // concurrently with the parent's destroy-first shutdown. No late tty
        // writes are needed to expose the missing final renderer-owned reset.
        record("child.crash");
        std::process::exit(7);
    }
    if let Ok(delay) = std::env::var("OC2_LIFECYCLE_CLEANUP_DELAY") {
        std::thread::sleep(std::time::Duration::from_secs_f64(
            delay.parse().expect("delay"),
        ));
    }
    drop(terminal);
    record("child.restore");
}
