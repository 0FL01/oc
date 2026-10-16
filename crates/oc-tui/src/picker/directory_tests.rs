use super::ModelDirectory;
use oc_core::queries::{
    CatalogSnapshot, ModelCatalogs, ModelEntry, ModelPreference, ModelPreferenceModel,
};

fn entry(id: &str, provider_name: &str, free: bool, released: Option<u64>) -> ModelEntry {
    ModelEntry {
        id: id.into(),
        display_name: id.into(),
        provider_name: provider_name.into(),
        input_free: free,
        released,
        ..Default::default()
    }
}

fn catalog(provider: &str, models: Vec<ModelEntry>) -> CatalogSnapshot {
    CatalogSnapshot {
        chrome: Default::default(),
        auto_accept: Default::default(),
        provider: provider.into(),
        models,
        model_id: String::new(),
        variant: None,
        agents: Vec::new(),
        agent_id: None,
        commands: Vec::new(),
        command_descriptions: Default::default(),
    }
}

pub(crate) fn m(provider: &str, model: &str) -> ModelPreferenceModel {
    ModelPreferenceModel {
        provider: provider.into(),
        model: model.into(),
    }
}

pub(crate) fn fixture(connected: bool) -> ModelDirectory {
    let (catalogs, preference) = fixture_parts(connected);
    ModelDirectory::new(catalogs, preference)
}

pub(crate) fn fixture_parts(connected: bool) -> (ModelCatalogs, ModelPreference) {
    let mut deprecated = entry("Old", "OpenCode Go", true, Some(9));
    deprecated.deprecated = true;
    (
        ModelCatalogs {
            connected,
            catalogs: vec![
                catalog(
                    "ludka2",
                    vec![
                        entry("DeepSeek V4.1 Flash · opencode-go", "ludka2", false, None),
                        entry("GPT-6.1-Sol · codex", "ludka2", false, None),
                        entry("Muse/slash", "ludka2", false, None),
                    ],
                ),
                catalog(
                    "zai",
                    vec![
                        entry("GLM-5.3", "Z.AI Coding Plan", true, None),
                        entry("glm-4", "Z.AI Coding Plan", false, None),
                    ],
                ),
                catalog(
                    "opencode-go",
                    vec![
                        entry("Claude Haiku 5.5", "OpenCode Go", false, Some(5)),
                        entry("LongCat 2.5 Preview Free", "OpenCode Go", true, Some(2)),
                        entry("Cat Jam Free", "OpenCode Go", true, Some(3)),
                        deprecated,
                    ],
                ),
            ],
        },
        ModelPreference {
            favorite: vec![
                m("ludka2", "DeepSeek V4.1 Flash · opencode-go"),
                m("zai", "GLM-5.3"),
                m("gone", "x"),
            ],
            recent: vec![
                m("ludka2", "GPT-6.1-Sol · codex"),
                m("ludka2", "DeepSeek V4.1 Flash · opencode-go"),
                m("gone", "y"),
            ],
        },
    )
}

fn rows(options: &[crate::dialog::SelectOption]) -> Vec<String> {
    options
        .iter()
        .map(|o| {
            format!(
                "{}|{}|{}|{}{}",
                o.category,
                o.title,
                o.description,
                o.footer,
                if o.current { "|●" } else { "" }
            )
        })
        .collect()
}

#[test]
fn connected_browse_has_favorites_recent_then_ordered_provider_sections() {
    let directory = fixture(true);
    let options = directory.options(false, Some(m("ludka2", "GPT-6.1-Sol · codex")));
    assert_eq!(
        rows(&options),
        [
            "Favorites|DeepSeek V4.1 Flash · opencode-go|ludka2|",
            "Favorites|GLM-5.3|Z.AI Coding Plan|Free",
            "Recent|GPT-6.1-Sol · codex|ludka2||●",
            "OpenCode Go|Cat Jam Free||Free",
            "OpenCode Go|LongCat 2.5 Preview Free||Free",
            "OpenCode Go|Claude Haiku 5.5||",
            "ludka2|Muse/slash||",
            "Z.AI Coding Plan|glm-4||",
        ]
    );
    // Same identity encoding round-trips IDs containing `/`.
    let last = options.iter().find(|o| o.title == "Muse/slash").unwrap();
    assert_eq!(
        ModelDirectory::decode(&last.value),
        Some(m("ludka2", "Muse/slash"))
    );
}

#[test]
fn search_source_is_flat_metadata_sorted_with_favorites_first() {
    let directory = fixture(true);
    let options = directory.options(true, None);
    assert_eq!(
        rows(&options),
        [
            "Z.AI Coding Plan|GLM-5.3|(Favorite)|Free",
            "ludka2|DeepSeek V4.1 Flash · opencode-go|(Favorite)|",
            "OpenCode Go|Cat Jam Free||Free",
            "OpenCode Go|LongCat 2.5 Preview Free||Free",
            "OpenCode Go|Claude Haiku 5.5||",
            // Case-insensitive like localeCompare: glm < GPT.
            "Z.AI Coding Plan|glm-4||",
            "ludka2|GPT-6.1-Sol · codex||",
            "ludka2|Muse/slash||",
        ]
    );
}

#[test]
fn disconnected_has_no_sections_extras_or_favorite_marks() {
    let directory = fixture(false);
    let options = directory.options(false, None);
    assert_eq!(options.len(), 8);
    assert!(options.iter().all(|o| o.category.is_empty()));
    assert!(options.iter().all(|o| o.description.is_empty()));
    assert_eq!(options[0].title, "Cat Jam Free");
}
