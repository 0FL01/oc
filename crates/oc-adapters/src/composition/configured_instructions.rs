//! Config-declared instruction captures, separate from automatic Nested roots.
use super::*;
use crate::instructions::{Configured, Origin, Source};
use futures_util::StreamExt as _;
use sha2::{Digest as _, Sha256};
use std::cell::RefCell;
use std::os::unix::fs::MetadataExt as _;

const ENTRIES_CAP: usize = 4096;
const DEPTH_CAP: usize = 32;

pub(super) fn load(
    project: &Path,
    env: &BTreeMap<String, String>,
    documents: &[config::Source],
    roots: &[crate::instructions::Root],
    data: Option<&crate::files::Files>,
    watch: &RefCell<watch_inputs::WatchInputs>,
) -> Result<Configured, LoadFailure> {
    let mut configured = Configured::default();
    let mut digest = Sha256::new();
    let mut declarations = BTreeSet::new();
    let mut paths: BTreeSet<String> = roots
        .iter()
        .filter_map(|root| root.baseline.as_ref().map(|source| source.path.clone()))
        .collect();
    let stop = roots
        .iter()
        .filter(|root| root.origin == Origin::Project && project.starts_with(&root.path))
        .min_by_key(|root| root.path.components().count())
        .map_or(project, |root| root.path.as_path());
    for document in documents {
        let value = config::parse_jsonc(&document.text, &document.path)
            .map_err(|_| invalid(&document.path, &["instructions"]))?;
        // Cosmetic JSONC rewrites must retain the existing semantic reload noop.
        digest.update(document.path.as_bytes());
        digest.update([0, u8::from(document.trusted)]);
        digest.update(value.to_string().as_bytes());
        digest.update([0]);
        let Some(input) = value.get("instructions") else {
            continue;
        };
        if !document.trusted {
            return Err(failure(
                &document.path,
                &["instructions"],
                ServiceStage::Admission,
                ServiceCode::TrustRefused,
            ));
        }
        let entries = input
            .as_array()
            .ok_or_else(|| invalid(&document.path, &["instructions"]))?;
        let origin = if roots.iter().any(|root| {
            root.origin == Origin::Global && Path::new(&document.path).starts_with(&root.path)
        }) {
            Origin::Global
        } else {
            Origin::Project
        };
        for entry in entries {
            let entry = entry
                .as_str()
                .ok_or_else(|| invalid(&document.path, &["instructions"]))?;
            if !declarations.insert(entry.to_owned()) {
                continue;
            }
            if declarations.len() > crate::instructions::SOURCES_CAP {
                return Err(capacity());
            }
            if reqwest::Url::parse(entry).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            {
                configured.urls.push((
                    entry.to_owned(),
                    origin.clone(),
                    configured.sources.len() + configured.urls.len(),
                ));
                continue;
            }
            let expanded = if let Some(relative) = entry.strip_prefix("~/") {
                let Some(home) = env.get("HOME") else {
                    continue;
                };
                Path::new(home).join(relative.trim_start_matches('/'))
            } else {
                PathBuf::from(entry)
            };
            if expanded.as_os_str().is_empty() {
                continue;
            }
            let mut matches = Vec::new();
            if expanded.is_absolute() {
                matches.extend(scan(&expanded, project, data, watch)?);
            } else {
                for directory in project.ancestors() {
                    matches.extend(scan(&directory.join(&expanded), project, data, watch)?);
                    if directory == stop {
                        break;
                    }
                }
            }
            // Match/order belongs to the selected name; canonical identities
            // only own admission, dedup and the descriptor read below.
            matches.sort_by(|left, right| left.0.cmp(&right.0));
            for (_, path, root) in matches {
                if !paths.insert(path.to_string_lossy().into_owned()) {
                    continue;
                }
                let text = read_instruction(&root, &path, data)
                    .map_err(|_| invalid(&document.path, &["instructions"]))?;
                watch.borrow_mut().file(&path);
                configured
                    .sources
                    .push(Source::baseline(&path, &root.path, origin.clone(), &text));
                validate(roots, &configured.sources)?;
            }
        }
    }
    configured.config_digest = digest.finalize().into();
    Ok(configured)
}

