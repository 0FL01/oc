//! Scoped selection metadata in the existing prefs store. No history owner or
//! transcript copy. Keys are structured tuples (no delimiter collisions); only
//! one session's admitted agent drafts are loaded for an action/turn.
use super::*;
use oc_core::core_app::FreshSelection;
use oc_core::queries::SessionSelectionAction as Action;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ModelChoice {
    #[serde(default)]
    provider: Option<String>,
    id: String,
    variant: Option<String>,
}

fn normalized_variant(variant: &Option<String>) -> Option<&str> {
    variant.as_deref().filter(|name| *name != "default")
}

#[derive(Default, Serialize, Deserialize)]
struct SessionChoice {
    agent: Option<String>,
    models: BTreeMap<String, ModelChoice>,
    #[serde(default)]
    epoch: u64,
    #[serde(default)]
    inline_command: Option<String>,
    #[serde(default)]
    command_parents: Vec<String>,
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

/// Same collision-free Location tuple convention as the selection drafts.
pub(super) fn tab_deck_key(location: &str) -> String {
    crate::storage::tab_deck_key(location)
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

fn home_agent_key(c: &Composition) -> String {
    key(
        "home_agent",
        &[&c.project.to_string_lossy(), &c.catalog.provider],
    )
}

fn variant_key(provider: &str, id: &str) -> String {
    key("variant", &[provider, id])
}

fn load<T: serde::de::DeserializeOwned>(db: &Db, key: &str) -> Result<Option<T>, CoreError> {
    db.get_pref(key)
        .map_err(|error| query_storage_error(db, error))?
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|_| {
                CoreError::Diagnostic(saved_selection_issue(db, &["selection"]).diagnostic)
            })
        })
        .transpose()
}

fn record<T: Serialize>(key: String, value: &T) -> Result<(String, String), CoreError> {
    Ok((key, serde_json::to_string(value).map_err(app_error)?))
}

fn model(e: &Effective) -> ModelChoice {
    ModelChoice {
        provider: Some(e.provider_id.clone()),
        id: e.model_id.clone(),
        variant: e.variant.clone(),
    }
}

fn unavailable(c: &Composition, field: &str) -> CoreError {
    let (code, action) = match field {
        "variant" => (
            oc_core::queries::ServiceCode::VariantUnavailable,
            oc_core::queries::ServiceAction::SelectVariant,
        ),
        "model" => (
            oc_core::queries::ServiceCode::ModelUnavailable,
            oc_core::queries::ServiceAction::SelectModel,
        ),
        _ => unreachable!("only schema-owned model/variant call sites"),
    };
    CoreError::Diagnostic(crate::config::diagnostic::failure(
        &c.project.to_string_lossy(),
        &[field],
        oc_core::queries::ServiceStage::Admission,
        code,
        action,
    ))
}

fn set_model(e: &mut Effective, c: &Composition, choice: ModelChoice) -> Result<(), CoreError> {
    let provider = choice.provider.as_deref().unwrap_or(&c.catalog.provider);
    let catalog = c
        .catalog_for(provider)
        .ok_or_else(|| unavailable(c, "model"))?;
    let selected =
        crate::models::select_model(catalog, &choice.id).map_err(|_| unavailable(c, "model"))?;
    let selected = crate::models::select_variant(&selected, choice.variant.as_deref())
        .map_err(|_| unavailable(c, "variant"))?;
    e.model_id = selected.id;
    e.provider_id = provider.to_owned();
    e.variant = selected.variant.map(|v| v.name);
    // An explicit admitted model repairs a profile's blocked pin, while an
    // absent/nonprimary agent still requires a separate agent choice.
    if e.profile_issue
        .as_ref()
        .is_some_and(|issue| issue.code == oc_core::queries::ServiceCode::ModelUnavailable)
    {
        e.profile_issue = None;
    }
    Ok(())
}

fn preferred(
    db: &Db,
    c: &Composition,
    e: &Effective,
    id: String,
) -> Result<ModelChoice, CoreError> {
    // Some(None) is an explicit Default preference, distinct from no preference.
    let variant =
        if let Some(preferred) = load::<Option<String>>(db, &variant_key(&e.provider_id, &id))? {
            preferred
        } else if e.model_id == id {
            e.variant.clone()
        } else if c.catalog.provider == e.provider_id && c.model_id == id {
            c.variant.clone()
        } else {
            None
        };
    // A retired preference must remain visible until an explicit choice.
    Ok(ModelChoice {
        provider: Some(e.provider_id.clone()),
        id,
        variant,
    })
}

