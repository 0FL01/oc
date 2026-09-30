//! Owner preparation and donor project equivalence. No Git writes or index operations.
use crate::tools::{ToolCall, ToolContext};
use oc_core::approval::ApprovalPreview;
use sha2::{Digest as _, Sha256};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Donor project identity: origin hash, cached identity, first root commit,
/// or canonical non-Git directory hash. Never writes donor's Git cache file.
pub(crate) fn project_identity(path: &Path) -> Result<String, String> {
    let directory = std::fs::canonicalize(path).map_err(|_| "project root unavailable")?;
    let Some(root) = directory.ancestors().find(|p| p.join(".git").exists()) else {
        return Ok(identity_hash(&format!("directory:{}", directory.display())));
    };
    let git = |args: &[&str]| -> Option<String> {
        let output = std::process::Command::new("git")
            .arg("--no-optional-locks")
            .args(args)
            .current_dir(root)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .ok()?;
        if !output.status.success() || output.stdout.len() > 65536 {
            return None;
        }
        Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
    };
    if let Some(remote) = git(&["config", "--get", "remote.origin.url"])
        && let Some(normalized) = normalize_origin(&remote)
    {
        return Ok(identity_hash(&format!("git-remote:{normalized}")));
    }
    if let Some(common) = git(&["rev-parse", "--git-common-dir"]) {
        let cached = root.join(common).join("opencode");
        if std::fs::metadata(&cached).is_ok_and(|m| m.len() <= 4096)
            && let Ok(value) = std::fs::read_to_string(cached)
            && !value.trim().is_empty()
        {
            return Ok(value.trim().into());
        }
    }
    if let Some(commits) = git(&["rev-list", "--max-parents=0", "HEAD"])
        && let Some(first) = commits.lines().min()
    {
        return Ok(first.into());
    }
    // Unborn repositories must not accidentally share a global permission grant.
    Ok(identity_hash(&format!(
        "directory:{}",
        crate::storage::session_project_root(&directory)
            .unwrap_or(directory)
            .display()
    )))
}

