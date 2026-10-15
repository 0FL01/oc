use super::*;
use std::time::Duration;

async fn changed(watch: &SourceWatch) -> Change {
    tokio::time::timeout(Duration::from_secs(2), watch.changed())
        .await
        .expect("bounded admitted watch event")
}

async fn quiet(watch: &SourceWatch) {
    assert!(
        tokio::time::timeout(Duration::from_millis(60), watch.changed())
            .await
            .is_err()
    );
}

#[test]
fn hot02_source_watch_filters_shapes_and_marks_overflow_without_paths() {
    let mut inputs = WatchInputs::default();
    inputs.insert(Path::new("/known/opencode.json"), InputKind::File);
    inputs.insert(Path::new("/known/agents"), InputKind::AgentTree);
    inputs.insert(Path::new("/known/commands"), InputKind::MarkdownDirectory);
    inputs.insert(Path::new("/known/skills"), InputKind::Skills);
    inputs.insert(Path::new("/aliases/kit"), InputKind::SkillDirectory);
    for (path, directory, expected) in [
        ("/known/opencode.json", false, true),
        ("/known/cli.json", false, false),
        ("/known/.env", false, false),
        ("/known/oc.sqlite-wal", false, false),
        ("/known/agents/deep/a.md", false, true),
        ("/known/agents/deep", true, true),
        ("/known/agents/a.txt", false, false),
        ("/known/commands/a.md", false, true),
        ("/known/commands/deep/a.md", false, false),
        ("/known/commands/deep", true, false),
        ("/known/skills/a.md", false, true),
        ("/known/skills/kit", true, true),
        ("/known/skills/kit/SKILL.md", false, true),
        ("/known/skills/kit/body.txt", false, false),
        ("/known/skills/deep/kit/SKILL.md", false, false),
        ("/aliases/kit/SKILL.md", false, true),
        ("/aliases/kit/private.txt", false, false),
        ("/known", true, true),
        ("/unrelated", true, false),
    ] {
        assert_eq!(
            matches_input(&inputs, Path::new(path), directory),
            expected,
            "{path}"
        );
    }
    let dirty = Dirty::default();
    mark_event(
        &inputs,
        &dirty,
        Ok(
            Event::new(EventKind::Access(notify::event::AccessKind::Any))
                .add_path(PathBuf::from("/known/opencode.json")),
        ),
    );
    assert_eq!(dirty.bits.load(Ordering::Acquire), 0);
    mark_event(
        &inputs,
        &dirty,
        Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan)),
    );
    assert_eq!(dirty.bits.load(Ordering::Acquire), DIRTY | REARM);
    let epoch = dirty.epoch.load(Ordering::Acquire);
    mark_event(
        &inputs,
        &dirty,
        Err(notify::Error::generic("PRIVATE_ERROR_CANARY")),
    );
    assert!(dirty.epoch.load(Ordering::Acquire) > epoch);
    assert_eq!(dirty.bits.load(Ordering::Acquire), DIRTY | REARM);
    // Malformed frame input cannot become unchecked pointer arithmetic or paths.
    assert!(decode(&[0; 3], &BTreeMap::new(), &mut |_| {}).is_err());
}

#[tokio::test]
async fn hot02_source_watch_observes_writes_atomic_rename_and_joins_without_access_loops() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("opencode.json");
    std::fs::write(&file, "{}").unwrap();
    let mut inputs = WatchInputs::default();
    inputs.insert(&file, InputKind::File);
    let watch = SourceWatch::start(&inputs).unwrap();
    let ready = changed(&watch).await;
    let ready_epoch = watch.epoch();
    assert_eq!(ready_epoch, 1);
    assert!(!ready.failed);
    for _ in 0..10 {
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
        std::fs::write(root.path().join("cli.json"), "CLIENT_ONLY_CANARY").unwrap();
        std::fs::write(root.path().join("oc.sqlite-wal"), "unrelated").unwrap();
    }
    quiet(&watch).await;
    std::fs::write(&file, "{\"theme\":\"native-source\"}").unwrap();
    let write = changed(&watch).await;
    assert!(watch.epoch() > ready_epoch);
    assert!(!write.failed);
    // Drain only coalesced notifications, never a lifetime event history.
    tokio::time::sleep(Duration::from_millis(10)).await;
    let _ = tokio::time::timeout(Duration::from_millis(10), watch.changed()).await;
    let temporary = root.path().join("atomic-config.tmp");
    std::fs::write(&temporary, "{}").unwrap();
    std::fs::rename(&temporary, &file).unwrap();
    let rename = changed(&watch).await;
    assert!(rename.rearm);
    watch.reconcile(&inputs, true).await.unwrap();
    assert!(!changed(&watch).await.failed); // readiness requires a rescan
    let stable = watch.epoch();
    watch.reconcile(&inputs, false).await.unwrap();
    assert_eq!(watch.epoch(), stable); // same topology does not wake itself forever
    quiet(&watch).await;
    let task = watch.task.as_ref().unwrap().abort_handle();
    watch.shutdown().await.unwrap();
    assert!(task.is_finished());
}