fn base_for_agent(
    c: &Composition,
    fallback: &Effective,
    agent: Option<&str>,
) -> Result<Effective, CoreError> {
    let mut selected = fallback.clone();
    // A different unpinned agent must not inherit another agent's scoped model.
    // Absent/legacy-null means resolve the generation's default primary, as
    // pinned Agent.select(undefined), not an executable agentless lane.
    if let Some(agent) = agent.or(c.default_agent.as_deref()) {
        if selected.agent_id.as_deref() != Some(agent) {
            // Explicitly changing primary cannot inherit the previous agent's
            // model/variant. Scoped drafts are applied only after this base.
            selected.model_id = c.model_id.clone();
            selected.provider_id = c.catalog.provider.clone();
            selected.variant = c.variant.clone();
        }
        if selected.set_agent(c, agent).is_err() {
            // Saved references are view state, not execution admission. Preserve
            // the primary and its invalid pin separately; only a missing or
            // nonprimary definition is an unavailable agent.
            selected.retain_invalid_saved_agent(c, agent);
        }
    } else {
        return Err(app_error("no selectable primary agent"));
    }
    Ok(selected)
}

fn resolve(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    choice: &SessionChoice,
) -> Result<Effective, CoreError> {
    let mut selected = if let Some(command) = choice.inline_command.as_deref() {
        commands::restore_inline(
            c,
            fallback,
            choice.agent.as_deref(),
            command,
            &choice.command_parents,
        )?
    } else {
        base_for_agent(c, fallback, choice.agent.as_deref())?
    };
    let agent = choice.agent.as_deref().unwrap_or("");
    let model = match choice.models.get(agent) {
        Some(model) => model.clone(),
        None => {
            let draft = load::<ModelChoice>(db, &draft_key(c, choice.agent.as_deref()))?;
            match draft {
                Some(draft) => draft,
                None => preferred(db, c, &selected, selected.model_id.clone())?,
            }
        }
    };
    // Project a retired choice honestly: only turn acceptance validates it.
    // No replacement model/variant is chosen and no preference is rewritten.
    if model.id != selected.model_id
        && selected
            .profile_issue
            .as_ref()
            .is_some_and(|issue| issue.code == oc_core::queries::ServiceCode::ModelUnavailable)
    {
        selected.profile_issue = None;
    }
    selected.model_id = model.id;
    selected.provider_id = model.provider.unwrap_or_else(|| c.catalog.provider.clone());
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

/// Request-boundary read of the same durable owner record, scoped to the pinned
/// agent/Location. No draft, registry or archive is loaded here.
pub(crate) fn request_choice(
    db: &Db,
    project: &std::path::Path,
    provider: &str,
    session: &str,
    agent: Option<&str>,
) -> Result<Option<oc_core::queries::ModelRef>, crate::runtime::RuntimeError> {
    let project = project.to_string_lossy();
    let session_record = key("session", &[&project, provider, session]);
    let epoch = load::<u64>(db, &key("legacy_epoch", &[&project, provider]))
        .map_err(|_| crate::runtime::RuntimeError::Storage)?
        .unwrap_or(0);
    let choice = match db
        .get_pref_bounded(&session_record, 64 * 1024)
        .map_err(|_| crate::runtime::RuntimeError::Storage)?
    {
        crate::storage::BoundedPref::Missing => None,
        crate::storage::BoundedPref::TooLarge => return Err(crate::runtime::RuntimeError::Storage),
        crate::storage::BoundedPref::Value(raw) => Some(
            serde_json::from_str::<SessionChoice>(&raw)
                .map_err(|_| crate::runtime::RuntimeError::Storage)?,
        ),
    };
    Ok(choice
        .filter(|choice| choice.epoch >= epoch && choice.agent.as_deref() == agent)
        .and_then(|choice| choice.models.get(agent.unwrap_or("")).cloned())
        .map(|model| oc_core::queries::ModelRef {
            provider: model.provider.unwrap_or_else(|| provider.to_owned()),
            id: model.id,
            variant: model.variant,
        }))
}

/// Bounded, read-only fork selection: preserve this session's other agent
/// drafts while pinning the currently effective choice and legacy epoch.
pub(super) fn fork_choice(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    session: &str,
) -> Result<String, CoreError> {
    let mut choice = match db
        .get_pref_bounded(&session_key(c, session), 64 * 1024)
        .map_err(|error| query_storage_error(db, error))?
    {
        crate::storage::BoundedPref::Missing => SessionChoice::default(),
        crate::storage::BoundedPref::TooLarge => {
            return Err(CoreError::Diagnostic(crate::config::diagnostic::failure(
                &db.root().to_string_lossy(),
                &["selection"],
                oc_core::queries::ServiceStage::Admission,
                oc_core::queries::ServiceCode::CapacityExceeded,
                oc_core::queries::ServiceAction::ReduceCapacity,
            )));
        }
        crate::storage::BoundedPref::Value(raw) => serde_json::from_str(&raw).map_err(|_| {
            CoreError::Diagnostic(saved_selection_issue(db, &["selection"]).diagnostic)
        })?,
    };
    let selected = if choice.epoch >= fallback.legacy_epoch && !choice.models.is_empty() {
        resolve(db, c, fallback, &choice)?
    } else {
        fallback.clone()
    };
    // Validate without changing the source, Home drafts or global preferences.
    let (selected, _) = fresh(
        c,
        fallback,
        session,
        FreshSelection {
            binding: Some(oc_core::queries::SelectionBinding {
                location: Some(c.project.to_string_lossy().into_owned()),
                generation: 0,
                provider: selected.provider_id,
                agent_id: selected.agent_id.clone(),
            }),
            agent_id: selected.agent_id,
            model_id: selected.model_id,
            variant: selected.variant,
        },
    )?;
    choice.agent = selected.agent_id.clone();
    choice.models.insert(
        selected.agent_id.clone().unwrap_or_default(),
        model(&selected),
    );
    choice.epoch = fallback.legacy_epoch;
    serde_json::to_string(&choice).map_err(app_error)
}

/// Prevalidate an explicit Home choice and encode its session preference for
/// the same transaction that accepts the new root's first user message.
pub(super) fn fresh_captured(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    session: &str,
    choice: Option<FreshSelection>,
    home_choices: &BTreeMap<String, Effective>,
    generation: u64,
) -> Result<(Effective, (String, String)), CoreError> {
    let home = || match home_choices.get(c.project.to_string_lossy().as_ref()) {
        Some(selected) => Ok(selected.clone()),
        None => home_current(db, c, fallback),
    };
    let choice = match choice {
        Some(mut choice) => {
            if let Some(binding) = &choice.binding {
                let home = home()?;
                let visible = home.snapshot(c, generation);
                if binding.location.as_deref() != Some(c.project.to_string_lossy().as_ref())
                    || binding.generation != generation
                    || c.catalog_for(&binding.provider).is_none()
                    || binding.agent_id != visible.agent_id
                {
                    return Err(app_error("stale fresh model commit scope"));
                }
                choice.agent_id = home.agent_id.clone();
                if binding.provider == visible.provider
                    && choice.model_id == visible.model_id
                    && choice.variant == visible.variant
                {
                    home.admit_selection(c)?;
                    choice.model_id = home.model_id;
                    choice.variant = home.variant;
                }
            }
            choice
        }
        None => {
            let home = home()?;
            FreshSelection {
                binding: Some(oc_core::queries::SelectionBinding {
                    location: Some(c.project.to_string_lossy().into_owned()),
                    generation,
                    provider: home.provider_id,
                    agent_id: home.agent_id.clone(),
                }),
                agent_id: home.agent_id,
                model_id: home.model_id,
                variant: home.variant,
            }
        }
    };
    fresh(c, fallback, session, choice)
}

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
            provider: choice.binding.map(|binding| binding.provider),
            id: choice.model_id,
            variant: choice.variant,
        },
    )?;
    let agent = selected.agent_id.clone().unwrap_or_default();
    let session_choice = SessionChoice {
        agent: selected.agent_id.clone(),
        models: BTreeMap::from([(agent, model(&selected))]),
        epoch: fallback.legacy_epoch,
        inline_command: None,
        command_parents: Vec::new(),
    };
    Ok((selected, record(session_key(c, session), &session_choice)?))
}