// SHA-1 is donor's non-security project identifier (Hash.fast), not an authority
// digest. Invocation and preimage authority always use SHA-256 above.
fn identity_hash(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    let bits = (bytes.len() as u64) * 8;
    bytes.push(0x80);
    while bytes.len() % 64 != 56 {
        bytes.push(0);
    }
    bytes.extend(bits.to_be_bytes());
    let mut h = [
        0x67452301u32,
        0xefcdab89,
        0x98badcfe,
        0x10325476,
        0xc3d2e1f0,
    ];
    for block in bytes.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(chunk.try_into().expect("word"));
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (v, n) in h.iter_mut().zip([a, b, c, d, e]) {
            *v = v.wrapping_add(n);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

pub(crate) fn normalize_origin(value: &str) -> Option<String> {
    let value = value.trim();
    let (host, name) = if let Ok(url) = reqwest::Url::parse(value) {
        if url.scheme() == "file" {
            return None;
        }
        (url.host_str()?.to_string(), url.path().to_string())
    } else {
        let (host, name) = value.split_once(':')?;
        (host.rsplit('@').next()?.to_string(), name.to_string())
    };
    // Donor parts(): remove exactly one .git suffix (optionally one slash),
    // then trailing slashes. Repository-name case remains significant.
    let name = name.trim_start_matches('/');
    let name = name
        .strip_suffix(".git/")
        .or_else(|| name.strip_suffix(".git"))
        .unwrap_or(name)
        .trim_end_matches('/');
    if host.is_empty() || name.is_empty() || host.contains('/') {
        return None;
    }
    Some(format!("{}/{name}", host.to_lowercase()))
}

/// Paths in saved grants are checkout-root relative, even when Location is a
/// subdirectory. Each linked worktree/clone has its own checkout prefix removed.
pub(crate) fn grant_resources(project: &Path, action: &str, resources: &[String]) -> Vec<String> {
    if action == "shell" {
        return resources
            .iter()
            .map(|r| format!("{}{r}", crate::tools::shell_call::COMMAND_GRANT_PREFIX))
            .collect();
    }
    if !matches!(action, "read" | "apply_patch") {
        return resources.to_vec();
    }
    let checkout = project
        .ancestors()
        .find(|p| p.join(".git").exists())
        .unwrap_or(project);
    resources
        .iter()
        .map(|resource| {
            let path = project.join(resource);
            path.strip_prefix(checkout)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// Repeated after wait: actual patch preimages/hunks, roots, and cwd are pinned.
pub(crate) async fn prepare(
    ctx: &ToolContext<'_>,
    call: &ToolCall,
    resources: &[String],
) -> Result<
    (
        ApprovalPreview,
        String,
        Option<String>,
        Option<std::sync::Arc<crate::shell::PinnedCwd>>,
    ),
    String,
> {
    let roots = ctx.roots.as_ref().ok_or("missing trusted roots")?;
    let mut hash = Sha256::new();
    let mut patch_preimage = None;
    let mut shell_cwd = None;
    let root_metadata =
        std::fs::metadata(&roots.project).map_err(|_| "project root unavailable")?;
    hash.update(root_metadata.dev().to_le_bytes());
    hash.update(root_metadata.ino().to_le_bytes());
    hash.update(call.arguments.to_string());
    hash.update(
        std::fs::canonicalize(&roots.project)
            .map_err(|_| "project root unavailable")?
            .as_os_str()
            .as_encoded_bytes(),
    );
    let preview = match call.name.as_str() {
        "apply_patch" => {
            let (proposed, digest) = crate::patch::approval_preview(
                &roots.project,
                &roots.data,
                call.arguments["patchText"]
                    .as_str()
                    .ok_or("missing patch")?,
            )
            .map_err(|e| format!("patch preflight: {}", e.error))?;
            hash.update(&digest);
            patch_preimage = Some(digest);
            ApprovalPreview::Patch {
                files: proposed.files,
                total_files: proposed.total_files,
                truncated: proposed.truncated,
            }
        }
        "read" => {
            let path = ctx
                .files
                .resolve_path(call.arguments["path"].as_str().ok_or("missing path")?)
                .map_err(|e| e.to_string())?;
            hash.update(path.as_os_str().as_encoded_bytes());
            ApprovalPreview::Resource {
                values: resources.to_vec(),
            }
        }
        "shell" | "bash" => {
            let invocation = crate::tools::shell_call::invocation(call, ctx.parent_env)
                .map_err(|e| e.to_string())?;
            hash.update(serde_json::to_vec(&invocation.argv).map_err(|_| "invalid shell argv")?);
            let pinned = ctx
                .shell
                .pin_cwd(&invocation.argv, &invocation.cwd)
                .map_err(|e| e.to_string())?;
            for value in pinned.identity().map_err(|e| e.to_string())? {
                hash.update(value.to_le_bytes());
            }
            hash.update(pinned.path.as_os_str().as_encoded_bytes());
            let path = pinned.path.clone();
            shell_cwd = Some(pinned);
            ApprovalPreview::Shell {
                command: resources[0].clone(),
                cwd: path.display().to_string(),
            }
        }
        "glob" | "grep" => {
            let pattern = call.arguments["pattern"]
                .as_str()
                .ok_or("missing pattern")?;
            if pattern.is_empty()
                || pattern.len() > crate::files::SEARCH_PATTERN_BYTES_CAP
                || pattern.contains('\0')
            {
                return Err("invalid search pattern".into());
            }
            if call.name == "grep" && call.arguments["literal"].as_bool() == Some(false) {
                return Err("regex mode unsupported; pass literal=true".into());
            }
            ApprovalPreview::Search {
                pattern: pattern.into(),
                cwd: roots.project.display().to_string(),
                include: call.arguments["include"].as_str().map(str::to_string),
            }
        }
        "webfetch" => {
            let mut url = reqwest::Url::parse(call.arguments["url"].as_str().ok_or("missing URL")?)
                .map_err(|_| "invalid URL")?;
            crate::webfetch::validate_url(&mut url).map_err(|e| e.to_string())?;
            crate::webfetch::check_static_host(&url, ctx.webfetch_allow_private)
                .map_err(|e| e.to_string())?;
            ApprovalPreview::Resource {
                values: resources.to_vec(),
            }
        }
        "skill" => {
            crate::tools::preflight_skill(
                ctx.snapshot,
                call.arguments["id"].as_str().ok_or("missing skill")?,
            )?;
            ApprovalPreview::Resource {
                values: resources.to_vec(),
            }
        }
        "subagent" => {
            crate::tools::preflight_subagent(ctx, call).map_err(|e| e.to_string())?;
            ApprovalPreview::Resource {
                values: resources.to_vec(),
            }
        }
        _ => ApprovalPreview::Resource {
            values: resources.to_vec(),
        },
    };
    Ok((
        preview,
        format!("{:x}", hash.finalize()),
        patch_preimage,
        shell_cwd,
    ))
}

/// Exact literal resources are safe to save only when they contain no wildcard
/// metacharacters. This is separate from the actual policy resource set.
pub(crate) fn save_patterns(action: &str, resources: &[String]) -> Vec<String> {
    if !matches!(
        action,
        "read" | "apply_patch" | "shell" | "bash" | "skill" | "subagent"
    ) {
        return vec!["*".into()];
    }
    resources
        .iter()
        .filter(|r| !r.contains(['*', '?']))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "approval_tests.rs"]
mod tests;
