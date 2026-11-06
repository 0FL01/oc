use super::*;

#[tokio::test]
async fn generic_tool_preview_keeps_action_separate_from_policy_resources_and_digest() {
    use crate::tools::{AllowAllPolicy, SkillSnapshot, ToolRoots};
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let files = crate::files::Files::new(project.path(), data.path()).unwrap();
    let shell = crate::shell::Shell::new(project.path()).unwrap();
    let env = std::collections::BTreeMap::new();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let (snapshot, _) = SkillSnapshot::build(&[]);
    let ctx = ToolContext {
        files: &files,
        shell: &shell,
        parent_env: &env,
        webfetch_auth: None,
        policy: &AllowAllPolicy,
        subagent: None,
        snapshot: &snapshot,
        cancel: &cancel,
        roots: Some(ToolRoots {
            project: project.path().into(),
            data: data.path().into(),
        }),
    };
    let mut call = ToolCall {
        id: "call-mcp".into(),
        name: "vis36__echo".into(),
        arguments: serde_json::json!({"text": "actual invocation"}),
    };
    let resources = crate::tools::permission_resources(&call).unwrap();
    assert_eq!(resources, ["*"]);
    assert_ne!(call.name, resources[0]);
    let (preview, digest, preimage, cwd) = prepare(&ctx, &call, &resources).await.unwrap();
    assert_eq!(preview, ApprovalPreview::Tool);
    assert!(preimage.is_none() && cwd.is_none());
    assert_eq!(
        grant_resources(project.path(), &call.name, &resources),
        resources
    );
    assert_eq!(save_patterns(&call.name, &resources), resources);
    assert_eq!(prepare(&ctx, &call, &resources).await.unwrap().1, digest);
    call.arguments["text"] = "changed invocation".into();
    assert_ne!(prepare(&ctx, &call, &resources).await.unwrap().1, digest);
    for (name, arguments, resources) in [
        (
            "read",
            serde_json::json!({"path":"note.txt"}),
            vec!["note.txt".into()],
        ),
        (
            "webfetch",
            serde_json::json!({"url":"https://example.com/"}),
            vec!["https://example.com/".into()],
        ),
        (
            "image_output",
            serde_json::json!({"path":"pictures/new.png"}),
            vec!["pictures/new.png".into()],
        ),
    ] {
        call.name = name.into();
        call.arguments = arguments;
        assert_eq!(
            prepare(&ctx, &call, &resources).await.unwrap().0,
            ApprovalPreview::Resource { values: resources }
        );
    }
    assert_eq!(crate::config::legacy_key(&call.name), "apply_patch");
    assert!(
        !project.path().join("pictures").exists(),
        "approval must not create output directories"
    );
    std::fs::create_dir(project.path().join("pictures")).unwrap();
    std::fs::write(project.path().join("pictures/new.png"), b"occupied").unwrap();
    let resources = crate::tools::permission_resources(&call).unwrap();
    assert!(prepare(&ctx, &call, &resources).await.is_err());
}