/// Sessionless Home selection. The admitted agent and model draft are
/// Location-scoped prefs; Home never uses a fabricated session id.
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
    if choice.id != selected.model_id
        && selected
            .profile_issue
            .as_ref()
            .is_some_and(|issue| issue.code == oc_core::queries::ServiceCode::ModelUnavailable)
    {
        selected.profile_issue = None;
    }
    selected.model_id = choice.id;
    selected.provider_id = choice
        .provider
        .unwrap_or_else(|| c.catalog.provider.clone());
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
    let agent = load::<String>(db, &home_agent_key(c))?;
    home_for_agent(
        db,
        c,
        fallback,
        agent.as_deref().or(fallback.agent_id.as_deref()),
    )
}

pub(super) fn home(
    db: &Db,
    c: &Composition,
    fallback: &Effective,
    current: &Effective,
    action: Action,
) -> Result<Effective, CoreError> {
    match action {
        Action::Commit(commit) => {
            let mut selected = current.clone();
            if commit.binding.location.as_deref() != Some(c.project.to_string_lossy().as_ref())
                || commit.binding.agent_id != current.agent_id
                || c.catalog_for(&commit.binding.provider).is_none()
            {
                return Err(app_error("stale model commit scope"));
            }
            set_model(
                &mut selected,
                c,
                ModelChoice {
                    provider: Some(commit.binding.provider),
                    id: commit.model_id,
                    variant: commit.variant.filter(|name| name != "default"),
                },
            )?;
            selected.admit_selection(c)?;
            db.set_prefs(&[
                record(
                    key("variant", &[&selected.provider_id, &selected.model_id]),
                    &selected.variant,
                )?,
                record(
                    draft_key(c, selected.agent_id.as_deref()),
                    &model(&selected),
                )?,
            ])
            .map_err(|error| CoreError::Diagnostic(storage_diagnostic(db.root(), &error)))?;
            Ok(selected)
        }
        Action::Current => Ok(current.clone()),
        Action::Agent(agent) | Action::New(Some(agent)) => {
            let selected = home_for_agent(db, c, fallback, Some(&agent))?;
            selected.admit_selection(c)?;
            db.set_pref(&home_agent_key(c), &record(home_agent_key(c), &agent)?.1)
                .map_err(|error| CoreError::Diagnostic(storage_diagnostic(db.root(), &error)))?;
            Ok(selected)
        }
        Action::New(None) => {
            // An explicit new Home route resets to the configured/persisted
            // primary's admitted draft, even after another Home agent was
            // explicitly selected. It also records the user's reset.
            let selected = home_for_agent(db, c, fallback, fallback.agent_id.as_deref())?;
            selected.admit_selection(c)?;
            if let Some(agent) = &selected.agent_id {
                db.set_pref(&home_agent_key(c), &record(home_agent_key(c), agent)?.1)
                    .map_err(|error| {
                        CoreError::Diagnostic(storage_diagnostic(db.root(), &error))
                    })?;
            }
            Ok(selected)
        }
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
                let base = crate::models::select_model(
                    c.catalog_for(&current.provider_id)
                        .ok_or_else(|| unavailable(c, "model"))?,
                    &id,
                )
                .map_err(|_| unavailable(c, "model"))?;
                if crate::models::select_variant(&base, Some(variant)).is_err() {
                    choice.variant = None;
                    records.push(record(
                        variant_key(&current.provider_id, &id),
                        &Option::<String>::None,
                    )?);
                }
            }
            let mut selected = current.clone();
            set_model(&mut selected, c, choice)?;
            selected.admit_selection(c)?;
            records.push(record(
                draft_key(c, selected.agent_id.as_deref()),
                &model(&selected),
            )?);
            db.set_prefs(&records)
                .map_err(|error| CoreError::Diagnostic(storage_diagnostic(db.root(), &error)))?;
            Ok(selected)
        }
        Action::Variant(variant) => {
            let mut selected = current.clone();
            set_model(
                &mut selected,
                c,
                ModelChoice {
                    provider: Some(current.provider_id.clone()),
                    id: current.model_id.clone(),
                    variant: variant.clone(),
                },
            )?;
            selected.admit_selection(c)?;
            db.set_prefs(&[
                record(
                    variant_key(&selected.provider_id, &selected.model_id),
                    &variant,
                )?,
                record(
                    draft_key(c, selected.agent_id.as_deref()),
                    &model(&selected),
                )?,
            ])
            .map_err(|error| CoreError::Diagnostic(storage_diagnostic(db.root(), &error)))?;
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
    generation: u64,
) -> Result<Effective, CoreError> {
    let previous = for_turn(db, c, fallback, session)?;
    if let Action::Commit(commit) = &action {
        validate_commit(db, c, &previous, session, generation, commit)?;
        let visible = previous.snapshot(c, generation);
        if visible.provider == commit.binding.provider
            && visible.model_id == commit.model_id
            && normalized_variant(&visible.variant) == normalized_variant(&commit.variant)
        {
            // An unchanged captured projection can carry an opaque retired
            // identity. Keep its original owner cause/admission, never treat
            // that display identity as a replacement model or silent fallback.
            previous.admit_selection(c)?;
            return Ok(previous);
        }
    }
    let key = session_key(c, session);
    let existing = load::<SessionChoice>(db, &key)?;
    if existing.is_none() && action == Action::Current && fallback.legacy_epoch != 0 {
        return Ok(fallback.clone());
    }
    let mut choice = existing.unwrap_or_else(|| SessionChoice {
        agent: fallback.agent_id.clone(),
        models: BTreeMap::new(),
        epoch: fallback.legacy_epoch,
        inline_command: None,
        command_parents: Vec::new(),
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
        choice.inline_command = None;
        choice.command_parents.clear();
    } else if let Action::New(agent) = &action {
        choice.agent = agent.clone();
        choice.inline_command = None;
        choice.command_parents.clear();
    }
    let mut selected = if matches!(action, Action::Model(_) | Action::Commit(_))
        || (choice.models.is_empty()
            && fallback.legacy_epoch != 0
            && !matches!(action, Action::Agent(_)))
    {
        // Replacing a retired model must never require resolving the old id.
        if choice.inline_command.is_some() {
            resolve(db, c, fallback, &choice)?
        } else {
            base_for_agent(c, fallback, choice.agent.as_deref())?
        }
    } else {
        resolve(db, c, fallback, &choice)?
    };
    if matches!(action, Action::Agent(_) | Action::New(_)) {
        selected.admit_selection(c)?;
    }
    let mut records = Vec::new();
    match &action {
        Action::Commit(commit) => {
            set_model(
                &mut selected,
                c,
                ModelChoice {
                    provider: Some(commit.binding.provider.clone()),
                    id: commit.model_id.clone(),
                    variant: commit.variant.clone().filter(|name| name != "default"),
                },
            )?;
            records.push(record(
                variant_key(&selected.provider_id, &selected.model_id),
                &selected.variant,
            )?);
        }
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
                let base = crate::models::select_model(
                    c.catalog_for(preferred.provider.as_deref().unwrap_or(&c.catalog.provider))
                        .ok_or_else(|| unavailable(c, "model"))?,
                    id,
                )
                .map_err(|_| unavailable(c, "model"))?;
                if crate::models::select_variant(&base, Some(variant)).is_err() {
                    // The explicit model choice accepts Default for a retired
                    // variant; persist that remediation, not an implicit fallback.
                    preferred.variant = None;
                    records.push(record(
                        variant_key(
                            preferred.provider.as_deref().unwrap_or(&c.catalog.provider),
                            id,
                        ),
                        &Option::<String>::None,
                    )?);
                }
            }
            set_model(&mut selected, c, preferred)?;
        }
        Action::Variant(variant) => {
            let id = selected.model_id.clone();
            let provider = selected.provider_id.clone();
            set_model(
                &mut selected,
                c,
                ModelChoice {
                    provider: Some(provider),
                    id,
                    variant: variant.clone(),
                },
            )?;
            records.push(record(
                variant_key(&selected.provider_id, &selected.model_id),
                variant,
            )?);
        }
        _ => {}
    }
    selected.admit_selection(c)?;
    let model_action = matches!(
        action,
        Action::Model(_) | Action::Variant(_) | Action::Commit(_)
    );
    if model_action
        && selected.provider_id == previous.provider_id
        && selected.model_id == previous.model_id
        && selected.agent_id == previous.agent_id
        && normalized_variant(&selected.variant) == normalized_variant(&previous.variant)
    {
        return Ok(selected);
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
    let persisted = if model_action {
        let commit = publication(c, &selected, generation, &action);
        let effort = |choice: &Effective| {
            choice
                .variant
                .as_ref()
                .and_then(|variant| {
                    c.catalog_for(&choice.provider_id)
                        .and_then(|catalog| {
                            crate::models::select_model(catalog, &choice.model_id).ok()
                        })
                        .and_then(|model| crate::models::select_variant(&model, Some(variant)).ok())
                        .and_then(|model| {
                            model.variant.and_then(|variant| variant.reasoning_effort)
                        })
                })
                .or_else(|| {
                    c.provider_for(&choice.provider_id)?
                        .for_selection(&choice.model_id, choice.variant.as_deref())
                        .wire
                        .settings
                        .current_effort()
                        .map(str::to_owned)
                })
        };
        let old_effort = effort(&previous);
        let new_effort = effort(&selected);
        let mut payload = serde_json::json!({"session":session,"commit":commit});
        if old_effort != new_effort {
            payload["effort_update"] =
                serde_json::json!({"previous":old_effort,"effort":new_effort});
        }
        db.commit_session_model_choice(&records, session, &payload.to_string())
    } else {
        let reminder = crate::plan::switched(
            previous.agent_id.as_deref(),
            selected.agent_id.as_deref(),
            c.parent_env.get("HOME").map(String::as_str),
        );
        db.commit_session_agent_choice(&records, session, reminder.as_deref())
    };
    persisted.map_err(|error| CoreError::Diagnostic(storage_diagnostic(db.root(), &error)))?;
    Ok(selected)
}