fn capacity() -> LoadFailure {
    failure(
        "configured instructions",
        &["instructions"],
        ServiceStage::Admission,
        ServiceCode::CapacityExceeded,
    )
}

fn validate(roots: &[crate::instructions::Root], configured: &[Source]) -> Result<(), LoadFailure> {
    let mut sources = Vec::new();
    for source in roots.iter().filter_map(|root| root.baseline.as_deref()) {
        if !sources
            .iter()
            .any(|present: &Source| present.path == source.path)
        {
            sources.push(source.clone());
        }
    }
    sources.extend_from_slice(configured);
    crate::instructions::validate_sources(&sources).map_err(|_| capacity())
}

fn scan(
    pattern: &Path,
    project: &Path,
    data: Option<&crate::files::Files>,
    watch: &RefCell<watch_inputs::WatchInputs>,
) -> Result<Vec<(PathBuf, PathBuf, AdmittedRoot)>, LoadFailure> {
    let meta = |part: &std::ffi::OsStr| part.to_string_lossy().contains(['*', '?', '[', '{']);
    let glob = pattern.components().any(|part| meta(part.as_os_str()));
    let directory = if glob {
        pattern
            .components()
            .take_while(|part| !meta(part.as_os_str()))
            .collect::<PathBuf>()
    } else {
        pattern.parent().unwrap_or(Path::new("/")).to_path_buf()
    };
    // A missing literal prefix is observed, but never creates a discovery root.
    watch.borrow_mut().file(&directory);
    if let Some(scope) = data {
        scope
            .io_target(&directory)
            .map_err(|_| invalid("configured instructions", &["data_root"]))?;
    }
    let Some(root) = admit_root(&directory, project, false)? else {
        return Ok(Vec::new());
    };
    if let Some(scope) = data {
        scope
            .check_data_file(&root.dir)
            .map_err(|_| invalid("configured instructions", &["data_root"]))?;
    }
    if !glob {
        watch.borrow_mut().file(pattern);
        let Some(path) = admit_instruction(pattern, &root.path)? else {
            return Ok(Vec::new());
        };
        if let Some(scope) = data {
            scope
                .io_target(&path)
                .map_err(|_| invalid("configured instructions", &["data_root"]))?;
        }
        if !path.is_file() {
            return Ok(Vec::new());
        }
        return Ok(vec![(pattern.to_path_buf(), path, root)]);
    }
    let relative = pattern
        .strip_prefix(&directory)
        .map_err(|_| invalid("configured instructions", &["instructions"]))?;
    let relative = relative
        .to_str()
        .ok_or_else(|| invalid("configured instructions", &["instructions"]))?;
    let mut builder = ignore::overrides::OverrideBuilder::new(&root.path);
    builder
        .add(relative)
        .map_err(|_| invalid("configured instructions", &["instructions"]))?;
    let matcher = builder
        .build()
        .map_err(|_| invalid("configured instructions", &["instructions"]))?;
    let recursive = relative.contains('/');
    let mut pending = vec![(PathBuf::from("."), 0usize)];
    let mut seen = BTreeSet::new();
    let mut inspected = 0usize;
    let mut matches = BTreeMap::new();
    while let Some((relative, depth)) = pending.pop() {
        let dir = admitted_fs::open_beneath_no_symlinks(
            &root.dir,
            &relative,
            libc::O_RDONLY | libc::O_DIRECTORY,
        )
        .map_err(|_| invalid("configured instructions", &["instructions"]))?;
        if data.is_some_and(|scope| scope.check_data_file(&dir).is_err()) {
            continue;
        }
        let metadata = dir
            .metadata()
            .map_err(|_| invalid("configured instructions", &["instructions"]))?;
        if !seen.insert((metadata.dev(), metadata.ino())) {
            continue;
        }
        watch.borrow_mut().opened(
            &root.path.join(&relative),
            &dir,
            defs::InputKind::InstructionDirectory,
        );
        let entries = std::fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd()))
            .map_err(|_| invalid("configured instructions", &["instructions"]))?;
        for entry in entries {
            inspected += 1;
            if inspected > ENTRIES_CAP {
                return Err(capacity());
            }
            let entry = entry.map_err(|_| invalid("configured instructions", &["instructions"]))?;
            let kind = entry
                .file_type()
                .map_err(|_| invalid("configured instructions", &["instructions"]))?;
            if !kind.is_file() && !kind.is_dir() && !kind.is_symlink() {
                continue;
            }
            let candidate = root.path.join(&relative).join(entry.file_name());
            if kind.is_dir() && !recursive {
                continue;
            }
            let matched = matcher.matched(&candidate, false).is_whitelist();
            if (kind.is_file() || !recursive) && !matched {
                continue;
            }
            let Some(path) = admit_instruction(&entry.path(), &root.path)? else {
                continue;
            };
            if data.is_some_and(|scope| scope.io_target(&path).is_err()) {
                continue;
            }
            let admitted = admitted_fs::open_beneath_no_symlinks(
                &root.dir,
                path.strip_prefix(&root.path)
                    .map_err(|_| invalid("configured instructions", &["instructions"]))?,
                libc::O_RDONLY,
            )
            .map_err(|_| invalid("configured instructions", &["instructions"]))?;
            if data.is_some_and(|scope| scope.check_data_file(&admitted).is_err()) {
                continue;
            }
            let metadata = admitted
                .metadata()
                .map_err(|_| invalid("configured instructions", &["instructions"]))?;
            if metadata.is_dir() && recursive {
                if depth == DEPTH_CAP {
                    return Err(capacity());
                }
                pending.push((
                    path.strip_prefix(&root.path).unwrap().to_path_buf(),
                    depth + 1,
                ));
            } else if metadata.is_file() && matched {
                matches.entry(candidate).or_insert(path);
                if matches.len() > crate::instructions::SOURCES_CAP {
                    return Err(capacity());
                }
            }
        }
    }
    matches
        .into_iter()
        .map(|(candidate, path)| {
            let dir = root
                .dir
                .try_clone()
                .map_err(|_| invalid("configured instructions", &["instructions"]))?;
            Ok((
                candidate,
                path,
                AdmittedRoot {
                    path: root.path.clone(),
                    dir,
                },
            ))
        })
        .collect()
}

