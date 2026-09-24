//! Scoped selection metadata in the existing prefs store. No history owner or
//! transcript copy. Keys are structured tuples (no delimiter collisions); only
//! one session's admitted agent drafts are loaded for an action/turn.
use super::*;
use oc_core::core_app::FreshSelection;
use oc_core::queries::SessionSelectionAction as Action;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct ModelChoice {
    id: String,
    variant: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct SessionChoice {
    agent: Option<String>,
    models: BTreeMap<String, ModelChoice>,
    #[serde(default)]
    epoch: u64,
}

fn epoch_key(c: &Composition) -> String {
    key(
        "legacy_epoch",
        &[&c.project.to_string_lossy(), &c.catalog.provider],
    )
}

pub(super) fn legacy_epoch(db: &Db, c: &Composition) -> Result<u64, CoreError> {
    Ok(load::<u64>(db, &epoch_key(c))?.unwrap_or(0))
}

pub(super) fn next_legacy_epoch(db: &Db, c: &Composition) -> Result<(String, String), CoreError> {
    let next = legacy_epoch(db, c)?
        .checked_add(1)
        .ok_or_else(|| app_error("selection revision exhausted"))?;
    record(epoch_key(c), &next)
}

fn key(kind: &str, parts: &[&str]) -> String {
    format!(
        "tui.selection.{kind}:{}",
        serde_json::to_string(parts).expect("strings")
    )
}

fn session_key(c: &Composition, session: &str) -> String {
    key(
        "session",
        &[&c.project.to_string_lossy(), &c.catalog.provider, session],
    )
}

fn draft_key(c: &Composition, agent: Option<&str>) -> String {
    key(
        "draft",
        &[
            &c.project.to_string_lossy(),
            &c.catalog.provider,
            agent.unwrap_or(""),
        ],
    )
}

fn variant_key(c: &Composition, id: &str) -> String {
    key("variant", &[&c.catalog.provider, id])
}

fn load<T: serde::de::DeserializeOwned>(db: &Db, key: &str) -> Result<Option<T>, CoreError> {
    db.get_pref(key)
        .map_err(app_error)?
        .map(|raw| {
            serde_json::from_str(&raw)
                .map_err(|_| app_error("malformed scoped selection preference"))
        })
        .transpose()
}

fn record<T: Serialize>(key: String, value: &T) -> Result<(String, String), CoreError> {
    Ok((key, serde_json::to_string(value).map_err(app_error)?))
}

fn model(e: &Effective) -> ModelChoice {
    ModelChoice {
        id: e.model_id.clone(),
        variant: e.variant.clone(),
    }
}

fn set_model(e: &mut Effective, c: &Composition, choice: ModelChoice) -> Result<(), CoreError> {
    let selected = crate::models::select_model(&c.catalog, &choice.id)
        .and_then(|base| crate::models::select_variant(&base, choice.variant.as_deref()))
        .map_err(app_error)?;
    e.model_id = selected.id;
    e.variant = selected.variant.map(|v| v.name);
    Ok(())
}

fn preferred(
    db: &Db,
    c: &Composition,
    e: &Effective,
    id: String,
) -> Result<ModelChoice, CoreError> {
    // Some(None) is an explicit Default preference, distinct from no preference.
    let variant = if let Some(preferred) = load::<Option<String>>(db, &variant_key(c, &id))? {
        preferred
    } else if e.model_id == id {
        e.variant.clone()
    } else if c.model_id == id {
        c.variant.clone()
    } else {
        None
    };
    // A retired preference must remain visible until an explicit choice.
    Ok(ModelChoice { id, variant })
}

fn base_for_agent(
    c: &Composition,
    fallback: &Effective,
    agent: Option<&str>,
) -> Result<Effective, CoreError> {
    let mut selected = fallback.clone();
    // A different unpinned agent must not inherit another agent's scoped model.
    if let Some(agent) = agent {
        selected.set_agent(c, agent)?;
    } else {
        selected.agent_id = None;
        selected.agent_prompt = None;
        selected.agent_digest = None;
    }
    Ok(selected)
}

fn resolve(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    choice: &SessionChoice,
) -> Result<Effective, CoreError> {
    let mut selected = base_for_agent(c, fallback, choice.agent.as_deref())?;
    let agent = choice.agent.as_deref().unwrap_or("");
    let model = match choice.models.get(agent) {
        Some(model) => model.clone(),
        None => {
            let draft = load::<ModelChoice>(db, &draft_key(c, choice.agent.as_deref()))?;
            preferred(
                db,
                c,
                &selected,
                draft
                    .map(|m| m.id)
                    .unwrap_or_else(|| selected.model_id.clone()),
            )?
        }
    };
    // Project a retired choice honestly: only turn acceptance validates it.
    // No replacement model/variant is chosen and no preference is rewritten.
    selected.model_id = model.id;
    selected.variant = model.variant;
    Ok(selected)
}

pub(super) fn for_turn(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    session: &str,
) -> Result<Effective, CoreError> {
    match load::<SessionChoice>(db, &session_key(c, session))? {
        Some(choice) if choice.epoch >= fallback.legacy_epoch => resolve(db, c, fallback, &choice),
        Some(_) => Ok(fallback.clone()), // explicit legacy API supersedes older scoped drafts
        None => Ok(fallback.clone()),    // existing headless selection contract
    }
}

/// Prevalidate an explicit Home choice and encode its session preference for
/// the same transaction that accepts the new root's first user message.
pub(super) fn fresh(
    c: &Composition,
    fallback: &Effective,
    session: &str,
    choice: FreshSelection,
) -> Result<(Effective, (String, String)), CoreError> {
    let mut selected = base_for_agent(c, fallback, choice.agent_id.as_deref())?;
    set_model(
        &mut selected,
        c,
        ModelChoice {
            id: choice.model_id,
            variant: choice.variant,
        },
    )?;
    let agent = selected.agent_id.clone().unwrap_or_default();
    let session_choice = SessionChoice {
        agent: selected.agent_id.clone(),
        models: BTreeMap::from([(agent, model(&selected))]),
        epoch: fallback.legacy_epoch,
    };
    Ok((selected, record(session_key(c, session), &session_choice)?))
}

/// Sessionless Home selection. Only Location/agent drafts and the existing
/// provider/model variant preference are durable; the Home choice itself is
/// owned by the application worker and never uses a fabricated session id.
fn home_for_agent(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    agent: Option<&str>,
) -> Result<Effective, CoreError> {
    let mut selected = base_for_agent(c, fallback, agent)?;
    let choice = match load::<ModelChoice>(db, &draft_key(c, agent))? {
        Some(draft) => draft,
        None => preferred(db, c, &selected, selected.model_id.clone())?,
    };
    // A retired draft stays visible until the user explicitly replaces it.
    selected.model_id = choice.id;
    selected.variant = choice.variant;
    Ok(selected)
}

/// Resolve an unchosen Home from this generation and the effective agent's
/// durable Location draft. A read must not turn this into an explicit choice.
pub(super) fn home_current(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
) -> Result<Effective, CoreError> {
    home_for_agent(db, c, fallback, fallback.agent_id.as_deref())
}

pub(super) fn home(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    current: &Effective,
    action: Action,
) -> Result<Effective, CoreError> {
    match action {
        Action::Current => Ok(current.clone()),
        Action::Agent(agent) | Action::New(Some(agent)) => {
            home_for_agent(db, c, fallback, Some(&agent))
        }
        Action::New(None) => home_for_agent(db, c, fallback, fallback.agent_id.as_deref()),
        Action::Model(id) => {
            // An explicit replacement must work even when the old Home model
            // has retired. Keep a valid same-model choice, otherwise restore
            // the remembered variant (or explicitly remediate a retired one).
            let mut choice = if current.model_id == id {
                model(current)
            } else {
                preferred(db, c, current, id.clone())?
            };
            let mut records = Vec::new();
            if let Some(variant) = choice.variant.as_deref() {
                let base = crate::models::select_model(&c.catalog, &id).map_err(app_error)?;
                if crate::models::select_variant(&base, Some(variant)).is_err() {
                    choice.variant = None;
                    records.push(record(variant_key(c, &id), &Option::<String>::None)?);
                }
            }
            let mut selected = current.clone();
            set_model(&mut selected, c, choice)?;
            records.push(record(
                draft_key(c, selected.agent_id.as_deref()),
                &model(&selected),
            )?);
            db.set_prefs(&records).map_err(app_error)?;
            Ok(selected)
        }
        Action::Variant(variant) => {
            let mut selected = current.clone();
            set_model(
                &mut selected,
                c,
                ModelChoice {
                    id: current.model_id.clone(),
                    variant: variant.clone(),
                },
            )?;
            db.set_prefs(&[
                record(variant_key(c, &selected.model_id), &variant)?,
                record(
                    draft_key(c, selected.agent_id.as_deref()),
                    &model(&selected),
                )?,
            ])
            .map_err(app_error)?;
            Ok(selected)
        }
    }
}

pub(super) fn apply(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    session: &str,
    home: bool,
    action: Action,
) -> Result<Effective, CoreError> {
    let key = session_key(c, session);
    let existing = load::<SessionChoice>(db, &key)?;
    if existing.is_none() && action == Action::Current && fallback.legacy_epoch != 0 {
        return Ok(fallback.clone());
    }
    let mut choice = existing.unwrap_or_else(|| SessionChoice {
        agent: fallback.agent_id.clone(),
        models: BTreeMap::new(),
        epoch: fallback.legacy_epoch,
    });
    if choice.epoch < fallback.legacy_epoch {
        if action == Action::Current {
            return Ok(fallback.clone());
        }
        choice.agent = fallback.agent_id.clone();
        choice.models.clear();
    }
    if action == Action::Current {
        return resolve(db, c, fallback, &choice);
    }
    if let Action::Agent(agent) = &action {
        choice.agent = Some(agent.clone());
    } else if let Action::New(agent) = &action {
        choice.agent = agent.clone();
    }
    let mut selected = if matches!(action, Action::Model(_))
        || (choice.models.is_empty()
            && fallback.legacy_epoch != 0
            && !matches!(action, Action::Agent(_)))
    {
        // Replacing a retired model must never require resolving the old id.
        base_for_agent(c, fallback, choice.agent.as_deref())?
    } else {
        resolve(db, c, fallback, &choice)?
    };
    let mut records = Vec::new();
    match &action {
        Action::Model(id) => {
            let mut preferred = if choice
                .models
                .get(choice.agent.as_deref().unwrap_or(""))
                .is_some_and(|current| current.id == *id)
            {
                choice.models[choice.agent.as_deref().unwrap_or("")].clone()
            } else {
                preferred(
                    db,
                    c,
                    &base_for_agent(c, fallback, choice.agent.as_deref())?,
                    id.clone(),
                )?
            };
            if let Some(variant) = preferred.variant.as_deref() {
                let base = crate::models::select_model(&c.catalog, id).map_err(app_error)?;
                if crate::models::select_variant(&base, Some(variant)).is_err() {
                    // The explicit model choice accepts Default for a retired
                    // variant; persist that remediation, not an implicit fallback.
                    preferred.variant = None;
                    records.push(record(variant_key(c, id), &Option::<String>::None)?);
                }
            }
            set_model(&mut selected, c, preferred)?;
        }
        Action::Variant(variant) => {
            let id = selected.model_id.clone();
            set_model(
                &mut selected,
                c,
                ModelChoice {
                    id,
                    variant: variant.clone(),
                },
            )?;
            records.push(record(variant_key(c, &selected.model_id), variant)?);
        }
        _ => {}
    }
    choice
        .models
        .insert(choice.agent.clone().unwrap_or_default(), model(&selected));
    choice.epoch = fallback.legacy_epoch;
    if home && matches!(action, Action::Model(_)) {
        records.push(record(
            draft_key(c, choice.agent.as_deref()),
            &model(&selected),
        )?);
    }
    records.push(record(key, &choice)?);
    db.set_prefs(&records).map_err(app_error)?;
    Ok(selected)
}