/// Read-only command selection; the acceptance owner commits the encoded pair.
pub(super) fn command_record(
    db: &Db,
    c: &Composition,
    session: &str,
    selected: &Effective,
) -> Result<(String, String), CoreError> {
    let key = session_key(c, session);
    let mut choice = load::<SessionChoice>(db, &key)?.unwrap_or_default();
    if choice.epoch < selected.legacy_epoch {
        choice.models.clear();
    }
    choice.agent = selected.agent_id.clone();
    choice.inline_command = selected.inline_command.clone();
    choice.command_parents = selected.command_parents.clone();
    choice.epoch = selected.legacy_epoch;
    choice
        .models
        .insert(choice.agent.clone().unwrap_or_default(), model(selected));
    record(key, &choice)
}

pub(super) fn command_commit(
    db: &Db,
    c: &Composition,
    current: &Effective,
    session: &str,
    generation: u64,
    commit: &oc_core::queries::ModelCommit,
) -> Result<Effective, CoreError> {
    validate_commit(db, c, current, session, generation, commit)?;
    let mut selected = current.clone();
    set_model(
        &mut selected,
        c,
        ModelChoice {
            provider: Some(commit.binding.provider.clone()),
            id: commit.model_id.clone(),
            variant: commit.variant.clone(),
        },
    )?;
    Ok(selected)
}

