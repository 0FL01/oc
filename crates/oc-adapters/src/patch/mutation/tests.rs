use super::*;

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
    let input = Input::Write {
        path: "nested/f",
        content: "\u{feff}λ\r\nEOF",
    };
    let (preview, digest) = preview(&project, &data, &input).unwrap();
    assert!(!project.join("nested").exists());
    assert_eq!(preview.total_files, 1);
    let (result, effects) = execute(&project, &data, &input, &AllowAll, Some(&digest));
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
    let (_, digest) = super::preview(&project, &data, &input).unwrap();
    // Same bytes, different inode: approval cannot authorize the replacement.
    std::fs::rename(project.join("nested/f"), project.join("nested/old")).unwrap();
    std::fs::copy(project.join("nested/old"), project.join("nested/f")).unwrap();
    let (result, effects) = execute(&project, &data, &input, &AllowAll, Some(&digest));
    assert!(result.is_err());
    assert!(effects.files.is_empty());
    let (_, digest) = super::preview(&project, &data, &input).unwrap();
    assert!(
        execute(&project, &data, &input, &AllowAll, Some(&digest))
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
    let (_, digest) = super::preview(&project, &data, &input).unwrap();
    std::fs::write(project.join("absent"), "foreign").unwrap();
    let (result, effects) = execute(&project, &data, &input, &AllowAll, Some(&digest));
    assert!(result.is_err());
    assert!(effects.files.is_empty());
    assert_eq!(std::fs::read(project.join("absent")).unwrap(), b"foreign");
}
