//! Model picker (UI02): dynamic catalog, variants, refresh status,
//! persisted choice, retired-model action, no silent fallback.
//!
//! Pure state over [`oc_adapters::models`]: exact-id selection, variant
//! validation, and re-resolution on catalog refresh. Persistence stays
//! outside this crate: the caller feeds the stored record with
//! [`ModelPicker::load_persisted_raw`] and reads the record to save with
//! [`ModelPicker::persisted_record`]. Disappearance of the persisted id
//! surfaces `Retired` with the actionable available list and never silently
//! falls back to another model.

use oc_adapters::models::{self, ModelCatalog, Selection};

/// Prefs key holding the persisted selection JSON.
pub const PREF_MODEL: &str = oc_core::queries::PREF_MODEL_SELECTION;
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
    /// Explicit variant cycle position (0 = model default).
    variant_cursor: usize,
    variant_changed: bool,
}

impl ModelPicker {
    /// Complete catalog options; the shared SelectList owns filtering/viewport.
    pub fn options(&self) -> Vec<crate::dialog::SelectOption> {
        let mut options: Vec<_> = self
            .catalog
            .models
            .iter()
            .map(|(id, spec)| {
                let title = spec
                    .get("name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(id)
                    .to_string();
                let category = spec
                    .get("provider_name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&self.catalog.provider)
                    .to_string();
                let number = |v: &serde_json::Value| {
                    v.as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                };
                let free = spec.get("cost").is_some_and(|p| {
                    number(&p["input"]) == Some(0.0) && number(&p["output"]) == Some(0.0)
                });
                crate::dialog::SelectOption {
                    value: id.clone(),
                    title,
                    category,
                    footer: if free { "Free".into() } else { String::new() },
                    current: self.selected.as_ref().is_some_and(|s| s.id == *id),
                }
            })
            .collect();
        options.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| b.footer.cmp(&a.footer))
                .then_with(|| a.title.cmp(&b.title))
        });
        options
    }
    /// Bind to a catalog; a stored record loads separately via
    /// [`ModelPicker::load_persisted_raw`].
    pub fn new(catalog: ModelCatalog) -> Self {
        Self {
            catalog,
            cursor: 0,
            selected: None,
            persisted: None,
            refresh: RefreshStatus::Fresh(0),
            refresh_count: 0,
            last_error: None,
            variant_cursor: 0,
            variant_changed: false,
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

    /// Provider id of the bound catalog.
    pub fn provider(&self) -> &str {
        &self.catalog.provider
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

    /// Human metadata for a browsed id; selection continues to use the exact id.
    pub fn display_label(&self, id: &str) -> String {
        let Some(spec) = self.catalog.models.get(id) else {
            return id.to_string();
        };
        let name = spec.get("name").and_then(|v| v.as_str()).unwrap_or(id);
        let provider = spec
            .get("provider_name")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.catalog.provider);
        let price = spec.get("cost");
        let free = price.is_some_and(|p| {
            p["input"].as_str().and_then(|s| s.parse::<f64>().ok()) == Some(0.0)
                && p["output"].as_str().and_then(|s| s.parse::<f64>().ok()) == Some(0.0)
        });
        let suffix = if free { " · Free" } else { "" };
        format!("{name} · {provider}{suffix}")
    }

    /// Exact id under the browse cursor, if the catalog is non-empty.
    pub fn cursor_id(&self) -> Option<String> {
        self.sorted_ids()
            .get(self.cursor)
            .map(|id| (*id).to_string())
    }

    /// Park the browse cursor on an exact id; false when it is unknown.
    pub fn focus_id(&mut self, id: &str) -> bool {
        match self
            .sorted_ids()
            .iter()
            .position(|candidate| *candidate == id)
        {
            Some(position) => {
                self.cursor = position;
                self.variant_cursor = 0;
                self.variant_changed = false;
                true
            }
            None => false,
        }
    }

    /// Enabled variant names (sorted) of the model under the cursor.
    pub fn variants(&self) -> Vec<String> {
        let Some(id) = self.cursor_id() else {
            return Vec::new();
        };
        let Some(entry) = self.catalog.models.get(&id) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entry
            .get("variants")
            .and_then(|value| value.as_object())
            .map(|variants| {
                variants
                    .iter()
                    .filter(|(_, variant)| {
                        variant.get("disabled") != Some(&serde_json::Value::Bool(true))
                    })
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// Pending explicit variant choice (None = model default).
    pub fn pending_variant(&self) -> Option<String> {
        if self.variant_cursor == 0 {
            return None;
        }
        self.variants().get(self.variant_cursor - 1).cloned()
    }

    pub fn variant_changed(&self) -> bool {
        self.variant_changed
    }

    /// Cycle the pending variant of the model under the cursor:
    /// default -> variant 1 -> … -> variant N -> default.
    pub fn cycle_variant(&mut self, delta: isize) {
        self.variant_changed = true;
        let count = self.variants().len() + 1;
        let next = self.variant_cursor as isize + delta;
        self.variant_cursor = next.rem_euclid(count as isize) as usize;
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
        self.variant_cursor = 0;
    }

    /// Choose the model under the cursor by exact id. Persisting the
    /// resulting [`ModelPicker::persisted_record`] is the caller's job.
    pub fn choose_cursor(&mut self) -> Result<(), String> {
        let id = self
            .sorted_ids()
            .get(self.cursor)
            .ok_or_else(|| "empty catalog".to_string())?
            .to_string();
        self.choose_id(&id, None)
    }

    /// Choose an exact model id with an optional variant.
    pub fn choose_id(&mut self, id: &str, variant: Option<&str>) -> Result<(), String> {
        let base = models::select_model(&self.catalog, id).map_err(|e| e.to_string())?;
        let selection = models::select_variant(&base, variant).map_err(|e| e.to_string())?;
        self.selected = Some(selection);
        self.persisted = Some(Persisted {
            provider: self.catalog.provider.clone(),
            id: id.to_string(),
            variant: variant.map(str::to_string),
        });
        self.last_error = None;
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

    /// Load a stored selection record and resolve it against the catalog.
    pub fn load_persisted_raw(&mut self, raw: Option<&str>) {
        self.persisted = raw.and_then(parse_persisted);
        self.reresolve();
    }

    /// Stored selection record for the caller to persist, if any.
    pub fn persisted_record(&self) -> Option<String> {
        let persisted = self.persisted.as_ref()?;
        Some(
            serde_json::json!({
                "provider": persisted.provider,
                "id": persisted.id,
                "variant": persisted.variant,
            })
            .to_string(),
        )
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
        self.focus_selected();
    }

    /// Park the browse cursor on the resolved selection, if any.
    fn focus_selected(&mut self) {
        let Some(selection) = &self.selected else {
            return;
        };
        if let Some(index) = self.sorted_ids().iter().position(|id| *id == selection.id) {
            self.cursor = index;
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

    #[test]
    fn modal_options_use_known_prices_and_declared_variants_only() {
        let mut c = catalog();
        c.models.get_mut("a").unwrap()["name"] = json!("Human A");
        c.models.get_mut("a").unwrap()["provider_name"] = json!("Provider A");
        c.models.get_mut("a").unwrap()["cost"] = json!({"input":"0","output":"0"});
        c.models.get_mut("b").unwrap()["cost"] = json!({"input":0});
        let mut p = ModelPicker::new(c);
        let options = p.options();
        let a = options.iter().find(|o| o.value == "a").unwrap();
        assert_eq!(
            (&*a.title, &*a.category, &*a.footer),
            ("Human A", "Provider A", "Free")
        );
        assert!(
            options
                .iter()
                .find(|o| o.value == "b")
                .unwrap()
                .footer
                .is_empty(),
            "unknown output price is not Free"
        );
        p.focus_id("b");
        assert_eq!(p.variants(), ["low"]);
        p.cycle_variant(1);
        assert_eq!(p.pending_variant().as_deref(), Some("low"));
        p.cycle_variant(1);
        assert!(
            p.variant_changed() && p.pending_variant().is_none(),
            "explicit catalog default differs from no edit"
        );
    }

    #[test]
    fn choose_roundtrips_through_the_stored_record() {
        let mut picker = ModelPicker::new(catalog());
        picker.choose_id("b", Some("low")).expect("choose");
        assert_eq!(picker.state(), PickerState::Selected);
        assert!(picker.status_line().contains("b:low"));
        let raw = picker.persisted_record().expect("record");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(value["provider"], "ludka2");
        assert_eq!(value["id"], "b");
        assert_eq!(value["variant"], "low");

        let mut reloaded = ModelPicker::new(catalog());
        reloaded.load_persisted_raw(Some(&raw));
        assert_eq!(reloaded.state(), PickerState::Selected);
        assert!(reloaded.status_line().contains("b:low"));
    }

    #[test]
    fn no_record_browses_and_foreign_records_are_ignored() {
        let mut picker = ModelPicker::new(catalog());
        picker.load_persisted_raw(None);
        assert_eq!(picker.state(), PickerState::Browsing);
        assert!(picker.selection().is_none());

        picker.load_persisted_raw(Some(
            &json!({"provider": "other", "id": "b", "variant": null}).to_string(),
        ));
        assert_eq!(picker.state(), PickerState::Browsing);
        assert!(picker.persisted_record().is_some());
        // A later exact choice stays explicit.
        picker.choose_id("a", None).expect("choose");
        assert_eq!(picker.state(), PickerState::Selected);
    }

    #[test]
    fn unknown_id_never_falls_back() {
        let mut picker = ModelPicker::new(catalog());
        let error = picker.choose_id("zzz", None).expect_err("unknown");
        assert!(error.contains("unknown model"), "{error}");
        assert_eq!(picker.state(), PickerState::Browsing);
        assert!(picker.selection().is_none());
    }

    #[test]
    fn retired_model_is_actionable() {
        let mut picker = ModelPicker::new(catalog());
        picker.choose_id("b", None).expect("choose");

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
        picker.choose_id("a", None).expect("choose a");
        assert_eq!(picker.state(), PickerState::Selected);
    }

    #[test]
    fn retired_variant_keeps_model_and_surfaces_note() {
        let mut picker = ModelPicker::new(catalog());
        picker.choose_id("b", Some("low")).expect("choose");
        picker.load_persisted_raw(Some(
            &json!({"provider": "ludka2", "id": "b", "variant": "gone"}).to_string(),
        ));
        assert_eq!(picker.state(), PickerState::Selected);
        assert!(picker.selection().expect("model").variant.is_none());
        assert!(picker.last_error().is_some(), "visible note");
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
