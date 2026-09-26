//! A PTY child must own its controlling terminal so TIOCSWINSZ delivers
//! SIGWINCH, just as a terminal emulator does for an interactive process.
use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn controlling_terminal(command: &mut Command) {
    let setup = || {
        // SAFETY: setsid has no pointer arguments and is async-signal-safe.
        if unsafe { libc::setsid() } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: Command has mapped stdin to the open slave; ioctl does not
        // take a pointer for TIOCSCTTY. No allocation/locks occur in this hook.
        if unsafe { libc::ioctl(0, libc::TIOCSCTTY, 0) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    };
    // SAFETY: setup performs only async-signal-safe operations after fork.
    unsafe {
        command.pre_exec(setup);
    }
}
