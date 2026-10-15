//! Same-Location candidate assembly and publication. The existing application
//! owns this operation through ACK; it never creates a replacement runtime/owner.
use super::*;
use std::collections::BTreeSet;
use std::path::PathBuf;

mod coordinator;
pub(super) use coordinator::Coordinator;

pub(super) struct BuildInputs {
    project: PathBuf,
    env: BTreeMap<String, String>,
    local: Option<Arc<composition::ReloadInputs>>,
    catalogs: composition::CatalogReuse,
}

impl BuildInputs {
    fn capture(composition: &Composition) -> Self {
        Self {
            project: composition.project.clone(),
            env: composition.parent_env.clone(),
            local: composition.local_inputs.clone(),
            catalogs: composition.catalog_reuse(),
        }
    }
}

pub(super) struct Candidate {
    next: Box<Composition>,
    previous: Option<Arc<composition::ReloadInputs>>,
    epoch: u64,
    force: bool,
    noop: bool,
}

fn configuration(failure: composition::LoadFailure) -> CoreError {
    let issue = SpawnIssue::configuration(failure);
    CoreError::LocationSwitch {
        category: LocationSwitchFailure::Configuration,
        detail: issue.to_string(),
        diagnostic: Some(issue.diagnostic),
    }
}

/// Resolve retained choices without changing any deck, selection or history.
/// Valid removals become exact unavailable views; malformed sources were already
/// rejected by the strict loader before reaching this step.
fn retained_selections(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &Effective,
    sessions: &BTreeMap<String, String>,
    generation: u64,
) -> Result<(), CoreError> {
    let mut retained = tab_deck::load(db, runtime)?.sessions;
    if let Some(id) = sessions.get(runtime.location())
        && !retained.iter().any(|session| &session.0 == id)
    {
        retained.push(SessionId(id.clone()));
    }
    for session in retained {
        runtime.open_session(&session.0).map_err(runtime_error)?;
        selection::apply(
            db,
            composition,
            effective,
            &session.0,
            false,
            oc_core::queries::SessionSelectionAction::Current,
            generation,
        )?;
        selection::for_turn(db, composition, effective, &session.0)?;
    }
    Ok(())
}

fn superseded() -> CoreError {
    CoreError::Diagnostic(crate::config::diagnostic::failure(
        "native reload candidate",
        &["source", "superseded"],
        oc_core::queries::ServiceStage::Admission,
        oc_core::queries::ServiceCode::SourceUnavailable,
        oc_core::queries::ServiceAction::ReviewConfiguration,
    ))
}

