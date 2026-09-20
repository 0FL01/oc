//! Model picker (UI02): dynamic catalog, variants, refresh status,
//! persisted choice, retired-model action, no silent fallback.
//!
//! Pure state over [`oc_adapters::models`]: exact-id selection, variant
//! validation, and re-resolution on catalog refresh. The persisted choice
//! lives in data-root prefs (`tui.model_selection`); disappearance of the
//! persisted id surfaces `Retired` with the actionable available list and
//! never silently falls back to another model.

use oc_adapters::models::{self, ModelCatalog, Selection};
use oc_adapters::storage::{Db, StorageError};

/// Prefs key holding the persisted selection JSON.
pub const PREF_MODEL: &str = "tui.model_selection";
/// Bounded browse window (view state, not the catalog).
pub const PICKER_WINDOW: usize = 10;

/// Catalog freshness as observed by the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshStatus {
    /// Catalog loaded (refresh count for diagnostics).
    Fresh(u64),
    /// Last refresh attempt failed; previous catalog still shown.
    Failed(String),
}

/// Picker selection state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerState {
    /// Browsing without a confirmed choice.
    Browsing,
    /// Exact-id choice active.
    Selected,
    /// Persisted id vanished from the catalog: actionable, never fallback.
    Retired {
        /// Wanted model id.
        wanted: String,
        /// Sorted available ids (bounded).
        available: String,
    },
}

/// Persisted selection shape.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Persisted {
    provider: String,
    id: String,
    variant: Option<String>,
}

/// Dynamic model picker bound to one provider catalog.
pub struct ModelPicker {
    catalog: ModelCatalog,
    cursor: usize,
    selected: Option<Selection>,
    persisted: Option<Persisted>,
    refresh: RefreshStatus,
    refresh_count: u64,
    last_error: Option<String>,
}

impl ModelPicker {
    /// Bind to a catalog; persisted choice loads separately via `load_persisted`.
    pub fn new(catalog: ModelCatalog) -> Self {
        Self {
            catalog,
            cursor: 0,
            selected: None,
            persisted: None,
            refresh: RefreshStatus::Fresh(0),
            refresh_count: 0,
            last_error: None,
        }
    }

    /// Current picker state.
    pub fn state(&self) -> PickerState {
        if let Some(retired) = self.retired() {
            return retired;
        }
        if self.selected.is_some() {
            PickerState::Selected
        } else {
            PickerState::Browsing
        }
    }

    /// Active selection, if any.
    pub fn selection(&self) -> Option<&Selection> {
        self.selected.as_ref()
    }

    /// Refresh status line for the view.
    pub fn status_line(&self) -> String {
        let catalog_part = match &self.refresh {
            RefreshStatus::Fresh(n) => format!("catalog fresh#{n}"),
            RefreshStatus::Failed(reason) => format!("catalog failed: {reason}"),
        };
        let selection_part = match &self.selected {
            Some(selection) => match &selection.variant {
                Some(variant) => format!("{}:{}", selection.id, variant.name),
                None => selection.id.clone(),
            },
            None => "no model".to_string(),
        };
        format!("{catalog_part} | {selection_part}")
    }

    /// Last error for the view (selection failures are visible, not silent).
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Sorted ids for the bounded browse window around the cursor.
    pub fn window(&self) -> Vec<String> {
        let ids = self.sorted_ids();
        if ids.is_empty() {
            return Vec::new();
        }
        let cursor = self.cursor.min(ids.len() - 1);
        let start = cursor.saturating_sub(PICKER_WINDOW / 2);
        ids.into_iter()
            .skip(start)
            .take(PICKER_WINDOW)
            .map(str::to_string)
            .collect()
    }

    /// Move the browse cursor (clamped, never wraps silently past the end).
    pub fn move_cursor(&mut self, delta: isize) {
        let len = self.sorted_ids().len();
        if len == 0 {
            self.cursor = 0;
            return;
        }
        let next = self.cursor as isize + delta;
        self.cursor = next.clamp(0, len as isize - 1) as usize;
    }

    /// Choose the model under the cursor by exact id and persist it.
    pub fn choose_cursor(&mut self, db: &Db) -> Result<(), String> {
        let id = self
            .sorted_ids()
            .get(self.cursor)
            .ok_or_else(|| "empty catalog".to_string())?
            .to_string();
        self.choose_id(&id, None, db)
    }

    /// Choose an exact model id with an optional variant and persist it.
    pub fn choose_id(&mut self, id: &str, variant: Option<&str>, db: &Db) -> Result<(), String> {
        let base = models::select_model(&self.catalog, id).map_err(|e| e.to_string())?;
        let selection = models::select_variant(&base, variant).map_err(|e| e.to_string())?;
        self.selected = Some(selection);
        self.persisted = Some(Persisted {
            provider: self.catalog.provider.clone(),
            id: id.to_string(),
            variant: variant.map(str::to_string),
        });
        self.last_error = None;
        self.persist(db).map_err(|e| format!("persist: {e}"))?;
        Ok(())
    }

    /// Replace the catalog (refresh): re-resolve the persisted choice.
    ///
    /// A vanished persisted id becomes `Retired`; a vanished variant keeps
    /// the model without variant and records a visible note. Nothing falls
    /// back to another model.
    pub fn refresh(&mut self, catalog: ModelCatalog) {
        self.catalog = catalog;
        self.refresh_count += 1;
        self.refresh = RefreshStatus::Fresh(self.refresh_count);
        self.cursor = 0;
        self.reresolve();
    }