impl Composition {
    /// Resolve once on a fresh local candidate, never inside local re-admission.
    pub(crate) async fn refresh_instruction_urls(
        &mut self,
        previous: Option<&Configured>,
        force: bool,
    ) -> Result<(), LoadFailure> {
        let changed = force
            || previous
                .is_none_or(|old| old.config_digest != self.configured_instructions.config_digest);
        let remote = futures_util::stream::iter(self.configured_instructions.urls.clone())
            .map(|(url, origin, position)| async move {
                // The URL may carry a query credential. It is never a durable label.
                let label = format!("url:{:x}", Sha256::digest(url.as_bytes()));
                let prior = previous
                    .and_then(|old| old.sources.iter().find(|source| source.path == label))
                    .cloned()
                    .map(|mut source| {
                        source.origin = origin.clone();
                        source
                    });
                if !changed {
                    return (position, prior);
                }
                let result = crate::webfetch::fetch_raw(
                    crate::webfetch::RawRequest {
                        url,
                        method: "GET".into(),
                        headers: Vec::new(),
                        body: None,
                    },
                    crate::webfetch::FetchOptions {
                        timeout: Duration::from_secs(5),
                        connect_timeout: Duration::from_secs(5),
                        body_cap: defs::MAX_INSTRUCTIONS_FILE,
                        ..Default::default()
                    },
                    &AtomicBool::new(false),
                    // This exact trusted config declaration owns the GET; this is
                    // not a model webfetch grant. Raw fetch keeps TLS/userinfo gates.
                    &crate::tools::AllowAllPolicy,
                )
                .await;
                let content = result
                    .ok()
                    .filter(|response| (200..300).contains(&response.status))
                    .and_then(|response| String::from_utf8(response.body).ok());
                let source = content
                    .map(|content| {
                        Source::baseline(
                            Path::new(&label),
                            Path::new("configured URL"),
                            origin,
                            &content,
                        )
                    })
                    .or(prior);
                (position, source)
            })
            .buffered(4)
            .collect::<Vec<_>>()
            .await;
        let mut skipped = 0;
        for (position, source) in remote {
            if let Some(source) = source {
                self.configured_instructions
                    .sources
                    .insert(position - skipped, source);
            } else {
                skipped += 1;
            }
        }
        validate(
            &self.instruction_roots,
            &self.configured_instructions.sources,
        )?;
        let files = self
            .instruction_roots
            .iter()
            .filter_map(|root| root.baseline.as_deref())
            .chain(&self.configured_instructions.sources)
            .filter_map(|source| {
                source
                    .content
                    .as_ref()
                    .map(|content| (source.path.clone(), Ok(content.clone())))
            })
            .collect::<Vec<_>>();
        let (text, _) = defs::load_instruction_texts(&files);
        self.instructions = text;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    #[tokio::test]
    async fn w10_configured_sources_exclude_actual_data_root_before_reading_descendants() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let external = root.path().join("external");
        let data = external.join("state");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(external.join("public.md"), "PUBLIC_SOURCE").unwrap();
        std::fs::write(data.join("private.md"), "PRIVATE_DATA_CANARY").unwrap();
        let path = project.join("opencode.json");
        let mut input =
            serde_json::json!({"instructions":[format!("{}/**/*.md",external.display())]});
        std::fs::write(&path, input.to_string()).unwrap();
        let env = BTreeMap::from([(
            "OPENCODE_CONFIG_DIR".into(),
            root.path().join("global").to_string_lossy().into_owned(),
        )]);
        let loaded = load_local_inputs(&project, env.clone(), false, Some(&data))
            .await
            .unwrap();
        assert!(loaded.instructions.contains("PUBLIC_SOURCE"));
        assert!(!loaded.instructions.contains("PRIVATE_DATA_CANARY"));
        assert!(
            !loaded
                .configured_instructions
                .sources
                .iter()
                .any(|source| Path::new(&source.path).starts_with(&data))
        );
        input["instructions"] = serde_json::json!([data.join("private.md")]);
        std::fs::write(&path, input.to_string()).unwrap();
        assert!(
            load_local_inputs(&project, env, false, Some(&data))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn w10_url_capture_is_raw_bounded_last_good_and_not_repeated_on_local_rescans() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicUsize;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "HTTP://{}/instructions?credential=URL_CANARY",
            listener.local_addr().unwrap()
        );
        let requests = Arc::new(AtomicUsize::new(0));
        let bad_auth = Arc::new(AtomicBool::new(false));
        let count = requests.clone();
        let auth = bad_auth.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut header = Vec::new();
                while !header.ends_with(b"\r\n\r\n") && header.len() < 8192 {
                    let mut byte = [0];
                    if stream.read(&mut byte).await.unwrap() == 0 {
                        break;
                    }
                    header.push(byte[0]);
                }
                let header = String::from_utf8_lossy(&header).to_ascii_lowercase();
                auth.fetch_or(
                    header.contains("\r\nauthorization:") || header.contains("\r\nx-api-key:"),
                    Ordering::SeqCst,
                );
                let index = count.fetch_add(1, Ordering::SeqCst);
                let (status, body) = match index {
                    0 => ("200 OK", "REMOTE_A <b>raw</b>"),
                    1 => ("200 OK", "REMOTE_B <b>raw</b>"),
                    _ => ("500 Internal Server Error", "not instruction data"),
                };
                stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("state");
        std::fs::create_dir_all(&project).unwrap();
        let path = project.join("opencode.json");
        let mut input = serde_json::json!({"instructions":[url,"rule.md"]});
        std::fs::write(project.join("rule.md"), "LOCAL_A").unwrap();
        std::fs::write(&path, input.to_string()).unwrap();
        let env = BTreeMap::from([(
            "OPENCODE_CONFIG_DIR".into(),
            root.path().join("global").to_string_lossy().into_owned(),
        )]);
        let mut initial = load_local_inputs(&project, env.clone(), false, Some(&data))
            .await
            .unwrap();
        initial.refresh_instruction_urls(None, false).await.unwrap();
        let initial_raw = initial.instructions.contains("REMOTE_A <b>raw</b>");
        let declared_order = initial
            .instructions
            .find("REMOTE_A")
            .zip(initial.instructions.find("LOCAL_A"))
            .is_some_and(|(remote, local)| remote < local)
            && initial
                .configured_instructions
                .sources
                .first()
                .and_then(|source| source.content.as_deref())
                .is_some_and(|text| text.contains("REMOTE_A"));
        let labels_safe = !serde_json::to_string(&initial.configured_instructions.sources)
            .unwrap()
            .contains("URL_CANARY");
        std::fs::write(
            &path,
            format!(
                "// cosmetic\n{}",
                serde_json::to_string_pretty(&input).unwrap()
            ),
        )
        .unwrap();
        let cosmetic = load_local_inputs(&project, env.clone(), true, Some(&data))
            .await
            .unwrap();
        let semantic_noop = cosmetic.local_inputs == initial.local_inputs;
        std::fs::write(project.join("rule.md"), "LOCAL_B").unwrap();
        let mut local = load_local_inputs(&project, env.clone(), true, Some(&data))
            .await
            .unwrap();
        let local_changed = local.local_inputs != initial.local_inputs;
        local
            .refresh_instruction_urls(Some(&initial.configured_instructions), false)
            .await
            .unwrap();
        let local_kept = local.instructions.contains("LOCAL_B")
            && local.instructions.contains("REMOTE_A <b>raw</b>")
            && requests.load(Ordering::SeqCst) == 1;
        let mut forced = load_local_inputs(&project, env.clone(), true, Some(&data))
            .await
            .unwrap();
        forced
            .refresh_instruction_urls(Some(&local.configured_instructions), true)
            .await
            .unwrap();
        let force_updated = forced.instructions.contains("REMOTE_B <b>raw</b>")
            && requests.load(Ordering::SeqCst) == 2;
        let mut failed = load_local_inputs(&project, env.clone(), true, Some(&data))
            .await
            .unwrap();
        failed
            .refresh_instruction_urls(Some(&forced.configured_instructions), true)
            .await
            .unwrap();
        let last_good = failed.instructions.contains("REMOTE_B <b>raw</b>")
            && !failed.instructions.contains("not instruction data");
        input["instructions"] = serde_json::json!(["rule.md"]);
        std::fs::write(&path, input.to_string()).unwrap();
        let mut removed = load_local_inputs(&project, env.clone(), true, Some(&data))
            .await
            .unwrap();
        removed
            .refresh_instruction_urls(Some(&failed.configured_instructions), false)
            .await
            .unwrap();
        let removed_stays_removed =
            !removed.instructions.contains("REMOTE_") && requests.load(Ordering::SeqCst) == 3;
        input["instructions"] = serde_json::json!([format!("{url}&new=1"), url, "rule.md"]);
        std::fs::write(&path, input.to_string()).unwrap();
        let mut skipped = load_local_inputs(&project, env, true, Some(&data))
            .await
            .unwrap();
        skipped
            .refresh_instruction_urls(Some(&failed.configured_instructions), false)
            .await
            .unwrap();
        let failed_new_url_skipped = skipped.configured_instructions.sources.len() == 2
            && skipped
                .instructions
                .find("REMOTE_B")
                .zip(skipped.instructions.find("LOCAL_B"))
                .is_some_and(|(remote, local)| remote < local)
            && requests.load(Ordering::SeqCst) == 5;
        server.abort();
        let _ = server.await;
        assert!(
            initial_raw
                && declared_order
                && labels_safe
                && semantic_noop
                && local_changed
                && local_kept
                && force_updated
                && last_good
                && removed_stays_removed
                && failed_new_url_skipped
        );
        assert!(!bad_auth.load(Ordering::SeqCst));
    }
}
