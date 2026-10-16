//! R6 Long Horizon source diagnostics, independent of remaining profile parity.
use super::*;
use oc_core::queries::{ServiceCode, ServiceStage};

#[test]
fn step_constraints_are_typed_source_fields_not_silent_profile_limits() {
    {
        let domain = "agents";
        let mut defs = LoadedDefs::default();
        merge_config_definitions(
            &mut defs,
            &serde_json::json!({domain:{"constrained":{"steps":16},
                                      "sibling":{"system":"unchanged"}}}),
            "/admitted/opencode.jsonc",
        );
        assert!(!defs.agents.contains_key("constrained"));
        assert_eq!(defs.agents["sibling"].body, "unchanged");
        assert_eq!(defs.diagnostics.len(), 1);
        for diagnostic in &defs.diagnostics {
            assert_eq!(diagnostic.path, "/admitted/opencode.jsonc");
            assert_eq!(diagnostic.failure.code, ServiceCode::UnsupportedCapability);
            assert_eq!(diagnostic.failure.stage, ServiceStage::Capability);
            assert!(!diagnostic.failure.source.is_empty());
            assert_eq!(diagnostic.failure.field[0], "agent");
            assert_eq!(diagnostic.failure.field[1], "entry");
            assert!(matches!(
                diagnostic.failure.field[2].as_str(),
                "steps" | "maxSteps"
            ));
        }
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("agents")).unwrap();
    let path = dir.path().join("agents/constrained.md");
    // Canonical `steps` is the Long Horizon refusal; `maxSteps` is an unknown key (T66).
    std::fs::write(&path, "---\nsteps: 16\n---\nbody").unwrap();
    let defs = load_definitions(&[DefRoot {
        dir: dir.path().into(),
        origin: "fixture".into(),
    }]);
    assert!(!defs.agents.contains_key("constrained"));
    assert_eq!(defs.diagnostics.len(), 1);
    let diagnostic = &defs.diagnostics[0];
    assert_eq!(diagnostic.path, path.to_string_lossy());
    assert_eq!(diagnostic.failure.code, ServiceCode::UnsupportedCapability);
    assert_eq!(diagnostic.failure.field, ["agent", "entry", "steps"]);
}
