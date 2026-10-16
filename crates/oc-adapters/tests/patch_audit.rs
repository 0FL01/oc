//! Independent T32 regressions. All writes, including outside sentinels, are temporary.
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use oc_adapters::patch::{AllowAll, PatchError, WritePolicy, apply_patch};
use sha2::{Digest as _, Sha256};

fn roots() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let data = temp.path().join("data");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&data).unwrap();
    (temp, project, data)
}

// The production permission boundary is called during preflight and again
// before execution. Mutate only test fixtures at that deterministic race point.
struct Race<'a> {
    path: &'a str,
    calls: std::cell::Cell<usize>,
    action: Box<dyn Fn() + 'a>,
}

impl WritePolicy for Race<'_> {
    fn check(&self, path: &str) -> Result<(), PatchError> {
        if path == self.path {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() == 2 {
                (self.action)();
            }
        }
        Ok(())
    }
}

#[test]
fn aud03_pinned_upstream_grammar_independent_bytes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/patch.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let (_temp, project, data) = roots();
        for (path, body) in case["seed"].as_object().unwrap() {
            fs::write(project.join(path), body.as_str().unwrap()).unwrap();
        }
        apply_patch(&project, &data, case["patch"].as_str().unwrap(), &AllowAll).unwrap();
        for (path, body) in case["expected"].as_object().unwrap() {
            if body.is_null() {
                assert!(!project.join(path).exists());
            } else {
                assert_eq!(
                    fs::read(project.join(path)).unwrap(),
                    body.as_str().unwrap().as_bytes()
                );
            }
        }
    }
}

#[test]
fn aud03_ambiguous_hunks_and_overlapping_paths_are_preflight_conflicts() {
    let (_temp, project, data) = roots();
    fs::write(project.join("repeat"), b"same\nsame\n").unwrap();
    let ambiguous = "*** Begin Patch\n*** Add File: first\n+x\n*** Update File: repeat\n@@\n-same\n+new\n*** End Patch";
    let failure = apply_patch(&project, &data, ambiguous, &AllowAll).unwrap_err();
    assert!(failure.done.is_empty());
    assert!(matches!(failure.error, PatchError::Conflict { .. }));
    assert_eq!(fs::read(project.join("repeat")).unwrap(), b"same\nsame\n");
    assert!(!project.join("first").exists());
    for patch in [
        "*** Begin Patch\n*** Add File: a\n+x\n*** Add File: ./a\n+y\n*** End Patch",
        "*** Begin Patch\n*** Add File: a\n+x\n*** Add File: a/b\n+y\n*** End Patch",
        "*** Begin Patch\n*** Update File: repeat\n*** Move to: a\n@@\n-same\n-same\n+x\n*** Add File: a\n+y\n*** End Patch",
    ] {
        let failure = apply_patch(&project, &data, patch, &AllowAll).unwrap_err();
        assert!(failure.done.is_empty());
        assert!(matches!(failure.error, PatchError::Conflict { .. }));
        assert!(!project.join("a").exists());
    }
}

#[test]
fn aud04_parent_symlink_and_replacement_do_not_escape() {
    let (temp, project, data) = roots();
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel"), b"safe\n").unwrap();
    symlink(&outside, project.join("parent")).unwrap();
    let patch = "*** Begin Patch\n*** Update File: parent/sentinel\n@@\n-safe\n+bad\n*** End Patch";
    assert!(apply_patch(&project, &data, patch, &AllowAll).is_err());
    fs::remove_file(project.join("parent")).unwrap();
    fs::create_dir(project.join("parent")).unwrap();
    fs::write(project.join("parent/sentinel"), b"safe\n").unwrap();
    let race = Race {
        path: "parent/sentinel",
        calls: 0.into(),
        action: Box::new(|| {
            fs::rename(project.join("parent"), project.join("old-parent")).unwrap();
            symlink(&outside, project.join("parent")).unwrap();
        }),
    };
    let failure = apply_patch(&project, &data, patch, &race).unwrap_err();
    assert!(failure.done.is_empty());
    assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"safe\n");
    assert_eq!(
        fs::read(project.join("old-parent/sentinel")).unwrap(),
        b"safe\n"
    );
}

#[test]
fn aud04_concurrent_add_and_move_targets_are_not_overwritten() {
    let (_temp, project, data) = roots();
    let race = Race {
        path: "new",
        calls: 0.into(),
        action: Box::new(|| {
            fs::write(project.join("new"), b"concurrent\n").unwrap();
        }),
    };
    let patch = "*** Begin Patch\n*** Add File: new\n+patch\n*** End Patch";
    let failure = apply_patch(&project, &data, patch, &race).unwrap_err();
    assert!(failure.done.is_empty());
    assert_eq!(fs::read(project.join("new")).unwrap(), b"concurrent\n");

    fs::write(project.join("source"), b"before\n").unwrap();
    let race = Race {
        path: "target",
        calls: 0.into(),
        action: Box::new(|| {
            fs::write(project.join("target"), b"concurrent\n").unwrap();
        }),
    };
    let patch = "*** Begin Patch\n*** Update File: source\n*** Move to: target\n@@\n-before\n+after\n*** End Patch";
    let failure = apply_patch(&project, &data, patch, &race).unwrap_err();
    assert_eq!(fs::read(project.join("target")).unwrap(), b"concurrent\n");
    assert_eq!(fs::read(project.join("source")).unwrap(), b"after\n");
    assert_eq!(failure.done.len(), 1);
    let done = &failure.done[0];
    assert_eq!(
        (done.path.as_str(), done.new_path.as_deref(), done.op),
        ("source", None, "update")
    );
    assert_eq!(
        done.hash_before,
        Some(format!("{:x}", Sha256::digest(b"before\n")))
    );
    assert_eq!(
        done.hash_after,
        Some(format!("{:x}", Sha256::digest(b"after\n")))
    );
}

