//! Model dialog read view (pinned OC2 DialogModel): every admitted provider
//! whose catalog is already loaded, plus favorites/recents in existing prefs.
//! No discovery, selection, credential value or request effect.
use super::*;
use crate::storage::BoundedPref;
use oc_core::queries::{
    AccountAuthSource, MAX_MODEL_PREFERENCE_BYTES, ModelCatalogs, ModelPreference,
    ModelPreferenceAction, PREF_MODEL_PREFERENCE,
};

#[cfg(test)]
#[path = "model_picker/tests.rs"]
mod tests;

/// Configured display name of an admitted provider, falling back to its ID.
pub(super) fn provider_name(c: &Composition, provider: &str) -> String {
    c.provider_views
        .get(provider)
        .map(|view| &view.entry)
        .or_else(|| c.generation.providers.get(provider))
        .and_then(|entry| entry.name.clone())
        .unwrap_or_else(|| provider.to_owned())
}

/// Declared release time in Unix milliseconds: numeric `/time/released`, or a
/// models.dev style `YYYY-MM-DD` date. Unknown stays `None` (sorted last).
pub(super) fn released(spec: &serde_json::Value) -> Option<u64> {
    let value = spec
        .pointer("/time/released")
        .or_else(|| spec.get("released"))
        .or_else(|| spec.get("release_date"))?;
    value.as_u64().or_else(|| value.as_str().and_then(date_ms))
}

fn date_ms(text: &str) -> Option<u64> {
    let mut parts = text.get(..10).unwrap_or(text).splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next().map_or(Some(1), |m| m.parse().ok())?;
    let day: i64 = parts.next().map_or(Some(1), |d| d.parse().ok())?;
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days).ok()?.checked_mul(86_400_000)
}

fn usable(source: AccountAuthSource) -> bool {
    matches!(
        source,
        AccountAuthSource::Stored
            | AccountAuthSource::Environment
            | AccountAuthSource::Configured
            | AccountAuthSource::Anonymous
    )
}

/// Selected provider's effective snapshot first, then every other admitted
/// provider with a usable credential source, from already-loaded catalogs.
pub(super) fn catalogs(
    db: &Db,
    c: &Composition,
    effective: &Effective,
    generation: u64,
) -> Result<ModelCatalogs, CoreError> {
    // A connection read failure must not hide the selected provider's models.
    let connections = accounts::connections(db, c).unwrap_or_default();
    let mut catalogs = vec![effective.snapshot(c, generation)];
    for connection in &connections {
        if !usable(connection.source) || connection.provider == effective.provider_id {
            continue;
        }
        let Some(catalog) = c.catalog_for(&connection.provider) else {
            continue;
        };
        let mut unchosen = Effective::from_composition(c);
        unchosen.model_id.clear();
        unchosen.variant = None;
        catalogs.push(unchosen.snapshot_catalog(
            c,
            generation,
            catalog,
            c.readiness_for(&connection.provider, "", None),
        ));
    }
    Ok(ModelCatalogs {
        connected: connections.iter().any(|c| usable(c.source)),
        catalogs,
    })
}

/// Read, or read-modify-write one mutation of, the stored preference document.
pub(super) fn preference(
    db: &Db,
    action: Option<ModelPreferenceAction>,
) -> Result<ModelPreference, CoreError> {
    let raw = match db
        .get_pref_bounded(PREF_MODEL_PREFERENCE, MAX_MODEL_PREFERENCE_BYTES)
        .map_err(|error| query_storage_error(db, error))?
    {
        BoundedPref::Value(raw) => Some(raw),
        BoundedPref::Missing | BoundedPref::TooLarge => None,
    };
    let mut preference = ModelPreference::decode(raw.as_deref());
    if let Some(ModelPreferenceAction::Variant { model, variant }) = &action {
        if model.provider.is_empty()
            || model.model.is_empty()
            || variant
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 128)
        {
            return Err(app_error("invalid remembered variant"));
        }
        let (key, value) =
            super::selection::variant_record(&model.provider, &model.model, variant)?;
        db.set_pref(&key, &value)
            .map_err(|error| query_storage_error(db, error))?;
    } else if let Some(action) = action {
        preference.apply(action);
        db.set_pref(
            PREF_MODEL_PREFERENCE,
            &preference.encode_over(raw.as_deref()),
        )
        .map_err(|error| query_storage_error(db, error))?;
    }
    // The selection owner's per-model variant memory (`[provider, model]`).
    const PREFIX: &str = "tui.selection.variant:";
    for (key, raw) in db
        .prefs_with_prefix(PREFIX, 1024, 1024)
        .map_err(|error| query_storage_error(db, error))?
    {
        let Ok([provider, model]) = serde_json::from_str::<[String; 2]>(&key[PREFIX.len()..])
        else {
            continue;
        };
        if let Ok(Some(variant)) = serde_json::from_str::<Option<String>>(&raw)
            && variant != "default"
        {
            preference.variants.push((
                oc_core::queries::ModelPreferenceModel { provider, model },
                variant,
            ));
        }
    }
    Ok(preference)
}
