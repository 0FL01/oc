use super::*;

fn workspace(owner: &Runtime<'_>, label: &str, permission: Permission) {
    owner
        .publish_workspace(
            Some(label),
            &format!("{label} instructions"),
            vec![(
                "manual".into(),
                format!("---\nname: manual\ndescription: {label} metadata\n---\n{label} body\n"),
            )],
            BTreeMap::new(),
            Some(label.into()),
            Some("build".into()),
            None,
            BTreeMap::from([("read".into(), permission)]),
            Default::default(),
            Default::default(),
        )
        .unwrap();
}

#[test]
fn hot01_captured_publication_keeps_its_workspace_and_policy() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let owner = binding_replay_tests::owner(&db, project.path());
    workspace(&owner, "G1", Permission::Allow);
    let captured = owner.current.read().unwrap().clone();
    let issuing = owner.primary_lane(&captured);

    workspace(&owner, "G2", Permission::Deny);
    owner
        .reload_dcp(DcpConfig {
            debug: !captured.dcp_config.debug,
            ..captured.dcp_config.clone()
        })
        .unwrap();
    owner
        .publish_dcp_protection(ProtectedSpec {
            protect_tags: !captured.dcp_protected.protect_tags,
            ..captured.dcp_protected.clone()
        })
        .unwrap();
    owner
        .publish_provider_state(crate::composition::ProviderState::new(
            "fixture", "G2", false, false,
        ))
        .unwrap();
    let still_issuing = owner.primary_lane(&captured);
    assert_eq!(still_issuing.fixed_input, issuing.fixed_input);
    assert_eq!(still_issuing.agent_digest, issuing.agent_digest);
    assert_eq!(
        RuntimePolicy::with_rules(&still_issuing.permissions, &still_issuing.permission_rules)
            .effect("read", "file"),
        Permission::Allow,
    );

    let current = owner.current.read().unwrap().clone();
    assert!(!Arc::ptr_eq(&captured, &current));
    assert!(
        captured.workspace.skills.entries["manual"]
            .body
            .contains("G1 body")
    );
    assert!(
        current.workspace.skills.entries["manual"]
            .body
            .contains("G2 body")
    );
    assert_ne!(captured.dcp_config.debug, current.dcp_config.debug);
    assert_ne!(
        captured.dcp_protected.protect_tags,
        current.dcp_protected.protect_tags
    );
    assert!(captured.provider_state.is_none());
    assert!(current.provider_state.is_some());
    let fresh = owner.primary_lane(&current);
    assert_ne!(fresh.fixed_input, issuing.fixed_input);
    assert_eq!(fresh.agent_digest.as_deref(), Some("G2"));
    assert_eq!(
        RuntimePolicy::with_rules(&fresh.permissions, &fresh.permission_rules)
            .effect("read", "file"),
        Permission::Deny,
    );
    assert!(db.list_sessions().unwrap().is_empty());
}