#[test]
fn aud05_concurrent_preimage_is_preserved() {
    let (_temp, project, data) = roots();
    fs::write(project.join("source"), b"before\n").unwrap();
    let race = Race {
        path: "source",
        calls: 0.into(),
        action: Box::new(|| {
            fs::write(project.join("source"), b"concurrent\n").unwrap();
        }),
    };
    let patch = "*** Begin Patch\n*** Update File: source\n@@\n-before\n+after\n*** End Patch";
    let failure = apply_patch(&project, &data, patch, &race).unwrap_err();
    assert!(failure.done.is_empty());
    assert!(matches!(failure.error, PatchError::Conflict { .. }));
    assert_eq!(fs::read(project.join("source")).unwrap(), b"concurrent\n");
}

#[test]
fn aud05_io_failure_after_commit_reports_precise_partial_outcome() {
    let (_temp, project, data) = roots();
    fs::create_dir(project.join("later")).unwrap();
    let race = Race {
        path: "later/second",
        calls: 0.into(),
        action: Box::new(|| {
            fs::remove_dir(project.join("later")).unwrap();
            fs::write(project.join("later"), b"no longer a directory").unwrap();
        }),
    };
    let patch = "*** Begin Patch\n*** Add File: first\n+first\n*** Add File: later/second\n+second\n*** End Patch";
    let failure = apply_patch(&project, &data, patch, &race).unwrap_err();
    assert!(matches!(failure.error, PatchError::Io { .. }));
    assert_eq!(
        (failure.failed_op, failure.failed_path.as_str()),
        (1, "later/second")
    );
    assert_eq!(failure.done.len(), 1);
    let done = &failure.done[0];
    assert_eq!(
        (done.path.as_str(), done.new_path.as_deref(), done.op),
        ("first", None, "add")
    );
    assert_eq!(done.hash_before, None);
    assert_eq!(
        done.hash_after,
        Some(format!("{:x}", Sha256::digest(b"first\n")))
    );
    assert_eq!(fs::read(project.join("first")).unwrap(), b"first\n");
}

#[test]
fn aud05_late_filesystem_permission_failure_is_preflight() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_temp, project, data) = roots();
    let locked = project.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
    let result = apply_patch(
        &project,
        &data,
        "*** Begin Patch\n*** Add File: first\n+first\n*** Add File: locked/second\n+second\n*** End Patch",
        &AllowAll,
    );
    // Restore fixture permissions before assertions/temporary directory cleanup.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    let failure = result.unwrap_err();
    assert!(failure.done.is_empty());
    assert!(!project.join("first").exists());
    assert!(!locked.join("second").exists());
}

#[test]
fn aud03_add_file_decodes_the_patch_plus_prefix() {
    let (_temp, project, data) = roots();
    apply_patch(
        &project,
        &data,
        "*** Begin Patch\n*** Add File: added.txt\n+hello\n*** End Patch\n",
        &AllowAll,
    )
    .unwrap();
    assert_eq!(fs::read(project.join("added.txt")).unwrap(), b"hello\n");
}

#[test]
fn aud04_predictable_temp_symlink_must_not_modify_outside_file() {
    let (temp, project, data) = roots();
    let outside = temp.path().join("outside-sentinel");
    fs::write(&outside, b"DO NOT TOUCH\n").unwrap();
    symlink(
        &outside,
        project.join(format!("new.txt.tmp-{}", std::process::id())),
    )
    .unwrap();
    let result = apply_patch(
        &project,
        &data,
        "*** Begin Patch\n*** Add File: new.txt\n+safe\n*** End Patch\n",
        &AllowAll,
    );
    assert_eq!(fs::read(&outside).unwrap(), b"DO NOT TOUCH\n");
    if result.is_ok() {
        assert!(
            fs::symlink_metadata(project.join("new.txt"))
                .unwrap()
                .is_file()
        );
        assert_eq!(fs::read(project.join("new.txt")).unwrap(), b"safe\n");
    }
}

#[test]
fn aud05_late_stale_hunk_must_not_commit_earlier_file() {
    let (_temp, project, data) = roots();
    fs::write(project.join("second.txt"), b"real\n").unwrap();
    let failure = apply_patch(
        &project, &data,
        "*** Begin Patch\n*** Add File: first.txt\n+first\n*** Update File: second.txt\n@@\n-imagined\n+new\n*** End Patch\n",
        &AllowAll,
    ).unwrap_err();
    assert!(
        failure.done.is_empty(),
        "deterministic preimage failure committed a file"
    );
    assert!(!project.join("first.txt").exists());
    assert_eq!(fs::read(project.join("second.txt")).unwrap(), b"real\n");
}
