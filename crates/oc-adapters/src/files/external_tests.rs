use super::*;
use std::sync::atomic::AtomicBool;

#[test]
fn external_descriptor_is_invocation_only_nofollow_and_data_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let cache = tmp.path().join("cache");
    let data = cache.join("private");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(cache.join("fact.txt"), "SENTINEL\n").unwrap();
    std::fs::write(data.join("secret.txt"), "PROTECTED\n").unwrap();
    let files = Files::new(&project, &data).unwrap();
    assert_eq!(
        files.read(cache.join("fact.txt").to_str().unwrap(), 1, 1),
        Err(FileToolError::OutsideRoot)
    );
    let admitted = files.pin_external(&cache, true).unwrap();
    assert_eq!(
        admitted.resolve_path(cache.join("fact.txt").to_str().unwrap()),
        Err(FileToolError::OutsideRoot)
    );
    assert_eq!(
        admitted.external_resource().unwrap(),
        cache.join("*").to_string_lossy()
    );
    assert_eq!(
        admitted
            .read(cache.join("fact.txt").to_str().unwrap(), 1, 1)
            .unwrap()
            .lines,
        ["SENTINEL"]
    );
    assert_eq!(
        admitted.read(data.join("secret.txt").to_str().unwrap(), 1, 1),
        Err(FileToolError::OwnDataRoot)
    );
    assert!(
        admitted
            .grep_search(
                &search::GrepOptions {
                    pattern: "PROTECTED",
                    path: cache.to_str().unwrap(),
                    include: None,
                    literal: true,
                    case_sensitive: true,
                    offset: 0,
                    limit: 10
                },
                |_| true,
                None
            )
            .unwrap()
            .is_empty()
    );
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        admitted.read_admitted(
            cache.join("fact.txt").to_str().unwrap(),
            1,
            1,
            |_| true,
            &cancelled
        ),
        Err(FileToolError::Cancelled)
    ));
    // Already pinned directory survives replacement. A fresh invocation refuses
    // symlink ancestors; neither path ever reads the replacement's contents.
    let parked = tmp.path().join("parked");
    let escape = tmp.path().join("escape");
    std::fs::create_dir(&escape).unwrap();
    std::fs::write(escape.join("fact.txt"), "ESCAPE").unwrap();
    std::fs::rename(&cache, &parked).unwrap();
    std::os::unix::fs::symlink(&escape, &cache).unwrap();
    assert_eq!(
        admitted
            .read(cache.join("fact.txt").to_str().unwrap(), 1, 1)
            .unwrap()
            .lines,
        ["SENTINEL"]
    );
    assert!(matches!(
        files.pin_external(&cache.join("fact.txt"), false),
        Err(FileToolError::SymlinkEscape)
    ));
    // No automatic sibling or external mutation/root authority persists.
    assert_eq!(
        files.resolve_path(parked.join("fact.txt").to_str().unwrap()),
        Err(FileToolError::OutsideRoot)
    );
    assert_eq!(
        admitted.read(escape.join("fact.txt").to_str().unwrap(), 1, 1),
        Err(FileToolError::OutsideRoot)
    );
}
