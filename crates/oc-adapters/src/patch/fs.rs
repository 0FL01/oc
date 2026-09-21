//! Linux handle-relative patch I/O. Every descendant directory is opened
//! O_NOFOLLOW; names used by mutation syscalls are single components.
use std::ffi::{CString, OsStr};
use std::fs::{File, Metadata};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};

pub(super) struct Root(File);

pub(super) struct Entry {
    dir: File,
    name: CString,
}

pub(super) struct Snapshot {
    pub bytes: Vec<u8>,
    pub mode: u32,
    identity: (u64, u64, i64, i64, i64, i64),
}

fn identity(meta: &Metadata) -> (u64, u64, i64, i64, i64, i64) {
    (
        meta.dev(),
        meta.ino(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
    )
}

fn name(value: &OsStr) -> io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

fn open(dir: &File, name: &CString, flags: i32, mode: u32) -> io::Result<File> {
    // SAFETY: borrowed fd and NUL-terminated name stay valid for openat.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            mode,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

impl Root {
    pub fn new(root: &Path) -> io::Result<Self> {
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root)
            .map(Self)
    }

    /// With create=false, an absent ancestor is returned as NotFound, but
    /// symlink/non-directory ancestors are always errors. No path-based reopen.
    pub fn entry(&self, rel: &Path, create: bool) -> io::Result<Entry> {
        let mut parts = rel.components().peekable();
        let mut dir = self.0.try_clone()?;
        while let Some(part) = parts.next() {
            let Component::Normal(part) = part else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            let component = name(part)?;
            if parts.peek().is_none() {
                return Ok(Entry {
                    dir,
                    name: component,
                });
            }
            let next = open(&dir, &component, libc::O_RDONLY | libc::O_DIRECTORY, 0);
            dir = match next {
                Ok(next) => next,
                Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                    // SAFETY: valid directory fd and single NUL-terminated component.
                    let result =
                        unsafe { libc::mkdirat(dir.as_raw_fd(), component.as_ptr(), 0o755) };
                    if result != 0
                        && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists
                    {
                        return Err(io::Error::last_os_error());
                    }
                    let next = open(&dir, &component, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
                    dir.sync_all()?;
                    next
                }
                Err(error) => return Err(error),
            };
        }
        Err(io::ErrorKind::InvalidInput.into())
    }

    pub fn snapshot(&self, rel: &Path) -> io::Result<Snapshot> {
        self.entry(rel, false)?.snapshot()
    }

    /// Check the existing destination parent (or first missing ancestor's
    /// parent) before any plan writes. Later permission changes remain I/O
    /// failures; this is preflight, not a substitute for syscall enforcement.
    pub fn writable_parent(&self, rel: &Path) -> io::Result<()> {
        let mut dir = self.0.try_clone()?;
        for part in rel
            .parent()
            .ok_or(io::ErrorKind::InvalidInput)?
            .components()
        {
            let Component::Normal(part) = part else {
                return Err(io::ErrorKind::InvalidInput.into());
            };
            match open(&dir, &name(part)?, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
                Ok(next) => dir = next,
                Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                Err(error) => return Err(error),
            }
        }
        // SAFETY: valid directory fd and constant NUL-terminated relative name.
        let result = unsafe {
            libc::faccessat(
                dir.as_raw_fd(),
                c".".as_ptr(),
                libc::W_OK | libc::X_OK,
                libc::AT_EACCESS,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn absent(&self, rel: &Path) -> io::Result<bool> {
        match self.entry(rel, false) {
            Ok(entry) => entry.absent(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(error),
        }
    }
}

impl Entry {
    pub fn absent(&self) -> io::Result<bool> {
        // O_PATH opens the entry itself with NOFOLLOW: a dangling symlink is
        // occupied too, never an absent add/move target.
        match open(&self.dir, &self.name, libc::O_PATH, 0) {
            Ok(_) => Ok(false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(error),
        }
    }

    pub fn snapshot(&self) -> io::Result<Snapshot> {
        let mut file = open(&self.dir, &self.name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if meta.len() > super::FILE_BYTES_CAP as u64 {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(super::FILE_BYTES_CAP as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > super::FILE_BYTES_CAP {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        if identity(&meta) != identity(&file.metadata()?) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Snapshot {
            bytes,
            mode: meta.mode() & 0o777,
            identity: identity(&meta),
        })
    }

    pub fn unchanged(&self, before: &Snapshot) -> io::Result<bool> {
        let now = self.snapshot()?;
        Ok(now.identity == before.identity && now.mode == before.mode && now.bytes == before.bytes)
    }

    /// Exclusively create a fresh inode. getrandom supplies names; O_EXCL, not
    /// randomness, is the safety boundary against preplanted entries.
    pub fn stage(&self, bytes: &[u8], mode: Option<u32>) -> io::Result<Staged<'_>> {
        let mut random = [0u8; 16];
        // SAFETY: random is a writable buffer of the supplied size.
        let count = unsafe { libc::getrandom(random.as_mut_ptr().cast(), random.len(), 0) };
        if count != random.len() as isize {
            return Err(io::Error::other("temporary name unavailable"));
        }
        let tmp =
            CString::new(format!(".oc-patch-{:032x}", u128::from_ne_bytes(random))).expect("hex");
        let mut file = open(
            &self.dir,
            &tmp,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            0o600,
        )?;
        let staged = Staged {
            entry: self,
            name: tmp,
        };
        file.write_all(bytes)?;
        if let Some(mode) = mode {
            file.set_permissions(std::fs::Permissions::from_mode(mode))?;
        }
        file.sync_all()?;
        Ok(staged)
    }

    pub fn remove(&self) -> io::Result<()> {
        // SAFETY: valid fd and single NUL-terminated name, no directory removal.
        let result = unsafe { libc::unlinkat(self.dir.as_raw_fd(), self.name.as_ptr(), 0) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn move_to(&self, target: &Self) -> io::Result<()> {
        rename(
            &self.dir,
            &self.name,
            &target.dir,
            &target.name,
            libc::RENAME_NOREPLACE,
        )
    }

    pub fn sync(&self) -> io::Result<()> {
        self.dir.sync_all()
    }
}

pub(super) struct Staged<'a> {
    entry: &'a Entry,
    name: CString,
}

impl Staged<'_> {
    pub fn commit(&self, no_replace: bool) -> io::Result<()> {
        rename(
            &self.entry.dir,
            &self.name,
            &self.entry.dir,
            &self.entry.name,
            if no_replace {
                libc::RENAME_NOREPLACE
            } else {
                0
            },
        )
    }
}

impl Drop for Staged<'_> {
    fn drop(&mut self) {
        // SAFETY: this is our exclusively-created name in the pinned directory.
        // After rename it is absent. Never use a pathname to a parent here.
        unsafe { libc::unlinkat(self.entry.dir.as_raw_fd(), self.name.as_ptr(), 0) };
    }
}

fn rename(from: &File, name: &CString, to: &File, target: &CString, flags: u32) -> io::Result<()> {
    // SAFETY: both owned fds and single NUL-terminated components are valid.
    let result = unsafe {
        libc::renameat2(
            from.as_raw_fd(),
            name.as_ptr(),
            to.as_raw_fd(),
            target.as_ptr(),
            flags,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn aud04_pinned_parent_handle_never_follows_replacement_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(project.join("parent")).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("file"), b"sentinel").unwrap();
        let root = super::Root::new(&project).unwrap();
        let entry = root
            .entry(std::path::Path::new("parent/file"), false)
            .unwrap();
        let staged = entry.stage(b"safe", None).unwrap();
        std::fs::rename(project.join("parent"), project.join("original")).unwrap();
        std::os::unix::fs::symlink(&outside, project.join("parent")).unwrap();
        staged.commit(true).unwrap();
        entry.sync().unwrap();
        assert_eq!(std::fs::read(outside.join("file")).unwrap(), b"sentinel");
        assert_eq!(
            std::fs::read(project.join("original/file")).unwrap(),
            b"safe"
        );
    }
}
