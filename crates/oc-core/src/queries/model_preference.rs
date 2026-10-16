//! Model dialog favorites/recents (pinned OC2 `model-preference.ts`). Identity
//! only: a remembered model never grants selection, credentials or a request.

/// Prefs key holding the model dialog preference document.
pub const PREF_MODEL_PREFERENCE: &str = "tui.model_preference";
/// Pinned `recentModels` keeps the newest ten distinct identities.
pub const MAX_RECENT_MODELS: usize = 10;
/// Bounded stored document; larger values decode as empty.
pub const MAX_MODEL_PREFERENCE_BYTES: usize = 64 * 1024;
const MAX_FAVORITE_MODELS: usize = 256;

/// Exact provider/model identity, stored in the upstream document shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModelPreferenceModel {
    #[serde(rename = "providerID")]
    pub provider: String,
    #[serde(rename = "modelID")]
    pub model: String,
}

/// Ordered favorites and recents; stale identities are kept and hidden by views.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelPreference {
    pub recent: Vec<ModelPreferenceModel>,
    pub favorite: Vec<ModelPreferenceModel>,
    /// Remembered named variant per model (pinned `variant` map), owned by the
    /// selection prefs; read-only here. Default/no memory is absent.
    pub variants: Vec<(ModelPreferenceModel, String)>,
}

/// Owner-side read-modify-write mutation, so parked tabs never save stale lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelPreferenceAction {
    /// A model chosen from the dialog becomes the newest recent entry.
    Recent(ModelPreferenceModel),
    /// Add (first) or remove one favorite.
    Favorite {
        model: ModelPreferenceModel,
        enabled: bool,
    },
    /// Pinned `saveVariant`: remember this model's effort as soon as it is
    /// chosen (dialog or Ctrl+T); `None` is an explicit Default. Stored by the
    /// selection owner, not in this document.
    Variant {
        model: ModelPreferenceModel,
        variant: Option<String>,
    },
}

fn models(value: Option<&serde_json::Value>, cap: usize) -> Vec<ModelPreferenceModel> {
    let Some(items) = value.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut out: Vec<ModelPreferenceModel> = Vec::new();
    for item in items {
        let field = |key| {
            item.get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
        };
        if let (Some(provider), Some(model)) = (field("providerID"), field("modelID")) {
            let model = ModelPreferenceModel {
                provider: provider.into(),
                model: model.into(),
            };
            if !out.contains(&model) {
                out.push(model);
            }
        }
        if out.len() == cap {
            break;
        }
    }
    out
}

impl ModelPreference {
    /// Tolerant decode like `decodeModelPreference`: invalid entries are skipped.
    pub fn decode(raw: Option<&str>) -> Self {
        let root = raw
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .unwrap_or_default();
        Self {
            recent: models(root.get("recent"), MAX_RECENT_MODELS),
            favorite: models(root.get("favorite"), MAX_FAVORITE_MODELS),
            variants: Vec::new(),
        }
    }

    /// Apply one mutation (`recentModels` / `favoriteModels`).
    pub fn apply(&mut self, action: ModelPreferenceAction) {
        match action {
            ModelPreferenceAction::Recent(model) => {
                self.recent.retain(|item| item != &model);
                self.recent.insert(0, model);
                self.recent.truncate(MAX_RECENT_MODELS);
            }
            ModelPreferenceAction::Favorite { model, enabled } => {
                self.favorite.retain(|item| item != &model);
                if enabled {
                    self.favorite.insert(0, model);
                    self.favorite.truncate(MAX_FAVORITE_MODELS);
                }
            }
            ModelPreferenceAction::Variant { .. } => {}
        }
    }

    /// Patch the stored document, preserving unrelated upstream-shaped keys.
    pub fn encode_over(&self, raw: Option<&str>) -> String {
        let mut root = raw
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}));
        root["recent"] = serde_json::json!(self.recent);
        root["favorite"] = serde_json::json!(self.favorite);
        root.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(provider: &str, model: &str) -> ModelPreferenceModel {
        ModelPreferenceModel {
            provider: provider.into(),
            model: model.into(),
        }
    }

    #[test]
    fn recent_dedupes_newest_first_and_caps_at_ten() {
        let mut pref = ModelPreference::default();
        for i in 0..12 {
            pref.apply(ModelPreferenceAction::Recent(m("p", &i.to_string())));
        }
        pref.apply(ModelPreferenceAction::Recent(m("p", "5")));
        assert_eq!(pref.recent.len(), MAX_RECENT_MODELS);
        assert_eq!(pref.recent[0], m("p", "5"));
        assert_eq!(pref.recent[1], m("p", "11"));
        assert_eq!(pref.recent.iter().filter(|x| **x == m("p", "5")).count(), 1);
        // Same model ID under another provider is a different identity.
        pref.apply(ModelPreferenceAction::Recent(m("q", "5")));
        assert_eq!(&pref.recent[..2], &[m("q", "5"), m("p", "5")]);
    }

    #[test]
    fn favorite_toggle_prepends_and_removes_and_document_roundtrips() {
        let mut pref = ModelPreference::default();
        let add = |model| ModelPreferenceAction::Favorite {
            model,
            enabled: true,
        };
        pref.apply(add(m("a", "1")));
        pref.apply(add(m("b", "2")));
        pref.apply(add(m("a", "1")));
        assert_eq!(pref.favorite, vec![m("a", "1"), m("b", "2")]);
        pref.apply(ModelPreferenceAction::Favorite {
            model: m("a", "1"),
            enabled: false,
        });
        assert_eq!(pref.favorite, vec![m("b", "2")]);
        let raw = pref.encode_over(Some(r#"{"variant":{"b/2":"high"},"recent":"bad"}"#));
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["variant"]["b/2"], "high");
        assert_eq!(value["favorite"][0]["providerID"], "b");
        assert_eq!(ModelPreference::decode(Some(&raw)), pref);
    }

    #[test]
    fn decode_skips_invalid_entries_and_garbage() {
        assert_eq!(
            ModelPreference::decode(Some("not json")),
            ModelPreference::default()
        );
        let pref = ModelPreference::decode(Some(
            r#"{"recent":[{"providerID":"p","modelID":"m"},{"providerID":"","modelID":"x"},7,{"modelID":"y"}],"favorite":{}}"#,
        ));
        assert_eq!(pref.recent, vec![m("p", "m")]);
        assert!(pref.favorite.is_empty());
    }
}