pub(super) fn publication(
    c: &Composition,
    selected: &Effective,
    generation: u64,
    action: &Action,
) -> oc_core::queries::ModelCommit {
    oc_core::queries::ModelCommit {
        caller: if let Action::Commit(commit) = action {
            commit.caller
        } else {
            0
        },
        binding: oc_core::queries::SelectionBinding {
            location: Some(c.project.to_string_lossy().into_owned()),
            generation,
            provider: selected.provider_id.clone(),
            agent_id: selected.agent_id.clone(),
        },
        model_id: selected.model_id.clone(),
        variant: normalized_variant(&selected.variant).map(str::to_owned),
        draft_revision: if let Action::Commit(commit) = action {
            commit.draft_revision
        } else {
            0
        },
    }
}

fn validate_commit(
    db: &Db,
    c: &Composition,
    current: &Effective,
    session: &str,
    generation: u64,
    commit: &oc_core::queries::ModelCommit,
) -> Result<(), CoreError> {
    if commit.binding.location.as_deref() != Some(c.project.to_string_lossy().as_ref())
        || commit.binding.generation != generation
        || c.catalog_for(&commit.binding.provider).is_none()
        || commit.binding.agent_id != current.snapshot(c, generation).agent_id
    {
        return Err(app_error("stale model commit scope"));
    }
    if db
        .session_meta(session)
        .map_err(app_error)?
        .parent_id
        .is_some()
    {
        return Err(app_error("child session is read-only"));
    }
    Ok(())
}
