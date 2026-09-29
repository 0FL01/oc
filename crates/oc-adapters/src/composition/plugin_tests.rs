//! CFG09: the presentation window never becomes an admission/activation cap.
use super::*;
use serde_json::json;

#[tokio::test]
async fn cfg09_bounded_inventory_classifies_later_aliases_and_counts_omitted_failures() {
    let root = tempfile::tempdir().unwrap();
    let mut aliases = vec!["@tarquinen/opencode-dcp@3.1.16"; 65];
    aliases.push("@tarquinen/opencode-dcp");
    std::fs::write(
        root.path().join("opencode.json"),
        json!({
            "model":"fixture/m", "provider":{"fixture":{
                "options":{"baseURL":"https://example.invalid/v1","apiKey":"dummy"},
                "models":{"m":{}}
            }}, "plugin":aliases
        })
        .to_string(),
    )
    .unwrap();
    let composition = load_with_env(root.path(), BTreeMap::new()).await.unwrap();
    let inventory = &composition.tui_chrome.plugins;
    assert_eq!(inventory.entries.len(), 64);
    assert_eq!(inventory.omitted, 2);
    assert_eq!(inventory.omitted_failed, 1);
    assert_eq!(inventory.active_modules, [NativePlugin::Dcp]);
    assert_eq!(composition.native_modules, ["dcp".into()].into());
    assert!(
        inventory
            .entries
            .iter()
            .all(|entry| entry.status == PluginStatus::Failed && entry.current.is_none())
    );
    assert!(
        composition
            .diagnostics
            .last()
            .unwrap()
            .contains("1 failed/unsupported_plugin")
    );
}