pub(super) async fn build(
    db: &Db,
    base: BuildInputs,
    watch: &composition::Observation,
    force: bool,
) -> Result<Candidate, CoreError> {
    let mut next = composition::load_reload_local_with_env(&base.project, base.env.clone())
        .await
        .map_err(configuration)?;
    if next.project != base.project {
        return Err(CoreError::Diagnostic(crate::config::diagnostic::failure(
            "native source publication",
            &["location"],
            oc_core::queries::ServiceStage::Admission,
            oc_core::queries::ServiceCode::TrustRefused,
            oc_core::queries::ServiceAction::ReviewConfiguration,
        )));
    }
    let admitted_inputs = next.reload_inputs().map_err(configuration)?;
    watch
        .reconcile(&next.watch_inputs, false)
        .await
        .map_err(CoreError::Diagnostic)?;
    // Closing the load→watch registration window is a local admitted rescan,
    // not a second discovery or an expanded read capability.
    let ready_inputs = composition::load_reload_local_with_env(&base.project, base.env.clone())
        .await
        .map_err(configuration)?
        .reload_inputs()
        .map_err(configuration)?;
    if ready_inputs != admitted_inputs {
        return Err(superseded());
    }
    let epoch = watch.epoch();
    let noop = !force && next.local_inputs == base.local;
    if noop {
        // A topology repair is still meaningful even when admitted source values
        // are unchanged. Do not replace resolved credentials/catalogs or revive
        // a manually disconnected MCP client just to acknowledge such an edit.
        return Ok(Candidate {
            next: Box::new(next),
            previous: base.local,
            epoch,
            force,
            noop,
        });
    }
    next.attach_custom_connections(db).map_err(configuration)?;
    next.resolve_credentials(db).await.map_err(configuration)?;
    next.attach_public_catalog(db).await;
    let retained = if force {
        BTreeSet::new()
    } else {
        next.retain_catalog_reuse(&base.catalogs)
    };
    if force
        || [crate::models_dev::PROVIDER, crate::models_dev::OPENAI]
            .iter()
            .any(|id| next.public_provider(id) && !retained.contains(*id))
    {
        next.refresh_public_catalog(db, force).await;
    }
    next.refresh_provider().await.map_err(configuration)?;
    // Discovery is bounded but can outlive a file replacement. Re-admission is
    // local-only: no second discovery, credentials, MCP activation or effects.
    // Compare original resolved inputs, never the remote-merged model catalog.
    let current_inputs = composition::load_reload_local_with_env(&base.project, base.env.clone())
        .await
        .map_err(configuration)?
        .reload_inputs()
        .map_err(configuration)?;
    if current_inputs != admitted_inputs || watch.epoch() != epoch {
        return Err(superseded());
    }
    Ok(Candidate {
        next: Box::new(next),
        previous: base.local,
        epoch,
        force,
        noop,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn publish(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &mut Composition,
    effective: &mut Effective,
    registry: &mut WorkspaceRegistry,
    sessions: &BTreeMap<String, String>,
    location_epoch: &AtomicU64,
    provider_work: &mut provider_catalog::ProviderWork,
    events: &broadcast::Sender<CoreEvent>,
    watch: &composition::Observation,
    candidate: Candidate,
    active: Option<(&str, &Effective)>,
) -> Result<ReloadLocationSnapshot, CoreError> {
    if candidate.previous != composition.local_inputs || watch.epoch() != candidate.epoch {
        return Err(superseded());
    }
    let Candidate {
        next, force, noop, ..
    } = candidate;
    let next = *next;
    if noop {
        composition.watch_inputs = next.watch_inputs;
        let generation = location_epoch.load(Ordering::SeqCst);
        return Ok(ReloadLocationSnapshot {
            location: runtime.location().into(),
            generation,
            catalog: effective.snapshot(composition, generation),
            diagnostics: composition.diagnostics.clone(),
            notices: composition.startup_notices.clone(),
        });
    }
    if next.provider_state.catalog_status == oc_core::queries::ProviderStatus::Failed {
        let diagnostic = next
            .provider_state
            .for_model(&next.model_id, false)
            .diagnostic
            .expect("failed refresh has a cause");
        if next.catalog.provider == composition.catalog.provider {
            let same_binding = composition
                .provider
                .same_request_binding(&next.provider)
                .map_err(|_| {
                    CoreError::Diagnostic(crate::config::diagnostic::failure(
                        "native provider binding",
                        &["provider", "options", "headers"],
                        oc_core::queries::ServiceStage::Config,
                        oc_core::queries::ServiceCode::InvalidHeader,
                        oc_core::queries::ServiceAction::ReviewConfiguration,
                    ))
                })?;
            composition
                .provider_state
                .retain_failed_attempt(&next.provider_state, same_binding);
            if active.is_none() {
                runtime
                    .publish_provider_state(
                        composition.provider_state.clone(),
                        composition.request_provider(&composition.catalog.provider),
                    )
                    .map_err(runtime_error)?;
            }
            let _ = events.send(CoreEvent::ProviderChanged);
        }
        return Err(CoreError::ProviderUnavailable(diagnostic));
    }

    let generation = runtime
        .generation_id()
        .checked_add(1)
        .ok_or_else(|| runtime_error(RuntimeError::Storage))?;
    let mut next_effective = Effective::from_composition(&next);
    next_effective.legacy_epoch = selection::legacy_epoch(db, &next)?;
    let mut next_registry = WorkspaceRegistry::bind(
        generation,
        runtime.location(),
        &next.generation,
        workspace_agents(&next),
        skill_metas(&next),
    );
    let mut notes = next.diagnostics.clone();
    notes.extend(
        next_effective
            .apply_persisted_model(db, &next)
            .map_err(|issue| CoreError::Diagnostic(issue.diagnostic))?,
    );
    notes.extend(
        next_effective
            .apply_persisted_agent(db, &next, &mut next_registry)
            .map_err(|issue| CoreError::Diagnostic(issue.diagnostic))?,
    );
    retained_selections(db, runtime, &next, &next_effective, sessions, generation)?;
    // Consumer ownership belongs to this live application, not newly read files.
    next.approval_consumer_mode.store(
        composition.approval_consumer_mode.load(Ordering::SeqCst),
        Ordering::SeqCst,
    );
    next.question_consumer.store(
        composition.question_consumer.load(Ordering::SeqCst),
        Ordering::SeqCst,
    );
    // A running root keeps its accepted session/profile identity. Resolve its
    // contents against G2, never substitute the worker's Home/default profile.
    let request_effective = if let Some((session, accepted)) = active {
        selection::for_reload(db, &next, &next_effective, session, accepted)?
    } else {
        next_effective.clone()
    };
    let input = crate::runtime::SourcePublication {
        generation: next.generation.clone(),
        workspace: workspace_publication(&next, &request_effective),
        dcp_config: next.dcp_config.clone(),
        dcp_protected: next.dcp_protected.clone(),
        provider_state: next.provider_state.clone(),
        request_provider: next.request_provider(&next.catalog.provider),
        mcp_activation: Some(next.mcp_activation.clone()),
    };
    // Retire old GET outcomes only after every candidate check has succeeded.
    // Those read-only jobs must not publish an old binding after this commit.
    provider_work.stop().await.map_err(CoreError::Diagnostic)?;
    runtime
        .publish_source(input, force)
        .await
        .map_err(runtime_error)?;
    let epoch = location_epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let mut notices = next.startup_notices.clone();
    if notes.len() > next.diagnostics.len() {
        notices.push(StartupNotice::SavedSelection);
    }
    *composition = next;
    *effective = next_effective;
    *registry = next_registry;
    let _ = events.send(CoreEvent::ProviderChanged);
    Ok(ReloadLocationSnapshot {
        location: runtime.location().into(),
        generation: epoch,
        catalog: effective.snapshot(composition, epoch),
        diagnostics: notes,
        notices,
    })
}