#[test]
fn donor_identity_hash_and_origin_forms() {
    assert_eq!(
        identity_hash("abc"),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        identity_hash("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
    );
    assert_eq!(
        normalize_origin("git@EXAMPLE.com:owner/repo.git"),
        normalize_origin("https://user:password@example.com/owner/repo.git/")
    );
    assert_eq!(normalize_origin("file:///repo"), None);
    // Pinned donor project.ts parts(), including its single optional slash.
    for (input, expected) in [
        (
            "git@EXAMPLE.com:owner/repo.git.git",
            "example.com/owner/repo.git",
        ),
        (
            "https://example.com/owner/repo.git.git/",
            "example.com/owner/repo.git",
        ),
        (
            "https://example.com/owner/repo.git//",
            "example.com/owner/repo.git",
        ),
        ("git@example.com:Owner/Repo.git", "example.com/Owner/Repo"),
    ] {
        assert_eq!(normalize_origin(input).as_deref(), Some(expected));
    }
    assert_ne!(
        normalize_origin("git@example.com:owner/repo.git"),
        normalize_origin("git@example.com:owner/repo.git.git")
    );
    assert_ne!(
        normalize_origin("git@example.com:owner/Repo.git"),
        normalize_origin("git@example.com:owner/repo.git")
    );
}

#[test]
fn saved_file_patterns_are_wildcard_without_changing_literal_match_identity() {
    use crate::permissions::wildcard_preserving_identity as matches;
    let patterns = save_patterns("read", &[r"a\b".into(), "Case".into()]);
    assert_eq!(patterns, ["*"]);
    assert!(matches(r"a\b", r"a\b"));
    assert!(!matches("a/b", r"a\b"));
    assert!(!matches("case", "Case"));
    assert!(matches("a/b/c", "a/*"));
    assert!(matches(r"a\bc", r"a\b?"));
    assert!(!matches("a/bc", r"a\b?"));
    assert!(matches("git status", "git *"));
    assert!(matches("git", "git *"));
    assert_eq!(
        save_patterns("opencode_session_rename", &["session-a".into()]),
        ["session-a"]
    );
    assert!(save_patterns("opencode_session_rename", &["session-*".into()]).is_empty());
}

#[test]
fn w7_external_save_uses_nearest_target_repository_or_target_directory() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("repo/nested/target");
    std::fs::create_dir_all(&target).unwrap();
    let resource = target.join("*").to_string_lossy().into_owned();
    assert_eq!(
        save_patterns("external_directory", std::slice::from_ref(&resource)),
        std::slice::from_ref(&resource)
    );
    std::fs::create_dir(root.path().join("repo/.hg")).unwrap();
    assert_eq!(
        save_patterns("external_directory", std::slice::from_ref(&resource)),
        [root.path().join("repo/*").to_string_lossy()]
    );
    std::fs::write(root.path().join("repo/nested/.git"), "gitdir: fixture").unwrap();
    assert_eq!(
        save_patterns("external_directory", &[resource]),
        [root.path().join("repo/nested/*").to_string_lossy()]
    );
}

#[test]
fn donor_git_root_origin_worktree_clone_and_non_git_identity() {
    fn git(path: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("HOME", path)
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "fixture Git operation failed");
        String::from_utf8(output.stdout).unwrap().trim().into()
    }
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-qm",
            "root",
        ],
    );
    let original = project_identity(repo.path()).unwrap();
    assert_eq!(original, git(repo.path(), &["rev-parse", "HEAD"]));
    let other = tempfile::tempdir().unwrap();
    let linked = other.path().join("linked");
    git(
        repo.path(),
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.to_str().unwrap(),
        ],
    );
    assert_eq!(project_identity(&linked).unwrap(), original);
    git(
        repo.path(),
        &["remote", "add", "origin", "git@Example.COM:owner/repo.git"],
    );
    let remote = project_identity(repo.path()).unwrap();
    assert_eq!(project_identity(&linked).unwrap(), remote);
    let clone = other.path().join("clone");
    git(
        other.path(),
        &[
            "clone",
            "-q",
            repo.path().to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
    );
    git(
        &clone,
        &[
            "remote",
            "set-url",
            "origin",
            "https://example.com/owner/repo.git/",
        ],
    );
    assert_eq!(project_identity(&clone).unwrap(), remote);
    git(
        &clone,
        &[
            "remote",
            "set-url",
            "origin",
            "https://example.com/owner/Repo.git/",
        ],
    );
    assert_ne!(project_identity(&clone).unwrap(), remote);
    git(
        &clone,
        &[
            "remote",
            "set-url",
            "origin",
            "https://example.com/owner/repo.git.git/",
        ],
    );
    assert_ne!(project_identity(&clone).unwrap(), remote);
    std::fs::create_dir(linked.join("subdir")).unwrap();
    assert_eq!(
        grant_resources(&linked.join("subdir"), "read", &["file".into()]),
        vec!["subdir/file"]
    );
    let plain = tempfile::tempdir().unwrap();
    let another = tempfile::tempdir().unwrap();
    assert_ne!(
        project_identity(plain.path()).unwrap(),
        project_identity(another.path()).unwrap()
    );
    let child = plain.path().join("child");
    std::fs::create_dir(&child).unwrap();
    assert_ne!(
        project_identity(plain.path()).unwrap(),
        project_identity(&child).unwrap()
    );
    let alias = another.path().join("alias");
    std::os::unix::fs::symlink(plain.path(), &alias).unwrap();
    assert_eq!(
        project_identity(&alias).unwrap(),
        project_identity(plain.path()).unwrap()
    );
    assert_eq!(
        grant_resources(plain.path(), "read", &[r"a\b".into(), "Case".into()]),
        vec![r"a\b", "Case"]
    );
    assert!(!repo.path().join(".git/opencode").exists());
}
