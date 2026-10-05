//! Private admitted-destination handoff to the existing application supervisor.
use super::*;
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::os::fd::AsRawFd as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, PathBuf};

pub(crate) struct Prepared {
    pub(crate) record: crate::storage::MoveRecord,
    pub(crate) composition: Composition,
    pub(super) effective: Effective,
    pub(super) registry: WorkspaceRegistry,
    pub(super) directory: File,
    pub(super) identity: (u64, u64),
}

pub(crate) fn resolve(
    source: &str,
    raw: &str,
    env: &BTreeMap<String, String>,
) -> Result<String, String> {
    let raw = raw.trim();
    let path = if raw == "~" || raw.starts_with("~/") {
        let home = env
            .get("HOME")
            .filter(|s| Path::new(s).is_absolute())
            .ok_or("move home unavailable")?;
        Path::new(home).join(raw.strip_prefix("~/").unwrap_or(""))
    } else {
        if raw.starts_with('~') {
            return Err("unsupported move home".into());
        }
        Path::new(source).join(raw)
    };
    let mut normalized = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            _ => normalized.push(c),
        }
    }
    let resolved = normalized
        .into_os_string()
        .into_string()
        .map_err(|_| "invalid move directory".to_string())?;
    if resolved.len() > 4096 || resolved.chars().any(char::is_control) {
        return Err("invalid move directory".into());
    }
    Ok(resolved)
}

fn pin(path: &str, data: &Path) -> Result<File, String> {
    if Path::new(path).starts_with(data) {
        return Err("move directory protected".into());
    }
    let slash = File::open("/").map_err(|_| "move directory unavailable")?;
    crate::admitted_fs::open_beneath_no_symlinks(
        &slash,
        Path::new(path)
            .strip_prefix("/")
            .map_err(|_| "invalid move directory")?,
        libc::O_RDONLY | libc::O_DIRECTORY,
    )
    .map_err(|_| "move directory unavailable".into())
}

pub(crate) async fn prepare(
    db: &Db,
    mut record: crate::storage::MoveRecord,
    env: BTreeMap<String, String>,
) -> Result<Prepared, String> {
    let directory = pin(&record.directory, db.root())?;
    let metadata = directory
        .metadata()
        .map_err(|_| "move directory unavailable")?;
    let mut composition = composition::load_local_with_env(Path::new(&record.directory), env)
        .await
        .map_err(|_| "move destination admission failed")?;
    composition
        .resolve_credentials(db)
        .map_err(|_| "move destination auth admission failed")?;
    composition
        .refresh_provider()
        .await
        .map_err(|_| "move destination catalog admission failed")?;
    let runtime =
        build_runtime(db, &composition).map_err(|_| "move destination runtime admission failed")?;
    let mut effective = Effective::from_composition(&composition);
    effective.legacy_epoch = selection::legacy_epoch(db, &composition)
        .map_err(|_| "move destination selection unavailable")?;
    let mut registry = WorkspaceRegistry::bind(
        runtime.generation_id(),
        runtime.location(),
        &composition.generation,
        workspace_agents(&composition),
        skill_metas(&composition),
    );
    effective
        .apply_persisted_model(db, &composition)
        .map_err(|_| "move destination selection unavailable")?;
    effective
        .apply_persisted_agent(db, &composition, &mut registry)
        .map_err(|_| "move destination selection unavailable")?;
    if db
        .session_meta(&record.session)
        .map_err(|_| "move target unavailable")?
        .parent_id
        .is_none()
    {
        let deck = tab_deck::load(db, &runtime).map_err(|_| "move destination tabs unavailable")?;
        if deck.projected() {
            return Err("move destination tabs require repair".into());
        }
    }
    let future = selection::for_turn(db, &composition, &effective, &record.session)
        .map_err(|_| "move destination session selection unavailable")?;
    future
        .admit_selection(&composition)
        .map_err(|_| "move destination session selection unavailable")?;
    publish_workspace(&runtime, &composition, &effective)
        .map_err(|_| "move destination workspace admission failed")?;
    runtime
        .admit_provider(
            &composition.catalog,
            &future.model_id,
            &composition.provider,
        )
        .map_err(|_| "move destination provider unavailable")?;
    if composition
        .generation
        .mcp
        .values()
        .filter(|entry| entry.enabled)
        .count()
        > crate::runtime::MAX_MCP_SERVERS
    {
        return Err("move destination MCP capacity exceeded".into());
    }
    let bytes = format!(
        "{:?}\n{:?}\n{}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        composition.generation,
        composition.catalog,
        composition.instructions,
        composition.agents,
        composition.skills,
        composition.parent_env,
        (
            &composition.model_id,
            &composition.variant,
            &composition.default_agent
        ),
        future.snapshot(&composition, 0)
    );
    // Debug intentionally masks credentials. Bind the complete admitted native
    // options too, hashing in memory only; no secret-bearing payload is stored.
    let bindings = serde_json::to_vec(&(
        &composition.generation.providers,
        &composition.generation.mcp,
    ))
    .map_err(|_| "move destination binding unavailable")?;
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest.update(bindings);
    let fingerprint = format!("{:x}", digest.finalize());
    if !record.fingerprint.is_empty() && record.fingerprint != fingerprint {
        return Err("move destination generation changed".into());
    }
    record.fingerprint = fingerprint;
    let prepared = Prepared {
        record,
        composition,
        effective,
        registry,
        directory,
        identity: (metadata.dev(), metadata.ino()),
    };
    prepared.recheck(db)?;
    Ok(prepared)
}

impl Prepared {
    pub(crate) fn recheck(&self, db: &Db) -> Result<(), String> {
        Self::recheck_parts(db, &self.record, &self.directory, self.identity)
    }

    pub(super) fn recheck_parts(
        db: &Db,
        record: &crate::storage::MoveRecord,
        directory: &File,
        identity: (u64, u64),
    ) -> Result<(), String> {
        db.check_move_deck(&record.session, &record.directory)
            .map_err(|_| "move destination tabs unavailable")?;
        if db
            .get_pref(&format!(
                "{}{}",
                crate::storage::SESSION_LOCATION_PREFIX,
                record.session
            ))
            .map_err(|_| "move storage unavailable")?
            .as_deref()
            != Some(&record.source)
        {
            return Err("stale move target".into());
        }
        let file = pin(&record.directory, db.root())?;
        let meta = file.metadata().map_err(|_| "move directory unavailable")?;
        let actual = std::fs::canonicalize(format!("/proc/self/fd/{}", directory.as_raw_fd()))
            .map_err(|_| "move directory unavailable")?;
        if (meta.dev(), meta.ino()) != identity || actual != Path::new(&record.directory) {
            return Err("move directory changed".into());
        }
        Ok(())
    }
}
