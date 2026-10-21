//! Filesystem registration is only a discovery hint. Both endpoints also admit
//! the actual socket peer's UID; only the Db election winner publishes/removes it.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::net::{UnixListener, UnixStream};

pub(super) const VERSION: u32 = 6;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Registration {
    pub version: u32,
    pub uid: u32,
    pub pid: u32,
    pub instance: String,
    pub device: u64,
    pub inode: u64,
    pub socket: PathBuf,
}

pub(super) fn uid() -> u32 {
    // SAFETY: getuid has no failure mode and touches no Rust memory.
    unsafe { libc::getuid() }
}
#[derive(Debug)]
struct Refused;
impl std::fmt::Display for Refused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("native endpoint admission refused")
    }
}
impl std::error::Error for Refused {}

fn refused() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, Refused)
}

pub(super) fn diagnostic(root: &Path, error: io::Error) -> crate::application::SpawnDiagnostic {
    if error.get_ref().is_some_and(|error| error.is::<Refused>()) {
        super::issue(oc_core::queries::ServiceCode::UnsafeDataRoot)
    } else {
        crate::application::SpawnDiagnostic {
            category: crate::application::SpawnFailure::DataRootUnavailable,
            diagnostic: crate::application::storage_diagnostic(
                root,
                &crate::storage::StorageError::Io(error),
            ),
        }
    }
}

fn private_dir(path: &Path) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.uid() != uid() || meta.mode() & 0o077 != 0 {
        return Err(refused());
    }
    Ok(())
}

pub(super) fn root(path: &Path) -> io::Result<PathBuf> {
    crate::storage::prepare_root(path).map_err(|error| match error {
        crate::storage::StorageError::Io(error) => error,
        _ => refused(),
    })?;
    // Do not canonicalize before storage's final-component no-follow check.
    fs::canonicalize(path)
}

/// Discovery must not create storage before the launching project's local
/// configuration has passed admission in the owner process.
pub(super) fn candidate(path: &Path) -> io::Result<PathBuf> {
    crate::storage::validate_root(path).map_err(|_| refused())?;
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && meta.uid() == uid() => fs::canonicalize(path),
        Ok(_) => Err(refused()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut ancestor = path;
            let mut missing = Vec::new();
            while !ancestor.try_exists()? {
                missing.push(ancestor.file_name().ok_or_else(refused)?);
                ancestor = ancestor.parent().ok_or_else(refused)?;
            }
            let mut canonical = fs::canonicalize(ancestor)?;
            for component in missing.into_iter().rev() {
                canonical.push(component);
            }
            Ok(canonical)
        }
        Err(error) => Err(error),
    }
}

fn registry(root: &Path) -> PathBuf {
    root.join("native-owner.json")
}
fn socket_parent(root: &Path) -> io::Result<PathBuf> {
    let local = root.join("native");
    let parent = if local.as_os_str().len() + 56 < 104 {
        local
    } else {
        PathBuf::from(format!("/tmp/oc2-{}", uid()))
    };
    private_dir(&parent)?;
    Ok(parent)
}
fn stem(root: &Path) -> String {
    let digest = Sha256::digest(root.as_os_str().as_encoded_bytes());
    digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn admit_socket_path(root: &Path, path: &Path) -> io::Result<()> {
    if path.parent() != Some(socket_parent(root)?.as_path()) {
        return Err(refused());
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(refused)?;
    let prefix = format!("{}-", stem(root));
    if !name.starts_with(&prefix) || !name.ends_with(".sock") || name.len() > 56 {
        return Err(refused());
    }
    Ok(())
}
fn admit_socket(path: &Path) -> io::Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_socket() || meta.uid() != uid() || meta.mode() & 0o077 != 0 {
        return Err(refused());
    }
    Ok(meta)
}

pub(super) fn read(root: &Path) -> io::Result<Option<Registration>> {
    let parent = match fs::symlink_metadata(root) {
        Ok(meta) if meta.is_dir() && meta.uid() == uid() => meta,
        Ok(_) => return Err(refused()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if parent.mode() & 0o077 != 0 {
        // A fresh user directory may be secured by Db admission, but an
        // existing registration in a shared directory is never trusted/read.
        return match fs::symlink_metadata(registry(root)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            _ => Err(refused()),
        };
    }
    let file = match OpenOptions::new()
        .read(true)
        // Inspect the opened descriptor before reading. O_NOFOLLOW alone does
        // not stop a FIFO from blocking in open before the file-type check.
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(registry(root))
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != uid() || meta.mode() & 0o077 != 0 || meta.len() > 4096 {
        return Err(refused());
    }
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(refused());
    }
    let registration: Registration = serde_json::from_slice(&bytes).map_err(|_| refused())?;
    let identity = fs::metadata(root)?;
    if registration.uid != uid()
        || registration.device != identity.dev()
        || registration.inode != identity.ino()
        || registration.instance.len() != 32
        || !registration
            .instance
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(refused());
    }
    admit_socket_path(root, &registration.socket)?;
    Ok(Some(registration))
}

pub(super) async fn connect(root: &Path, registration: &Registration) -> io::Result<UnixStream> {
    admit_socket_path(root, &registration.socket)?;
    admit_socket(&registration.socket)?;
    let stream = UnixStream::connect(&registration.socket).await?;
    let peer = stream.peer_cred()?;
    if peer.uid() != uid()
        || peer.pid().and_then(|pid| u32::try_from(pid).ok()) != Some(registration.pid)
    {
        return Err(refused());
    }
    Ok(stream)
}

pub(super) struct Endpoint {
    pub listener: UnixListener,
    pub registration: Registration,
    root: PathBuf,
    socket_inode: u64,
}
impl Endpoint {
    /// Caller already owns the actual Db. No pre-lock/unlock/reopen election.
    pub(super) fn bind(root: &Path) -> io::Result<Self> {
        if let Some(previous) = read(root)? {
            match admit_socket(&previous.socket) {
                Ok(_) => fs::remove_file(&previous.socket)?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let mut nonce = [0_u8; 16];
        File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        let instance: String = nonce.iter().map(|byte| format!("{byte:02x}")).collect();
        let socket =
            socket_parent(root)?.join(format!("{}-{}.sock", stem(root), std::process::id()));
        let listener = UnixListener::bind(&socket)?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        let socket_inode = admit_socket(&socket)?.ino();
        let meta = fs::metadata(root)?;
        let registration = Registration {
            version: VERSION,
            uid: uid(),
            pid: std::process::id(),
            instance,
            device: meta.dev(),
            inode: meta.ino(),
            socket,
        };
        let temporary = root.join(format!(".native-owner-{}.json", registration.instance));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)?;
        let publication = (|| {
            file.write_all(&serde_json::to_vec(&registration).map_err(|_| refused())?)?;
            file.sync_all()?;
            fs::rename(&temporary, registry(root))
        })();
        if publication.is_err() {
            let _ = fs::remove_file(&temporary);
            let _ = fs::remove_file(&registration.socket);
        }
        publication?;
        Ok(Self {
            listener,
            registration,
            root: root.to_path_buf(),
            socket_inode,
        })
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        if read(&self.root)
            .ok()
            .flatten()
            .is_some_and(|current| current.instance == self.registration.instance)
        {
            let _ = fs::remove_file(registry(&self.root));
        }
        if admit_socket(&self.registration.socket).is_ok_and(|meta| meta.ino() == self.socket_inode)
        {
            let _ = fs::remove_file(&self.registration.socket);
        }
    }
}

#[cfg(test)]
mod tests;
