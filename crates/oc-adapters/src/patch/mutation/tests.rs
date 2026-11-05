use super::*;

#[test]
fn w8_write_binary_and_large_preimages_still_bind_every_approved_byte() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let data = temp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let files = Files::new(&project, &data).unwrap();
    let mut refused = Vec::new();
    for (path, mut before) in [
        ("nul", b"old\0bytes".to_vec()),
        ("invalid", vec![0xff, 0xfe]),
        ("large", vec![b'a'; 9 * 1024 * 1024]),
    ] {
        let target = project.join(path);
        std::fs::write(&target, &before).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(
            preview(
                &files,
                &Input::Edit {
                    path,
                    old: "a",
                    new: "b",
                    all: false
                }
            )
            .is_err()
        );
        let input = Input::Write {
            path,
            content: "replacement\n",
        };
        let digest = match preview(&files, &input) {
            Ok((_, digest)) => digest,
            Err(error) => {
                refused.push((path, error.to_string()));
                continue;
            }
        };
        *before.last_mut().unwrap() ^= 1;
        std::fs::write(&target, &before).unwrap();
        let (result, effects) = execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {});
        assert!(result.is_err() && effects.files.is_empty());
        assert!(
            std::fs::read(&target).unwrap() == before,
            "stale refusal changed {path}"
        );
        let (_, digest) = preview(&files, &input).unwrap();
        assert!(
            execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {})
                .0
                .is_ok()
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"replacement\n");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
    assert!(
        refused.is_empty(),
        "overwrite must not text-parse or cap old preimages at 8 MiB: {refused:?}"
    );
}

#[test]
fn w2_write_edit_follow_symlinks_and_bind_approval_to_referent() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let data = temp.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&data).unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    std::fs::write(&a, "before\n").unwrap();
    std::fs::write(&b, "before\n").unwrap();
    let link = project.join("link");
    std::os::unix::fs::symlink(&a, &link).unwrap();
    let files = Files::new(&project, &data).unwrap();
    let input = Input::Write {
        path: "link",
        content: "after\n",
    };
    let (_, digest) = preview(&files, &input).unwrap();
    assert!(
        execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {})
            .0
            .is_ok()
    );
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read(&a).unwrap(), b"after\n");
    let input = Input::Edit {
        path: "link",
        old: "after",
        new: "edited",
        all: false,
    };
    assert!(
        execute(&files, &input, &AllowAll, None, &mut |_| {})
            .0
            .is_ok()
    );
    assert_eq!(std::fs::read(&a).unwrap(), b"edited\n");
    std::fs::write(&a, "before\n").unwrap();
    let input = Input::Write {
        path: "link",
        content: "unapproved\n",
    };
    let (_, digest) = preview(&files, &input).unwrap();
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&b, &link).unwrap();
    let (result, effects) = execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {});
    assert!(result.is_err() && effects.files.is_empty());
    assert_eq!(std::fs::read(&a).unwrap(), b"before\n");
    assert_eq!(std::fs::read(&b).unwrap(), b"before\n");
    std::fs::remove_file(&link).unwrap();
    std::fs::write(data.join("private"), "private\n").unwrap();
    std::os::unix::fs::symlink(data.join("private"), &link).unwrap();
    assert!(matches!(
        preview(&files, &input),
        Err(PatchError::OwnDataRoot { .. })
    ));
}

#[test]
fn tool20_matching_precedence_offsets_crlf_eof() {
    for (source, old, new, all, want, count) in [
        ("a a", "a", "", true, " ", 2),
        ("aaa", "aa", "b", true, "ba", 1),
        ("‘x’ 'x'", "'x'", "y", false, "‘x’ y", 1),
        ("λ ‘x’ — z", "'x' -", "q", false, "λ q z", 1),
        ("a  \nb\t\n", "a\nb\n", "z\n", false, "z\n", 1),
        ("a  \nb\t\n", "a\nb", "z", false, "z\n", 1),
        ("a \r\nb\t\r\n", "a\nb", "z\nq", false, "z\r\nq\r\n", 1),
        ("a\r\nb", "a\nb", "x\ny", false, "x\r\ny", 1),
        ("a  ", "a\n", "x", false, "", 0),
    ] {
        if count == 0 {
            assert!(replace(source, old, new, all).is_err());
        } else {
            assert_eq!(
                replace(source, old, new, all).unwrap(),
                (want.into(), count),
                "source={source:?}"
            );
        }
    }
    assert!(
        replace("x x", "x", "y", false)
            .unwrap_err()
            .contains("2 matches")
    );
    assert!(replace("x", "absent", "y", false).is_err());
    let huge_search = "\n".repeat(oc_core::patch::EFFECT_INDEX_LINES_CAP + 1) + "missing";
    assert!(
        replace("unchanged", &huge_search, "x", false)
            .unwrap_err()
            .contains("index exceeds budget")
    );
    for args in [
        json!({"path":"f","oldString":"","newString":"x"}),
        json!({"path":"f","oldString":"x","newString":"x"}),
    ] {
        assert!(Input::parse("edit", &args).is_err());
    }
}

#[test]
fn tool20_prepared_identity_absence_and_confirmed_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let files = Files::new(&project, &data).unwrap();
    let input = Input::Write {
        path: "nested/f",
        content: "\u{feff}λ\r\nEOF",
    };
    let (preview, digest) = preview(&files, &input).unwrap();
    assert!(!project.join("nested").exists());
    assert_eq!(preview.total_files, 1);
    let (result, effects) = execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {});
    assert_eq!(result.unwrap()["existed"], false);
    assert_eq!(effects.total_files, 1);
    assert_eq!(
        std::fs::read(project.join("nested/f")).unwrap(),
        "\u{feff}λ\r\nEOF".as_bytes()
    );
    let input = Input::Edit {
        path: "nested/f",
        old: "λ\nEOF",
        new: "",
        all: false,
    };
    let (_, digest) = super::preview(&files, &input).unwrap();
    // Same bytes, different inode: approval cannot authorize the replacement.
    std::fs::rename(project.join("nested/f"), project.join("nested/old")).unwrap();
    std::fs::copy(project.join("nested/old"), project.join("nested/f")).unwrap();
    let (result, effects) = execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {});
    assert!(result.is_err());
    assert!(effects.files.is_empty());
    let (_, digest) = super::preview(&files, &input).unwrap();
    assert!(
        execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {})
            .0
            .is_ok()
    );
    assert_eq!(
        std::fs::read(project.join("nested/f")).unwrap(),
        b"\xef\xbb\xbf"
    );
    let input = Input::Write {
        path: "absent",
        content: "",
    };
    let (_, digest) = super::preview(&files, &input).unwrap();
    std::fs::write(project.join("absent"), "foreign").unwrap();
    let (result, effects) = execute(&files, &input, &AllowAll, Some(&digest), &mut |_| {});
    assert!(result.is_err());
    assert!(effects.files.is_empty());
    assert_eq!(std::fs::read(project.join("absent")).unwrap(), b"foreign");
}