    /// Mark the last refresh attempt failed; the previous catalog stays.
    pub fn refresh_failed(&mut self, reason: String) {
        self.refresh = RefreshStatus::Failed(reason);
    }

    /// Load the persisted choice and resolve it against the catalog.
    pub fn load_persisted(&mut self, db: &Db) -> Result<(), StorageError> {
        self.persisted = db
            .get_pref(PREF_MODEL)?
            .and_then(|raw| parse_persisted(&raw));
        self.reresolve();
        Ok(())
    }

    fn reresolve(&mut self) {
        let Some(wanted) = self.persisted.clone() else {
            return;
        };
        if wanted.provider != self.catalog.provider {
            // A foreign-provider record never applies here; keep browsing.
            self.selected = None;
            return;
        }
        match models::select_model(&self.catalog, &wanted.id) {
            Ok(base) => match models::select_variant(&base, wanted.variant.as_deref()) {
                Ok(selection) => {
                    self.selected = Some(selection);
                    self.last_error = None;
                }
                Err(e) => {
                    // Variant retired: keep the exact model, surface the note.
                    self.selected = Some(base);
                    self.last_error = Some(e.to_string());
                }
            },
            Err(_) => {
                self.selected = None;
            }
        }
    }

    fn retired(&self) -> Option<PickerState> {
        let wanted = self.persisted.as_ref()?;
        if wanted.provider != self.catalog.provider {
            return None;
        }
        if self.catalog.models.contains_key(&wanted.id) {
            return None;
        }
        Some(PickerState::Retired {
            wanted: wanted.id.clone(),
            available: self.sorted_ids().join(", "),
        })
    }

    fn persist(&self, db: &Db) -> Result<(), StorageError> {
        let Some(persisted) = &self.persisted else {
            return Ok(());
        };
        let raw = serde_json::json!({
            "provider": persisted.provider,
            "id": persisted.id,
            "variant": persisted.variant,
        });
        db.set_pref(PREF_MODEL, &raw.to_string())
    }

    fn sorted_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self.catalog.models.keys().map(String::as_str).collect();
        ids.sort();
        ids
    }
}

fn parse_persisted(raw: &str) -> Option<Persisted> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    Some(Persisted {
        provider: value.get("provider")?.as_str()?.to_string(),
        id: value.get("id")?.as_str()?.to_string(),
        variant: value
            .get("variant")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::{ModelPicker, PICKER_WINDOW, PickerState};
    use oc_adapters::models::ModelCatalog;
    use oc_adapters::storage::Db;
    use serde_json::json;

    fn catalog() -> ModelCatalog {
        ModelCatalog {
            provider: "ludka2".to_string(),
            models: [
                (
                    "b".to_string(),
                    json!({"limit": {"context": 1000, "output": 100},
                           "variants": {"low": {}, "high": {"disabled": true}}}),
                ),
                (
                    "a".to_string(),
                    json!({"limit": {"context": 1000, "output": 100},
                           "variants": {"none": {}}}),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }

    fn test_db(name: &str) -> Db {
        let root =
            std::env::temp_dir().join(format!("oc-tui-picker-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Db::open(&root).expect("db")
    }

    #[test]
    fn choose_persists_and_reloads() {
        let db = test_db("roundtrip");
        let mut picker = ModelPicker::new(catalog());
        picker.choose_id("b", Some("low"), &db).expect("choose");
        assert_eq!(picker.state(), PickerState::Selected);
        assert!(picker.status_line().contains("b:low"));

        let mut reloaded = ModelPicker::new(catalog());
        reloaded.load_persisted(&db).expect("load");
        assert_eq!(reloaded.state(), PickerState::Selected);
        assert!(reloaded.status_line().contains("b:low"));
    }

    #[test]
    fn unknown_id_never_falls_back() {
        let db = test_db("unknown");
        let mut picker = ModelPicker::new(catalog());
        let error = picker.choose_id("zzz", None, &db).expect_err("unknown");
        assert!(error.contains("unknown model"), "{error}");
        assert_eq!(picker.state(), PickerState::Browsing);
        assert!(picker.selection().is_none());
    }

    #[test]
    fn retired_model_is_actionable() {
        let db = test_db("retired");
        let mut picker = ModelPicker::new(catalog());
        picker.choose_id("b", None, &db).expect("choose");

        let mut next = catalog();
        next.models.remove("b");
        picker.refresh(next);
        match picker.state() {
            PickerState::Retired { wanted, available } => {
                assert_eq!(wanted, "b");
                assert!(available.contains('a'), "{available}");
                assert!(!available.contains('b'), "{available}");
            }
            other => panic!("must retire, got {other:?}"),
        }
        // Choosing the visible alternative recovers explicitly.
        picker.choose_id("a", None, &db).expect("choose a");
        assert_eq!(picker.state(), PickerState::Selected);
    }

    #[test]
    fn window_is_bounded() {
        let mut models = ModelCatalog {
            provider: "ludka2".to_string(),
            models: Default::default(),
        };
        for i in 0..50 {
            models.models.insert(format!("m{i:02}"), json!({}));
        }
        let picker = ModelPicker::new(models);
        assert!(picker.window().len() <= PICKER_WINDOW);
    }
}
