//! Catalog-only command. No application startup, selection or store mutation.
use std::io::Write;
use std::os::fd::{AsFd, AsRawFd};
use std::process::ExitCode;

// No detached blocking threads: output itself must remain cancellable. Shared
// descriptor flags are restored on every exit. Regular-file redirects also work.
struct Output {
    file: std::fs::File,
    flags: libc::c_int,
}

impl Output {
    fn new(fd: impl AsFd) -> std::io::Result<Self> {
        let file = std::fs::File::from(fd.as_fd().try_clone_to_owned()?);
        // SAFETY: the owned file pins this valid descriptor for both calls.
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the owned file still pins the descriptor; only status flags change.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { file, flags })
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // SAFETY: this owner still holds the descriptor, restoring only flags.
        unsafe {
            libc::fcntl(self.file.as_raw_fd(), libc::F_SETFL, self.flags);
        }
    }
}

pub async fn run(data: Option<&std::path::Path>) -> ExitCode {
    let (mut out, mut err) = match (
        Output::new(std::io::stdout()),
        Output::new(std::io::stderr()),
    ) {
        (Ok(out), Ok(err)) => (out, err),
        _ => return ExitCode::from(1),
    };
    let project = match std::env::current_dir() {
        Ok(project) => project,
        Err(_) => {
            let _ = err
                .file
                .write_all(b"error: catalog location unavailable; review current directory\n");
            return ExitCode::from(1);
        }
    };
    // One cancellation scope spans discovery, diagnostics and output. Dropping
    // it closes the owned GET and releases output without an orphan task.
    tokio::select! {
        exit = async {
            match oc_adapters::composition::load_catalog_cached(&project, data).await {
                Err(diagnostic) => {
                    let _ = write_bytes(&mut err.file, format!("error: {diagnostic}\n").as_bytes()).await;
                    ExitCode::from(1)
                }
                Ok(listing) => emit(listing, &mut out.file, &mut err.file).await,
            }
        } => exit,
        _ = tokio::signal::ctrl_c() => {
            let _ = err.file.write_all(b"error: catalog cancelled\n");
            ExitCode::from(130)
        }
    }
}

async fn write_bytes(out: &mut impl Write, mut bytes: &[u8]) -> std::io::Result<()> {
    while !bytes.is_empty() {
        match out.write(bytes) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                tokio::task::yield_now().await;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

async fn emit(
    listing: oc_adapters::composition::CatalogListing,
    out: &mut impl Write,
    err: &mut impl Write,
) -> ExitCode {
    for diagnostic in &listing.diagnostics {
        let level = if listing.complete {
            "warning"
        } else {
            "error: incomplete catalog"
        };
        if write_bytes(err, format!("{level}: {diagnostic}\n").as_bytes())
            .await
            .is_err()
        {
            return ExitCode::from(1);
        }
    }
    for (index, reference) in listing.references.iter().enumerate() {
        if write_bytes(out, reference.as_bytes()).await.is_err()
            || write_bytes(out, b"\n").await.is_err()
        {
            let _ = err.write_all(b"error: catalog output failed\n");
            return ExitCode::from(1);
        }
        if index % 64 == 0 {
            tokio::task::yield_now().await;
        }
    }
    if out.flush().is_err() || err.flush().is_err() {
        return ExitCode::from(1);
    }
    if listing.complete {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests;
