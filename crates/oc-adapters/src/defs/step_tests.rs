//! R6 Long Horizon source diagnostics, independent of remaining profile parity.
use super::*;
use oc_core::queries::{ServiceCode, ServiceStage};

#[test]
fn step_constraints_are_typed_source_fields_not_silent_profile_limits() {
    for domain in ["agent", "agents"] {
        let mut defs = LoadedDefs::default();
        merge_config_definitions(
            &mut defs,
            &serde_json::json!({domain:{"constrained":{"steps":16,"maxSteps":8},
                                      "sibling":{"prompt":"unchanged"}}}),
            "/admitted/opencode.jsonc",
        );
        assert!(!defs.agents.contains_key("constrained"));
        if domain == "agent" {
            assert_eq!(defs.agents["sibling"].body, "unchanged");
        }
        assert_eq!(defs.diagnostics.len(), 2);
        for diagnostic in &defs.diagnostics {
            assert_eq!(diagnostic.path, "/admitted/opencode.jsonc");
            assert_eq!(diagnostic.failure.code, ServiceCode::UnsupportedCapability);
            assert_eq!(diagnostic.failure.stage, ServiceStage::Capability);
            assert!(!diagnostic.failure.source.is_empty());
            assert_eq!(diagnostic.failure.field[0], domain);
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
    std::fs::write(&path, "---\nsteps: 16\nmaxSteps: 8\n---\nbody").unwrap();
    let defs = load_definitions(&[DefRoot {
        dir: dir.path().into(),
        origin: "fixture".into(),
    }]);
    assert!(!defs.agents.contains_key("constrained"));
    assert_eq!(defs.diagnostics.len(), 2);
    for diagnostic in defs.diagnostics {
        assert_eq!(diagnostic.path, path.to_string_lossy());
        assert_eq!(diagnostic.failure.code, ServiceCode::UnsupportedCapability);
        assert_eq!(diagnostic.failure.field[..2], ["agent", "entry"]);
        assert!(matches!(
            diagnostic.failure.field[2].as_str(),
            "steps" | "maxSteps"
        ));
    }
}
