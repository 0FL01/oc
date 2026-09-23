//! Linux descriptor-relative opens shared by config and definition admission.

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

/// Open the resolved relative target under a pinned admitted directory.
/// Files and directories are nonblocking and no-follow at the final component;
/// intermediate replacements cannot escape the root, including on a race.
pub(crate) fn open_beneath(dir: &File, relative: &Path, flags: i32) -> io::Result<File> {
    #[repr(C)]
    struct OpenHow {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let path = CString::new(relative.as_os_str().as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let how = OpenHow {
        flags: (flags | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOFOLLOW) as u64,
        mode: 0,
        // RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS (linux/openat2.h).
        resolve: 0x08 | 0x02,
    };
    // SAFETY: syscall borrows a live root fd, a NUL-terminated path and a
    // correctly sized C-compatible open_how; a successful fd is newly owned.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            dir.as_raw_fd(),
            path.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat2 returned a unique owned descriptor on success.
    Ok(unsafe { File::from_raw_fd(fd as i32) })
}
