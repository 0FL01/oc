//! VIS35 independent byte/mode checks alongside confirmed result metadata.
use oc_adapters::patch::{AllowAll, PatchError, WritePolicy, apply_patch_with_effects};
use oc_core::patch::{
    DiffAlgorithm, EFFECT_INDEX_LINES_CAP, EFFECT_LINES_CAP, LineEnding, PatchOperation,
};
use std::{cell::Cell, fs, os::unix::fs::PermissionsExt};

#[test]
fn vis35_actual_positions_empty_create_multihunk_move_delete_replace() {
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("src"),
        b"zero\r\none\r\ntwo\r\nthree\r\nfour",
    )
    .unwrap();
    fs::set_permissions(root.path().join("src"), fs::Permissions::from_mode(0o751)).unwrap();
    fs::write(root.path().join("gone"), "α\nbeta").unwrap();
    let patch = "*** Begin Patch\n*** Add File: empty\n*** Update File: src\n*** Move to: dst\n@@\n-one\n+ONE\n@@\n-three\n+THREE\n*** Delete File: gone\n*** End Patch";
    let (result, effects) = apply_patch_with_effects(root.path(), data.path(), patch, &AllowAll);
    assert_eq!(result.unwrap().len(), 3);
    assert_eq!(fs::read(root.path().join("empty")).unwrap(), b"");
    assert_eq!(
        fs::read(root.path().join("dst")).unwrap(),
        b"zero\r\nONE\r\ntwo\r\nTHREE\r\nfour"
    );
    assert_eq!(
        fs::metadata(root.path().join("dst"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o751
    );
    assert!(!root.path().join("src").exists());
    assert!(!root.path().join("gone").exists());
    assert_eq!((effects.additions, effects.deletions), (2, 4));
    assert_eq!(effects.files[0].operation, PatchOperation::Create);
    assert!(effects.files[0].hunks.is_empty());
    let moved = &effects.files[1];
    assert_eq!(moved.operation, PatchOperation::Move);
    assert_eq!(moved.destination.as_deref(), Some("dst"));
    assert_eq!(moved.hunks.len(), 1);
    assert_eq!((moved.hunks[0].old.start, moved.hunks[0].new.start), (1, 1));
    assert_eq!(
        moved.hunks[0].lines[0].kind,
        oc_core::patch::PatchLineKind::Context
    );
    let added = moved.hunks[0]
        .lines
        .iter()
        .find(|l| l.text == "ONE")
        .unwrap();
    assert_eq!(added.new_line, Some(2));
    assert_eq!(added.ending, LineEnding::CrLf);
    assert_eq!(effects.files[2].hunks[0].lines[1].ending, LineEnding::None);
    let (result, effects) = apply_patch_with_effects(
        root.path(),
        data.path(),
        "*** Begin Patch\n*** Update File: dst\n@@\n-zero\n-ONE\n-two\n-THREE\n-four\n+replacement\n*** End Patch",
        &AllowAll,
    );
    result.unwrap();
    assert_eq!(fs::read(root.path().join("dst")).unwrap(), b"replacement");
    assert_eq!((effects.additions, effects.deletions), (1, 5));
    assert!(
        effects.files[0].hunks[0]
            .lines
            .iter()
            .any(|l| l.text == "replacement" && l.ending == LineEnding::None)
    );
    let (result, effects) = apply_patch_with_effects(
        root.path(),
        data.path(),
        "*** Begin Patch\n*** Update File: empty\n@@\n+first\n+second\n*** End Patch",
        &AllowAll,
    );
    result.unwrap();
    assert_eq!(
        fs::read(root.path().join("empty")).unwrap(),
        b"first\nsecond\n"
    );
    assert_eq!(effects.files[0].hunks[0].old.count, 0);
    assert_eq!(effects.files[0].hunks[0].lines[1].new_line, Some(2));
}

#[test]
fn vis35_postcommit_move_failure_and_unapplied_tail_are_honest() {
    struct DenyMove(Cell<usize>);
    impl WritePolicy for DenyMove {
        fn check(&self, path: &str) -> Result<(), PatchError> {
            if path == "dst" {
                self.0.set(self.0.get() + 1);
                if self.0.get() == 2 {
                    return Err(PatchError::Denied { path: path.into() });
                }
            }
            Ok(())
        }
    }
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::write(root.path().join("src"), b"old\n").unwrap();
    fs::set_permissions(root.path().join("src"), fs::Permissions::from_mode(0o740)).unwrap();
    let (result, effects) = apply_patch_with_effects(
        root.path(),
        data.path(),
        "*** Begin Patch\n*** Add File: first\n+ok\n*** Update File: src\n*** Move to: dst\n@@\n-old\n+new\n*** Add File: tail\n+never\n*** End Patch",
        &DenyMove(Cell::new(0)),
    );
    let failure = result.unwrap_err();
    assert_eq!(failure.done.len(), 2);
    assert_eq!(effects.total_files, 2);
    assert_eq!(effects.files[1].operation, PatchOperation::Update);
    assert_eq!(effects.files[1].destination, None);
    assert_eq!(fs::read(root.path().join("src")).unwrap(), b"new\n");
    assert_eq!(fs::read(root.path().join("first")).unwrap(), b"ok\n");
    assert_eq!(
        fs::metadata(root.path().join("src"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o740
    );
    assert!(!root.path().join("dst").exists());
    assert!(!root.path().join("tail").exists());
    let (result, effects) = apply_patch_with_effects(
        root.path(),
        data.path(),
        "*** Begin Patch\n*** Update File: src\n@@\n-stale\n+fake\n*** End Patch",
        &AllowAll,
    );
    assert!(result.is_err());
    assert!(effects.files.is_empty());
    assert_eq!(effects.total_files, 0);
}

#[test]
fn vis35_caps_preserve_counts_utf8_and_explicit_streaming_fallback() {
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let content = "界".repeat(600);
    let mut patch = format!("*** Begin Patch\n*** Add File: long\n+{content}\n");
    for _ in 0..200 {
        patch.push_str("+line\n");
    }
    patch.push_str("*** End Patch");
    let (result, effects) = apply_patch_with_effects(root.path(), data.path(), &patch, &AllowAll);
    result.unwrap();
    assert_eq!(effects.additions, 201);
    assert!(effects.truncated);
    assert_eq!(effects.files[0].hunks[0].lines.len(), EFFECT_LINES_CAP);
    assert!(effects.files[0].hunks[0].lines[0].truncated);
    assert!(effects.files[0].hunks[0].lines[0].text.ends_with('界'));
    let wide_patch = format!(
        "*** Begin Patch\n*** Add File: wide\n{}*** End Patch",
        format!("+{content}\n").repeat(EFFECT_LINES_CAP)
    );
    let (result, bounded) =
        apply_patch_with_effects(root.path(), data.path(), &wide_patch, &AllowAll);
    result.unwrap();
    let retained: usize = bounded
        .files
        .iter()
        .flat_map(|f| &f.hunks)
        .flat_map(|h| &h.lines)
        .map(|line| line.text.len())
        .sum();
    assert!(retained <= oc_core::patch::EFFECT_PREVIEW_BYTES_CAP);
    assert!(
        serde_json::to_vec(&bounded).unwrap().len() <= oc_core::patch::EFFECT_PREVIEW_BYTES_CAP
    );
    assert_eq!(bounded.additions, EFFECT_LINES_CAP);
    assert!(bounded.truncated);
    fs::write(
        root.path().join("huge"),
        "a\n".repeat(EFFECT_INDEX_LINES_CAP + 1),
    )
    .unwrap();
    let (result, effects) = apply_patch_with_effects(
        root.path(),
        data.path(),
        "*** Begin Patch\n*** Delete File: huge\n*** End Patch",
        &AllowAll,
    );
    result.unwrap();
    assert_eq!(effects.deletions, EFFECT_INDEX_LINES_CAP + 1);
    assert_eq!(
        effects.files[0].algorithm,
        DiffAlgorithm::StreamingReplacement
    );
    assert!(effects.truncated);
    assert!(!root.path().join("huge").exists());
    fs::write(
        root.path().join("big_update"),
        format!("{}tail\n", "a\n".repeat(EFFECT_INDEX_LINES_CAP + 1)),
    )
    .unwrap();
    let (result, effects) = apply_patch_with_effects(
        root.path(),
        data.path(),
        "*** Begin Patch\n*** Update File: big_update\n@@\n-tail\n+TAIL\n*** End Patch",
        &AllowAll,
    );
    result.unwrap();
    assert_eq!((effects.additions, effects.deletions), (1, 1));
    assert_eq!(
        effects.files[0].hunks[0].new.start,
        EFFECT_INDEX_LINES_CAP - 2
    );
    assert!(!effects.truncated);
}

#[test]
fn vis35_file_caps_and_shifted_new_positions_keep_exact_totals() {
    let root = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    fs::write(root.path().join("shift"), b"head\na\nmiddle\nb\nend\n").unwrap();
    let (result, effects) = apply_patch_with_effects(
        root.path(),
        data.path(),
        "*** Begin Patch\n*** Update File: shift\n@@\n-a\n+A\n+extra\n@@\n-b\n+B\n*** End Patch",
        &AllowAll,
    );
    result.unwrap();
    assert_eq!(
        fs::read(root.path().join("shift")).unwrap(),
        b"head\nA\nextra\nmiddle\nB\nend\n"
    );
    assert_eq!((effects.additions, effects.deletions), (3, 2));
    assert_eq!(effects.files[0].hunks.len(), 1);
    let b = effects.files[0].hunks[0]
        .lines
        .iter()
        .find(|l| l.text == "B")
        .unwrap();
    assert_eq!(b.new_line, Some(5));
    let b = effects.files[0].hunks[0]
        .lines
        .iter()
        .find(|l| l.text == "b")
        .unwrap();
    assert_eq!(b.old_line, Some(4));
    let mut patch = String::from("*** Begin Patch\n");
    for i in 0..10 {
        patch.push_str(&format!("*** Add File: f{i}\n+value\n"));
    }
    patch.push_str("*** End Patch");
    let (result, effects) = apply_patch_with_effects(root.path(), data.path(), &patch, &AllowAll);
    assert_eq!(result.unwrap().len(), 10);
    assert_eq!(effects.total_files, 10);
    assert_eq!(effects.files.len(), oc_core::patch::EFFECT_FILES_CAP);
    assert_eq!(effects.additions, 10);
    assert!(effects.truncated);
    for i in 0..10 {
        assert_eq!(
            fs::read(root.path().join(format!("f{i}"))).unwrap(),
            b"value\n"
        );
        assert_eq!(
            fs::metadata(root.path().join(format!("f{i}")))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
