//! Crash only the owner of an isolated fixture, never a recycled or foreign PID.
use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::Path,
};

pub struct NativeOwner {
    pub pid: u32,
    descriptor: OwnedFd,
}
impl NativeOwner {
    pub fn of_fixture(data: &Path) -> Self {
        let bytes =
            std::fs::read(data.join("native-owner.json")).expect("native owner registration");
        assert!(bytes.len() <= 4096);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let pid = u32::try_from(value["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 1);
        assert_eq!(
            std::fs::read_link(format!("/proc/{pid}/exe")).unwrap(),
            Path::new(env!("CARGO_BIN_EXE_oc2"))
        );
        // SAFETY: pidfd_open returns a fresh descriptor and sends no signal.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        assert!(fd >= 0);
        Self {
            pid,
            // SAFETY: the successful syscall returned this newly owned descriptor.
            descriptor: unsafe { OwnedFd::from_raw_fd(fd as i32) },
        }
    }
    pub fn kill(&self) -> std::io::Result<()> {
        // SAFETY: the pidfd pins the verified fixture process, not a reusable PID.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.descriptor.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
impl Drop for NativeOwner {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
