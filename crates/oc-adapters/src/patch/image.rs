//! Binary image creation reuses patch's pinned directories and no-replace commit.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct ImageDestination {
    entry: fs::Entry,
    path: PathBuf,
    lexical: PathBuf,
    files: Files,
}

fn destination(
    files: &Files,
    path: &str,
) -> Result<(Arc<fs::Root>, PathBuf, PathBuf, PathBuf), String> {
    let lexical = files.concrete_scope(path).map_err(|e| e.to_string())?;
    // An occupied leaf includes dangling symlinks; never follow one as an add target.
    match std::fs::symlink_metadata(&lexical) {
        Ok(_) => return Err("output_path already exists; choose a new filename".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("output_path metadata unavailable".into()),
    }
    let absolute = files.resolve_mutation(path).map_err(|e| e.to_string())?;
    let resolved = files.io_target(&absolute).map_err(|e| e.to_string())?;
    let (root, relative) = mutation_scope(files, &resolved, &mut BTreeMap::new())
        .map_err(|_| "output_path parent unavailable")?;
    root.writable_parent(&relative)
        .map_err(|_| "output_path parent is not writable")?;
    if !root
        .absent(&relative)
        .map_err(|_| "output_path unavailable")?
    {
        return Err("output_path already exists; choose a new filename".into());
    }
    Ok((root, relative, resolved, lexical))
}

impl ImageDestination {
    /// Read-only approval identity, repeated after Ask; no directory/file creation.
    pub(crate) fn approval_digest(files: &Files, path: &str) -> Result<String, String> {
        let (root, relative, resolved, _) = destination(files, path)?;
        let (device, inode) = root
            .authority_identity()
            .map_err(|_| "output root unavailable")?;
        let mut hash = Sha256::new();
        hash.update(device.to_le_bytes());
        hash.update(inode.to_le_bytes());
        hash.update(relative.as_os_str().as_encoded_bytes());
        hash.update(resolved.as_os_str().as_encoded_bytes());
        Ok(format!("{:x}", hash.finalize()))
    }

    /// Called only after path/boundary admission, before consuming remote quota.
    pub(crate) fn prepare(files: &Files, path: &str) -> Result<Self, String> {
        let (root, relative, resolved, lexical) = destination(files, path)?;
        let entry = root
            .entry(&relative, true)
            .map_err(|_| "cannot prepare output directory")?;
        // Exercise exclusive staging before the remote call without publishing an empty image.
        drop(
            entry
                .stage(&[], None)
                .map_err(|_| "cannot stage output image")?,
        );
        Ok(Self {
            entry,
            path: resolved,
            lexical,
            files: files.clone(),
        })
    }

    pub(crate) fn save(self, bytes: &[u8], cancel: &AtomicBool) -> Result<PathBuf, String> {
        if cancel.load(Ordering::Acquire) {
            return Err("image save cancelled".into());
        }
        if self
            .files
            .io_target(&self.lexical)
            .map_err(|e| e.to_string())?
            != self.path
            || self
                .entry
                .absolute_path()
                .map_err(|_| "output directory changed")?
                != self.path
        {
            return Err("output directory changed during image generation".into());
        }
        let staged = self
            .entry
            .stage(bytes, None)
            .map_err(|_| "cannot stage returned image")?;
        if cancel.load(Ordering::Acquire) {
            return Err("image save cancelled".into());
        }
        staged.commit(true).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "output_path became occupied; image not overwritten"
            } else {
                "cannot commit returned image"
            }
        })?;
        self.entry
            .sync()
            .map_err(|_| "image saved but output directory sync failed")?;
        Ok(self.lexical)
    }
}
