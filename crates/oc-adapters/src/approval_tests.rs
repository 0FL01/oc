use super::*;

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
fn saved_patterns_preserve_unix_literal_identity_and_support_actual_patterns() {
    use crate::permissions::wildcard_preserving_identity as matches;
    let patterns = save_patterns("read", &[r"a\b".into(), "Case".into()]);
    assert_eq!(patterns, vec![r"a\b", "Case"]);
    assert!(matches(r"a\b", &patterns[0]));
    assert!(!matches("a/b", &patterns[0]));
    assert!(!matches("case", &patterns[1]));
    assert!(matches("a/b/c", "a/*"));
    assert!(matches(r"a\bc", r"a\b?"));
    assert!(!matches("a/bc", r"a\b?"));
    assert!(matches("git status", "git *"));
    assert!(matches("git", "git *"));
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