#[tokio::test]
async fn hot02_source_watch_rearms_missing_global_recursive_definitions_and_recreated_roots() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let global = root.path().join("missing/global");
    let env = BTreeMap::from([(
        "OPENCODE_CONFIG_DIR".to_owned(),
        global.to_string_lossy().into_owned(),
    )]);
    let load = || crate::composition::load_reload_local_with_env(&project, env.clone());
    let initial = load().await.unwrap();
    let watch = SourceWatch::start(&initial.watch_inputs).unwrap();
    changed(&watch).await;
    assert!(!global.exists());
    assert!(!project.join(".opencode").exists());
    std::fs::create_dir_all(global.join("agents/deep")).unwrap();
    std::fs::write(
        global.join("agents/deep/worker.md"),
        "---\ndescription: worker\nmode: subagent\n---\nWORKER",
    )
    .unwrap();
    assert!(changed(&watch).await.rearm);
    let next = load().await.unwrap();
    assert!(next.agents.contains_key("deep/worker"));
    watch.reconcile(&next.watch_inputs, true).await.unwrap();
    changed(&watch).await;
    std::fs::write(
        global.join("agents/deep/worker.md"),
        "---\ndescription: changed\nmode: subagent\n---\nWORKER",
    )
    .unwrap();
    assert!(!changed(&watch).await.failed);
    assert!(watch.epoch() > 1);
    let local = project.join(".opencode");
    std::fs::create_dir_all(local.join("skills/kit")).unwrap();
    // A supported empty skill directory must be watched before SKILL.md exists.
    changed(&watch).await;
    let next = load().await.unwrap();
    watch.reconcile(&next.watch_inputs, true).await.unwrap();
    changed(&watch).await;
    std::fs::write(
        local.join("skills/kit/SKILL.md"),
        "---\ndescription: kit\n---\nBODY",
    )
    .unwrap();
    changed(&watch).await;
    std::fs::rename(&local, project.join("retired-inputs")).unwrap();
    assert!(changed(&watch).await.rearm);
    let absent = load().await.unwrap();
    watch.reconcile(&absent.watch_inputs, true).await.unwrap();
    changed(&watch).await;
    std::fs::create_dir(&local).unwrap();
    assert!(changed(&watch).await.rearm);
    let recreated = load().await.unwrap();
    watch
        .reconcile(&recreated.watch_inputs, true)
        .await
        .unwrap();
    changed(&watch).await;
    std::fs::write(
        local.join("opencode.json"),
        "{\"tool_output\":{\"max_lines\":7}}",
    )
    .unwrap();
    changed(&watch).await;
    assert_eq!(load().await.unwrap().generation.tool_output.max_lines, 7);
    watch.shutdown().await.unwrap();
}

#[tokio::test]
async fn hot02_source_watch_failed_replacement_retains_live_observation_and_abort_is_non_success() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("opencode.json");
    let mut inputs = WatchInputs::default();
    inputs.insert(&file, InputKind::File);
    let watch = SourceWatch::start(&inputs).unwrap();
    changed(&watch).await;
    let mut oversized = inputs.clone();
    for index in 0..=super::super::MAX_TARGETS {
        oversized.insert(&root.path().join(format!("{index}.json")), InputKind::File);
    }
    let failure = watch.reconcile(&oversized, false).await.unwrap_err();
    assert_eq!(failure.code, ServiceCode::CapacityExceeded);
    assert!(
        !failure
            .to_string()
            .contains(&root.path().to_string_lossy().to_string())
    );
    assert!(changed(&watch).await.rearm);
    std::fs::write(&file, "{}").unwrap();
    assert!(!changed(&watch).await.failed);
    watch.task.as_ref().unwrap().abort();
    assert!(changed(&watch).await.failed);
    assert!(watch.shutdown().await.is_err());

    // A failed extra observation can leave the same retained target set. The
    // semantic no-op fast path must not hide the collector's capacity failure.
    let mut full = WatchInputs::default();
    for index in 0..super::super::MAX_TARGETS {
        full.insert(&root.path().join(format!("{index}.json")), InputKind::File);
    }
    let watch = SourceWatch::start(&full).unwrap();
    changed(&watch).await;
    let mut over = full.clone();
    over.insert(&root.path().join("extra.json"), InputKind::File);
    assert!(over.targets == full.targets);
    let failure = watch.reconcile(&over, false).await.unwrap_err();
    assert_eq!(failure.code, ServiceCode::CapacityExceeded);
    watch.shutdown().await.unwrap();
}
